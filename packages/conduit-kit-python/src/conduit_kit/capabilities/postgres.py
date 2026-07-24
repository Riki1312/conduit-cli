from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
from typing import Any


class PostgresErrorKind(Enum):
    """Error kinds returned by Conduit's `postgres-query-v1` capability."""

    INVALID_REQUEST = "InvalidRequest"
    PERMISSION_DENIED = "PermissionDenied"
    UNAVAILABLE = "Unavailable"
    INTERNAL = "Internal"


@dataclass(frozen=True)
class PostgresConnection:
    """Connection name and credentials for a granted Postgres connection."""

    name: str
    username: str
    password: str
    connect_timeout_ms: int | None = None


@dataclass(frozen=True)
class PostgresResult:
    """Rows returned by `postgres-query-v1` as JSON objects."""

    rows_json: list[str]


@dataclass(frozen=True)
class PostgresError(Exception):
    """Failure returned by Conduit's Postgres query capability."""

    kind: PostgresErrorKind
    message: str

    def __str__(self) -> str:
        return self.message


class PostgresClient:
    """Client for Conduit's provider-scoped `postgres-query-v1` capability."""

    def __init__(self, binding: Any):
        self._binding = binding

    def query(
        self,
        connection: PostgresConnection,
        sql: str,
        params: list[str],
        *,
        timeout_ms: int | None = None,
    ) -> PostgresResult:
        """Run one granted Postgres query through the host."""

        try:
            result = self._binding.query(
                self._binding.PostgresQuery(
                    connection=self._binding.PostgresConnection(
                        name=connection.name,
                        username=connection.username,
                        password=connection.password,
                        connect_timeout_ms=connection.connect_timeout_ms,
                    ),
                    sql=sql,
                    params=params,
                    timeout_ms=timeout_ms,
                )
            )
        except Exception as error:
            raise _postgres_error(self._binding, error) from error

        return PostgresResult(rows_json=list(result.rows_json))


def _postgres_error(binding: Any, error: Exception) -> PostgresError:
    kind = getattr(getattr(error, "value", None), "kind", None)
    message = getattr(getattr(error, "value", None), "message", str(error))

    for name, value in (
        ("InvalidRequest", PostgresErrorKind.INVALID_REQUEST),
        ("PermissionDenied", PostgresErrorKind.PERMISSION_DENIED),
        ("Unavailable", PostgresErrorKind.UNAVAILABLE),
    ):
        variant = getattr(binding, f"PostgresErrorKind_{name}", None)
        if variant is not None and isinstance(kind, variant):
            return PostgresError(value, message)

    return PostgresError(PostgresErrorKind.INTERNAL, message)
