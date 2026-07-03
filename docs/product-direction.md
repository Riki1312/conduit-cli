# Product Direction

Conduit is an agent-first developer operations CLI.

It exists because modern engineering work depends on many systems that do not
share a coherent command-line interface: test runners, build tools, GitHub,
logs, OpenAPI documentation, service catalogs, internal docs, auth flows,
deployment systems, incident tools, and company-specific utilities.

Some systems have excellent CLIs. `gh` is the clearest example. Many internal
systems do not. Agents and humans then compensate by scraping noisy output,
manually opening dashboards, copying logs, and remembering local conventions.
That wastes attention and context.

Conduit should provide the missing interface layer: stable, compact,
structured engineering primitives over company and project systems.

## Product Thesis

Companies need a standard way to expose basic engineering operations to humans,
agents, scripts, and IDEs without forcing every tool to understand every
internal system.

Conduit should be that bridge.

The core CLI should define common contracts and command semantics. Companies
and projects should customize those contracts with plugins that connect to the
systems they already use.

## Non-Goals

- Conduit is not a company-specific CLI.
- Conduit is not an AI-only CLI.
- Conduit is not a replacement for Git, GitHub CLI, Gradle, Cargo, OpenSearch,
  service catalogs, or internal documentation systems.
- Conduit should not encode personal workflow preferences.
- Conduit should not become an unstructured bucket of company scripts.

## Principles

### Agent-First Output

The default output should be concise, deterministic, and easy for agents to
consume. Commands should expose strict machine-readable output as well, but the
normal command experience should already avoid noisy logs and incidental
progress text.

Commands should return facts, not stories. For example, a test command should
summarize failed tests, assertion messages, report paths, and rerun commands
instead of streaming full build output by default.

### Structured Facts Over Text Scraping

The CLI should parse noisy tools once and expose stable facts. Agents and users
should not need to repeatedly parse Gradle logs, CI logs, OpenSearch pages, or
internal documentation HTML.

### Core Contracts, Plugin Adapters

The core should own command semantics, output schemas, rendering, permissions,
plugin loading, and stable provider contracts.

Plugins should own company-specific data sources and integration details.

This keeps the standard portable while still allowing deep customization.

### Plugins Return Data, Not Terminal Output

Plugins should not behave like mini CLIs that print arbitrary text. They should
return typed data through declared interfaces. The Conduit core should validate,
format, and render that data consistently.

### Capabilities Are Explicit

Plugin access to files, network, secrets, environment variables, and subprocesses
must be explicit. Default to least privilege.

### Building Blocks, Not Workflow Ownership

Conduit should provide reusable building blocks:

```text
conduit test failed
conduit logs search
conduit openapi get
conduit worktree list
conduit ci failures
```

It should avoid commands that decide an entire project or team workflow.
Workflow orchestration can live in shell scripts, agent instructions, IDE
actions, CI jobs, or project-local wrappers.

## Initial Domains

### Tests

Normalize test execution and failure reporting across common build tools.

Near-term Java/Gradle value:

- Run focused tests.
- Parse JUnit XML results.
- Store and rerun the previous failed selectors.
- Print compact failures and exact rerun commands.
- Preserve full logs separately when needed.

The long-term contract should be language-neutral.

### Git, GitHub, And Worktrees

Expose concise repository state:

- Current branch and upstream.
- Dirty files.
- Ahead/behind counts.
- PR metadata.
- CI failure summaries.
- Worktree discovery and lookup.

This should build on Git and `gh`, not replace them.

### OpenAPI

Expose service API documentation through a common contract:

- List services.
- List operations.
- Fetch an operation by method/path.
- Return request and response schemas.
- Include source metadata.

Companies can implement providers backed by internal documentation portals,
service catalogs, repositories, or generated artifacts.

### Logs

Expose logs through a common search contract:

- Search by correlation id, trace id, text, class, severity, time range, and
  service.
- Return compact log events with structured fields.
- Keep auth and backend details inside provider plugins.

### Service Catalog And Docs

Expose service ownership, repositories, docs, dashboards, environments, and
runbooks through typed provider contracts.

## Architecture

Use Rust for the core CLI.

Use Wasmtime and the WebAssembly Component Model for plugins. Define plugin
interfaces with explicit contracts and versioning.

The first architecture should be:

```text
conduit-cli
  command routing
  structured output
  config loading
  plugin discovery
  plugin runtime
  capability enforcement
  built-in common-tool providers

plugins
  OpenAPI providers
  log providers
  service catalog providers
  docs providers
  auth/secret adapters
  custom command adapters
```

Prefer provider interfaces over arbitrary custom commands. Custom commands are
an escape hatch, not the main extension model.

## Configuration

Projects should be able to configure plugins and defaults locally:

```toml
[plugins.company]
path = ".conduit/plugins/company.wasm"

[openapi]
provider = "company"

[logs]
provider = "company"
default_environment = "staging"
```

Plugin artifacts may embed metadata for distribution and compatibility checks,
but project configuration should decide how providers are wired.

## MVP Direction

The first useful slice should prove the core value without building the whole
platform:

1. Rust CLI workspace and command skeleton.
2. Compact test failure parsing from JUnit XML.
3. `test failed` state persistence and rerun support.
4. Worktree/repository status summary.
5. Plugin runtime spike with one narrow provider contract.

After that, move toward OpenAPI and logs providers.
