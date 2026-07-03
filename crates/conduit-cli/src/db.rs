use serde::Serialize;

pub(crate) const DEFAULT_DB_READ_LIMIT: usize = 20;
pub(crate) const MAX_DB_READ_LIMIT: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DbResourceRequest {
    pub(crate) service: String,
    pub(crate) environment: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DbDescribeRequest {
    pub(crate) service: String,
    pub(crate) resource: String,
    pub(crate) environment: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DbReadRequest {
    pub(crate) service: String,
    pub(crate) resource: String,
    pub(crate) environment: Option<String>,
    pub(crate) id: Option<String>,
    pub(crate) filters: Vec<DbFilter>,
    pub(crate) limit: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct DbFilter {
    pub(crate) field: String,
    pub(crate) value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct DbResourceList {
    pub(crate) provider: String,
    pub(crate) service: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) environment: Option<String>,
    pub(crate) resources: Vec<DbResource>,
}

impl DbResourceList {
    pub(crate) fn render_text(&self) -> String {
        let mut lines = vec![
            format!("provider: {}", self.provider),
            format!("service: {}", self.service),
        ];
        push_optional(&mut lines, "environment", self.environment.as_ref());
        lines.push(format!("resources: {}", self.resources.len()));

        for resource in &self.resources {
            lines.push(String::new());
            lines.push(format!("resource: {}", resource.name));
        }

        lines.join("\n")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct DbResource {
    pub(crate) name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct DbResourceDescription {
    pub(crate) provider: String,
    pub(crate) service: String,
    pub(crate) resource: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) environment: Option<String>,
    pub(crate) id_field: String,
    pub(crate) fields: Vec<DbField>,
}

impl DbResourceDescription {
    pub(crate) fn render_text(&self) -> String {
        let mut lines = vec![
            format!("provider: {}", self.provider),
            format!("service: {}", self.service),
            format!("resource: {}", self.resource),
        ];
        push_optional(&mut lines, "environment", self.environment.as_ref());
        lines.push(format!("id_field: {}", self.id_field));
        lines.push(format!("fields: {}", self.fields.len()));

        for field in &self.fields {
            lines.push(String::new());
            lines.push(format!("field: {}", field.name));
            push_optional(&mut lines, "type", field.kind.as_ref());
        }

        lines.join("\n")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct DbField {
    pub(crate) name: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub(crate) kind: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct DbReadResult {
    pub(crate) status: DbStatus,
    pub(crate) provider: String,
    pub(crate) service: String,
    pub(crate) resource: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) environment: Option<String>,
    pub(crate) matched: usize,
    pub(crate) shown: usize,
    pub(crate) records: Vec<serde_json::Value>,
}

impl DbReadResult {
    pub(crate) fn render_text(&self) -> String {
        let mut lines = vec![
            format!("status: {}", self.status.as_str()),
            format!("provider: {}", self.provider),
            format!("service: {}", self.service),
            format!("resource: {}", self.resource),
        ];
        push_optional(&mut lines, "environment", self.environment.as_ref());
        lines.push(format!("matched: {}", self.matched));
        lines.push(format!("shown: {}", self.shown));

        for record in &self.records {
            lines.push(String::new());
            lines.push("record:".to_string());
            if let Some(fields) = record.as_object() {
                for (name, value) in fields {
                    lines.push(format!("  {name}: {}", render_json_value(value)));
                }
            } else {
                lines.push(format!("  value: {}", render_json_value(record)));
            }
        }

        lines.join("\n")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DbStatus {
    Ok,
    Partial,
    AuthRequired,
    Unavailable,
    InvalidRequest,
    Error,
}

impl DbStatus {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Partial => "partial",
            Self::AuthRequired => "auth_required",
            Self::Unavailable => "unavailable",
            Self::InvalidRequest => "invalid_request",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct DbError {
    pub(crate) kind: DbErrorKind,
    pub(crate) message: String,
}

impl DbError {
    pub(crate) fn new(kind: DbErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self::new(DbErrorKind::NotFound, message)
    }

    fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(DbErrorKind::InvalidRequest, message)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DbErrorKind {
    AuthRequired,
    Internal,
    InvalidRequest,
    NotFound,
    PermissionDenied,
    Unavailable,
    Unsupported,
}

pub(crate) trait DbProvider {
    fn resources(&self, request: &DbResourceRequest) -> Result<DbResourceList, DbError>;
    fn describe(&self, request: &DbDescribeRequest) -> Result<DbResourceDescription, DbError>;
    fn read(&self, request: &DbReadRequest) -> Result<DbReadResult, DbError>;
}

pub(crate) struct FixtureDbProvider;

impl DbProvider for FixtureDbProvider {
    fn resources(&self, request: &DbResourceRequest) -> Result<DbResourceList, DbError> {
        if request.service != "checkout-service" {
            return Err(DbError::not_found(format!(
                "db service not found: {}",
                request.service
            )));
        }

        Ok(DbResourceList {
            provider: "fixture-db".to_string(),
            service: request.service.clone(),
            environment: request.environment.clone(),
            resources: vec![DbResource {
                name: "payment_account".to_string(),
            }],
        })
    }

    fn describe(&self, request: &DbDescribeRequest) -> Result<DbResourceDescription, DbError> {
        ensure_fixture_resource(&request.service, &request.resource)?;

        Ok(DbResourceDescription {
            provider: "fixture-db".to_string(),
            service: request.service.clone(),
            resource: request.resource.clone(),
            environment: request.environment.clone(),
            id_field: "id".to_string(),
            fields: vec![
                DbField {
                    name: "id".to_string(),
                    kind: Some("string".to_string()),
                },
                DbField {
                    name: "status".to_string(),
                    kind: Some("string".to_string()),
                },
                DbField {
                    name: "currency".to_string(),
                    kind: Some("string".to_string()),
                },
                DbField {
                    name: "created_at".to_string(),
                    kind: Some("timestamp".to_string()),
                },
            ],
        })
    }

    fn read(&self, request: &DbReadRequest) -> Result<DbReadResult, DbError> {
        ensure_fixture_resource(&request.service, &request.resource)?;

        if request.id.is_some() && !request.filters.is_empty() {
            return Err(DbError::invalid_request(
                "`--id` cannot be combined with `--filter`",
            ));
        }

        let mut records = fixture_records();
        if let Some(id) = &request.id {
            records.retain(|record| record.get("id").and_then(|value| value.as_str()) == Some(id));
        }
        for filter in &request.filters {
            records.retain(|record| {
                record
                    .get(&filter.field)
                    .is_some_and(|value| json_value_matches(value, &filter.value))
            });
        }

        let matched = records.len();
        records.truncate(request.limit);

        Ok(DbReadResult {
            status: DbStatus::Ok,
            provider: "fixture-db".to_string(),
            service: request.service.clone(),
            resource: request.resource.clone(),
            environment: request.environment.clone(),
            matched,
            shown: records.len(),
            records,
        })
    }
}

fn ensure_fixture_resource(service: &str, resource: &str) -> Result<(), DbError> {
    if service != "checkout-service" {
        return Err(DbError::not_found(format!(
            "db service not found: {service}"
        )));
    }
    if resource != "payment_account" {
        return Err(DbError::not_found(format!(
            "db resource not found: {resource} for service {service}"
        )));
    }
    Ok(())
}

fn fixture_records() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "id": "acc_123",
            "status": "ACTIVE",
            "currency": "EUR",
            "created_at": "2026-06-16T08:12:01Z"
        }),
        serde_json::json!({
            "id": "acc_456",
            "status": "DISABLED",
            "currency": "EUR",
            "created_at": "2026-06-17T09:34:11Z"
        }),
    ]
}

fn json_value_matches(value: &serde_json::Value, expected: &str) -> bool {
    match value {
        serde_json::Value::String(value) => value == expected,
        serde_json::Value::Bool(value) => value.to_string() == expected,
        serde_json::Value::Number(value) => value.to_string() == expected,
        _ => false,
    }
}

fn render_json_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::Null => "null".to_string(),
        _ => value.to_string(),
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
    fn fixture_reads_by_id() {
        let result = FixtureDbProvider
            .read(&DbReadRequest {
                service: "checkout-service".to_string(),
                resource: "payment_account".to_string(),
                environment: Some("test".to_string()),
                id: Some("acc_123".to_string()),
                filters: vec![],
                limit: DEFAULT_DB_READ_LIMIT,
            })
            .expect("read succeeds");

        assert_eq!(result.matched, 1);
        assert_eq!(result.records[0]["status"], "ACTIVE");
    }

    #[test]
    fn fixture_reads_by_filter() {
        let result = FixtureDbProvider
            .read(&DbReadRequest {
                service: "checkout-service".to_string(),
                resource: "payment_account".to_string(),
                environment: Some("test".to_string()),
                id: None,
                filters: vec![DbFilter {
                    field: "status".to_string(),
                    value: "DISABLED".to_string(),
                }],
                limit: DEFAULT_DB_READ_LIMIT,
            })
            .expect("read succeeds");

        assert_eq!(result.matched, 1);
        assert_eq!(result.records[0]["id"], "acc_456");
    }

    #[test]
    fn renders_minimal_description() {
        let description = FixtureDbProvider
            .describe(&DbDescribeRequest {
                service: "checkout-service".to_string(),
                resource: "payment_account".to_string(),
                environment: None,
            })
            .expect("describe succeeds");

        assert_eq!(description.id_field, "id");
        assert_eq!(description.fields.len(), 4);
        assert!(description.render_text().contains("field: currency\n"));
        assert!(!description.render_text().contains("sensitive:"));
    }
}
