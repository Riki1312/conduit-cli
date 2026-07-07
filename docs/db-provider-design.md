# DB Provider Design

Conduit DB commands should provide constrained operational data access for
development and pre-production environments. The feature should help humans and
agents inspect and adjust test/staging data without exposing raw SQL or broad
database privileges.

The goal is not to turn Conduit into a database client. The goal is to define a
small, safe interface over service-owned resources, with provider plugins
handling auth, connection details, database technology, and resource mapping.

## Goals

- Keep the command surface product-neutral and service-oriented.
- Avoid raw SQL and backend-specific query languages in the core CLI.
- Make common test/staging data tasks short, deterministic, and scriptable.
- Return compact structured facts by default and JSON when requested.
- Keep auth, database routing, schemas, and backend details in plugins.
- Prevent dangerous operations by excluding delete, bulk update, and schema
  changes from the first contract.
- Make the safe path useful enough that humans and agents do not need to reach
  for raw DB tools for normal test-data work.

## Non-Goals

- Do not provide a generic SQL shell.
- Do not support production access in the first implementation.
- Do not support delete, truncate, migration, DDL, or schema mutation.
- Do not support bulk updates.
- Do not stream raw database driver output.
- Do not store credentials, certificates, tokens, or connection strings in
  repository files.
- Do not encode company-specific database topology in Conduit core.

## Command Shape

Start with a minimal command set:

```bash
conduit db resources SERVICE --env test
conduit db describe SERVICE RESOURCE --env test
conduit db read SERVICE RESOURCE --id ID --env test
conduit db read SERVICE RESOURCE --filter status=ACTIVE --limit 20 --env test
```

`read` is the only read command. Reading by id is just the most common exact
selector. The command can render a one-record result cleanly while still using
the same provider request shape as filtered reads.

`resources` lists the resource names a provider exposes for a service. A
resource may map to a SQL table, NoSQL collection, API-backed store, view, or
other service-owned data source. The core should not require database-specific
terms such as table or collection.

`describe` returns the minimal useful shape of one resource: identity field,
field names, and field types when available. It should be broad enough to
replace a separate `schema` command without implying that every provider has a
formal database schema.

`insert` and `update` should be added later, after the read path and provider
contract are proven with fixture data and a PostgreSQL-backed example plugin.

## Read Semantics

Reads must be bounded.

Defaults:

- `--env`: provider or project default, usually `test`.
- `--limit`: default `20`.
- maximum limit: provider-defined, but core should enforce a conservative
  ceiling such as `100` unless config chooses a lower value.

Selectors:

- `--id ID`: exact identity lookup.
- `--filter field=value`: repeatable equality filters.

The first implementation should keep filters intentionally simple. Range
filters, contains filters, sorting, and provider-specific query syntax can wait
until real usage proves they are needed.

Example text output:

```text
status: ok
provider: company-db
environment: test
service: checkout-service
resource: payment_account
matched: 1
shown: 1

record:
  id: acc_123
  status: ACTIVE
  created_at: 2026-06-16T08:12:01Z
```

JSON output should include the same normalized facts plus records as structured
objects:

```json
{
  "status": "ok",
  "provider": "company-db",
  "environment": "test",
  "service": "checkout-service",
  "resource": "payment_account",
  "matched": 1,
  "shown": 1,
  "records": [
    {
      "id": "acc_123",
      "status": "ACTIVE",
      "created_at": "2026-06-16T08:12:01Z"
    }
  ],
  "diagnostics": []
}
```

## Describe Semantics

`describe` should stay small. Its job is to give humans and agents enough
shape information to perform safe reads without understanding the backing
database.

Example text output:

```text
provider: company-db
service: checkout-service
resource: payment_account
environment: test
id_field: id
fields: 3

field: id
type: string

field: status
type: string

field: currency
type: string
```

Example JSON output:

```json
{
  "provider": "company-db",
  "service": "checkout-service",
  "resource": "payment_account",
  "environment": "test",
  "id_field": "id",
  "fields": [
    {
      "name": "id",
      "type": "string"
    },
    {
      "name": "status",
      "type": "string"
    },
    {
      "name": "currency",
      "type": "string"
    }
  ]
}
```

The first response should include only `provider`, `service`, `resource`,
`environment`, `id_field`, and `fields`. Each field should include `name` and
`type` when known.

Avoid sensitive-field markers, descriptions, writable flags, insert
requirements, indexes, constraints, relation graphs, and provider notes until
the read-only contract has been used in real work. PostgreSQL metadata can
provide names and types reliably; anything beyond that needs explicit
provider-owned configuration.

## Future Write Semantics

Writes should be useful but deliberately narrow.

`insert`:

- accepts exactly one JSON object from `--data`;
- returns the inserted record id when the provider can determine it;
- may return the inserted record or a compact field summary;
- never accepts inline SQL or backend expressions.

`update`:

- requires exactly one `--id`;
- accepts repeatable `--set field=value`;
- updates only that one record;
- does not accept `--filter`;
- does not support multiple ids;
- does not support provider-specific update expressions.

Writes are intentionally out of the first implementation. When added, they
should avoid dry-run, apply tokens, reason prompts, and multi-step confirmation
flows. Safety should come from the constrained command surface, environment
policy, explicit plugin capabilities, bounded reads, and absence of dangerous
operations.

## Safety Defaults

Core-level safety defaults:

- no production environment support in the first implementation;
- no raw SQL;
- no delete;
- no insert or update in the first implementation;
- no bulk update;
- no schema changes;
- reads are bounded;
- credentials and connection details never appear in text or JSON output.

Provider or project policy should decide which environments are allowed:

```toml
[defaults]
environment = "test"

[db]
provider = "company-db"
```

Unsupported environments should fail with compact, actionable provider errors.
Write policy can be added when insert/update commands are introduced.

## Provider Responsibilities

DB provider plugins own:

- service-to-database or service-to-backend routing;
- environment-specific connection details;
- authentication and token/session refresh;
- mapping resources to backend tables, collections, views, or APIs;
- field typing;
- backend-specific query generation;
- future write permission checks and audit metadata when writes are added.

Conduit core owns:

- CLI arguments and validation;
- normalized request and response models;
- text and JSON rendering;
- limit handling;
- redaction rules that can be driven by provider metadata;
- plugin loading and capability enforcement;
- stable exit semantics.

## Provider Contract

The first provider interface should be small:

```text
db-provider-v1

resources(request) -> resource-list
describe(request) -> resource-description
read(request) -> read-result
```

Request shape:

```text
service: string
resource: optional string
environment: optional string
id: optional string
filters: list field-filter
limit: u32
```

Resource description shape:

```text
provider: string
service: string
resource-name: string
id-field: string
fields: list field-description
```

Field description shape:

```text
name: string
data-type: optional string
```

Read result shape:

```text
status: ok | partial | auth-required | unavailable | invalid-request | error
provider: string
service: string
resource-name: string
environment: optional string
matched: optional u64
shown: u64
records-json: list string
```

`records-json` lets the provider preserve native field shapes without the core
needing to model every scalar and nested value type in WIT. The core should
parse these JSON strings for rendering and redaction.

## Capabilities

The existing plugin capability model is reused. PostgreSQL-backed plugins use
named connection grants, plus exact secret grants for credentials:

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

`ssl_mode` defaults to `disable`; `require` can be paired with `ssl_root_cert`
when a database needs TLS with a project-pinned CA bundle. In that mode, the
configured host must match the server certificate name.

The PostgreSQL capability is intentionally narrow. Plugins can read project
manifests through `file-read-v1`, request an exact connection name, pass
credentials read through `secret-store-v1`, and submit a single read-only query.
Conduit core resolves the host/database from project config, enforces the
connection grant, wraps rows as JSON, and rejects obvious non-read statements.
For company deployments, a future gateway can centralize auth, audit, network
policy, and environment routing outside the Conduit process without changing
the DB provider command shape.

## Exit Semantics

- Successful reads exit `0`, including empty results.
- `read --id` exits `0` with `matched: 0` when the id is not found, unless the
  provider treats missing ids as an invalid request.
- Invalid selectors, unsupported resources, unsupported environments, auth
  failures, and provider failures exit non-zero.
- Text and JSON output should make the status explicit.

## First Implementation Slice

1. Add `docs/db-provider-design.md` and link it from project docs.
2. Add `db` command parsing with fixture provider behavior.
3. Implement `resources`, `describe`, and `read` against fixture data.
4. Add compact text and JSON renderers.
5. Add config loading for `[db]` provider selection and environment defaults.
6. Add WIT contract and Wasmtime provider loading for `db-provider-v1`.
7. Add a narrow PostgreSQL host capability for read-only plugin queries.
8. Build a PostgreSQL-backed example plugin that implements read-only access.
9. Use the PostgreSQL plugin to validate whether the provider contract needs
   changes before adding writes.

Insert and update should be documented and implemented as a later slice.
