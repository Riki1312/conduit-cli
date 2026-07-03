# Security

Conduit can interact with local repositories, build tools, plugin components,
HTTP services, and user-scoped secrets. Treat configuration and plugin
capabilities as security-sensitive.

## Reporting

Please report security issues privately to the repository owner. Do not open a
public issue with exploit details, credentials, tokens, cookies, customer data,
or private infrastructure information.

## Project Safety Rules

- Do not commit credentials, cookies, tokens, auth headers, or copied log
  payloads.
- Grant plugin capabilities narrowly: exact HTTP hosts, exact secret names, and
  project-local file paths.
- Keep company-specific integrations outside the public core.
- Review generated artifacts before committing them. Plugin build outputs and
  caches should normally remain ignored.
- Prefer fixture data in tests and documentation.
