# Context System

This folder is lightweight shared memory for humans and agents working on
Conduit.

## Files

- `current.md`: current truth for active work. Read it at the start of a run and
  update it at the end.
- `decisions.md`: append-only log of meaningful product, architecture, and
  workflow decisions.
- `sessions/`: optional notes for substantial work sessions.

## Rules

- Keep entries short and concrete.
- Timestamp entries with local date/time.
- Separate `fact`, `hypothesis`, and `todo`.
- Link evidence when possible, such as file paths, commands, commits, and docs.
- Do not store secrets, credentials, cookies, tokens, or private customer data.
