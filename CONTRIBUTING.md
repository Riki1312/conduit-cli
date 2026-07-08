# Contributing

Conduit is an agent-first developer operations CLI. The core should stay
product-neutral; company-specific behavior belongs in plugins or external
adapters.

## Development Setup

Use the pinned Rust toolchain from `rust-toolchain.toml`.

Run the main checks before opening a change:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo doc --workspace --no-deps
git diff --check
```

When changing release settings in `dist-workspace.toml`, regenerate and check
the cargo-dist workflow:

```bash
dist generate --mode ci
dist generate --mode ci --check
```

## Design Rules

- Keep command output compact, deterministic, and structured.
- Prefer provider contracts and typed models over parsing human output.
- Keep plugins as adapters over external systems; plugins should return typed
  data, not arbitrary terminal text.
- Do not add company-specific service names, hostnames, credentials, auth
  material, or workflow policy to the public core.
- Update nearby docs when behavior, architecture, or public command output
  changes.

## Commits

Use conventional commit prefixes where practical:

- `feat`: user-visible behavior or public API additions.
- `fix`: bug fixes.
- `refactor`: internal restructuring without behavior changes.
- `test`: test-only changes.
- `docs`: documentation-only changes.
- `chore`: repository maintenance.
