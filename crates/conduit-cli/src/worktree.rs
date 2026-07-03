use crate::git_status::{GitStatusError, GitStatusSummary};
use serde::Serialize;
use std::fs;
use std::path::Path;

#[derive(Debug, PartialEq, Eq, Serialize)]
pub(crate) struct WorktreeListSummary {
    pub(crate) root: String,
    pub(crate) worktrees: Vec<WorktreeSummary>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub(crate) struct WorktreeSummary {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) status: GitStatusSummary,
}

impl WorktreeListSummary {
    pub(crate) fn load(root: &Path) -> Result<Self, GitStatusError> {
        let mut worktrees = Vec::new();

        for entry in fs::read_dir(root).map_err(|error| {
            GitStatusError::new(format!(
                "failed to read worktree root {}: {error}",
                root.display()
            ))
        })? {
            let entry = entry.map_err(|error| {
                GitStatusError::new(format!(
                    "failed to read worktree entry under {}: {error}",
                    root.display()
                ))
            })?;
            let path = entry.path();

            if !path.is_dir() || !path.join(".git").exists() {
                continue;
            }

            let status = GitStatusSummary::load(&path)?;
            worktrees.push(WorktreeSummary {
                name: path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("")
                    .to_string(),
                path: path.to_string_lossy().to_string(),
                status,
            });
        }

        worktrees.sort_by(|left, right| left.name.cmp(&right.name));

        Ok(Self {
            root: root.to_string_lossy().to_string(),
            worktrees,
        })
    }

    pub(crate) fn render_text(&self) -> String {
        let mut lines = vec![
            format!("root: {}", self.root),
            format!("worktrees: {}", self.worktrees.len()),
        ];

        for worktree in &self.worktrees {
            lines.push(String::new());
            lines.push(format!("worktree: {}", worktree.name));
            lines.push(format!("repo: {}", worktree.status.repo));
            lines.push(format!(
                "branch: {}",
                worktree.status.branch.as_deref().unwrap_or("none")
            ));
            lines.push(format!("dirty: {}", worktree.status.dirty));
            lines.push(format!("changed: {}", worktree.status.changed));
            lines.push(format!("ahead: {}", worktree.status.ahead));
            lines.push(format!("behind: {}", worktree.status.behind));
            lines.push(format!("path: {}", worktree.path));
        }

        lines.join("\n")
    }
}
