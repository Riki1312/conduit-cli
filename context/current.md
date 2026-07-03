# Current Context

## Goal

- Establish Conduit as an agent-first developer operations CLI.
- Keep the core product-neutral and move company-specific integrations behind
  plugin adapters.
- Use Rust for the core CLI and Wasmtime with the WebAssembly Component Model
  for plugin execution.

## Current Direction

- Conduit should turn noisy engineering systems into compact, structured
  primitives that humans, agents, scripts, and IDEs can compose.
- The core CLI owns command semantics, structured output schemas, rendering,
  plugin loading, capability enforcement, and provider contracts.
- Plugins own company-specific data sources, auth adapters, backend query
  languages, service catalogs, and documentation systems.
- Default output should be compact, deterministic, and agent-friendly.
- Machine-readable output should be available through JSON or JSONL where that
  matches the command shape.
- Credentials, cookies, tokens, customer data, and private operational details
  must not be stored in repository files or context notes.

## Implemented Core

- The Rust workspace contains the `conduit-cli` crate and `conduit` binary.
- The public library surface intentionally exposes only the CLI runner.
- The CLI supports compact text output by default and JSON output for bounded
  commands.
- `about`, `help`, `git status`, `worktree list`, `stats`, `openapi`, `logs`,
  `db`, `plugin check`, and `test` command families exist.
- `.conduit/conduit.toml` config is discovered from the current directory and
  then ancestor directories.
- User-level stats are stored under `$CONDUIT_STATS_DIR`,
  `$XDG_STATE_HOME/conduit`, or `~/.local/state/conduit`.
- Project-local state remains under `.conduit/state` by default for captured
  logs and last-test metadata.

## Test Runner

- `test run gradle` wraps `./gradlew`, captures stdout/stderr to a log file,
  parses JUnit-style XML reports, and prints compact structured summaries.
- Gradle profiles can define reusable defaults under
  `[test.gradle.profiles.<name>]`, including `task`, `report_path`, `mode`,
  `args`, and `env`.
- `--tests`, `--failed`, `--task`, `--report-path`, `--mode`, `--tail`,
  `--timeout`, and `--heartbeat` are supported.
- Test summaries distinguish `termination`, `test_outcome`, `report_status`,
  `tests_ran`, `tests_passed`, failures, passed selectors, diagnostics, and log
  path.
- `test failed`, `test last`, `test log`, `test failures`, and
  `test rerun gradle` provide follow-up inspection and rerun workflows.
- `docs/test-runner-ux-design.md` captures the design direction for runner
  visibility, timeouts, no-source tasks, environment defaults, and preserving
  rerunnable failure state.

## Plugin Runtime

- Plugin contracts are defined in WIT under `wit/conduit-plugin`.
- Wasmtime component bindings are compile-validated through
  `wasmtime::component::bindgen!`.
- Plugins are instantiated with explicit capabilities for file reads, HTTP, and
  exact user-scoped secret names.
- Plugin metadata must report protocol version `1` and the expected provider
  interface.
- `plugin check` validates provider metadata by path or from configured
  providers and warms the Wasmtime compilation cache.
- Wasmtime uses Cranelift, copy-on-write memory initialization,
  signals-based traps, and a project-local compilation cache under
  `.conduit/state/wasmtime-cache`.

## OpenAPI

- `openapi operation`, `openapi list`, and `openapi search` expose normalized
  API operation facts.
- Without project config, OpenAPI commands use a built-in fixture provider for
  `catalog-service`.
- With project config, OpenAPI commands use the provider selected by
  `[openapi].provider`.
- OpenAPI fixture examples are intentionally public-safe and do not require
  private services.

## Logs

- `logs search`, `logs errors`, `logs wait`, `logs watch`, and `logs auth`
  exist behind a product-neutral logs provider contract.
- Core owns normalized filters, resolved time ranges, count-only output,
  watch/wait loops, deduplication, text/JSON/JSONL rendering, and auth command
  semantics.
- Providers own backend query languages, index naming, dashboard details, and
  auth implementation.
- Logs filters include environment, time range, level, correlation id, trace
  id, message, logger/class, negative message/logger filters, limit, and stack
  trace inclusion.
- `logs auth --secret-stdin` lets providers accept externally acquired auth
  material without Conduit rendering it.
- `logs auth --check` validates already stored provider auth material.
- `docs/logs-provider-design.md` captures the provider contract direction and
  safety rules.

## DB Provider Direction

- `docs/db-provider-design.md` captures the proposed constrained operational
  data interface.
- The command shape should be `db resources`, `db describe`, `db read`,
  and later `db insert` and `db update`.
- Fixture-backed `db resources`, `db describe`, and `db read` are implemented.
- `read` is the single read command; id lookups are modeled as exact reads.
- The first implementation should be read-only: resources, describe, and read.
- The first contract should avoid raw SQL, production access, delete, insert,
  update, bulk update, schema changes, dry-run flows, and reason prompts.
- Provider plugins should own auth, database routing, backend queries, resource
  mapping, and write policy.

## Public Repository

- The public repository is a sanitized snapshot with product-neutral docs,
  public-safe fixtures, and no company-specific plugin sources.
- Company-specific adapters should live outside this repository and connect to
  Conduit through the documented plugin contracts.
- Keep future docs, examples, tests, and context notes free of private service
  names, hostnames, customer data, personal paths, credentials, and workflow
  details that only apply to one company.

## Near-Term Todo

- Keep README and design docs concise enough for new users to understand the
  product without reading the full design archive.
- Add small, public-safe plugin examples when the build flow is stable enough
  to maintain.
- Add release packaging later, likely after internal dogfooding has settled the
  command surface.
