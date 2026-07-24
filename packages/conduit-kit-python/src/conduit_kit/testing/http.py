from __future__ import annotations

from dataclasses import dataclass, field

from conduit_kit.capabilities.http import Header, Response, normalize_headers


@dataclass
class FakeHttpClient:
    """Queue-backed fake for provider code that accepts an HTTP client."""

    responses: list[Response]
    requests: list[dict] = field(default_factory=list)

    def post(self, url, *, headers=None, body=None, timeout_ms=None):
        return self.request(
            "post",
            url,
            headers=headers,
            body=body,
            timeout_ms=timeout_ms,
        )

    def request(self, method, url, *, headers=None, body=None, timeout_ms=None):
        self.requests.append(
            {
                "method": method,
                "url": url,
                "headers": normalize_headers(headers),
                "body": body,
                "timeout_ms": timeout_ms,
            }
        )
        if not self.responses:
            raise AssertionError("fake HTTP client has no queued responses")
        return self.responses.pop(0)


def response(status: int, body: str, headers: list[Header] | None = None) -> Response:
    """Build a queued fake HTTP response."""

    return Response(status=status, headers=headers or [], body=body)
