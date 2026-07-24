from __future__ import annotations

from dataclasses import dataclass, field

from conduit_kit.capabilities.postgres import PostgresConnection, PostgresResult


@dataclass
class FakePostgresClient:
    """Queue-backed fake for provider code that accepts a Postgres client."""

    results: list[PostgresResult]
    queries: list[dict] = field(default_factory=list)

    def query(
        self,
        connection: PostgresConnection,
        sql: str,
        params: list[str],
        *,
        timeout_ms: int | None = None,
    ) -> PostgresResult:
        self.queries.append(
            {
                "connection": connection,
                "sql": sql,
                "params": params,
                "timeout_ms": timeout_ms,
            }
        )
        if not self.results:
            raise AssertionError("fake Postgres client has no queued results")
        return self.results.pop(0)


def postgres_result(rows_json: list[str]) -> PostgresResult:
    """Build a queued fake Postgres result."""

    return PostgresResult(rows_json=rows_json)
