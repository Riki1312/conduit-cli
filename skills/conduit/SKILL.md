---
name: conduit
description: Use when working in a repository or workspace that has Conduit installed or configured, especially for running tests, inspecting failures, querying logs, looking up OpenAPI operations, reading constrained DB resources, checking git/worktree state, or validating plugins through compact structured Conduit output.
---

# Conduit

Conduit turns noisy developer tools into compact, structured facts. Prefer it
when a repo has `conduit` installed or a `.conduit/conduit.toml` config.

## First Checks

```bash
conduit about
conduit --help
```

Use command-specific help before guessing flags:

```bash
conduit test run gradle --help
conduit logs search --help
conduit openapi operation --help
conduit db read --help
```

## Tests

Prefer Conduit over raw Gradle when the goal is running tests or reading test
failures:

```bash
conduit test run gradle --tests SomeTest
conduit test run gradle --profile integration --tests '*SdkTest'
conduit test run gradle --task :service:test --tests SomeTest
conduit test run gradle --failed
conduit test run gradle --tests SomeTest --tail 40 --timeout 2m
conduit test last
conduit test failed --tail 20
conduit test rerun gradle
conduit test log --tail 80
conduit stats
```

Use raw build tools for non-test tasks that Conduit does not wrap, such as code
formatting, compilation diagnostics, dependency changes, or custom build tasks.
Prefer existing project profiles over inventing build flags. If a needed
profile is missing, use explicit mode/args only when project docs or the user
provide them.

## Provider Commands

Provider commands require project config and plugins. If a provider is missing,
report the setup issue instead of inventing backend-specific commands.

```bash
conduit plugin check --provider logs
conduit logs errors service-name --since 30m --limit 20
conduit logs search service-name --message 'known text' --limit 0
conduit logs wait service-name --since now --timeout 2m --message 'known text'

conduit plugin check --provider openapi
conduit openapi operation --service service-name --method GET --path /path
conduit openapi search --service service-name --query field_name

conduit plugin check --provider db
conduit db resources service-name --env test
conduit db describe service-name resource_name --env test
conduit db read service-name resource_name --id '<id>' --env test
```

Use `--json` for scripts or when another tool will parse the result. Use
`--jsonl` for streaming commands such as log watch.

## Safety

- Do not store secrets, cookies, tokens, usernames, or passwords in repo files.
- Treat provider commands as capability-scoped; project config controls what a
  plugin can read or call.
- For DB work, use Conduit's read-only resource commands. Do not fall back to
  raw SQL unless the user explicitly asks and the environment policy allows it.
- Preserve Conduit's compact output in summaries; mention raw log paths only
  when deeper inspection is needed.
