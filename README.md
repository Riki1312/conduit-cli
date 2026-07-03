# Conduit CLI

Conduit turns noisy developer tools into compact, structured facts for humans,
agents, scripts, and IDEs.

Modern engineering work crosses test runners, build tools, logs, OpenAPI docs,
Git worktrees, service catalogs, and company dashboards. Each system has its
own output shape and failure modes. Conduit sits above them as a small command
layer that:

- reduces noisy output to stable summaries;
- preserves full logs and state when they are needed;
- gives agents deterministic text, JSON, and JSONL to consume;
- keeps company-specific systems behind explicit plugins;
- avoids encoding one person's workflow into the core CLI.

The core is product-neutral. Project and company behavior belongs in
configuration and WebAssembly Component Model plugins.

## Install

The repository pins Rust `1.94.0`. Install the local binary from a checkout:

```bash
cargo install --path crates/conduit-cli
conduit about
```

Run without installing:

```bash
cargo run -p conduit-cli -- about
```

Use `--help` on any command to inspect flags:

```bash
conduit test run gradle --help
```

## Start With

These commands show the intended shape of Conduit output:

```bash
conduit test run gradle --tests SomeTest
conduit test failed
conduit logs search fixture-service --date 2026-05-22 --limit 1
conduit openapi operation --service catalog-service --method GET --path /items
conduit db read checkout-service payment_account --id acc_123
conduit git status
conduit worktree list --root ..
```

For real project integrations, configure providers and profiles in
`.conduit/conduit.toml`. Without project config, the OpenAPI and logs commands
use public-safe fixture providers so the CLI can be tried immediately.

## Tests

`conduit test run gradle` wraps `./gradlew`, captures stdout and stderr to a log
file, parses JUnit-style XML reports, and prints a compact summary.

Common commands:

```bash
conduit test run gradle --tests SomeTest
conduit test run gradle --failed
conduit test run gradle --profile integration --tests '*SdkTest'
conduit test run gradle --mode integration --task integrationTest
conduit test run gradle --timeout 2m --tests SomeTest
conduit test run gradle --heartbeat 30s --tests SomeTest
conduit test run gradle -- -Penvironment=staging
conduit test run gradle --task :service:test --tests SomeTest
conduit test run gradle \
  --task :service:test \
  --report-path service/build/test-results/test \
  --tests SomeTest
conduit test run gradle --tests SomeTest --tail 20
```

Output fields are intentionally stable and easy to scan:

```text
runner: gradle
profile: none
mode: unit
termination: exit
test_outcome: executed
command: ./gradlew test --tests SomeTest
exit_code: 0
log_path: .conduit/state/logs/test-run-...
report_status: fresh
status: passed
tests_ran: 1
tests_passed: 1
failures: 0
sources: 1
passed_selectors: 1
passed: com.example.SomeTest.passes
```

Useful follow-up commands:

```bash
conduit test last
conduit test failed
conduit test failed --tail 20
conduit test rerun gradle
conduit test log --tail 80
conduit test log --path .conduit/state/logs/test-run-123.log --json
conduit test failures build/test-results/test
conduit test failures build/test-results/test --json
```

State files are written under `.conduit/state` by default. Set
`CONDUIT_STATE_DIR` to override this location.

Configure reusable Gradle profiles in `.conduit/conduit.toml`. Conduit
discovers config in the current directory first, then walks ancestors, so
workspace-level profiles can be shared by nested worktrees.

```toml
[test.gradle.profiles.integration]
task = "test"
report_path = "build/test-results/test"
mode = "integration"
args = ["-Dexample.integration=true"]

[test.gradle.profiles.integration.env]
JAVA_HOME = "/path/to/jdk"
```

Profile values provide defaults. Command-line flags and passthrough Gradle
arguments can still refine a specific run.

## Logs

Logs commands provide normalized service log search, compact text rendering,
JSON output, JSONL watch output, and provider-neutral wait/watch loops.

```bash
conduit logs search fixture-service --cid CID-123 --date 2026-05-22
conduit logs search fixture-service --message ACCOUNT_NOT_ACTIVATED --date 2026-05-22
conduit logs search fixture-service --exclude-message 'known noise' --limit 0
conduit logs errors fixture-service --date 2026-05-22
conduit logs wait fixture-service --cid CID-123 --timeout 2m
conduit logs watch fixture-service --level ERROR --since now --jsonl
conduit logs auth --env staging
conduit logs auth --env staging --check
conduit logs search fixture-service --json
```

Useful filters include `--env`, `--since`, `--from`, `--to`, `--date`,
`--level`, `--cid`, `--trace-id`, `--message`, `--logger`, `--class`,
`--exclude-message`, `--exclude-logger`, `--exclude-class`, `--limit`, and
`--include-trace`.

`logs errors` is a convenience command for error-level logs and includes stack
traces by default. `logs wait` exits successfully when a matching log appears
and exits non-zero on timeout. `logs watch` keeps polling until interrupted, or
until an optional `--timeout` is reached.

Configure logs defaults and plugin capabilities in `.conduit/conduit.toml`:

```toml
[plugins.company-logs]
path = ".conduit/plugins/company-logs.wasm"

[plugins.company-logs.capabilities.http]
hosts = ["logs.example.com"]

[plugins.company-logs.capabilities.secrets]
names = ["company-logs/staging/token"]

[logs]
provider = "company-logs"
default_environment = "staging"
default_since = "15m"
```

Secret capabilities grant exact user-scoped secret names. Conduit stores plugin
secrets under `$CONDUIT_SECRET_DIR`, `$XDG_STATE_HOME/conduit/secrets`, or
`~/.local/state/conduit/secrets`; repository files should not contain cookies
or tokens.

## OpenAPI

OpenAPI commands expose normalized API operation facts from a configured
provider. The built-in fixture provider works without project config.

```bash
conduit openapi operation --service catalog-service --method GET --path /items
conduit openapi search --service catalog-service --query item_id --method GET
conduit openapi list --service catalog-service --json
```

Configure an OpenAPI provider plugin:

```toml
[plugins.company-openapi]
path = ".conduit/plugins/company-openapi.wasm"

[plugins.company-openapi.capabilities.http]
hosts = ["docs.example.com"]

[plugins.company-openapi.capabilities.file-read]
paths = [".conduit/company-openapi"]

[openapi]
provider = "company-openapi"
```

## DB

DB commands expose constrained operational data access through service-owned
resources. The first slice is read-only and uses a built-in fixture provider
unless a project config selects a DB plugin.

```bash
conduit db resources checkout-service --env test
conduit db describe checkout-service payment_account --env test
conduit db read checkout-service payment_account --id acc_123 --env test
conduit db read checkout-service payment_account --filter status=ACTIVE --limit 20
conduit db read checkout-service payment_account --id acc_123 --json
```

The provider contract intentionally avoids raw SQL, delete, bulk update,
writes, and schema changes in the first implementation. DB plugins can use
exact secret grants and named PostgreSQL connections configured by the project:

```toml
[plugins.company-db]
path = ".conduit/plugins/company-db.wasm"

[plugins.company-db.capabilities.postgres]
connections = [
  { name = "checkout-test", host = "test-db.example.com", database = "postgres", ssl_mode = "require", ssl_root_cert = ".conduit/certs/rds.pem" },
]

[plugins.company-db.capabilities.secrets]
names = [
  "company-db/checkout/test/username",
  "company-db/checkout/test/password",
]

[db]
provider = "company-db"
default_environment = "test"
```

The PostgreSQL host capability is read-only and exact-connection based; plugins
do not receive arbitrary socket or process access. `ssl_mode` defaults to
`disable`; use `require` with `ssl_root_cert` when the database requires TLS
with a project-pinned CA bundle.

## Git And Worktrees

```bash
conduit git status
conduit git status --path ../some-repo --json
conduit worktree list --root /path/to/worktrees
```

## Stats

`conduit stats` shows user-level adoption and noise-reduction counters. Stats
updates are silent and best-effort; a failed stats write never changes command
output.

```bash
conduit stats
conduit stats --json
```

Usage stats are written under `$XDG_STATE_HOME/conduit` or
`~/.local/state/conduit`; set `CONDUIT_STATS_DIR` to override the stats
location.

## Plugins

Plugins are WebAssembly components that implement typed provider contracts.
They return data to Conduit; Conduit owns rendering, validation, output shape,
and capability enforcement.

Validate and warm a plugin before regular commands use it:

```bash
conduit plugin check --path .conduit/plugins/company-openapi.wasm
conduit plugin check --provider openapi
conduit plugin check --path .conduit/plugins/company-logs.wasm --provider logs
conduit plugin check --provider logs
conduit plugin check --path .conduit/plugins/company-db.wasm --provider db
conduit plugin check --provider db
conduit plugin check --path .conduit/plugins/company-openapi.wasm --json
```

See [Building Plugins](docs/plugin-build-guide.md) for the contract shape,
configuration examples, and implementation guidance.

## Development

```bash
cargo fmt --all
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Source layout:

- `crates/conduit-cli/src/app.rs`: command parsing and dispatch.
- `crates/conduit-cli/src/logs.rs`: logs query model, fixture provider,
  watch/wait loop, and rendering.
- `crates/conduit-cli/src/test_run.rs`: Gradle runner integration.
- `crates/conduit-cli/src/plugin_runtime.rs`: Wasmtime component runtime.
- `wit/`: plugin contracts.

Design and project docs:

- [Product direction](docs/product-direction.md)
- [Plugin system direction](docs/plugin-system-direction.md)
- [Building plugins](docs/plugin-build-guide.md)
- [Logs provider design](docs/logs-provider-design.md)
- [DB provider design](docs/db-provider-design.md)
- [Test runner UX design](docs/test-runner-ux-design.md)
- [Agent guidance](AGENTS.md)
- [Context system](context/README.md)
- [Contributing](CONTRIBUTING.md)
- [Security](SECURITY.md)
