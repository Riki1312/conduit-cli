from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Any


@dataclass(frozen=True)
class TimeRange:
    """Time window selected by Conduit from `--since` or explicit timestamps."""

    from_: str
    to: str | None
    source: str


@dataclass(frozen=True)
class Query:
    """Normalized log search request from Conduit.

    `message` is message-field-only. `grep` is broad text search and should
    include message, stack trace, and logger-like fields when the backend can
    support them. Providers should treat `limit=0` as count-only.
    """

    service: str
    environment: str | None
    time_range: TimeRange
    limit: int
    levels: list[str]
    cid: str | None
    trace_id: str | None
    message: str | None
    grep: str | None
    logger: str | None
    exclude_messages: list[str]
    exclude_greps: list[str]
    exclude_loggers: list[str]
    include_trace: bool
    cursor: str | None = None


@dataclass(frozen=True)
class AuthRequest:
    """Request to store, check, or explain provider auth material."""

    environment: str | None
    secret: str | None
    check: bool


class Status(Enum):
    """Search result statuses defined by `logs-provider-v1`."""

    OK = "Ok"
    PARTIAL = "Partial"
    AUTH_REQUIRED = "AuthRequired"
    UNAVAILABLE = "Unavailable"
    INVALID_REQUEST = "InvalidRequest"
    ERROR = "Error"


class AuthStatus(Enum):
    """Auth result statuses defined by `logs-provider-v1`."""

    OK = "Ok"
    ACTION_REQUIRED = "ActionRequired"


@dataclass(frozen=True)
class Event:
    """One normalized log event returned by a logs provider."""

    id: str | None
    timestamp: str
    level: str | None
    service: str | None
    environment: str | None
    cid: str | None
    trace_id: str | None
    logger: str | None
    message: str
    stack_trace: str | None = None
    source: str | None = None
    attributes_json: str | None = None


@dataclass(frozen=True)
class Diagnostic:
    """Structured hint about provider behavior or partial results."""

    kind: str
    hint: str | None = None


@dataclass(frozen=True)
class SearchResult:
    """Normalized logs provider response before WIT conversion."""

    status: Status
    provider: str
    service: str
    environment: str | None
    time_range: TimeRange
    logs: list[Event]
    matches: int | None = None
    next_cursor: str | None = None
    checked_until: str | None = None
    diagnostics: list[Diagnostic] = field(default_factory=list)


@dataclass(frozen=True)
class AuthResult:
    """Normalized logs auth response before WIT conversion."""

    status: AuthStatus
    provider: str
    environment: str | None
    destination: str | None = None
    expires_at: str | None = None
    diagnostics: list[Diagnostic] = field(default_factory=list)


def query_from_wit(query: Any) -> Query:
    """Convert a generated `LogQuery` record to the kit query model."""

    return Query(
        service=query.service,
        environment=query.environment,
        time_range=time_range_from_wit(query.time_range),
        limit=query.limit,
        levels=list(query.levels),
        cid=query.cid,
        trace_id=query.trace_id,
        message=query.message,
        grep=query.grep,
        logger=query.logger,
        exclude_messages=list(query.exclude_messages),
        exclude_greps=list(query.exclude_greps),
        exclude_loggers=list(query.exclude_loggers),
        include_trace=query.include_trace,
        cursor=query.cursor,
    )


def auth_request_from_wit(request: Any) -> AuthRequest:
    """Convert a generated `AuthRequest` record to the kit auth model."""

    return AuthRequest(
        environment=request.environment,
        secret=request.secret,
        check=request.check,
    )


def time_range_from_wit(value: Any) -> TimeRange:
    """Convert a generated `TimeRange` record to the kit time range model."""

    return TimeRange(from_=value.from_, to=value.to, source=value.source)


def search_result_to_wit(provider: Any, result: SearchResult):
    """Convert a kit search result to a generated `SearchResult` record."""

    return provider.SearchResult(
        status=_log_status_to_wit(provider, result.status),
        provider=result.provider,
        service=result.service,
        environment=result.environment,
        time_range=time_range_to_wit(provider, result.time_range),
        matches=result.matches,
        shown=len(result.logs),
        logs=[event_to_wit(provider, event) for event in result.logs],
        next_cursor=result.next_cursor,
        checked_until=result.checked_until,
        diagnostics=[
            diagnostic_to_wit(provider, diagnostic)
            for diagnostic in result.diagnostics
        ],
    )


def auth_result_to_wit(provider: Any, result: AuthResult):
    """Convert a kit auth result to a generated `AuthResult` record."""

    return provider.AuthResult(
        status=_auth_status_to_wit(provider, result.status),
        provider=result.provider,
        environment=result.environment,
        destination=result.destination,
        expires_at=result.expires_at,
        diagnostics=[
            diagnostic_to_wit(provider, diagnostic)
            for diagnostic in result.diagnostics
        ],
    )


def time_range_to_wit(provider: Any, value: TimeRange):
    """Convert a kit time range to a generated `TimeRange` record."""

    return provider.TimeRange(from_=value.from_, to=value.to, source=value.source)


def event_to_wit(provider: Any, event: Event):
    """Convert a kit log event to a generated `LogEvent` record."""

    return provider.LogEvent(
        id=event.id,
        timestamp=event.timestamp,
        level=event.level,
        service=event.service,
        environment=event.environment,
        cid=event.cid,
        trace_id=event.trace_id,
        logger=event.logger,
        message=event.message,
        stack_trace=event.stack_trace,
        source=event.source,
        attributes_json=event.attributes_json,
    )


def diagnostic_to_wit(provider: Any, diagnostic: Diagnostic):
    """Convert a kit diagnostic to a generated `Diagnostic` record."""

    return provider.Diagnostic(kind=diagnostic.kind, hint=diagnostic.hint)


def _log_status_to_wit(provider: Any, status: Status):
    return getattr(provider, f"LogStatus_{status.value}")()


def _auth_status_to_wit(provider: Any, status: AuthStatus):
    return getattr(provider, f"AuthStatus_{status.value}")()
