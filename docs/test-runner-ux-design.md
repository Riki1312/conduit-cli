# Test Runner UX Design

Conduit test commands should make noisy build tools usable as precise
engineering primitives. The current Gradle runner already improves the final
summary, but dogfooding against several Gradle repositories exposed gaps around
long runs, environment setup, no-source tasks, and rerun state.

This document defines the next design direction before implementation.

## Dogfooding Observations

### Modern Gradle Services

Recent Gradle services are the happy path:

- Focused unit tests complete quickly.
- JUnit XML is fresh.
- The compact summary is enough:
  `mode`, `command`, `exit_code`, `report_status`, `status`, `failures`, and
  `sources`.
- Full Gradle output stays available through `test log`.

This confirms the core model is useful when Gradle produces normal JUnit XML.

### Older Java 8 Gradle Services

Older Java 8 Gradle services exposed a different set of constraints:

- The repo uses Gradle 4.10.2, Java 8 source/target, and JUnit 4.
- Running with Java 21 can hang or take several minutes before a useful final
  answer is produced.
- Running with `JAVA_HOME` set to Corretto 8 works and produces a good compact
  result.
- A missing selector produces no JUnit XML, but the Gradle log includes a
  precise diagnostic:
  `No tests found for given includes: [...]`.

This shows that Conduit needs to manage runner environment and pre-report
failures as first-class concerns.

### Multi-Module SDK Repositories

Multi-module SDK-style repositories exposed a no-source case:

- A subproject test task completed successfully with Gradle `NO-SOURCE`.
- No JUnit XML report was produced.
- Conduit returned a data error because the inferred report path did not exist.

This should be a structured successful run summary, not an error.

## Goals

- Keep Conduit product-neutral. Do not encode company-specific test flags, Java
  paths, or build conventions in core behavior.
- Preserve compact final output as the default.
- Add enough process visibility that agents can distinguish slow, stuck,
  compiling, resolving, and failed-before-report states.
- Make Gradle profiles capable of describing project defaults without becoming
  a workflow engine.
- Avoid losing useful rerun selectors when a later runner failure does not
  produce test failures.

## Non-Goals

- Do not replace Gradle.
- Do not stream full build logs by default.
- Do not build a company-only Java profile system.
- Do not solve CI log ingestion in this pass.
- Do not add broad workflow orchestration commands.

## Proposed Behavior

### Incremental Log Capture

`test run gradle` should write stdout and stderr to the log file while the
process is running, not only after it exits.

Final output remains compact. Incremental capture enables other features:

- Timeouts can include the current log tail.
- Heartbeats can point to the log file.
- A future `test log --follow` command can tail active runs.

The log file should still start with the rendered command.

### Heartbeat Output

Add an opt-in heartbeat for interactive or agent long-running sessions:

```bash
conduit test run gradle --heartbeat 30s --tests SomeTest
```

Text output example:

```text
running: 30s
log_path: .conduit/state/logs/test-run-123.log
last_log: > Task :compileTestJava
```

JSON output should remain final-result-only for now. A future event-stream mode
can be designed separately.

Default heartbeat should be off to keep command output deterministic.

### Timeout

Add a runner timeout:

```bash
conduit test run gradle --timeout 2m --tests SomeTest
```

On timeout, Conduit should terminate the child process and return a structured
summary:

```text
runner: gradle
mode: unit
command: ./gradlew test --tests SomeTest
exit_code: timeout
duration_ms: 120000
log_path: .conduit/state/logs/test-run-123.log
report_path: build/test-results/test
report_status: missing
status: failed
failures: 0
sources: 0

log_tail: 40
log: ...
```

JSON should expose a stable `termination` field:

```json
{
  "termination": "timeout"
}
```

For normal process exits, `termination` should be `exit`.

### No-Source And No-Test Outcomes

Conduit should distinguish:

- Gradle succeeded but produced no report because the task had no tests.
- Gradle failed before report generation.
- Gradle succeeded and reused existing reports.

Proposed fields:

```text
report_status: missing
test_outcome: no_source
status: passed
```

Initial detection can be Gradle-log based:

- `> Task ... NO-SOURCE`
- `BUILD SUCCESSFUL`
- no fresh or existing JUnit XML reports

This is heuristic but valuable. The output should make that explicit through a
neutral `test_outcome` value rather than overloading `status`.

Proposed `test_outcome` values:

- `executed`
- `no_source`
- `no_matching_tests`
- `runner_failed`
- `unknown`

`status` remains the pass/fail process result used for exit code decisions.

### Profile Environment

Profiles should support project-defined environment variables:

```toml
[test.gradle.profiles.unit-java8]
task = "test"
mode = "unit"

[test.gradle.profiles.unit-java8.env]
JAVA_HOME = "/path/to/jdk8"
```

Command-line environment should still win if the user already set a variable.
This avoids surprising overrides.

A later enhancement can support variable interpolation:

```toml
JAVA_HOME = "${JAVA8_HOME}"
```

For the first implementation, either reject unresolved `${...}` values or pass
them literally. Rejecting is clearer.

### Preflight Checks

Conduit should eventually support lightweight preflight facts, but this can
come after profile env support.

Useful Java/Gradle preflight facts:

- `java.version`
- `JAVA_HOME`
- Gradle wrapper version
- inferred report path
- selected profile

Potential command:

```bash
conduit test preflight gradle --profile unit-java8
```

Do not block normal `test run gradle` on preflight unless a required configured
value is invalid.

### Rerun State Separation

Conduit currently stores parsed failures in
`.conduit/state/last-test-failures.json`. A runner failure with no parsed
failure selectors can overwrite that state with an empty failure set.

Separate state into:

- `last-test-run.json`: every run summary, including runner failures and
  no-source outcomes.
- `last-test-failures.json`: only updated when Conduit has parsed rerunnable
  failure selectors.

`test failed` should continue to show rerunnable parsed failures.

Add a future command for the last run:

```bash
conduit test last
```

This keeps `--failed` useful after compile errors, missing selector errors, and
timeouts.

### Profile Discovery

This is deferred but should fit the model:

```bash
conduit test profiles
conduit test profiles --json
```

Static CLI help should describe `--profile`; project profile names should be
discovered by a command that reads config.

## Output Contract Changes

Extend `TestRunSummary` carefully. New fields should be additive:

- `termination`: `exit`, `timeout`, or `spawn_failed`
- `test_outcome`: `executed`, `no_source`, `no_matching_tests`,
  `runner_failed`, or `unknown`
- `profile`: optional profile name
- `log_tail`: unchanged

Existing fields remain:

- `runner`
- `mode`
- `command`
- `exit_code`
- `duration_ms`
- `log_path`
- `report_path`
- `report_status`
- `result`

Text output should keep high-signal fields near the top:

```text
runner: gradle
profile: unit-java8
mode: unit
termination: exit
test_outcome: executed
command: ./gradlew test --tests SomeTest
exit_code: 0
duration_ms: 33047
log_path: .conduit/state/logs/test-run-123.log
report_path: build/test-results/test
report_status: fresh
status: passed
failures: 0
sources: 1
```

## Implementation Plan

1. Incremental log writer
   - Replace `Command::output()` with spawned child processes.
   - Capture stdout/stderr concurrently.
   - Write the log file during execution.
   - Preserve current final output for normal pass/fail cases.

2. Timeout
   - Add `--timeout <duration>`.
   - Parse simple durations like `30s`, `2m`, and `1h`.
   - Kill the child process on timeout.
   - Return structured timeout summaries with bounded log tails.

3. No-source and no-matching-tests classification
   - Add `test_outcome`.
   - Classify successful no-report Gradle runs as `no_source` when logs support
     that inference.
   - Classify missing-selector failures as `no_matching_tests` when Gradle logs
     include the known diagnostic.

4. Profile env
   - Extend `[test.gradle.profiles.<name>]` with an `env` table.
   - Apply profile env to the child process.
   - Reject unresolved interpolation syntax for now.

5. Rerun state separation
   - Add `last-test-run.json`.
   - Only update `last-test-failures.json` when parsed failures contain
     rerunnable selectors.
   - Add tests for compile failure, missing selector, timeout, and no-source
     cases.

6. Optional heartbeat
   - Add `--heartbeat <duration>` after incremental logging is in place.
   - Keep disabled by default.
   - Print compact progress lines only in text mode.

7. Profile discovery
   - Add `conduit test profiles`.
   - Include profile names, runner, task, report path, mode, arg count, and env
     key count in text output.
   - Include exact args and env keys in JSON.

## Open Questions

- Should profile env override existing process env, or should existing env
  always win? Current recommendation: existing env wins.
- Should `test run gradle` have a default timeout? Current recommendation: no
  default timeout until we have enough dogfooding data.
- Should heartbeat output be text-only, or should JSON have an event mode?
  Current recommendation: text-only heartbeat now, separate JSON event stream
  later.
- Should `no_matching_tests` exit code mirror Gradle's non-zero exit code?
  Current recommendation: yes.
