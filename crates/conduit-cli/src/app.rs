//! CLI argument parsing and command execution.
//!
//! This module is the public boundary used by the binary. It keeps command
//! parsing separate from `main` so integration tests can exercise the same code
//! path as the installed executable.

use crate::config::{ConduitConfig, ConfiguredGradleTestProfile};
use crate::git_status::GitStatusSummary;
use crate::logs::{
    DEFAULT_LOG_LIMIT, DEFAULT_LOG_WATCH_LIMIT, LogAuthRequest, LogAuthStatus, LogDiagnostic,
    LogError, LogErrorKind, LogQuery, LogStreamFormat, LogTimeRangeInput, LogWaitRequest,
    LogWaitStatus, LogWatchEvent, LogWatchRequest, wait_for_logs, watch_logs,
};
use crate::logs_provider::configured_log_provider;
use crate::openapi::OpenApiRequest;
use crate::openapi_provider::configured_openapi_provider;
use crate::output::{OutputFormat, fields, json_object};
use crate::plugin_check::{PluginCheckProvider, check_configured_plugin, check_plugin};
use crate::state::{
    LastTestFailures, read_last_test_failures, read_last_test_run, write_last_test_failures,
};
use crate::stats::{StatsUpdate, StatsView, read_stats, record_stats};
use crate::test_log::TestLogSummary;
use crate::test_report::TestFailureSummary;
use crate::test_rerun::RerunCommand;
use crate::test_run::{GradleRunRequest, TestRunMode, TestRunSummary, run_gradle};
use crate::worktree::WorktreeListSummary;
use clap::{Args, Parser, Subcommand, ValueEnum, error::ErrorKind};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const NAME: &str = "conduit";
const PURPOSE: &str = "agent-first developer operations CLI";

/// Error returned by the CLI runner before process-level rendering.
#[derive(Debug, PartialEq, Eq)]
pub struct CliError {
    /// Suggested process exit code.
    pub code: u8,
    /// Compact error message written by `main`.
    pub message: String,
    /// Optional compact hint written by `main`.
    pub hint: Option<String>,
}

impl CliError {
    fn usage(message: impl Into<String>) -> Self {
        Self {
            code: 2,
            message: message.into(),
            hint: Some("run `conduit help` for available commands".to_string()),
        }
    }

    fn data(message: impl Into<String>) -> Self {
        Self {
            code: 1,
            message: message.into(),
            hint: None,
        }
    }
}

/// Parses and executes a Conduit CLI invocation.
///
/// The iterator should contain command arguments without the executable name,
/// matching `std::env::args().skip(1)`. Command output is printed to stdout;
/// errors are returned for `main` to render on stderr.
///
/// # Errors
///
/// Returns [`CliError`] when arguments are invalid or a command cannot complete.
pub fn run(args: impl IntoIterator<Item = String>) -> Result<u8, CliError> {
    let command = parse(args)?;
    let mut outcome = command.execute()?;
    let stats_update = if command.records_stats() {
        Some(
            outcome
                .stats_update
                .take()
                .unwrap_or_else(StatsUpdate::command),
        )
    } else {
        None
    };
    if !outcome.output.is_empty() {
        println!("{}", outcome.output);
    }
    if let Some(update) = stats_update {
        let _ = record_stats(&update);
    }
    Ok(outcome.code)
}

#[derive(Debug, Parser)]
#[command(name = "conduit")]
#[command(about = PURPOSE)]
#[command(disable_help_subcommand = true)]
struct Cli {
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Option<TopLevelCommand>,
}

#[derive(Debug, Subcommand)]
enum TopLevelCommand {
    About,
    Git(GitCommand),
    Help,
    Logs(Box<LogsCommand>),
    Openapi(OpenApiCommand),
    Plugin(PluginCommand),
    Stats,
    Test(TestCommand),
    Worktree(WorktreeCommand),
}

#[derive(Debug, Args)]
struct LogsCommand {
    #[command(subcommand)]
    command: LogsSubcommand,
}

#[derive(Debug, Subcommand)]
enum LogsSubcommand {
    Auth(LogsAuthArgs),
    Errors(LogsErrorsArgs),
    Search(LogsSearchArgs),
    Wait(LogsWaitArgs),
    Watch(LogsWatchArgs),
}

#[derive(Debug, Args)]
struct LogsAuthArgs {
    #[arg(long = "env")]
    environment: Option<String>,

    #[arg(long = "secret-stdin")]
    secret_stdin: bool,

    #[arg(long)]
    check: bool,
}

#[derive(Debug, Args)]
struct LogsSearchArgs {
    service: String,

    #[command(flatten)]
    filters: LogsFilterArgs,

    #[arg(long = "level")]
    levels: Vec<String>,
}

#[derive(Debug, Args)]
struct LogsErrorsArgs {
    service: String,

    #[command(flatten)]
    filters: LogsFilterArgs,
}

#[derive(Debug, Args)]
struct LogsWatchArgs {
    service: String,

    #[command(flatten)]
    filters: LogsFilterArgs,

    #[arg(long = "level")]
    levels: Vec<String>,

    #[arg(long, value_parser = parse_duration_arg, default_value = "5s")]
    interval: Duration,

    #[arg(long, value_parser = parse_duration_arg)]
    timeout: Option<Duration>,

    #[arg(long)]
    jsonl: bool,
}

#[derive(Debug, Args)]
struct LogsWaitArgs {
    service: String,

    #[command(flatten)]
    filters: LogsFilterArgs,

    #[arg(long = "level")]
    levels: Vec<String>,

    #[arg(long, value_parser = parse_duration_arg, default_value = "2m")]
    timeout: Duration,

    #[arg(long, value_parser = parse_duration_arg, default_value = "2s")]
    interval: Duration,
}

#[derive(Clone, Debug, Args, Default, PartialEq, Eq)]
struct LogsFilterArgs {
    #[arg(long = "env")]
    environment: Option<String>,

    #[arg(long)]
    since: Option<String>,

    #[arg(long = "from")]
    from: Option<String>,

    #[arg(long = "to")]
    to: Option<String>,

    #[arg(long)]
    date: Option<String>,

    #[arg(long, default_value_t = DEFAULT_LOG_LIMIT)]
    limit: usize,

    #[arg(long)]
    cid: Option<String>,

    #[arg(long = "correlation-id")]
    correlation_id: Option<String>,

    #[arg(long = "trace-id")]
    trace_id: Option<String>,

    #[arg(long)]
    message: Option<String>,

    #[arg(long)]
    logger: Option<String>,

    #[arg(long = "class")]
    class_name: Option<String>,

    #[arg(long = "exclude-message")]
    exclude_messages: Vec<String>,

    #[arg(long = "exclude-logger")]
    exclude_loggers: Vec<String>,

    #[arg(long = "exclude-class")]
    exclude_class_names: Vec<String>,

    #[arg(long = "include-trace")]
    include_trace: bool,
}

#[derive(Debug, Args)]
struct OpenApiCommand {
    #[command(subcommand)]
    command: OpenApiSubcommand,
}

#[derive(Debug, Subcommand)]
enum OpenApiSubcommand {
    List(OpenApiListArgs),
    Operation(OpenApiOperationArgs),
    Search(OpenApiSearchArgs),
}

#[derive(Debug, Args)]
struct OpenApiListArgs {
    #[arg(long)]
    service: String,

    #[arg(long)]
    environment: Option<String>,
}

#[derive(Debug, Args)]
struct OpenApiOperationArgs {
    #[arg(long)]
    service: String,

    #[arg(long)]
    method: String,

    #[arg(long)]
    path: String,

    #[arg(long)]
    environment: Option<String>,
}

#[derive(Debug, Args)]
struct OpenApiSearchArgs {
    #[arg(long)]
    service: String,

    #[arg(long)]
    query: String,

    #[arg(long)]
    method: Option<String>,

    #[arg(long)]
    environment: Option<String>,
}

#[derive(Debug, Args)]
struct PluginCommand {
    #[command(subcommand)]
    command: PluginSubcommand,
}

#[derive(Debug, Subcommand)]
enum PluginSubcommand {
    Check(PluginCheckArgs),
}

#[derive(Debug, Args)]
struct PluginCheckArgs {
    #[arg(long)]
    path: Option<PathBuf>,

    #[arg(long)]
    provider: Option<String>,
}

#[derive(Debug, Args)]
struct WorktreeCommand {
    #[command(subcommand)]
    command: WorktreeSubcommand,
}

#[derive(Debug, Subcommand)]
enum WorktreeSubcommand {
    List(WorktreeListArgs),
}

#[derive(Debug, Args)]
struct WorktreeListArgs {
    #[arg(long, default_value = ".")]
    root: PathBuf,
}

#[derive(Debug, Args)]
struct GitCommand {
    #[command(subcommand)]
    command: GitSubcommand,
}

#[derive(Debug, Subcommand)]
enum GitSubcommand {
    Status(GitStatusArgs),
}

#[derive(Debug, Args)]
struct GitStatusArgs {
    #[arg(long, default_value = ".")]
    path: PathBuf,
}

#[derive(Debug, Args)]
struct TestCommand {
    #[command(subcommand)]
    command: TestSubcommand,
}

#[derive(Debug, Subcommand)]
enum TestSubcommand {
    Failed(TestFailedArgs),
    Failures(TestFailuresArgs),
    Last,
    Log(TestLogArgs),
    Rerun(TestRerunArgs),
    Run(TestRunCommand),
}

#[derive(Debug, Args)]
struct TestFailedArgs {
    #[arg(long)]
    tail: Option<usize>,
}

#[derive(Debug, Args)]
struct TestFailuresArgs {
    #[arg(default_value = "build/test-results/test")]
    path: PathBuf,
}

#[derive(Debug, Args)]
struct TestRerunArgs {
    runner: String,
}

#[derive(Debug, Args)]
struct TestLogArgs {
    #[arg(long)]
    path: Option<PathBuf>,

    #[arg(long, default_value_t = 40)]
    tail: usize,
}

#[derive(Debug, Args)]
struct TestRunCommand {
    #[command(subcommand)]
    command: TestRunSubcommand,
}

#[derive(Debug, Subcommand)]
enum TestRunSubcommand {
    Gradle(TestRunGradleArgs),
}

#[derive(Debug, Args)]
struct TestRunGradleArgs {
    #[arg(long)]
    profile: Option<String>,

    #[arg(long)]
    task: Option<String>,

    #[arg(long)]
    report_path: Option<PathBuf>,

    #[arg(long = "tests")]
    selectors: Vec<String>,

    #[arg(long)]
    failed: bool,

    #[arg(long)]
    mode: Option<TestRunModeArg>,

    #[arg(long)]
    tail: Option<usize>,

    #[arg(long, value_parser = parse_duration_arg)]
    timeout: Option<Duration>,

    #[arg(long, value_parser = parse_duration_arg)]
    heartbeat: Option<Duration>,

    #[arg(last = true, allow_hyphen_values = true)]
    gradle_args: Vec<String>,
}

#[derive(Clone, Debug, ValueEnum)]
enum TestRunModeArg {
    Unit,
    Integration,
}

impl From<TestRunModeArg> for TestRunMode {
    fn from(value: TestRunModeArg) -> Self {
        match value {
            TestRunModeArg::Unit => Self::Unit,
            TestRunModeArg::Integration => Self::Integration,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Command {
    About {
        format: OutputFormat,
    },
    Display {
        output: String,
    },
    Help {
        format: OutputFormat,
    },
    LogsSearch {
        format: OutputFormat,
        service: String,
        filters: LogsFilterArgs,
        levels: Vec<String>,
    },
    LogsAuth {
        format: OutputFormat,
        environment: Option<String>,
        secret_stdin: bool,
        check: bool,
    },
    LogsErrors {
        format: OutputFormat,
        service: String,
        filters: LogsFilterArgs,
    },
    LogsWatch {
        format: LogStreamFormat,
        service: String,
        filters: LogsFilterArgs,
        levels: Vec<String>,
        interval: Duration,
        timeout: Option<Duration>,
    },
    LogsWait {
        format: OutputFormat,
        service: String,
        filters: LogsFilterArgs,
        levels: Vec<String>,
        interval: Duration,
        timeout: Duration,
    },
    GitStatus {
        format: OutputFormat,
        path: PathBuf,
    },
    OpenApiList {
        format: OutputFormat,
        service: String,
        environment: Option<String>,
    },
    OpenApiOperation {
        format: OutputFormat,
        service: String,
        environment: Option<String>,
        method: String,
        path: String,
    },
    OpenApiSearch {
        format: OutputFormat,
        service: String,
        environment: Option<String>,
        query: String,
        method: Option<String>,
    },
    PluginCheck {
        format: OutputFormat,
        target: PluginCheckTarget,
    },
    TestFailed {
        format: OutputFormat,
        tail_lines: Option<usize>,
    },
    TestFailures {
        format: OutputFormat,
        path: PathBuf,
    },
    TestLast {
        format: OutputFormat,
    },
    TestLog {
        format: OutputFormat,
        path: Option<PathBuf>,
        tail_lines: usize,
    },
    TestRerun {
        format: OutputFormat,
        runner: String,
    },
    Stats {
        format: OutputFormat,
    },
    TestRunGradle {
        format: OutputFormat,
        selectors: Vec<String>,
        failed: bool,
        profile: Option<String>,
        task: Option<String>,
        report_path: Option<PathBuf>,
        mode: Option<TestRunMode>,
        tail_lines: Option<usize>,
        timeout: Option<Duration>,
        heartbeat: Option<Duration>,
        env: BTreeMap<String, String>,
        gradle_args: Vec<String>,
    },
    WorktreeList {
        format: OutputFormat,
        root: PathBuf,
    },
}

impl Command {
    fn execute(&self) -> Result<CommandOutcome, CliError> {
        match self {
            Command::About { format } => Ok(CommandOutcome::success(match format {
                OutputFormat::Text => fields([
                    ("name", NAME),
                    ("version", env!("CARGO_PKG_VERSION")),
                    ("purpose", PURPOSE),
                ]),
                OutputFormat::Json => json_object([
                    ("name", NAME),
                    ("version", env!("CARGO_PKG_VERSION")),
                    ("purpose", PURPOSE),
                ]),
            })),
            Command::Display { output } => Ok(CommandOutcome::success(output.clone())),
            Command::Help { format } => Ok(CommandOutcome::success(match format {
                OutputFormat::Text => fields([
                    ("usage", "conduit <command> [args] [--json]"),
                    (
                        "commands",
                        "about, git status, help, logs auth, logs errors, logs search, logs wait, logs watch, openapi list, openapi operation, openapi search, plugin check, stats, test failed, test failures, test last, test log, test rerun, test run gradle, worktree list",
                    ),
                    ("default_output", "compact deterministic text"),
                ]),
                OutputFormat::Json => json_object([
                    ("usage", "conduit <command> [args] [--json]"),
                    (
                        "commands",
                        "about,git status,help,logs auth,logs errors,logs search,logs wait,logs watch,openapi list,openapi operation,openapi search,plugin check,stats,test failed,test failures,test last,test log,test rerun,test run gradle,worktree list",
                    ),
                    ("default_output", "compact deterministic text"),
                ]),
            })),
            Command::LogsSearch {
                format,
                service,
                filters,
                levels,
            } => self.execute_logs_search(*format, service, filters, levels.clone()),
            Command::LogsAuth {
                format,
                environment,
                secret_stdin,
                check,
            } => self.execute_logs_auth(*format, environment.clone(), *secret_stdin, *check),
            Command::LogsErrors {
                format,
                service,
                filters,
            } => {
                let mut filters = filters.clone();
                filters.include_trace = true;
                self.execute_logs_search(*format, service, &filters, vec!["ERROR".to_string()])
            }
            Command::LogsWatch {
                format,
                service,
                filters,
                levels,
                interval,
                timeout,
            } => self.execute_logs_watch(
                *format,
                service,
                filters,
                levels.clone(),
                *interval,
                *timeout,
            ),
            Command::LogsWait {
                format,
                service,
                filters,
                levels,
                interval,
                timeout,
            } => self.execute_logs_wait(
                *format,
                service,
                filters,
                levels.clone(),
                *interval,
                *timeout,
            ),
            Command::GitStatus { format, path } => {
                let status =
                    GitStatusSummary::load(path).map_err(|error| CliError::data(error.message))?;

                if format.is_json() {
                    Ok(CommandOutcome::success(json(&status)?))
                } else {
                    Ok(CommandOutcome::success(status.render_text()))
                }
            }
            Command::OpenApiList {
                format,
                service,
                environment,
            } => {
                let request = OpenApiRequest {
                    service: service.clone(),
                    environment: environment.clone(),
                    method: None,
                    path: None,
                };
                let provider =
                    configured_openapi_provider().map_err(|error| CliError::data(error.message))?;
                let list = provider
                    .list(&request)
                    .map_err(|error| CliError::data(error.message))?;

                if format.is_json() {
                    Ok(CommandOutcome::success(json(&list)?))
                } else {
                    Ok(CommandOutcome::success(list.render_text()))
                }
            }
            Command::OpenApiOperation {
                format,
                service,
                environment,
                method,
                path,
            } => {
                let request = OpenApiRequest {
                    service: service.clone(),
                    environment: environment.clone(),
                    method: Some(method.clone()),
                    path: Some(path.clone()),
                };
                let provider =
                    configured_openapi_provider().map_err(|error| CliError::data(error.message))?;
                let operation = provider
                    .operation(&request)
                    .map_err(|error| CliError::data(error.message))?;

                if format.is_json() {
                    Ok(CommandOutcome::success(json(&operation)?))
                } else {
                    Ok(CommandOutcome::success(operation.render_text()))
                }
            }
            Command::OpenApiSearch {
                format,
                service,
                environment,
                query,
                method,
            } => {
                let provider =
                    configured_openapi_provider().map_err(|error| CliError::data(error.message))?;
                let request = OpenApiRequest {
                    service: service.clone(),
                    environment: environment.clone(),
                    method: None,
                    path: None,
                };
                let mut operations = provider
                    .list(&request)
                    .map_err(|error| CliError::data(error.message))?;
                filter_openapi_operations(&mut operations.operations, query, method.as_deref());

                if format.is_json() {
                    Ok(CommandOutcome::success(json(&operations)?))
                } else {
                    Ok(CommandOutcome::success(operations.render_text()))
                }
            }
            Command::PluginCheck { format, target } => {
                let summary = match target {
                    PluginCheckTarget::Path { path, provider } => check_plugin(path, *provider),
                    PluginCheckTarget::ConfiguredProvider(provider) => {
                        check_configured_plugin(*provider)
                    }
                }
                .map_err(|error| CliError::data(error.message))?;

                if format.is_json() {
                    Ok(CommandOutcome::success(json(&summary)?))
                } else {
                    Ok(CommandOutcome::success(summary.render_text()))
                }
            }
            Command::Stats { format } => {
                let stats = read_stats().map_err(|error| CliError::data(error.message))?;
                let view = StatsView::from_stats(&stats);

                if format.is_json() {
                    Ok(CommandOutcome::success(json(&view)?))
                } else {
                    Ok(CommandOutcome::success(view.render_text()))
                }
            }
            Command::TestFailures { format, path } => {
                let summary = TestFailureSummary::from_path(path)
                    .map_err(|error| CliError::data(error.message))?;
                let state = LastTestFailures::new(path.to_string_lossy().to_string(), summary);
                write_last_test_failures(&state).map_err(|error| CliError::data(error.message))?;

                if format.is_json() {
                    Ok(CommandOutcome::success(json(&state.summary)?))
                } else {
                    Ok(CommandOutcome::success(state.summary.render_text()))
                }
            }
            Command::TestFailed { format, tail_lines } => {
                let state =
                    read_last_test_failures().map_err(|error| CliError::data(error.message))?;

                if format.is_json() {
                    Ok(CommandOutcome::success(json(&state)?))
                } else {
                    Ok(CommandOutcome::success(
                        render_last_test_failures(&state, *tail_lines)
                            .map_err(|error| CliError::data(error.message))?,
                    ))
                }
            }
            Command::TestLast { format } => {
                let state = read_last_test_run().map_err(|error| CliError::data(error.message))?;

                if format.is_json() {
                    Ok(CommandOutcome::success(json(&state)?))
                } else {
                    Ok(CommandOutcome::success(state.summary.render_text()))
                }
            }
            Command::TestLog {
                format,
                path,
                tail_lines,
            } => {
                let summary = if let Some(path) = path {
                    TestLogSummary::from_path(path, *tail_lines)
                } else {
                    TestLogSummary::latest(*tail_lines)
                }
                .map_err(|error| CliError::data(error.message))?;

                if format.is_json() {
                    Ok(CommandOutcome::success(json(&summary)?))
                } else {
                    Ok(CommandOutcome::success(summary.render_text()))
                }
            }
            Command::TestRerun { format, runner } => {
                let state =
                    read_last_test_failures().map_err(|error| CliError::data(error.message))?;
                let command = match runner.as_str() {
                    "gradle" => RerunCommand::gradle(&state),
                    value => Err(crate::test_report::TestReportError::new(format!(
                        "unsupported test runner `{value}`"
                    ))),
                }
                .map_err(|error| CliError::data(error.message))?;

                if format.is_json() {
                    Ok(CommandOutcome::success(json(&command)?))
                } else {
                    Ok(CommandOutcome::success(command.render_text()))
                }
            }
            Command::TestRunGradle { .. } => self.execute_test_run_gradle(),
            Command::WorktreeList { format, root } => {
                let summary = WorktreeListSummary::load(root)
                    .map_err(|error| CliError::data(error.message))?;

                if format.is_json() {
                    Ok(CommandOutcome::success(json(&summary)?))
                } else {
                    Ok(CommandOutcome::success(summary.render_text()))
                }
            }
        }
    }

    fn records_stats(&self) -> bool {
        !matches!(self, Self::Stats { .. })
    }

    fn execute_logs_search(
        &self,
        format: OutputFormat,
        service: &str,
        filters: &LogsFilterArgs,
        levels: Vec<String>,
    ) -> Result<CommandOutcome, CliError> {
        let configured =
            configured_log_provider().map_err(|error| CliError::data(error.message))?;
        let query = log_query(
            service,
            filters,
            levels,
            configured.default_environment.as_deref(),
            &configured.default_since,
        )?;
        let mut result = configured
            .provider
            .search(&query)
            .map_err(log_error_to_cli_error)?;
        apply_count_only_output(&mut result, query.limit);

        if format.is_json() {
            Ok(CommandOutcome::success(json(&result)?))
        } else {
            Ok(CommandOutcome::success(result.render_text()))
        }
    }

    fn execute_logs_auth(
        &self,
        format: OutputFormat,
        environment: Option<String>,
        secret_stdin: bool,
        check: bool,
    ) -> Result<CommandOutcome, CliError> {
        if check && secret_stdin {
            return Err(CliError::usage(
                "`logs auth --check` cannot be combined with `--secret-stdin`",
            ));
        }

        let configured =
            configured_log_provider().map_err(|error| CliError::data(error.message))?;
        let secret = if secret_stdin {
            Some(read_secret_stdin()?)
        } else {
            None
        };
        let request = LogAuthRequest {
            environment: environment.or(configured.default_environment),
            secret,
            check,
        };
        let result = configured
            .provider
            .authenticate(&request)
            .map_err(log_error_to_cli_error)?;
        let code = if result.status == LogAuthStatus::ActionRequired {
            1
        } else {
            0
        };
        let output = if format.is_json() {
            json(&result)?
        } else {
            result.render_text()
        };

        Ok(CommandOutcome {
            output,
            code,
            stats_update: None,
        })
    }

    fn execute_logs_watch(
        &self,
        format: LogStreamFormat,
        service: &str,
        filters: &LogsFilterArgs,
        levels: Vec<String>,
        interval: Duration,
        timeout: Option<Duration>,
    ) -> Result<CommandOutcome, CliError> {
        let configured =
            configured_log_provider().map_err(|error| CliError::data(error.message))?;
        let mut filters = filters.clone();
        if filters.limit == 0 {
            return Err(CliError::usage(
                "logs watch --limit must be greater than zero",
            ));
        }
        if filters.limit == DEFAULT_LOG_LIMIT {
            filters.limit = DEFAULT_LOG_WATCH_LIMIT;
        }
        let query = log_query(
            service,
            &filters,
            levels,
            configured.default_environment.as_deref(),
            "now",
        )?;
        let request = LogWatchRequest {
            query,
            interval,
            timeout,
            poll_to_now: logs_should_poll_to_now(&filters),
        };
        let mut stdout = std::io::stdout().lock();

        watch_logs(configured.provider.as_ref(), request, |event| {
            write_watch_event(&mut stdout, format, &event)
        })
        .map_err(log_error_to_cli_error)?;

        Ok(CommandOutcome::success(String::new()))
    }

    fn execute_logs_wait(
        &self,
        format: OutputFormat,
        service: &str,
        filters: &LogsFilterArgs,
        levels: Vec<String>,
        interval: Duration,
        timeout: Duration,
    ) -> Result<CommandOutcome, CliError> {
        let configured =
            configured_log_provider().map_err(|error| CliError::data(error.message))?;
        if filters.limit == 0 {
            return Err(CliError::usage(
                "logs wait --limit must be greater than zero",
            ));
        }
        let query = log_query(
            service,
            filters,
            levels,
            configured.default_environment.as_deref(),
            "now",
        )?;
        let result = wait_for_logs(
            configured.provider.as_ref(),
            LogWaitRequest {
                query,
                interval,
                timeout,
                poll_to_now: logs_should_poll_to_now(filters),
            },
        )
        .map_err(log_error_to_cli_error)?;
        let code = if result.status == LogWaitStatus::Timeout {
            1
        } else {
            0
        };
        let output = if format.is_json() {
            json(&result)?
        } else {
            result.render_text()
        };

        Ok(CommandOutcome {
            output,
            code,
            stats_update: None,
        })
    }

    fn execute_test_run_gradle(&self) -> Result<CommandOutcome, CliError> {
        let Command::TestRunGradle {
            format,
            selectors,
            failed,
            profile,
            task,
            report_path,
            mode,
            tail_lines,
            timeout,
            heartbeat,
            env,
            gradle_args,
        } = self
        else {
            unreachable!("execute_test_run_gradle called for non-Gradle command");
        };

        let profile = load_gradle_profile(profile.as_deref())?;
        let task = task
            .clone()
            .or_else(|| profile.as_ref().and_then(|profile| profile.task.clone()))
            .unwrap_or_else(|| "test".to_string());
        let report_path = report_path
            .clone()
            .or_else(|| {
                profile
                    .as_ref()
                    .and_then(|profile| profile.report_path.clone())
            })
            .unwrap_or_else(|| infer_gradle_report_path(&task));
        let mode = resolve_gradle_mode(mode.clone(), profile.as_ref())?;
        let gradle_args = profile
            .as_ref()
            .map(|profile| profile.args.clone())
            .unwrap_or_default()
            .into_iter()
            .chain(gradle_args.clone())
            .collect::<Vec<_>>();
        let env = profile
            .as_ref()
            .map(|profile| profile.env.clone())
            .unwrap_or_default()
            .into_iter()
            .chain(env.clone())
            .collect::<BTreeMap<_, _>>();
        let selectors = if *failed {
            failed_selectors()?
        } else {
            selectors.clone()
        };
        let request = GradleRunRequest {
            profile: profile.as_ref().map(|profile| profile.name.clone()),
            task,
            report_path,
            selectors,
            mode,
            tail_lines: *tail_lines,
            timeout: *timeout,
            heartbeat: if format.is_json() { None } else { *heartbeat },
            env,
            gradle_args,
        };
        let summary = run_gradle(&request).map_err(|error| CliError::data(error.message))?;

        render_test_run_summary(*format, &summary)
    }
}

struct CommandOutcome {
    output: String,
    code: u8,
    stats_update: Option<StatsUpdate>,
}

impl CommandOutcome {
    fn success(output: String) -> Self {
        Self {
            output,
            code: 0,
            stats_update: None,
        }
    }
}

fn render_test_run_summary(
    format: OutputFormat,
    summary: &TestRunSummary,
) -> Result<CommandOutcome, CliError> {
    let code = summary.exit_code();
    let output = if format.is_json() {
        json(summary)?
    } else {
        summary.render_text()
    };
    let raw_log = fs::read_to_string(&summary.log_path).ok();
    let mut stats_update = StatsUpdate::command();
    stats_update.add_test_run(raw_log.as_deref(), &output);

    Ok(CommandOutcome {
        output,
        code,
        stats_update: Some(stats_update),
    })
}

fn json(value: &impl serde::Serialize) -> Result<String, CliError> {
    serde_json::to_string(value)
        .map_err(|error| CliError::data(format!("failed to render JSON: {error}")))
}

fn read_secret_stdin() -> Result<String, CliError> {
    let mut value = String::new();
    std::io::stdin()
        .read_to_string(&mut value)
        .map_err(|error| CliError::data(format!("failed to read secret from stdin: {error}")))?;
    if value.is_empty() {
        return Err(CliError::usage(
            "`--secret-stdin` requires a non-empty value on stdin",
        ));
    }

    Ok(value)
}

fn write_watch_event(
    writer: &mut impl Write,
    format: LogStreamFormat,
    event: &LogWatchEvent,
) -> Result<(), LogError> {
    match format {
        LogStreamFormat::Text => {
            writeln!(writer, "{}", event.render_text()).map_err(watch_write_error)?;
            writeln!(writer).map_err(watch_write_error)?;
        }
        LogStreamFormat::Jsonl => {
            writeln!(writer, "{}", event.render_json_line()?).map_err(watch_write_error)?;
        }
    }
    writer.flush().map_err(watch_write_error)
}

fn watch_write_error(error: std::io::Error) -> LogError {
    LogError::new(
        LogErrorKind::Internal,
        format!("failed to write logs watch output: {error}"),
    )
}

fn apply_count_only_output(result: &mut crate::logs::LogSearchResult, limit: usize) {
    if limit != 0 {
        return;
    }

    result.logs.clear();
    result.shown = 0;
    result
        .diagnostics
        .retain(|diagnostic| diagnostic.kind != "query_truncated");
    result.diagnostics.push(LogDiagnostic {
        kind: "count_only".to_string(),
        hint: Some("increase --limit to show matching logs".to_string()),
    });
}

fn log_query(
    service: &str,
    filters: &LogsFilterArgs,
    levels: Vec<String>,
    default_environment: Option<&str>,
    default_since: &str,
) -> Result<LogQuery, CliError> {
    let cid = one_optional_filter(
        "--cid",
        filters.cid.as_ref(),
        "--correlation-id",
        filters.correlation_id.as_ref(),
    )?
    .cloned();
    let logger = one_optional_filter(
        "--logger",
        filters.logger.as_ref(),
        "--class",
        filters.class_name.as_ref(),
    )?
    .cloned();
    let mut exclude_loggers = filters.exclude_loggers.clone();
    exclude_loggers.extend(filters.exclude_class_names.clone());
    let time_range = LogTimeRangeInput {
        since: filters.since.clone(),
        from: filters.from.clone(),
        to: filters.to.clone(),
        date: filters.date.clone(),
    }
    .resolve(SystemTime::now(), Some(default_since))
    .map_err(log_error_to_cli_error)?;

    Ok(LogQuery {
        service: service.to_string(),
        environment: filters
            .environment
            .clone()
            .or_else(|| default_environment.map(str::to_string)),
        time_range,
        limit: filters.limit,
        levels,
        cid,
        trace_id: filters.trace_id.clone(),
        message: filters.message.clone(),
        logger,
        exclude_messages: filters.exclude_messages.clone(),
        exclude_loggers,
        include_trace: filters.include_trace,
        cursor: None,
    })
}

fn logs_should_poll_to_now(filters: &LogsFilterArgs) -> bool {
    filters.date.is_none() && filters.to.is_none()
}

fn one_optional_filter<'a>(
    left_name: &str,
    left: Option<&'a String>,
    right_name: &str,
    right: Option<&'a String>,
) -> Result<Option<&'a String>, CliError> {
    match (left, right) {
        (Some(left), Some(right)) if left != right => Err(CliError::usage(format!(
            "{left_name} and {right_name} cannot have different values"
        ))),
        (Some(value), _) | (_, Some(value)) => Ok(Some(value)),
        (None, None) => Ok(None),
    }
}

fn log_error_to_cli_error(error: crate::logs::LogError) -> CliError {
    match error.kind {
        LogErrorKind::InvalidRequest => CliError::usage(error.message),
        _ => CliError::data(error.message),
    }
}

fn parse_duration_arg(value: &str) -> Result<Duration, String> {
    let (number, unit) = value.split_at(
        value
            .find(|ch: char| !ch.is_ascii_digit())
            .unwrap_or(value.len()),
    );
    if number.is_empty() || unit.is_empty() {
        return Err("expected duration like 30s, 2m, or 1h".to_string());
    }
    let amount = number
        .parse::<u64>()
        .map_err(|_| "duration amount must be a positive integer".to_string())?;
    if amount == 0 {
        return Err("duration amount must be greater than zero".to_string());
    }

    match unit {
        "ms" => Ok(Duration::from_millis(amount)),
        "s" => Ok(Duration::from_secs(amount)),
        "m" => Ok(Duration::from_secs(amount * 60)),
        "h" => Ok(Duration::from_secs(amount * 60 * 60)),
        _ => Err("duration unit must be ms, s, m, or h".to_string()),
    }
}

fn failed_selectors() -> Result<Vec<String>, CliError> {
    let state = read_last_test_failures().map_err(|error| CliError::data(error.message))?;
    let selectors = state
        .summary
        .failures
        .iter()
        .filter_map(|failure| failure.selector.clone())
        .collect::<Vec<_>>();

    if selectors.is_empty() {
        Err(CliError::data(
            "last test failures do not contain rerunnable selectors",
        ))
    } else {
        Ok(selectors)
    }
}

fn load_gradle_profile(
    name: Option<&str>,
) -> Result<Option<ConfiguredGradleTestProfile>, CliError> {
    let Some(name) = name else {
        return Ok(None);
    };
    let config =
        ConduitConfig::load_current_dir().map_err(|error| CliError::data(error.message))?;
    let Some(config) = config else {
        return Err(CliError::data(format!(
            "gradle test profile `{name}` is not configured in .conduit/conduit.toml in the current directory or its ancestors"
        )));
    };

    config
        .gradle_test_profile(name)
        .map_err(|error| CliError::data(error.message))?
        .ok_or_else(|| {
            CliError::data(format!(
                "gradle test profile `{name}` is not configured in .conduit/conduit.toml in the current directory or its ancestors"
            ))
        })
        .map(Some)
}

fn resolve_gradle_mode(
    mode: Option<TestRunMode>,
    profile: Option<&ConfiguredGradleTestProfile>,
) -> Result<Option<TestRunMode>, CliError> {
    if mode.is_some() {
        return Ok(mode);
    }
    let Some(profile_mode) = profile.and_then(|profile| profile.mode.as_deref()) else {
        return Ok(None);
    };

    TestRunMode::from_label(profile_mode)
        .ok_or_else(|| {
            CliError::data(format!(
                "gradle test profile mode `{profile_mode}` is invalid; expected `unit` or `integration`"
            ))
        })
        .map(Some)
}

fn parse(args: impl IntoIterator<Item = String>) -> Result<Command, CliError> {
    let args = args.into_iter().collect::<Vec<_>>();
    if let Some(command) = first_positional(&args)
        && !matches!(
            command,
            "about"
                | "git"
                | "help"
                | "logs"
                | "openapi"
                | "plugin"
                | "stats"
                | "test"
                | "worktree"
        )
    {
        return Err(CliError::usage(format!("unknown command `{command}`")));
    }

    let cli = match Cli::try_parse_from(std::iter::once("conduit".to_string()).chain(args)) {
        Ok(cli) => cli,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            return Ok(Command::Display {
                output: error.to_string().trim_end().to_string(),
            });
        }
        Err(error) => return Err(CliError::usage(error.to_string())),
    };
    let format = if cli.json {
        OutputFormat::Json
    } else {
        OutputFormat::Text
    };

    Ok(match cli.command {
        None | Some(TopLevelCommand::About) => Command::About { format },
        Some(TopLevelCommand::Git(git)) => match git.command {
            GitSubcommand::Status(args) => Command::GitStatus {
                format,
                path: args.path,
            },
        },
        Some(TopLevelCommand::Help) => Command::Help { format },
        Some(TopLevelCommand::Logs(logs)) => match logs.command {
            LogsSubcommand::Auth(args) => Command::LogsAuth {
                format,
                environment: args.environment,
                secret_stdin: args.secret_stdin,
                check: args.check,
            },
            LogsSubcommand::Search(args) => Command::LogsSearch {
                format,
                service: args.service,
                filters: args.filters,
                levels: args.levels,
            },
            LogsSubcommand::Errors(args) => Command::LogsErrors {
                format,
                service: args.service,
                filters: args.filters,
            },
            LogsSubcommand::Watch(args) => Command::LogsWatch {
                format: if args.jsonl || format.is_json() {
                    LogStreamFormat::Jsonl
                } else {
                    LogStreamFormat::Text
                },
                service: args.service,
                filters: args.filters,
                levels: args.levels,
                interval: args.interval,
                timeout: args.timeout,
            },
            LogsSubcommand::Wait(args) => Command::LogsWait {
                format,
                service: args.service,
                filters: args.filters,
                levels: args.levels,
                interval: args.interval,
                timeout: args.timeout,
            },
        },
        Some(TopLevelCommand::Openapi(openapi)) => match openapi.command {
            OpenApiSubcommand::List(args) => Command::OpenApiList {
                format,
                service: args.service,
                environment: args.environment,
            },
            OpenApiSubcommand::Operation(args) => Command::OpenApiOperation {
                format,
                service: args.service,
                environment: args.environment,
                method: args.method,
                path: args.path,
            },
            OpenApiSubcommand::Search(args) => Command::OpenApiSearch {
                format,
                service: args.service,
                environment: args.environment,
                query: args.query,
                method: args.method,
            },
        },
        Some(TopLevelCommand::Plugin(plugin)) => match plugin.command {
            PluginSubcommand::Check(args) => {
                let target = plugin_check_target(args)?;
                Command::PluginCheck { format, target }
            }
        },
        Some(TopLevelCommand::Stats) => Command::Stats { format },
        Some(TopLevelCommand::Test(test)) => match test.command {
            TestSubcommand::Failed(args) => Command::TestFailed {
                format,
                tail_lines: args.tail,
            },
            TestSubcommand::Failures(args) => Command::TestFailures {
                format,
                path: args.path,
            },
            TestSubcommand::Last => Command::TestLast { format },
            TestSubcommand::Log(args) => Command::TestLog {
                format,
                path: args.path,
                tail_lines: args.tail,
            },
            TestSubcommand::Rerun(args) => Command::TestRerun {
                format,
                runner: args.runner,
            },
            TestSubcommand::Run(run) => match run.command {
                TestRunSubcommand::Gradle(args) => Command::TestRunGradle {
                    format,
                    selectors: args.selectors,
                    failed: args.failed,
                    profile: args.profile,
                    task: args.task,
                    report_path: args.report_path,
                    mode: args.mode.map(Into::into),
                    tail_lines: args.tail,
                    timeout: args.timeout,
                    heartbeat: args.heartbeat,
                    env: BTreeMap::new(),
                    gradle_args: args.gradle_args,
                },
            },
        },
        Some(TopLevelCommand::Worktree(worktree)) => match worktree.command {
            WorktreeSubcommand::List(args) => Command::WorktreeList {
                format,
                root: args.root,
            },
        },
    })
}

#[derive(Debug, PartialEq, Eq)]
enum PluginCheckTarget {
    Path {
        path: PathBuf,
        provider: PluginCheckProvider,
    },
    ConfiguredProvider(PluginCheckProvider),
}

fn plugin_check_target(args: PluginCheckArgs) -> Result<PluginCheckTarget, CliError> {
    match (args.path, args.provider) {
        (Some(path), provider) => {
            let provider = provider
                .as_deref()
                .map(plugin_check_provider)
                .transpose()?
                .unwrap_or(PluginCheckProvider::OpenApi);
            Ok(PluginCheckTarget::Path { path, provider })
        }
        (None, Some(provider)) => Ok(PluginCheckTarget::ConfiguredProvider(
            plugin_check_provider(&provider)?,
        )),
        (None, None) => Err(CliError::usage(
            "plugin check requires --path <component.wasm> or --provider <openapi|logs>",
        )),
    }
}

fn plugin_check_provider(value: &str) -> Result<PluginCheckProvider, CliError> {
    PluginCheckProvider::from_name(value).ok_or_else(|| {
        CliError::usage(format!(
            "unknown plugin provider `{value}`; expected `openapi` or `logs`"
        ))
    })
}

fn filter_openapi_operations(
    operations: &mut Vec<crate::openapi::OpenApiOperation>,
    query: &str,
    method: Option<&str>,
) {
    let query = query.to_lowercase();
    operations.retain(|operation| {
        method.is_none_or(|method| operation.method.eq_ignore_ascii_case(method))
            && openapi_operation_matches(operation, &query)
    });
}

fn openapi_operation_matches(operation: &crate::openapi::OpenApiOperation, query: &str) -> bool {
    operation.path.to_lowercase().contains(query)
        || operation
            .operation_id
            .as_deref()
            .unwrap_or_default()
            .to_lowercase()
            .contains(query)
        || operation
            .summary
            .as_deref()
            .unwrap_or_default()
            .to_lowercase()
            .contains(query)
        || operation
            .description
            .as_deref()
            .unwrap_or_default()
            .to_lowercase()
            .contains(query)
        || operation
            .parameters
            .iter()
            .any(|parameter| parameter.name.to_lowercase().contains(query))
}

fn infer_gradle_report_path(task: &str) -> PathBuf {
    let parts = task
        .split(':')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();

    match parts.as_slice() {
        [] => PathBuf::from("build/test-results/test"),
        [task_name] => PathBuf::from("build/test-results").join(task_name),
        parts => {
            let task_name = parts.last().expect("non-empty parts");
            let project_path = parts[..parts.len() - 1].iter().collect::<PathBuf>();
            project_path.join("build/test-results").join(task_name)
        }
    }
}

fn first_positional(args: &[String]) -> Option<&str> {
    args.iter()
        .find(|arg| {
            !matches!(
                arg.as_str(),
                "--json" | "--help" | "-h" | "--version" | "-V"
            )
        })
        .map(String::as_str)
}

fn render_last_test_failures(
    state: &LastTestFailures,
    tail_lines: Option<usize>,
) -> Result<String, crate::test_report::TestReportError> {
    let mut lines = vec![
        format!("report_path: {}", state.report_path),
        state.summary.render_text(),
    ];
    let selectors = state
        .summary
        .failures
        .iter()
        .filter_map(|failure| failure.selector.as_deref())
        .collect::<Vec<_>>();

    if !selectors.is_empty() {
        lines.push(String::new());
        lines.push(format!("selectors: {}", selectors.len()));
        lines.extend(
            selectors
                .into_iter()
                .map(|selector| format!("selector: {selector}")),
        );
    }

    if let Some(tail_lines) = tail_lines {
        lines.push(String::new());
        if let Some(log_path) = &state.log_path {
            let log = TestLogSummary::from_path(Path::new(log_path), tail_lines)?;
            lines.push(format!("log_tail: {}", log.lines.len()));
            lines.extend(log.lines.iter().map(|line| format!("log: {line}")));
        } else {
            lines.push("log_tail: unavailable".to_string());
            lines.push("hint: last failure state does not include a log path".to_string());
        }
    }

    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_about_command() {
        assert_eq!(
            parse([]).unwrap(),
            Command::About {
                format: OutputFormat::Text
            }
        );
    }

    #[test]
    fn parses_json_format() {
        assert_eq!(
            parse(["help".to_string(), "--json".to_string()]).unwrap(),
            Command::Help {
                format: OutputFormat::Json
            }
        );
    }

    #[test]
    fn parses_git_status() {
        assert_eq!(
            parse([
                "git".to_string(),
                "status".to_string(),
                "--path".to_string(),
                "/tmp/repo".to_string(),
                "--json".to_string()
            ])
            .unwrap(),
            Command::GitStatus {
                format: OutputFormat::Json,
                path: PathBuf::from("/tmp/repo")
            }
        );
    }

    #[test]
    fn parses_logs_search() {
        let mut filters = logs_filters();
        filters.environment = Some("staging".to_string());
        filters.since = Some("30m".to_string());
        filters.cid = Some("CID-123".to_string());
        filters.logger = Some("FixturePaymentService".to_string());
        filters.exclude_messages = vec!["known noise".to_string()];
        filters.exclude_class_names = vec!["NoisyClass".to_string()];
        filters.include_trace = true;

        assert_eq!(
            parse([
                "logs".to_string(),
                "search".to_string(),
                "fixture-service".to_string(),
                "--env".to_string(),
                "staging".to_string(),
                "--since".to_string(),
                "30m".to_string(),
                "--level".to_string(),
                "ERROR".to_string(),
                "--cid".to_string(),
                "CID-123".to_string(),
                "--logger".to_string(),
                "FixturePaymentService".to_string(),
                "--exclude-message".to_string(),
                "known noise".to_string(),
                "--exclude-class".to_string(),
                "NoisyClass".to_string(),
                "--include-trace".to_string(),
                "--json".to_string(),
            ])
            .unwrap(),
            Command::LogsSearch {
                format: OutputFormat::Json,
                service: "fixture-service".to_string(),
                filters,
                levels: vec!["ERROR".to_string()],
            }
        );
    }

    #[test]
    fn parses_logs_auth() {
        assert_eq!(
            parse([
                "logs".to_string(),
                "auth".to_string(),
                "--env".to_string(),
                "production".to_string(),
                "--secret-stdin".to_string(),
                "--json".to_string(),
            ])
            .unwrap(),
            Command::LogsAuth {
                format: OutputFormat::Json,
                environment: Some("production".to_string()),
                secret_stdin: true,
                check: false,
            }
        );
    }

    #[test]
    fn parses_logs_auth_check() {
        assert_eq!(
            parse([
                "logs".to_string(),
                "auth".to_string(),
                "--env".to_string(),
                "staging".to_string(),
                "--check".to_string(),
            ])
            .unwrap(),
            Command::LogsAuth {
                format: OutputFormat::Text,
                environment: Some("staging".to_string()),
                secret_stdin: false,
                check: true,
            }
        );
    }

    #[test]
    fn parses_logs_errors() {
        let mut filters = logs_filters();
        filters.date = Some("2026-05-22".to_string());
        filters.limit = 5;
        filters.class_name = Some("FixturePaymentService".to_string());

        assert_eq!(
            parse([
                "logs".to_string(),
                "errors".to_string(),
                "fixture-service".to_string(),
                "--date".to_string(),
                "2026-05-22".to_string(),
                "--limit".to_string(),
                "5".to_string(),
                "--class".to_string(),
                "FixturePaymentService".to_string(),
            ])
            .unwrap(),
            Command::LogsErrors {
                format: OutputFormat::Text,
                service: "fixture-service".to_string(),
                filters,
            }
        );
    }

    #[test]
    fn count_only_output_hides_logs_and_truncation_diagnostic() {
        let mut result = crate::logs::LogSearchResult {
            status: crate::logs::LogStatus::Ok,
            provider: "provider".to_string(),
            service: "service".to_string(),
            environment: None,
            time_range: crate::logs::LogTimeRange {
                from: "2026-05-22T10:00:00Z".to_string(),
                to: Some("2026-05-22T10:15:00Z".to_string()),
                source: "test".to_string(),
            },
            matches: 42,
            shown: 1,
            logs: vec![crate::logs::LogEvent {
                id: None,
                timestamp: "2026-05-22T10:00:01Z".to_string(),
                level: None,
                service: None,
                environment: None,
                cid: None,
                trace_id: None,
                logger: None,
                message: "log".to_string(),
                stack_trace: None,
                source: None,
                attributes_json: None,
            }],
            next_cursor: None,
            checked_until: None,
            diagnostics: vec![LogDiagnostic {
                kind: "query_truncated".to_string(),
                hint: Some("returned 1 of 42 matching logs".to_string()),
            }],
        };

        apply_count_only_output(&mut result, 0);

        assert_eq!(result.shown, 0);
        assert!(result.logs.is_empty());
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].kind, "count_only");
    }

    #[test]
    fn parses_logs_watch() {
        let mut filters = logs_filters();
        filters.since = Some("now".to_string());

        assert_eq!(
            parse([
                "logs".to_string(),
                "watch".to_string(),
                "fixture-service".to_string(),
                "--since".to_string(),
                "now".to_string(),
                "--level".to_string(),
                "ERROR".to_string(),
                "--interval".to_string(),
                "500ms".to_string(),
                "--timeout".to_string(),
                "1s".to_string(),
                "--jsonl".to_string(),
            ])
            .unwrap(),
            Command::LogsWatch {
                format: LogStreamFormat::Jsonl,
                service: "fixture-service".to_string(),
                filters,
                levels: vec!["ERROR".to_string()],
                interval: Duration::from_millis(500),
                timeout: Some(Duration::from_secs(1)),
            }
        );
    }

    #[test]
    fn parses_logs_wait() {
        let mut filters = logs_filters();
        filters.cid = Some("CID-123".to_string());

        assert_eq!(
            parse([
                "logs".to_string(),
                "wait".to_string(),
                "fixture-service".to_string(),
                "--cid".to_string(),
                "CID-123".to_string(),
                "--timeout".to_string(),
                "10s".to_string(),
                "--interval".to_string(),
                "500ms".to_string(),
            ])
            .unwrap(),
            Command::LogsWait {
                format: OutputFormat::Text,
                service: "fixture-service".to_string(),
                filters,
                levels: Vec::new(),
                interval: Duration::from_millis(500),
                timeout: Duration::from_secs(10),
            }
        );
    }

    #[test]
    fn parses_openapi_operation() {
        assert_eq!(
            parse([
                "openapi".to_string(),
                "operation".to_string(),
                "--service".to_string(),
                "catalog-service".to_string(),
                "--method".to_string(),
                "GET".to_string(),
                "--path".to_string(),
                "/items".to_string(),
                "--environment".to_string(),
                "staging".to_string(),
                "--json".to_string()
            ])
            .unwrap(),
            Command::OpenApiOperation {
                format: OutputFormat::Json,
                service: "catalog-service".to_string(),
                environment: Some("staging".to_string()),
                method: "GET".to_string(),
                path: "/items".to_string()
            }
        );
    }

    #[test]
    fn parses_openapi_search() {
        assert_eq!(
            parse([
                "openapi".to_string(),
                "search".to_string(),
                "--service".to_string(),
                "catalog-service".to_string(),
                "--query".to_string(),
                "price".to_string(),
                "--method".to_string(),
                "GET".to_string(),
            ])
            .unwrap(),
            Command::OpenApiSearch {
                format: OutputFormat::Text,
                service: "catalog-service".to_string(),
                environment: None,
                query: "price".to_string(),
                method: Some("GET".to_string()),
            }
        );
    }

    #[test]
    fn filters_openapi_operations_by_parameter_name() {
        let mut operations = vec![crate::openapi::OpenApiOperation {
            service: "catalog-service".to_string(),
            environment: None,
            method: "GET".to_string(),
            path: "/items/{item_id}/prices".to_string(),
            parameters: vec![crate::openapi::OpenApiParameter {
                name: "item_id".to_string(),
                location: crate::openapi::OpenApiParameterLocation::Path,
                required: true,
                description: None,
                schema_json: None,
            }],
            operation_id: Some("getItemPrices".to_string()),
            summary: None,
            description: None,
            request_schema_json: None,
            response_schema_json: None,
            source: None,
        }];

        filter_openapi_operations(&mut operations, "item_id", Some("GET"));

        assert_eq!(operations.len(), 1);
    }

    #[test]
    fn parses_openapi_list() {
        assert_eq!(
            parse([
                "openapi".to_string(),
                "list".to_string(),
                "--service".to_string(),
                "catalog-service".to_string()
            ])
            .unwrap(),
            Command::OpenApiList {
                format: OutputFormat::Text,
                service: "catalog-service".to_string(),
                environment: None
            }
        );
    }

    #[test]
    fn parses_plugin_check() {
        assert_eq!(
            parse([
                "plugin".to_string(),
                "check".to_string(),
                "--path".to_string(),
                ".conduit/plugins/company.wasm".to_string(),
                "--json".to_string()
            ])
            .unwrap(),
            Command::PluginCheck {
                format: OutputFormat::Json,
                target: PluginCheckTarget::Path {
                    path: PathBuf::from(".conduit/plugins/company.wasm"),
                    provider: PluginCheckProvider::OpenApi,
                }
            }
        );
    }

    #[test]
    fn parses_plugin_check_provider() {
        assert_eq!(
            parse([
                "plugin".to_string(),
                "check".to_string(),
                "--provider".to_string(),
                "openapi".to_string(),
            ])
            .unwrap(),
            Command::PluginCheck {
                format: OutputFormat::Text,
                target: PluginCheckTarget::ConfiguredProvider(PluginCheckProvider::OpenApi)
            }
        );
    }

    #[test]
    fn parses_plugin_check_path_for_logs_provider() {
        assert_eq!(
            parse([
                "plugin".to_string(),
                "check".to_string(),
                "--path".to_string(),
                ".conduit/plugins/logs.wasm".to_string(),
                "--provider".to_string(),
                "logs".to_string(),
            ])
            .unwrap(),
            Command::PluginCheck {
                format: OutputFormat::Text,
                target: PluginCheckTarget::Path {
                    path: PathBuf::from(".conduit/plugins/logs.wasm"),
                    provider: PluginCheckProvider::Logs,
                }
            }
        );
    }

    #[test]
    fn parses_plugin_check_configured_logs_provider() {
        assert_eq!(
            parse([
                "plugin".to_string(),
                "check".to_string(),
                "--provider".to_string(),
                "logs".to_string(),
            ])
            .unwrap(),
            Command::PluginCheck {
                format: OutputFormat::Text,
                target: PluginCheckTarget::ConfiguredProvider(PluginCheckProvider::Logs)
            }
        );
    }

    #[test]
    fn parses_stats() {
        assert_eq!(
            parse(["stats".to_string(), "--json".to_string()]).unwrap(),
            Command::Stats {
                format: OutputFormat::Json
            }
        );
    }

    #[test]
    fn parses_worktree_list() {
        assert_eq!(
            parse([
                "worktree".to_string(),
                "list".to_string(),
                "--root".to_string(),
                "/tmp/worktrees".to_string(),
                "--json".to_string()
            ])
            .unwrap(),
            Command::WorktreeList {
                format: OutputFormat::Json,
                root: PathBuf::from("/tmp/worktrees")
            }
        );
    }

    #[test]
    fn rejects_unknown_commands() {
        let error = parse(["missing".to_string()]).unwrap_err();
        assert_eq!(error.code, 2);
        assert_eq!(error.message, "unknown command `missing`");
    }

    #[test]
    fn parses_test_failures_with_path() {
        assert_eq!(
            parse([
                "test".to_string(),
                "failures".to_string(),
                "reports".to_string(),
                "--json".to_string()
            ])
            .unwrap(),
            Command::TestFailures {
                format: OutputFormat::Json,
                path: PathBuf::from("reports")
            }
        );
    }

    #[test]
    fn parses_test_failed() {
        assert_eq!(
            parse(["test".to_string(), "failed".to_string()]).unwrap(),
            Command::TestFailed {
                format: OutputFormat::Text,
                tail_lines: None
            }
        );
    }

    #[test]
    fn parses_test_failed_tail() {
        assert_eq!(
            parse([
                "test".to_string(),
                "failed".to_string(),
                "--tail".to_string(),
                "5".to_string()
            ])
            .unwrap(),
            Command::TestFailed {
                format: OutputFormat::Text,
                tail_lines: Some(5)
            }
        );
    }

    #[test]
    fn parses_test_last() {
        assert_eq!(
            parse(["test".to_string(), "last".to_string(), "--json".to_string()]).unwrap(),
            Command::TestLast {
                format: OutputFormat::Json
            }
        );
    }

    #[test]
    fn parses_test_log() {
        assert_eq!(
            parse([
                "test".to_string(),
                "log".to_string(),
                "--path".to_string(),
                "logs/test.log".to_string(),
                "--tail".to_string(),
                "5".to_string(),
                "--json".to_string()
            ])
            .unwrap(),
            Command::TestLog {
                format: OutputFormat::Json,
                path: Some(PathBuf::from("logs/test.log")),
                tail_lines: 5
            }
        );
    }

    #[test]
    fn parses_test_rerun() {
        assert_eq!(
            parse([
                "test".to_string(),
                "rerun".to_string(),
                "gradle".to_string(),
                "--json".to_string()
            ])
            .unwrap(),
            Command::TestRerun {
                format: OutputFormat::Json,
                runner: "gradle".to_string()
            }
        );
    }

    #[test]
    fn parses_test_run_gradle() {
        assert_eq!(
            parse([
                "test".to_string(),
                "run".to_string(),
                "gradle".to_string(),
                "--tests".to_string(),
                "com.example.PaymentServiceTest".to_string(),
                "--failed".to_string(),
                "--profile".to_string(),
                "integration".to_string(),
                "--task".to_string(),
                ":service:test".to_string(),
                "--report-path".to_string(),
                "service/build/test-results/test".to_string(),
                "--mode".to_string(),
                "integration".to_string(),
                "--tail".to_string(),
                "5".to_string(),
                "--timeout".to_string(),
                "2m".to_string(),
                "--heartbeat".to_string(),
                "30s".to_string(),
                "--json".to_string(),
                "--".to_string(),
                "-Penvironment=staging".to_string()
            ])
            .unwrap(),
            Command::TestRunGradle {
                format: OutputFormat::Json,
                selectors: vec!["com.example.PaymentServiceTest".to_string()],
                failed: true,
                profile: Some("integration".to_string()),
                task: Some(":service:test".to_string()),
                report_path: Some(PathBuf::from("service/build/test-results/test")),
                mode: Some(TestRunMode::Integration),
                tail_lines: Some(5),
                timeout: Some(Duration::from_secs(120)),
                heartbeat: Some(Duration::from_secs(30)),
                env: BTreeMap::new(),
                gradle_args: vec!["-Penvironment=staging".to_string()]
            }
        );
    }

    #[test]
    fn parses_duration_args() {
        assert_eq!(
            parse_duration_arg("500ms").unwrap(),
            Duration::from_millis(500)
        );
        assert_eq!(parse_duration_arg("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration_arg("2m").unwrap(), Duration::from_secs(120));
        assert_eq!(parse_duration_arg("1h").unwrap(), Duration::from_secs(3600));
        assert!(parse_duration_arg("30").is_err());
    }

    #[test]
    fn infers_gradle_report_paths_from_task() {
        assert_eq!(
            infer_gradle_report_path("test"),
            PathBuf::from("build/test-results/test")
        );
        assert_eq!(
            infer_gradle_report_path(":service:test"),
            PathBuf::from("service/build/test-results/test")
        );
        assert_eq!(
            infer_gradle_report_path(":apps:api:integrationTest"),
            PathBuf::from("apps/api/build/test-results/integrationTest")
        );
    }

    fn logs_filters() -> LogsFilterArgs {
        LogsFilterArgs {
            limit: DEFAULT_LOG_LIMIT,
            ..LogsFilterArgs::default()
        }
    }
}
