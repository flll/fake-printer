//! IPP operation handlers. Port of the operation branches in the Python
//! `IppHandler.do_POST`, minus the HTTP plumbing (see `server.rs`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::events::{self, UiEvent};
use crate::ipp::wire::{self, ParsedRequest};
use crate::jobs::Overrides;
use crate::render;
use crate::server::AppState;
use crate::upload;

fn utc_timestamp_compact() -> String {
    chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string()
}

fn ipp_response(bytes: Vec<u8>) -> Response {
    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "application/ipp")],
        bytes,
    )
        .into_response()
}

fn http_error(status: StatusCode, message: &str) -> Response {
    (status, message.to_string()).into_response()
}

fn write_spool_request(spool_dir: &Path, raw: &[u8], req: &ParsedRequest) -> std::io::Result<()> {
    std::fs::create_dir_all(spool_dir)?;
    std::fs::write(spool_dir.join("request.ipp"), raw)?;
    let meta = serde_json::to_string_pretty(&req.meta_json()).unwrap_or_default();
    std::fs::write(spool_dir.join("meta.json"), meta)?;
    Ok(())
}

fn store_first_png_in_temp(temp_dir: &Path, job_id: &str, page_num: usize, png: &[u8]) {
    if std::fs::create_dir_all(temp_dir).is_err() {
        return;
    }
    let path = temp_dir.join(format!("{job_id}_p{page_num}.png"));
    match std::fs::write(&path, png) {
        Ok(()) => tracing::info!(
            "PNG stored: job_id={} first_page={} png_bytes={} path={}",
            job_id,
            page_num,
            png.len(),
            path.display(),
        ),
        Err(e) => tracing::warn!("Failed to store temp PNG: {e}"),
    }
}

pub fn handle_get_printer_attributes(state: &AppState, req: &ParsedRequest, host: &str) -> Response {
    tracing::debug!("Handling Get-Printer-Attributes");
    let formats = render::supported_document_formats();
    let attrs = wire::printer_attributes(
        host,
        &state.config.ipp_path,
        state.registry.printer_is_busy(),
        &formats,
    );
    ipp_response(wire::build_response(
        req.version_major,
        req.version_minor,
        wire::STATUS_OK,
        req.request_id,
        &attrs,
    ))
}

pub fn handle_validate_job(req: &ParsedRequest) -> Response {
    tracing::debug!("Handling Validate-Job");
    ipp_response(wire::build_response(
        req.version_major,
        req.version_minor,
        wire::STATUS_OK,
        req.request_id,
        &wire::operation_attributes(),
    ))
}

pub fn handle_get_job_attributes(state: &AppState, req: &ParsedRequest, host: &str) -> Response {
    tracing::debug!("Handling Get-Job-Attributes");
    let job_id = req.job_id();
    let job_state = state.registry.get_job_state(job_id, 9);
    let attrs = wire::get_job_attributes_response(host, &state.config.ipp_path, job_id, job_state);
    ipp_response(wire::build_response(
        req.version_major,
        req.version_minor,
        wire::STATUS_OK,
        req.request_id,
        &attrs,
    ))
}

pub fn handle_get_jobs(state: &AppState, req: &ParsedRequest, host: &str) -> Response {
    tracing::debug!("Handling Get-Jobs");
    let attrs = wire::get_jobs_response(host, &state.config.ipp_path, &state.registry.list_jobs());
    ipp_response(wire::build_response(
        req.version_major,
        req.version_minor,
        wire::STATUS_OK,
        req.request_id,
        &attrs,
    ))
}

pub fn handle_unsupported(req: &ParsedRequest) -> Response {
    tracing::warn!(
        "Unsupported IPP operation {}; returning operation-not-supported",
        wire::op_name(req.operation_id)
    );
    ipp_response(wire::build_response(
        req.version_major,
        req.version_minor,
        wire::STATUS_SERVER_ERROR_OPERATION_NOT_SUPPORTED,
        req.request_id,
        &wire::operation_attributes(),
    ))
}

pub fn handle_create_job(
    state: &AppState,
    req: &ParsedRequest,
    raw: &[u8],
    host: &str,
    overrides: &Overrides,
) -> Response {
    tracing::debug!("Handling Create-Job");

    let job_id = state.registry.allocate_job_id();
    let job_uuid = uuid::Uuid::new_v4().simple().to_string();
    let spool_dir = state
        .config
        .spool_dir
        .join(format!("{}_{}_{}", utc_timestamp_compact(), job_id, job_uuid));

    tracing::info!("Spooling Create-Job job-id={} to {}", job_id, spool_dir.display());
    if let Err(e) = write_spool_request(&spool_dir, raw, req) {
        tracing::error!("Spool write failed: {e}");
        return http_error(StatusCode::INTERNAL_SERVER_ERROR, "spool write failed");
    }
    state.registry.register_job(job_id, spool_dir);
    // Persist per-request overrides so Send-Document can reuse them; macOS may
    // not preserve query params/path segments on the follow-up request.
    state.registry.register_job_overrides(job_id, overrides);

    let attrs = wire::create_job_response(host, &state.config.ipp_path, job_id);
    ipp_response(wire::build_response(
        req.version_major,
        req.version_minor,
        wire::STATUS_OK,
        req.request_id,
        &attrs,
    ))
}

/// Reject an empty document payload: HTTP 200 with IPP bad-request status.
fn empty_document_response(req: &ParsedRequest) -> Response {
    ipp_response(wire::build_response(
        req.version_major,
        req.version_minor,
        wire::STATUS_CLIENT_ERROR_BAD_REQUEST,
        req.request_id,
        &wire::operation_attributes(),
    ))
}

/// Shared render → spool PNGs → temp PNG → upload pipeline for
/// Print-Job / Send-Document. Runs pdfium in a blocking task.
#[allow(clippy::too_many_arguments)]
async fn render_and_store(
    state: &Arc<AppState>,
    req: &ParsedRequest,
    spool_dir: &Path,
    display_job_id: &str,
    upload_job_id: &str,
    effective_endpoint: &str,
    effective_auth_value: &str,
    post_enabled: bool,
) -> Result<(usize, BTreeMap<usize, Vec<u8>>), render::RenderError> {
    let document = req.document.clone();
    let declared_format = req.meta_str("document-format").to_string();
    let dpi = state.config.render_dpi;

    let (total, pages, assumed_format) =
        tokio::task::spawn_blocking(move || render::render_document_to_pngs(&document, &declared_format, dpi))
            .await
            .map_err(|e| render::RenderError {
                message: format!("render task join error: {e}"),
                unsupported: false,
            })??;

    if let Some(fmt) = assumed_format {
        tracing::debug!("document-format assumed as {fmt}");
    }

    for (page_num, png) in &pages {
        std::fs::write(spool_dir.join(format!("page_{page_num:04}.png")), png).map_err(|e| {
            render::RenderError { message: format!("spool PNG write failed: {e}"), unsupported: false }
        })?;
        events::send(&state.events, UiEvent::JobProgress { pages_seen: *page_num });
    }
    tracing::info!(
        "PNG generation succeeded: job_id={} total_pages={} spool_dir={}",
        display_job_id,
        total,
        spool_dir.display(),
    );

    if let Some((&first_page, png)) = pages.iter().next() {
        store_first_png_in_temp(&state.config.temp_dir, upload_job_id, first_page, png);
    }

    if post_enabled {
        // POST in background so the IPP response is quick.
        let upload_req = upload::UploadRequest {
            endpoint: effective_endpoint.to_string(),
            auth_header: state.config.post_auth_header.clone(),
            auth_value: effective_auth_value.to_string(),
            timeout_seconds: state.config.post_timeout_seconds,
            file_field: state.config.post_file_field.clone(),
            include_meta_fields: state.config.post_include_meta_fields,
            send_all_pages: state.config.post_send_all_pages,
            job_id: upload_job_id.to_string(),
            request_id: req.request_id.to_string(),
            document_format: req.meta_str("document-format").to_string(),
            job_name: req.meta_str("job-name").to_string(),
            printer_uri: req.meta_str("printer-uri").to_string(),
            user: req.meta_str("requesting-user-name").to_string(),
            total_pages: total,
            png_pages: pages.clone(),
        };
        tokio::spawn(upload::post_pages(upload_req));
    } else {
        tracing::info!("Upload disabled (POST_ENDPOINT empty); skipping POST");
    }

    Ok((total, pages))
}

/// Kick off postprocess (text-layer check → inbox) out of band, notifying the
/// dashboard when the mode is known. Mirrors the Python dashboard behavior.
fn spawn_postprocess(state: &Arc<AppState>, spool_dir: PathBuf, token: String) {
    let state = Arc::clone(state);
    tokio::task::spawn_blocking(move || {
        let result = crate::postprocess::process_job(&spool_dir, &state.config.inbox_dir);
        tracing::info!(
            "Postprocess: name={} mode={} pages={} chars={} inbox_files={} job_dir={} status={}",
            result.name,
            result.mode.as_str(),
            result.pages,
            result.chars,
            result.inbox.len(),
            result.job_dir.display(),
            result.status,
        );
        events::send(
            &state.events,
            UiEvent::JobMode { token, mode: result.mode.as_str().to_string() },
        );
    });
}

pub async fn handle_send_document(
    state: &Arc<AppState>,
    req: &ParsedRequest,
    raw: &[u8],
    host: &str,
    effective: &Overrides,
    safe_path_for_logs: &str,
) -> Response {
    tracing::debug!("Handling Send-Document");

    let job_id = req.job_id();
    if job_id > 0 {
        state.registry.set_job_state(job_id, 5);
    }

    // Resolve overrides for this job. If the HTTP path doesn't carry query
    // params anymore (common on macOS), reuse Create-Job overrides.
    let mut paper_id = effective.paper_id.clone();
    let mut auth_value = effective.auth_value.clone();
    if job_id > 0 && (paper_id.is_empty() || auth_value.is_empty()) {
        let job_overrides = state.registry.get_job_overrides(job_id);
        if paper_id.is_empty() {
            paper_id = job_overrides.paper_id;
        }
        if auth_value.is_empty() {
            auth_value = job_overrides.auth_value;
        }
    }

    let effective_endpoint = upload::resolve_endpoint_template(&state.config.post_endpoint, &paper_id);
    let mut post_enabled = !effective_endpoint.is_empty();
    if post_enabled && paper_id.is_empty() {
        tracing::warn!("Upload disabled for this job: missing paper_id (path={safe_path_for_logs})");
        post_enabled = false;
    }

    let spool_dir = match state.registry.get_job_spool_dir(job_id) {
        Some(dir) if job_id > 0 => dir,
        _ => {
            let job_uuid = uuid::Uuid::new_v4().simple().to_string();
            state
                .config
                .spool_dir
                .join(format!("{}_send_{}", utc_timestamp_compact(), job_uuid))
        }
    };

    let display_job_id =
        if job_id > 0 { job_id.to_string() } else { "(unknown)".to_string() };
    tracing::info!("Spooling Send-Document job-id={} to {}", display_job_id, spool_dir.display());
    if let Err(e) = write_spool_request(&spool_dir, raw, req) {
        tracing::error!("Spool write failed: {e}");
        return http_error(StatusCode::INTERNAL_SERVER_ERROR, "spool write failed");
    }
    if std::fs::write(spool_dir.join("document.bin"), &req.document).is_err() {
        return http_error(StatusCode::INTERNAL_SERVER_ERROR, "spool write failed");
    }

    if req.document.is_empty() {
        tracing::warn!(
            "Rejecting Send-Document with empty payload: request_id={} job_id={}",
            req.request_id,
            display_job_id,
        );
        return empty_document_response(req);
    }

    events::send(
        &state.events,
        UiEvent::JobStarted { name: req.meta_str("job-name").to_string() },
    );

    let upload_job_id = if job_id > 0 { job_id.to_string() } else { "send".to_string() };
    state.registry.job_activity_begin();
    let render_result = render_and_store(
        state,
        req,
        &spool_dir,
        &display_job_id,
        &upload_job_id,
        &effective_endpoint,
        &auth_value,
        post_enabled,
    )
    .await;
    state
        .registry
        .job_activity_end(std::time::Duration::from_secs(state.config.busy_grace_seconds));

    match render_result {
        Ok((total, _pages)) => {
            if job_id > 0 {
                state.registry.set_job_state(job_id, 9);
            }
            let token = format!("send-{}", req.request_id);
            events::send(
                &state.events,
                UiEvent::JobDone {
                    token: token.clone(),
                    name: req.meta_str("job-name").to_string(),
                    pages: total,
                },
            );
            spawn_postprocess(state, spool_dir, token);

            // Include job attributes (job-id/job-uri/job-state) so clients can
            // confirm acceptance.
            let attrs = wire::get_job_attributes_response(
                host,
                &state.config.ipp_path,
                job_id,
                state.registry.get_job_state(job_id, 9),
            );
            ipp_response(wire::build_response(
                req.version_major,
                req.version_minor,
                wire::STATUS_OK,
                req.request_id,
                &attrs,
            ))
        }
        Err(e) if e.unsupported => {
            if job_id > 0 {
                state.registry.set_job_state(job_id, 8);
            }
            events::send(&state.events, UiEvent::JobFailed);
            tracing::warn!("{e}");
            http_error(StatusCode::UNSUPPORTED_MEDIA_TYPE, &e.message)
        }
        Err(e) => {
            if job_id > 0 {
                state.registry.set_job_state(job_id, 8);
            }
            events::send(&state.events, UiEvent::JobFailed);
            tracing::error!("Render failed: {e}");
            http_error(StatusCode::INTERNAL_SERVER_ERROR, &format!("Render failed: {e}"))
        }
    }
}

pub async fn handle_print_job(
    state: &Arc<AppState>,
    req: &ParsedRequest,
    raw: &[u8],
    host: &str,
    effective: &Overrides,
    safe_path_for_logs: &str,
) -> Response {
    let effective_endpoint =
        upload::resolve_endpoint_template(&state.config.post_endpoint, &effective.paper_id);
    let mut post_enabled = !effective_endpoint.is_empty();
    if post_enabled && effective.paper_id.is_empty() {
        tracing::warn!("Upload disabled for this request: missing paper_id (path={safe_path_for_logs})");
        post_enabled = false;
    }

    let job_uuid = uuid::Uuid::new_v4().simple().to_string();
    let job_id_int = state.registry.allocate_job_id();
    let spool_dir = state
        .config
        .spool_dir
        .join(format!("{}_{}", utc_timestamp_compact(), job_uuid));

    tracing::info!(
        "Spooling job {} (job-id={}) to {}",
        job_uuid,
        job_id_int,
        spool_dir.display(),
    );
    if let Err(e) = write_spool_request(&spool_dir, raw, req) {
        tracing::error!("Spool write failed: {e}");
        return http_error(StatusCode::INTERNAL_SERVER_ERROR, "spool write failed");
    }
    if std::fs::write(spool_dir.join("document.bin"), &req.document).is_err() {
        return http_error(StatusCode::INTERNAL_SERVER_ERROR, "spool write failed");
    }
    state.registry.register_job(job_id_int, spool_dir.clone());

    if req.document.is_empty() {
        state.registry.set_job_state(job_id_int, 8);
        tracing::warn!(
            "Rejecting Print-Job with empty payload: request_id={}",
            req.request_id,
        );
        return empty_document_response(req);
    }

    events::send(
        &state.events,
        UiEvent::JobStarted { name: req.meta_str("job-name").to_string() },
    );

    state.registry.job_activity_begin();
    let render_result = render_and_store(
        state,
        req,
        &spool_dir,
        &job_uuid,
        &job_uuid,
        &effective_endpoint,
        &effective.auth_value,
        post_enabled,
    )
    .await;
    state
        .registry
        .job_activity_end(std::time::Duration::from_secs(state.config.busy_grace_seconds));

    match render_result {
        Ok((total, _pages)) => {
            // RFC 8011 requires job-id / job-uri / job-state in a Print-Job
            // response; without job-id, Android (Mopria) cannot confirm the job
            // was accepted and keeps resending it.
            state.registry.set_job_state(job_id_int, 9);
            events::send(
                &state.events,
                UiEvent::JobDone {
                    token: job_uuid.clone(),
                    name: req.meta_str("job-name").to_string(),
                    pages: total,
                },
            );
            spawn_postprocess(state, spool_dir, job_uuid);

            let attrs = wire::get_job_attributes_response(
                host,
                &state.config.ipp_path,
                job_id_int,
                9, // completed: the document is already rendered at this point
            );
            let response = ipp_response(wire::build_response(
                req.version_major,
                req.version_minor,
                wire::STATUS_OK,
                req.request_id,
                &attrs,
            ));
            tracing::debug!("IPP response sent: status=successful-ok request_id={}", req.request_id);
            response
        }
        Err(e) if e.unsupported => {
            state.registry.set_job_state(job_id_int, 8);
            events::send(&state.events, UiEvent::JobFailed);
            tracing::warn!("{e}");
            http_error(StatusCode::UNSUPPORTED_MEDIA_TYPE, &e.message)
        }
        Err(e) => {
            state.registry.set_job_state(job_id_int, 8);
            events::send(&state.events, UiEvent::JobFailed);
            tracing::error!("Render/POST failed: {e}");
            http_error(StatusCode::INTERNAL_SERVER_ERROR, &format!("Render/POST failed: {e}"))
        }
    }
}
