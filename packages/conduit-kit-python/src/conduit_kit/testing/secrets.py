from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class FakeSecretStore:
    """In-memory fake for provider code that accepts a secret store."""

    values: dict[str, str] = field(default_factory=dict)

    def read(self, name: str) -> str | None:
        return self.values.get(name)

    def write(self, name: str, value: str) -> bool:
        self.values[name] = value
        return True

    def delete(self, name: str) -> bool:
        return self.values.pop(name, None) is not None
