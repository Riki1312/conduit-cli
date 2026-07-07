# Logs Provider Design

Conduit logs commands should make service logs usable as compact engineering
facts. The core should define a standard log query model, output schema,
rendering, watch behavior, and capability boundaries. Plugins should adapt that
model to specific log platforms such as OpenSearch, Datadog, CloudWatch, Loki,
Elastic, or company-internal dashboards.

The goal is not to wrap one existing log script. The goal is to define a useful
developer and agent experience for the common debugging loop: trigger an
action, find the relevant backend logs, inspect the failure, and optionally wait
for a new matching event.

## Goals

- Keep the command surface product-neutral and provider-independent.
- Make common debugging queries short, precise, and reproducible.
- Support bounded one-shot searches and long-running watch/wait workflows.
- Return normalized log events that are easy to read and easy to parse.
- Keep auth, index naming, backend query languages, and dashboard details inside
  provider plugins.
- Treat log data, cookies, tokens, and customer identifiers as sensitive.
- Make the resolved time range explicit in every result.

## Non-Goals

- Do not expose every backend query feature as first-class CLI flags.
- Do not stream raw dashboard output by default.
- Do not store credentials, cookies, or raw log captures in repository files.
- Do not make the company-specific OpenSearch model part of the core contract.
- Do not let plugins print arbitrary terminal output.
- Do not build an incident-management or alerting product in the first pass.

## Command Shape

Start with a small command set:

```bash
conduit logs search SERVICE --cid PaM9CQjf --since 30m
conduit logs search SERVICE --message ACCOUNT_NOT_ACTIVATED --since 1h
conduit logs search SERVICE --logger TradeIntentService --level error --date 2026-05-22
conduit logs search SERVICE --exclude-class NoisyLogger --exclude-message "known noise" --limit 0
conduit logs errors SERVICE --since 15m --limit 20
conduit logs watch SERVICE --level error --since now --interval 5s
conduit logs watch SERVICE --level error --since now --interval 5s --timeout 2m
conduit logs wait SERVICE --cid PaM9CQjf --timeout 2m
conduit logs auth --env production
conduit logs auth --env production --secret-stdin
```

`search` runs a bounded query and exits.

`errors` is a core convenience command that maps to `search` with an error-level
filter. It is not a separate provider interface.

`watch` repeatedly queries or streams, prints only new matching events, and
continues until interrupted. An optional `--timeout` can bound watch mode for
scripts, agents, and tests without changing the default long-running behavior.

`wait` exits successfully when at least one matching log appears and fails on
timeout. This is useful after triggering an action from an integration test, API
script, or agent workflow.

`auth` delegates provider-specific authentication to the configured logs plugin
while preserving Conduit-owned output and redaction. `--secret-stdin` is the
initial provider-neutral path for externally acquired auth material; the plugin
decides what that value means and stores it through explicit secret
capabilities. A later `--provider` flag can support authenticating a provider
outside the current project config.

## Filters

The first core filters should cover the most common debugging tasks:

- `--env`: logical environment such as `dev`, `test`, `staging`, or
  `production`.
- `--since`: relative time range such as `5m`, `30m`, `2h`, or `1d`.
- `--from` and `--to`: precise RFC 3339 timestamps.
- `--date`: whole-day convenience for date-partitioned backends.
- `--limit`: maximum number of returned events. `search` and `errors` accept
  `--limit 0` for count-only queries; `watch` and `wait` require a positive
  limit because they need returned events to make progress.
- `--level`: repeatable severity filter.
- `--cid` / `--correlation-id`: correlation id filter.
- `--trace-id`: distributed trace id filter.
- `--message`: message text filter.
- `--logger`: logger, class, module, or component filter.
- `--class`: alias for `--logger`, useful in Java services.
- `--exclude-message`: repeatable message text exclusion.
- `--exclude-logger`: repeatable logger, class, module, or component exclusion.
- `--exclude-class`: alias for `--exclude-logger`, useful in Java services.
- `--include-trace`: include stack traces or extended exception fields.

The core should parse and validate common filters, then pass a normalized
request to the provider. Plugins decide how the filters map to backend query
syntax.

Provider-specific raw queries should be deferred. A future escape hatch such as
`--provider-query` can be added if standard filters are insufficient for real
debugging work, but it should not be part of the first implementation.

## Time Ranges

Time range handling belongs in core because it affects reproducibility and watch
semantics.

Supported forms:

```bash
--since 30m
--from 2026-05-22T10:00:00Z --to 2026-05-22T10:30:00Z
--date 2026-05-22
```

Defaults:

- `search`: `--since 15m` unless the project config chooses another default.
- `errors`: `--since 15m`.
- `watch`: `--since now`.
- `wait`: `--since now`.

Conduit should resolve relative time locally and echo absolute timestamps in the
result:

```text
time_range:
  from: 2026-05-22T08:45:00Z
  to: 2026-05-22T09:00:00Z
  source: since 15m
```

For date-partitioned providers, the provider can derive the index or partition
set from the resolved range and optional `date` source.

## Output Formats

Text output should remain the default because it is the best interactive agent
experience. It must still be a rendering of structured facts.

Conduit already uses `--json` as the machine-readable output flag. Logs should
keep that convention instead of introducing a separate `--format` flag for the
first implementation.

Supported output modes:

- default text: compact, deterministic, agent-first text.
- `--json`: one complete JSON object for bounded commands.
- `--jsonl`: newline-delimited events for watch mode.

TOML should remain a configuration format, not a log output format. Logs are
runtime event data, and JSON/JSONL compose better with scripts, streaming tools,
and other systems.

Text search output example:

```text
status: ok
provider: company-logs
environment: staging
service: checkout-service
time_range:
  from: 2026-05-22T08:45:00Z
  to: 2026-05-22T09:00:00Z
  source: since 15m
matches: 3
shown: 3

log:
  timestamp: 2026-05-22T08:58:03.421Z
  level: ERROR
  cid: PaM9CQjf
  logger: TradeIntentService
  message: ACCOUNT_NOT_ACTIVATED
```

JSON search output example:

```json
{
  "status": "ok",
  "provider": "company-logs",
  "service": "checkout-service",
  "environment": "staging",
  "time_range": {
    "from": "2026-05-22T08:45:00Z",
    "to": "2026-05-22T09:00:00Z",
    "source": "since 15m"
  },
  "matches": 3,
  "shown": 3,
  "logs": [
    {
      "id": "provider-stable-id",
      "timestamp": "2026-05-22T08:58:03.421Z",
      "level": "ERROR",
      "cid": "PaM9CQjf",
      "trace_id": null,
      "logger": "TradeIntentService",
      "message": "ACCOUNT_NOT_ACTIVATED"
    }
  ],
  "diagnostics": []
}
```

Watch mode should use event-oriented output. Text remains compact, while JSONL
is the integration format:

```jsonl
{"event":"started","service":"checkout-service","environment":"staging","interval_ms":5000}
{"event":"log","timestamp":"2026-05-22T09:00:03.421Z","level":"ERROR","cid":"PaM9CQjf","message":"ACCOUNT_NOT_ACTIVATED"}
{"event":"heartbeat","checked_until":"2026-05-22T09:00:10Z","new_logs":0}
```

## Provider Contract

The first logs provider should expose bounded search. Watch and wait can be
implemented by core with repeated searches before adding a streaming provider
method.

Request shape:

```text
service: string
environment: optional string
time_range:
  from: timestamp
  to: optional timestamp
  source: string
limit: integer
levels: list string
cid: optional string
trace_id: optional string
message: optional string
logger: optional string
exclude_messages: list string
exclude_loggers: list string
include_trace: boolean
cursor: optional string
```

Response shape:

```text
status: ok | partial | auth_required | unavailable | invalid_request | error
provider: string
matches: optional integer
logs: list LogEvent
next_cursor: optional string
checked_until: optional timestamp
diagnostics: list Diagnostic
```

Log event shape:

```text
id: optional string
timestamp: timestamp
level: optional string
service: optional string
environment: optional string
cid: optional string
trace_id: optional string
logger: optional string
message: string
stack_trace: optional string
source: optional string
attributes_json: optional string
```

`attributes_json` is an escape hatch for provider-specific fields. Core output
should hide it by default in text mode and include it in JSON.

Diagnostics should use stable kinds such as:

- `auth_required`
- `rate_limited`
- `query_truncated`
- `backend_timeout`
- `partial_results`
- `unsupported_filter`

## Exit Semantics

Exit behavior should distinguish "the query ran and found nothing" from "the
tool failed".

- `search` and `errors` should exit successfully when the provider query
  succeeds, even when `matches: 0`.
- `wait` should exit successfully only when a matching event appears.
- `wait` should exit non-zero on timeout, invalid filters, auth failure, or
  provider failure.
- `watch` should exit non-zero on startup failures such as invalid filters or
  auth failure.
- User interruption of `watch` should not be treated as a provider failure in
  rendered output.

The exact numeric exit codes can follow the existing CLI conventions during
implementation, but the status fields should make the outcome explicit.

## Ordering And Pagination

Ordering should match the workflow:

- `search` and `errors` should show newest logs first by default.
- `watch` and `wait` should process matching logs oldest-first within each poll
  so streamed output preserves event order.

Providers should return a cursor when their backend supports it. Without a
cursor, core should advance by timestamp with a small overlap and deduplicate
events. This avoids missing logs that arrive late because of backend ingestion
delay or timestamp precision differences.

If a bounded search is truncated by `--limit`, the provider should report that
through diagnostics such as `query_truncated` and include a cursor when possible.

## Watch And Wait

Core can implement `watch` and `wait` over bounded provider searches:

1. Resolve the initial time range.
2. Query the provider.
3. Render only events that have not been seen before.
4. Store seen event ids when available.
5. Fall back to `(timestamp, level, logger, message, cid)` identity when the
   provider has no stable id.
6. Advance the cursor or `from` timestamp using provider response metadata.
7. Sleep for `--interval`.

`watch` should emit a startup line and optional heartbeats. Heartbeats are useful
for agents because they distinguish "no matching logs yet" from "the command is
stuck".

`--since now` should mean the command start time. Implementations should still
use cursoring or overlap/deduplication internally so delayed log ingestion does
not cause missed events.

`wait` should share the same loop but exit:

- `0` when a matching event appears.
- non-zero on timeout, invalid request, auth failure, or provider failure.

Defaults:

- `watch --interval`: `5s`.
- `watch --limit`: small per-poll limit, for example `50`.
- `wait --timeout`: required or defaulted conservatively, for example `2m`.
- `wait --interval`: `2s` or `5s`.

## Auth And State

Log auth is provider-specific, but the safety rules should be core-level:

- Auth material must be user-scoped, not project-scoped.
- Secrets, cookies, and tokens must never be printed.
- State files containing auth material must be written with restrictive
  permissions.
- Plugins should receive secrets through explicit host capabilities, not by
  reading arbitrary files.
- `logs auth` should report facts such as provider, environment, destination,
  and expiry if known, without revealing secret values.
- `logs auth --check` should validate already stored auth material through the
  provider and return `status: ok` or `status: action_required`.

Some log platforms require browser-derived auth material instead of API tokens.
That should be modeled as a provider implementation detail. Conduit may need a
future host capability for browser-backed auth, but the core logs contract
should not mention a specific dashboard or cookie format. Until
`browser-auth-v1` exists, `logs auth --secret-stdin` gives providers a safe way
to accept user-provided auth material without storing it in repository files or
rendering it in output.

## Privacy And Redaction

Logs can contain production data and customer identifiers. The first
implementation should be conservative:

- Never render auth headers, cookies, tokens, or secret values.
- Do not include full provider request/response payloads in text output.
- Do not include stack traces unless `--include-trace` is set or the command is
  explicitly designed for errors.
- Keep result counts bounded by default.
- Treat `attributes_json` and provider raw fields as JSON-only data unless a
  later command explicitly asks for raw output.

Conduit should normalize and render the fields needed for debugging, not mirror
an entire log dashboard response.

## Time Zones

Conduit should render resolved timestamps in UTC by default. Relative inputs
such as `--since 30m` are resolved against the local clock, then converted to
UTC for provider requests and output.

`--date` should initially mean a UTC day. A future `--timezone` option can be
added if a provider or team workflow needs local-day semantics.

## Capability Status

Logs use the shared plugin host capability model:

- `http-client-v2`: method, URL, headers, JSON/text body, timeout, status code,
  response headers, and body.
- `secret-store-v1`: exact-name secret reads and writes through a
  host-controlled user-scoped store.

A future `browser-auth-v1` host capability can support providers that cannot
use API tokens or pasted auth material directly. Browser automation should stay
behind a narrow host capability rather than exposing arbitrary process
execution to plugins.

## Configuration

Project configuration wires the logs provider and defaults:

```toml
[plugins.company]
path = ".conduit/plugins/company.wasm"

[plugins.company.capabilities.http]
hosts = ["logs.example.com"]

[plugins.company.capabilities.secrets]
names = ["company-logs/staging/cookie"]

[defaults]
environment = "staging"

[logs]
provider = "company"
default_since = "15m"
```

User configuration can hold local auth preferences and secret references. The
project should decide which provider handles logs for that repository; the user
should decide how their local credentials are acquired and stored.

Logs commands use the nearest ancestor `.conduit/conduit.toml` that defines
`[logs]`. If no logs provider is configured, Conduit fails explicitly instead
of falling back to fixture logs. Fixture providers remain useful for tests and
explicit demos, without creating misleading results in real workspaces.

## OpenSearch Plugin Example

A company-specific logs plugin can implement the standard provider over an
OpenSearch Dashboards flow:

- Map `environment` to provider-specific dashboard hosts.
- Map time ranges and service names to indexes such as
  `log-{environment}-{service}-{date}`.
- Translate core filters into OpenSearch DSL.
- Map backend-specific logger fields such as `class` to the core `logger`
  field.
- Request only the fields needed by the core log event shape by default.
- Include stack traces only for `--include-trace` or `logs errors`.
- Refresh auth through `conduit logs auth --env ... --secret-stdin`.

This keeps company-specific hostnames, cookie names, dashboard paths, and index
rules out of core Conduit.

## Implementation Status

The core logs slice is implemented:

- normalized filters, time ranges, result models, and diagnostics;
- compact text, JSON, and JSONL rendering;
- `[logs]` provider selection and defaults;
- `logs search`, `logs errors`, `logs watch`, `logs wait`, and `logs auth`;
- auth storage and checks through exact secret grants;
- plugin HTTP support for backend search requests.

Fixture providers are kept for tests and explicit examples. Real projects
should select a logs plugin in `.conduit/conduit.toml`; Conduit should fail
clearly when no logs provider is configured.

The next high-value work is provider-specific polish outside core and, only if
real usage needs it, a narrow browser-auth capability.
