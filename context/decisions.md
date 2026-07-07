# Decisions Log

Append only. Record durable decisions that affect product direction,
architecture, behavior, or workflow.

## Entry Template

### YYYY-MM-DD HH:MM - Short Title

- `decision`: What was decided.
- `rationale`: Why this is the best choice now.
- `impact`: What changes because of this decision.
- `evidence`: Related file paths, commands, PRs, or commits.

### 2026-04-28 00:00 - Core Runtime Plus Plugin Adapters

- `decision`: Conduit should be architected from the beginning as a core runtime
  with stable contracts and plugin adapters, not as a monolithic CLI that gains
  plugins later.
- `rationale`: The product goal is to provide a reusable bridge between company
  engineering systems and agents/tooling. Company-specific behavior must stay
  outside the core so the CLI remains broadly useful.
- `impact`: Initial design work should define core responsibilities, provider
  contracts, plugin loading, and capability boundaries before adding
  company-specific integrations.
- `evidence`: `docs/product-direction.md`; `AGENTS.md`.

### 2026-04-29 00:00 - OpenAPI Provider Before Wasmtime Runtime

- `decision`: Start the plugin system with a written direction and an
  in-process OpenAPI provider command/model before implementing Wasmtime plugin
  loading.
- `rationale`: The CLI now has enough real command and output conventions to
  design provider contracts, but the user-facing OpenAPI request/response model
  should be proven before locking the component boundary.
- `impact`: The next implementation should add `conduit openapi operation` and
  `conduit openapi list` against a fixture provider, then extract a provider
  trait and WIT contract, and only then add Wasmtime runtime support.
- `evidence`: `docs/plugin-system-direction.md`; `docs/product-direction.md`.

### 2026-04-28 00:00 - Rust And Wasmtime

- `decision`: Use Rust for the core CLI and Wasmtime with the WebAssembly
  Component Model for plugin execution.
- `rationale`: Rust is a strong fit for infrastructure tooling and single
  binary distribution. Wasmtime and the Component Model provide a portable,
  language-neutral, sandboxed plugin boundary with explicit interfaces.
- `impact`: The repository should be structured around typed contracts,
  capability enforcement, and component-based plugin execution from the start.
- `evidence`: `docs/product-direction.md`; `AGENTS.md`.

### 2026-04-29 00:00 - Pin Rust For Wasmtime 44

- `decision`: Pin the repository to Rust `1.94.0` while using Wasmtime
  `44.0.0`.
- `rationale`: Wasmtime `44.0.0` and its Cranelift runtime dependencies require
  Rust `1.92.0` or newer. The local default toolchain was `1.87.0-nightly`,
  which could update the lockfile but could not compile the runtime.
- `impact`: Contributors and agents should use the pinned toolchain through
  `rust-toolchain.toml`; CI should install a compatible Rust version before
  running checks.
- `evidence`: `rust-toolchain.toml`; `crates/conduit-cli/Cargo.toml`;
  `cargo +1.94.0 test --workspace`.

### 2026-05-14 00:00 - Company OpenAPI Defaults Belong In Plugins

- `decision`: Provider plugins may derive company-standard OpenAPI URLs or
  service aliases internally, while Conduit core should only see the normalized
  provider request and response.
- `rationale`: URL patterns, service aliases, and documentation portals are
  company-specific. Keeping those defaults in plugins preserves core
  portability while reducing setup noise for teams.
- `impact`: OpenAPI provider plugins can offer sensible company defaults
  without expanding the Conduit core configuration model.
- `evidence`: `docs/plugin-system-direction.md`; `wit/conduit-plugin`.

### 2026-05-19 00:00 - User-Level Usage Stats

- `decision`: Conduit usage statistics are stored at user level by default
  instead of inside each project `.conduit/state` directory.
- `rationale`: The value of stats is cumulative across the user's work: total
  commands, test runs, and Gradle verbosity reduction. Project-local stats make
  that overview fragmented and undercount real adoption.
- `impact`: `conduit stats` now reports `scope: user` and reads/writes
  `$CONDUIT_STATS_DIR/stats.json`, `$XDG_STATE_HOME/conduit/stats.json`, or
  `~/.local/state/conduit/stats.json`. Project-local state remains for logs,
  last test runs, and rerunnable failures.
- `evidence`: `crates/conduit-cli/src/stats.rs`; `README.md`.

### 2026-05-19 00:00 - Ancestor Config Discovery

- `decision`: Conduit discovers `.conduit/conduit.toml` in the current
  directory first, then walks ancestors.
- `rationale`: Worktrees often need the same high-level profiles and provider
  configuration as their workspace without copying local `.conduit` files into
  every worktree.
- `impact`: Workspace-level configs can define reusable Gradle profiles, while
  project-local configs still override them when present.
- `evidence`: `crates/conduit-cli/src/config.rs`;
  `crates/conduit-cli/tests/cli.rs`.

### 2026-05-22 00:00 - Logs Provider Contract Direction

- `decision`: Logs should be implemented as a core-owned provider contract with
  normalized filters, resolved time ranges, bounded search, watch/wait loops,
  JSON/JSONL integration output, and provider-owned backend/auth adapters.
- `rationale`: Logs are a high-value debugging primitive for both humans and
  agents, but backend query languages, index naming, cookies, tokens, and login
  flows are company-specific. Keeping the contract in core and the backend
  details in plugins preserves portability while improving developer experience.
- `impact`: Logs work should start from `docs/logs-provider-design.md`, extend
  plugin capabilities for richer HTTP and user-scoped state, and prove the core
  UX with a fixture provider before adding backend-specific adapters outside
  core.
- `evidence`: `docs/logs-provider-design.md`; `wit/conduit-plugin`.

### 2026-06-16 00:00 - Constrained DB Provider Direction

- `decision`: DB access should be modeled as a constrained operational data
  provider that starts with `resources`, `describe`, and `read`, not raw SQL.
- `rationale`: Humans and agents need a safe way to inspect and adjust
  test/staging data, but generic database clients expose too much destructive
  surface. A resource-oriented contract keeps Conduit product-neutral while
  allowing plugins to handle auth, routing, schemas, and backend-specific
  queries.
- `impact`: Initial DB work should avoid production, insert, update, delete,
  bulk update, schema changes, dry-run flows, and reason prompts. Reads should
  be bounded. Insert and update can be added later after a PostgreSQL-backed
  example plugin validates the provider contract.
- `evidence`: `docs/db-provider-design.md`.

### 2026-07-03 19:59 - Static GitHub Pages Website

- `decision`: Host the Conduit project website as a static site under `site/`
  and deploy it with GitHub Pages through a Pages workflow.
- `rationale`: The project needs a simple public-facing site, not a framework
  app or generated documentation pipeline. A static site keeps maintenance low
  and matches the compact, infrastructure-focused product style.
- `impact`: Website changes should usually touch `site/` and
  `.github/workflows/pages.yml`; repository settings must use GitHub Actions as
  the Pages source.
- `evidence`: `site/index.html`; `site/styles.css`;
  `.github/workflows/pages.yml`.

### 2026-07-07 00:00 - Release Assets Feed Homebrew Tap

- `decision`: Conduit releases should publish checksummed platform archives
  and a generated Homebrew formula asset from this repository; the Homebrew tap
  should live outside the core repo and copy the generated formula for each
  release.
- `rationale`: The public repository should remain product-neutral and own
  source, release artifacts, and reusable agent guidance. A separate tap keeps
  Homebrew distribution conventional without mixing tap-only history into the
  core project.
- `impact`: Tag pushes run `.github/workflows/release.yml`; maintainers update
  the external tap from the release's generated `conduit.rb`. Agent guidance is
  distributed as a lean skill under `skills/conduit/SKILL.md`.
- `evidence`: `.github/workflows/release.yml`; `README.md`;
  `packaging/homebrew/conduit.rb.template`; `skills/conduit/SKILL.md`.
