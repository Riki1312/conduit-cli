use crate::config::ConduitConfig;
use crate::db::{DbError, DbProvider, FixtureDbProvider};
use crate::plugin_bindings::db::exports::conduit::plugin::metadata::PluginMetadata;
use crate::plugin_runtime::{PluginRuntime, PluginRuntimeError};

const DB_PROVIDER: &str = "db-provider-v1";
const EXPECTED_PROTOCOL_VERSION: &str = "1";

pub(crate) struct ConfiguredDbProvider {
    pub(crate) provider: Box<dyn DbProvider>,
    pub(crate) default_environment: Option<String>,
}

pub(crate) fn configured_db_provider() -> Result<ConfiguredDbProvider, DbProviderLoadError> {
    let search = ConduitConfig::load_current_dir_for_db().map_err(DbProviderLoadError::from)?;
    let db = search
        .config
        .as_ref()
        .map(|config| config.db())
        .transpose()
        .map_err(DbProviderLoadError::from)?
        .flatten();
    let default_environment = db.as_ref().and_then(|db| db.default_environment.clone());

    let Some(config) = &search.config else {
        if search.found_any_config {
            return Err(DbProviderLoadError {
                message: "db provider is not configured in .conduit/conduit.toml".to_string(),
            });
        }
        return Ok(ConfiguredDbProvider {
            provider: Box::new(FixtureDbProvider),
            default_environment,
        });
    };
    if db.as_ref().and_then(|db| db.provider.as_deref()).is_none() {
        return Err(DbProviderLoadError {
            message: "db provider is not configured in .conduit/conduit.toml".to_string(),
        });
    }
    let Some(plugin) = config.db_plugin().map_err(DbProviderLoadError::from)? else {
        return Ok(ConfiguredDbProvider {
            provider: Box::new(FixtureDbProvider),
            default_environment,
        });
    };

    let runtime = PluginRuntime::new().map_err(DbProviderLoadError::from)?;
    let provider = runtime
        .instantiate_db_provider_with_capabilities(plugin.path, plugin.capabilities)
        .map_err(DbProviderLoadError::from)?;
    validate_db_plugin_metadata(&provider.metadata().map_err(DbProviderLoadError::from)?)?;

    Ok(ConfiguredDbProvider {
        provider: Box::new(provider),
        default_environment,
    })
}

pub(crate) fn validate_db_plugin_metadata(
    metadata: &PluginMetadata,
) -> Result<(), DbProviderLoadError> {
    if metadata.protocol_version != EXPECTED_PROTOCOL_VERSION {
        return Err(DbProviderLoadError {
            message: format!(
                "plugin `{}` uses unsupported protocol version `{}`; expected `{}`",
                metadata.id, metadata.protocol_version, EXPECTED_PROTOCOL_VERSION
            ),
        });
    }

    if !metadata
        .providers
        .iter()
        .any(|provider| provider == DB_PROVIDER)
    {
        return Err(DbProviderLoadError {
            message: format!(
                "plugin `{}` does not declare provider `{}`",
                metadata.id, DB_PROVIDER
            ),
        });
    }

    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct DbProviderLoadError {
    pub(crate) message: String,
}

impl From<crate::config::ConfigError> for DbProviderLoadError {
    fn from(error: crate::config::ConfigError) -> Self {
        Self {
            message: error.message,
        }
    }
}

impl From<PluginRuntimeError> for DbProviderLoadError {
    fn from(error: PluginRuntimeError) -> Self {
        Self {
            message: error.message,
        }
    }
}

impl From<DbError> for DbProviderLoadError {
    fn from(error: DbError) -> Self {
        Self {
            message: error.message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_supported_db_provider_metadata() {
        let metadata = metadata("fixture", "1", ["db-provider-v1"]);

        validate_db_plugin_metadata(&metadata).unwrap();
    }

    #[test]
    fn rejects_unsupported_protocol_version() {
        let metadata = metadata("fixture", "2", ["db-provider-v1"]);

        let error = validate_db_plugin_metadata(&metadata).unwrap_err();

        assert_eq!(
            error.message,
            "plugin `fixture` uses unsupported protocol version `2`; expected `1`"
        );
    }

    #[test]
    fn rejects_missing_db_provider_declaration() {
        let metadata = metadata("fixture", "1", ["logs-provider-v1"]);

        let error = validate_db_plugin_metadata(&metadata).unwrap_err();

        assert_eq!(
            error.message,
            "plugin `fixture` does not declare provider `db-provider-v1`"
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
