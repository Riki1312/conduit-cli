use crate::test_report::TestReportError;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq, Serialize)]
pub(crate) struct TestLogSummary {
    pub(crate) path: String,
    pub(crate) lines: Vec<String>,
}

impl TestLogSummary {
    pub(crate) fn latest(tail_lines: usize) -> Result<Self, TestReportError> {
        let path = latest_log_path()?;
        Self::from_path(&path, tail_lines)
    }

    pub(crate) fn from_path(path: &Path, tail_lines: usize) -> Result<Self, TestReportError> {
        let content = fs::read_to_string(path).map_err(|error| {
            TestReportError::new(format!("failed to read log {}: {error}", path.display()))
        })?;

        Ok(Self {
            path: path.to_string_lossy().to_string(),
            lines: tail(&content, tail_lines),
        })
    }

    pub(crate) fn render_text(&self) -> String {
        let mut lines = vec![
            format!("path: {}", self.path),
            format!("lines: {}", self.lines.len()),
        ];
        lines.extend(self.lines.iter().map(|line| format!("log: {line}")));
        lines.join("\n")
    }
}

fn latest_log_path() -> Result<PathBuf, TestReportError> {
    let directory = state_logs_dir();
    let entries = fs::read_dir(&directory).map_err(|error| {
        TestReportError::new(format!(
            "failed to read log directory {}: {error}",
            directory.display()
        ))
    })?;

    let mut latest = None;

    for entry in entries {
        let entry = entry.map_err(|error| {
            TestReportError::new(format!(
                "failed to read log entry under {}: {error}",
                directory.display()
            ))
        })?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        let modified = path
            .metadata()
            .and_then(|metadata| metadata.modified())
            .map_err(|error| {
                TestReportError::new(format!("failed to inspect log {}: {error}", path.display()))
            })?;

        if latest
            .as_ref()
            .is_none_or(|(latest_modified, _)| modified > *latest_modified)
        {
            latest = Some((modified, path));
        }
    }

    latest.map(|(_, path)| path).ok_or_else(|| {
        TestReportError::new(format!("no test logs found under {}", directory.display()))
    })
}

fn state_logs_dir() -> PathBuf {
    std::env::var_os("CONDUIT_STATE_DIR")
        .map_or_else(|| PathBuf::from(".conduit/state"), PathBuf::from)
        .join("logs")
}

fn tail(content: &str, tail_lines: usize) -> Vec<String> {
    let lines = content
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();

    let start = lines.len().saturating_sub(tail_lines);
    lines[start..]
        .iter()
        .map(std::string::ToString::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_bounded_tail() {
        assert_eq!(tail("one\ntwo\nthree\n", 2), ["two", "three"]);
    }
}
