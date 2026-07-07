# Plugin System Direction

Conduit should be architected as a stable core runtime with plugin adapters.

The core CLI defines command semantics, typed provider contracts, output
schemas, rendering, configuration, state, versioning, and capability
enforcement. Plugins connect those contracts to company-specific systems such
as internal documentation portals, log backends, service catalogs, deployment
systems, auth flows, and secret stores.

The goal is not to let plugins become arbitrary CLIs. The goal is to let
companies implement standard engineering interfaces over their own systems.

## Goals

- Keep Conduit product-neutral.
- Let company-specific integrations live outside the core binary.
- Expose stable, typed facts to humans, agents, scripts, and IDEs.
- Keep provider outputs consistent across companies.
- Make plugin capabilities explicit and reviewable.
- Support private/internal plugins without forking Conduit.
- Prefer reusable provider contracts over custom command surfaces.

## Non-Goals

- Do not build a general package manager in the first iteration.
- Do not let plugins print arbitrary terminal output as their primary result.
- Do not give plugins unrestricted filesystem, network, environment, process,
  or secret access.
- Do not start with a company-specific plugin API.
- Do not make custom commands the default extension mechanism.

## Core Responsibilities

The Conduit core owns:

- CLI command routing and argument parsing.
- Stable provider interfaces.
- Provider request validation.
- Provider response validation.
- Text and JSON rendering.
- Exit code conventions.
- Project and user config loading.
- Plugin discovery and version checks.
- Plugin runtime instantiation.
- Capability policy and enforcement.
- State directories and caches.
- Built-in provider implementations for generic local tools when useful.

The core should remain the only renderer. A plugin returns typed data; the core
decides how that data appears in compact text and JSON.

## Plugin Responsibilities

Plugins own adapter logic:

- Discover company-specific services.
- Fetch OpenAPI specs from internal documentation systems.
- Query log backends.
- Resolve service ownership and runbooks.
- Refresh or consume auth material through explicit capabilities.
- Adapt deployment, incident, feature flag, or service catalog systems.

Plugins should be small bridges over external systems. They should avoid
encoding full user workflows or making policy decisions that belong to teams.

## Provider Interfaces Over Custom Commands

The primary extension model should be provider interfaces:

- `openapi-provider`
- `logs-provider`
- `db-provider`
- `service-catalog-provider`
- `docs-provider`
- `ci-provider`
- `secret-provider`

Custom commands are an escape hatch for integrations that do not yet fit a
standard provider. They should still return structured data through a core-owned
response shape.

Provider interfaces are what make Conduit portable. Custom commands are useful,
but less reusable.

## Component Model

Use Wasmtime and the WebAssembly Component Model.

Interface definitions should use WIT. This gives Conduit a typed boundary and
allows plugins to be written in any language that can target components.

The first runtime should support one plugin instance per configured plugin
artifact. The core should pass one request at a time and receive one typed
response. Streaming can wait.

## Versioning

Every plugin declares:

- Plugin id.
- Plugin version.
- Supported Conduit plugin protocol version.
- Implemented provider interfaces.
- Required capabilities.

Provider contracts should be versioned independently from plugin artifact
versions. A plugin can implement `openapi-provider@1` and later add
`logs-provider@1` without changing the OpenAPI contract.

Conduit should reject incompatible plugins with a compact error that names the
expected and actual protocol versions.

The first implemented protocol version is `1`. Provider plugins must declare
`protocol-version = "1"` and include their provider id, such as
`openapi-provider-v1`, `logs-provider-v1`, or `db-provider-v1`, in their
metadata provider list.

## Capability Model

Capabilities must be explicit. A plugin should receive only the capabilities
configured for it.

Implemented capability families:

- `http.hosts`: outbound HTTP access to exact allowlisted hosts.
- `file-read.paths`: read access to exact project-local path grants.
- `secrets.names`: access to exact user-scoped secret names.
- `postgres.connections`: access to exact named PostgreSQL connections.

Capabilities are configured in project or user config, not hidden in plugin
code. Plugin metadata can report what it implements; config grants what this
project allows.

HTTP hosts are exact host strings only; wildcards and URL schemes are rejected.
Plugin artifact paths and file-read paths are resolved relative to the project
root and cannot be empty, absolute, or contain parent traversal. Secret names
are exact user-scoped names only; wildcards, absolute paths, parent traversal,
and non-ASCII punctuation are rejected. PostgreSQL access is granted by
connection name, with host/database details resolved by the core.

The implemented host imports are `file-read-v1`, `http-client-v1`,
`http-client-v2`, `secret-store-v1`, and `postgres-v1`.
`file-read-v1` exposes one narrow operation, `read-text(path)`, where `path`
must be relative and must resolve inside a configured `file-read.paths` grant.
The host canonicalizes targets before reading so symlink escapes are denied.
`http-client-v1` exposes `get(url)` and allows only `http` or `https` URLs whose
host exactly matches a configured `http.hosts` grant.
`http-client-v2` adds method, headers, body, response headers, and timeout for
logs-style backend searches. `secret-store-v1` reads, writes, and deletes exact
named secrets from a user-scoped store, never from repository files.
`postgres-v1` executes bounded read-only queries through exact named
connections configured in project config.

The runtime test suite includes a component fixture that imports
`file-read-v1`, calls `read-text`, and returns the loaded file contents through
the OpenAPI provider contract. This keeps host import behavior tested across
the Wasmtime component boundary, not only as direct Rust policy tests.

## Runtime Performance

Large language-runtime components, such as Python components built with
`componentize-py`, can be expensive to compile on the first invocation. Conduit
enables Wasmtime's Cranelift strategy, copy-on-write memory initialization,
signals-based traps, and a project-local Wasmtime compilation cache at
`.conduit/state/wasmtime-cache`.

This keeps plugin loading safe and source-driven while removing compilation
from repeated CLI invocations. A local Python OpenAPI plugin prototype showed a
debug-build cold invocation around 92 seconds and a repeated cached invocation
around 1.4 seconds.

`conduit plugin check --path <component.wasm>` and
`conduit plugin check --provider openapi|logs` are the explicit cache-warming
and validation entry points for plugin artifacts. The path form defaults to the
OpenAPI provider world for backward compatibility; pass `--provider logs` to
validate a logs component by path. The check validates provider-world metadata
compatibility and reports compact plugin facts. The provider form reads
`.conduit/conduit.toml` and applies the configured capabilities while
instantiating the plugin.

Provider plugins should live close to the team or company systems they adapt.
A generic OpenAPI provider, for example, can read a project-local service
manifest, load JSON specs through `file-read-v1` or `http-client-v1`, match
exact paths and OpenAPI `{param}` path templates, and build to a component with
`componentize-py`.

## Configuration

Project config should wire providers to plugin implementations.

Example:

```toml
[plugins.company]
path = ".conduit/plugins/company.wasm"

[plugins.company.capabilities.http]
hosts = [
  "internal-documentation.example.com",
  "opensearch.example.com",
]

[plugins.company.capabilities.file-read]
paths = [
  ".conduit/company",
]

[plugins.company.capabilities.secrets]
names = [
  "company-logs/staging/cookie",
]

[openapi]
provider = "company"

[defaults]
environment = "staging"

[logs]
provider = "company"
```

User config may provide local defaults and secrets, but project config should
decide which provider handles a command in that project.

## Implemented Provider Contracts

OpenAPI was the first provider because it is high-value and has a clean
contract: request an operation or list of operations, return structured API
facts. Logs and DB now follow the same model.

OpenAPI command shape:

```bash
conduit openapi operation --service catalog-service --method GET --path /items
conduit openapi list --service catalog-service
conduit openapi search --service catalog-service --query item_id
```

### Request Shape

```text
service: string
environment: optional string
method: optional string
path: optional string
query: list key/value
```

### Operation Shape

```text
service: string
environment: optional string
method: string
path: string
operation_id: optional string
summary: optional string
description: optional string
request_schema_json: optional string
response_schema_json: optional string
source: optional string
parameters: list of name/location/required/description/schema-json
```

Schemas are JSON strings in the first version. The core can later expose more
typed schema helpers, but raw JSON keeps the initial contract honest and
compatible with OpenAPI variants.

### Text Rendering

The core should render compact facts:

```text
service: catalog-service
method: GET
path: /items
operation_id: listItems
summary: List catalog items
source: fixture://catalog-service/openapi.json
```

JSON output should include the full structured response.

## Error Model

Provider errors should be structured:

```text
kind: not-found | invalid-request | auth-required | permission-denied |
      unavailable | unsupported | internal
message: string
details: optional string
source: optional string
```

The core maps provider errors to compact CLI errors and stable exit codes.

Initial exit code convention:

- `0`: success.
- `1`: provider/runtime/data failure.
- `2`: usage error.

## WIT Sketch

This is the first compile-validated contract. The Rust crate generates
Wasmtime bindings from this WIT so syntax and naming mistakes fail at compile
time.

```wit
package conduit:plugin;

interface metadata {
  record plugin-metadata {
    id: string,
    version: string,
    protocol-version: string,
    providers: list<string>,
  }

  metadata: func() -> plugin-metadata;
}

interface file-read-v1 {
  variant file-read-error-kind {
    not-found,
    invalid-path,
    permission-denied,
    internal,
  }

  record file-read-error {
    kind: file-read-error-kind,
    message: string,
  }

  read-text: func(path: string) -> result<string, file-read-error>;
}

interface openapi-provider-v1 {
  record operation-request {
    service: string,
    environment: option<string>,
    method: option<string>,
    path: option<string>,
  }

  record operation {
    service: string,
    environment: option<string>,
    method: string,
    path: string,
    operation-id: option<string>,
    summary: option<string>,
    description: option<string>,
    request-schema-json: option<string>,
    response-schema-json: option<string>,
    source: option<string>,
  }

  variant provider-error-kind {
    not-found,
    invalid-request,
    auth-required,
    permission-denied,
    unavailable,
    unsupported,
    internal,
  }

  record provider-error {
    kind: provider-error-kind,
    message: string,
    details: option<string>,
    source: option<string>,
  }

  get-operation: func(request: operation-request) -> result<operation, provider-error>;
  operations: func(request: operation-request) -> result<list<operation>, provider-error>;
}

world openapi-provider {
  import file-read-v1;

  export metadata;
  export openapi-provider-v1;
}
```

## Implementation Status

The core plugin runtime is implemented with Wasmtime component loading,
metadata validation, project configuration, host capability enforcement, and a
project-local compilation cache.

Implemented provider worlds:

- `openapi-provider-v1`;
- `logs-provider-v1`;
- `db-provider-v1`.

Implemented host imports:

- `file-read-v1`;
- `http-client-v1`;
- `http-client-v2`;
- `secret-store-v1`;
- `postgres-v1`.

Fixture providers remain for tests and explicit examples. Real projects should
select provider plugins in `.conduit/conduit.toml`.
