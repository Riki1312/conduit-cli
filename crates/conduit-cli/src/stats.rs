use crate::test_report::TestReportError;
use serde::{Deserialize, Serialize};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

const STATS_DIR_ENV: &str = "CONDUIT_STATS_DIR";
const XDG_STATE_HOME_ENV: &str = "XDG_STATE_HOME";
const HOME_ENV: &str = "HOME";
const STATS_FILE: &str = "stats.json";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StatsUpdate {
    pub(crate) commands: u64,
    pub(crate) test_runs: u64,
    pub(crate) raw_log_lines: u64,
    pub(crate) conduit_output_lines: u64,
    pub(crate) raw_log_bytes: u64,
    pub(crate) conduit_output_bytes: u64,
}

impl StatsUpdate {
    pub(crate) fn command() -> Self {
        Self {
            commands: 1,
            ..Self::default()
        }
    }

    pub(crate) fn add_test_run(&mut self, raw_log: Option<&str>, conduit_output: &str) {
        self.test_runs += 1;
        if let Some(raw_log) = raw_log {
            self.raw_log_lines += count_lines(raw_log);
            self.raw_log_bytes += raw_log.len() as u64;
        }
        self.conduit_output_lines += count_lines(conduit_output);
        self.conduit_output_bytes += conduit_output.len() as u64;
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct Stats {
    version: u32,
    pub(crate) commands: u64,
    pub(crate) test_runs: u64,
    pub(crate) raw_log_lines: u64,
    pub(crate) conduit_output_lines: u64,
    pub(crate) raw_log_bytes: u64,
    pub(crate) conduit_output_bytes: u64,
}

impl Default for Stats {
    fn default() -> Self {
        Self {
            version: 1,
            commands: 0,
            test_runs: 0,
            raw_log_lines: 0,
            conduit_output_lines: 0,
            raw_log_bytes: 0,
            conduit_output_bytes: 0,
        }
    }
}

impl Stats {
    fn apply(&mut self, update: &StatsUpdate) {
        self.commands += update.commands;
        self.test_runs += update.test_runs;
        self.raw_log_lines += update.raw_log_lines;
        self.conduit_output_lines += update.conduit_output_lines;
        self.raw_log_bytes += update.raw_log_bytes;
        self.conduit_output_bytes += update.conduit_output_bytes;
    }
}

#[derive(Debug, PartialEq, Serialize)]
pub(crate) struct StatsView {
    pub(crate) scope: &'static str,
    pub(crate) commands: u64,
    pub(crate) test_runs: u64,
    pub(crate) raw_log_lines: u64,
    pub(crate) conduit_output_lines: u64,
    pub(crate) line_reduction_percent: f64,
    pub(crate) raw_log_bytes: u64,
    pub(crate) conduit_output_bytes: u64,
    pub(crate) byte_reduction_percent: f64,
}

impl StatsView {
    pub(crate) fn from_stats(stats: &Stats) -> Self {
        Self {
            scope: "user",
            commands: stats.commands,
            test_runs: stats.test_runs,
            raw_log_lines: stats.raw_log_lines,
            conduit_output_lines: stats.conduit_output_lines,
            line_reduction_percent: reduction_percent(
                stats.raw_log_lines,
                stats.conduit_output_lines,
            ),
            raw_log_bytes: stats.raw_log_bytes,
            conduit_output_bytes: stats.conduit_output_bytes,
            byte_reduction_percent: reduction_percent(
                stats.raw_log_bytes,
                stats.conduit_output_bytes,
            ),
        }
    }

    pub(crate) fn render_text(&self) -> String {
        [
            format!("scope: {}", self.scope),
            format!("commands: {}", self.commands),
            format!("test_runs: {}", self.test_runs),
            format!("raw_log_lines: {}", self.raw_log_lines),
            format!("conduit_output_lines: {}", self.conduit_output_lines),
            format!("line_reduction: {:.1}%", self.line_reduction_percent),
            format!("raw_log_bytes: {}", self.raw_log_bytes),
            format!("conduit_output_bytes: {}", self.conduit_output_bytes),
            format!("byte_reduction: {:.1}%", self.byte_reduction_percent),
        ]
        .join("\n")
    }
}

pub(crate) fn read_stats() -> Result<Stats, TestReportError> {
    let path = stats_path();
    if !path.exists() {
        return Ok(Stats::default());
    }

    let json = fs::read_to_string(&path).map_err(|error| {
        TestReportError::new(format!(
            "failed to read stats from {}: {error}",
            path.display()
        ))
    })?;

    serde_json::from_str(&json).map_err(|error| {
        TestReportError::new(format!(
            "failed to parse stats from {}: {error}",
            path.display()
        ))
    })
}

pub(crate) fn record_stats(update: &StatsUpdate) -> Result<(), TestReportError> {
    let mut stats = read_stats()?;
    stats.apply(update);
    write_stats(&stats)
}

fn write_stats(stats: &Stats) -> Result<(), TestReportError> {
    let path = stats_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            TestReportError::new(format!(
                "failed to create stats directory {}: {error}",
                parent.display()
            ))
        })?;
    }

    let json = serde_json::to_string_pretty(stats)
        .map_err(|error| TestReportError::new(format!("failed to serialize stats: {error}")))?;

    fs::write(&path, json).map_err(|error| {
        TestReportError::new(format!("failed to write stats {}: {error}", path.display()))
    })
}

fn stats_path() -> PathBuf {
    stats_dir().join(STATS_FILE)
}

fn stats_dir() -> PathBuf {
    stats_dir_from_env(
        env::var_os(STATS_DIR_ENV),
        env::var_os(XDG_STATE_HOME_ENV),
        env::var_os(HOME_ENV),
    )
}

fn stats_dir_from_env(
    stats_dir: Option<OsString>,
    xdg_state_home: Option<OsString>,
    home: Option<OsString>,
) -> PathBuf {
    if let Some(path) = stats_dir {
        return PathBuf::from(path);
    }

    if let Some(path) = xdg_state_home {
        return PathBuf::from(path).join("conduit");
    }

    if let Some(path) = home {
        return PathBuf::from(path).join(".local/state/conduit");
    }

    PathBuf::from(".conduit/state")
}

fn count_lines(value: &str) -> u64 {
    value.lines().count() as u64
}

fn reduction_percent(raw: u64, conduit: u64) -> f64 {
    if raw == 0 {
        return 0.0;
    }

    let reduction = raw.saturating_sub(conduit);
    (reduction as f64 / raw as f64) * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_zero_stats_without_reduction() {
        let view = StatsView::from_stats(&Stats::default());

        assert_eq!(
            view.render_text(),
            "scope: user\ncommands: 0\ntest_runs: 0\nraw_log_lines: 0\nconduit_output_lines: 0\nline_reduction: 0.0%\nraw_log_bytes: 0\nconduit_output_bytes: 0\nbyte_reduction: 0.0%"
        );
    }

    #[test]
    fn applies_updates_and_computes_reduction() {
        let mut stats = Stats::default();
        let mut update = StatsUpdate::command();
        update.add_test_run(Some("one\ntwo\nthree\nfour\n"), "status: passed\n");

        stats.apply(&update);
        let view = StatsView::from_stats(&stats);

        assert_eq!(view.commands, 1);
        assert_eq!(view.test_runs, 1);
        assert_eq!(view.raw_log_lines, 4);
        assert_eq!(view.conduit_output_lines, 1);
        assert_eq!(view.line_reduction_percent, 75.0);
    }

    #[test]
    fn resolves_user_stats_directory() {
        assert_eq!(
            stats_dir_from_env(
                Some("/tmp/conduit-stats-test".into()),
                Some("/tmp/xdg-state".into()),
                Some("/tmp/home".into())
            ),
            PathBuf::from("/tmp/conduit-stats-test")
        );
        assert_eq!(
            stats_dir_from_env(
                None,
                Some("/tmp/xdg-state".into()),
                Some("/tmp/home".into())
            ),
            PathBuf::from("/tmp/xdg-state/conduit")
        );
        assert_eq!(
            stats_dir_from_env(None, None, Some("/tmp/home".into())),
            PathBuf::from("/tmp/home/.local/state/conduit")
        );
    }
}
