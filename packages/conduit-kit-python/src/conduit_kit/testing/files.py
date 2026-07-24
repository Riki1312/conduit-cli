from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class FakeFileReader:
    """In-memory fake for provider code that accepts a file reader."""

    files: dict[str, str] = field(default_factory=dict)

    def read_text(self, path: str) -> str:
        try:
            return self.files[path]
        except KeyError as error:
            raise FileNotFoundError(path) from error

    def read_optional_text(self, path: str) -> str | None:
        return self.files.get(path)
