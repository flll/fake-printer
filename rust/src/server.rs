//! HTTP layer: axum router, body handling, auth, override extraction and
//! operation dispatch. Port of the Python `IppHandler` HTTP plumbing.
//!
//! Deviation from Python: `Expect: 100-continue` is answered automatically by
//! hyper when the body is read (the Python default suppressed it as a
//! reverse-proxy workaround; irrelevant for direct LAN use).

use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{ConnectInfo, State};
use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use axum::Router;

use crate::config::Config;
use crate::events::EventSender;
use crate::ipp::{ops, wire};
use crate::jobs::{JobRegistry, Overrides};

pub struct AppState {
    pub config: Config,
    pub registry: JobRegistry,
    pub events: EventSender,
}

pub struct SplitPath {
    /// None when the path does not match the configured IPP base path.
    pub path_only: Option<String>,
    pub overrides: Overrides,
    /// Path + query with secrets redacted, for logging.
    pub safe_path_for_logs: String,
}

/// Port of `_split_ipp_path_and_overrides`. Accepts:
/// - /ipp/print
/// - /ipp/print?paper_id=123&auth_value=TOKEN
/// - /ipp/print/123
/// - /ipp/print/123/TOKEN
/// - /ipp/print/job/<id>  (from Create-Job job-uri)
pub fn split_ipp_path_and_overrides(raw_path: &str, ipp_base_path: &str) -> SplitPath {
    let (path_only, query) = match raw_path.split_once('?') {
        Some((p, q)) => (p, q),
        None => (raw_path, ""),
    };

    let base_prefix = format!("{}/", ipp_base_path.trim_end_matches('/'));
    if path_only != ipp_base_path && !path_only.starts_with(&base_prefix) {
        return SplitPath {
            path_only: None,
            overrides: Overrides::default(),
            safe_path_for_logs: raw_path.to_string(),
        };
    }

    let pairs: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes())
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let first = |names: &[&str]| -> String {
        for name in names {
            if let Some((_, v)) = pairs.iter().find(|(k, _)| k == name) {
                let v = v.trim();
                if !v.is_empty() {
                    return v.to_string();
                }
            }
        }
        String::new()
    };

    let mut overrides = Overrides {
        paper_id: first(&["paper_id", "paperId", "paper", "PAPER_ID"]),
        auth_value: first(&["auth_value", "token", "auth", "AUTH_VALUE"]),
    };

    // Optional path segments after the base path.
    let remainder = path_only[ipp_base_path.len().min(path_only.len())..].trim_start_matches('/');
    if !remainder.is_empty() && !remainder.starts_with("job/") {
        let segs: Vec<String> = remainder
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| {
                percent_encoding::percent_decode_str(s)
                    .decode_utf8_lossy()
                    .into_owned()
            })
            .collect();
        if let Some(first_seg) = segs.first()
            && overrides.paper_id.is_empty()
        {
            overrides.paper_id = first_seg.trim().to_string();
        }
        if let Some(second_seg) = segs.get(1)
            && overrides.auth_value.is_empty()
        {
            overrides.auth_value = second_seg.trim().to_string();
        }
    }

    // Redact secrets in logs (never log auth_value/token).
    let safe_query: Vec<String> = pairs
        .iter()
        .map(|(k, v)| {
            if matches!(k.to_lowercase().as_str(), "auth_value" | "token" | "auth") {
                format!("{k}=<redacted>")
            } else {
                format!("{k}={v}")
            }
        })
        .collect();
    let safe_path_for_logs = if safe_query.is_empty() {
        path_only.to_string()
    } else {
        format!("{}?{}", path_only, safe_query.join("&"))
    };

    SplitPath {
        path_only: Some(path_only.to_string()),
        overrides,
        safe_path_for_logs,
    }
}

async fn health() -> Response {
    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        "ok\n",
    )
        .into_response()
}

async fn not_found() -> Response {
    StatusCode::NOT_FOUND.into_response()
}

async fn handle_post(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    request: Request<Body>,
) -> Response {
    let config = &state.config;
    let raw_path = request
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str().to_string())
        .unwrap_or_else(|| request.uri().path().to_string());

    let split = split_ipp_path_and_overrides(&raw_path, &config.ipp_path);

    if config.log_headers {
        let redacted: Vec<String> = request
            .headers()
            .iter()
            .map(|(k, v)| {
                let lk = k.as_str().to_lowercase();
                if matches!(lk.as_str(), "authorization" | "cookie" | "x-api-key" | "x-ipp-token") {
                    format!("{k}: <redacted>")
                } else {
                    format!("{k}: {}", v.to_str().unwrap_or("<binary>"))
                }
            })
            .collect();
        tracing::debug!("HTTP request: path={} headers={:?}", split.safe_path_for_logs, redacted);
    } else {
        tracing::debug!("HTTP request: path={}", split.safe_path_for_logs);
    }

    let Some(_path_only) = split.path_only.as_ref() else {
        tracing::warn!(
            "Unexpected path {} (expected {})",
            split.safe_path_for_logs,
            config.ipp_path
        );
        return StatusCode::NOT_FOUND.into_response();
    };

    // Cache per-client overrides when present. macOS may later omit them.
    let user_agent = request
        .headers()
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .trim()
        .to_string();
    let client_key = format!("{}|{}", peer.ip(), user_agent);
    state.registry.register_client_overrides(&client_key, &split.overrides);

    // IMPORTANT: do not fall back to .env values here. The per-request values
    // (e.g. from a waitlist-generated printer URL) are the source of truth.
    let mut effective = split.overrides.clone();
    if effective.paper_id.is_empty() || effective.auth_value.is_empty() {
        let cached = state.registry.get_client_overrides(&client_key);
        if effective.paper_id.is_empty() {
            effective.paper_id = cached.paper_id;
        }
        if effective.auth_value.is_empty() {
            effective.auth_value = cached.auth_value;
        }
    }

    if !config.shared_token.is_empty() {
        let token = request
            .headers()
            .get("X-IPP-Token")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if token != config.shared_token {
            tracing::warn!("Unauthorized: missing/invalid X-IPP-Token");
            return StatusCode::UNAUTHORIZED.into_response();
        }
    }

    // Body limits: invalid Content-Length -> 400, oversize -> 413,
    // chunked overflow -> 400 (mirrors the Python branches).
    let content_length_header = request
        .headers()
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    if let Some(ref cl) = content_length_header {
        let Ok(parsed) = cl.trim().parse::<u64>() else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        if parsed > config.max_bytes as u64 {
            tracing::warn!("Invalid Content-Length={} (max={})", parsed, config.max_bytes);
            return StatusCode::PAYLOAD_TOO_LARGE.into_response();
        }
    }

    let host = request
        .headers()
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let body = request.into_body();
    let raw = match axum::body::to_bytes(body, config.max_bytes).await {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::warn!("Failed to read request body: {e}");
            return StatusCode::BAD_REQUEST.into_response();
        }
    };
    tracing::debug!("Read {} bytes from request body", raw.len());

    let req = match wire::parse_request(&raw) {
        Ok(req) => req,
        Err(e) => {
            tracing::warn!("Failed to parse IPP request: {e}");
            return (StatusCode::BAD_REQUEST, e).into_response();
        }
    };

    tracing::info!(
        "IPP request: op={} request_id={} job_name={} document_format={} document_bytes={}",
        wire::op_name(req.operation_id),
        req.request_id,
        req.meta_str("job-name"),
        req.meta_str("document-format"),
        req.document.len(),
    );

    match req.operation_id {
        wire::OP_GET_PRINTER_ATTRIBUTES => ops::handle_get_printer_attributes(&state, &req, &host),
        wire::OP_VALIDATE_JOB => ops::handle_validate_job(&req),
        wire::OP_GET_JOB_ATTRIBUTES => ops::handle_get_job_attributes(&state, &req, &host),
        wire::OP_GET_JOBS => ops::handle_get_jobs(&state, &req, &host),
        wire::OP_CREATE_JOB => {
            ops::handle_create_job(&state, &req, &raw, &host, &split.overrides)
        }
        wire::OP_SEND_DOCUMENT => {
            ops::handle_send_document(
                &state,
                &req,
                &raw,
                &host,
                &effective,
                &split.safe_path_for_logs,
            )
            .await
        }
        wire::OP_PRINT_JOB => {
            ops::handle_print_job(&state, &req, &raw, &host, &effective, &split.safe_path_for_logs)
                .await
        }
        _ => ops::handle_unsupported(&req),
    }
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/healthz", get(health))
        .route("/health", get(health))
        .fallback(any(fallback))
        .with_state(state)
}

async fn fallback(
    state: State<Arc<AppState>>,
    peer: ConnectInfo<SocketAddr>,
    request: Request<Body>,
) -> Response {
    match *request.method() {
        axum::http::Method::POST => handle_post(state, peer, request).await,
        axum::http::Method::GET => not_found().await,
        _ => StatusCode::METHOD_NOT_ALLOWED.into_response(),
    }
}

/// Bind and serve until the process exits.
pub async fn run(state: Arc<AppState>) -> std::io::Result<()> {
    let addr = format!("{}:{}", state.config.listen_host, state.config.listen_port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Listening on http://{addr}{}", state.config.ipp_path);
    axum::serve(
        listener,
        router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::split_ipp_path_and_overrides;

    const BASE: &str = "/ipp/print";

    #[test]
    fn plain_path() {
        let s = split_ipp_path_and_overrides("/ipp/print", BASE);
        assert_eq!(s.path_only.as_deref(), Some("/ipp/print"));
        assert!(s.overrides.is_empty());
    }

    #[test]
    fn query_params() {
        let s = split_ipp_path_and_overrides("/ipp/print?paper_id=123&auth_value=SECRET", BASE);
        assert_eq!(s.overrides.paper_id, "123");
        assert_eq!(s.overrides.auth_value, "SECRET");
        assert!(s.safe_path_for_logs.contains("auth_value=<redacted>"));
        assert!(!s.safe_path_for_logs.contains("SECRET"));
    }

    #[test]
    fn path_segments() {
        let s = split_ipp_path_and_overrides("/ipp/print/123/TOKEN", BASE);
        assert_eq!(s.overrides.paper_id, "123");
        assert_eq!(s.overrides.auth_value, "TOKEN");
    }

    #[test]
    fn job_uri_not_treated_as_overrides() {
        let s = split_ipp_path_and_overrides("/ipp/print/job/7", BASE);
        assert_eq!(s.path_only.as_deref(), Some("/ipp/print/job/7"));
        assert!(s.overrides.is_empty());
    }

    #[test]
    fn wrong_path_rejected() {
        let s = split_ipp_path_and_overrides("/other", BASE);
        assert!(s.path_only.is_none());
    }

    #[test]
    fn alternate_query_names() {
        let s = split_ipp_path_and_overrides("/ipp/print?paperId=9&token=T", BASE);
        assert_eq!(s.overrides.paper_id, "9");
        assert_eq!(s.overrides.auth_value, "T");
    }
}
