//! Label and assignee changes written as directives in the review body (`+label`, `-label`,
//! `+assignee:@user`). The frontend parses the body; this validates the names against GitHub
//! before anything is posted, and applies the changes after the review.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::submit::GraphQl;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Directives {
    pub add_labels: Vec<String>,
    pub remove_labels: Vec<String>,
    pub add_assignees: Vec<String>,
    pub remove_assignees: Vec<String>,
}

impl Directives {
    pub fn is_empty(&self) -> bool {
        self.add_labels.is_empty()
            && self.remove_labels.is_empty()
            && self.add_assignees.is_empty()
            && self.remove_assignees.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Item {
    id: String,
    /// The label name or user login as GitHub spells it.
    name: String,
}

/// A validated set of changes, with ids resolved and no-ops left out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pr_id: String,
    add_labels: Vec<Item>,
    remove_labels: Vec<Item>,
    add_users: Vec<Item>,
    remove_users: Vec<Item>,
    /// Changes that were already in place.
    notes: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    /// What was changed, and what was already in place.
    pub changes: Vec<String>,
    /// Set when a change failed; later changes were not tried.
    pub error: Option<String>,
    /// The directives that were not applied, so they can be put back in the review body.
    pub unapplied: Directives,
}

pub(crate) const LOOKUP: &str =
    "query($owner: String!, $repo: String!, $number: Int!, $after: String) {
  viewer { login }
  repository(owner: $owner, name: $repo) {
    labels(first: 100, after: $after) { nodes { id name } pageInfo { hasNextPage endCursor } }
    pullRequest(number: $number) {
      labels(first: 100) { nodes { id } }
      assignees(first: 100) { nodes { id login } }
    }
  }
}";
const USER: &str = "query($owner: String!, $repo: String!, $login: String!) {
  repository(owner: $owner, name: $repo) {
    assignableUsers(query: $login, first: 20) { nodes { id login } }
  }
}";
pub(crate) const ADD_LABELS: &str = "mutation($input: AddLabelsToLabelableInput!) {
  addLabelsToLabelable(input: $input) { clientMutationId }
}";
const REMOVE_LABELS: &str = "mutation($input: RemoveLabelsFromLabelableInput!) {
  removeLabelsFromLabelable(input: $input) { clientMutationId }
}";
const ADD_ASSIGNEES: &str = "mutation($input: AddAssigneesToAssignableInput!) {
  addAssigneesToAssignable(input: $input) { clientMutationId }
}";
const REMOVE_ASSIGNEES: &str = "mutation($input: RemoveAssigneesFromAssignableInput!) {
  removeAssigneesFromAssignable(input: $input) { clientMutationId }
}";

fn same(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

fn conflicts(d: &Directives) -> Vec<String> {
    let mut out = vec![];
    for (adds, removes, what) in [
        (&d.add_labels, &d.remove_labels, "label"),
        (&d.add_assignees, &d.remove_assignees, "assignee"),
    ] {
        for a in adds {
            if removes.iter().any(|r| same(a, r)) {
                out.push(format!("{what} '{a}'"));
            }
        }
    }
    out
}

fn nodes<'a>(v: &'a Value, pointer: &str) -> &'a [Value] {
    v.pointer(pointer)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn item(v: &Value, name_key: &str) -> Option<Item> {
    Some(Item {
        id: v.get("id")?.as_str()?.to_string(),
        name: v.get(name_key)?.as_str()?.to_string(),
    })
}

fn items(v: &Value, pointer: &str, name_key: &str) -> Vec<Item> {
    nodes(v, pointer)
        .iter()
        .filter_map(|n| item(n, name_key))
        .collect()
}

async fn repo_labels(
    gh: &impl GraphQl,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result<(String, Vec<Item>, Vec<Item>, Vec<Item>)> {
    let mut labels = vec![];
    let mut after = Value::Null;
    loop {
        let page = gh
            .call(
                LOOKUP,
                json!({ "owner": owner, "repo": repo, "number": number, "after": after }),
            )
            .await?;
        labels.extend(items(&page, "/repository/labels/nodes", "name"));
        let info = page.pointer("/repository/labels/pageInfo");
        let more = info
            .and_then(|i| i.get("hasNextPage"))
            .and_then(Value::as_bool)
            == Some(true);
        if !more {
            let viewer = page
                .pointer("/viewer/login")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let on_pr = items(&page, "/repository/pullRequest/labels/nodes", "id")
                .into_iter()
                .map(|i| i.id)
                .collect::<Vec<_>>();
            let on_pr_labels = labels
                .iter()
                .filter(|l| on_pr.contains(&l.id))
                .cloned()
                .collect();
            let assignees = items(&page, "/repository/pullRequest/assignees/nodes", "login");
            return Ok((viewer, labels, on_pr_labels, assignees));
        }
        after = info
            .and_then(|i| i.get("endCursor"))
            .cloned()
            .unwrap_or(Value::Null);
    }
}

/// Checks every name against GitHub and resolves it to an id. Fails, changing nothing, when a
/// label or user does not exist or a directive contradicts another.
pub async fn plan(
    gh: &impl GraphQl,
    owner: &str,
    repo: &str,
    number: u64,
    pr_id: &str,
    d: &Directives,
) -> Result<Plan> {
    let contradictions = conflicts(d);
    if !contradictions.is_empty() {
        bail!(
            "The directives both add and remove {}.",
            contradictions.join(", ")
        );
    }
    let (viewer, all_labels, pr_labels, assignees) = repo_labels(gh, owner, repo, number).await?;

    let mut plan = Plan {
        pr_id: pr_id.to_string(),
        ..Plan::default()
    };
    let mut unknown_labels = vec![];
    for (names, add) in [(&d.add_labels, true), (&d.remove_labels, false)] {
        for name in names {
            let Some(label) = all_labels.iter().find(|l| same(&l.name, name)) else {
                unknown_labels.push(format!("'{name}'"));
                continue;
            };
            let present = pr_labels.iter().any(|l| l.id == label.id);
            match (add, present) {
                (true, false) => plan.add_labels.push(label.clone()),
                (false, true) => plan.remove_labels.push(label.clone()),
                (true, true) => plan
                    .notes
                    .push(format!("Label {} was already on the PR", label.name)),
                (false, false) => plan
                    .notes
                    .push(format!("Label {} was not on the PR", label.name)),
            }
        }
    }

    let mut unknown_users = vec![];
    for (names, add) in [(&d.add_assignees, true), (&d.remove_assignees, false)] {
        for name in names {
            let login = if same(name, "me") { &viewer } else { name };
            let current = assignees.iter().find(|a| same(&a.name, login)).cloned();
            let user = match current.clone() {
                Some(u) => Some(u),
                None => lookup_user(gh, owner, repo, login).await?,
            };
            let Some(user) = user else {
                unknown_users.push(format!("'{name}'"));
                continue;
            };
            match (add, current.is_some()) {
                (true, false) => plan.add_users.push(user),
                (false, true) => plan.remove_users.push(user),
                (true, true) => plan
                    .notes
                    .push(format!("@{} was already assigned", user.name)),
                (false, false) => plan.notes.push(format!("@{} was not assigned", user.name)),
            }
        }
    }

    let mut problems = vec![];
    if !unknown_labels.is_empty() {
        problems.push(format!(
            "no such label in {owner}/{repo}: {}",
            unknown_labels.join(", ")
        ));
    }
    if !unknown_users.is_empty() {
        problems.push(format!(
            "no such assignable user: {}",
            unknown_users.join(", ")
        ));
    }
    if !problems.is_empty() {
        bail!("{}.", problems.join("; "));
    }
    Ok(plan)
}

async fn lookup_user(
    gh: &impl GraphQl,
    owner: &str,
    repo: &str,
    login: &str,
) -> Result<Option<Item>> {
    let found = gh
        .call(
            USER,
            json!({ "owner": owner, "repo": repo, "login": login }),
        )
        .await?;
    Ok(items(&found, "/repository/assignableUsers/nodes", "login")
        .into_iter()
        .find(|u| same(&u.name, login)))
}

fn names(items: &[Item]) -> Vec<String> {
    items.iter().map(|i| i.name.clone()).collect()
}

fn ids(items: &[Item]) -> Vec<String> {
    items.iter().map(|i| i.id.clone()).collect()
}

/// Applies the plan in order (add labels, remove labels, assign, unassign). The first failure
/// stops it; everything not yet applied comes back as `unapplied`.
pub async fn apply(gh: &impl GraphQl, plan: &Plan) -> Applied {
    let mut out = Applied {
        changes: plan.notes.clone(),
        ..Applied::default()
    };
    let steps: [(&str, &str, &str, &[Item], &str); 4] = [
        (
            ADD_LABELS,
            "labelableId",
            "labelIds",
            &plan.add_labels,
            "Added label",
        ),
        (
            REMOVE_LABELS,
            "labelableId",
            "labelIds",
            &plan.remove_labels,
            "Removed label",
        ),
        (
            ADD_ASSIGNEES,
            "assignableId",
            "assigneeIds",
            &plan.add_users,
            "Assigned",
        ),
        (
            REMOVE_ASSIGNEES,
            "assignableId",
            "assigneeIds",
            &plan.remove_users,
            "Unassigned",
        ),
    ];
    let mut failed_at = None;
    for (i, (query, target, list, items, verb)) in steps.iter().enumerate() {
        if items.is_empty() {
            continue;
        }
        let input = json!({ *target: plan.pr_id, *list: ids(items) });
        match gh.call(query, json!({ "input": input })).await {
            Ok(_) => {
                for name in names(items) {
                    let shown = if verb.ends_with("label") {
                        name
                    } else {
                        format!("@{name}")
                    };
                    out.changes.push(format!("{verb} {shown}"));
                }
            }
            Err(e) => {
                out.error = Some(format!("{verb} {}: {e:#}", names(items).join(", ")));
                failed_at = Some(i);
                break;
            }
        }
    }
    if let Some(from) = failed_at {
        let left = |i: usize, items: &[Item]| if i >= from { names(items) } else { vec![] };
        out.unapplied = Directives {
            add_labels: left(0, &plan.add_labels),
            remove_labels: left(1, &plan.remove_labels),
            add_assignees: left(2, &plan.add_users),
            remove_assignees: left(3, &plan.remove_users),
        };
    }
    out
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    struct Fake {
        calls: Mutex<Vec<(&'static str, Value)>>,
        fail: &'static [&'static str],
    }

    impl Fake {
        fn new() -> Self {
            Fake {
                calls: Mutex::new(vec![]),
                fail: &[],
            }
        }
        fn names(&self) -> Vec<&'static str> {
            self.calls.lock().unwrap().iter().map(|(n, _)| *n).collect()
        }
    }

    impl GraphQl for Fake {
        async fn call(&self, query: &str, variables: Value) -> Result<Value> {
            let (name, response) = match query {
                LOOKUP => (
                    "lookup",
                    json!({
                        "viewer": {"login": "Me"},
                        "repository": {
                            "labels": {
                                "nodes": [
                                    {"id": "L1", "name": "waiting-for-risk-review"},
                                    {"id": "L2", "name": "waiting-for-team"},
                                    {"id": "L3", "name": "risk-review-ongoing"},
                                ],
                                "pageInfo": {"hasNextPage": false, "endCursor": null},
                            },
                            "pullRequest": {
                                "labels": {"nodes": [{"id": "L1"}, {"id": "L3"}]},
                                "assignees": {"nodes": [{"id": "U9", "login": "carol"}]},
                            },
                        },
                    }),
                ),
                USER => {
                    let login = variables["login"].as_str().unwrap().to_lowercase();
                    let nodes = match login.as_str() {
                        "sighol" => {
                            json!([{"id": "U1", "login": "sighol"}, {"id": "U2", "login": "sigholm"}])
                        }
                        _ => json!([]),
                    };
                    (
                        "user",
                        json!({"repository": {"assignableUsers": {"nodes": nodes}}}),
                    )
                }
                ADD_LABELS => ("add_labels", json!({})),
                REMOVE_LABELS => ("remove_labels", json!({})),
                ADD_ASSIGNEES => ("add_users", json!({})),
                REMOVE_ASSIGNEES => ("remove_users", json!({})),
                other => panic!("unexpected query {other}"),
            };
            self.calls.lock().unwrap().push((name, variables));
            if self.fail.contains(&name) {
                bail!("boom in {name}");
            }
            Ok(response)
        }
    }

    fn d(add: &[&str], remove: &[&str], assign: &[&str], unassign: &[&str]) -> Directives {
        let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect();
        Directives {
            add_labels: v(add),
            remove_labels: v(remove),
            add_assignees: v(assign),
            remove_assignees: v(unassign),
        }
    }

    #[tokio::test]
    async fn plans_and_applies_the_example() {
        let gh = Fake::new();
        let directives = d(
            &["waiting-for-team", "risk-review-ongoing"],
            &["waiting-for-risk-review"],
            &["SigHol"],
            &[],
        );
        let plan = plan(&gh, "o", "r", 7, "PR1", &directives).await.unwrap();
        // risk-review-ongoing is already on the PR, so only waiting-for-team is added.
        assert_eq!(names(&plan.add_labels), ["waiting-for-team"]);
        assert_eq!(names(&plan.remove_labels), ["waiting-for-risk-review"]);
        assert_eq!(names(&plan.add_users), ["sighol"]);
        assert_eq!(
            plan.notes,
            ["Label risk-review-ongoing was already on the PR"]
        );

        let applied = apply(&gh, &plan).await;
        assert_eq!(applied.error, None);
        assert_eq!(
            applied.changes,
            [
                "Label risk-review-ongoing was already on the PR",
                "Added label waiting-for-team",
                "Removed label waiting-for-risk-review",
                "Assigned @sighol",
            ]
        );
        assert_eq!(
            gh.names(),
            ["lookup", "user", "add_labels", "remove_labels", "add_users"]
        );
        let calls = gh.calls.lock().unwrap();
        assert_eq!(
            calls[2].1["input"],
            json!({"labelableId": "PR1", "labelIds": ["L2"]})
        );
        assert_eq!(
            calls[4].1["input"],
            json!({"assignableId": "PR1", "assigneeIds": ["U1"]})
        );
    }

    #[tokio::test]
    async fn contradicting_assignees_fail_and_a_current_assignee_needs_no_lookup() {
        let gh = Fake::new();
        let err = plan(
            &gh,
            "o",
            "r",
            7,
            "PR1",
            &d(&[], &[], &["carol"], &["carol"]),
        )
        .await
        .unwrap_err();
        assert!(
            err.to_string()
                .contains("both add and remove assignee 'carol'")
        );
        assert_eq!(gh.names(), [] as [&str; 0]);

        let ok = plan(&gh, "o", "r", 7, "PR1", &d(&[], &[], &["CAROL"], &[]))
            .await
            .unwrap();
        assert!(ok.add_users.is_empty());
        assert_eq!(ok.notes, ["@carol was already assigned"]);
        assert_eq!(gh.names(), ["lookup"]);
    }

    #[tokio::test]
    async fn contradictions_are_refused_before_any_call() {
        let gh = Fake::new();
        let err = plan(&gh, "o", "r", 7, "PR1", &d(&["Foo"], &["foo"], &[], &[]))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("both add and remove label 'Foo'"));
        assert_eq!(gh.names(), [] as [&str; 0]);
    }

    #[tokio::test]
    async fn unknown_labels_and_users_are_all_reported_and_nothing_is_changed() {
        let gh = Fake::new();
        let err = plan(
            &gh,
            "o",
            "r",
            7,
            "PR1",
            &d(&["waiting-for-tem"], &["nope"], &["ghost"], &[]),
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(err.contains("'waiting-for-tem', 'nope'"), "{err}");
        assert!(err.contains("no such assignable user: 'ghost'"), "{err}");
        assert!(
            !gh.names()
                .iter()
                .any(|n| n.starts_with("add") || n.starts_with("remove"))
        );
    }

    #[tokio::test]
    async fn removing_what_is_absent_is_a_noop() {
        let gh = Fake::new();
        let plan = plan(
            &gh,
            "o",
            "r",
            7,
            "PR1",
            &d(&[], &["waiting-for-team"], &[], &["carol"]),
        )
        .await
        .unwrap();
        assert!(plan.remove_labels.is_empty());
        assert_eq!(names(&plan.remove_users), ["carol"]);
        assert_eq!(plan.notes, ["Label waiting-for-team was not on the PR"]);
    }

    #[tokio::test]
    async fn a_failure_stops_and_returns_what_was_not_applied() {
        let gh = Fake {
            fail: &["remove_labels"],
            ..Fake::new()
        };
        let plan = plan(
            &gh,
            "o",
            "r",
            7,
            "PR1",
            &d(
                &["waiting-for-team"],
                &["waiting-for-risk-review"],
                &["sighol"],
                &["carol"],
            ),
        )
        .await
        .unwrap();
        let applied = apply(&gh, &plan).await;
        assert!(applied.error.unwrap().contains("boom in remove_labels"));
        assert_eq!(applied.changes, ["Added label waiting-for-team"]);
        assert_eq!(
            applied.unapplied,
            d(&[], &["waiting-for-risk-review"], &["sighol"], &["carol"])
        );
        assert!(!gh.names().contains(&"add_users"));
    }

    #[tokio::test]
    async fn pages_through_the_repo_labels() {
        struct Paged;
        impl GraphQl for Paged {
            async fn call(&self, _query: &str, variables: Value) -> Result<Value> {
                let (nodes, next) = if variables["after"].is_null() {
                    (json!([{"id": "L1", "name": "a"}]), json!("c1"))
                } else {
                    (json!([{"id": "L2", "name": "b"}]), Value::Null)
                };
                Ok(json!({"viewer": {"login": "x"}, "repository": {
                    "labels": {"nodes": nodes, "pageInfo": {"hasNextPage": !next.is_null(), "endCursor": next}},
                    "pullRequest": {"labels": {"nodes": []}, "assignees": {"nodes": []}},
                }}))
            }
        }
        let plan = plan(&Paged, "o", "r", 1, "PR", &d(&["b"], &[], &[], &[]))
            .await
            .unwrap();
        assert_eq!(names(&plan.add_labels), ["b"]);
    }
}
