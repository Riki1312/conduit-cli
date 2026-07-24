# Conduit CLI

[![skills.sh](https://skills.sh/b/Riki1312/conduit-cli)](https://skills.sh/Riki1312/conduit-cli)

Conduit turns noisy developer tools into compact, structured facts for humans,
agents, scripts, and IDEs.

Modern engineering work crosses test runners, build tools, logs, OpenAPI docs,
Git worktrees, service catalogs, and internal dashboards. Conduit sits above
those systems as a small, product-neutral command layer:

- reduce noisy output to stable summaries;
- preserve full logs and state when deeper inspection is needed;
- emit deterministic text, JSON, and JSONL for agents and scripts;
- keep company-specific systems behind explicit WebAssembly plugins;
- avoid baking one person's workflow into the core CLI.

Project and company behavior belongs in `.conduit/conduit.toml` and plugins.

## Install

Homebrew is the recommended install path:

```bash
brew install Riki1312/tap/conduit
conduit about
```

For local development, install from source with the pinned Rust toolchain:

```bash
cargo install --path crates/conduit-cli --locked
conduit about
```

Tagged releases are built by `cargo-dist`, which publishes platform archives,
checksums, release notes, and the Homebrew formula.

Use `--help` on any command to inspect flags:

```bash
conduit test run gradle --help
```

Agents can install the bundled Conduit skill through the open skills CLI:

```bash
npx skills add https://github.com/Riki1312/conduit-cli/tree/main/skills/conduit
```

Use the `skills` CLI's `--agent` and `--global` options when you want to
target a specific agent or install scope.

## Core Workflows

### Tests

`conduit test run gradle` wraps `./gradlew`, captures raw output to
`.conduit/state`, parses JUnit XML, and prints a compact summary.

```bash
conduit test run gradle --tests SomeTest
conduit test run gradle --failed
conduit test run gradle --profile integration --tests '*SdkTest'
conduit test run gradle --task :service:test --tests SomeTest --tail 20
conduit test rerun gradle
conduit test failed --tail 20
conduit test log --tail 80
```

Example output:

```text
runner: gradle
mode: unit
termination: exit
test_outcome: executed
exit_code: 0
log_path: .conduit/state/logs/test-run-...
report_status: fresh
status: passed
tests_ran: 1
tests_passed: 1
failures: 0
passed: com.example.SomeTest.passes
```

Reusable Gradle profiles live in project config:

```toml
[test.gradle.profiles.integration]
task = "test"
report_path = "build/test-results/test"
mode = "integration"
args = ["-Dexample.integration=true"]
```

### Logs

Logs commands provide normalized search, error-focused views, JSON output, and
watch/wait loops. They require a configured logs provider.

```bash
conduit logs search checkout-service --since 15m --level ERROR
conduit logs search checkout-service --message 'known text' --limit 0
conduit logs search checkout-service --grep 'stack trace text' --include-trace
conduit logs errors checkout-service --since 1h --limit 20
conduit logs wait checkout-service --since now --message 'job completed'
conduit logs watch checkout-service --level ERROR --since now --jsonl
conduit logs auth --check
```

Useful filters include `--env`, `--since`, `--from`, `--to`, `--date`,
`--level`, `--cid`, `--trace-id`, `--message`, `--grep`, `--logger`,
`--class`, `--exclude-message`, `--exclude-grep`, `--exclude-logger`,
`--exclude-class`, `--limit`, and `--include-trace`. Use `--message` for
message-field filters and `--grep` for broad text search across message,
stack trace, and logger-like fields.

### OpenAPI

OpenAPI commands expose normalized operation facts from a configured provider.

```bash
conduit openapi operation --service catalog-service --method GET --path /items
conduit openapi search --service catalog-service --query item_id --method GET
conduit openapi list --service catalog-service --json
```

### DB

DB commands expose constrained operational data access through service-owned
resources. The first implementation is read-only: no raw SQL, writes, delete,
bulk update, or schema changes.

```bash
conduit db resources checkout-service --env test
conduit db describe checkout-service payment_account --env test
conduit db read checkout-service payment_account --id acc_123 --env test
conduit db read checkout-service payment_account --filter status=ACTIVE --limit 20
```

### Git, Worktrees, And Stats

```bash
conduit git status
conduit git status --path ../some-repo --json
conduit worktree list --root /path/to/worktrees
conduit stats
conduit stats --json
```

Stats are user-scoped, silent, and best-effort. A failed stats write never
changes command output.

## Project Config

Conduit discovers `.conduit/conduit.toml` in the current directory, then walks
ancestors. A workspace config can therefore serve nested worktrees.
Provider-backed commands such as logs, OpenAPI, and DB use the provider selected
by project config. If no provider is selected, they fail with a compact setup
error instead of guessing a backend.

```toml
[defaults]
environment = "staging"

[plugins.company-openapi]
path = ".conduit/plugins/company-openapi.wasm"

[plugins.company-openapi.capabilities.http]
hosts = ["docs.example.com"]

[plugins.company-openapi.capabilities.file-read]
paths = [".conduit/company-openapi"]

[openapi]
provider = "company-openapi"

[plugins.company-logs]
path = ".conduit/plugins/company-logs.wasm"

[plugins.company-logs.capabilities.http]
hosts = ["logs.example.com"]

[plugins.company-logs.capabilities.secrets]
names = ["company-logs/staging/token"]

[logs]
provider = "company-logs"
default_since = "15m"

[plugins.company-db]
path = ".conduit/plugins/company-db.wasm"

[plugins.company-db.capabilities.postgres]
connections = [
  { name = "checkout-test", host = "test-db.example.com", database = "postgres", ssl_mode = "require", ssl_root_cert = ".conduit/certs/rds.pem" },
]

[plugins.company-db.capabilities.file-read]
paths = [".conduit/company-db"]

[plugins.company-db.capabilities.secrets]
names = [
  "company-db/checkout/test/username",
  "company-db/checkout/test/password",
]

[db]
provider = "company-db"
```

`file-read.paths`, `http.hosts`, and `secrets.names` are shared host
capabilities. Grant only the ones a plugin uses; for example, a DB adapter may
need `file-read.paths` when it reads a service manifest.

Secrets are exact user-scoped grants. Conduit stores plugin secrets under
`$CONDUIT_SECRET_DIR`, `$XDG_STATE_HOME/conduit/secrets`, or
`~/.local/state/conduit/secrets`; repository files should not contain cookies,
tokens, usernames, or passwords.

## Plugins

Plugins are WebAssembly components that implement typed provider contracts.
They return structured data to Conduit; Conduit owns rendering, validation,
output shape, and capability enforcement.

```bash
conduit plugin check --path .conduit/plugins/company-openapi.wasm
conduit plugin check --provider openapi
conduit plugin check --path .conduit/plugins/company-logs.wasm --provider logs
conduit plugin check --provider logs
conduit plugin check --path .conduit/plugins/company-db.wasm --provider db
conduit plugin check --provider db
```

See [Building Plugins](docs/plugin-build-guide.md) for contract details and
implementation guidance.

## Development

The repository pins Rust `1.94.0`.

```bash
cargo fmt --all
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Run without installing:

```bash
cargo run -p conduit-cli -- about
```

Useful project docs:

- [Building Plugins](docs/plugin-build-guide.md)
- [Agent Skill](skills/conduit/SKILL.md)
- [Contributing](CONTRIBUTING.md)
- [Security](SECURITY.md)

Tag pushes run `.github/workflows/release.yml`, generated from
`dist-workspace.toml` by `cargo-dist`. The release workflow publishes archives,
checksum files, GitHub release notes, and `Formula/conduit.rb` in
`Riki1312/homebrew-tap`.

The release workflow requires a `HOMEBREW_TAP_TOKEN` repository secret with
permission to push to `Riki1312/homebrew-tap`.

When release settings change, run:

```bash
dist generate --mode ci
dist generate --mode ci --check
```
