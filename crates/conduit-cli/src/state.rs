use crate::test_report::{TestFailureSummary, TestReportError};
use crate::test_run::TestRunSummary;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const STATE_DIR_ENV: &str = "CONDUIT_STATE_DIR";
const DEFAULT_STATE_DIR: &str = ".conduit/state";
const LAST_TEST_FAILURES_FILE: &str = "last-test-failures.json";
const LAST_TEST_RUN_FILE: &str = "last-test-run.json";

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct LastTestFailures {
    pub(crate) version: u32,
    pub(crate) report_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) log_path: Option<String>,
    pub(crate) summary: TestFailureSummary,
}

impl LastTestFailures {
    pub(crate) fn new(report_path: String, summary: TestFailureSummary) -> Self {
        Self {
            version: 1,
            report_path,
            log_path: None,
            summary,
        }
    }

    pub(crate) fn with_log_path(
        report_path: String,
        log_path: String,
        summary: TestFailureSummary,
    ) -> Self {
        Self {
            version: 1,
            report_path,
            log_path: Some(log_path),
            summary,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct LastTestRun {
    pub(crate) version: u32,
    pub(crate) summary: TestRunSummary,
}

impl LastTestRun {
    pub(crate) fn new(summary: TestRunSummary) -> Self {
        Self {
            version: 1,
            summary,
        }
    }
}

pub(crate) fn write_last_test_failures(state: &LastTestFailures) -> Result<(), TestReportError> {
    write_state(&last_test_failures_path(), state)
}

pub(crate) fn write_last_test_run(state: &LastTestRun) -> Result<(), TestReportError> {
    write_state(&last_test_run_path(), state)
}

fn write_state(path: &Path, state: &impl Serialize) -> Result<(), TestReportError> {
    let path = path.to_path_buf();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            TestReportError::new(format!(
                "failed to create state directory {}: {error}",
                parent.display()
            ))
        })?;
    }

    let json = serde_json::to_string_pretty(state)
        .map_err(|error| TestReportError::new(format!("failed to serialize state: {error}")))?;

    fs::write(&path, json).map_err(|error| {
        TestReportError::new(format!("failed to write state {}: {error}", path.display()))
    })
}

pub(crate) fn read_last_test_failures() -> Result<LastTestFailures, TestReportError> {
    let path = last_test_failures_path();
    let json = fs::read_to_string(&path).map_err(|error| {
        TestReportError::new(format!(
            "failed to read last test failures from {}: {error}",
            path.display()
        ))
    })?;

    serde_json::from_str(&json).map_err(|error| {
        TestReportError::new(format!(
            "failed to parse last test failures from {}: {error}",
            path.display()
        ))
    })
}

pub(crate) fn read_last_test_run() -> Result<LastTestRun, TestReportError> {
    let path = last_test_run_path();
    let json = fs::read_to_string(&path).map_err(|error| {
        TestReportError::new(format!(
            "failed to read last test run from {}: {error}",
            path.display()
        ))
    })?;

    serde_json::from_str(&json).map_err(|error| {
        TestReportError::new(format!(
            "failed to parse last test run from {}: {error}",
            path.display()
        ))
    })
}

fn last_test_failures_path() -> PathBuf {
    state_dir().join(LAST_TEST_FAILURES_FILE)
}

fn last_test_run_path() -> PathBuf {
    state_dir().join(LAST_TEST_RUN_FILE)
}

fn state_dir() -> PathBuf {
    env::var_os(STATE_DIR_ENV).map_or_else(|| PathBuf::from(DEFAULT_STATE_DIR), PathBuf::from)
}
