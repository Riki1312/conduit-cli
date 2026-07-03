use crate::config::PluginCapabilities;
use crate::db::{
    DbDescribeRequest, DbError, DbErrorKind, DbField, DbFilter, DbProvider, DbReadRequest,
    DbReadResult, DbResource, DbResourceDescription, DbResourceList, DbResourceRequest, DbStatus,
};
use crate::logs::{
    LogAuthRequest, LogAuthResult, LogAuthStatus, LogDiagnostic, LogError, LogErrorKind, LogEvent,
    LogProvider, LogQuery, LogSearchResult, LogStatus, LogTimeRange,
};
use crate::openapi::{
    OpenApiError, OpenApiErrorKind, OpenApiOperation, OpenApiOperationList, OpenApiParameter,
    OpenApiParameterLocation, OpenApiProvider, OpenApiRequest,
};
use crate::plugin_bindings::db::DbProvider as ComponentDbProvider;
use crate::plugin_bindings::db::exports::conduit::plugin::db_provider_v1::{
    DbResource as PluginDbResource, DescribeRequest as PluginDbDescribeRequest,
    FieldDescription as PluginDbFieldDescription, FieldFilter as PluginDbFieldFilter,
    ProviderError as PluginDbProviderError, ProviderErrorKind as PluginDbProviderErrorKind,
    ReadRequest as PluginDbReadRequest, ReadResult as PluginDbReadResult,
    ReadStatus as PluginDbReadStatus, ResourceDescription as PluginDbResourceDescription,
    ResourceList as PluginDbResourceList, ResourceRequest as PluginDbResourceRequest,
};
use crate::plugin_bindings::db::exports::conduit::plugin::metadata::PluginMetadata as DbPluginMetadata;
use crate::plugin_bindings::logs::LogsProvider;
use crate::plugin_bindings::logs::exports::conduit::plugin::logs_provider_v1::{
    AuthRequest as PluginLogAuthRequest, AuthResult as PluginLogAuthResult,
    AuthStatus as PluginLogAuthStatus, Diagnostic as PluginLogDiagnostic,
    LogEvent as PluginLogEvent, LogQuery as PluginLogQuery, LogStatus as PluginLogStatus,
    ProviderError as PluginLogProviderError, ProviderErrorKind as PluginLogProviderErrorKind,
    SearchResult as PluginLogSearchResult, TimeRange as PluginLogTimeRange,
};
use crate::plugin_bindings::logs::exports::conduit::plugin::metadata::PluginMetadata as LogsPluginMetadata;
use crate::plugin_bindings::openapi::OpenapiProvider;
use crate::plugin_bindings::openapi::exports::conduit::plugin::metadata::PluginMetadata as OpenApiPluginMetadata;
use crate::plugin_bindings::openapi::exports::conduit::plugin::openapi_provider_v1::{
    Operation, OperationRequest, Parameter, ParameterLocation, ProviderError, ProviderErrorKind,
};
use crate::plugin_host::PluginHostState;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Cache, CacheConfig, Config, Engine, Store, Strategy};

const WASMTIME_CACHE_DIR: &str = ".conduit/state/wasmtime-cache";

#[derive(Debug)]
pub(crate) struct PluginRuntime {
    engine: Engine,
}

impl PluginRuntime {
    pub(crate) fn new() -> Result<Self, PluginRuntimeError> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        config.strategy(Strategy::Cranelift);
        config.memory_init_cow(true);
        config.signals_based_traps(true);
        configure_compilation_cache(&mut config);
        let engine = Engine::new(&config).map_err(PluginRuntimeError::engine)?;

        Ok(Self { engine })
    }

    pub(crate) fn instantiate_openapi_provider_with_capabilities(
        &self,
        path: impl AsRef<Path>,
        capabilities: PluginCapabilities,
    ) -> Result<WasmtimeOpenApiProvider, PluginRuntimeError> {
        let path = path.as_ref();
        let component = Component::from_file(&self.engine, path)
            .map_err(|source| PluginRuntimeError::component(path.to_path_buf(), source))?;
        let mut linker = Linker::new(&self.engine);
        OpenapiProvider::add_to_linker::<_, HasSelf<_>>(&mut linker, |state| state)
            .map_err(|source| PluginRuntimeError::linker(path.to_path_buf(), source))?;
        let mut store = Store::new(&self.engine, PluginHostState::new(capabilities));
        let bindings = OpenapiProvider::instantiate(&mut store, &component, &linker)
            .map_err(|source| PluginRuntimeError::instantiate(path.to_path_buf(), source))?;

        Ok(WasmtimeOpenApiProvider {
            inner: Mutex::new(WasmtimeOpenApiProviderState { store, bindings }),
        })
    }

    pub(crate) fn instantiate_logs_provider_with_capabilities(
        &self,
        path: impl AsRef<Path>,
        capabilities: PluginCapabilities,
    ) -> Result<WasmtimeLogsProvider, PluginRuntimeError> {
        let path = path.as_ref();
        let component = Component::from_file(&self.engine, path)
            .map_err(|source| PluginRuntimeError::component(path.to_path_buf(), source))?;
        let mut linker = Linker::new(&self.engine);
        LogsProvider::add_to_linker::<_, HasSelf<_>>(&mut linker, |state| state)
            .map_err(|source| PluginRuntimeError::linker(path.to_path_buf(), source))?;
        let mut store = Store::new(&self.engine, PluginHostState::new(capabilities));
        let bindings = LogsProvider::instantiate(&mut store, &component, &linker)
            .map_err(|source| PluginRuntimeError::instantiate(path.to_path_buf(), source))?;

        Ok(WasmtimeLogsProvider {
            inner: Mutex::new(WasmtimeLogsProviderState { store, bindings }),
        })
    }

    pub(crate) fn instantiate_db_provider_with_capabilities(
        &self,
        path: impl AsRef<Path>,
        capabilities: PluginCapabilities,
    ) -> Result<WasmtimeDbProvider, PluginRuntimeError> {
        let path = path.as_ref();
        let component = Component::from_file(&self.engine, path)
            .map_err(|source| PluginRuntimeError::component(path.to_path_buf(), source))?;
        let mut linker = Linker::new(&self.engine);
        ComponentDbProvider::add_to_linker::<_, HasSelf<_>>(&mut linker, |state| state)
            .map_err(|source| PluginRuntimeError::linker(path.to_path_buf(), source))?;
        let mut store = Store::new(&self.engine, PluginHostState::new(capabilities));
        let bindings = ComponentDbProvider::instantiate(&mut store, &component, &linker)
            .map_err(|source| PluginRuntimeError::instantiate(path.to_path_buf(), source))?;

        Ok(WasmtimeDbProvider {
            inner: Mutex::new(WasmtimeDbProviderState { store, bindings }),
        })
    }
}

fn configure_compilation_cache(config: &mut Config) {
    let Ok(cache_dir) = absolute_cache_dir() else {
        return;
    };
    if fs::create_dir_all(&cache_dir).is_err() {
        return;
    }

    let mut cache_config = CacheConfig::new();
    cache_config.with_directory(cache_dir);
    if let Ok(cache) = Cache::new(cache_config) {
        config.cache(Some(cache));
    }
}

fn absolute_cache_dir() -> Result<PathBuf, std::io::Error> {
    Ok(std::env::current_dir()?.join(WASMTIME_CACHE_DIR))
}

pub(crate) struct WasmtimeOpenApiProvider {
    inner: Mutex<WasmtimeOpenApiProviderState>,
}

impl WasmtimeOpenApiProvider {
    pub(crate) fn metadata(&self) -> Result<OpenApiPluginMetadata, OpenApiError> {
        let mut inner = self.lock()?;
        let WasmtimeOpenApiProviderState { store, bindings } = &mut *inner;
        let metadata = bindings
            .conduit_plugin_metadata()
            .call_metadata(store)
            .map_err(runtime_openapi_error)?;

        Ok(metadata)
    }

    fn lock(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, WasmtimeOpenApiProviderState>, OpenApiError> {
        self.inner.lock().map_err(|_| {
            OpenApiError::new(
                OpenApiErrorKind::Internal,
                "openapi plugin provider state is unavailable",
            )
        })
    }
}

impl OpenApiProvider for WasmtimeOpenApiProvider {
    fn operation(&self, request: &OpenApiRequest) -> Result<OpenApiOperation, OpenApiError> {
        let mut inner = self.lock()?;
        let request = operation_request(request);
        let WasmtimeOpenApiProviderState { store, bindings } = &mut *inner;
        let operation = bindings
            .conduit_plugin_openapi_provider_v1()
            .call_get_operation(store, &request)
            .map_err(runtime_openapi_error)?
            .map_err(provider_openapi_error)?;

        Ok(openapi_operation(operation))
    }

    fn list(&self, request: &OpenApiRequest) -> Result<OpenApiOperationList, OpenApiError> {
        let mut inner = self.lock()?;
        let plugin_request = operation_request(request);
        let WasmtimeOpenApiProviderState { store, bindings } = &mut *inner;
        let operations = bindings
            .conduit_plugin_openapi_provider_v1()
            .call_operations(store, &plugin_request)
            .map_err(runtime_openapi_error)?
            .map_err(provider_openapi_error)?
            .into_iter()
            .map(openapi_operation)
            .collect();

        Ok(OpenApiOperationList {
            service: request.service.clone(),
            environment: request.environment.clone(),
            operations,
        })
    }
}

struct WasmtimeOpenApiProviderState {
    store: Store<PluginHostState>,
    bindings: OpenapiProvider,
}

pub(crate) struct WasmtimeLogsProvider {
    inner: Mutex<WasmtimeLogsProviderState>,
}

impl WasmtimeLogsProvider {
    pub(crate) fn metadata(&self) -> Result<LogsPluginMetadata, LogError> {
        let mut inner = self.lock()?;
        let WasmtimeLogsProviderState { store, bindings } = &mut *inner;
        bindings
            .conduit_plugin_metadata()
            .call_metadata(store)
            .map_err(runtime_log_error)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, WasmtimeLogsProviderState>, LogError> {
        self.inner.lock().map_err(|_| {
            LogError::new(
                LogErrorKind::Internal,
                "logs plugin provider state is unavailable",
            )
        })
    }
}

impl LogProvider for WasmtimeLogsProvider {
    fn search(&self, query: &LogQuery) -> Result<LogSearchResult, LogError> {
        let mut inner = self.lock()?;
        let plugin_query = plugin_log_query(query)?;
        let WasmtimeLogsProviderState { store, bindings } = &mut *inner;
        let result = bindings
            .conduit_plugin_logs_provider_v1()
            .call_search(store, &plugin_query)
            .map_err(runtime_log_error)?
            .map_err(provider_log_error)?;

        log_search_result(result)
    }

    fn authenticate(&self, request: &LogAuthRequest) -> Result<LogAuthResult, LogError> {
        let mut inner = self.lock()?;
        let plugin_request = plugin_log_auth_request(request);
        let WasmtimeLogsProviderState { store, bindings } = &mut *inner;
        let result = bindings
            .conduit_plugin_logs_provider_v1()
            .call_authenticate(store, &plugin_request)
            .map_err(runtime_log_error)?
            .map_err(provider_log_error)?;

        Ok(log_auth_result(result))
    }
}

struct WasmtimeLogsProviderState {
    store: Store<PluginHostState>,
    bindings: LogsProvider,
}

pub(crate) struct WasmtimeDbProvider {
    inner: Mutex<WasmtimeDbProviderState>,
}

impl WasmtimeDbProvider {
    pub(crate) fn metadata(&self) -> Result<DbPluginMetadata, DbError> {
        let mut inner = self.lock()?;
        let WasmtimeDbProviderState { store, bindings } = &mut *inner;
        bindings
            .conduit_plugin_metadata()
            .call_metadata(store)
            .map_err(runtime_db_error)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, WasmtimeDbProviderState>, DbError> {
        self.inner.lock().map_err(|_| {
            DbError::new(
                DbErrorKind::Internal,
                "db plugin provider state is unavailable",
            )
        })
    }
}

impl DbProvider for WasmtimeDbProvider {
    fn resources(&self, request: &DbResourceRequest) -> Result<DbResourceList, DbError> {
        let mut inner = self.lock()?;
        let plugin_request = plugin_db_resource_request(request);
        let WasmtimeDbProviderState { store, bindings } = &mut *inner;
        let result = bindings
            .conduit_plugin_db_provider_v1()
            .call_resources(store, &plugin_request)
            .map_err(runtime_db_error)?
            .map_err(provider_db_error)?;

        Ok(db_resource_list(result))
    }

    fn describe(&self, request: &DbDescribeRequest) -> Result<DbResourceDescription, DbError> {
        let mut inner = self.lock()?;
        let plugin_request = plugin_db_describe_request(request);
        let WasmtimeDbProviderState { store, bindings } = &mut *inner;
        let result = bindings
            .conduit_plugin_db_provider_v1()
            .call_describe(store, &plugin_request)
            .map_err(runtime_db_error)?
            .map_err(provider_db_error)?;

        Ok(db_resource_description(result))
    }

    fn read(&self, request: &DbReadRequest) -> Result<DbReadResult, DbError> {
        let mut inner = self.lock()?;
        let plugin_request = plugin_db_read_request(request)?;
        let WasmtimeDbProviderState { store, bindings } = &mut *inner;
        let result = bindings
            .conduit_plugin_db_provider_v1()
            .call_read(store, &plugin_request)
            .map_err(runtime_db_error)?
            .map_err(provider_db_error)?;

        db_read_result(result)
    }
}

struct WasmtimeDbProviderState {
    store: Store<PluginHostState>,
    bindings: ComponentDbProvider,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PluginRuntimeError {
    pub(crate) kind: PluginRuntimeErrorKind,
    pub(crate) message: String,
    pub(crate) path: Option<PathBuf>,
}

impl PluginRuntimeError {
    fn engine(source: wasmtime::Error) -> Self {
        Self {
            kind: PluginRuntimeErrorKind::Engine,
            message: format!("failed to create Wasmtime engine: {source}"),
            path: None,
        }
    }

    fn component(path: PathBuf, source: wasmtime::Error) -> Self {
        Self {
            kind: PluginRuntimeErrorKind::Component,
            message: format!("failed to load component {}: {source}", path.display()),
            path: Some(path),
        }
    }

    fn instantiate(path: PathBuf, source: wasmtime::Error) -> Self {
        Self {
            kind: PluginRuntimeErrorKind::Instantiate,
            message: format!(
                "failed to instantiate component {}: {source}",
                path.display()
            ),
            path: Some(path),
        }
    }

    fn linker(path: PathBuf, source: wasmtime::Error) -> Self {
        Self {
            kind: PluginRuntimeErrorKind::Linker,
            message: format!(
                "failed to configure component imports for {}: {source}",
                path.display()
            ),
            path: Some(path),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PluginRuntimeErrorKind {
    Engine,
    Component,
    Linker,
    Instantiate,
}

fn operation_request(request: &OpenApiRequest) -> OperationRequest {
    OperationRequest {
        service: request.service.clone(),
        environment: request.environment.clone(),
        method: request.method.clone(),
        path: request.path.clone(),
    }
}

fn openapi_operation(operation: Operation) -> OpenApiOperation {
    OpenApiOperation {
        service: operation.service,
        environment: operation.environment,
        method: operation.method,
        path: operation.path,
        parameters: operation
            .parameters
            .into_iter()
            .map(openapi_parameter)
            .collect(),
        operation_id: operation.operation_id,
        summary: operation.summary,
        description: operation.description,
        request_schema_json: operation.request_schema_json,
        response_schema_json: operation.response_schema_json,
        source: operation.source,
    }
}

fn openapi_parameter(parameter: Parameter) -> OpenApiParameter {
    OpenApiParameter {
        name: parameter.name,
        location: match parameter.location {
            ParameterLocation::Path => OpenApiParameterLocation::Path,
            ParameterLocation::Query => OpenApiParameterLocation::Query,
            ParameterLocation::Header => OpenApiParameterLocation::Header,
            ParameterLocation::Cookie => OpenApiParameterLocation::Cookie,
        },
        required: parameter.required,
        description: parameter.description,
        schema_json: parameter.schema_json,
    }
}

fn provider_openapi_error(error: ProviderError) -> OpenApiError {
    OpenApiError {
        kind: match error.kind {
            ProviderErrorKind::AuthRequired => OpenApiErrorKind::AuthRequired,
            ProviderErrorKind::Internal => OpenApiErrorKind::Internal,
            ProviderErrorKind::InvalidRequest => OpenApiErrorKind::InvalidRequest,
            ProviderErrorKind::NotFound => OpenApiErrorKind::NotFound,
            ProviderErrorKind::PermissionDenied => OpenApiErrorKind::PermissionDenied,
            ProviderErrorKind::Unavailable => OpenApiErrorKind::Unavailable,
            ProviderErrorKind::Unsupported => OpenApiErrorKind::Unsupported,
        },
        message: error.message,
        details: error.details,
        source: error.source,
    }
}

fn runtime_openapi_error(error: wasmtime::Error) -> OpenApiError {
    OpenApiError::new(
        OpenApiErrorKind::Internal,
        format!("openapi plugin runtime error: {error}"),
    )
}

fn plugin_log_query(query: &LogQuery) -> Result<PluginLogQuery, LogError> {
    let limit = u32::try_from(query.limit).map_err(|_| {
        LogError::new(
            LogErrorKind::InvalidRequest,
            "logs limit is too large for plugin provider request",
        )
    })?;

    Ok(PluginLogQuery {
        service: query.service.clone(),
        environment: query.environment.clone(),
        time_range: plugin_log_time_range(&query.time_range),
        limit,
        levels: query.levels.clone(),
        cid: query.cid.clone(),
        trace_id: query.trace_id.clone(),
        message: query.message.clone(),
        logger: query.logger.clone(),
        exclude_messages: query.exclude_messages.clone(),
        exclude_loggers: query.exclude_loggers.clone(),
        include_trace: query.include_trace,
        cursor: query.cursor.clone(),
    })
}

fn plugin_log_time_range(time_range: &LogTimeRange) -> PluginLogTimeRange {
    PluginLogTimeRange {
        from: time_range.from.clone(),
        to: time_range.to.clone(),
        source: time_range.source.clone(),
    }
}

fn plugin_log_auth_request(request: &LogAuthRequest) -> PluginLogAuthRequest {
    PluginLogAuthRequest {
        environment: request.environment.clone(),
        secret: request.secret.clone(),
        check: request.check,
    }
}

fn log_auth_result(result: PluginLogAuthResult) -> LogAuthResult {
    LogAuthResult {
        status: log_auth_status(result.status),
        provider: result.provider,
        environment: result.environment,
        destination: result.destination,
        expires_at: result.expires_at,
        diagnostics: result.diagnostics.into_iter().map(log_diagnostic).collect(),
    }
}

fn log_auth_status(status: PluginLogAuthStatus) -> LogAuthStatus {
    match status {
        PluginLogAuthStatus::Ok => LogAuthStatus::Ok,
        PluginLogAuthStatus::ActionRequired => LogAuthStatus::ActionRequired,
    }
}

fn log_search_result(result: PluginLogSearchResult) -> Result<LogSearchResult, LogError> {
    let logs = result.logs.into_iter().map(log_event).collect::<Vec<_>>();
    let matches = result
        .matches
        .map(usize::try_from)
        .transpose()
        .map_err(|_| {
            LogError::new(
                LogErrorKind::Internal,
                "logs provider returned a match count that is too large",
            )
        })?
        .unwrap_or(logs.len());
    let shown = usize::try_from(result.shown).map_err(|_| {
        LogError::new(
            LogErrorKind::Internal,
            "logs provider returned a shown count that is too large",
        )
    })?;

    Ok(LogSearchResult {
        status: log_status(result.status),
        provider: result.provider,
        service: result.service,
        environment: result.environment,
        time_range: log_time_range(result.time_range),
        matches,
        shown,
        logs,
        next_cursor: result.next_cursor,
        checked_until: result.checked_until,
        diagnostics: result.diagnostics.into_iter().map(log_diagnostic).collect(),
    })
}

fn log_time_range(time_range: PluginLogTimeRange) -> LogTimeRange {
    LogTimeRange {
        from: time_range.from,
        to: time_range.to,
        source: time_range.source,
    }
}

fn log_status(status: PluginLogStatus) -> LogStatus {
    match status {
        PluginLogStatus::Ok => LogStatus::Ok,
        PluginLogStatus::Partial => LogStatus::Partial,
        PluginLogStatus::AuthRequired => LogStatus::AuthRequired,
        PluginLogStatus::Unavailable => LogStatus::Unavailable,
        PluginLogStatus::InvalidRequest => LogStatus::InvalidRequest,
        PluginLogStatus::Error => LogStatus::Error,
    }
}

fn log_event(event: PluginLogEvent) -> LogEvent {
    LogEvent {
        id: event.id,
        timestamp: event.timestamp,
        level: event.level,
        service: event.service,
        environment: event.environment,
        cid: event.cid,
        trace_id: event.trace_id,
        logger: event.logger,
        message: event.message,
        stack_trace: event.stack_trace,
        source: event.source,
        attributes_json: event.attributes_json,
    }
}

fn log_diagnostic(diagnostic: PluginLogDiagnostic) -> LogDiagnostic {
    LogDiagnostic {
        kind: diagnostic.kind,
        hint: diagnostic.hint,
    }
}

fn provider_log_error(error: PluginLogProviderError) -> LogError {
    LogError::new(
        match error.kind {
            PluginLogProviderErrorKind::AuthRequired => LogErrorKind::AuthRequired,
            PluginLogProviderErrorKind::Internal => LogErrorKind::Internal,
            PluginLogProviderErrorKind::InvalidRequest => LogErrorKind::InvalidRequest,
            PluginLogProviderErrorKind::PermissionDenied => LogErrorKind::PermissionDenied,
            PluginLogProviderErrorKind::Unavailable => LogErrorKind::Unavailable,
            PluginLogProviderErrorKind::Unsupported => LogErrorKind::Unsupported,
        },
        error.message,
    )
}

fn runtime_log_error(error: wasmtime::Error) -> LogError {
    LogError::new(
        LogErrorKind::Internal,
        format!("logs plugin runtime error: {error}"),
    )
}

fn plugin_db_resource_request(request: &DbResourceRequest) -> PluginDbResourceRequest {
    PluginDbResourceRequest {
        service: request.service.clone(),
        environment: request.environment.clone(),
    }
}

fn plugin_db_describe_request(request: &DbDescribeRequest) -> PluginDbDescribeRequest {
    PluginDbDescribeRequest {
        service: request.service.clone(),
        resource_name: request.resource.clone(),
        environment: request.environment.clone(),
    }
}

fn plugin_db_read_request(request: &DbReadRequest) -> Result<PluginDbReadRequest, DbError> {
    let limit = u32::try_from(request.limit).map_err(|_| {
        DbError::new(
            DbErrorKind::InvalidRequest,
            "db read limit is too large for plugin provider request",
        )
    })?;

    Ok(PluginDbReadRequest {
        service: request.service.clone(),
        resource_name: request.resource.clone(),
        environment: request.environment.clone(),
        id: request.id.clone(),
        filters: request.filters.iter().map(plugin_db_filter).collect(),
        limit,
    })
}

fn plugin_db_filter(filter: &DbFilter) -> PluginDbFieldFilter {
    PluginDbFieldFilter {
        field: filter.field.clone(),
        value: filter.value.clone(),
    }
}

fn db_resource_list(list: PluginDbResourceList) -> DbResourceList {
    DbResourceList {
        provider: list.provider,
        service: list.service,
        environment: list.environment,
        resources: list.resources.into_iter().map(db_resource).collect(),
    }
}

fn db_resource(resource: PluginDbResource) -> DbResource {
    DbResource {
        name: resource.name,
    }
}

fn db_resource_description(description: PluginDbResourceDescription) -> DbResourceDescription {
    DbResourceDescription {
        provider: description.provider,
        service: description.service,
        resource: description.resource_name,
        environment: description.environment,
        id_field: description.id_field,
        fields: description.fields.into_iter().map(db_field).collect(),
    }
}

fn db_field(field: PluginDbFieldDescription) -> DbField {
    DbField {
        name: field.name,
        kind: field.data_type,
    }
}

fn db_read_result(result: PluginDbReadResult) -> Result<DbReadResult, DbError> {
    let records = result
        .records_json
        .into_iter()
        .map(|record| {
            serde_json::from_str(&record).map_err(|error| {
                DbError::new(
                    DbErrorKind::Internal,
                    format!("db plugin returned invalid record JSON: {error}"),
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let shown = usize::try_from(result.shown).map_err(|_| {
        DbError::new(
            DbErrorKind::Internal,
            "db plugin returned a shown count that is too large",
        )
    })?;
    let matched = result
        .matched
        .map(usize::try_from)
        .transpose()
        .map_err(|_| {
            DbError::new(
                DbErrorKind::Internal,
                "db plugin returned a matched count that is too large",
            )
        })?
        .unwrap_or(records.len());

    Ok(DbReadResult {
        status: db_status(result.status),
        provider: result.provider,
        service: result.service,
        resource: result.resource_name,
        environment: result.environment,
        matched,
        shown,
        records,
    })
}

fn db_status(status: PluginDbReadStatus) -> DbStatus {
    match status {
        PluginDbReadStatus::Ok => DbStatus::Ok,
        PluginDbReadStatus::Partial => DbStatus::Partial,
        PluginDbReadStatus::AuthRequired => DbStatus::AuthRequired,
        PluginDbReadStatus::Unavailable => DbStatus::Unavailable,
        PluginDbReadStatus::InvalidRequest => DbStatus::InvalidRequest,
        PluginDbReadStatus::Error => DbStatus::Error,
    }
}

fn provider_db_error(error: PluginDbProviderError) -> DbError {
    DbError::new(
        match error.kind {
            PluginDbProviderErrorKind::AuthRequired => DbErrorKind::AuthRequired,
            PluginDbProviderErrorKind::Internal => DbErrorKind::Internal,
            PluginDbProviderErrorKind::InvalidRequest => DbErrorKind::InvalidRequest,
            PluginDbProviderErrorKind::PermissionDenied => DbErrorKind::PermissionDenied,
            PluginDbProviderErrorKind::Unavailable => DbErrorKind::Unavailable,
            PluginDbProviderErrorKind::Unsupported => DbErrorKind::Unsupported,
        },
        error.message,
    )
}

fn runtime_db_error(error: wasmtime::Error) -> DbError {
    DbError::new(
        DbErrorKind::Internal,
        format!("db plugin runtime error: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn missing_component_reports_path() {
        let path = test_component_path("missing-component.wasm");
        let runtime = PluginRuntime::new().unwrap();

        let error = match runtime
            .instantiate_openapi_provider_with_capabilities(&path, PluginCapabilities::default())
        {
            Ok(_) => panic!("missing component should fail to instantiate"),
            Err(error) => error,
        };

        assert_eq!(error.kind, PluginRuntimeErrorKind::Component);
        assert_eq!(error.path.as_deref(), Some(path.as_path()));
    }

    #[test]
    fn empty_component_is_not_an_openapi_provider() {
        let path = test_component_path("empty-openapi-provider.wasm");
        fs::write(&path, b"\0asm\r\0\x01\0").unwrap();

        let runtime = PluginRuntime::new().unwrap();
        let error = match runtime
            .instantiate_openapi_provider_with_capabilities(&path, PluginCapabilities::default())
        {
            Ok(_) => panic!("empty component should not instantiate as an openapi provider"),
            Err(error) => error,
        };

        assert_eq!(error.kind, PluginRuntimeErrorKind::Instantiate);
        assert!(error.message.contains("conduit:plugin/metadata"));

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn openapi_provider_component_calls_metadata_operation_and_list() {
        let path = test_component_path("openapi-provider.wasm");
        fs::write(&path, openapi_provider_component()).unwrap();

        let runtime = PluginRuntime::new().unwrap();
        let provider = runtime
            .instantiate_openapi_provider_with_capabilities(&path, PluginCapabilities::default())
            .unwrap();
        let request = OpenApiRequest {
            service: "fixture-service".to_string(),
            environment: Some("test".to_string()),
            method: Some("GET".to_string()),
            path: Some("/fixture".to_string()),
        };

        let metadata = provider.metadata().unwrap();
        let operation = provider.operation(&request).unwrap();
        let operations = provider.list(&request).unwrap();

        assert_eq!(metadata.id, "fixture-openapi");
        assert_eq!(metadata.version, "0.1.0");
        assert_eq!(metadata.providers, ["openapi-provider-v1"]);
        assert_eq!(
            operation,
            OpenApiOperation {
                service: "fixture-service".to_string(),
                environment: None,
                method: "GET".to_string(),
                path: "/fixture".to_string(),
                parameters: Vec::new(),
                operation_id: Some("fixtureOperation".to_string()),
                summary: Some("Fixture operation".to_string()),
                description: None,
                request_schema_json: None,
                response_schema_json: None,
                source: None,
            }
        );
        assert_eq!(operations.service, "fixture-service");
        assert_eq!(operations.environment.as_deref(), Some("test"));
        assert!(operations.operations.is_empty());

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn logs_provider_component_calls_metadata_search_and_authenticate() {
        let path = test_component_path("logs-provider.wasm");
        fs::write(&path, logs_provider_component()).unwrap();

        let runtime = PluginRuntime::new().unwrap();
        let provider = runtime
            .instantiate_logs_provider_with_capabilities(&path, PluginCapabilities::default())
            .unwrap();
        let query = LogQuery {
            service: "fixture-service".to_string(),
            environment: Some("staging".to_string()),
            time_range: LogTimeRange {
                from: "2026-05-22T10:00:00Z".to_string(),
                to: Some("2026-05-22T10:15:00Z".to_string()),
                source: "since 15m".to_string(),
            },
            limit: 20,
            levels: vec!["ERROR".to_string()],
            cid: None,
            trace_id: None,
            message: None,
            logger: None,
            exclude_messages: Vec::new(),
            exclude_loggers: Vec::new(),
            include_trace: false,
            cursor: None,
        };

        let metadata = provider.metadata().unwrap();
        let result = provider.search(&query).unwrap();
        let auth = provider
            .authenticate(&LogAuthRequest {
                environment: Some("staging".to_string()),
                secret: Some("session=fixture".to_string()),
                check: false,
            })
            .unwrap();

        assert_eq!(metadata.id, "fixture-logs");
        assert_eq!(metadata.version, "0.1.0");
        assert_eq!(metadata.providers, ["logs-provider-v1"]);
        assert_eq!(result.status, LogStatus::Ok);
        assert_eq!(result.provider, "fixture-logs");
        assert_eq!(result.service, "fixture-service");
        assert_eq!(result.environment.as_deref(), Some("staging"));
        assert_eq!(result.time_range, query.time_range);
        assert_eq!(result.matches, 1);
        assert_eq!(result.shown, 1);
        assert_eq!(result.logs.len(), 1);
        assert_eq!(result.logs[0].timestamp, "2026-05-22T10:00:00Z");
        assert_eq!(result.logs[0].message, "ACCOUNT_NOT_ACTIVATED");
        assert_eq!(
            result.checked_until.as_deref(),
            Some("2026-05-22T10:15:00Z")
        );
        assert_eq!(auth.status, LogAuthStatus::Ok);
        assert_eq!(auth.provider, "fixture-logs");
        assert_eq!(auth.environment.as_deref(), Some("staging"));
        assert_eq!(auth.destination.as_deref(), Some("fixture://logs/auth"));
        assert_eq!(
            auth.diagnostics,
            [LogDiagnostic {
                kind: "auth_stored".to_string(),
                hint: Some("stored by fixture".to_string()),
            }]
        );

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn openapi_provider_component_can_read_allowed_files() {
        let workspace = test_workspace("openapi-provider-file-read");
        let allowed_dir = workspace.join("allowed");
        let summary_path = allowed_dir.join("summary.txt");
        fs::create_dir_all(&allowed_dir).unwrap();
        fs::write(&summary_path, "Loaded from file-read host import").unwrap();
        let component_path = test_component_path("openapi-provider-file-read.wasm");
        fs::write(&component_path, openapi_provider_file_read_component()).unwrap();

        let runtime = PluginRuntime::new().unwrap();
        let provider = runtime
            .instantiate_openapi_provider_with_capabilities(
                &component_path,
                PluginCapabilities {
                    file_read_paths: vec![allowed_dir],
                    ..PluginCapabilities::default()
                },
            )
            .unwrap();
        let request = OpenApiRequest {
            service: "fixture-service".to_string(),
            environment: None,
            method: Some("GET".to_string()),
            path: Some(summary_path.to_string_lossy().to_string()),
        };

        let operation = provider.operation(&request).unwrap();

        assert_eq!(
            operation.summary.as_deref(),
            Some("Loaded from file-read host import")
        );

        fs::remove_file(component_path).unwrap();
        fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn openapi_provider_component_can_read_allowed_http_urls() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 1024];
            let _ = std::io::Read::read(&mut stream, &mut request).unwrap();
            let response = "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 22\r\n\r\nLoaded from http host\n";
            std::io::Write::write_all(&mut stream, response.as_bytes()).unwrap();
        });
        let component_path = test_component_path("openapi-provider-http.wasm");
        fs::write(&component_path, openapi_provider_http_component()).unwrap();

        let runtime = PluginRuntime::new().unwrap();
        let provider = runtime
            .instantiate_openapi_provider_with_capabilities(
                &component_path,
                PluginCapabilities {
                    http_hosts: vec!["127.0.0.1".to_string()],
                    ..PluginCapabilities::default()
                },
            )
            .unwrap();
        let request = OpenApiRequest {
            service: "fixture-service".to_string(),
            environment: None,
            method: Some("GET".to_string()),
            path: Some(format!("http://{address}/openapi.json")),
        };

        let operation = provider.operation(&request).unwrap();

        assert_eq!(
            operation.summary.as_deref(),
            Some("Loaded from http host\n")
        );

        server.join().unwrap();
        fs::remove_file(component_path).unwrap();
    }

    fn openapi_provider_component() -> Vec<u8> {
        openapi_provider_component_with_core_module(openapi_provider_core_module())
    }

    fn openapi_provider_file_read_component() -> Vec<u8> {
        openapi_provider_component_with_core_module(openapi_provider_file_read_core_module())
    }

    fn openapi_provider_http_component() -> Vec<u8> {
        openapi_provider_component_with_core_module(openapi_provider_http_core_module())
    }

    fn openapi_provider_component_with_core_module(module: Vec<u8>) -> Vec<u8> {
        provider_component_with_core_module("openapi-provider", module)
    }

    fn logs_provider_component() -> Vec<u8> {
        provider_component_with_core_module("logs-provider", logs_provider_core_module())
    }

    fn provider_component_with_core_module(world_name: &str, mut module: Vec<u8>) -> Vec<u8> {
        // The fixture modules are core Wasm modules that export canonical ABI
        // functions. Embedding WIT metadata and re-encoding them as components
        // keeps the runtime tests focused on Conduit's real component boundary
        // without checking in generated component binaries.
        let mut resolve = wit_parser::Resolve::default();
        let wit_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../wit/conduit-plugin");
        let (package, _) = resolve.push_dir(wit_dir).unwrap();
        let world = resolve.select_world(&[package], Some(world_name)).unwrap();
        wit_component::embed_component_metadata(
            &mut module,
            &resolve,
            world,
            wit_component::StringEncoding::UTF8,
        )
        .unwrap();

        wit_component::ComponentEncoder::default()
            .module(&module)
            .unwrap()
            .encode()
            .unwrap()
    }

    fn openapi_provider_core_module() -> Vec<u8> {
        wat::parse_str(include_str!(
            "../tests/fixtures/openapi-provider/module.wat"
        ))
        .unwrap()
    }

    fn openapi_provider_file_read_core_module() -> Vec<u8> {
        wat::parse_str(include_str!(
            "../tests/fixtures/openapi-provider/file-read-module.wat"
        ))
        .unwrap()
    }

    fn openapi_provider_http_core_module() -> Vec<u8> {
        wat::parse_str(include_str!(
            "../tests/fixtures/openapi-provider/http-module.wat"
        ))
        .unwrap()
    }

    fn logs_provider_core_module() -> Vec<u8> {
        wat::parse_str(include_str!("../tests/fixtures/logs-provider/module.wat")).unwrap()
    }

    fn test_component_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("conduit-{nanos}-{name}"))
    }

    fn test_workspace(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = PathBuf::from("target").join(format!(
            "conduit-plugin-runtime-tests-{}-{nanos}-{name}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
}
