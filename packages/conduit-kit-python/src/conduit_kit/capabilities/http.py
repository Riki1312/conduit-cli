from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Mapping, Sequence

from conduit_kit.errors import CapabilityError


@dataclass(frozen=True)
class Header:
    """HTTP header record used by Conduit host capability clients."""

    name: str
    value: str


@dataclass(frozen=True)
class Response:
    """HTTP response returned by Conduit's HTTP capabilities."""

    status: int
    headers: list[Header]
    body: str


class HttpClient:
    """Client for Conduit's HTTP host capabilities.

    `http-client-v2` supports `request`. Older provider worlds may expose a
    smaller `get` import; the client maps that response into the same `Response`
    model with an empty header list.
    """

    def __init__(self, binding: Any):
        self._binding = binding

    def post(
        self,
        url: str,
        *,
        headers: Mapping[str, str] | Sequence[Header] | None = None,
        body: str | None = None,
        timeout_ms: int | None = None,
    ) -> Response:
        """Send a POST request through `http-client-v2`."""

        return self.request(
            "post",
            url,
            headers=headers,
            body=body,
            timeout_ms=timeout_ms,
        )

    def get(self, url: str, *, timeout_ms: int | None = None) -> Response:
        """Send a GET request through the available HTTP host capability."""

        if hasattr(self._binding, "get"):
            try:
                response = self._binding.get(url)
            except Exception as error:
                detail = getattr(getattr(error, "value", None), "message", str(error))
                raise CapabilityError(detail) from error

            return Response(status=response.status, headers=[], body=response.body)

        return self.request("get", url, timeout_ms=timeout_ms)

    def request(
        self,
        method: str,
        url: str,
        *,
        headers: Mapping[str, str] | Sequence[Header] | None = None,
        body: str | None = None,
        timeout_ms: int | None = None,
    ) -> Response:
        """Send one HTTP request through the host allowlist."""

        method_variant = self._method(method)
        try:
            response = self._binding.request(
                self._binding.HttpRequest(
                    method=method_variant,
                    url=url,
                    headers=[
                        self._binding.HttpHeader(name=header.name, value=header.value)
                        for header in normalize_headers(headers)
                    ],
                    body=body,
                    timeout_ms=timeout_ms,
                )
            )
        except Exception as error:
            detail = getattr(getattr(error, "value", None), "message", str(error))
            raise CapabilityError(detail) from error

        return Response(
            status=response.status,
            headers=[
                Header(name=header.name, value=header.value)
                for header in response.headers
            ],
            body=response.body,
        )

    def _method(self, method: str):
        normalized = method.lower()
        if normalized == "get":
            return self._binding.HttpMethod_Get()
        if normalized == "post":
            return self._binding.HttpMethod_Post()
        raise ValueError(f"unsupported HTTP method `{method}`")


def normalize_headers(headers: Mapping[str, str] | Sequence[Header] | None) -> list[Header]:
    """Normalize plugin-friendly header inputs into Conduit header records."""

    if headers is None:
        return []
    if isinstance(headers, Mapping):
        return [Header(name=name, value=value) for name, value in headers.items()]
    return list(headers)
