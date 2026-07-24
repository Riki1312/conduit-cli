from __future__ import annotations

from typing import Any

from conduit_kit.errors import CapabilityError


class SecretStore:
    """Client for Conduit's user-scoped `secret-store-v1` capability."""

    def __init__(self, binding: Any):
        self._binding = binding

    def read(self, name: str) -> str | None:
        try:
            return self._binding.read(name)
        except Exception as error:
            raise CapabilityError(_secret_error_message(error)) from error

    def write(self, name: str, value: str) -> bool:
        try:
            return self._binding.write(name, value)
        except Exception as error:
            raise CapabilityError(_secret_error_message(error)) from error

    def delete(self, name: str) -> bool:
        try:
            return self._binding.delete(name)
        except Exception as error:
            raise CapabilityError(_secret_error_message(error)) from error


def _secret_error_message(error: Exception) -> str:
    return getattr(getattr(error, "value", None), "message", str(error))
