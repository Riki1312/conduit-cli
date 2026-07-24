from __future__ import annotations

from typing import Any

from conduit_kit.errors import CapabilityError


class FileReader:
    """Client for Conduit's project-scoped `file-read-v1` capability."""

    def __init__(self, binding: Any):
        self._binding = binding

    def read_text(self, path: str) -> str:
        """Read a project-allowed text file or raise `CapabilityError`."""

        try:
            return self._binding.read_text(path)
        except Exception as error:
            raise CapabilityError(_file_error_message(error)) from error

    def read_optional_text(self, path: str) -> str | None:
        """Read text, returning `None` when the host reports not-found."""

        try:
            return self._binding.read_text(path)
        except Exception as error:
            if _is_not_found(self._binding, error):
                return None
            raise CapabilityError(_file_error_message(error)) from error


def _is_not_found(binding: Any, error: Exception) -> bool:
    kind = getattr(getattr(error, "value", None), "kind", None)
    not_found = getattr(binding, "FileReadErrorKind_NotFound", None)
    return not_found is not None and isinstance(kind, not_found)


def _file_error_message(error: Exception) -> str:
    return getattr(getattr(error, "value", None), "message", str(error))
