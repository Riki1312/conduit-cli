# Building Plugins

Conduit plugins are WebAssembly components that implement typed provider
contracts. A plugin adapts an external system to a standard Conduit interface;
it does not print terminal output or define its own command UX.

The core CLI owns command parsing, validation, rendering, JSON/JSONL shape,
capability enforcement, state directories, and exit semantics. Plugins own the
adapter logic needed to fetch OpenAPI docs, query logs, authenticate to a
backend, or read company/project data sources.

## Current Provider Contracts

Plugin contracts live in `wit/conduit-plugin`.

- `openapi-provider-v1`: lists operations and fetches one operation by
  service, method, path, and optional environment.
- `logs-provider-v1`: searches logs and handles provider-specific auth.
- `db-provider-v1`: lists, describes, and reads service-owned DB resources.

Every plugin exports `metadata` and reports:

- `id`: stable plugin id.
- `version`: plugin artifact version.
- `protocol-version`: currently `1`.
- `providers`: implemented provider ids, such as `openapi-provider-v1`,
  `logs-provider-v1`, or `db-provider-v1`.

Conduit rejects a plugin when the protocol version or provider list does not
match the configured command.

## Capabilities

Plugins receive only the capabilities granted in `.conduit/conduit.toml`.
Capabilities are exact and reviewable.

Supported grants today:

- `file-read.paths`: relative project paths the plugin may read.
- `http.hosts`: exact HTTP hosts the plugin may call.
- `postgres.connections`: exact named PostgreSQL targets the plugin may query.
- `secrets.names`: exact user-scoped secret names the plugin may read or write.

Example OpenAPI provider:

```toml
[plugins.company-openapi]
path = ".conduit/plugins/company-openapi.wasm"

[plugins.company-openapi.capabilities.file-read]
paths = [".conduit/company-openapi"]

[plugins.company-openapi.capabilities.http]
hosts = ["docs.example.com"]

[openapi]
provider = "company-openapi"
```

Example logs provider:

```toml
[plugins.company-logs]
path = ".conduit/plugins/company-logs.wasm"

[plugins.company-logs.capabilities.http]
hosts = ["logs.example.com"]

[plugins.company-logs.capabilities.secrets]
names = ["company-logs/staging/token"]

[defaults]
environment = "staging"

[logs]
provider = "company-logs"
default_since = "15m"
```

Example DB provider:

```toml
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

[defaults]
environment = "staging"

[db]
provider = "company-db"
```

PostgreSQL connections default to `ssl_mode = "disable"`. Use
`ssl_mode = "require"` and `ssl_root_cert` for TLS with a project-pinned CA
bundle.

Repository config should not contain credentials, cookies, tokens, or raw log
captures. Use the secret capability for user-scoped auth material.

## Implementation Shape

A plugin usually has three small layers:

1. Contract bindings generated from the WIT files.
2. Adapter code that calls the external system through host capabilities.
3. Mapping code that returns Conduit provider records.

For an OpenAPI provider, keep the adapter focused on:

- loading service metadata or specs;
- resolving the requested service/environment;
- matching exact paths and OpenAPI `{param}` path templates;
- returning operation fields and schemas as structured records.

For a logs provider, keep the adapter focused on:

- mapping Conduit filters to the backend query language;
- applying time ranges and limits;
- returning normalized log events;
- storing and checking auth through `secret-store-v1`.

For a DB provider, keep the adapter focused on:

- mapping service/resource names to backend tables, views, or APIs;
- resolving environment-specific connection and secret names;
- generating bounded read-only queries through host capabilities;
- returning resource descriptions and records as normalized provider records.

Provider-specific fields can be returned through JSON escape hatches where the
contract allows them, but compact text output should remain useful without raw
backend payloads.

## Language Choice

Any language that can produce WebAssembly components can be used. Python is a
pragmatic starting point for internal adapters because it is concise and works
with `componentize-py`. Rust is a good fit when the plugin needs stronger
static checking or lower cold-start cost.

Keep the boundary small regardless of language. The plugin should translate
data, not become a second CLI.

## Validation

Validate and warm plugins before normal commands use them:

```bash
conduit plugin check --path .conduit/plugins/company-openapi.wasm
conduit plugin check --provider openapi
conduit plugin check --path .conduit/plugins/company-logs.wasm --provider logs
conduit plugin check --provider logs
conduit plugin check --path .conduit/plugins/company-db.wasm --provider db
conduit plugin check --provider db
conduit plugin check --path .conduit/plugins/company-openapi.wasm --json
```

The `--path` form validates a component directly. The `--provider` form loads
the current project config, applies configured capabilities, checks metadata,
and warms the Wasmtime compilation cache.

## Minimal Project Layout

One possible project-local layout:

```text
.conduit/
  conduit.toml
  plugins/
    company-openapi.wasm
    company-logs.wasm
  company-openapi/
    services.json
```

For private company adapters, keep plugin source code and build scripts in a
private repository. Commit only the project config and the compiled plugin
artifact when that matches the team's distribution model.

## Design Rules

- Prefer provider contracts over custom commands.
- Return typed data, not arbitrary terminal text.
- Request the smallest capability set that works.
- Keep auth material user-scoped and out of repository files.
- Make backend-specific assumptions visible in plugin code and docs.
- Test mapping behavior with fixture backend responses before using real
  services.
