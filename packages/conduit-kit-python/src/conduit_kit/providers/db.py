from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
from typing import Any


@dataclass(frozen=True)
class ResourceRequest:
    """Request for resources exposed by one service database."""

    service: str
    environment: str | None


@dataclass(frozen=True)
class DescribeRequest:
    """Request for one database resource description."""

    service: str
    resource_name: str
    environment: str | None


@dataclass(frozen=True)
class FieldFilter:
    """String filter requested by `db-provider-v1`."""

    field: str
    value: str


@dataclass(frozen=True)
class ReadRequest:
    """Request to read records from one database resource."""

    service: str
    resource_name: str
    environment: str | None
    id: str | None
    filters: list[FieldFilter]
    limit: int


@dataclass(frozen=True)
class Resource:
    """Database resource name exposed by a provider."""

    name: str


@dataclass(frozen=True)
class ResourceList:
    """Normalized DB resource list before WIT conversion."""

    provider: str
    service: str
    environment: str | None
    resources: list[Resource]


@dataclass(frozen=True)
class FieldDescription:
    """Field metadata exposed by `db-provider-v1`."""

    name: str
    data_type: str | None = None


@dataclass(frozen=True)
class ResourceDescription:
    """Normalized DB resource description before WIT conversion."""

    provider: str
    service: str
    resource_name: str
    environment: str | None
    id_field: str
    fields: list[FieldDescription]


class ReadStatus(Enum):
    """Read result statuses defined by `db-provider-v1`."""

    OK = "Ok"
    PARTIAL = "Partial"
    AUTH_REQUIRED = "AuthRequired"
    UNAVAILABLE = "Unavailable"
    INVALID_REQUEST = "InvalidRequest"
    ERROR = "Error"


@dataclass(frozen=True)
class ReadResult:
    """Normalized DB read response before WIT conversion."""

    status: ReadStatus
    provider: str
    service: str
    resource_name: str
    environment: str | None
    records_json: list[str]
    matched: int | None = None


def resource_request_from_wit(request: Any) -> ResourceRequest:
    """Convert a generated `ResourceRequest` record to the kit model."""

    return ResourceRequest(service=request.service, environment=request.environment)


def describe_request_from_wit(request: Any) -> DescribeRequest:
    """Convert a generated `DescribeRequest` record to the kit model."""

    return DescribeRequest(
        service=request.service,
        resource_name=request.resource_name,
        environment=request.environment,
    )


def read_request_from_wit(request: Any) -> ReadRequest:
    """Convert a generated `ReadRequest` record to the kit model."""

    return ReadRequest(
        service=request.service,
        resource_name=request.resource_name,
        environment=request.environment,
        id=request.id,
        filters=[
            FieldFilter(field=field_filter.field, value=field_filter.value)
            for field_filter in request.filters
        ],
        limit=request.limit,
    )


def resource_list_to_wit(provider: Any, value: ResourceList):
    """Convert a kit resource list to a generated `ResourceList` record."""

    return provider.ResourceList(
        provider=value.provider,
        service=value.service,
        environment=value.environment,
        resources=[
            provider.DbResource(name=resource.name)
            for resource in value.resources
        ],
    )


def resource_description_to_wit(provider: Any, value: ResourceDescription):
    """Convert a kit resource description to a generated record."""

    return provider.ResourceDescription(
        provider=value.provider,
        service=value.service,
        resource_name=value.resource_name,
        environment=value.environment,
        id_field=value.id_field,
        fields=[
            provider.FieldDescription(
                name=field.name,
                data_type=field.data_type,
            )
            for field in value.fields
        ],
    )


def read_result_to_wit(provider: Any, value: ReadResult):
    """Convert a kit read result to a generated `ReadResult` record."""

    return provider.ReadResult(
        status=_read_status_to_wit(provider, value.status),
        provider=value.provider,
        service=value.service,
        resource_name=value.resource_name,
        environment=value.environment,
        matched=value.matched,
        shown=len(value.records_json),
        records_json=value.records_json,
    )


def _read_status_to_wit(provider: Any, status: ReadStatus):
    return getattr(provider, f"ReadStatus_{status.value}")()
