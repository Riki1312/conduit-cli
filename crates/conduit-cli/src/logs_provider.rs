use crate::config::ConduitConfig;
use crate::logs::{DEFAULT_LOG_SINCE, FixtureLogProvider, LogError, LogProvider};
use crate::plugin_bindings::logs::exports::conduit::plugin::metadata::PluginMetadata;
use crate::plugin_runtime::{PluginRuntime, PluginRuntimeError};

const EXPECTED_PROTOCOL_VERSION: &str = "1";
const LOGS_PROVIDER: &str = "logs-provider-v1";

pub(crate) struct ConfiguredLogProvider {
    pub(crate) provider: Box<dyn LogProvider>,
    pub(crate) default_environment: Option<String>,
    pub(crate) default_since: String,
}

pub(crate) fn configured_log_provider() -> Result<ConfiguredLogProvider, LogProviderLoadError> {
    let search = ConduitConfig::load_current_dir_for_logs().map_err(LogProviderLoadError::from)?;
    let logs = search
        .config
        .as_ref()
        .map(|config| config.logs())
        .transpose()
        .map_err(LogProviderLoadError::from)?
        .flatten();
    let default_environment = search
        .config
        .as_ref()
        .and_then(|config| config.defaults().environment);
    let default_since = logs
        .as_ref()
        .and_then(|logs| logs.default_since.clone())
        .unwrap_or_else(|| DEFAULT_LOG_SINCE.to_string());

    let Some(config) = &search.config else {
        return Err(LogProviderLoadError {
            message: "logs provider is not configured in .conduit/conduit.toml".to_string(),
        });
    };
    let Some(provider_name) = logs.as_ref().and_then(|logs| logs.provider.as_deref()) else {
        return Err(LogProviderLoadError {
            message: "logs provider is not configured in .conduit/conduit.toml".to_string(),
        });
    };
    if provider_name == "fixture" {
        return Ok(ConfiguredLogProvider {
            provider: Box::new(FixtureLogProvider),
            default_environment,
            default_since,
        });
    }
    let Some(plugin) = config.logs_plugin().map_err(LogProviderLoadError::from)? else {
        return Err(LogProviderLoadError {
            message: "logs provider is not configured in .conduit/conduit.toml".to_string(),
        });
    };

    let runtime = PluginRuntime::new().map_err(LogProviderLoadError::from)?;
    let provider = runtime
        .instantiate_logs_provider_with_capabilities(plugin.path, plugin.capabilities)
        .map_err(LogProviderLoadError::from)?;
    validate_logs_plugin_metadata(&provider.metadata().map_err(LogProviderLoadError::from)?)?;

    Ok(ConfiguredLogProvider {
        provider: Box::new(provider),
        default_environment,
        default_since,
    })
}

pub(crate) fn validate_logs_plugin_metadata(
    metadata: &PluginMetadata,
) -> Result<(), LogProviderLoadError> {
    if metadata.protocol_version != EXPECTED_PROTOCOL_VERSION {
        return Err(LogProviderLoadError {
            message: format!(
                "plugin `{}` uses unsupported protocol version `{}`; expected `{}`",
                metadata.id, metadata.protocol_version, EXPECTED_PROTOCOL_VERSION
            ),
        });
    }

    if !metadata
        .providers
        .iter()
        .any(|provider| provider == LOGS_PROVIDER)
    {
        return Err(LogProviderLoadError {
            message: format!(
                "plugin `{}` does not declare provider `{}`",
                metadata.id, LOGS_PROVIDER
            ),
        });
    }

    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct LogProviderLoadError {
    pub(crate) message: String,
}

impl From<crate::config::ConfigError> for LogProviderLoadError {
    fn from(error: crate::config::ConfigError) -> Self {
        Self {
            message: error.message,
        }
    }
}

impl From<PluginRuntimeError> for LogProviderLoadError {
    fn from(error: PluginRuntimeError) -> Self {
        Self {
            message: error.message,
        }
    }
}

impl From<LogError> for LogProviderLoadError {
    fn from(error: LogError) -> Self {
        Self {
            message: error.message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_supported_logs_provider_metadata() {
        let metadata = metadata("fixture", "1", ["logs-provider-v1"]);

        validate_logs_plugin_metadata(&metadata).unwrap();
    }

    #[test]
    fn rejects_unsupported_protocol_version() {
        let metadata = metadata("fixture", "2", ["logs-provider-v1"]);

        let error = validate_logs_plugin_metadata(&metadata).unwrap_err();

        assert_eq!(
            error.message,
            "plugin `fixture` uses unsupported protocol version `2`; expected `1`"
        );
    }

    #[test]
    fn rejects_missing_logs_provider_declaration() {
        let metadata = metadata("fixture", "1", ["openapi-provider-v1"]);

        let error = validate_logs_plugin_metadata(&metadata).unwrap_err();

        assert_eq!(
            error.message,
            "plugin `fixture` does not declare provider `logs-provider-v1`"
        );
    }

    fn metadata(
        id: &str,
        protocol_version: &str,
        providers: impl IntoIterator<Item = &'static str>,
    ) -> PluginMetadata {
        PluginMetadata {
            id: id.to_string(),
            version: "0.1.0".to_string(),
            protocol_version: protocol_version.to_string(),
            providers: providers
                .into_iter()
                .map(std::string::ToString::to_string)
                .collect(),
        }
    }
}
