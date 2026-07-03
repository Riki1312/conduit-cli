use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const MAX_TESTS_WITH_PASSED_SELECTORS: usize = 20;

/// Error returned while reading or parsing JUnit XML reports.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TestReportError {
    pub(crate) message: String,
}

impl TestReportError {
    /// Creates a report error with a compact user-facing message.
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Compact summary extracted from one or more JUnit XML report files.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct TestFailureSummary {
    pub(crate) status: TestStatus,
    #[serde(default)]
    pub(crate) tests_ran: u64,
    #[serde(default)]
    pub(crate) tests_passed: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) passed_selectors: Vec<String>,
    pub(crate) failures: Vec<TestFailure>,
    pub(crate) sources: Vec<String>,
}

impl TestFailureSummary {
    /// Parses all JUnit XML report files under a file or directory.
    pub(crate) fn from_path(path: &Path) -> Result<Self, TestReportError> {
        let sources = junit_xml_files(path)?;
        Self::from_sources(&sources)
    }

    /// Parses only reports modified after `modified_after`.
    pub(crate) fn from_path_modified_after(
        path: &Path,
        modified_after: SystemTime,
    ) -> Result<Self, TestReportError> {
        let sources = junit_xml_files(path)?;
        let sources = sources
            .into_iter()
            .filter(|source| was_modified_after(source, modified_after))
            .collect::<Vec<_>>();

        Self::from_sources(&sources)
    }

    /// Creates an empty summary for runner failures without JUnit XML.
    pub(crate) fn empty(status: TestStatus) -> Self {
        Self {
            status,
            tests_ran: 0,
            tests_passed: 0,
            passed_selectors: Vec::new(),
            failures: Vec::new(),
            sources: Vec::new(),
        }
    }

    fn from_sources(sources: &[PathBuf]) -> Result<Self, TestReportError> {
        let mut failures = Vec::new();
        let mut passed_selectors = Vec::new();
        let mut tests_ran = 0;
        let mut tests_passed = 0;

        for source in sources {
            let source_summary = parse_junit_source(source)?;
            tests_ran += source_summary.tests_ran;
            tests_passed += source_summary.tests_passed;
            passed_selectors.extend(source_summary.passed_selectors);
            failures.extend(source_summary.failures);
        }

        passed_selectors.sort();
        if tests_ran > MAX_TESTS_WITH_PASSED_SELECTORS as u64 {
            passed_selectors.clear();
        } else {
            passed_selectors.truncate(MAX_TESTS_WITH_PASSED_SELECTORS);
        }

        failures.sort_by(|left, right| {
            left.class_name
                .cmp(&right.class_name)
                .then(left.name.cmp(&right.name))
                .then(left.kind.cmp(&right.kind))
        });

        Ok(Self {
            status: if failures.is_empty() {
                TestStatus::Passed
            } else {
                TestStatus::Failed
            },
            tests_ran,
            tests_passed,
            passed_selectors,
            failures,
            sources: sources
                .iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect(),
        })
    }

    /// Renders the summary as compact deterministic text.
    pub(crate) fn render_text(&self) -> String {
        let mut lines = vec![
            format!("status: {}", self.status.as_str()),
            format!("tests_ran: {}", self.tests_ran),
            format!("tests_passed: {}", self.tests_passed),
            format!("failures: {}", self.failures.len()),
            format!("sources: {}", self.sources.len()),
        ];

        if !self.passed_selectors.is_empty() {
            lines.push(format!("passed_selectors: {}", self.passed_selectors.len()));
            lines.extend(
                self.passed_selectors
                    .iter()
                    .map(|selector| format!("passed: {selector}")),
            );
        }

        for failure in &self.failures {
            lines.push(String::new());
            lines.push(format!(
                "failure: {}",
                failure.selector.as_deref().unwrap_or(&failure.name)
            ));
            lines.push(format!("kind: {}", failure.kind));

            if let Some(message) = &failure.message {
                lines.push(format!("message: {}", compact_line(message)));
            }

            if let Some(details) = &failure.details {
                lines.push(format!("details: {}", compact_line(details)));
            }

            lines.push(format!("source: {}", failure.source));
        }

        lines.join("\n")
    }
}

struct JunitSourceSummary {
    tests_ran: u64,
    tests_passed: u64,
    passed_selectors: Vec<String>,
    failures: Vec<TestFailure>,
}

fn was_modified_after(path: &Path, modified_after: SystemTime) -> bool {
    path.metadata()
        .and_then(|metadata| metadata.modified())
        .map(|modified| modified >= modified_after)
        .unwrap_or(false)
}

/// Overall status of a parsed JUnit report set.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TestStatus {
    Passed,
    Failed,
}

impl TestStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
        }
    }
}

/// A single failed or errored test case from a JUnit report.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct TestFailure {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) selector: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) suite: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) class_name: Option<String>,
    pub(crate) name: String,
    pub(crate) kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) details: Option<String>,
    pub(crate) source: String,
}

fn junit_xml_files(path: &Path) -> Result<Vec<PathBuf>, TestReportError> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }

    if !path.exists() {
        return Err(TestReportError::new(format!(
            "test report path does not exist: {}",
            path.display()
        )));
    }

    if !path.is_dir() {
        return Err(TestReportError::new(format!(
            "test report path is not a file or directory: {}",
            path.display()
        )));
    }

    let mut files = Vec::new();
    collect_xml_files(path, &mut files)?;
    files.sort();

    if files.is_empty() {
        return Err(TestReportError::new(format!(
            "no XML test reports found under {}",
            path.display()
        )));
    }

    Ok(files)
}

fn collect_xml_files(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), TestReportError> {
    let entries = fs::read_dir(path).map_err(|error| {
        TestReportError::new(format!("failed to read {}: {error}", path.display()))
    })?;

    for entry in entries {
        let entry = entry.map_err(|error| {
            TestReportError::new(format!(
                "failed to read entry under {}: {error}",
                path.display()
            ))
        })?;
        let path = entry.path();

        if path.is_dir() {
            collect_xml_files(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "xml") {
            files.push(path);
        }
    }

    Ok(())
}

fn parse_junit_source(path: &Path) -> Result<JunitSourceSummary, TestReportError> {
    let xml = fs::read_to_string(path).map_err(|error| {
        TestReportError::new(format!("failed to read {}: {error}", path.display()))
    })?;
    let document = roxmltree::Document::parse(&xml).map_err(|error| {
        TestReportError::new(format!(
            "failed to parse {} as XML: {error}",
            path.display()
        ))
    })?;

    let mut failures = Vec::new();
    let mut passed_selectors = Vec::new();
    let mut tests_ran = 0;
    let mut tests_passed = 0;

    for testcase in document
        .descendants()
        .filter(|node| node.has_tag_name("testcase"))
    {
        tests_ran += 1;
        let suite = nearest_suite_name(testcase);
        let class_name = testcase.attribute("classname").map(str::to_string);
        let name = testcase
            .attribute("name")
            .unwrap_or("<unnamed>")
            .to_string();
        let mut failed = false;

        for child in testcase
            .children()
            .filter(|node| node.has_tag_name("failure") || node.has_tag_name("error"))
        {
            failed = true;
            let kind = child.tag_name().name().to_string();
            let message = child.attribute("message").map(str::to_string);
            let details = child.text().and_then(first_non_empty_line);
            let selector = class_name
                .as_ref()
                .map(|class_name| format!("{class_name}.{name}"));

            failures.push(TestFailure {
                selector,
                suite: suite.clone(),
                class_name: class_name.clone(),
                name: name.clone(),
                kind,
                message,
                details,
                source: path.to_string_lossy().to_string(),
            });
        }

        let skipped = testcase.children().any(|node| node.has_tag_name("skipped"));
        if !failed && !skipped {
            tests_passed += 1;
            passed_selectors.push(
                class_name
                    .as_ref()
                    .map_or_else(|| name.clone(), |class_name| format!("{class_name}.{name}")),
            );
        }
    }

    Ok(JunitSourceSummary {
        tests_ran,
        tests_passed,
        passed_selectors,
        failures,
    })
}

fn nearest_suite_name(testcase: roxmltree::Node<'_, '_>) -> Option<String> {
    testcase
        .ancestors()
        .find(|node| node.has_tag_name("testsuite"))
        .and_then(|node| node.attribute("name"))
        .map(str::to_string)
}

fn compact_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn first_non_empty_line(value: &str) -> Option<String> {
    value
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_failures_from_junit_xml() {
        let path = Path::new("tests/fixtures/junit/TEST-sample.xml");
        let summary = TestFailureSummary::from_path(path).unwrap();

        assert_eq!(summary.status, TestStatus::Failed);
        assert_eq!(summary.tests_ran, 3);
        assert_eq!(summary.tests_passed, 1);
        assert_eq!(
            summary.passed_selectors,
            ["com.example.PaymentServiceTest.loadsPayment"]
        );
        assert_eq!(summary.failures.len(), 2);
        assert_eq!(
            summary.failures[0].selector.as_deref(),
            Some("com.example.PaymentServiceTest.createsPayment")
        );
        assert_eq!(summary.failures[0].kind, "failure");
        assert_eq!(
            summary.failures[1].selector.as_deref(),
            Some("com.example.PaymentServiceTest.refundsPayment")
        );
        assert_eq!(summary.failures[1].kind, "error");
    }

    #[test]
    fn reports_passed_when_no_failures_exist() {
        let path = Path::new("tests/fixtures/junit/TEST-passing.xml");
        let summary = TestFailureSummary::from_path(path).unwrap();

        assert_eq!(summary.status, TestStatus::Passed);
        assert_eq!(summary.tests_ran, 1);
        assert_eq!(summary.tests_passed, 1);
        assert_eq!(summary.passed_selectors, ["com.example.PassingTest.passes"]);
        assert!(summary.failures.is_empty());
    }

    #[test]
    fn renders_compact_text() {
        let path = Path::new("tests/fixtures/junit/TEST-sample.xml");
        let summary = TestFailureSummary::from_path(path).unwrap();

        assert!(
            summary
                .render_text()
                .contains("status: failed\ntests_ran: 3\ntests_passed: 1\nfailures: 2")
        );
        assert!(
            summary
                .render_text()
                .contains("failure: com.example.PaymentServiceTest.createsPayment")
        );
        assert!(
            summary
                .render_text()
                .contains("message: expected:<200> but was:<500>")
        );
    }
}
