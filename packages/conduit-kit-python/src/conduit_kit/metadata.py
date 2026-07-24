from __future__ import annotations

from typing import Any, Sequence


def plugin_metadata(
    metadata: Any,
    *,
    id: str,
    version: str,
    providers: Sequence[str],
    protocol_version: str = "1",
):
    """Build a generated `PluginMetadata` record for a Conduit plugin."""

    return metadata.PluginMetadata(
        id=id,
        version=version,
        protocol_version=protocol_version,
        providers=list(providers),
    )
