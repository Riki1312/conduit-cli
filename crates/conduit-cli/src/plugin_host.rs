use crate::config::PluginCapabilities;
use crate::plugin_bindings::logs::conduit::plugin::file_read_v1 as logs_file_read;
use crate::plugin_bindings::logs::conduit::plugin::http_client_v2 as logs_http;
use crate::plugin_bindings::logs::conduit::plugin::secret_store_v1 as logs_secret;
use crate::plugin_bindings::openapi::conduit::plugin::file_read_v1::{
    FileReadError as OpenApiFileReadError, FileReadErrorKind as OpenApiFileReadErrorKind,
    Host as OpenApiFileReadHost,
};
use crate::plugin_bindings::openapi::conduit::plugin::http_client_v1::{
    Host as OpenApiHttpClientHost, HttpError as OpenApiHttpError,
    HttpErrorKind as OpenApiHttpErrorKind, HttpResponse as OpenApiHttpResponse,
};
use std::fs;
use std::path::Component as PathComponent;
use std::path::{Path, PathBuf};
use std::time::Duration;
use url::Url;

#[derive(Debug, Default)]
pub(crate) struct PluginHostState {
    capabilities: PluginCapabilities,
}

impl PluginHostState {
    pub(crate) fn new(capabilities: PluginCapabilities) -> Self {
        Self { capabilities }
    }
}

impl OpenApiFileReadHost for PluginHostState {
    fn read_text(&mut self, path: String) -> Result<String, OpenApiFileReadError> {
        read_text_with_capabilities(&self.capabilities, &path).map_err(openapi_file_error)
    }
}

impl OpenApiHttpClientHost for PluginHostState {
    fn get(&mut self, url: String) -> Result<OpenApiHttpResponse, OpenApiHttpError> {
        http_get_with_capabilities(&self.capabilities.http_hosts, &url)
            .map_err(openapi_http_error)
            .map(|response| OpenApiHttpResponse {
                status: response.status,
                body: response.body,
            })
    }
}

impl logs_file_read::Host for PluginHostState {
    fn read_text(&mut self, path: String) -> Result<String, logs_file_read::FileReadError> {
        read_text_with_capabilities(&self.capabilities, &path).map_err(logs_file_error)
    }
}

impl logs_http::Host for PluginHostState {
    fn request(
        &mut self,
        request: logs_http::HttpRequest,
    ) -> Result<logs_http::HttpResponse, logs_http::HttpError> {
        http_request_with_capabilities(&self.capabilities.http_hosts, request)
    }
}

impl logs_secret::Host for PluginHostState {
    fn read(&mut self, name: String) -> Result<Option<String>, logs_secret::SecretError> {
        secret_read_with_capabilities(&self.capabilities.secret_names, &name)
            .map_err(logs_secret_error)
    }

    fn write(&mut self, name: String, value: String) -> Result<bool, logs_secret::SecretError> {
        secret_write_with_capabilities(&self.capabilities.secret_names, &name, &value)
            .map_err(logs_secret_error)
    }

    fn delete(&mut self, name: String) -> Result<bool, logs_secret::SecretError> {
        secret_delete_with_capabilities(&self.capabilities.secret_names, &name)
            .map_err(logs_secret_error)
    }
}

fn http_get_with_capabilities(
    allowed_hosts: &[String],
    url: &str,
) -> Result<HostHttpResponse, HostHttpError> {
    http_request(
        allowed_hosts,
        "GET",
        url,
        &[],
        None,
        Duration::from_secs(30),
    )
}

fn http_request_with_capabilities(
    allowed_hosts: &[String],
    request: logs_http::HttpRequest,
) -> Result<logs_http::HttpResponse, logs_http::HttpError> {
    let method = match request.method {
        logs_http::HttpMethod::Get => "GET",
        logs_http::HttpMethod::Post => "POST",
    };
    let headers = request
        .headers
        .iter()
        .map(|header| (header.name.as_str(), header.value.as_str()))
        .collect::<Vec<_>>();
    let timeout = Duration::from_millis(u64::from(request.timeout_ms.unwrap_or(30_000)));
    let response = http_request(
        allowed_hosts,
        method,
        &request.url,
        &headers,
        request.body.as_deref(),
        timeout,
    )
    .map_err(logs_http_error)?;

    Ok(logs_http::HttpResponse {
        status: response.status,
        headers: response
            .headers
            .into_iter()
            .map(|(name, value)| logs_http::HttpHeader { name, value })
            .collect(),
        body: response.body,
    })
}

fn http_request(
    allowed_hosts: &[String],
    method: &str,
    url: &str,
    headers: &[(&str, &str)],
    body: Option<&str>,
    timeout: Duration,
) -> Result<HostHttpResponse, HostHttpError> {
    // Parse before checking capabilities so host comparisons use the URL
    // parser's normalized host view instead of ad hoc string matching.
    let parsed = Url::parse(url).map_err(|error| {
        http_error(
            HostHttpErrorKind::InvalidUrl,
            format!("http url `{url}` is invalid: {error}"),
        )
    })?;

    match parsed.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(http_error(
                HostHttpErrorKind::InvalidUrl,
                format!("http url `{url}` uses unsupported scheme `{scheme}`"),
            ));
        }
    }

    let Some(host) = parsed.host_str() else {
        return Err(http_error(
            HostHttpErrorKind::InvalidUrl,
            format!("http url `{url}` must include a host"),
        ));
    };

    if !allowed_hosts
        .iter()
        .any(|allowed_host| allowed_host == host)
    {
        return Err(http_error(
            HostHttpErrorKind::PermissionDenied,
            format!("http access denied for host `{host}`"),
        ));
    }

    let mut request = ureq::request(method, url).timeout(timeout);
    for (name, value) in headers {
        request = request.set(name, value);
    }
    let response = http_response_from_result(
        match body {
            Some(body) => request.send_string(body),
            None => request.call(),
        },
        method,
        url,
    )?;

    let status = response.status();
    let headers = response
        .headers_names()
        .into_iter()
        .filter_map(|name| {
            response
                .header(&name)
                .map(|value| (name, value.to_string()))
        })
        .collect();
    let body = response.into_string().map_err(|error| {
        http_error(
            HostHttpErrorKind::Internal,
            format!("failed to read http response body from `{url}`: {error}"),
        )
    })?;

    Ok(HostHttpResponse {
        status,
        headers,
        body,
    })
}

fn http_response_from_result(
    result: Result<ureq::Response, ureq::Error>,
    method: &str,
    url: &str,
) -> Result<ureq::Response, HostHttpError> {
    match result {
        Ok(response) | Err(ureq::Error::Status(_, response)) => Ok(response),
        Err(error) => Err(http_error(
            HostHttpErrorKind::Unavailable,
            format!("http {method} `{url}` failed: {error}"),
        )),
    }
}

fn read_text_with_capabilities(
    capabilities: &PluginCapabilities,
    path: &str,
) -> Result<String, HostFileReadError> {
    let requested_path = validate_plugin_path(path)?;
    let requested_lexical =
        absolute_lexical_path(&file_read_base(capabilities).join(requested_path))
            .map_err(internal_file_error)?;
    let allowed_lexical_paths = capabilities
        .file_read_paths
        .iter()
        .map(|allowed_path| absolute_lexical_path(allowed_path).map_err(internal_file_error))
        .collect::<Result<Vec<_>, _>>()?;

    if !allowed_lexical_paths
        .iter()
        .any(|allowed_path| requested_lexical.starts_with(allowed_path))
    {
        return Err(file_error(
            HostFileReadErrorKind::PermissionDenied,
            format!("file-read access denied for `{path}`"),
        ));
    }

    let requested_canonical = requested_lexical
        .canonicalize()
        .map_err(|error| read_io_error(path, error))?;

    let allowed = capabilities.file_read_paths.iter().any(|allowed_path| {
        allowed_path
            .canonicalize()
            .map(|allowed_canonical| requested_canonical.starts_with(allowed_canonical))
            .unwrap_or(false)
    });

    if !allowed {
        return Err(file_error(
            HostFileReadErrorKind::PermissionDenied,
            format!("file-read access denied for `{path}`"),
        ));
    }

    fs::read_to_string(&requested_canonical).map_err(|error| read_io_error(path, error))
}

fn file_read_base(capabilities: &PluginCapabilities) -> &Path {
    capabilities
        .file_read_base
        .as_deref()
        .unwrap_or_else(|| Path::new("."))
}

fn secret_read_with_capabilities(
    allowed_names: &[String],
    name: &str,
) -> Result<Option<String>, HostSecretError> {
    validate_secret_capability(allowed_names, name)?;
    crate::secrets::read_secret(name).map_err(host_secret_error)
}

fn secret_write_with_capabilities(
    allowed_names: &[String],
    name: &str,
    value: &str,
) -> Result<bool, HostSecretError> {
    validate_secret_capability(allowed_names, name)?;
    crate::secrets::write_secret(name, value).map_err(host_secret_error)?;
    Ok(true)
}

fn secret_delete_with_capabilities(
    allowed_names: &[String],
    name: &str,
) -> Result<bool, HostSecretError> {
    validate_secret_capability(allowed_names, name)?;
    crate::secrets::delete_secret(name).map_err(host_secret_error)
}

fn validate_secret_capability(allowed_names: &[String], name: &str) -> Result<(), HostSecretError> {
    crate::config::validate_secret_name("secret", name)
        .map_err(|error| secret_error(HostSecretErrorKind::InvalidName, error.message))?;
    if !allowed_names
        .iter()
        .any(|allowed_name| allowed_name == name)
    {
        return Err(secret_error(
            HostSecretErrorKind::PermissionDenied,
            format!("secret access denied for `{name}`"),
        ));
    }

    Ok(())
}

fn validate_plugin_path(path: &str) -> Result<PathBuf, HostFileReadError> {
    let path = PathBuf::from(path);
    if path.as_os_str().is_empty() {
        return Err(file_error(
            HostFileReadErrorKind::InvalidPath,
            "file-read path cannot be empty",
        ));
    }
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, PathComponent::ParentDir))
    {
        return Err(file_error(
            HostFileReadErrorKind::InvalidPath,
            format!(
                "file-read path `{}` must be relative and stay within the project",
                path.display()
            ),
        ));
    }

    Ok(path)
}

fn absolute_lexical_path(path: &Path) -> Result<PathBuf, std::io::Error> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };

    Ok(normalize_lexical_path(&path))
}

fn normalize_lexical_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            PathComponent::CurDir => {}
            PathComponent::Normal(value) => normalized.push(value),
            PathComponent::Prefix(_) | PathComponent::RootDir | PathComponent::ParentDir => {
                normalized.push(component.as_os_str());
            }
        }
    }

    normalized
}

#[derive(Debug, PartialEq, Eq)]
struct HostFileReadError {
    kind: HostFileReadErrorKind,
    message: String,
}

#[derive(Debug, PartialEq, Eq)]
enum HostFileReadErrorKind {
    NotFound,
    InvalidPath,
    PermissionDenied,
    Internal,
}

fn read_io_error(path: &str, error: std::io::Error) -> HostFileReadError {
    let kind = if error.kind() == std::io::ErrorKind::NotFound {
        HostFileReadErrorKind::NotFound
    } else {
        HostFileReadErrorKind::Internal
    };

    file_error(kind, format!("failed to read `{path}`: {error}"))
}

fn internal_file_error(error: std::io::Error) -> HostFileReadError {
    file_error(
        HostFileReadErrorKind::Internal,
        format!("failed to resolve current directory: {error}"),
    )
}

fn file_error(kind: HostFileReadErrorKind, message: impl Into<String>) -> HostFileReadError {
    HostFileReadError {
        kind,
        message: message.into(),
    }
}

#[derive(Debug, PartialEq, Eq)]
struct HostSecretError {
    kind: HostSecretErrorKind,
    message: String,
}

#[derive(Debug, PartialEq, Eq)]
enum HostSecretErrorKind {
    InvalidName,
    PermissionDenied,
    Internal,
}

fn host_secret_error(error: crate::secrets::SecretStoreError) -> HostSecretError {
    secret_error(
        match error.kind {
            crate::secrets::SecretStoreErrorKind::InvalidName => HostSecretErrorKind::InvalidName,
            crate::secrets::SecretStoreErrorKind::Internal => HostSecretErrorKind::Internal,
        },
        error.message,
    )
}

fn secret_error(kind: HostSecretErrorKind, message: impl Into<String>) -> HostSecretError {
    HostSecretError {
        kind,
        message: message.into(),
    }
}

fn openapi_file_error(error: HostFileReadError) -> OpenApiFileReadError {
    OpenApiFileReadError {
        kind: match error.kind {
            HostFileReadErrorKind::NotFound => OpenApiFileReadErrorKind::NotFound,
            HostFileReadErrorKind::InvalidPath => OpenApiFileReadErrorKind::InvalidPath,
            HostFileReadErrorKind::PermissionDenied => OpenApiFileReadErrorKind::PermissionDenied,
            HostFileReadErrorKind::Internal => OpenApiFileReadErrorKind::Internal,
        },
        message: error.message,
    }
}

fn logs_file_error(error: HostFileReadError) -> logs_file_read::FileReadError {
    logs_file_read::FileReadError {
        kind: match error.kind {
            HostFileReadErrorKind::NotFound => logs_file_read::FileReadErrorKind::NotFound,
            HostFileReadErrorKind::InvalidPath => logs_file_read::FileReadErrorKind::InvalidPath,
            HostFileReadErrorKind::PermissionDenied => {
                logs_file_read::FileReadErrorKind::PermissionDenied
            }
            HostFileReadErrorKind::Internal => logs_file_read::FileReadErrorKind::Internal,
        },
        message: error.message,
    }
}

fn logs_secret_error(error: HostSecretError) -> logs_secret::SecretError {
    logs_secret::SecretError {
        kind: match error.kind {
            HostSecretErrorKind::InvalidName => logs_secret::SecretErrorKind::InvalidName,
            HostSecretErrorKind::PermissionDenied => logs_secret::SecretErrorKind::PermissionDenied,
            HostSecretErrorKind::Internal => logs_secret::SecretErrorKind::Internal,
        },
        message: error.message,
    }
}

#[derive(Debug, PartialEq, Eq)]
struct HostHttpResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

#[derive(Debug, PartialEq, Eq)]
struct HostHttpError {
    kind: HostHttpErrorKind,
    message: String,
}

#[derive(Debug, PartialEq, Eq)]
enum HostHttpErrorKind {
    InvalidUrl,
    PermissionDenied,
    Unavailable,
    Internal,
}

fn http_error(kind: HostHttpErrorKind, message: impl Into<String>) -> HostHttpError {
    HostHttpError {
        kind,
        message: message.into(),
    }
}

fn openapi_http_error(error: HostHttpError) -> OpenApiHttpError {
    OpenApiHttpError {
        kind: match error.kind {
            HostHttpErrorKind::InvalidUrl => OpenApiHttpErrorKind::InvalidUrl,
            HostHttpErrorKind::PermissionDenied => OpenApiHttpErrorKind::PermissionDenied,
            HostHttpErrorKind::Unavailable => OpenApiHttpErrorKind::Unavailable,
            HostHttpErrorKind::Internal => OpenApiHttpErrorKind::Internal,
        },
        message: error.message,
    }
}

fn logs_http_error(error: HostHttpError) -> logs_http::HttpError {
    logs_http::HttpError {
        kind: match error.kind {
            HostHttpErrorKind::InvalidUrl => logs_http::HttpErrorKind::InvalidUrl,
            HostHttpErrorKind::PermissionDenied => logs_http::HttpErrorKind::PermissionDenied,
            HostHttpErrorKind::Unavailable => logs_http::HttpErrorKind::Unavailable,
            HostHttpErrorKind::Internal => logs_http::HttpErrorKind::Internal,
        },
        message: error.message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn http_host_reads_allowed_urls() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 1024];
            let _ = std::io::Read::read(&mut stream, &mut request).unwrap();
            let response =
                "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 12\r\n\r\n{\"ok\":true}\n";
            std::io::Write::write_all(&mut stream, response.as_bytes()).unwrap();
        });

        let response = http_get_with_capabilities(
            &["127.0.0.1".to_string()],
            &format!("http://{address}/openapi.json"),
        )
        .unwrap();

        assert_eq!(response.status, 200);
        assert_eq!(response.body, "{\"ok\":true}\n");
        server.join().unwrap();
    }

    #[test]
    fn http_v2_host_sends_allowed_post_requests() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_http_request(&mut stream);
            assert!(request.starts_with("POST /logs HTTP/1.1\r\n"));
            assert!(request.contains("x-test: yes\r\n"));
            assert!(request.ends_with("{\"query\":\"error\"}"));
            let response = "HTTP/1.1 200 OK\r\nConnection: close\r\nX-Result: ok\r\nContent-Length: 11\r\n\r\n{\"hits\":1}\n";
            std::io::Write::write_all(&mut stream, response.as_bytes()).unwrap();
        });

        let response = http_request_with_capabilities(
            &["127.0.0.1".to_string()],
            logs_http::HttpRequest {
                method: logs_http::HttpMethod::Post,
                url: format!("http://{address}/logs"),
                headers: vec![logs_http::HttpHeader {
                    name: "x-test".to_string(),
                    value: "yes".to_string(),
                }],
                body: Some(r#"{"query":"error"}"#.to_string()),
                timeout_ms: Some(5_000),
            },
        )
        .unwrap();

        assert_eq!(response.status, 200);
        assert_eq!(response.body, "{\"hits\":1}\n");
        assert!(response.headers.iter().any(|header| {
            header.name.eq_ignore_ascii_case("x-result") && header.value == "ok"
        }));
        server.join().unwrap();
    }

    #[test]
    fn http_host_returns_non_success_status_responses() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = read_http_request(&mut stream);
            let response = "HTTP/1.1 401 Unauthorized\r\nConnection: close\r\nContent-Length: 12\r\n\r\nauth needed\n";
            std::io::Write::write_all(&mut stream, response.as_bytes()).unwrap();
        });

        let response = http_get_with_capabilities(
            &["127.0.0.1".to_string()],
            &format!("http://{address}/logs"),
        )
        .unwrap();

        assert_eq!(response.status, 401);
        assert_eq!(response.body, "auth needed\n");
        server.join().unwrap();
    }

    fn read_http_request(stream: &mut std::net::TcpStream) -> String {
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        let mut chunk = [0; 1024];

        loop {
            let read = std::io::Read::read(stream, &mut chunk).unwrap();
            assert!(
                read > 0,
                "client closed connection before request completed"
            );
            request.extend_from_slice(&chunk[..read]);
            if http_request_is_complete(&request) {
                return String::from_utf8(request).unwrap();
            }
        }
    }

    fn http_request_is_complete(request: &[u8]) -> bool {
        let Some(header_end) = request
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|position| position + 4)
        else {
            return false;
        };
        let headers = String::from_utf8_lossy(&request[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().unwrap())
            })
            .unwrap_or(0);

        request.len() >= header_end + content_length
    }

    #[test]
    fn file_read_host_reads_allowed_text() {
        let root = test_workspace("file-read-allowed");
        let allowed_dir = root.join("allowed");
        fs::create_dir_all(&allowed_dir).unwrap();
        fs::write(allowed_dir.join("openapi.json"), "{\"openapi\":\"3.1.0\"}").unwrap();

        let capabilities = PluginCapabilities {
            file_read_paths: vec![allowed_dir],
            ..PluginCapabilities::default()
        };
        let text = read_text_with_capabilities(
            &capabilities,
            root.join("allowed/openapi.json").to_str().unwrap(),
        )
        .unwrap();

        assert_eq!(text, "{\"openapi\":\"3.1.0\"}");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn file_read_host_resolves_relative_paths_from_config_root() {
        let root = test_workspace("file-read-config-root");
        let allowed_dir = root.join(".conduit/company-openapi");
        fs::create_dir_all(&allowed_dir).unwrap();
        fs::write(allowed_dir.join("services.json"), "{\"services\":{}}").unwrap();

        let capabilities = PluginCapabilities {
            file_read_base: Some(root.clone()),
            file_read_paths: vec![allowed_dir],
            ..PluginCapabilities::default()
        };
        let text =
            read_text_with_capabilities(&capabilities, ".conduit/company-openapi/services.json")
                .unwrap();

        assert_eq!(text, "{\"services\":{}}");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn file_read_host_denies_unconfigured_paths() {
        let root = test_workspace("file-read-denied");
        let allowed_dir = root.join("allowed");
        let denied_dir = root.join("denied");
        fs::create_dir_all(&allowed_dir).unwrap();
        fs::create_dir_all(&denied_dir).unwrap();
        fs::write(denied_dir.join("secret.txt"), "secret").unwrap();

        let capabilities = PluginCapabilities {
            file_read_paths: vec![allowed_dir],
            ..PluginCapabilities::default()
        };
        let error = read_text_with_capabilities(
            &capabilities,
            denied_dir.join("secret.txt").to_str().unwrap(),
        )
        .unwrap_err();

        assert!(matches!(
            error.kind,
            HostFileReadErrorKind::PermissionDenied
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn file_read_host_rejects_parent_traversal() {
        let error = read_text_with_capabilities(&PluginCapabilities::default(), "../secret.txt")
            .unwrap_err();

        assert!(matches!(error.kind, HostFileReadErrorKind::InvalidPath));
    }

    #[test]
    fn file_read_host_reports_missing_allowed_files() {
        let root = test_workspace("file-read-missing");
        let allowed_dir = root.join("allowed");
        fs::create_dir_all(&allowed_dir).unwrap();

        let capabilities = PluginCapabilities {
            file_read_paths: vec![allowed_dir],
            ..PluginCapabilities::default()
        };
        let error = read_text_with_capabilities(
            &capabilities,
            root.join("allowed/missing.txt").to_str().unwrap(),
        )
        .unwrap_err();

        assert!(matches!(error.kind, HostFileReadErrorKind::NotFound));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn secret_host_allows_declared_secret_names() {
        validate_secret_capability(
            &["company-logs/staging/cookie".to_string()],
            "company-logs/staging/cookie",
        )
        .unwrap();
    }

    #[test]
    fn secret_host_denies_unconfigured_secret_names() {
        let error = validate_secret_capability(
            &["company-logs/staging/cookie".to_string()],
            "company-logs/production/cookie",
        )
        .unwrap_err();

        assert!(matches!(error.kind, HostSecretErrorKind::PermissionDenied));
    }

    #[test]
    fn secret_host_rejects_invalid_secret_names() {
        let error = validate_secret_capability(&[], "../cookie").unwrap_err();

        assert!(matches!(error.kind, HostSecretErrorKind::InvalidName));
    }

    #[cfg(unix)]
    #[test]
    fn file_read_host_denies_symlink_escape() {
        use std::os::unix::fs::symlink;

        let root = test_workspace("file-read-symlink");
        let allowed_dir = root.join("allowed");
        let outside_dir = root.join("outside");
        fs::create_dir_all(&allowed_dir).unwrap();
        fs::create_dir_all(&outside_dir).unwrap();
        let outside_file = outside_dir.join("secret.txt");
        fs::write(&outside_file, "secret").unwrap();
        symlink(
            outside_file.canonicalize().unwrap(),
            allowed_dir.join("secret-link.txt"),
        )
        .unwrap();

        let capabilities = PluginCapabilities {
            file_read_paths: vec![allowed_dir],
            ..PluginCapabilities::default()
        };
        let error = read_text_with_capabilities(
            &capabilities,
            root.join("allowed/secret-link.txt").to_str().unwrap(),
        )
        .unwrap_err();

        assert!(matches!(
            error.kind,
            HostFileReadErrorKind::PermissionDenied
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn http_host_denies_unconfigured_hosts() {
        let error =
            http_get_with_capabilities(&[], "http://docs.example.com/openapi.json").unwrap_err();

        assert!(matches!(error.kind, HostHttpErrorKind::PermissionDenied));
    }

    #[test]
    fn http_host_rejects_invalid_urls() {
        let error =
            http_get_with_capabilities(&["docs.example.com".to_string()], "not a url").unwrap_err();

        assert!(matches!(error.kind, HostHttpErrorKind::InvalidUrl));
    }

    fn test_workspace(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = PathBuf::from("target").join(format!(
            "conduit-plugin-host-tests-{}-{nanos}-{name}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
}
