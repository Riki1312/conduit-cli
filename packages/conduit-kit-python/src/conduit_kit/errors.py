from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
from typing import Any


class ProviderErrorKind(Enum):
    """Conduit provider error kinds shared by provider contracts."""

    AUTH_REQUIRED = "AuthRequired"
    INVALID_REQUEST = "InvalidRequest"
    PERMISSION_DENIED = "PermissionDenied"
    UNAVAILABLE = "Unavailable"
    UNSUPPORTED = "Unsupported"
    INTERNAL = "Internal"


@dataclass(frozen=True)
class CapabilityError(Exception):
    """Failure returned by a Conduit host capability import."""

    message: str

    def __str__(self) -> str:
        return self.message


def provider_error(
    provider: Any,
    kind: ProviderErrorKind,
    message: str,
    details: str | None = None,
    source: str | None = None,
):
    """Build an `Err(ProviderError)` for a generated provider binding module."""

    from componentize_py_types import Err

    variant = getattr(provider, f"ProviderErrorKind_{kind.value}")()
    return Err(
        provider.ProviderError(
            kind=variant,
            message=message,
            details=details,
            source=source,
        )
    )
