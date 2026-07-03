# Conduit CLI

Conduit is an agent-first developer operations CLI. It turns noisy engineering
systems into compact, structured primitives that humans, agents, scripts, and
IDEs can compose.

The repository is intentionally not a company-specific CLI. Company and project
behavior belongs behind explicit adapters and plugins. The core should provide
stable contracts, capability boundaries, output formats, and command semantics.

## Working Principles

- Work autonomously when the path is clear.
- Ask for clarification when requirements are ambiguous or when a foundational
  product or architecture choice is uncertain.
- Be explicit about tradeoffs and call out risks directly.
- Prefer small, stable interfaces over broad convenience workflows.
- Keep the core CLI product-neutral. Do not encode a user's personal workflow,
  company-specific process, or repository convention into core behavior.
- Make outputs deterministic, compact, and agent-friendly by default.
- Treat human-readable output as a rendering of structured facts, not as the
  source of truth.
- Keep plugins as adapters over external systems. Plugins should return typed
  data through declared contracts, not arbitrary terminal text.
- Treat credentials, auth material, and company-internal data as sensitive.
  Never store secrets in repository files or context notes.

## Product Boundaries

The core CLI may own:

- Command routing and argument parsing.
- Structured output schemas and rendering.
- Built-in primitives for common tools such as Git, GitHub, build tools, test
  result parsing, and local worktree discovery.
- Plugin discovery, loading, version negotiation, permissions, and capability
  enforcement.
- Stable provider contracts for domains such as logs, OpenAPI, service
  catalogs, docs, secrets, CI, and test reports.

Plugins may own:

- Company-specific service catalogs.
- Internal OpenAPI discovery.
- Log backend queries.
- Authentication adapters.
- Documentation sources.
- Deployment, runtime, or incident-system integrations.

Avoid adding orchestration commands that decide a full workflow for the user or
agent. Prefer factual building blocks that can be composed externally.

## Architecture Direction

- Use Rust for the core CLI.
- Use Wasmtime and the WebAssembly Component Model for plugin execution.
- Define plugin contracts with explicit interfaces and versioning.
- Keep plugins sandboxed through explicit capabilities such as HTTP, filesystem,
  secrets, and process execution. Default to least privilege.
- Keep command implementations thin. Domain behavior should live in library
  modules that can be tested without invoking the CLI process.
- Prefer schemas and typed models over parsing human output.

## Context Recovery

- Read `context/current.md` at the start of a run.
- Update `context/current.md` at the end of a run.
- Record durable decisions in `context/decisions.md` using append-only entries.
- Use `context/sessions/YYYY-MM-DD.md` only for substantial work.
- Do not store secrets, credentials, cookies, tokens, or private customer data
  in `context/`.

## Style and Structure

- Prefer simple, direct code and precise names.
- Keep comments minimal and decision-focused; do not restate code.
- Update nearby docs when behavior or architecture changes.
- Prefer ASCII punctuation in prose.
- Keep modules cohesive. Split files when a boundary is real, not preemptively.
- Keep public APIs small and intentional.

## Testing

- Tests should validate structured behavior and contracts, not incidental text.
- Snapshot tests are useful for stable command output, but schema-level tests
  should cover the underlying facts.
- If a test exposes likely incorrect behavior, flag the bug instead of
  codifying it.

## Commits and PRs

Use conventional commits:

- `feat`: user-visible behavior or public API additions.
- `fix`: bug fixes.
- `refactor`: internal restructuring without behavior changes.
- `test`: test-only changes.
- `docs`: documentation-only changes.
- `chore`: repository maintenance.

Keep commits focused on one logical change.
