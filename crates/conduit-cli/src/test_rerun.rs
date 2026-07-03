use crate::state::LastTestFailures;
use crate::test_report::TestReportError;
use serde::Serialize;

#[derive(Debug, PartialEq, Eq, Serialize)]
pub(crate) struct RerunCommand {
    pub(crate) runner: String,
    pub(crate) executable: String,
    pub(crate) args: Vec<String>,
    pub(crate) selectors: Vec<String>,
    pub(crate) command: String,
}

impl RerunCommand {
    pub(crate) fn gradle(state: &LastTestFailures) -> Result<Self, TestReportError> {
        let selectors = state
            .summary
            .failures
            .iter()
            .filter_map(|failure| failure.selector.clone())
            .collect::<Vec<_>>();

        if selectors.is_empty() {
            return Err(TestReportError::new(
                "last test failures do not contain rerunnable selectors",
            ));
        }

        let executable = "./gradlew".to_string();
        let mut args = vec!["test".to_string()];

        for selector in &selectors {
            args.push("--tests".to_string());
            args.push(selector.clone());
        }

        let command = shell_command(&executable, &args);

        Ok(Self {
            runner: "gradle".to_string(),
            executable,
            args,
            selectors,
            command,
        })
    }

    pub(crate) fn render_text(&self) -> String {
        [
            format!("runner: {}", self.runner),
            format!("selectors: {}", self.selectors.len()),
            format!("command: {}", self.command),
        ]
        .join("\n")
    }
}

fn shell_command(executable: &str, args: &[String]) -> String {
    std::iter::once(executable.to_string())
        .chain(args.iter().map(|arg| shell_quote(arg)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '/' | '_' | '-' | ':'))
    {
        return value.to_string();
    }

    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::LastTestFailures;
    use crate::test_report::{TestFailure, TestFailureSummary, TestStatus};

    #[test]
    fn builds_gradle_rerun_command_from_selectors() {
        let state = LastTestFailures::new(
            "build/test-results/test".to_string(),
            TestFailureSummary {
                status: TestStatus::Failed,
                tests_ran: 1,
                tests_passed: 0,
                passed_selectors: Vec::new(),
                failures: vec![failure("com.example.PaymentServiceTest.createsPayment")],
                sources: vec!["TEST-sample.xml".to_string()],
            },
        );

        let command = RerunCommand::gradle(&state).unwrap();

        assert_eq!(command.runner, "gradle");
        assert_eq!(
            command.args,
            [
                "test",
                "--tests",
                "com.example.PaymentServiceTest.createsPayment"
            ]
        );
        assert_eq!(
            command.command,
            "./gradlew test --tests com.example.PaymentServiceTest.createsPayment"
        );
    }

    #[test]
    fn quotes_shell_arguments_when_needed() {
        assert_eq!(
            shell_quote("com.example.Test.some test"),
            "'com.example.Test.some test'"
        );
    }

    fn failure(selector: &str) -> TestFailure {
        TestFailure {
            selector: Some(selector.to_string()),
            suite: None,
            class_name: None,
            name: "createsPayment".to_string(),
            kind: "failure".to_string(),
            message: None,
            details: None,
            source: "TEST-sample.xml".to_string(),
        }
    }
}
