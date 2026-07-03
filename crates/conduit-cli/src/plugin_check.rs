use crate::config::{ConduitConfig, PluginCapabilities};
use crate::db_provider::validate_db_plugin_metadata;
use crate::logs_provider::validate_logs_plugin_metadata;
use crate::openapi_provider::validate_openapi_plugin_metadata;
use crate::plugin_runtime::PluginRuntime;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PluginCheckError {
    pub(crate) message: String,
}

impl PluginCheckError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub(crate) struct PluginCheckSummary {
    pub(crate) status: String,
    pub(crate) path: String,
    pub(crate) id: String,
    pub(crate) version: String,
    pub(crate) protocol_version: String,
    pub(crate) providers: Vec<String>,
}

impl PluginCheckSummary {
    fn from_plugin(
        provider: PluginCheckProvider,
        path: &Path,
        capabilities: PluginCapabilities,
    ) -> Result<Self, PluginCheckError> {
        match provider {
            PluginCheckProvider::OpenApi => Self::from_openapi_plugin(path, capabilities),
            PluginCheckProvider::Logs => Self::from_logs_plugin(path, capabilities),
            PluginCheckProvider::Db => Self::from_db_plugin(path, capabilities),
        }
    }

    fn from_openapi_plugin(
        path: &Path,
        capabilities: PluginCapabilities,
    ) -> Result<Self, PluginCheckError> {
        let runtime = PluginRuntime::new().map_err(|error| PluginCheckError {
            message: error.message,
        })?;
        let provider = runtime
            .instantiate_openapi_provider_with_capabilities(path, capabilities)
            .map_err(|error| PluginCheckError {
                message: error.message,
            })?;
        let metadata = provider.metadata().map_err(|error| PluginCheckError {
            message: error.message,
        })?;

        validate_openapi_plugin_metadata(&metadata).map_err(|error| PluginCheckError {
            message: error.message,
        })?;

        Ok(Self {
            status: "ok".to_string(),
            path: path.to_string_lossy().to_string(),
            id: metadata.id,
            version: metadata.version,
            protocol_version: metadata.protocol_version,
            providers: metadata.providers,
        })
    }

    fn from_logs_plugin(
        path: &Path,
        capabilities: PluginCapabilities,
    ) -> Result<Self, PluginCheckError> {
        let runtime = PluginRuntime::new().map_err(|error| PluginCheckError {
            message: error.message,
        })?;
        let provider = runtime
            .instantiate_logs_provider_with_capabilities(path, capabilities)
            .map_err(|error| PluginCheckError {
                message: error.message,
            })?;
        let metadata = provider.metadata().map_err(|error| PluginCheckError {
            message: error.message,
        })?;

        validate_logs_plugin_metadata(&metadata).map_err(|error| PluginCheckError {
            message: error.message,
        })?;

        Ok(Self {
            status: "ok".to_string(),
            path: path.to_string_lossy().to_string(),
            id: metadata.id,
            version: metadata.version,
            protocol_version: metadata.protocol_version,
            providers: metadata.providers,
        })
    }

    fn from_db_plugin(
        path: &Path,
        capabilities: PluginCapabilities,
    ) -> Result<Self, PluginCheckError> {
        let runtime = PluginRuntime::new().map_err(|error| PluginCheckError {
            message: error.message,
        })?;
        let provider = runtime
            .instantiate_db_provider_with_capabilities(path, capabilities)
            .map_err(|error| PluginCheckError {
                message: error.message,
            })?;
        let metadata = provider.metadata().map_err(|error| PluginCheckError {
            message: error.message,
        })?;

        validate_db_plugin_metadata(&metadata).map_err(|error| PluginCheckError {
            message: error.message,
        })?;

        Ok(Self {
            status: "ok".to_string(),
            path: path.to_string_lossy().to_string(),
            id: metadata.id,
            version: metadata.version,
            protocol_version: metadata.protocol_version,
            providers: metadata.providers,
        })
    }

    pub(crate) fn render_text(&self) -> String {
        let mut lines = vec![
            format!("status: {}", self.status),
            format!("path: {}", self.path),
            format!("id: {}", self.id),
            format!("version: {}", self.version),
            format!("protocol_version: {}", self.protocol_version),
            format!("providers: {}", self.providers.len()),
        ];

        lines.extend(
            self.providers
                .iter()
                .map(|provider| format!("provider: {provider}")),
        );

        lines.join("\n")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PluginCheckProvider {
    OpenApi,
    Logs,
    Db,
}

impl PluginCheckProvider {
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "openapi" => Some(Self::OpenApi),
            "logs" => Some(Self::Logs),
            "db" => Some(Self::Db),
            _ => None,
        }
    }
}

pub(crate) fn check_plugin(
    path: impl Into<PathBuf>,
    provider: PluginCheckProvider,
) -> Result<PluginCheckSummary, PluginCheckError> {
    let path = path.into();
    PluginCheckSummary::from_plugin(provider, &path, PluginCapabilities::default())
}

pub(crate) fn check_configured_plugin(
    provider: PluginCheckProvider,
) -> Result<PluginCheckSummary, PluginCheckError> {
    match provider {
        PluginCheckProvider::OpenApi => check_configured_openapi_plugin(),
        PluginCheckProvider::Logs => check_configured_logs_plugin(),
        PluginCheckProvider::Db => check_configured_db_plugin(),
    }
}

fn check_configured_openapi_plugin() -> Result<PluginCheckSummary, PluginCheckError> {
    let search =
        ConduitConfig::load_current_dir_for_openapi().map_err(|error| PluginCheckError {
            message: error.message,
        })?;
    let Some(config) = search.config else {
        let message = if search.found_any_config {
            "openapi provider is not configured in .conduit/conduit.toml"
        } else {
            "openapi provider is not configured; expected .conduit/conduit.toml"
        };
        return Err(PluginCheckError::new(message));
    };
    let Some(plugin) = config.openapi_plugin().map_err(|error| PluginCheckError {
        message: error.message,
    })?
    else {
        return Err(PluginCheckError::new(
            "openapi provider is not configured in .conduit/conduit.toml",
        ));
    };

    PluginCheckSummary::from_openapi_plugin(&plugin.path, plugin.capabilities)
}

fn check_configured_logs_plugin() -> Result<PluginCheckSummary, PluginCheckError> {
    let search = ConduitConfig::load_current_dir_for_logs().map_err(|error| PluginCheckError {
        message: error.message,
    })?;
    let Some(config) = search.config else {
        let message = if search.found_any_config {
            "logs provider is not configured in .conduit/conduit.toml"
        } else {
            "logs provider is not configured; expected .conduit/conduit.toml"
        };
        return Err(PluginCheckError::new(message));
    };
    let Some(plugin) = config.logs_plugin().map_err(|error| PluginCheckError {
        message: error.message,
    })?
    else {
        return Err(PluginCheckError::new(
            "logs provider is not configured as a plugin in .conduit/conduit.toml",
        ));
    };

    PluginCheckSummary::from_logs_plugin(&plugin.path, plugin.capabilities)
}

fn check_configured_db_plugin() -> Result<PluginCheckSummary, PluginCheckError> {
    let search = ConduitConfig::load_current_dir_for_db().map_err(|error| PluginCheckError {
        message: error.message,
    })?;
    let Some(config) = search.config else {
        let message = if search.found_any_config {
            "db provider is not configured in .conduit/conduit.toml"
        } else {
            "db provider is not configured; expected .conduit/conduit.toml"
        };
        return Err(PluginCheckError::new(message));
    };
    let Some(plugin) = config.db_plugin().map_err(|error| PluginCheckError {
        message: error.message,
    })?
    else {
        return Err(PluginCheckError::new(
            "db provider is not configured as a plugin in .conduit/conduit.toml",
        ));
    };

    PluginCheckSummary::from_db_plugin(&plugin.path, plugin.capabilities)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_compact_summary() {
        let summary = PluginCheckSummary {
            status: "ok".to_string(),
            path: ".conduit/plugins/company.wasm".to_string(),
            id: "company".to_string(),
            version: "0.1.0".to_string(),
            protocol_version: "1".to_string(),
            providers: vec!["openapi-provider-v1".to_string()],
        };

        assert_eq!(
            summary.render_text(),
            "status: ok\npath: .conduit/plugins/company.wasm\nid: company\nversion: 0.1.0\nprotocol_version: 1\nproviders: 1\nprovider: openapi-provider-v1"
        );
    }
}
