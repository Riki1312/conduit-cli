# conduit-kit

`conduit-kit` is a small Python authoring kit for Conduit plugins. It keeps
generated WIT bindings at the edge of the plugin and lets adapter code work
with plain Python dataclasses.

The kit is experimental. Its API should stay narrow until real plugin
migrations prove that it reduces glue without hiding Conduit contract
semantics.

## Logs Example

```python
from conduit_kit import errors
from conduit_kit.capabilities.http import HttpClient
from conduit_kit.capabilities.secrets import SecretStore
from conduit_kit.providers import logs
from wit_world import exports
from wit_world.exports import logs_provider_v1, metadata
from wit_world.imports import http_client_v2, secret_store_v1


def search(query: logs.Query) -> logs.SearchResult:
    http = HttpClient(http_client_v2)
    secrets = SecretStore(secret_store_v1)
    ...


class LogsProviderV1(exports.LogsProviderV1):
    def search(self, query: logs_provider_v1.LogQuery):
        try:
            result = search(logs.query_from_wit(query))
            return logs.search_result_to_wit(logs_provider_v1, result)
        except ValueError as error:
            raise errors.provider_error(
                logs_provider_v1,
                errors.ProviderErrorKind.INVALID_REQUEST,
                str(error),
            )
```

## OpenAPI Example

```python
from conduit_kit import errors
from conduit_kit.capabilities.files import FileReader
from conduit_kit.providers import openapi
from wit_world import exports
from wit_world.exports import openapi_provider_v1
from wit_world.imports import file_read_v1


def load_operations(request: openapi.OperationRequest) -> list[openapi.Operation]:
    manifest = FileReader(file_read_v1).read_optional_text(".conduit/openapi.json")
    ...


class OpenapiProviderV1(exports.OpenapiProviderV1):
    def operations(self, request: openapi_provider_v1.OperationRequest):
        try:
            result = load_operations(openapi.operation_request_from_wit(request))
            return openapi.operations_to_wit(openapi_provider_v1, result)
        except ValueError as error:
            raise errors.provider_error(
                openapi_provider_v1,
                errors.ProviderErrorKind.INVALID_REQUEST,
                str(error),
            )
```

## DB Example

```python
from conduit_kit.capabilities.postgres import PostgresClient
from conduit_kit.providers import db
from wit_world import exports
from wit_world.exports import db_provider_v1
from wit_world.imports import postgres_query_v1


def read_records(request: db.ReadRequest) -> db.ReadResult:
    postgres = PostgresClient(postgres_query_v1)
    ...


class DbProviderV1(exports.DbProviderV1):
    def read(self, request: db_provider_v1.ReadRequest):
        result = read_records(db.read_request_from_wit(request))
        return db.read_result_to_wit(db_provider_v1, result)
```

Public APIs document Conduit behavior when it is not obvious. They should not
restate normal Python behavior.
