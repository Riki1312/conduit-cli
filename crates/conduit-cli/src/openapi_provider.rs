use crate::config::ConduitConfig;
use crate::openapi::{FixtureOpenApiProvider, OpenApiProvider};
use crate::plugin_bindings::openapi::exports::conduit::plugin::metadata::PluginMetadata;
use crate::plugin_runtime::{PluginRuntime, PluginRuntimeError};

const EXPECTED_PROTOCOL_VERSION: &str = "1";
const OPENAPI_PROVIDER: &str = "openapi-provider-v1";

type OpenApiProviderResult = Result<Box<dyn OpenApiProvider>, OpenApiProviderLoadError>;

pub(crate) fn configured_openapi_provider() -> OpenApiProviderResult {
    let search =
        ConduitConfig::load_current_dir_for_openapi().map_err(OpenApiProviderLoadError::from)?;
    let Some(config) = search.config else {
        if search.found_any_config {
            return Err(OpenApiProviderLoadError {
                message: "openapi provider is not configured in .conduit/conduit.toml".to_string(),
            });
        }
        return Ok(Box::new(FixtureOpenApiProvider));
    };
    let Some(plugin) = config
        .openapi_plugin()
        .map_err(OpenApiProviderLoadError::from)?
    else {
        return Err(OpenApiProviderLoadError {
            message: "openapi provider is not configured in .conduit/conduit.toml".to_string(),
        });
    };

    let runtime = PluginRuntime::new().map_err(OpenApiProviderLoadError::from)?;
    let provider = runtime
        .instantiate_openapi_provider_with_capabilities(plugin.path, plugin.capabilities)
        .map_err(OpenApiProviderLoadError::from)?;
    validate_openapi_plugin_metadata(
        &provider
            .metadata()
            .map_err(OpenApiProviderLoadError::from)?,
    )?;

    Ok(Box::new(provider))
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct OpenApiProviderLoadError {
    pub(crate) message: String,
}

impl From<crate::config::ConfigError> for OpenApiProviderLoadError {
    fn from(error: crate::config::ConfigError) -> Self {
        Self {
            message: error.message,
        }
    }
}

impl From<PluginRuntimeError> for OpenApiProviderLoadError {
    fn from(error: PluginRuntimeError) -> Self {
        Self {
            message: error.message,
        }
    }
}

impl From<crate::openapi::OpenApiError> for OpenApiProviderLoadError {
    fn from(error: crate::openapi::OpenApiError) -> Self {
        Self {
            message: error.message,
        }
    }
}

pub(crate) fn validate_openapi_plugin_metadata(
    metadata: &PluginMetadata,
) -> Result<(), OpenApiProviderLoadError> {
    if metadata.protocol_version != EXPECTED_PROTOCOL_VERSION {
        return Err(OpenApiProviderLoadError {
            message: format!(
                "plugin `{}` uses unsupported protocol version `{}`; expected `{}`",
                metadata.id, metadata.protocol_version, EXPECTED_PROTOCOL_VERSION
            ),
        });
    }

    if !metadata
        .providers
        .iter()
        .any(|provider| provider == OPENAPI_PROVIDER)
    {
        return Err(OpenApiProviderLoadError {
            message: format!(
                "plugin `{}` does not declare provider `{}`",
                metadata.id, OPENAPI_PROVIDER
            ),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_supported_openapi_provider_metadata() {
        let metadata = metadata("fixture", "1", ["openapi-provider-v1"]);

        validate_openapi_plugin_metadata(&metadata).unwrap();
    }

    #[test]
    fn rejects_unsupported_protocol_version() {
        let metadata = metadata("fixture", "2", ["openapi-provider-v1"]);

        let error = validate_openapi_plugin_metadata(&metadata).unwrap_err();

        assert_eq!(
            error.message,
            "plugin `fixture` uses unsupported protocol version `2`; expected `1`"
        );
    }

    #[test]
    fn rejects_missing_openapi_provider_declaration() {
        let metadata = metadata("fixture", "1", ["logs-provider-v1"]);

        let error = validate_openapi_plugin_metadata(&metadata).unwrap_err();

        assert_eq!(
            error.message,
            "plugin `fixture` does not declare provider `openapi-provider-v1`"
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
