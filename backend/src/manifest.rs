use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use crate::git;
use crate::github::PrMeta;

/// Same JSON shape as `ui/scripts/make-fixtures.py` writes to `public/pr/manifest.json`,
/// plus `pr` when a real PR was opened.
#[derive(Debug, Clone, Serialize)]
pub struct Manifest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr: Option<PrMeta>,
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

    let files = stats
        .into_iter()
        .map(|(path, (added, removed))| ManifestFile {
            dir: path.replace('/', "__"),
            changed_by: changed_by.remove(&path).unwrap_or_default(),
            path,
            added,
            removed,
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

    Ok(Manifest { pr: None, base, head, commits, files })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::testutil::TestRepo;

    #[tokio::test]
    async fn manifest_matches_fixture_shape() {
        let repo = TestRepo::new();
        let base = repo.commit("base", &[("a.txt", Some("1\n")), ("keep.txt", Some("k\n"))]);
        let c1 = repo.commit("edit a", &[("a.txt", Some("1\n2\n"))]);
        let c2 = repo.commit("add b", &[("dir/b.txt", Some("b\n"))]);
        let c3 = repo.commit("edit a again", &[("a.txt", Some("1\n2\n3\n"))]);

        let m = build_manifest(repo.path(), &base[..8], "HEAD").await.unwrap();
        assert_eq!(m.base, base);
        assert_eq!(m.head, c3);
        assert_eq!(
            m.commits.iter().map(|c| (c.n, c.oid.as_str(), c.subject.as_str())).collect::<Vec<_>>(),
            vec![(1, c1.as_str(), "edit a"), (2, c2.as_str(), "add b"), (3, c3.as_str(), "edit a again")]
        );
        assert_eq!(m.commits[0].short, c1[..7]);

        let files: Vec<_> = m
            .files
            .iter()
            .map(|f| (f.path.as_str(), f.dir.as_str(), f.added, f.removed, f.changed_by.clone()))
            .collect();
        assert_eq!(
            files,
            vec![("a.txt", "a.txt", 2, 0, vec![1, 3]), ("dir/b.txt", "dir__b.txt", 1, 0, vec![2])]
        );

        let json = serde_json::to_value(&m).unwrap();
        assert_eq!(json["files"][0]["changedBy"], serde_json::json!([1, 3]));
    }
}
