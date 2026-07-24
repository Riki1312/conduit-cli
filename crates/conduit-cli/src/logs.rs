use serde::Serialize;
use std::collections::BTreeSet;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use time::format_description::well_known::Rfc3339;
use time::{Date, Month, OffsetDateTime, UtcOffset};

pub(crate) const DEFAULT_LOG_LIMIT: usize = 20;
pub(crate) const DEFAULT_LOG_WATCH_LIMIT: usize = 50;
pub(crate) const DEFAULT_LOG_SINCE: &str = "15m";
const LOG_POLL_OVERLAP_SECONDS: i64 = 2;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LogTimeRangeInput {
    pub(crate) since: Option<String>,
    pub(crate) from: Option<String>,
    pub(crate) to: Option<String>,
    pub(crate) date: Option<String>,
}

impl LogTimeRangeInput {
    pub(crate) fn resolve(
        &self,
        now: SystemTime,
        default_since: Option<&str>,
    ) -> Result<LogTimeRange, LogError> {
        if self.date.is_some() && (self.since.is_some() || self.from.is_some() || self.to.is_some())
        {
            return Err(LogError::invalid_request(
                "`--date` cannot be combined with `--since`, `--from`, or `--to`",
            ));
        }
        if self.since.is_some() && (self.from.is_some() || self.to.is_some()) {
            return Err(LogError::invalid_request(
                "`--since` cannot be combined with `--from` or `--to`",
            ));
        }
        if self.to.is_some() && self.from.is_none() {
            return Err(LogError::invalid_request(
                "`--to` requires `--from` so the time range is explicit",
            ));
        }

        if let Some(date) = &self.date {
            let date = parse_date(date)?;
            let next = date.next_day().ok_or_else(|| {
                LogError::invalid_request("date is outside the supported time range")
            })?;
            return Ok(LogTimeRange {
                from: timestamp_for_date(date),
                to: Some(timestamp_for_date(next)),
                source: format!("date {}", format_date(date)),
            });
        }

        if let Some(from) = &self.from {
            let from = validate_utc_timestamp(from)?;
            let to = self
                .to
                .as_deref()
                .map(validate_utc_timestamp)
                .transpose()?
                .unwrap_or_else(|| format_system_time_utc(now));
            return Ok(LogTimeRange {
                source: format!("from {from} to {to}"),
                from,
                to: Some(to),
            });
        }

        let since = self
            .since
            .as_deref()
            .or(default_since)
            .unwrap_or(DEFAULT_LOG_SINCE);
        resolve_since(now, since)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct LogTimeRange {
    pub(crate) from: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) to: Option<String>,
    pub(crate) source: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LogQuery {
    pub(crate) service: String,
    pub(crate) environment: Option<String>,
    pub(crate) time_range: LogTimeRange,
    pub(crate) limit: usize,
    pub(crate) levels: Vec<String>,
    pub(crate) cid: Option<String>,
    pub(crate) trace_id: Option<String>,
    pub(crate) message: Option<String>,
    pub(crate) grep: Option<String>,
    pub(crate) logger: Option<String>,
    pub(crate) exclude_messages: Vec<String>,
    pub(crate) exclude_greps: Vec<String>,
    pub(crate) exclude_loggers: Vec<String>,
    pub(crate) include_trace: bool,
    pub(crate) cursor: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LogAuthRequest {
    pub(crate) environment: Option<String>,
    pub(crate) secret: Option<String>,
    pub(crate) check: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct LogAuthResult {
    pub(crate) status: LogAuthStatus,
    pub(crate) provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) environment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) destination: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) expires_at: Option<String>,
    pub(crate) diagnostics: Vec<LogDiagnostic>,
}

impl LogAuthResult {
    pub(crate) fn render_text(&self) -> String {
        let mut lines = vec![
            format!("status: {}", self.status.as_str()),
            format!("provider: {}", self.provider),
        ];
        push_optional(&mut lines, "environment", self.environment.as_ref());
        push_optional(&mut lines, "destination", self.destination.as_ref());
        push_optional(&mut lines, "expires_at", self.expires_at.as_ref());

        for diagnostic in &self.diagnostics {
            lines.push(String::new());
            lines.push(format!("diagnostic: {}", diagnostic.kind));
            push_optional(&mut lines, "hint", diagnostic.hint.as_ref());
        }

        lines.join("\n")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LogAuthStatus {
    Ok,
    ActionRequired,
}

impl LogAuthStatus {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::ActionRequired => "action_required",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct LogSearchResult {
    pub(crate) status: LogStatus,
    pub(crate) provider: String,
    pub(crate) service: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) environment: Option<String>,
    pub(crate) time_range: LogTimeRange,
    pub(crate) matches: usize,
    pub(crate) shown: usize,
    pub(crate) logs: Vec<LogEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) next_cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) checked_until: Option<String>,
    pub(crate) diagnostics: Vec<LogDiagnostic>,
}

impl LogSearchResult {
    pub(crate) fn render_text(&self) -> String {
        let mut lines = self.header_lines();
        push_diagnostics(&mut lines, &self.diagnostics);
        push_result_logs(&mut lines, &self.logs);

        lines.join("\n")
    }

    fn header_lines(&self) -> Vec<String> {
        let mut lines = vec![
            format!("status: {}", self.status.as_str()),
            format!("provider: {}", self.provider),
            format!("service: {}", self.service),
        ];
        push_optional(&mut lines, "environment", self.environment.as_ref());
        push_time_range(&mut lines, &self.time_range);
        lines.push(format!("matches: {}", self.matches));
        lines.push(format!("shown: {}", self.shown));
        lines
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LogStatus {
    Ok,
    Partial,
    AuthRequired,
    Unavailable,
    InvalidRequest,
    Error,
}

impl LogStatus {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Partial => "partial",
            Self::AuthRequired => "auth_required",
            Self::Unavailable => "unavailable",
            Self::InvalidRequest => "invalid_request",
            Self::Error => "error",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct LogEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) id: Option<String>,
    pub(crate) timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) service: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) environment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) logger: Option<String>,
    pub(crate) message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) stack_trace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) attributes_json: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct LogDiagnostic {
    pub(crate) kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) hint: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct LogError {
    pub(crate) kind: LogErrorKind,
    pub(crate) message: String,
}

impl LogError {
    pub(crate) fn new(kind: LogErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(LogErrorKind::InvalidRequest, message)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LogErrorKind {
    AuthRequired,
    Internal,
    InvalidRequest,
    PermissionDenied,
    Unavailable,
    Unsupported,
}

pub(crate) trait LogProvider {
    fn search(&self, query: &LogQuery) -> Result<LogSearchResult, LogError>;

    fn authenticate(&self, _request: &LogAuthRequest) -> Result<LogAuthResult, LogError> {
        Err(LogError::new(
            LogErrorKind::Unsupported,
            "logs provider does not support authentication",
        ))
    }
}

pub(crate) struct LogWatchRequest {
    pub(crate) query: LogQuery,
    pub(crate) interval: Duration,
    pub(crate) timeout: Option<Duration>,
    pub(crate) poll_to_now: bool,
}

pub(crate) struct LogWaitRequest {
    pub(crate) query: LogQuery,
    pub(crate) interval: Duration,
    pub(crate) timeout: Duration,
    pub(crate) poll_to_now: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LogStreamFormat {
    Text,
    Jsonl,
}

pub(crate) enum LogWatchEvent {
    Started(LogWatchStarted),
    Log(Box<LogEvent>),
    Heartbeat(LogWatchHeartbeat),
    Finished(LogWatchFinished),
}

impl LogWatchEvent {
    pub(crate) fn render_text(&self) -> String {
        match self {
            Self::Started(started) => started.render_text(),
            Self::Log(event) => render_stream_log_text(event),
            Self::Heartbeat(heartbeat) => heartbeat.render_text(),
            Self::Finished(finished) => finished.render_text(),
        }
    }

    pub(crate) fn render_json_line(&self) -> Result<String, LogError> {
        match self {
            Self::Started(started) => json_line(started),
            Self::Log(event) => json_line(&LogEventLine {
                event: "log",
                log: event,
            }),
            Self::Heartbeat(heartbeat) => json_line(heartbeat),
            Self::Finished(finished) => json_line(finished),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct LogWatchStarted {
    event: &'static str,
    pub(crate) service: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) environment: Option<String>,
    pub(crate) interval_ms: u64,
}

impl LogWatchStarted {
    fn new(query: &LogQuery, interval: Duration) -> Self {
        Self {
            event: "started",
            service: query.service.clone(),
            environment: query.environment.clone(),
            interval_ms: duration_ms(interval),
        }
    }

    fn render_text(&self) -> String {
        let mut lines = vec![
            "event: started".to_string(),
            format!("service: {}", self.service),
        ];
        push_optional(&mut lines, "environment", self.environment.as_ref());
        lines.push(format!("interval_ms: {}", self.interval_ms));
        lines.join("\n")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct LogWatchHeartbeat {
    event: &'static str,
    pub(crate) checked_until: String,
    pub(crate) new_logs: usize,
}

impl LogWatchHeartbeat {
    fn render_text(&self) -> String {
        [
            ("event", "heartbeat".to_string()),
            ("checked_until", self.checked_until.clone()),
            ("new_logs", self.new_logs.to_string()),
        ]
        .into_iter()
        .map(|(key, value)| format!("{key}: {value}"))
        .collect::<Vec<_>>()
        .join("\n")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct LogWatchFinished {
    event: &'static str,
    pub(crate) status: &'static str,
    pub(crate) checked_until: String,
}

impl LogWatchFinished {
    fn timeout(checked_until: String) -> Self {
        Self {
            event: "finished",
            status: "timeout",
            checked_until,
        }
    }

    fn render_text(&self) -> String {
        [
            ("event", "finished".to_string()),
            ("status", self.status.to_string()),
            ("checked_until", self.checked_until.clone()),
        ]
        .into_iter()
        .map(|(key, value)| format!("{key}: {value}"))
        .collect::<Vec<_>>()
        .join("\n")
    }
}

#[derive(Serialize)]
struct LogEventLine<'a> {
    event: &'static str,
    #[serde(flatten)]
    log: &'a LogEvent,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct LogWaitResult {
    pub(crate) status: LogWaitStatus,
    pub(crate) provider: String,
    pub(crate) service: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) environment: Option<String>,
    pub(crate) time_range: LogTimeRange,
    pub(crate) matches: usize,
    pub(crate) shown: usize,
    pub(crate) logs: Vec<LogEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) checked_until: Option<String>,
    pub(crate) timeout_ms: u64,
    pub(crate) diagnostics: Vec<LogDiagnostic>,
}

impl LogWaitResult {
    pub(crate) fn render_text(&self) -> String {
        let mut lines = vec![
            format!("status: {}", self.status.as_str()),
            format!("provider: {}", self.provider),
            format!("service: {}", self.service),
        ];
        push_optional(&mut lines, "environment", self.environment.as_ref());
        push_time_range(&mut lines, &self.time_range);
        lines.push(format!("matches: {}", self.matches));
        lines.push(format!("shown: {}", self.shown));
        push_optional(&mut lines, "checked_until", self.checked_until.as_ref());
        lines.push(format!("timeout_ms: {}", self.timeout_ms));
        push_diagnostics(&mut lines, &self.diagnostics);
        push_result_logs(&mut lines, &self.logs);

        lines.join("\n")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LogWaitStatus {
    Matched,
    Timeout,
}

impl LogWaitStatus {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Matched => "matched",
            Self::Timeout => "timeout",
        }
    }
}

pub(crate) fn watch_logs(
    provider: &dyn LogProvider,
    request: LogWatchRequest,
    mut emit: impl FnMut(LogWatchEvent) -> Result<(), LogError>,
) -> Result<(), LogError> {
    let started_at = SystemTime::now();
    let deadline = request
        .timeout
        .and_then(|timeout| started_at.checked_add(timeout));
    let mut query = request.query;
    let mut seen = BTreeSet::new();

    emit(LogWatchEvent::Started(LogWatchStarted::new(
        &query,
        request.interval,
    )))?;

    loop {
        prepare_poll_query(&mut query, request.poll_to_now, SystemTime::now());
        let result = provider.search(&query)?;
        ensure_poll_result_is_usable(&result)?;
        let checked_until = checked_until(&result);

        let logs = new_logs(result.logs.clone(), &mut seen);
        let new_log_count = logs.len();
        for event in logs {
            emit(LogWatchEvent::Log(Box::new(event)))?;
        }
        emit(LogWatchEvent::Heartbeat(LogWatchHeartbeat {
            event: "heartbeat",
            checked_until: checked_until.clone(),
            new_logs: new_log_count,
        }))?;

        advance_poll_query(&mut query, &result, request.poll_to_now);
        if deadline_reached(deadline, SystemTime::now()) {
            emit(LogWatchEvent::Finished(LogWatchFinished::timeout(
                checked_until,
            )))?;
            return Ok(());
        }
        sleep_until_next_poll(deadline, request.interval);
    }
}

pub(crate) fn wait_for_logs(
    provider: &dyn LogProvider,
    request: LogWaitRequest,
) -> Result<LogWaitResult, LogError> {
    let started_at = SystemTime::now();
    let deadline = started_at.checked_add(request.timeout).ok_or_else(|| {
        LogError::invalid_request("logs wait timeout is outside the supported time range")
    })?;
    let mut query = request.query;
    let mut seen = BTreeSet::new();

    loop {
        prepare_poll_query(&mut query, request.poll_to_now, SystemTime::now());
        let result = provider.search(&query)?;
        ensure_poll_result_is_usable(&result)?;
        let logs = new_logs(result.logs.clone(), &mut seen);

        if !logs.is_empty() {
            return Ok(wait_result(
                LogWaitStatus::Matched,
                result,
                logs,
                request.timeout,
            ));
        }

        if deadline_reached(Some(deadline), SystemTime::now()) {
            return Ok(wait_result(
                LogWaitStatus::Timeout,
                result,
                Vec::new(),
                request.timeout,
            ));
        }

        advance_poll_query(&mut query, &result, request.poll_to_now);
        sleep_until_next_poll(Some(deadline), request.interval);
    }
}

pub(crate) struct FixtureLogProvider;

impl LogProvider for FixtureLogProvider {
    fn search(&self, query: &LogQuery) -> Result<LogSearchResult, LogError> {
        let mut logs = fixture_logs(&query.service, query.environment.as_deref())
            .into_iter()
            .filter(|event| log_matches(event, query))
            .filter(|event| log_time_matches(event, &query.time_range))
            .map(|mut event| {
                if !query.include_trace {
                    event.stack_trace = None;
                }
                event
            })
            .collect::<Vec<_>>();
        let matches = logs.len();
        logs.truncate(query.limit);

        Ok(LogSearchResult {
            status: LogStatus::Ok,
            provider: "fixture-logs".to_string(),
            service: query.service.clone(),
            environment: query.environment.clone(),
            time_range: query.time_range.clone(),
            matches,
            shown: logs.len(),
            logs,
            next_cursor: None,
            checked_until: query.time_range.to.clone(),
            diagnostics: Vec::new(),
        })
    }

    fn authenticate(&self, request: &LogAuthRequest) -> Result<LogAuthResult, LogError> {
        let diagnostics = if request.check {
            vec![LogDiagnostic {
                kind: "auth_valid".to_string(),
                hint: Some("fixture logs auth is valid".to_string()),
            }]
        } else {
            Vec::new()
        };

        Ok(LogAuthResult {
            status: LogAuthStatus::Ok,
            provider: "fixture-logs".to_string(),
            environment: request.environment.clone(),
            destination: Some("fixture://logs/auth".to_string()),
            expires_at: None,
            diagnostics,
        })
    }
}

fn resolve_since(now: SystemTime, since: &str) -> Result<LogTimeRange, LogError> {
    let to = format_system_time_utc(now);
    if since == "now" {
        return Ok(LogTimeRange {
            from: to.clone(),
            to: Some(to),
            source: "since now".to_string(),
        });
    }

    let duration = parse_duration(since)?;
    let from = now.checked_sub(duration).ok_or_else(|| {
        LogError::invalid_request(format!(
            "`--since {since}` is outside the supported time range"
        ))
    })?;

    Ok(LogTimeRange {
        from: format_system_time_utc(from),
        to: Some(to),
        source: format!("since {since}"),
    })
}

fn wait_result(
    status: LogWaitStatus,
    result: LogSearchResult,
    logs: Vec<LogEvent>,
    timeout: Duration,
) -> LogWaitResult {
    LogWaitResult {
        status,
        provider: result.provider,
        service: result.service,
        environment: result.environment,
        time_range: result.time_range,
        matches: logs.len(),
        shown: logs.len(),
        logs,
        checked_until: result.checked_until,
        timeout_ms: duration_ms(timeout),
        diagnostics: result.diagnostics,
    }
}

fn render_stream_log_text(event: &LogEvent) -> String {
    let mut lines = vec![
        "event: log".to_string(),
        format!("timestamp: {}", event.timestamp),
    ];
    push_optional(&mut lines, "level", event.level.as_ref());
    push_optional(&mut lines, "service", event.service.as_ref());
    push_optional(&mut lines, "environment", event.environment.as_ref());
    push_optional(&mut lines, "cid", event.cid.as_ref());
    push_optional(&mut lines, "trace_id", event.trace_id.as_ref());
    push_optional(&mut lines, "logger", event.logger.as_ref());
    lines.push(format!("message: {}", event.message));
    if let Some(stack_trace) = &event.stack_trace {
        lines.push(format!("stack_trace: {stack_trace}"));
    }
    lines.join("\n")
}

fn json_line(value: &impl Serialize) -> Result<String, LogError> {
    serde_json::to_string(value).map_err(|error| {
        LogError::new(
            LogErrorKind::Internal,
            format!("failed to render logs JSONL event: {error}"),
        )
    })
}

fn duration_ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn prepare_poll_query(query: &mut LogQuery, poll_to_now: bool, now: SystemTime) {
    if poll_to_now && query.cursor.is_none() {
        query.time_range.to = Some(format_system_time_utc(now));
    }
}

fn advance_poll_query(query: &mut LogQuery, result: &LogSearchResult, poll_to_now: bool) {
    if let Some(cursor) = &result.next_cursor {
        query.cursor = Some(cursor.clone());
        return;
    }

    query.cursor = None;
    if poll_to_now {
        let checked_until = checked_until(result);
        query.time_range.from = timestamp_with_poll_overlap(&checked_until);
        query.time_range.source = format!("follow from {}", query.time_range.from);
    }
}

fn checked_until(result: &LogSearchResult) -> String {
    result
        .checked_until
        .clone()
        .or_else(|| result.time_range.to.clone())
        .unwrap_or_else(|| result.time_range.from.clone())
}

fn new_logs(events: Vec<LogEvent>, seen: &mut BTreeSet<String>) -> Vec<LogEvent> {
    let mut events = events
        .into_iter()
        .filter(|event| seen.insert(log_identity(event)))
        .collect::<Vec<_>>();
    events.sort_by(|left, right| {
        left.timestamp
            .cmp(&right.timestamp)
            .then_with(|| left.message.cmp(&right.message))
    });
    events
}

fn log_identity(event: &LogEvent) -> String {
    event.id.clone().unwrap_or_else(|| {
        [
            event.timestamp.as_str(),
            event.level.as_deref().unwrap_or_default(),
            event.logger.as_deref().unwrap_or_default(),
            event.message.as_str(),
            event.cid.as_deref().unwrap_or_default(),
        ]
        .join("\u{1f}")
    })
}

fn ensure_poll_result_is_usable(result: &LogSearchResult) -> Result<(), LogError> {
    match &result.status {
        LogStatus::Ok | LogStatus::Partial => Ok(()),
        LogStatus::AuthRequired => Err(LogError::new(
            LogErrorKind::AuthRequired,
            "logs provider requires authentication",
        )),
        LogStatus::Unavailable => Err(LogError::new(
            LogErrorKind::Unavailable,
            "logs provider is unavailable",
        )),
        LogStatus::InvalidRequest => Err(LogError::new(
            LogErrorKind::InvalidRequest,
            "logs provider rejected the request",
        )),
        LogStatus::Error => Err(LogError::new(
            LogErrorKind::Internal,
            "logs provider returned an error",
        )),
    }
}

fn deadline_reached(deadline: Option<SystemTime>, now: SystemTime) -> bool {
    deadline.is_some_and(|deadline| now.duration_since(deadline).is_ok())
}

fn sleep_until_next_poll(deadline: Option<SystemTime>, interval: Duration) {
    let now = SystemTime::now();
    let remaining = deadline.and_then(|deadline| deadline.duration_since(now).ok());
    let sleep_for = remaining.map_or(interval, |remaining| remaining.min(interval));
    if !sleep_for.is_zero() {
        thread::sleep(sleep_for);
    }
}

fn timestamp_with_poll_overlap(timestamp: &str) -> String {
    parse_timestamp(timestamp)
        .and_then(|timestamp| {
            OffsetDateTime::from_unix_timestamp(
                timestamp
                    .unix_timestamp()
                    .saturating_sub(LOG_POLL_OVERLAP_SECONDS),
            )
            .ok()
        })
        .map(format_timestamp)
        .unwrap_or_else(|| timestamp.to_string())
}

fn log_time_matches(event: &LogEvent, time_range: &LogTimeRange) -> bool {
    let Some(timestamp) = parse_timestamp(&event.timestamp) else {
        return false;
    };
    let Some(from) = parse_timestamp(&time_range.from) else {
        return false;
    };
    let to = time_range.to.as_deref().and_then(parse_timestamp);

    timestamp >= from && to.is_none_or(|to| timestamp < to)
}

fn parse_timestamp(value: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(value, &Rfc3339)
        .map(|timestamp| timestamp.to_offset(UtcOffset::UTC))
        .ok()
}

fn parse_duration(value: &str) -> Result<Duration, LogError> {
    let (number, unit) = value.split_at(
        value
            .find(|ch: char| !ch.is_ascii_digit())
            .unwrap_or(value.len()),
    );
    if number.is_empty() || unit.is_empty() {
        return Err(LogError::invalid_request(
            "duration must look like `30s`, `15m`, `2h`, or `1d`",
        ));
    }
    let amount = number
        .parse::<u64>()
        .map_err(|_| LogError::invalid_request("duration amount must be a positive integer"))?;
    if amount == 0 {
        return Err(LogError::invalid_request(
            "duration amount must be greater than zero",
        ));
    }

    match unit {
        "s" => Ok(Duration::from_secs(amount)),
        "m" => Ok(Duration::from_secs(amount * 60)),
        "h" => Ok(Duration::from_secs(amount * 60 * 60)),
        "d" => Ok(Duration::from_secs(amount * 60 * 60 * 24)),
        _ => Err(LogError::invalid_request(
            "duration unit must be `s`, `m`, `h`, or `d`",
        )),
    }
}

fn validate_utc_timestamp(value: &str) -> Result<String, LogError> {
    OffsetDateTime::parse(value, &Rfc3339)
        .map(format_timestamp)
        .map_err(|_| {
            LogError::invalid_request(
                "timestamp must use RFC3339 format like `2026-05-22T10:00:00Z`",
            )
        })
}

fn parse_date(value: &str) -> Result<Date, LogError> {
    let bytes = value.as_bytes();
    let valid = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit());
    if !valid {
        return Err(LogError::invalid_request(
            "date must use format `YYYY-MM-DD`",
        ));
    }

    let year = value[..4]
        .parse::<i32>()
        .map_err(|_| LogError::invalid_request("date year must be valid"))?;
    let month = parse_two_digits(&value[5..7], "date month")?;
    let day = parse_two_digits(&value[8..10], "date day")?;
    let month = Month::try_from(month as u8)
        .map_err(|_| LogError::invalid_request("date must be a valid calendar day"))?;
    Date::from_calendar_date(year, month, day as u8)
        .map_err(|_| LogError::invalid_request("date must be a valid calendar day"))
}

fn parse_two_digits(value: &str, label: &str) -> Result<u32, LogError> {
    value
        .parse::<u32>()
        .map_err(|_| LogError::invalid_request(format!("{label} must be numeric")))
}

fn timestamp_for_date(date: Date) -> String {
    format!("{}T00:00:00Z", format_date(date))
}

fn format_system_time_utc(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let timestamp = OffsetDateTime::from_unix_timestamp(seconds as i64)
        .expect("system time after UNIX_EPOCH should fit OffsetDateTime");
    format_timestamp(timestamp)
}

fn format_timestamp(timestamp: OffsetDateTime) -> String {
    let timestamp = timestamp.to_offset(UtcOffset::UTC);
    let timestamp = timestamp
        .replace_nanosecond(0)
        .expect("zero nanoseconds should be valid");
    timestamp
        .format(&Rfc3339)
        .expect("formatting RFC3339 timestamps should not fail")
}

fn format_date(date: Date) -> String {
    format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        u8::from(date.month()),
        date.day()
    )
}

fn fixture_logs(service: &str, environment: Option<&str>) -> Vec<LogEvent> {
    if service != "fixture-service" {
        return Vec::new();
    }

    let environment = environment.map(str::to_string);
    vec![
        LogEvent {
            id: Some("fixture-2".to_string()),
            timestamp: "2026-05-22T09:01:00Z".to_string(),
            level: Some("ERROR".to_string()),
            service: Some(service.to_string()),
            environment: environment.clone(),
            cid: Some("CID-123".to_string()),
            trace_id: Some("trace-456".to_string()),
            logger: Some("FixturePaymentService".to_string()),
            message: "ACCOUNT_NOT_ACTIVATED".to_string(),
            stack_trace: Some("java.lang.IllegalStateException: account not activated".to_string()),
            source: Some("fixture://logs/fixture-service".to_string()),
            attributes_json: None,
        },
        LogEvent {
            id: Some("fixture-1".to_string()),
            timestamp: "2026-05-22T09:00:00Z".to_string(),
            level: Some("INFO".to_string()),
            service: Some(service.to_string()),
            environment,
            cid: Some("CID-123".to_string()),
            trace_id: None,
            logger: Some("FixturePaymentService".to_string()),
            message: "payment accepted".to_string(),
            stack_trace: None,
            source: Some("fixture://logs/fixture-service".to_string()),
            attributes_json: None,
        },
    ]
}

fn log_matches(event: &LogEvent, query: &LogQuery) -> bool {
    (query.levels.is_empty()
        || query.levels.iter().any(|level| {
            event
                .level
                .as_deref()
                .is_some_and(|event_level| event_level.eq_ignore_ascii_case(level))
        }))
        && query
            .cid
            .as_deref()
            .is_none_or(|cid| event.cid.as_deref() == Some(cid))
        && query
            .trace_id
            .as_deref()
            .is_none_or(|trace_id| event.trace_id.as_deref() == Some(trace_id))
        && query
            .message
            .as_deref()
            .is_none_or(|message| contains_case_insensitive(&event.message, message))
        && query
            .grep
            .as_deref()
            .is_none_or(|grep| log_text_matches(event, grep))
        && query.logger.as_deref().is_none_or(|logger| {
            contains_case_insensitive(event.logger.as_deref().unwrap_or_default(), logger)
        })
        && !query
            .exclude_messages
            .iter()
            .any(|message| contains_case_insensitive(&event.message, message))
        && !query
            .exclude_greps
            .iter()
            .any(|grep| log_text_matches(event, grep))
        && !query.exclude_loggers.iter().any(|logger| {
            contains_case_insensitive(event.logger.as_deref().unwrap_or_default(), logger)
        })
}

fn log_text_matches(event: &LogEvent, needle: &str) -> bool {
    contains_case_insensitive(&event.message, needle)
        || event
            .stack_trace
            .as_deref()
            .is_some_and(|stack_trace| contains_case_insensitive(stack_trace, needle))
        || event
            .logger
            .as_deref()
            .is_some_and(|logger| contains_case_insensitive(logger, needle))
}

fn contains_case_insensitive(value: &str, needle: &str) -> bool {
    value.to_lowercase().contains(&needle.to_lowercase())
}

fn push_optional(lines: &mut Vec<String>, key: &str, value: Option<&String>) {
    if let Some(value) = value {
        lines.push(format!("{key}: {value}"));
    }
}

fn push_optional_indented(lines: &mut Vec<String>, key: &str, value: Option<&String>) {
    if let Some(value) = value {
        lines.push(format!("  {key}: {value}"));
    }
}

fn push_time_range(lines: &mut Vec<String>, time_range: &LogTimeRange) {
    lines.push("time_range:".to_string());
    lines.push(format!("  from: {}", time_range.from));
    if let Some(to) = &time_range.to {
        lines.push(format!("  to: {to}"));
    }
    lines.push(format!("  source: {}", time_range.source));
}

fn push_diagnostics(lines: &mut Vec<String>, diagnostics: &[LogDiagnostic]) {
    for diagnostic in diagnostics {
        lines.push(String::new());
        lines.push(format!("diagnostic: {}", diagnostic.kind));
        push_optional(lines, "hint", diagnostic.hint.as_ref());
    }
}

fn push_result_logs(lines: &mut Vec<String>, logs: &[LogEvent]) {
    for event in logs {
        lines.push(String::new());
        lines.push("log:".to_string());
        lines.push(format!("  timestamp: {}", event.timestamp));
        push_optional_indented(lines, "level", event.level.as_ref());
        push_optional_indented(lines, "cid", event.cid.as_ref());
        push_optional_indented(lines, "trace_id", event.trace_id.as_ref());
        push_optional_indented(lines, "logger", event.logger.as_ref());
        lines.push(format!("  message: {}", event.message));
        if let Some(stack_trace) = &event.stack_trace {
            lines.push(format!("  stack_trace: {stack_trace}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_since_range() {
        let range = LogTimeRangeInput {
            since: Some("30m".to_string()),
            ..LogTimeRangeInput::default()
        }
        .resolve(now(), None)
        .unwrap();

        assert_eq!(range.from, "2026-05-22T13:30:00Z");
        assert_eq!(range.to.as_deref(), Some("2026-05-22T14:00:00Z"));
        assert_eq!(range.source, "since 30m");
    }

    #[test]
    fn resolves_date_range() {
        let range = LogTimeRangeInput {
            date: Some("2026-05-22".to_string()),
            ..LogTimeRangeInput::default()
        }
        .resolve(now(), None)
        .unwrap();

        assert_eq!(range.from, "2026-05-22T00:00:00Z");
        assert_eq!(range.to.as_deref(), Some("2026-05-23T00:00:00Z"));
        assert_eq!(range.source, "date 2026-05-22");
    }

    #[test]
    fn resolves_rfc3339_offsets_to_utc() {
        let range = LogTimeRangeInput {
            from: Some("2026-05-22T12:00:00+02:00".to_string()),
            to: Some("2026-05-22T12:30:00+02:00".to_string()),
            ..LogTimeRangeInput::default()
        }
        .resolve(now(), None)
        .unwrap();

        assert_eq!(range.from, "2026-05-22T10:00:00Z");
        assert_eq!(range.to.as_deref(), Some("2026-05-22T10:30:00Z"));
        assert_eq!(
            range.source,
            "from 2026-05-22T10:00:00Z to 2026-05-22T10:30:00Z"
        );
    }

    #[test]
    fn rejects_ambiguous_time_filters() {
        let error = LogTimeRangeInput {
            since: Some("15m".to_string()),
            from: Some("2026-05-22T10:00:00Z".to_string()),
            ..LogTimeRangeInput::default()
        }
        .resolve(now(), None)
        .unwrap_err();

        assert_eq!(
            error.message,
            "`--since` cannot be combined with `--from` or `--to`"
        );
    }

    #[test]
    fn fixture_provider_filters_logs() {
        let query = LogQuery {
            levels: vec!["ERROR".to_string()],
            cid: Some("CID-123".to_string()),
            message: Some("activated".to_string()),
            logger: Some("payment".to_string()),
            include_trace: true,
            ..fixture_query()
        };

        let result = FixtureLogProvider.search(&query).unwrap();

        assert_eq!(result.matches, 1);
        assert_eq!(result.logs[0].message, "ACCOUNT_NOT_ACTIVATED");
        assert_eq!(result.logs[0].environment.as_deref(), Some("staging"));
    }

    #[test]
    fn fixture_provider_greps_message_stack_trace_and_logger() {
        let query = LogQuery {
            grep: Some("illegalstateexception".to_string()),
            include_trace: true,
            ..fixture_query()
        };

        let result = FixtureLogProvider.search(&query).unwrap();

        assert_eq!(result.matches, 1);
        assert_eq!(result.logs[0].message, "ACCOUNT_NOT_ACTIVATED");

        let query = LogQuery {
            grep: Some("paymentservice".to_string()),
            ..fixture_query()
        };
        let result = FixtureLogProvider.search(&query).unwrap();

        assert_eq!(result.matches, 2);
    }

    #[test]
    fn fixture_provider_excludes_messages_and_loggers() {
        let query = LogQuery {
            exclude_messages: vec!["accepted".to_string()],
            ..fixture_query()
        };

        let result = FixtureLogProvider.search(&query).unwrap();

        assert_eq!(result.matches, 1);
        assert_eq!(result.logs[0].message, "ACCOUNT_NOT_ACTIVATED");

        let query = LogQuery {
            exclude_messages: Vec::new(),
            exclude_loggers: vec!["FixturePaymentService".to_string()],
            ..query
        };
        let result = FixtureLogProvider.search(&query).unwrap();

        assert_eq!(result.matches, 0);
    }

    #[test]
    fn fixture_provider_excludes_grep_matches() {
        let query = LogQuery {
            exclude_greps: vec!["IllegalStateException".to_string()],
            ..fixture_query()
        };

        let result = FixtureLogProvider.search(&query).unwrap();

        assert_eq!(result.matches, 1);
        assert_eq!(result.logs[0].message, "payment accepted");
    }

    #[test]
    fn renders_compact_text() {
        let query = LogQuery {
            limit: 1,
            ..fixture_query()
        };
        let result = FixtureLogProvider.search(&query).unwrap();

        let text = result.render_text();

        assert!(text.contains("status: ok\n"));
        assert!(text.contains("provider: fixture-logs\n"));
        assert!(text.contains("time_range:\n"));
        assert!(text.contains("matches: 2\nshown: 1\n"));
        assert!(text.contains("log:\n  timestamp: 2026-05-22T09:01:00Z\n"));
    }

    #[test]
    fn fixture_provider_filters_by_time_range() {
        let query = LogQuery {
            environment: None,
            time_range: LogTimeRangeInput {
                from: Some("2026-05-22T09:00:30Z".to_string()),
                to: Some("2026-05-22T09:02:00Z".to_string()),
                ..LogTimeRangeInput::default()
            }
            .resolve(now(), None)
            .unwrap(),
            ..fixture_query()
        };

        let result = FixtureLogProvider.search(&query).unwrap();

        assert_eq!(result.matches, 1);
        assert_eq!(result.logs[0].id.as_deref(), Some("fixture-2"));
    }

    fn now() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_779_458_400)
    }

    fn fixture_query() -> LogQuery {
        LogQuery {
            service: "fixture-service".to_string(),
            environment: Some("staging".to_string()),
            time_range: LogTimeRangeInput {
                date: Some("2026-05-22".to_string()),
                ..LogTimeRangeInput::default()
            }
            .resolve(now(), None)
            .unwrap(),
            limit: 20,
            levels: Vec::new(),
            cid: None,
            trace_id: None,
            message: None,
            grep: None,
            logger: None,
            exclude_messages: Vec::new(),
            exclude_greps: Vec::new(),
            exclude_loggers: Vec::new(),
            include_trace: false,
            cursor: None,
        }
    }
}
