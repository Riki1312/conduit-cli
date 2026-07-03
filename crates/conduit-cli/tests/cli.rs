use std::fs;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static STATE_DIR_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[test]
fn about_command_prints_compact_fields() {
    let output = conduit_command()
        .arg("about")
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("name: conduit\n"));
    assert!(stdout.contains("purpose: agent-first developer operations CLI\n"));
}

#[test]
fn json_flag_prints_compact_json() {
    let output = conduit_command()
        .args(["about", "--json"])
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.starts_with("{\"name\":\"conduit\","));
    assert!(stdout.ends_with("\"purpose\":\"agent-first developer operations CLI\"}\n"));
}

#[test]
fn unknown_command_exits_with_usage_error() {
    let output = conduit_command()
        .arg("missing")
        .output()
        .expect("run conduit");

    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8(output.stderr).expect("stderr is utf8");
    assert!(stderr.contains("error: unknown command `missing`\n"));
    assert!(stderr.contains("hint: run `conduit help` for available commands\n"));
}

#[test]
fn help_flag_exits_successfully() {
    let output = conduit_command()
        .args(["test", "run", "gradle", "--help"])
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    let stderr = String::from_utf8(output.stderr).expect("stderr is utf8");
    assert!(stdout.contains("Usage: conduit test run gradle"));
    assert!(stdout.contains("--timeout <TIMEOUT>"));
    assert!(stderr.is_empty());
}

#[test]
fn git_status_prints_compact_repo_state() {
    let output = conduit_command()
        .args(["git", "status"])
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("repo: "));
    assert!(stdout.contains("branch: "));
    assert!(stdout.contains("dirty: "));
    assert!(stdout.contains("root: "));
}

#[test]
fn git_status_prints_json_repo_state() {
    let output = conduit_command()
        .args(["git", "status", "--json"])
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    let payload: serde_json::Value = serde_json::from_str(&stdout).expect("valid json");
    assert!(
        payload["repo"]
            .as_str()
            .is_some_and(|repo| !repo.is_empty())
    );
    assert!(
        payload["branch"]
            .as_str()
            .is_some_and(|branch| !branch.is_empty())
    );
    assert!(payload["dirty"].is_boolean());
}

#[test]
fn db_resources_prints_compact_resources() {
    let output = fixture_command()
        .args(["db", "resources", "checkout-service", "--env", "test"])
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("provider: fixture-db\n"));
    assert!(stdout.contains("service: checkout-service\n"));
    assert!(stdout.contains("environment: test\n"));
    assert!(stdout.contains("resources: 1\n"));
    assert!(stdout.contains("resource: payment_account\n"));
    assert!(!stdout.contains("description:"));
}

#[test]
fn db_describe_prints_minimal_resource_shape() {
    let output = fixture_command()
        .args([
            "db",
            "describe",
            "checkout-service",
            "payment_account",
            "--env",
            "test",
        ])
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("provider: fixture-db\n"));
    assert!(stdout.contains("resource: payment_account\n"));
    assert!(stdout.contains("id_field: id\n"));
    assert!(stdout.contains("field: currency\n"));
    assert!(!stdout.contains("sensitive:"));
    assert!(!stdout.contains("description:"));
}

#[test]
fn db_read_prints_compact_records_by_id() {
    let output = fixture_command()
        .args([
            "db",
            "read",
            "checkout-service",
            "payment_account",
            "--id",
            "acc_123",
            "--env",
            "test",
        ])
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("status: ok\n"));
    assert!(stdout.contains("matched: 1\nshown: 1\n"));
    assert!(stdout.contains("record:\n"));
    assert!(stdout.contains("  id: acc_123\n"));
    assert!(stdout.contains("  status: ACTIVE\n"));
    assert!(stdout.contains("  currency: EUR\n"));
}

#[test]
fn db_read_prints_json_records_by_filter() {
    let output = fixture_command()
        .args([
            "db",
            "read",
            "checkout-service",
            "payment_account",
            "--filter",
            "status=DISABLED",
            "--json",
        ])
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    let payload: serde_json::Value = serde_json::from_str(&stdout).expect("valid json");
    assert_eq!(payload["provider"], "fixture-db");
    assert_eq!(payload["matched"], 1);
    assert_eq!(payload["records"][0]["id"], "acc_456");
}

#[test]
fn db_config_rejects_fixture_fallback_when_project_config_has_no_db() {
    let project = project_dir("db_no_provider_no_fixture_fallback");
    write_gradle_profile_project_config(&project);

    let output = conduit_command()
        .args(["db", "resources", "checkout-service"])
        .current_dir(&project)
        .output()
        .expect("run conduit");

    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8(output.stderr).expect("stderr is utf8");
    assert!(stderr.contains("error: db provider is not configured in .conduit/conduit.toml\n"));
}

#[test]
fn db_config_reports_missing_provider_plugin() {
    let project = project_dir("db_missing_provider_plugin");
    fs::create_dir_all(project.join(".conduit")).expect("create config dir");
    fs::write(
        project.join(".conduit/conduit.toml"),
        r#"
        [db]
        provider = "company"
        "#,
    )
    .expect("write config");

    let output = conduit_command()
        .args(["db", "resources", "checkout-service"])
        .current_dir(&project)
        .output()
        .expect("run conduit");

    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8(output.stderr).expect("stderr is utf8");
    assert!(stderr.contains("error: db provider `company` is not configured as a plugin\n"));
}

#[test]
fn openapi_operation_prints_compact_operation() {
    let output = fixture_command()
        .args([
            "openapi",
            "operation",
            "--service",
            "catalog-service",
            "--method",
            "GET",
            "--path",
            "/items",
            "--environment",
            "staging",
        ])
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("service: catalog-service\n"));
    assert!(stdout.contains("method: GET\n"));
    assert!(stdout.contains("path: /items\n"));
    assert!(stdout.contains("environment: staging\n"));
    assert!(stdout.contains("operation_id: listItems\n"));
    assert!(stdout.contains("source: fixture://catalog-service/openapi.json\n"));
}

#[test]
fn openapi_operation_prints_json_operation() {
    let output = fixture_command()
        .args([
            "openapi",
            "operation",
            "--service",
            "catalog-service",
            "--method",
            "GET",
            "--path",
            "/items",
            "--json",
        ])
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.starts_with("{\"service\":\"catalog-service\""));
    assert!(stdout.contains("\"operation_id\":\"listItems\""));
    assert!(stdout.contains("\"response_schema_json\":\"{\\\"type\\\":\\\"object\\\"}\""));
}

#[test]
fn openapi_list_prints_compact_operations() {
    let output = fixture_command()
        .args(["openapi", "list", "--service", "catalog-service"])
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("service: catalog-service\n"));
    assert!(stdout.contains("operations: 2\n"));
    assert!(stdout.contains("operation: GET /items\n"));
    assert!(stdout.contains("operation: GET /items/{item_id}/prices\n"));
}

#[test]
fn openapi_search_prints_matching_operations() {
    let output = fixture_command()
        .args([
            "openapi",
            "search",
            "--service",
            "catalog-service",
            "--query",
            "item_id",
            "--method",
            "GET",
        ])
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("operations: 1\n"));
    assert!(stdout.contains("operation: GET /items/{item_id}/prices\n"));
}

#[test]
fn logs_search_prints_compact_events() {
    let output = fixture_command()
        .args([
            "logs",
            "search",
            "fixture-service",
            "--env",
            "staging",
            "--date",
            "2026-05-22",
            "--level",
            "ERROR",
            "--cid",
            "CID-123",
            "--logger",
            "FixturePaymentService",
        ])
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("status: ok\n"));
    assert!(stdout.contains("provider: fixture-logs\n"));
    assert!(stdout.contains("service: fixture-service\n"));
    assert!(stdout.contains("environment: staging\n"));
    assert!(stdout.contains("source: date 2026-05-22\n"));
    assert!(stdout.contains("matches: 1\nshown: 1\n"));
    assert!(stdout.contains("logger: FixturePaymentService\n"));
    assert!(stdout.contains("message: ACCOUNT_NOT_ACTIVATED\n"));
    assert!(!stdout.contains("stack_trace:"));
}

#[test]
fn logs_errors_include_stack_traces() {
    let output = fixture_command()
        .args(["logs", "errors", "fixture-service", "--date", "2026-05-22"])
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("matches: 1\nshown: 1\n"));
    assert!(stdout.contains("level: ERROR\n"));
    assert!(stdout.contains("stack_trace: java.lang.IllegalStateException"));
}

#[test]
fn logs_search_excludes_known_noise() {
    let output = fixture_command()
        .args([
            "logs",
            "search",
            "fixture-service",
            "--date",
            "2026-05-22",
            "--exclude-message",
            "accepted",
            "--exclude-class",
            "NoisyClass",
            "--json",
        ])
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    let payload: serde_json::Value = serde_json::from_str(&stdout).expect("valid json");
    assert_eq!(payload["matches"], 1);
    assert_eq!(payload["logs"][0]["message"], "ACCOUNT_NOT_ACTIVATED");
}

#[test]
fn logs_search_prints_json_for_zero_matches() {
    let output = fixture_command()
        .args([
            "logs",
            "search",
            "fixture-service",
            "--message",
            "missing",
            "--date",
            "2026-05-22",
            "--json",
        ])
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.starts_with("{\"status\":\"ok\",\"provider\":\"fixture-logs\""));
    assert!(stdout.contains("\"matches\":0,\"shown\":0"));
    assert!(stdout.contains("\"logs\":[]"));
}

#[test]
fn logs_search_allows_zero_limit_for_count_only_queries() {
    let output = fixture_command()
        .args([
            "logs",
            "search",
            "fixture-service",
            "--date",
            "2026-05-22",
            "--limit",
            "0",
        ])
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("matches: 2\nshown: 0\n"));
    assert!(stdout.contains("diagnostic: count_only\n"));
    assert!(stdout.contains("hint: increase --limit to show matching logs\n"));
    assert!(!stdout.contains("diagnostic: query_truncated\n"));
    assert!(!stdout.contains("log:\n"));
}

#[test]
fn logs_auth_prints_redacted_status() {
    let output = fixture_command()
        .args(["logs", "auth", "--env", "staging"])
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("status: ok\n"));
    assert!(stdout.contains("provider: fixture-logs\n"));
    assert!(stdout.contains("environment: staging\n"));
    assert!(stdout.contains("destination: fixture://logs/auth\n"));
}

#[test]
fn logs_auth_reads_secret_from_stdin_without_rendering_it() {
    let mut child = fixture_command()
        .args(["logs", "auth", "--secret-stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn conduit");
    {
        use std::io::Write as _;

        let stdin = child.stdin.as_mut().expect("stdin is piped");
        stdin
            .write_all(b"security_authentication=value")
            .expect("write stdin");
    }
    let output = child.wait_with_output().expect("wait for conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("status: ok\n"));
    assert!(!stdout.contains("security_authentication=value"));
}

#[test]
fn logs_auth_check_prints_validation_status() {
    let output = fixture_command()
        .args(["logs", "auth", "--env", "staging", "--check", "--json"])
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    let payload: serde_json::Value = serde_json::from_str(&stdout).expect("valid json");
    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["provider"], "fixture-logs");
    assert_eq!(payload["environment"], "staging");
    assert_eq!(payload["diagnostics"][0]["kind"], "auth_valid");
}

#[test]
fn logs_auth_check_rejects_secret_stdin() {
    let output = fixture_command()
        .args(["logs", "auth", "--check", "--secret-stdin"])
        .output()
        .expect("run conduit");

    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8(output.stderr).expect("stderr is utf8");
    assert!(
        stderr.contains("error: `logs auth --check` cannot be combined with `--secret-stdin`\n")
    );
}

#[test]
fn logs_search_rejects_ambiguous_time_filters() {
    let output = fixture_command()
        .args([
            "logs",
            "search",
            "fixture-service",
            "--since",
            "15m",
            "--from",
            "2026-05-22T10:00:00Z",
        ])
        .output()
        .expect("run conduit");

    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8(output.stderr).expect("stderr is utf8");
    assert!(stderr.contains("error: `--since` cannot be combined with `--from` or `--to`\n"));
}

#[test]
fn logs_wait_exits_successfully_when_a_matching_event_exists() {
    let output = fixture_command()
        .args([
            "logs",
            "wait",
            "fixture-service",
            "--date",
            "2026-05-22",
            "--message",
            "ACCOUNT_NOT_ACTIVATED",
            "--timeout",
            "20ms",
            "--interval",
            "10ms",
        ])
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("status: matched\n"));
    assert!(stdout.contains("matches: 1\nshown: 1\n"));
    assert!(stdout.contains("message: ACCOUNT_NOT_ACTIVATED\n"));
}

#[test]
fn logs_wait_exits_nonzero_on_timeout() {
    let output = fixture_command()
        .args([
            "logs",
            "wait",
            "missing-service",
            "--timeout",
            "20ms",
            "--interval",
            "10ms",
        ])
        .output()
        .expect("run conduit");

    assert_eq!(output.status.code(), Some(1));

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("status: timeout\n"));
    assert!(stdout.contains("matches: 0\nshown: 0\n"));
}

#[test]
fn logs_watch_prints_jsonl_events_until_timeout() {
    let output = fixture_command()
        .args([
            "logs",
            "watch",
            "fixture-service",
            "--date",
            "2026-05-22",
            "--timeout",
            "20ms",
            "--interval",
            "10ms",
            "--jsonl",
        ])
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    let lines = stdout.lines().collect::<Vec<_>>();
    assert!(
        lines
            .iter()
            .any(|line| line.contains(r#""event":"started""#))
    );
    assert!(lines.iter().any(|line| {
        line.contains(r#""event":"log""#) && line.contains(r#""message":"payment accepted""#)
    }));
    assert!(lines.iter().any(|line| {
        line.contains(r#""event":"log""#) && line.contains(r#""message":"ACCOUNT_NOT_ACTIVATED""#)
    }));
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.contains(r#""event":"log""#))
            .count(),
        2
    );
    assert!(
        stdout.find("payment accepted").expect("info log exists")
            < stdout
                .find("ACCOUNT_NOT_ACTIVATED")
                .expect("error log exists")
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains(r#""event":"heartbeat""#))
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains(r#""event":"finished""#))
    );
}

#[test]
fn logs_config_reports_missing_provider_plugin() {
    let project = project_dir("logs_missing_provider_plugin");
    fs::create_dir_all(project.join(".conduit")).expect("create config dir");
    fs::write(
        project.join(".conduit/conduit.toml"),
        r#"
        [logs]
        provider = "company"
        "#,
    )
    .expect("write config");

    let output = conduit_command()
        .args(["logs", "search", "fixture-service"])
        .current_dir(&project)
        .output()
        .expect("run conduit");

    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8(output.stderr).expect("stderr is utf8");
    assert!(stderr.contains("error: logs provider `company` is not configured as a plugin\n"));
}

#[test]
fn logs_config_uses_ancestor_provider_when_nearest_config_has_no_logs() {
    let workspace = project_dir("logs_ancestor_provider");
    let project = workspace.join("repos/service");
    let plugin_dir = workspace.join(".conduit/plugins");
    fs::create_dir_all(&plugin_dir).expect("create plugin dir");
    fs::write(plugin_dir.join("company.wasm"), logs_provider_component())
        .expect("write plugin component");
    write_gradle_profile_project_config(&project);
    fs::write(
        workspace.join(".conduit/conduit.toml"),
        r#"
        [plugins.company]
        path = ".conduit/plugins/company.wasm"

        [logs]
        provider = "company"
        default_environment = "staging"
        "#,
    )
    .expect("write workspace config");

    let output = conduit_command()
        .args(["logs", "search", "fixture-service", "--date", "2026-05-22"])
        .current_dir(&project)
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("provider: fixture-logs\n"));
    assert!(stdout.contains("environment: staging\n"));
}

#[test]
fn logs_config_rejects_fixture_fallback_when_project_config_has_no_logs() {
    let project = project_dir("logs_no_provider_no_fixture_fallback");
    write_gradle_profile_project_config(&project);

    let output = conduit_command()
        .args(["logs", "search", "fixture-service"])
        .current_dir(&project)
        .output()
        .expect("run conduit");

    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8(output.stderr).expect("stderr is utf8");
    assert!(stderr.contains("error: logs provider is not configured in .conduit/conduit.toml\n"));
}

#[test]
fn logs_config_reports_missing_plugin_file() {
    let project = project_dir("logs_missing_plugin_file");
    fs::create_dir_all(project.join(".conduit")).expect("create config dir");
    fs::write(
        project.join(".conduit/conduit.toml"),
        r#"
        [plugins.company]
        path = ".conduit/plugins/company.wasm"

        [logs]
        provider = "company"
        "#,
    )
    .expect("write config");

    let output = conduit_command()
        .args(["logs", "search", "fixture-service"])
        .current_dir(&project)
        .output()
        .expect("run conduit");

    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8(output.stderr).expect("stderr is utf8");
    assert!(stderr.contains("error: failed to load component ./.conduit/plugins/company.wasm:"));
}

#[test]
fn plugin_check_validates_openapi_provider_and_prints_metadata() {
    let project = project_dir("plugin_check_openapi_provider");
    let plugin_dir = project.join(".conduit/plugins");
    fs::create_dir_all(&plugin_dir).expect("create plugin dir");
    let plugin_path = plugin_dir.join("company.wasm");
    fs::write(&plugin_path, openapi_provider_component()).expect("write plugin component");

    let output = conduit_command()
        .args([
            "plugin",
            "check",
            "--path",
            plugin_path.to_str().expect("utf8 plugin path"),
        ])
        .current_dir(&project)
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("status: ok\n"));
    assert!(stdout.contains("id: fixture-openapi\n"));
    assert!(stdout.contains("version: 0.1.0\n"));
    assert!(stdout.contains("protocol_version: 1\n"));
    assert!(stdout.contains("provider: openapi-provider-v1\n"));
    assert!(project.join(".conduit/state/wasmtime-cache").is_dir());
}

#[test]
fn plugin_check_prints_json_metadata() {
    let project = project_dir("plugin_check_json");
    let plugin_dir = project.join(".conduit/plugins");
    fs::create_dir_all(&plugin_dir).expect("create plugin dir");
    let plugin_path = plugin_dir.join("company.wasm");
    fs::write(&plugin_path, openapi_provider_component()).expect("write plugin component");

    let output = conduit_command()
        .args([
            "plugin",
            "check",
            "--path",
            plugin_path.to_str().expect("utf8 plugin path"),
            "--json",
        ])
        .current_dir(&project)
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.starts_with("{\"status\":\"ok\""));
    assert!(stdout.contains("\"id\":\"fixture-openapi\""));
    assert!(stdout.contains("\"providers\":[\"openapi-provider-v1\"]"));
}

#[test]
fn plugin_check_validates_logs_provider_and_prints_metadata() {
    let project = project_dir("plugin_check_logs_provider");
    let plugin_dir = project.join(".conduit/plugins");
    fs::create_dir_all(&plugin_dir).expect("create plugin dir");
    let plugin_path = plugin_dir.join("logs.wasm");
    fs::write(&plugin_path, logs_provider_component()).expect("write plugin component");

    let output = conduit_command()
        .args([
            "plugin",
            "check",
            "--path",
            plugin_path.to_str().expect("utf8 plugin path"),
            "--provider",
            "logs",
        ])
        .current_dir(&project)
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("status: ok\n"));
    assert!(stdout.contains("id: fixture-logs\n"));
    assert!(stdout.contains("version: 0.1.0\n"));
    assert!(stdout.contains("protocol_version: 1\n"));
    assert!(stdout.contains("provider: logs-provider-v1\n"));
}

#[test]
fn plugin_check_validates_configured_openapi_provider() {
    let project = project_dir("plugin_check_configured_openapi_provider");
    let plugin_dir = project.join(".conduit/plugins");
    fs::create_dir_all(&plugin_dir).expect("create plugin dir");
    fs::write(
        project.join(".conduit/conduit.toml"),
        r#"
        [plugins.company]
        path = ".conduit/plugins/company.wasm"

        [openapi]
        provider = "company"
        "#,
    )
    .expect("write config");
    fs::write(
        plugin_dir.join("company.wasm"),
        openapi_provider_component(),
    )
    .expect("write plugin component");

    let output = conduit_command()
        .args(["plugin", "check", "--provider", "openapi"])
        .current_dir(&project)
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("status: ok\n"));
    assert!(stdout.contains("path: ./.conduit/plugins/company.wasm\n"));
    assert!(stdout.contains("id: fixture-openapi\n"));
    assert!(stdout.contains("provider: openapi-provider-v1\n"));
}

#[test]
fn plugin_check_validates_configured_logs_provider() {
    let project = project_dir("plugin_check_configured_logs_provider");
    let plugin_dir = project.join(".conduit/plugins");
    fs::create_dir_all(&plugin_dir).expect("create plugin dir");
    fs::write(
        project.join(".conduit/conduit.toml"),
        r#"
        [plugins.company]
        path = ".conduit/plugins/company.wasm"

        [logs]
        provider = "company"
        "#,
    )
    .expect("write config");
    fs::write(plugin_dir.join("company.wasm"), logs_provider_component())
        .expect("write plugin component");

    let output = conduit_command()
        .args(["plugin", "check", "--provider", "logs"])
        .current_dir(&project)
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("status: ok\n"));
    assert!(stdout.contains("path: ./.conduit/plugins/company.wasm\n"));
    assert!(stdout.contains("id: fixture-logs\n"));
    assert!(stdout.contains("provider: logs-provider-v1\n"));
}

#[test]
fn openapi_operation_uses_configured_plugin_provider() {
    let project = project_dir("openapi_plugin_provider");
    let plugin_dir = project.join(".conduit/plugins");
    fs::create_dir_all(&plugin_dir).expect("create plugin dir");
    fs::write(
        project.join(".conduit/conduit.toml"),
        r#"
        [plugins.company]
        path = ".conduit/plugins/company.wasm"

        [openapi]
        provider = "company"
        "#,
    )
    .expect("write config");
    fs::write(
        plugin_dir.join("company.wasm"),
        openapi_provider_component(),
    )
    .expect("write plugin component");

    let output = conduit_command()
        .args([
            "openapi",
            "operation",
            "--service",
            "fixture-service",
            "--method",
            "GET",
            "--path",
            "/fixture",
        ])
        .current_dir(&project)
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("service: fixture-service\n"));
    assert!(stdout.contains("method: GET\n"));
    assert!(stdout.contains("path: /fixture\n"));
    assert!(stdout.contains("operation_id: fixtureOperation\n"));
    assert!(stdout.contains("summary: Fixture operation\n"));
}

#[test]
fn openapi_config_uses_ancestor_provider_when_nearest_config_has_no_openapi() {
    let workspace = project_dir("openapi_ancestor_provider");
    let project = workspace.join("repos/service");
    let plugin_dir = workspace.join(".conduit/plugins");
    fs::create_dir_all(&plugin_dir).expect("create plugin dir");
    fs::write(
        plugin_dir.join("company.wasm"),
        openapi_provider_component(),
    )
    .expect("write plugin component");
    write_gradle_profile_project_config(&project);
    fs::write(
        workspace.join(".conduit/conduit.toml"),
        r#"
        [plugins.company]
        path = ".conduit/plugins/company.wasm"

        [openapi]
        provider = "company"
        "#,
    )
    .expect("write workspace config");

    let output = conduit_command()
        .args([
            "openapi",
            "operation",
            "--service",
            "fixture-service",
            "--method",
            "GET",
            "--path",
            "/fixture",
        ])
        .current_dir(&project)
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("operation_id: fixtureOperation\n"));
    assert!(stdout.contains("summary: Fixture operation\n"));
}

#[test]
fn openapi_config_rejects_fixture_fallback_when_project_config_has_no_openapi() {
    let project = project_dir("openapi_no_provider_no_fixture_fallback");
    write_gradle_profile_project_config(&project);

    let output = conduit_command()
        .args([
            "openapi",
            "operation",
            "--service",
            "catalog-service",
            "--method",
            "GET",
            "--path",
            "/items",
        ])
        .current_dir(&project)
        .output()
        .expect("run conduit");

    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8(output.stderr).expect("stderr is utf8");
    assert!(
        stderr.contains("error: openapi provider is not configured in .conduit/conduit.toml\n")
    );
}

#[test]
fn openapi_config_reports_missing_provider_plugin() {
    let project = project_dir("openapi_missing_provider_plugin");
    fs::create_dir_all(project.join(".conduit")).expect("create config dir");
    fs::write(
        project.join(".conduit/conduit.toml"),
        r#"
        [openapi]
        provider = "company"
        "#,
    )
    .expect("write config");

    let output = conduit_command()
        .args(["openapi", "list", "--service", "fixture-service"])
        .current_dir(&project)
        .output()
        .expect("run conduit");

    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8(output.stderr).expect("stderr is utf8");
    assert!(stderr.contains("error: openapi provider `company` is not configured as a plugin\n"));
}

#[test]
fn openapi_config_reports_missing_plugin_file() {
    let project = project_dir("openapi_missing_plugin_file");
    fs::create_dir_all(project.join(".conduit")).expect("create config dir");
    fs::write(
        project.join(".conduit/conduit.toml"),
        r#"
        [plugins.company]
        path = ".conduit/plugins/company.wasm"

        [openapi]
        provider = "company"
        "#,
    )
    .expect("write config");

    let output = conduit_command()
        .args(["openapi", "list", "--service", "fixture-service"])
        .current_dir(&project)
        .output()
        .expect("run conduit");

    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8(output.stderr).expect("stderr is utf8");
    assert!(stderr.contains("error: failed to load component ./.conduit/plugins/company.wasm:"));
}

#[test]
fn openapi_config_rejects_unsupported_plugin_protocol() {
    let project = project_dir("openapi_unsupported_plugin_protocol");
    write_openapi_plugin_project_config(&project);
    fs::write(
        project.join(".conduit/plugins/company.wasm"),
        openapi_provider_component_with_fixture(&openapi_provider_fixture().replace(
            r#"(data (i32.const 40) "1")"#,
            r#"(data (i32.const 40) "2")"#,
        )),
    )
    .expect("write plugin component");

    let output = conduit_command()
        .args(["openapi", "list", "--service", "fixture-service"])
        .current_dir(&project)
        .output()
        .expect("run conduit");

    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8(output.stderr).expect("stderr is utf8");
    assert!(stderr.contains(
        "error: plugin `fixture-openapi` uses unsupported protocol version `2`; expected `1`\n"
    ));
}

#[test]
fn openapi_config_rejects_plugin_without_openapi_provider_metadata() {
    let project = project_dir("openapi_missing_provider_metadata");
    write_openapi_plugin_project_config(&project);
    fs::write(
        project.join(".conduit/plugins/company.wasm"),
        openapi_provider_component_with_fixture(
            &openapi_provider_fixture()
                .replace(
                    r#"(data (i32.const 48) "openapi-provider-v1")"#,
                    r#"(data (i32.const 48) "logs-provider-v1")"#,
                )
                .replace("i32.const 19\n    i32.store", "i32.const 16\n    i32.store"),
        ),
    )
    .expect("write plugin component");

    let output = conduit_command()
        .args(["openapi", "list", "--service", "fixture-service"])
        .current_dir(&project)
        .output()
        .expect("run conduit");

    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8(output.stderr).expect("stderr is utf8");
    assert!(stderr.contains(
        "error: plugin `fixture-openapi` does not declare provider `openapi-provider-v1`\n"
    ));
}

#[test]
fn worktree_list_prints_compact_statuses() {
    let root = project_dir("worktree_list_compact");
    let repo = root.join("repo-one");
    init_git_repo(&repo);

    let output = conduit_command()
        .args([
            "worktree",
            "list",
            "--root",
            root.to_str().expect("utf8 root"),
        ])
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains(&format!("root: {}\n", root.to_string_lossy())));
    assert!(stdout.contains("worktrees: 1\n"));
    assert!(stdout.contains("worktree: repo-one\n"));
    assert!(stdout.contains("branch: "));
}

#[test]
fn worktree_list_prints_json_statuses() {
    let root = project_dir("worktree_list_json");
    let repo = root.join("repo-one");
    init_git_repo(&repo);

    let output = conduit_command()
        .args([
            "worktree",
            "list",
            "--root",
            root.to_str().expect("utf8 root"),
            "--json",
        ])
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.starts_with(&format!("{{\"root\":\"{}\"", root.to_string_lossy())));
    assert!(stdout.contains("\"worktrees\":["));
    assert!(stdout.contains("\"name\":\"repo-one\""));
}

#[test]
fn test_failures_command_prints_compact_failure_summary() {
    let state_dir = state_dir("compact_failure_summary");
    let output = conduit_command()
        .args(["test", "failures", "tests/fixtures/junit"])
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(
        stdout.contains("status: failed\ntests_ran: 4\ntests_passed: 2\nfailures: 2\nsources: 2\n")
    );
    assert!(stdout.contains("failure: com.example.PaymentServiceTest.createsPayment\n"));
    assert!(stdout.contains("message: expected:<200> but was:<500>\n"));
    assert!(stdout.contains("failure: com.example.PaymentServiceTest.refundsPayment\n"));
}

#[test]
fn test_failures_command_prints_json_summary() {
    let state_dir = state_dir("json_summary");
    let output = conduit_command()
        .args(["test", "failures", "tests/fixtures/junit", "--json"])
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.starts_with("{\"status\":\"failed\",\"tests_ran\":4,\"tests_passed\":2,"));
    assert!(stdout.contains("\"passed_selectors\":["));
    assert!(stdout.contains("\"failures\":["));
    assert!(stdout.contains("\"selector\":\"com.example.PaymentServiceTest.createsPayment\""));
    assert!(stdout.contains("\"sources\":["));
}

#[test]
fn test_failed_command_reads_last_failure_summary() {
    let state_dir = state_dir("read_last_failure_summary");

    let write_output = conduit_command()
        .args(["test", "failures", "tests/fixtures/junit"])
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert!(write_output.status.success());

    let read_output = conduit_command()
        .args(["test", "failed"])
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert!(read_output.status.success());

    let stdout = String::from_utf8(read_output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("report_path: tests/fixtures/junit\n"));
    assert!(
        stdout.contains("status: failed\ntests_ran: 4\ntests_passed: 2\nfailures: 2\nsources: 2\n")
    );
    assert!(stdout.contains("selectors: 2\n"));
    assert!(stdout.contains("selector: com.example.PaymentServiceTest.createsPayment\n"));
}

#[test]
fn test_failed_command_prints_json_state() {
    let state_dir = state_dir("json_state");

    let write_output = conduit_command()
        .args(["test", "failures", "tests/fixtures/junit"])
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert!(write_output.status.success());

    let read_output = conduit_command()
        .args(["test", "failed", "--json"])
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert!(read_output.status.success());

    let stdout = String::from_utf8(read_output.stdout).expect("stdout is utf8");
    assert!(stdout.starts_with("{\"version\":1,\"report_path\":\"tests/fixtures/junit\""));
    assert!(stdout.contains("\"summary\":{\"status\":\"failed\""));
}

#[test]
fn test_failed_command_prints_bounded_log_tail() {
    let project_dir = project_dir("failed_tail");
    let state_dir = state_dir("failed_tail");
    write_fake_gradlew(
        &project_dir,
        &format!(
            "printf 'line-one\nline-two\nline-three\n'\nmkdir -p build/test-results/test\ncat > build/test-results/test/TEST-sample.xml <<'EOF'\n{}EOF\nexit 1\n",
            failing_junit_report()
        ),
    );

    let run_output = conduit_command()
        .args(["test", "run", "gradle"])
        .current_dir(&project_dir)
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert_eq!(run_output.status.code(), Some(1));

    let failed_output = conduit_command()
        .args(["test", "failed", "--tail", "2"])
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert!(failed_output.status.success());

    let stdout = String::from_utf8(failed_output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("log_tail: 2\n"));
    assert!(stdout.contains("log: line-two\n"));
    assert!(stdout.contains("log: line-three\n"));
}

#[test]
fn test_rerun_gradle_command_reads_last_failure_selectors() {
    let state_dir = state_dir("rerun_gradle");

    let write_output = conduit_command()
        .args(["test", "failures", "tests/fixtures/junit"])
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert!(write_output.status.success());

    let rerun_output = conduit_command()
        .args(["test", "rerun", "gradle"])
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert!(rerun_output.status.success());

    let stdout = String::from_utf8(rerun_output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("runner: gradle\nselectors: 2\n"));
    assert!(
        stdout.contains(
            "command: ./gradlew test --tests com.example.PaymentServiceTest.createsPayment"
        )
    );
    assert!(stdout.contains("--tests com.example.PaymentServiceTest.refundsPayment\n"));
}

#[test]
fn test_rerun_gradle_command_prints_json() {
    let state_dir = state_dir("rerun_gradle_json");

    let write_output = conduit_command()
        .args(["test", "failures", "tests/fixtures/junit"])
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert!(write_output.status.success());

    let rerun_output = conduit_command()
        .args(["test", "rerun", "gradle", "--json"])
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert!(rerun_output.status.success());

    let stdout = String::from_utf8(rerun_output.stdout).expect("stdout is utf8");
    assert!(stdout.starts_with("{\"runner\":\"gradle\",\"executable\":\"./gradlew\""));
    assert!(stdout.contains("\"args\":[\"test\",\"--tests\""));
    assert!(stdout.contains("\"command\":\"./gradlew test --tests"));
}

#[test]
fn test_run_gradle_captures_output_and_reports_failures() {
    let project_dir = project_dir("run_gradle_failures");
    let state_dir = state_dir("run_gradle_failures");
    write_fake_gradlew(
        &project_dir,
        &format!(
            "echo running fake gradle\nmkdir -p build/test-results/test\ncat > build/test-results/test/TEST-sample.xml <<'EOF'\n{}EOF\nexit 1\n",
            failing_junit_report()
        ),
    );

    let output = conduit_command()
        .args([
            "test",
            "run",
            "gradle",
            "--tests",
            "com.example.PaymentServiceTest",
        ])
        .current_dir(&project_dir)
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert_eq!(output.status.code(), Some(1));

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("runner: gradle\n"));
    assert!(stdout.contains("mode: unit\n"));
    assert!(stdout.contains("command: ./gradlew test --tests com.example.PaymentServiceTest\n"));
    assert!(stdout.contains("exit_code: 1\n"));
    assert!(stdout.contains("log_path: "));
    assert!(stdout.contains("report_path: build/test-results/test\n"));
    assert!(stdout.contains("report_status: fresh\n"));
    assert!(
        stdout.contains("status: failed\ntests_ran: 1\ntests_passed: 0\nfailures: 1\nsources: 1\n")
    );

    let state = fs::read_to_string(format!("{state_dir}/last-test-failures.json"))
        .expect("last failures state exists");
    assert!(state.contains("com.example.PaymentServiceTest.createsPayment"));
}

#[test]
fn test_run_gradle_can_print_bounded_log_tail_on_failure() {
    let project_dir = project_dir("run_gradle_tail");
    let state_dir = state_dir("run_gradle_tail");
    write_fake_gradlew(
        &project_dir,
        "printf 'line-one\nline-two\nline-three\n'\nprintf 'line-four\n' >&2\nexit 1\n",
    );
    write_junit_report(&project_dir, failing_junit_report());

    let output = conduit_command()
        .args(["test", "run", "gradle", "--tail", "4"])
        .current_dir(&project_dir)
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert_eq!(output.status.code(), Some(1));

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("log_tail: 4\n"));
    assert!(stdout.contains("log: line-three\n"));
    assert!(stdout.contains("log: line-four\n"));
}

#[test]
fn test_run_gradle_ignores_stale_reports_when_runner_fails() {
    let project_dir = project_dir("run_gradle_stale_reports");
    let state_dir = state_dir("run_gradle_stale_reports");
    write_fake_gradlew(&project_dir, "echo wrapper failed\nexit 1\n");
    write_junit_report(&project_dir, passing_junit_report());

    let output = conduit_command()
        .args(["test", "run", "gradle", "--tail", "10"])
        .current_dir(&project_dir)
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert_eq!(output.status.code(), Some(1));

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("exit_code: 1\n"));
    assert!(stdout.contains("report_status: missing\n"));
    assert!(
        stdout.contains("status: failed\ntests_ran: 0\ntests_passed: 0\nfailures: 0\nsources: 0\n")
    );
    assert!(stdout.contains("log: wrapper failed\n"));
}

#[test]
fn test_run_gradle_uses_existing_reports_when_runner_succeeds_without_rewriting() {
    let project_dir = project_dir("run_gradle_existing_reports");
    let state_dir = state_dir("run_gradle_existing_reports");
    write_fake_gradlew(&project_dir, "echo up to date\nexit 0\n");
    write_junit_report(&project_dir, passing_junit_report());

    let output = conduit_command()
        .args(["test", "run", "gradle"])
        .current_dir(&project_dir)
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("exit_code: 0\n"));
    assert!(stdout.contains("report_status: existing\n"));
    assert!(
        stdout.contains("status: passed\ntests_ran: 1\ntests_passed: 1\nfailures: 0\nsources: 1\n")
    );
}

#[test]
fn test_run_gradle_prints_default_tail_when_runner_fails_before_reports() {
    let project_dir = project_dir("run_gradle_missing_reports_tail");
    let state_dir = state_dir("run_gradle_missing_reports_tail");
    write_fake_gradlew(
        &project_dir,
        "printf 'compile failed\nimportant compiler line\n'\nexit 1\n",
    );

    let output = conduit_command()
        .args(["test", "run", "gradle"])
        .current_dir(&project_dir)
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert_eq!(output.status.code(), Some(1));

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("report_status: missing\n"));
    assert!(stdout.contains("log_tail: 2\n"));
    assert!(stdout.contains("log: compile failed\n"));
    assert!(stdout.contains("log: important compiler line\n"));
}

#[test]
fn test_run_gradle_failed_uses_stored_selectors() {
    let project_dir = project_dir("run_gradle_failed");
    let state_dir = state_dir("run_gradle_failed");
    write_fake_gradlew(
        &project_dir,
        "printf '%s\n' \"$@\" > gradle-args.txt\nexit 0\n",
    );
    write_junit_report(&project_dir, passing_junit_report());

    let write_output = conduit_command()
        .args(["test", "failures", "tests/fixtures/junit"])
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert!(write_output.status.success());

    let run_output = conduit_command()
        .args(["test", "run", "gradle", "--failed"])
        .current_dir(&project_dir)
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert!(run_output.status.success());

    let args = fs::read_to_string(project_dir.join("gradle-args.txt")).expect("gradle args exist");
    assert!(args.contains("test\n"));
    assert!(args.contains("--tests\ncom.example.PaymentServiceTest.createsPayment\n"));
    assert!(args.contains("--tests\ncom.example.PaymentServiceTest.refundsPayment\n"));
}

#[test]
fn test_run_gradle_applies_profile_and_extra_gradle_args() {
    let project_dir = project_dir("run_gradle_profile_args");
    let state_dir = state_dir("run_gradle_profile_args");
    write_fake_gradlew(
        &project_dir,
        "printf '%s\n' \"$@\" > gradle-args.txt\nexit 0\n",
    );
    write_gradle_profile_project_config(&project_dir);
    write_junit_report(&project_dir, passing_junit_report());

    let output = conduit_command()
        .args([
            "test",
            "run",
            "gradle",
            "--profile",
            "integration",
            "--tests",
            "*SdkTest",
            "--",
            "-Penvironment=staging",
            "-Pswitch.consumer_safe_registration=false",
        ])
        .current_dir(&project_dir)
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("mode: integration\n"));
    assert!(stdout.contains(
        "command: ./gradlew test --tests '*SdkTest' '-Dexample.integration=true' '-Penvironment=staging' '-Pswitch.consumer_safe_registration=false'\n"
    ));

    let args = fs::read_to_string(project_dir.join("gradle-args.txt")).expect("gradle args exist");
    assert!(args.contains("test\n"));
    assert!(args.contains("--tests\n*SdkTest\n"));
    assert!(args.contains("-Dexample.integration=true\n"));
    assert!(args.contains("-Penvironment=staging\n"));
    assert!(args.contains("-Pswitch.consumer_safe_registration=false\n"));
}

#[test]
fn test_run_gradle_discovers_profile_from_ancestor_config() {
    let workspace_dir = project_dir("profile_workspace");
    let project_dir = workspace_dir.join("worktrees/service");
    let state_dir = state_dir("profile_workspace");
    fs::create_dir_all(&project_dir).expect("create nested project");
    write_gradle_profile_project_config(&workspace_dir);
    write_fake_gradlew(
        &project_dir,
        "printf '%s\n' \"$@\" > gradle-args.txt\nexit 0\n",
    );
    write_junit_report(&project_dir, passing_junit_report());

    let output = conduit_command()
        .args(["test", "run", "gradle", "--profile", "integration"])
        .current_dir(&project_dir)
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("profile: integration\n"));
    assert!(stdout.contains("mode: integration\n"));

    let args = fs::read_to_string(project_dir.join("gradle-args.txt")).expect("gradle args exist");
    assert!(args.contains("test\n"));
    assert!(args.contains("-Dexample.integration=true\n"));
}

#[test]
fn test_run_gradle_reports_missing_profile() {
    let project_dir = project_dir("run_gradle_missing_profile");
    let state_dir = state_dir("run_gradle_missing_profile");
    write_fake_gradlew(&project_dir, "exit 0\n");

    let output = conduit_command()
        .args(["test", "run", "gradle", "--profile", "integration"])
        .current_dir(&project_dir)
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8(output.stderr).expect("stderr is utf8");
    assert!(
        stderr.contains(
            "gradle test profile `integration` is not configured in .conduit/conduit.toml"
        )
    );
}

#[test]
fn test_run_gradle_applies_profile_env() {
    let project_dir = project_dir("run_gradle_profile_env");
    let state_dir = state_dir("run_gradle_profile_env");
    write_fake_gradlew(
        &project_dir,
        "printf '%s\n' \"$CONDUIT_TEST_PROFILE_ENV\" > profile-env.txt\nexit 0\n",
    );
    write_gradle_profile_with_env_project_config(&project_dir);
    write_junit_report(&project_dir, passing_junit_report());

    let output = conduit_command()
        .args(["test", "run", "gradle", "--profile", "unit-java8"])
        .current_dir(&project_dir)
        .env("CONDUIT_STATE_DIR", &state_dir)
        .env_remove("CONDUIT_TEST_PROFILE_ENV")
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let env_value =
        fs::read_to_string(project_dir.join("profile-env.txt")).expect("profile env exists");
    assert_eq!(env_value, "/tmp/java8\n");
}

#[test]
fn test_run_gradle_accepts_custom_task_and_report_path() {
    let project_dir = project_dir("run_gradle_custom_task");
    let state_dir = state_dir("run_gradle_custom_task");
    write_fake_gradlew(
        &project_dir,
        "printf '%s\n' \"$@\" > gradle-args.txt\nexit 0\n",
    );
    write_junit_report_at(
        &project_dir,
        "service/build/test-results/test",
        passing_junit_report(),
    );

    let output = conduit_command()
        .args([
            "test",
            "run",
            "gradle",
            "--task",
            ":service:test",
            "--report-path",
            "service/build/test-results/test",
            "--tests",
            "com.example.PaymentServiceTest",
        ])
        .current_dir(&project_dir)
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(
        stdout
            .contains("command: ./gradlew :service:test --tests com.example.PaymentServiceTest\n")
    );
    assert!(stdout.contains("report_path: service/build/test-results/test\n"));

    let args = fs::read_to_string(project_dir.join("gradle-args.txt")).expect("gradle args exist");
    assert!(args.contains(":service:test\n"));
    assert!(args.contains("--tests\ncom.example.PaymentServiceTest\n"));
}

#[test]
fn test_run_gradle_reports_no_source_success_without_reports() {
    let project_dir = project_dir("run_gradle_no_source");
    let state_dir = state_dir("run_gradle_no_source");
    write_fake_gradlew(
        &project_dir,
        "echo '> Task :catalog_service:test NO-SOURCE'\necho 'BUILD SUCCESSFUL in 1s'\nexit 0\n",
    );

    let output = conduit_command()
        .args(["test", "run", "gradle", "--task", ":catalog_service:test"])
        .current_dir(&project_dir)
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("test_outcome: no_source\n"));
    assert!(stdout.contains("report_status: missing\n"));
    assert!(
        stdout.contains("status: passed\ntests_ran: 0\ntests_passed: 0\nfailures: 0\nsources: 0\n")
    );
}

#[test]
fn test_run_gradle_reports_no_matching_tests_without_overwriting_failures() {
    let project_dir = project_dir("run_gradle_no_matching_tests");
    let state_dir = state_dir("run_gradle_no_matching_tests");
    write_fake_gradlew(
        &project_dir,
        "echo 'Execution failed for task' >&2\necho '> No tests found for given includes: [MissingTest](--tests filter)' >&2\nexit 1\n",
    );

    let write_output = conduit_command()
        .args(["test", "failures", "tests/fixtures/junit"])
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");
    assert!(write_output.status.success());

    let output = conduit_command()
        .args(["test", "run", "gradle", "--tests", "MissingTest"])
        .current_dir(&project_dir)
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert_eq!(output.status.code(), Some(1));

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("test_outcome: no_matching_tests\n"));
    assert!(stdout.contains("report_status: missing\n"));
    assert!(stdout.contains("diagnostic: no_tests_matched\n"));

    let failed_output = conduit_command()
        .args(["test", "failed"])
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");
    assert!(failed_output.status.success());

    let failed_stdout = String::from_utf8(failed_output.stdout).expect("stdout is utf8");
    assert!(failed_stdout.contains("com.example.PaymentServiceTest.createsPayment"));
}

#[test]
fn test_run_gradle_reports_spotless_hint() {
    let project_dir = project_dir("run_gradle_spotless");
    let state_dir = state_dir("run_gradle_spotless");
    write_fake_gradlew(
        &project_dir,
        "echo '> Task :spotlessJavaCheck FAILED'\necho 'Spotless violation found. Run ./gradlew spotlessApply to fix.'\nexit 1\n",
    );

    let output = conduit_command()
        .args(["test", "run", "gradle", "--tail", "4"])
        .current_dir(&project_dir)
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert_eq!(output.status.code(), Some(1));

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("diagnostic: spotless_violation\n"));
    assert!(stdout.contains("hint: run ./gradlew spotlessApply\n"));
}

#[test]
fn test_run_gradle_times_out_with_log_tail() {
    let project_dir = project_dir("run_gradle_timeout");
    let state_dir = state_dir("run_gradle_timeout");
    write_fake_gradlew(&project_dir, "echo starting slow test\nsleep 5\nexit 0\n");

    let output = conduit_command()
        .args(["test", "run", "gradle", "--timeout", "1s", "--tail", "5"])
        .current_dir(&project_dir)
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert_eq!(output.status.code(), Some(1));

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("termination: timeout\n"));
    assert!(stdout.contains("exit_code: timeout\n"));
    assert!(stdout.contains("test_outcome: runner_failed\n"));
}

#[test]
fn test_run_gradle_infers_report_path_from_subproject_task() {
    let project_dir = project_dir("run_gradle_inferred_report");
    let state_dir = state_dir("run_gradle_inferred_report");
    write_fake_gradlew(
        &project_dir,
        "printf '%s\n' \"$@\" > gradle-args.txt\nexit 0\n",
    );
    write_junit_report_at(
        &project_dir,
        "service/build/test-results/test",
        passing_junit_report(),
    );

    let output = conduit_command()
        .args([
            "test",
            "run",
            "gradle",
            "--task",
            ":service:test",
            "--tests",
            "com.example.PaymentServiceTest",
        ])
        .current_dir(&project_dir)
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("report_path: service/build/test-results/test\n"));
    assert!(
        stdout.contains("status: passed\ntests_ran: 1\ntests_passed: 1\nfailures: 0\nsources: 1\n")
    );
}

#[test]
fn test_log_reads_latest_captured_log() {
    let project_dir = project_dir("test_log_latest");
    let state_dir = state_dir("test_log_latest");
    write_fake_gradlew(&project_dir, "printf 'one\ntwo\nthree\n'\nexit 0\n");
    write_junit_report(&project_dir, passing_junit_report());

    let run_output = conduit_command()
        .args(["test", "run", "gradle"])
        .current_dir(&project_dir)
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert!(run_output.status.success());

    let log_output = conduit_command()
        .args(["test", "log", "--tail", "2"])
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert!(log_output.status.success());

    let stdout = String::from_utf8(log_output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("path: "));
    assert!(stdout.contains("lines: 2\n"));
    assert!(stdout.contains("log: three\n"));
    assert!(stdout.contains("log: three\n"));
    assert!(!stdout.contains("log: stdout:\n"));
    assert!(!stdout.contains("log: stderr:\n"));
}

#[test]
fn test_last_reads_latest_run_summary() {
    let project_dir = project_dir("test_last_run");
    let state_dir = state_dir("test_last_run");
    write_fake_gradlew(&project_dir, "exit 0\n");
    write_junit_report(&project_dir, passing_junit_report());

    let run_output = conduit_command()
        .args(["test", "run", "gradle"])
        .current_dir(&project_dir)
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert!(run_output.status.success());

    let last_output = conduit_command()
        .args(["test", "last"])
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert!(last_output.status.success());

    let stdout = String::from_utf8(last_output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("runner: gradle\n"));
    assert!(stdout.contains("termination: exit\n"));
    assert!(stdout.contains("test_outcome: executed\n"));
    assert!(stdout.contains("status: passed\n"));
}

#[test]
fn test_log_reads_explicit_log_path_as_json() {
    let state_dir = state_dir("test_log_explicit_json");
    let log_dir = std::path::Path::new(&state_dir).join("logs");
    fs::create_dir_all(&log_dir).expect("create log dir");
    let log_path = log_dir.join("manual.log");
    fs::write(&log_path, "alpha\nbeta\ngamma\n").expect("write log");

    let output = conduit_command()
        .args([
            "test",
            "log",
            "--path",
            log_path.to_str().expect("utf8 path"),
            "--tail",
            "2",
            "--json",
        ])
        .env("CONDUIT_STATE_DIR", &state_dir)
        .output()
        .expect("run conduit");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.starts_with("{\"path\":\""));
    assert!(stdout.contains("\"lines\":[\"beta\",\"gamma\"]"));
}

#[test]
fn stats_reports_command_count_without_mutating_itself() {
    let stats_dir = state_dir("stats_command_count_user");
    let state_dir = state_dir("stats_command_count");

    let about_output = conduit_command()
        .arg("about")
        .env("CONDUIT_STATE_DIR", &state_dir)
        .env("CONDUIT_STATS_DIR", &stats_dir)
        .output()
        .expect("run conduit");

    assert!(about_output.status.success());

    let first_stats = conduit_command()
        .arg("stats")
        .env("CONDUIT_STATE_DIR", &state_dir)
        .env("CONDUIT_STATS_DIR", &stats_dir)
        .output()
        .expect("run conduit");
    let second_stats = conduit_command()
        .arg("stats")
        .env("CONDUIT_STATE_DIR", &state_dir)
        .env("CONDUIT_STATS_DIR", &stats_dir)
        .output()
        .expect("run conduit");

    assert!(first_stats.status.success());
    assert!(second_stats.status.success());

    let first_stdout = String::from_utf8(first_stats.stdout).expect("stdout is utf8");
    let second_stdout = String::from_utf8(second_stats.stdout).expect("stdout is utf8");
    assert_eq!(first_stdout, second_stdout);
    assert!(first_stdout.contains("scope: user\n"));
    assert!(first_stdout.contains("commands: 1\n"));
    assert!(first_stdout.contains("test_runs: 0\n"));
}

#[test]
fn stats_accumulates_gradle_noise_reduction() {
    let project_dir = project_dir("stats_gradle");
    let stats_dir = state_dir("stats_gradle_user");
    let state_dir = state_dir("stats_gradle");
    write_fake_gradlew(
        &project_dir,
        "printf 'compileJava\ncompileTestJava\ntest\n'\nexit 0\n",
    );
    write_junit_report(&project_dir, passing_junit_report());

    let test_output = conduit_command()
        .args(["test", "run", "gradle"])
        .current_dir(&project_dir)
        .env("CONDUIT_STATE_DIR", &state_dir)
        .env("CONDUIT_STATS_DIR", &stats_dir)
        .output()
        .expect("run conduit");

    assert!(test_output.status.success());

    let stats_output = conduit_command()
        .args(["stats", "--json"])
        .env("CONDUIT_STATE_DIR", &state_dir)
        .env("CONDUIT_STATS_DIR", &stats_dir)
        .output()
        .expect("run conduit");

    assert!(stats_output.status.success());

    let stats: serde_json::Value =
        serde_json::from_slice(&stats_output.stdout).expect("stats json");
    assert_eq!(stats["scope"], "user");
    assert_eq!(stats["commands"], 1);
    assert_eq!(stats["test_runs"], 1);
    assert!(stats["raw_log_lines"].as_u64().expect("raw lines") > 0);
    assert!(
        stats["conduit_output_lines"]
            .as_u64()
            .expect("summary lines")
            > 0
    );
    assert!(stats["raw_log_bytes"].as_u64().expect("raw bytes") > 0);
    assert!(
        stats["conduit_output_bytes"]
            .as_u64()
            .expect("summary bytes")
            > 0
    );
}

fn state_dir(name: &str) -> String {
    let index = STATE_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir()
        .join(format!(
            "conduit-cli-test-{}-{name}-{index}",
            std::process::id()
        ))
        .to_string_lossy()
        .to_string()
}

fn conduit_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_conduit"));
    command.env("CONDUIT_STATS_DIR", state_dir("command_stats"));
    command
}

fn fixture_command() -> Command {
    let mut command = conduit_command();
    command.current_dir(project_dir("fixture_command"));
    command
}

fn project_dir(name: &str) -> std::path::PathBuf {
    let index = STATE_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "conduit-cli-project-{}-{name}-{index}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("create project dir");
    path
}

fn init_git_repo(path: &std::path::Path) {
    fs::create_dir_all(path).expect("create git repo dir");
    let output = Command::new("git")
        .args(["init"])
        .current_dir(path)
        .output()
        .expect("run git init");
    assert!(output.status.success());
}

fn write_fake_gradlew(project_dir: &std::path::Path, body: &str) {
    let path = project_dir.join("gradlew");
    fs::write(&path, format!("#!/bin/sh\n{body}")).expect("write fake gradlew");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).expect("make gradlew executable");
    }
}

fn write_junit_report(project_dir: &std::path::Path, xml: &str) {
    write_junit_report_at(project_dir, "build/test-results/test", xml);
}

fn write_junit_report_at(project_dir: &std::path::Path, report_path: &str, xml: &str) {
    let report_dir = project_dir.join(report_path);
    fs::create_dir_all(&report_dir).expect("create report dir");
    fs::write(report_dir.join("TEST-sample.xml"), xml).expect("write junit report");
}

fn failing_junit_report() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuite name="com.example.PaymentServiceTest" tests="1" failures="1" errors="0" skipped="0">
  <testcase classname="com.example.PaymentServiceTest" name="createsPayment">
    <failure message="expected:&lt;200&gt; but was:&lt;500&gt;" type="org.opentest4j.AssertionFailedError">
      org.opentest4j.AssertionFailedError: expected:&lt;200&gt; but was:&lt;500&gt;
    </failure>
  </testcase>
</testsuite>
"#
}

fn passing_junit_report() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuite name="com.example.PaymentServiceTest" tests="1" failures="0" errors="0" skipped="0">
  <testcase classname="com.example.PaymentServiceTest" name="createsPayment" />
</testsuite>
"#
}

fn openapi_provider_component() -> Vec<u8> {
    openapi_provider_component_with_fixture(&openapi_provider_fixture())
}

fn openapi_provider_component_with_fixture(wat: &str) -> Vec<u8> {
    provider_component_with_fixture("openapi-provider", wat)
}

fn logs_provider_component() -> Vec<u8> {
    provider_component_with_fixture(
        "logs-provider",
        include_str!("fixtures/logs-provider/module.wat"),
    )
}

fn provider_component_with_fixture(world_name: &str, wat: &str) -> Vec<u8> {
    let mut resolve = wit_parser::Resolve::default();
    let wit_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../wit/conduit-plugin");
    let (package, _) = resolve.push_dir(wit_dir).expect("load wit package");
    let world = resolve
        .select_world(&[package], Some(world_name))
        .expect("select provider world");
    let mut module = wat::parse_str(wat).expect("parse provider fixture");
    wit_component::embed_component_metadata(
        &mut module,
        &resolve,
        world,
        wit_component::StringEncoding::UTF8,
    )
    .expect("embed component metadata");

    wit_component::ComponentEncoder::default()
        .module(&module)
        .expect("load fixture module")
        .encode()
        .expect("encode fixture component")
}

fn openapi_provider_fixture() -> String {
    include_str!("fixtures/openapi-provider/module.wat").to_string()
}

fn write_openapi_plugin_project_config(project: &std::path::Path) {
    let plugin_dir = project.join(".conduit/plugins");
    fs::create_dir_all(&plugin_dir).expect("create plugin dir");
    fs::write(
        project.join(".conduit/conduit.toml"),
        r#"
        [plugins.company]
        path = ".conduit/plugins/company.wasm"

        [openapi]
        provider = "company"
        "#,
    )
    .expect("write config");
}

fn write_gradle_profile_project_config(project: &std::path::Path) {
    fs::create_dir_all(project.join(".conduit")).expect("create config dir");
    fs::write(
        project.join(".conduit/conduit.toml"),
        r#"
        [test.gradle.profiles.integration]
        task = "test"
        report_path = "build/test-results/test"
        mode = "integration"
        args = ["-Dexample.integration=true"]
        "#,
    )
    .expect("write config");
}

fn write_gradle_profile_with_env_project_config(project: &std::path::Path) {
    fs::create_dir_all(project.join(".conduit")).expect("create config dir");
    fs::write(
        project.join(".conduit/conduit.toml"),
        r#"
        [test.gradle.profiles.unit-java8]
        task = "test"
        mode = "unit"

        [test.gradle.profiles.unit-java8.env]
        CONDUIT_TEST_PROFILE_ENV = "/tmp/java8"
        "#,
    )
    .expect("write config");
}
