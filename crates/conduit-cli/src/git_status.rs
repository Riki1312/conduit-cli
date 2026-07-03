use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct GitStatusError {
    pub(crate) message: String,
}

impl GitStatusError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub(crate) struct GitStatusSummary {
    pub(crate) repo: String,
    pub(crate) root: String,
    pub(crate) branch: Option<String>,
    pub(crate) upstream: Option<String>,
    pub(crate) ahead: i32,
    pub(crate) behind: i32,
    pub(crate) dirty: bool,
    pub(crate) changed: usize,
}

impl GitStatusSummary {
    pub(crate) fn load(path: &Path) -> Result<Self, GitStatusError> {
        let root = git_root(path)?;
        let output = git(&root, ["status", "--porcelain=v2", "--branch"])?;
        Ok(Self::parse(&root, &output))
    }

    pub(crate) fn parse(root: &Path, output: &str) -> Self {
        let mut branch = None;
        let mut upstream = None;
        let mut ahead = 0;
        let mut behind = 0;
        let mut changed = 0;

        for line in output.lines() {
            if let Some(value) = line.strip_prefix("# branch.head ") {
                branch = if value == "(detached)" {
                    None
                } else {
                    Some(value.to_string())
                };
            } else if let Some(value) = line.strip_prefix("# branch.upstream ") {
                upstream = Some(value.to_string());
            } else if let Some(value) = line.strip_prefix("# branch.ab ") {
                if let Some((parsed_ahead, parsed_behind)) = parse_ahead_behind(value) {
                    ahead = parsed_ahead;
                    behind = parsed_behind;
                }
            } else if !line.starts_with('#') && !line.is_empty() {
                changed += 1;
            }
        }

        Self {
            repo: root
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("")
                .to_string(),
            root: root.to_string_lossy().to_string(),
            branch,
            upstream,
            ahead,
            behind,
            dirty: changed > 0,
            changed,
        }
    }

    pub(crate) fn render_text(&self) -> String {
        [
            format!("repo: {}", self.repo),
            format!("branch: {}", optional(self.branch.as_ref())),
            format!("upstream: {}", optional(self.upstream.as_ref())),
            format!("ahead: {}", self.ahead),
            format!("behind: {}", self.behind),
            format!("dirty: {}", self.dirty),
            format!("changed: {}", self.changed),
            format!("root: {}", self.root),
        ]
        .join("\n")
    }
}

fn git_root(path: &Path) -> Result<PathBuf, GitStatusError> {
    let output = git(path, ["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(output.trim()))
}

fn git<const N: usize>(path: &Path, args: [&str; N]) -> Result<String, GitStatusError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(path)
        .output()
        .map_err(|error| GitStatusError::new(format!("failed to run git: {error}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitStatusError::new(format!(
            "git command failed in {}: {}",
            path.display(),
            stderr.trim()
        )));
    }

    String::from_utf8(output.stdout)
        .map_err(|error| GitStatusError::new(format!("git output was not UTF-8: {error}")))
}

fn parse_ahead_behind(value: &str) -> Option<(i32, i32)> {
    let mut ahead = None;
    let mut behind = None;

    for part in value.split_whitespace() {
        if let Some(value) = part.strip_prefix('+') {
            ahead = value.parse::<i32>().ok();
        } else if let Some(value) = part.strip_prefix('-') {
            behind = value.parse::<i32>().ok();
        }
    }

    Some((ahead?, behind?))
}

fn optional(value: Option<&String>) -> &str {
    value.map_or("none", String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_clean_porcelain_v2_status() {
        let status = GitStatusSummary::parse(
            Path::new("/tmp/repo"),
            "# branch.oid abc\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +2 -3\n",
        );

        assert_eq!(status.repo, "repo");
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!(status.upstream.as_deref(), Some("origin/main"));
        assert_eq!(status.ahead, 2);
        assert_eq!(status.behind, 3);
        assert!(!status.dirty);
        assert_eq!(status.changed, 0);
    }

    #[test]
    fn counts_changed_entries() {
        let status = GitStatusSummary::parse(
            Path::new("/tmp/repo"),
            "# branch.head feature\n1 .M N... 100644 100644 100644 a b file.txt\n? new.txt\n",
        );

        assert!(status.dirty);
        assert_eq!(status.changed, 2);
    }
}
