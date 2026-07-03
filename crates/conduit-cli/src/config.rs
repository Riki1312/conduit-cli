use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

const PROJECT_CONFIG_PATH: &str = ".conduit/conduit.toml";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConduitConfig {
    project_root: PathBuf,
    file: ConfigFile,
}

impl ConduitConfig {
    pub(crate) fn load_current_dir() -> Result<Option<Self>, ConfigError> {
        Ok(Self::search_current_dir_configs(|_| true)?.config)
    }

    pub(crate) fn load_current_dir_for_logs() -> Result<ConfigSearch, ConfigError> {
        Self::search_current_dir_configs(|config| config.file.logs.is_some())
    }

    pub(crate) fn load_current_dir_for_openapi() -> Result<ConfigSearch, ConfigError> {
        Self::search_current_dir_configs(|config| config.file.openapi.is_some())
    }

    pub(crate) fn load_from_dir(root: impl AsRef<Path>) -> Result<Option<Self>, ConfigError> {
        let root = root.as_ref();
        let path = root.join(PROJECT_CONFIG_PATH);
        if !path.exists() {
            return Ok(None);
        }

        let contents = fs::read_to_string(&path).map_err(|source| ConfigError {
            message: format!("failed to read config {}: {source}", path.display()),
        })?;
        let file = toml::from_str(&contents).map_err(|source| ConfigError {
            message: format!("failed to parse config {}: {source}", path.display()),
        })?;

        Ok(Some(Self {
            project_root: root.to_path_buf(),
            file,
        }))
    }

    fn search_current_dir_configs(
        mut matches: impl FnMut(&Self) -> bool,
    ) -> Result<ConfigSearch, ConfigError> {
        // Provider-specific commands may be invoked inside repos with their own
        // partial config. Keep walking until the relevant section is found, but
        // remember whether any config existed so commands can avoid misleading
        // fixture fallbacks in configured workspaces.
        let mut found_any_config = false;
        if let Some(config) = Self::load_from_dir(".")? {
            found_any_config = true;
            if matches(&config) {
                return Ok(ConfigSearch {
                    config: Some(config),
                    found_any_config,
                });
            }
        }

        let mut root = std::env::current_dir().map_err(|source| ConfigError {
            message: format!("failed to resolve current directory: {source}"),
        })?;
        while root.pop() {
            if let Some(config) = Self::load_from_dir(&root)? {
                found_any_config = true;
                if matches(&config) {
                    return Ok(ConfigSearch {
                        config: Some(config),
                        found_any_config,
                    });
                }
            }
        }

        Ok(ConfigSearch {
            config: None,
            found_any_config,
        })
    }

    pub(crate) fn openapi_plugin(&self) -> Result<Option<ConfiguredPlugin>, ConfigError> {
        let Some(openapi) = &self.file.openapi else {
            return Ok(None);
        };
        let Some(provider) = &openapi.provider else {
            return Ok(None);
        };
        let plugin = self.file.plugins.get(provider).ok_or_else(|| ConfigError {
            message: format!("openapi provider `{provider}` is not configured as a plugin"),
        })?;

        Ok(Some(ConfiguredPlugin {
            name: provider.clone(),
            path: resolve_project_path("plugin", &self.project_root, &plugin.path)?,
            capabilities: plugin.capabilities.resolve(&self.project_root)?,
        }))
    }

    pub(crate) fn logs_plugin(&self) -> Result<Option<ConfiguredPlugin>, ConfigError> {
        let Some(logs) = &self.file.logs else {
            return Ok(None);
        };
        let Some(provider) = &logs.provider else {
            return Ok(None);
        };
        if provider == "fixture" {
            return Ok(None);
        }
        let plugin = self.file.plugins.get(provider).ok_or_else(|| ConfigError {
            message: format!("logs provider `{provider}` is not configured as a plugin"),
        })?;

        Ok(Some(ConfiguredPlugin {
            name: provider.clone(),
            path: resolve_project_path("plugin", &self.project_root, &plugin.path)?,
            capabilities: plugin.capabilities.resolve(&self.project_root)?,
        }))
    }

    pub(crate) fn logs(&self) -> Result<Option<ConfiguredLogs>, ConfigError> {
        let Some(logs) = &self.file.logs else {
            return Ok(None);
        };

        Ok(Some(ConfiguredLogs {
            provider: logs.provider.clone(),
            default_environment: logs.default_environment.clone(),
            default_since: logs.default_since.clone(),
        }))
    }

    /// Returns a configured Gradle test profile by name, when present.
    pub(crate) fn gradle_test_profile(
        &self,
        name: &str,
    ) -> Result<Option<ConfiguredGradleTestProfile>, ConfigError> {
        let Some(test) = &self.file.test else {
            return Ok(None);
        };
        let Some(gradle) = &test.gradle else {
            return Ok(None);
        };
        let Some(profile) = gradle.profiles.get(name) else {
            return Ok(None);
        };
        if let Some(report_path) = &profile.report_path {
            validate_project_relative_path("gradle test profile report_path", report_path)?;
        }

        Ok(Some(ConfiguredGradleTestProfile {
            name: name.to_string(),
            task: profile.task.clone(),
            report_path: profile.report_path.clone(),
            mode: profile.mode.clone(),
            args: profile.args.clone(),
            env: validate_env(&profile.env)?,
        }))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConfiguredPlugin {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    pub(crate) capabilities: PluginCapabilities,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConfiguredLogs {
    pub(crate) provider: Option<String>,
    pub(crate) default_environment: Option<String>,
    pub(crate) default_since: Option<String>,
}

/// Result of searching ancestor configs for a command-specific section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConfigSearch {
    /// Nearest config that matched the command-specific predicate.
    pub(crate) config: Option<ConduitConfig>,
    /// Whether any `.conduit/conduit.toml` existed during the search.
    pub(crate) found_any_config: bool,
}

/// Reusable defaults for a Gradle test invocation loaded from project config.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConfiguredGradleTestProfile {
    /// Profile name as referenced by `conduit test run gradle --profile`.
    pub(crate) name: String,
    /// Optional Gradle task, for example `test` or `:service:test`.
    pub(crate) task: Option<String>,
    /// Optional JUnit-style XML report path relative to the project root.
    pub(crate) report_path: Option<PathBuf>,
    /// Optional output mode label. Valid values are resolved by the CLI layer.
    pub(crate) mode: Option<String>,
    /// Gradle arguments prepended before command-line passthrough arguments.
    pub(crate) args: Vec<String>,
    /// Environment variables applied to the runner when not already set.
    pub(crate) env: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PluginCapabilities {
    pub(crate) http_hosts: Vec<String>,
    /// Base directory for relative file-read requests made by the plugin.
    pub(crate) file_read_base: Option<PathBuf>,
    pub(crate) file_read_paths: Vec<PathBuf>,
    pub(crate) secret_names: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConfigError {
    pub(crate) message: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    #[serde(default)]
    plugins: BTreeMap<String, PluginConfig>,
    logs: Option<LogsConfig>,
    openapi: Option<OpenApiConfig>,
    test: Option<TestConfig>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct PluginConfig {
    path: PathBuf,
    #[serde(default)]
    capabilities: CapabilityConfig,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct CapabilityConfig {
    http: Option<HttpCapabilityConfig>,
    #[serde(rename = "file-read")]
    file_read: Option<FileReadCapabilityConfig>,
    secrets: Option<SecretCapabilityConfig>,
}

impl CapabilityConfig {
    fn resolve(&self, project_root: &Path) -> Result<PluginCapabilities, ConfigError> {
        let http_hosts = self
            .http
            .as_ref()
            .map(|capability| {
                capability
                    .hosts
                    .iter()
                    .map(|host| validate_http_host(host).map(|()| host.clone()))
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();
        let file_read_paths = self
            .file_read
            .as_ref()
            .map(|capability| {
                capability
                    .paths
                    .iter()
                    .map(|path| resolve_project_path("file-read capability", project_root, path))
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();
        let secret_names = self
            .secrets
            .as_ref()
            .map(|capability| {
                capability
                    .names
                    .iter()
                    .map(|name| {
                        validate_secret_name("secret capability", name).map(|()| name.clone())
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();

        let file_read_base = (!file_read_paths.is_empty()).then(|| project_root.to_path_buf());

        Ok(PluginCapabilities {
            http_hosts,
            file_read_base,
            file_read_paths,
            secret_names,
        })
    }
}

pub(crate) fn validate_secret_name(label: &str, name: &str) -> Result<(), ConfigError> {
    if name.trim().is_empty() {
        return Err(ConfigError {
            message: format!("{label} name cannot be empty"),
        });
    }
    if name.starts_with('/') || name.contains("..") {
        return Err(ConfigError {
            message: format!("{label} name `{name}` must stay within the secret store"),
        });
    }
    if name == "*" || name.contains('*') {
        return Err(ConfigError {
            message: format!("{label} name `{name}` cannot contain wildcards"),
        });
    }
    if name
        .chars()
        .any(|ch| !(ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '/')))
    {
        return Err(ConfigError {
            message: format!(
                "{label} name `{name}` can only contain ASCII letters, digits, `.`, `_`, `-`, and `/`"
            ),
        });
    }

    Ok(())
}

fn validate_http_host(host: &str) -> Result<(), ConfigError> {
    if host.trim().is_empty() {
        return Err(ConfigError {
            message: "http capability host cannot be empty".to_string(),
        });
    }
    if host.chars().any(char::is_whitespace) {
        return Err(ConfigError {
            message: format!("http capability host `{host}` cannot contain whitespace"),
        });
    }
    if host == "*" || host.contains('*') {
        return Err(ConfigError {
            message: format!("http capability host `{host}` cannot contain wildcards"),
        });
    }
    if host.contains("://") {
        return Err(ConfigError {
            message: format!("http capability host `{host}` must not include a URL scheme"),
        });
    }

    Ok(())
}

fn resolve_project_path(
    label: &str,
    project_root: &Path,
    path: &Path,
) -> Result<PathBuf, ConfigError> {
    validate_project_relative_path(label, path)?;

    Ok(project_root.join(path))
}

fn validate_project_relative_path(label: &str, path: &Path) -> Result<(), ConfigError> {
    if path.as_os_str().is_empty() {
        return Err(ConfigError {
            message: format!("{label} path cannot be empty"),
        });
    }
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(ConfigError {
            message: format!(
                "{label} path `{}` must be relative and stay within the project",
                path.display()
            ),
        });
    }

    Ok(())
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct HttpCapabilityConfig {
    #[serde(default)]
    hosts: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct FileReadCapabilityConfig {
    #[serde(default)]
    paths: Vec<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct SecretCapabilityConfig {
    #[serde(default)]
    names: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct OpenApiConfig {
    provider: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct LogsConfig {
    provider: Option<String>,
    default_environment: Option<String>,
    default_since: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct TestConfig {
    gradle: Option<GradleTestConfig>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct GradleTestConfig {
    #[serde(default)]
    profiles: BTreeMap<String, GradleTestProfileConfig>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct GradleTestProfileConfig {
    task: Option<String>,
    report_path: Option<PathBuf>,
    mode: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
}

fn validate_env(env: &BTreeMap<String, String>) -> Result<BTreeMap<String, String>, ConfigError> {
    for (key, value) in env {
        if key.trim().is_empty() {
            return Err(ConfigError {
                message: "gradle test profile env key cannot be empty".to_string(),
            });
        }
        if key.chars().any(char::is_whitespace) {
            return Err(ConfigError {
                message: format!("gradle test profile env key `{key}` cannot contain whitespace"),
            });
        }
        if value.contains("${") {
            return Err(ConfigError {
                message: format!(
                    "gradle test profile env `{key}` contains unsupported interpolation"
                ),
            });
        }
    }

    Ok(env.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn missing_config_returns_none() {
        let root = test_dir("missing-config");

        let config = ConduitConfig::load_from_dir(&root).unwrap();

        assert_eq!(config, None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolves_openapi_plugin_path_from_project_root() {
        let root = test_dir("openapi-plugin");
        let config_dir = root.join(".conduit");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(
            config_dir.join("conduit.toml"),
            r#"
            [plugins.company]
            path = ".conduit/plugins/company.wasm"

            [openapi]
            provider = "company"
            "#,
        )
        .unwrap();

        let config = ConduitConfig::load_from_dir(&root).unwrap().unwrap();
        let plugin = config.openapi_plugin().unwrap().unwrap();

        assert_eq!(plugin.name, "company");
        assert_eq!(plugin.path, root.join(".conduit/plugins/company.wasm"));
        assert_eq!(plugin.capabilities, PluginCapabilities::default());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolves_logs_config() {
        let root = write_config(
            "logs-config",
            r#"
            [logs]
            provider = "fixture"
            default_environment = "staging"
            default_since = "30m"
            "#,
        );

        let config = ConduitConfig::load_from_dir(&root).unwrap().unwrap();
        let logs = config.logs().unwrap().unwrap();

        assert_eq!(logs.provider.as_deref(), Some("fixture"));
        assert_eq!(logs.default_environment.as_deref(), Some("staging"));
        assert_eq!(logs.default_since.as_deref(), Some("30m"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolves_logs_plugin() {
        let root = write_config(
            "logs-plugin",
            r#"
            [plugins.company]
            path = ".conduit/plugins/company.wasm"

            [plugins.company.capabilities.http]
            hosts = ["logs.example.com"]

            [logs]
            provider = "company"
            "#,
        );

        let config = ConduitConfig::load_from_dir(&root).unwrap().unwrap();
        let plugin = config.logs_plugin().unwrap().unwrap();

        assert_eq!(plugin.name, "company");
        assert_eq!(plugin.path, root.join(".conduit/plugins/company.wasm"));
        assert_eq!(plugin.capabilities.http_hosts, ["logs.example.com"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn logs_fixture_provider_does_not_resolve_plugin() {
        let root = write_config(
            "logs-fixture-provider",
            r#"
            [logs]
            provider = "fixture"
            "#,
        );

        let config = ConduitConfig::load_from_dir(&root).unwrap().unwrap();

        assert_eq!(config.logs_plugin().unwrap(), None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolves_plugin_capabilities() {
        let root = test_dir("plugin-capabilities");
        let config_dir = root.join(".conduit");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(
            config_dir.join("conduit.toml"),
            r#"
            [plugins.company]
            path = ".conduit/plugins/company.wasm"

            [plugins.company.capabilities.http]
            hosts = ["docs.example.com"]

            [plugins.company.capabilities.file-read]
            paths = [".conduit/company"]

            [plugins.company.capabilities.secrets]
            names = ["company-logs/staging/cookie"]

            [openapi]
            provider = "company"
            "#,
        )
        .unwrap();

        let config = ConduitConfig::load_from_dir(&root).unwrap().unwrap();
        let plugin = config.openapi_plugin().unwrap().unwrap();

        assert_eq!(plugin.capabilities.http_hosts, ["docs.example.com"]);
        assert_eq!(
            plugin.capabilities.file_read_paths,
            [root.join(".conduit/company")]
        );
        assert_eq!(
            plugin.capabilities.file_read_base.as_deref(),
            Some(root.as_path())
        );
        assert_eq!(
            plugin.capabilities.secret_names,
            ["company-logs/staging/cookie"]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_invalid_secret_capability_names() {
        let root = write_config(
            "secret-capability-outside-store",
            r#"
            [plugins.company]
            path = ".conduit/plugins/company.wasm"

            [plugins.company.capabilities.secrets]
            names = ["../cookie"]

            [openapi]
            provider = "company"
            "#,
        );

        let config = ConduitConfig::load_from_dir(&root).unwrap().unwrap();
        let error = config.openapi_plugin().unwrap_err();

        assert_eq!(
            error.message,
            "secret capability name `../cookie` must stay within the secret store"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_wildcard_http_hosts() {
        let root = write_config(
            "wildcard-http-host",
            r#"
            [plugins.company]
            path = ".conduit/plugins/company.wasm"

            [plugins.company.capabilities.http]
            hosts = ["*.example.com"]

            [openapi]
            provider = "company"
            "#,
        );

        let config = ConduitConfig::load_from_dir(&root).unwrap().unwrap();
        let error = config.openapi_plugin().unwrap_err();

        assert_eq!(
            error.message,
            "http capability host `*.example.com` cannot contain wildcards"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_http_hosts_with_whitespace() {
        let root = write_config(
            "http-host-with-whitespace",
            r#"
            [plugins.company]
            path = ".conduit/plugins/company.wasm"

            [plugins.company.capabilities.http]
            hosts = ["docs .example.com"]

            [openapi]
            provider = "company"
            "#,
        );

        let config = ConduitConfig::load_from_dir(&root).unwrap().unwrap();
        let error = config.openapi_plugin().unwrap_err();

        assert_eq!(
            error.message,
            "http capability host `docs .example.com` cannot contain whitespace"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_file_read_paths_outside_project() {
        let root = write_config(
            "file-read-outside-project",
            r#"
            [plugins.company]
            path = ".conduit/plugins/company.wasm"

            [plugins.company.capabilities.file-read]
            paths = ["../secret"]

            [openapi]
            provider = "company"
            "#,
        );

        let config = ConduitConfig::load_from_dir(&root).unwrap().unwrap();
        let error = config.openapi_plugin().unwrap_err();

        assert_eq!(
            error.message,
            "file-read capability path `../secret` must be relative and stay within the project"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_absolute_plugin_path() {
        let root = write_config(
            "absolute-plugin-path",
            r#"
            [plugins.company]
            path = "/tmp/company.wasm"

            [openapi]
            provider = "company"
            "#,
        );

        let config = ConduitConfig::load_from_dir(&root).unwrap().unwrap();
        let error = config.openapi_plugin().unwrap_err();

        assert_eq!(
            error.message,
            "plugin path `/tmp/company.wasm` must be relative and stay within the project"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_empty_project_paths() {
        let root = write_config(
            "empty-plugin-path",
            r#"
            [plugins.company]
            path = ""

            [openapi]
            provider = "company"
            "#,
        );

        let config = ConduitConfig::load_from_dir(&root).unwrap().unwrap();
        let error = config.openapi_plugin().unwrap_err();

        assert_eq!(error.message, "plugin path cannot be empty");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_missing_provider_plugin() {
        let root = test_dir("missing-provider-plugin");
        let config_dir = root.join(".conduit");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(
            config_dir.join("conduit.toml"),
            r#"
            [openapi]
            provider = "company"
            "#,
        )
        .unwrap();

        let config = ConduitConfig::load_from_dir(&root).unwrap().unwrap();
        let error = config.openapi_plugin().unwrap_err();

        assert_eq!(
            error.message,
            "openapi provider `company` is not configured as a plugin"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolves_gradle_test_profile() {
        let root = write_config(
            "gradle-test-profile",
            r#"
            [test.gradle.profiles.integration]
            task = "test"
            report_path = "build/test-results/test"
            mode = "integration"
            args = ["-Dexample.integration=true"]

            [test.gradle.profiles.integration.env]
            JAVA_HOME = "/tmp/java8"
            "#,
        );

        let config = ConduitConfig::load_from_dir(&root).unwrap().unwrap();
        let profile = config.gradle_test_profile("integration").unwrap().unwrap();

        assert_eq!(profile.name, "integration");
        assert_eq!(profile.task.as_deref(), Some("test"));
        assert_eq!(
            profile.report_path.as_deref(),
            Some(Path::new("build/test-results/test"))
        );
        assert_eq!(profile.mode.as_deref(), Some("integration"));
        assert_eq!(profile.args, ["-Dexample.integration=true"]);
        assert_eq!(
            profile.env.get("JAVA_HOME").map(String::as_str),
            Some("/tmp/java8")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_gradle_profile_env_interpolation() {
        let root = write_config(
            "gradle-test-profile-env-interpolation",
            r#"
            [test.gradle.profiles.integration.env]
            JAVA_HOME = "${JAVA8_HOME}"
            "#,
        );

        let config = ConduitConfig::load_from_dir(&root).unwrap().unwrap();
        let error = config.gradle_test_profile("integration").unwrap_err();

        assert_eq!(
            error.message,
            "gradle test profile env `JAVA_HOME` contains unsupported interpolation"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_gradle_profile_report_path_outside_project() {
        let root = write_config(
            "gradle-test-profile-outside-project",
            r#"
            [test.gradle.profiles.integration]
            report_path = "../build/test-results/test"
            "#,
        );

        let config = ConduitConfig::load_from_dir(&root).unwrap().unwrap();
        let error = config.gradle_test_profile("integration").unwrap_err();

        assert_eq!(
            error.message,
            "gradle test profile report_path path `../build/test-results/test` must be relative and stay within the project"
        );
        fs::remove_dir_all(root).unwrap();
    }

    fn test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("conduit-config-{nanos}-{name}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_config(name: &str, contents: &str) -> PathBuf {
        let root = test_dir(name);
        let config_dir = root.join(".conduit");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(config_dir.join("conduit.toml"), contents).unwrap();
        root
    }
}
