use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use serde::Serialize;
use tracing::warn;

use crate::git;
use crate::github::PrMeta;
use crate::threads::Thread;

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
        .map(|(path, (added, removed))| {
            let mut touched_by = changed_by.remove(&path).unwrap_or_default();
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
                added,
                removed,
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
        assert!(m.files.iter().all(|f| !f.changed_by.is_empty()), "{:?}", m.files);
        let evil = m.files.iter().find(|f| f.path == "evil.txt").unwrap();
        assert_eq!(evil.changed_by, vec![m.commits.len()]);
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
