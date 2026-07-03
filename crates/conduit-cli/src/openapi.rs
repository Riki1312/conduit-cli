use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OpenApiRequest {
    pub(crate) service: String,
    pub(crate) environment: Option<String>,
    pub(crate) method: Option<String>,
    pub(crate) path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct OpenApiParameter {
    pub(crate) name: String,
    pub(crate) location: OpenApiParameterLocation,
    pub(crate) required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) schema_json: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OpenApiParameterLocation {
    Path,
    Query,
    Header,
    Cookie,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct OpenApiOperation {
    pub(crate) service: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) environment: Option<String>,
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) parameters: Vec<OpenApiParameter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) operation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) request_schema_json: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) response_schema_json: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) source: Option<String>,
}

impl OpenApiOperation {
    pub(crate) fn render_text(&self) -> String {
        let mut lines = vec![
            format!("service: {}", self.service),
            format!("method: {}", self.method),
            format!("path: {}", self.path),
        ];

        push_optional(&mut lines, "environment", self.environment.as_ref());
        push_optional(&mut lines, "operation_id", self.operation_id.as_ref());
        push_optional(&mut lines, "summary", self.summary.as_ref());
        push_optional(&mut lines, "description", self.description.as_ref());
        if !self.parameters.is_empty() {
            lines.push(format!("parameters: {}", self.parameters.len()));
            lines.extend(self.parameters.iter().map(OpenApiParameter::render_text));
        }
        push_optional(&mut lines, "source", self.source.as_ref());

        lines.join("\n")
    }
}

impl OpenApiParameter {
    fn render_text(&self) -> String {
        let required = if self.required {
            "required"
        } else {
            "optional"
        };
        format!(
            "parameter: {} {} {}",
            self.location.as_str(),
            self.name,
            required
        )
    }
}

impl OpenApiParameterLocation {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Path => "path",
            Self::Query => "query",
            Self::Header => "header",
            Self::Cookie => "cookie",
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub(crate) struct OpenApiOperationList {
    pub(crate) service: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) environment: Option<String>,
    pub(crate) operations: Vec<OpenApiOperation>,
}

impl OpenApiOperationList {
    pub(crate) fn render_text(&self) -> String {
        let mut lines = vec![
            format!("service: {}", self.service),
            format!("operations: {}", self.operations.len()),
        ];
        push_optional(&mut lines, "environment", self.environment.as_ref());

        for operation in &self.operations {
            lines.push(String::new());
            lines.push(format!(
                "operation: {} {}",
                operation.method, operation.path
            ));
            push_optional(&mut lines, "operation_id", operation.operation_id.as_ref());
            push_optional(&mut lines, "summary", operation.summary.as_ref());
            push_optional(&mut lines, "source", operation.source.as_ref());
        }

        lines.join("\n")
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct OpenApiError {
    pub(crate) kind: OpenApiErrorKind,
    pub(crate) message: String,
    pub(crate) details: Option<String>,
    pub(crate) source: Option<String>,
}

impl OpenApiError {
    pub(crate) fn new(kind: OpenApiErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            details: None,
            source: None,
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self::new(OpenApiErrorKind::NotFound, message)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum OpenApiErrorKind {
    AuthRequired,
    Internal,
    InvalidRequest,
    NotFound,
    PermissionDenied,
    Unavailable,
    Unsupported,
}

pub(crate) trait OpenApiProvider {
    fn operation(&self, request: &OpenApiRequest) -> Result<OpenApiOperation, OpenApiError>;
    fn list(&self, request: &OpenApiRequest) -> Result<OpenApiOperationList, OpenApiError>;
}

pub(crate) struct FixtureOpenApiProvider;

impl OpenApiProvider for FixtureOpenApiProvider {
    fn operation(&self, request: &OpenApiRequest) -> Result<OpenApiOperation, OpenApiError> {
        let method = request
            .method
            .as_deref()
            .ok_or_else(|| OpenApiError::not_found("openapi operation requires a method"))?;
        let path = request
            .path
            .as_deref()
            .ok_or_else(|| OpenApiError::not_found("openapi operation requires a path"))?;

        fixture_operations(&request.service, request.environment.as_deref())
            .into_iter()
            .find(|operation| {
                operation.method.eq_ignore_ascii_case(method) && operation.path == path
            })
            .ok_or_else(|| {
                OpenApiError::not_found(format!(
                    "operation not found: {} {} for service {}",
                    method, path, request.service
                ))
            })
    }

    fn list(&self, request: &OpenApiRequest) -> Result<OpenApiOperationList, OpenApiError> {
        let operations = fixture_operations(&request.service, request.environment.as_deref());

        if operations.is_empty() {
            return Err(OpenApiError::not_found(format!(
                "service not found: {}",
                request.service
            )));
        }

        Ok(OpenApiOperationList {
            service: request.service.clone(),
            environment: request.environment.clone(),
            operations,
        })
    }
}

fn fixture_operations(service: &str, environment: Option<&str>) -> Vec<OpenApiOperation> {
    match service {
        "catalog-service" => vec![
            OpenApiOperation {
                service: service.to_string(),
                environment: environment.map(str::to_string),
                method: "GET".to_string(),
                path: "/items".to_string(),
                parameters: Vec::new(),
                operation_id: Some("listItems".to_string()),
                summary: Some("List catalog items".to_string()),
                description: None,
                request_schema_json: None,
                response_schema_json: Some(r#"{"type":"object"}"#.to_string()),
                source: Some("fixture://catalog-service/openapi.json".to_string()),
            },
            OpenApiOperation {
                service: service.to_string(),
                environment: environment.map(str::to_string),
                method: "GET".to_string(),
                path: "/items/{item_id}/prices".to_string(),
                parameters: vec![OpenApiParameter {
                    name: "item_id".to_string(),
                    location: OpenApiParameterLocation::Path,
                    required: true,
                    description: Some("Item id".to_string()),
                    schema_json: Some(r#"{"type":"string"}"#.to_string()),
                }],
                operation_id: Some("getItemPrices".to_string()),
                summary: Some("Load prices for a catalog item".to_string()),
                description: None,
                request_schema_json: None,
                response_schema_json: Some(r#"{"type":"object"}"#.to_string()),
                source: Some("fixture://catalog-service/openapi.json".to_string()),
            },
        ],
        _ => Vec::new(),
    }
}

fn push_optional(lines: &mut Vec<String>, key: &str, value: Option<&String>) {
    if let Some(value) = value {
        lines.push(format!("{key}: {value}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_provider_returns_operation() {
        let operation = FixtureOpenApiProvider
            .operation(&OpenApiRequest {
                service: "catalog-service".to_string(),
                environment: Some("staging".to_string()),
                method: Some("GET".to_string()),
                path: Some("/items".to_string()),
            })
            .unwrap();

        assert_eq!(operation.operation_id.as_deref(), Some("listItems"));
        assert_eq!(operation.environment.as_deref(), Some("staging"));
    }

    #[test]
    fn operation_text_renders_parameters() {
        let operation = FixtureOpenApiProvider
            .operation(&OpenApiRequest {
                service: "catalog-service".to_string(),
                environment: None,
                method: Some("GET".to_string()),
                path: Some("/items/{item_id}/prices".to_string()),
            })
            .unwrap();

        assert!(operation.render_text().contains("parameters: 1\n"));
        assert!(
            operation
                .render_text()
                .contains("parameter: path item_id required\n")
        );
    }

    #[test]
    fn fixture_provider_lists_operations() {
        let list = FixtureOpenApiProvider
            .list(&OpenApiRequest {
                service: "catalog-service".to_string(),
                environment: None,
                method: None,
                path: None,
            })
            .unwrap();

        assert_eq!(list.operations.len(), 2);
    }
}
