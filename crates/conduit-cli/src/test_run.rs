use crate::state::{LastTestFailures, LastTestRun, write_last_test_failures, write_last_test_run};
use crate::test_report::{TestFailureSummary, TestReportError, TestStatus};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const DEFAULT_FAILURE_TAIL_LINES: usize = 40;
const NO_TESTS_MATCHED_HINT: &str = "Gradle matched no tests; check the --tests selector. Older Gradle/JUnit setups may need a fully qualified selector.";
const SPOTLESS_APPLY_HINT: &str = "run ./gradlew spotlessApply";

/// Compact result of an executed test runner command.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct TestRunSummary {
    pub(crate) runner: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) profile: Option<String>,
    pub(crate) mode: TestRunMode,
    pub(crate) termination: TestRunTermination,
    pub(crate) test_outcome: TestOutcome,
    pub(crate) command: String,
    pub(crate) exit_code: Option<i32>,
    pub(crate) duration_ms: u128,
    pub(crate) log_path: String,
    pub(crate) report_path: String,
    pub(crate) report_status: TestReportStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) diagnostics: Vec<TestRunDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) log_tail: Option<Vec<String>>,
    pub(crate) result: TestFailureSummary,
}

/// Actionable issue inferred from runner output.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct TestRunDiagnostic {
    pub(crate) kind: String,
    pub(crate) hint: String,
}

/// How the runner process ended.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TestRunTermination {
    Exit,
    Timeout,
}

impl TestRunTermination {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Exit => "exit",
            Self::Timeout => "timeout",
        }
    }
}

/// High-level outcome inferred from reports and runner output.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TestOutcome {
    Executed,
    NoSource,
    NoMatchingTests,
    RunnerFailed,
    Unknown,
}

impl TestOutcome {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Executed => "executed",
            Self::NoSource => "no_source",
            Self::NoMatchingTests => "no_matching_tests",
            Self::RunnerFailed => "runner_failed",
            Self::Unknown => "unknown",
        }
    }
}

/// Broad test intent inferred from the Gradle task and runner arguments.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TestRunMode {
    Unit,
    Integration,
}

impl TestRunMode {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Unit => "unit",
            Self::Integration => "integration",
        }
    }

    pub(crate) fn from_label(value: &str) -> Option<Self> {
        match value {
            "unit" => Some(Self::Unit),
            "integration" => Some(Self::Integration),
            _ => None,
        }
    }
}

/// Describes whether parsed JUnit-style XML came from the current Gradle invocation.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TestReportStatus {
    Fresh,
    Existing,
    Missing,
}

impl TestReportStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::Existing => "existing",
            Self::Missing => "missing",
        }
    }
}

/// Inputs needed to execute Gradle and parse its JUnit-style XML output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GradleRunRequest {
    pub(crate) profile: Option<String>,
    pub(crate) task: String,
    pub(crate) report_path: PathBuf,
    pub(crate) selectors: Vec<String>,
    pub(crate) mode: Option<TestRunMode>,
    pub(crate) tail_lines: Option<usize>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) heartbeat: Option<Duration>,
    pub(crate) env: BTreeMap<String, String>,
    pub(crate) gradle_args: Vec<String>,
}

impl TestRunSummary {
    pub(crate) fn render_text(&self) -> String {
        let mut lines = vec![
            format!("runner: {}", self.runner),
            format!("profile: {}", self.profile.as_deref().unwrap_or("none")),
            format!("mode: {}", self.mode.as_str()),
            format!("termination: {}", self.termination.as_str()),
            format!("test_outcome: {}", self.test_outcome.as_str()),
            format!("command: {}", self.command),
            format!(
                "exit_code: {}",
                match self.termination {
                    TestRunTermination::Timeout => "timeout".to_string(),
                    TestRunTermination::Exit => self
                        .exit_code
                        .map_or_else(|| "unknown".to_string(), |code| code.to_string()),
                }
            ),
            format!("duration_ms: {}", self.duration_ms),
            format!("log_path: {}", self.log_path),
            format!("report_path: {}", self.report_path),
            format!("report_status: {}", self.report_status.as_str()),
            self.result.render_text(),
        ];

        for diagnostic in &self.diagnostics {
            lines.push(format!("diagnostic: {}", diagnostic.kind));
            lines.push(format!("hint: {}", diagnostic.hint));
        }

        if let Some(tail) = &self.log_tail
            && !tail.is_empty()
        {
            lines.push(String::new());
            lines.push(format!("log_tail: {}", tail.len()));
            lines.extend(tail.iter().map(|line| format!("log: {line}")));
        }

        lines.join("\n")
    }

    pub(crate) fn exit_code(&self) -> u8 {
        if !matches!(self.termination, TestRunTermination::Exit) {
            return 1;
        }

        if matches!(self.result.status, TestStatus::Failed) {
            return 1;
        }

        match self.exit_code {
            Some(0) => 0,
            Some(code) => normalize_exit_code(code),
            None => 1,
        }
    }
}

pub(crate) fn run_gradle(request: &GradleRunRequest) -> Result<TestRunSummary, TestReportError> {
    let executable = "./gradlew";
    let mut args = vec![request.task.clone()];

    for selector in &request.selectors {
        args.push("--tests".to_string());
        args.push(selector.clone());
    }

    args.extend(request.gradle_args.iter().cloned());

    let mode = infer_gradle_mode(&request.task, request.mode.clone());

    run_gradle_command(GradleCommandRequest {
        executable,
        args: &args,
        profile: request.profile.clone(),
        mode,
        report_path: &request.report_path,
        requested_tail_lines: request.tail_lines,
        timeout: request.timeout,
        heartbeat: request.heartbeat,
        env: &request.env,
    })
}

struct GradleCommandRequest<'a> {
    executable: &'a str,
    args: &'a [String],
    profile: Option<String>,
    mode: TestRunMode,
    report_path: &'a Path,
    requested_tail_lines: Option<usize>,
    timeout: Option<Duration>,
    heartbeat: Option<Duration>,
    env: &'a BTreeMap<String, String>,
}

fn run_gradle_command(
    request: GradleCommandRequest<'_>,
) -> Result<TestRunSummary, TestReportError> {
    let started_at = SystemTime::now();
    let process_output = run_process(
        request.executable,
        request.args,
        request.env,
        request.timeout,
        request.heartbeat,
    )?;
    let ParsedTestRun {
        result,
        report_status,
        test_outcome,
    } = parse_test_result(
        request.report_path,
        started_at,
        process_output.succeeded(),
        &process_output.termination,
        &process_output.output,
    )?;
    let tail_lines = request
        .requested_tail_lines
        .unwrap_or_else(|| default_tail_lines(&process_output, &result, &report_status));
    let log_tail = if tail_lines > 0
        && (!process_output.succeeded() || matches!(result.status, TestStatus::Failed))
    {
        Some(output_tail(&process_output.output, tail_lines))
    } else {
        None
    };
    let diagnostics = diagnostics_for(&process_output.output, &test_outcome);
    let summary = TestRunSummary {
        runner: "gradle".to_string(),
        profile: request.profile,
        mode: request.mode,
        termination: process_output.termination,
        test_outcome,
        command: shell_command(request.executable, request.args),
        exit_code: process_output.exit_code,
        duration_ms: process_output.duration_ms,
        log_path: process_output.log_path.to_string_lossy().to_string(),
        report_path: request.report_path.to_string_lossy().to_string(),
        report_status,
        diagnostics,
        log_tail,
        result,
    };

    write_last_test_run(&LastTestRun::new(summary.clone()))?;
    if has_rerunnable_failures(&summary.result) {
        write_last_test_failures(&LastTestFailures::with_log_path(
            request.report_path.to_string_lossy().to_string(),
            summary.log_path.clone(),
            TestFailureSummary {
                status: summary.result.status.clone(),
                tests_ran: summary.result.tests_ran,
                tests_passed: summary.result.tests_passed,
                passed_selectors: summary.result.passed_selectors.clone(),
                failures: summary.result.failures.clone(),
                sources: summary.result.sources.clone(),
            },
        ))?;
    }

    Ok(summary)
}

struct ParsedTestRun {
    result: TestFailureSummary,
    report_status: TestReportStatus,
    test_outcome: TestOutcome,
}

fn parse_test_result(
    report_path: &Path,
    started_at: SystemTime,
    process_succeeded: bool,
    termination: &TestRunTermination,
    output: &str,
) -> Result<ParsedTestRun, TestReportError> {
    if matches!(termination, TestRunTermination::Timeout) {
        return Ok(ParsedTestRun {
            result: TestFailureSummary::empty(TestStatus::Failed),
            report_status: TestReportStatus::Missing,
            test_outcome: TestOutcome::RunnerFailed,
        });
    }

    if !report_path.exists() && !process_succeeded {
        return Ok(ParsedTestRun {
            result: TestFailureSummary::empty(TestStatus::Failed),
            report_status: TestReportStatus::Missing,
            test_outcome: failed_before_report_outcome(output),
        });
    }

    let summary = match TestFailureSummary::from_path_modified_after(report_path, started_at) {
        Ok(summary) => summary,
        Err(_) if !process_succeeded => {
            return Ok(ParsedTestRun {
                result: TestFailureSummary::empty(TestStatus::Failed),
                report_status: TestReportStatus::Missing,
                test_outcome: failed_before_report_outcome(output),
            });
        }
        Err(_) if process_succeeded => {
            return Ok(ParsedTestRun {
                result: TestFailureSummary::empty(TestStatus::Passed),
                report_status: TestReportStatus::Missing,
                test_outcome: successful_without_report_outcome(output),
            });
        }
        Err(error) => return Err(error),
    };

    if summary.sources.is_empty() && !process_succeeded {
        Ok(ParsedTestRun {
            result: TestFailureSummary::empty(TestStatus::Failed),
            report_status: TestReportStatus::Missing,
            test_outcome: failed_before_report_outcome(output),
        })
    } else if summary.sources.is_empty() && process_succeeded {
        match TestFailureSummary::from_path(report_path) {
            Ok(result) => Ok(ParsedTestRun {
                result,
                report_status: TestReportStatus::Existing,
                test_outcome: TestOutcome::Executed,
            }),
            Err(_) => Ok(ParsedTestRun {
                result: TestFailureSummary::empty(TestStatus::Passed),
                report_status: TestReportStatus::Missing,
                test_outcome: successful_without_report_outcome(output),
            }),
        }
    } else {
        Ok(ParsedTestRun {
            result: summary,
            report_status: TestReportStatus::Fresh,
            test_outcome: TestOutcome::Executed,
        })
    }
}

fn default_tail_lines(
    output: &ProcessOutput,
    result: &TestFailureSummary,
    report_status: &TestReportStatus,
) -> usize {
    if !output.succeeded()
        && matches!(result.status, TestStatus::Failed)
        && result.failures.is_empty()
        && matches!(report_status, TestReportStatus::Missing)
    {
        DEFAULT_FAILURE_TAIL_LINES
    } else {
        0
    }
}

struct ProcessOutput {
    exit_code: Option<i32>,
    termination: TestRunTermination,
    duration_ms: u128,
    log_path: PathBuf,
    output: String,
}

impl ProcessOutput {
    fn succeeded(&self) -> bool {
        matches!(self.termination, TestRunTermination::Exit) && self.exit_code == Some(0)
    }
}

struct StreamLine {
    stream: &'static str,
    line: String,
}

fn run_process(
    executable: &str,
    args: &[String],
    env: &BTreeMap<String, String>,
    timeout: Option<Duration>,
    heartbeat: Option<Duration>,
) -> Result<ProcessOutput, TestReportError> {
    let directory = state_logs_dir();
    fs::create_dir_all(&directory).map_err(|error| {
        TestReportError::new(format!(
            "failed to create log directory {}: {error}",
            directory.display()
        ))
    })?;
    let path = directory.join(format!("test-run-{}.log", timestamp_millis()));
    let mut log = fs::File::create(&path).map_err(|error| {
        TestReportError::new(format!("failed to create log {}: {error}", path.display()))
    })?;
    writeln!(log, "command: {}\n", shell_command(executable, args)).map_err(|error| {
        TestReportError::new(format!("failed to write log {}: {error}", path.display()))
    })?;

    let started_at = Instant::now();
    let mut command = Command::new(executable);
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Gradle may leave shell grandchildren running; a process group lets timeout
        // terminate the whole runner tree instead of only the wrapper process.
        command.process_group(0);
    }
    apply_env(&mut command, env);

    let mut child = command.spawn().map_err(|error| {
        TestReportError::new(format!(
            "failed to run {}: {error}",
            shell_command(executable, args)
        ))
    })?;
    let stdout = child.stdout.take().ok_or_else(|| {
        TestReportError::new(format!(
            "failed to capture stdout for {}",
            shell_command(executable, args)
        ))
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        TestReportError::new(format!(
            "failed to capture stderr for {}",
            shell_command(executable, args)
        ))
    })?;

    let (tx, rx) = mpsc::channel();
    let stdout_thread = spawn_reader("stdout", stdout, tx.clone());
    let stderr_thread = spawn_reader("stderr", stderr, tx);
    let mut output = String::new();
    let mut last_non_empty_line = None::<String>;
    let mut current_stream = None::<&'static str>;
    let mut next_heartbeat = heartbeat.map(|interval| Instant::now() + interval);
    let deadline = timeout.map(|timeout| Instant::now() + timeout);
    let mut termination = TestRunTermination::Exit;
    let exit_code;

    loop {
        drain_stream_lines(
            &rx,
            &mut log,
            &path,
            &mut current_stream,
            &mut output,
            &mut last_non_empty_line,
        )?;

        if let Some(deadline) = deadline
            && Instant::now() >= deadline
        {
            termination = TestRunTermination::Timeout;
            terminate_child(&mut child);
            let _ = child.wait();
            exit_code = None;
            break;
        }

        if let Some(status) = child.try_wait().map_err(|error| {
            TestReportError::new(format!(
                "failed to wait for {}: {error}",
                shell_command(executable, args)
            ))
        })? {
            exit_code = status.code();
            break;
        }

        if let Some(next) = next_heartbeat.as_mut()
            && Instant::now() >= *next
        {
            print_heartbeat(started_at, &path, last_non_empty_line.as_deref());
            if let Some(interval) = heartbeat {
                *next = Instant::now() + interval;
            }
        }

        thread::sleep(Duration::from_millis(25));
    }

    stdout_thread
        .join()
        .map_err(|_| TestReportError::new("failed to join stdout reader thread"))?;
    stderr_thread
        .join()
        .map_err(|_| TestReportError::new("failed to join stderr reader thread"))?;
    drain_stream_lines(
        &rx,
        &mut log,
        &path,
        &mut current_stream,
        &mut output,
        &mut last_non_empty_line,
    )?;
    log.flush().map_err(|error| {
        TestReportError::new(format!("failed to flush log {}: {error}", path.display()))
    })?;

    Ok(ProcessOutput {
        exit_code,
        termination,
        duration_ms: started_at.elapsed().as_millis(),
        log_path: path,
        output,
    })
}

fn apply_env(command: &mut Command, env: &BTreeMap<String, String>) {
    for (key, value) in env {
        if std::env::var_os(key).is_none() {
            command.env(key, value);
        }
    }
}

fn spawn_reader<R: std::io::Read + Send + 'static>(
    stream: &'static str,
    reader: R,
    tx: mpsc::Sender<StreamLine>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if tx.send(StreamLine { stream, line }).is_err() {
                        break;
                    }
                }
            }
        }
    })
}

fn drain_stream_lines(
    rx: &mpsc::Receiver<StreamLine>,
    log: &mut fs::File,
    log_path: &Path,
    current_stream: &mut Option<&'static str>,
    output: &mut String,
    last_non_empty_line: &mut Option<String>,
) -> Result<(), TestReportError> {
    let mut wrote = false;
    while let Ok(stream_line) = rx.try_recv() {
        if current_stream != &Some(stream_line.stream) {
            writeln!(log, "{}:", stream_line.stream).map_err(|error| {
                TestReportError::new(format!(
                    "failed to write log {}: {error}",
                    log_path.display()
                ))
            })?;
            *current_stream = Some(stream_line.stream);
        }
        write!(log, "{}", stream_line.line).map_err(|error| {
            TestReportError::new(format!(
                "failed to write log {}: {error}",
                log_path.display()
            ))
        })?;
        if !stream_line.line.ends_with('\n') {
            writeln!(log).map_err(|error| {
                TestReportError::new(format!(
                    "failed to write log {}: {error}",
                    log_path.display()
                ))
            })?;
        }
        wrote = true;
        if !output.is_empty() && !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str(stream_line.line.trim_end());
        output.push('\n');
        let trimmed = stream_line.line.trim();
        if !trimmed.is_empty() {
            *last_non_empty_line = Some(trimmed.to_string());
        }
    }
    if wrote {
        log.flush().map_err(|error| {
            TestReportError::new(format!(
                "failed to flush log {}: {error}",
                log_path.display()
            ))
        })?;
    }

    Ok(())
}

fn print_heartbeat(started_at: Instant, log_path: &Path, last_line: Option<&str>) {
    println!("running: {}s", started_at.elapsed().as_secs());
    println!("log_path: {}", log_path.display());
    if let Some(last_line) = last_line {
        println!("last_log: {last_line}");
    }
}

fn terminate_child(child: &mut Child) {
    #[cfg(unix)]
    {
        let group = format!("-{}", child.id());
        let _ = Command::new("kill").args(["-TERM", &group]).status();
        thread::sleep(Duration::from_millis(100));
    }

    let _ = child.kill();
}

fn state_logs_dir() -> PathBuf {
    std::env::var_os("CONDUIT_STATE_DIR")
        .map_or_else(|| PathBuf::from(".conduit/state"), PathBuf::from)
        .join("logs")
}

fn timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_millis(0))
        .as_millis()
}

fn normalize_exit_code(code: i32) -> u8 {
    u8::try_from(code).unwrap_or(1)
}

fn infer_gradle_mode(task: &str, mode: Option<TestRunMode>) -> TestRunMode {
    if let Some(mode) = mode {
        return mode;
    }

    if task.to_ascii_lowercase().contains("integration") {
        TestRunMode::Integration
    } else {
        TestRunMode::Unit
    }
}

fn failed_before_report_outcome(output: &str) -> TestOutcome {
    if output.contains("No tests found for given includes") {
        TestOutcome::NoMatchingTests
    } else {
        TestOutcome::RunnerFailed
    }
}

fn successful_without_report_outcome(output: &str) -> TestOutcome {
    if output.contains("NO-SOURCE") && output.contains("BUILD SUCCESSFUL") {
        TestOutcome::NoSource
    } else {
        TestOutcome::Unknown
    }
}

fn diagnostics_for(output: &str, test_outcome: &TestOutcome) -> Vec<TestRunDiagnostic> {
    let mut diagnostics = Vec::new();

    if matches!(test_outcome, TestOutcome::NoMatchingTests) {
        diagnostics.push(TestRunDiagnostic {
            kind: "no_tests_matched".to_string(),
            hint: NO_TESTS_MATCHED_HINT.to_string(),
        });
    }

    if has_spotless_violation(output) {
        diagnostics.push(TestRunDiagnostic {
            kind: "spotless_violation".to_string(),
            hint: SPOTLESS_APPLY_HINT.to_string(),
        });
    }

    diagnostics
}

fn has_spotless_violation(output: &str) -> bool {
    let output = output.to_lowercase();
    output.contains("spotless")
        && (output.contains("violation")
            || output.contains("spotlessapply")
            || output.contains("spotlesscheck")
            || output.contains("spotlessjavacheck"))
}

fn has_rerunnable_failures(summary: &TestFailureSummary) -> bool {
    summary
        .failures
        .iter()
        .any(|failure| failure.selector.is_some())
}

fn output_tail(output: &str, lines: usize) -> Vec<String> {
    output
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .rev()
        .take(lines)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(str::to_string)
        .collect()
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

    #[test]
    fn extracts_tail_from_output() {
        let tail = output_tail("one\ntwo\nthree\nfour\n", 3);

        assert_eq!(tail, ["two", "three", "four"]);
    }

    #[test]
    fn infers_mode_from_task_or_explicit_mode() {
        assert_eq!(infer_gradle_mode("test", None), TestRunMode::Unit);
        assert_eq!(
            infer_gradle_mode("integrationTest", None),
            TestRunMode::Integration
        );
        assert_eq!(
            infer_gradle_mode("test", Some(TestRunMode::Integration)),
            TestRunMode::Integration
        );
    }
}
