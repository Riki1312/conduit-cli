# Python Plugin Kit Direction

Conduit should provide a small Python authoring kit for plugin developers.
The kit should make provider code feel like normal Python while keeping the
Conduit WIT contracts visible and stable.

The working package name is `conduit-kit`, with Python imports under
`conduit_kit`.

## Goals

- Remove repeated WIT and `componentize-py` glue from Python plugins.
- Keep plugin authors focused on provider logic and tests.
- Preserve Conduit's typed provider contracts and core-owned rendering model.
- Make capability use explicit and easy to fake in tests.
- Keep public APIs small, symmetrical, and documented where the contract is not
  obvious.
- Dogfood the kit by migrating the Satispay plugins before considering it
  stable.

## Non-Goals

- Do not create a generic plugin framework.
- Do not hide provider contract semantics behind broad abstractions.
- Do not let plugins define CLI output or custom terminal UX.
- Do not add runtime magic, global mutable state, or implicit configuration.
- Do not support Rust, Go, or other languages until the Python API shape has
  proved useful.
- Do not publish a package before at least one real plugin migration has
  improved clarity.

## Package Shape

Start inside this repository:

```text
packages/
  conduit-kit-python/
    pyproject.toml
    README.md
    src/conduit_kit/
      __init__.py
      errors.py
      metadata.py
      capabilities/
        files.py
        http.py
        postgres.py
        secrets.py
      providers/
        logs.py
        openapi.py
        db.py
      testing/
        files.py
        http.py
        postgres.py
        secrets.py
```

This is a direction, not a required first commit. Add modules only when a
plugin migration needs them.

## API Principles

- Prefer plain dataclasses and functions over decorators or inheritance.
- Keep generated WIT bindings at the edge of the plugin.
- Make provider request and response types mirror Conduit concepts.
- Keep capability clients thin wrappers over host imports.
- Return typed provider records, never rendered strings.
- Raise explicit provider errors with Conduit error kinds.
- Avoid helpers that save one line but introduce a new concept.

Good API shape:

```python
from conduit_kit import logs


def search(query: logs.Query, client: logs.Client) -> logs.SearchResult:
    ...
```

Avoid APIs that make the plugin lifecycle implicit:

```python
@logs.plugin(...)
class Provider:
    ...
```

Decorators can be reconsidered later if they remove meaningful boilerplate
without hiding the contract boundary.

## Public Documentation

Public APIs should have docstrings when they teach Conduit behavior. They
should not restate obvious Python behavior.

Useful:

```python
class Query:
    """Normalized log search request from Conduit.

    `message` is message-field-only. `grep` is broad text search and should
    include message, stack trace, and logger-like fields when the backend can
    support them. Providers should treat `limit=0` as count-only.
    """
```

Not useful:

```python
class Query:
    """Stores query fields."""
```

The package README should show one complete minimal plugin and one focused
test. Provider-specific docs should explain contract semantics, capability
usage, and build commands. Tests can support docs, but they are not a
replacement for public API documentation.

## Testing Standards

Tests should prove contract behavior and adapter mapping. Avoid tests that only
mirror implementation structure.

High-value tests:

- WIT binding adapters map generated records to kit dataclasses.
- Provider errors map to the expected Conduit error kind.
- Capability clients pass exact request data to host imports.
- Backend query builders preserve Conduit filter semantics.
- Migrated plugins keep their current behavior.

Low-value tests:

- Dataclass field assignment.
- One assertion per helper when the helper has no contract semantics.
- Tests that duplicate generated binding behavior without plugin logic.

## First Implementation Slice

Implement only what is needed to migrate one provider.

Preferred first slice:

- `conduit_kit.errors`
- `conduit_kit.metadata`
- `conduit_kit.capabilities.http`
- `conduit_kit.capabilities.secrets`
- `conduit_kit.providers.logs`
- `conduit_kit.testing.http`
- `conduit_kit.testing.secrets`

Then migrate `satispay-logs-python` in the Satispay plugin PR.

Logs are the best first dogfood target because they exercise auth, HTTP,
secrets, provider errors, and rich filter semantics. If the initial API feels
too broad, pause and reduce it before migrating OpenAPI or DB.

## Migration Feedback Loop

Every plugin migration should include a review pass:

1. Compare before and after code size and shape.
2. Remove kit APIs that did not improve plugin clarity.
3. Rename awkward concepts before they spread.
4. Trim tests that only validate glue.
5. Add docstrings only where they explain Conduit semantics.
6. Keep behavior unchanged unless the migration exposes a real bug.

Migration order:

1. `satispay-logs-python`
2. `satispay-openapi-python`
3. `satispay-db-python`

DB should come last because PostgreSQL access, SQL generation, and secret use
have the highest security sensitivity.

## Stability

The kit is experimental until all Satispay plugins have migrated and at least
one review pass has removed or renamed weak APIs.

After that, decide whether to publish it as a Python package. Until then, keep
it source-driven and versioned with the Conduit repository so the kit stays
aligned with the WIT contracts.
