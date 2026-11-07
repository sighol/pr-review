use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use serde::Serialize;
use tracing::warn;

use crate::git;
use crate::github::{PrMeta, RawComment, RawThread};
use crate::threads::{Thread, build_threads, follow_renames};

/// Same JSON shape as `ui/scripts/make-fixtures.py` writes to `public/pr/manifest.json`,
/// plus `pr` when a real PR was opened.
#[derive(Debug, Clone, Serialize)]
pub struct Manifest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr: Option<PrMeta>,
    /// Review threads and the PR conversation; present when a real PR was opened.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threads: Option<Vec<Thread>>,
    pub base: String,
    pub head: String,
    pub commits: Vec<ManifestCommit>,
    pub files: Vec<ManifestFile>,
}

impl Manifest {
    /// Maps a commit oid to its 1-based number in the PR.
    pub fn commit_numbers(&self) -> HashMap<&str, usize> {
        self.commits.iter().map(|c| (c.oid.as_str(), c.n)).collect()
    }

    /// Maps the path of each renamed file at the PR base to its path at the head.
    pub fn renames(&self) -> HashMap<&str, &str> {
        self.files
            .iter()
            .filter_map(|f| Some((f.old_path.as_deref()?, f.path.as_str())))
            .collect()
    }

    /// The review threads and the conversation from GitHub's raw data, mapped onto this PR's
    /// commit numbers and file names.
    pub fn threads_from(&self, raw: Vec<RawThread>, conversation: Vec<RawComment>) -> Vec<Thread> {
        let mut threads = build_threads(raw, conversation, &self.commit_numbers());
        follow_renames(&mut threads, &self.renames());
        threads
    }

    pub fn set_threads(&mut self, raw: Vec<RawThread>, conversation: Vec<RawComment>) {
        self.threads = Some(self.threads_from(raw, conversation));
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ManifestCommit {
    /// 1-based position in the PR; state 0 of a file is the PR base.
    pub n: usize,
    pub oid: String,
    pub short: String,
    pub subject: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestFile {
    pub path: String,
    /// The file's path at the PR base, when git detected a rename.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    pub dir: String,
    pub added: usize,
    pub removed: usize,
    /// Commit numbers (`ManifestCommit::n`) that touched this file.
    pub changed_by: Vec<usize>,
}

pub async fn build_manifest(repo: &Path, base_rev: &str, head_rev: &str) -> Result<Manifest> {
    let base = git::resolve_commit(repo, base_rev).await?;
    let head = git::resolve_commit(repo, head_rev).await?;
    let commits = git::commits_in_range(repo, &base, &head).await?;
    let stats = git::numstat(repo, &base, &head).await?;

    let mut changed_by: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, commit) in commits.iter().enumerate() {
        for path in git::files_in_commit(repo, &commit.oid).await? {
            changed_by.entry(path).or_default().push(i + 1);
        }
    }

    let last_commit = commits.len();
    let files = stats
        .into_iter()
        .map(|(path, stat)| {
            let mut touched_by = changed_by.remove(&path).unwrap_or_default();
            // A renamed file was touched under both names (each commit's own diff does not
            // follow renames), so its history is the union.
            if let Some(old) = &stat.old_path {
                touched_by.extend(changed_by.remove(old).unwrap_or_default());
                touched_by.sort_unstable();
                touched_by.dedup();
            }
            if touched_by.is_empty() {
                // The file differs between base and head, but no single commit's own diff shows it
                // (a merge commit that resolved a conflict, for instance). Attribute it to the last
                // commit so it can still be reviewed instead of breaking the commit ranges.
                warn!("{path} changed in the PR but no commit touches it on its own; attributing it to the last commit");
                touched_by.push(last_commit);
            }
            ManifestFile {
                dir: path.replace('/', "__"),
                changed_by: touched_by,
                path,
                old_path: stat.old_path,
                added: stat.added,
                removed: stat.removed,
            }
        })
        .collect();

    let commits = commits
        .into_iter()
        .enumerate()
        .map(|(i, c)| ManifestCommit {
            n: i + 1,
            short: c.oid.chars().take(7).collect(),
            oid: c.oid,
            subject: c.subject,
        })
        .collect();

    Ok(Manifest {
        pr: None,
        threads: None,
        base,
        head,
        commits,
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::testutil::TestRepo;

    #[tokio::test]
    async fn a_file_only_a_merge_commit_added_is_attributed_to_the_last_commit() {
        let repo = TestRepo::new();
        repo.git(&["symbolic-ref", "HEAD", "refs/heads/main"]);
        let base = repo.commit("base", &[("a.txt", Some("1\n"))]);
        repo.git(&["checkout", "-q", "-b", "side"]);
        repo.commit("side work", &[("side.txt", Some("s\n"))]);
        repo.git(&["checkout", "-q", "-b", "pr", &base]);
        repo.commit("pr work", &[("a.txt", Some("2\n"))]);
        repo.git(&["merge", "--no-ff", "--no-commit", "-q", "side"]);
        std::fs::write(repo.path().join("evil.txt"), "e\n").unwrap();
        repo.git(&["add", "-A"]);
        repo.git(&["commit", "-q", "-m", "merge side"]);

        let m = build_manifest(repo.path(), &base, "HEAD").await.unwrap();
        assert!(
            m.files.iter().all(|f| !f.changed_by.is_empty()),
            "{:?}",
            m.files
        );
        let evil = m.files.iter().find(|f| f.path == "evil.txt").unwrap();
        assert_eq!(evil.changed_by, vec![m.commits.len()]);
    }

    #[tokio::test]
    async fn a_renamed_file_is_one_file_touched_by_its_old_and_new_names_commits() {
        let repo = TestRepo::new();
        let text: String = (1..=10).map(|i| format!("line {i}\n")).collect();
        let base = repo.commit("base", &[("Old.kt", Some(&text)), ("keep.kt", Some("k\n"))]);
        repo.commit("pure rename", &[("Old.kt", None), ("New.kt", Some(&text))]);
        repo.commit("touch another file", &[("keep.kt", Some("k2\n"))]);
        repo.commit(
            "edit renamed",
            &[("New.kt", Some(&text.replace("line 3\n", "three\n")))],
        );

        let m = build_manifest(repo.path(), &base, "HEAD").await.unwrap();
        let paths: Vec<_> = m.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["New.kt", "keep.kt"]);
        let renamed = &m.files[0];
        assert_eq!(renamed.old_path.as_deref(), Some("Old.kt"));
        assert_eq!(renamed.changed_by, vec![1, 3]);
        assert_eq!((renamed.added, renamed.removed), (1, 1));
        assert_eq!(m.files[1].old_path, None);
    }

    #[tokio::test]
    async fn manifest_matches_fixture_shape() {
        let repo = TestRepo::new();
        let base = repo.commit("base", &[("a.txt", Some("1\n")), ("keep.txt", Some("k\n"))]);
        let c1 = repo.commit("edit a", &[("a.txt", Some("1\n2\n"))]);
        let c2 = repo.commit("add b", &[("dir/b.txt", Some("b\n"))]);
        let c3 = repo.commit("edit a again", &[("a.txt", Some("1\n2\n3\n"))]);

        let m = build_manifest(repo.path(), &base[..8], "HEAD")
            .await
            .unwrap();
        assert_eq!(m.base, base);
        assert_eq!(m.head, c3);
        assert_eq!(
            m.commits
                .iter()
                .map(|c| (c.n, c.oid.as_str(), c.subject.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (1, c1.as_str(), "edit a"),
                (2, c2.as_str(), "add b"),
                (3, c3.as_str(), "edit a again")
            ]
        );
        assert_eq!(m.commits[0].short, c1[..7]);

        let files: Vec<_> = m
            .files
            .iter()
            .map(|f| {
                (
                    f.path.as_str(),
                    f.dir.as_str(),
                    f.added,
                    f.removed,
                    f.changed_by.clone(),
                )
            })
            .collect();
        assert_eq!(
            files,
            vec![
                ("a.txt", "a.txt", 2, 0, vec![1, 3]),
                ("dir/b.txt", "dir__b.txt", 1, 0, vec![2])
            ]
        );

        let json = serde_json::to_value(&m).unwrap();
        assert_eq!(json["files"][0]["changedBy"], serde_json::json!([1, 3]));
    }
}
