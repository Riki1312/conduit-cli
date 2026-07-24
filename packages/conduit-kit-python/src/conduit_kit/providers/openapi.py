from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
from typing import Any


@dataclass(frozen=True)
class OperationRequest:
    """Request for OpenAPI operations for one service and environment."""

    service: str
    environment: str | None
    method: str | None
    path: str | None


class ParameterLocation(Enum):
    """Parameter locations defined by `openapi-provider-v1`."""

    PATH = "Path"
    QUERY = "Query"
    HEADER = "Header"
    COOKIE = "Cookie"


@dataclass(frozen=True)
class Parameter:
    """OpenAPI operation parameter normalized for Conduit."""

    name: str
    location: ParameterLocation
    required: bool
    description: str | None = None
    schema_json: str | None = None


@dataclass(frozen=True)
class Operation:
    """OpenAPI operation returned by a provider before WIT conversion."""

    service: str
    environment: str | None
    method: str
    path: str
    parameters: list[Parameter]
    operation_id: str | None = None
    summary: str | None = None
    description: str | None = None
    request_schema_json: str | None = None
    response_schema_json: str | None = None
    source: str | None = None


def operation_request_from_wit(request: Any) -> OperationRequest:
    """Convert a generated `OperationRequest` record to the kit model."""

    return OperationRequest(
        service=request.service,
        environment=request.environment,
        method=request.method,
        path=request.path,
    )


def operation_to_wit(provider: Any, operation: Operation):
    """Convert a kit operation to a generated `Operation` record."""

    return provider.Operation(
        service=operation.service,
        environment=operation.environment,
        method=operation.method,
        path=operation.path,
        parameters=[
            parameter_to_wit(provider, parameter)
            for parameter in operation.parameters
        ],
        operation_id=operation.operation_id,
        summary=operation.summary,
        description=operation.description,
        request_schema_json=operation.request_schema_json,
        response_schema_json=operation.response_schema_json,
        source=operation.source,
    )


def operations_to_wit(provider: Any, operations: list[Operation]):
    """Convert kit operations to generated `Operation` records."""

    return [operation_to_wit(provider, operation) for operation in operations]


def parameter_to_wit(provider: Any, parameter: Parameter):
    """Convert a kit parameter to a generated `Parameter` record."""

    return provider.Parameter(
        name=parameter.name,
        location=_parameter_location_to_wit(provider, parameter.location),
        required=parameter.required,
        description=parameter.description,
        schema_json=parameter.schema_json,
    )


def _parameter_location_to_wit(provider: Any, location: ParameterLocation):
    return getattr(provider, f"ParameterLocation_{location.value}")()
