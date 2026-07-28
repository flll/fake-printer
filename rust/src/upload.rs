//! Optional POST upload of rendered pages (paperlesspaper cloud integration).
//! Port of `post_pages` and `_resolve_endpoint_template` from the Python server.

use std::collections::BTreeMap;

use url::Url;

/// Resolve `POST_ENDPOINT` against a paper id: replace known placeholders, or
/// append the id as the final path segment when no placeholder is present.
pub fn resolve_endpoint_template(endpoint: &str, paper_id: &str) -> String {
    if endpoint.is_empty() || paper_id.is_empty() {
        return endpoint.to_string();
    }
    let placeholders = ["<paperId>", "{PAPER_ID}", "{paper_id}"];
    if placeholders.iter().any(|p| endpoint.contains(p)) {
        return endpoint
            .replace("<paperId>", paper_id)
            .replace("{PAPER_ID}", paper_id)
            .replace("{paper_id}", paper_id);
    }

    let Ok(mut parts) = Url::parse(endpoint) else {
        return endpoint.to_string();
    };
    let existing_path = parts.path().to_string();
    let normalized_existing = existing_path.trim_end_matches('/').to_string();

    // Avoid double-appending if it's already present.
    let candidate_path = if normalized_existing.ends_with(&format!("/{paper_id}"))
        || normalized_existing == *paper_id
    {
        existing_path
    } else {
        format!("{normalized_existing}/{paper_id}")
    };
    parts.set_path(&candidate_path);
    parts.to_string()
}

pub struct UploadRequest {
    pub endpoint: String,
    pub auth_header: String,
    pub auth_value: String,
    pub timeout_seconds: u64,
    pub file_field: String,
    pub include_meta_fields: bool,
    pub send_all_pages: bool,
    pub job_id: String,
    /// Meta fields forwarded when `include_meta_fields` is on.
    pub request_id: String,
    pub document_format: String,
    pub job_name: String,
    pub printer_uri: String,
    pub user: String,
    pub total_pages: usize,
    pub png_pages: BTreeMap<usize, Vec<u8>>,
}

/// POST rendered pages as multipart form data. Never panics; logs failures.
pub async fn post_pages(req: UploadRequest) {
    if req.png_pages.is_empty() {
        tracing::warn!("Skipping upload: no PNG pages to POST for job_id={}", req.job_id);
        return;
    }

    let mut page_numbers: Vec<usize> = req.png_pages.keys().copied().collect();
    if !req.send_all_pages {
        let first = if req.png_pages.contains_key(&1) { 1 } else { page_numbers[0] };
        page_numbers = vec![first];
    }

    let page_label = if page_numbers.len() == 1 {
        page_numbers[0].to_string()
    } else {
        format!("{}-{}", page_numbers[0], page_numbers[page_numbers.len() - 1])
    };
    let total_png_bytes: usize = page_numbers.iter().map(|n| req.png_pages[n].len()).sum();
    tracing::info!(
        "POST start: endpoint={} job_id={} pages={} file_parts={} total_pages={} png_bytes={} mode={}",
        req.endpoint,
        req.job_id,
        page_label,
        page_numbers.len(),
        req.total_pages,
        total_png_bytes,
        if req.send_all_pages { "all-pages-single-post" } else { "first-page-only" },
    );

    let mut form = reqwest::multipart::Form::new();
    if req.include_meta_fields {
        form = form
            .text("job_id", req.job_id.clone())
            .text("request_id", req.request_id.clone())
            .text("total_pages", req.total_pages.to_string())
            .text("document_format", req.document_format.clone())
            .text("job_name", req.job_name.clone())
            .text("printer_uri", req.printer_uri.clone())
            .text("user", req.user.clone());
        if page_numbers.len() == 1 {
            form = form.text("page", page_numbers[0].to_string());
        }
    }

    let file_field = if req.file_field.is_empty() { "file".to_string() } else { req.file_field.clone() };
    for page_num in &page_numbers {
        let part = reqwest::multipart::Part::bytes(req.png_pages[page_num].clone())
            .file_name(format!("{}_p{}.png", req.job_id, page_num))
            .mime_str("image/png")
            .expect("static mime type");
        form = form.part(file_field.clone(), part);
    }

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(req.timeout_seconds))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Upload failed building client: {e}");
            return;
        }
    };

    let mut builder = client.post(&req.endpoint).multipart(form);
    if !req.auth_header.is_empty() && !req.auth_value.is_empty() {
        builder = builder.header(&req.auth_header, &req.auth_value);
    }

    match builder.send().await {
        Ok(resp) => {
            let status = resp.status();
            tracing::info!(
                "POST response: endpoint={} status={} job_id={} pages={} file_parts={}",
                req.endpoint,
                status.as_u16(),
                req.job_id,
                page_label,
                page_numbers.len(),
            );
            if status.is_client_error() || status.is_server_error() {
                let mut body = resp.text().await.unwrap_or_else(|_| "<unreadable response body>".into());
                if body.len() > 2000 {
                    body.truncate(2000);
                    body.push_str("...<truncated>");
                }
                if !body.is_empty() {
                    tracing::warn!("POST response body: {body}");
                }
                tracing::error!(
                    "Upload failed: endpoint={} job_id={} pages={} status={}",
                    req.endpoint,
                    req.job_id,
                    page_label,
                    status.as_u16(),
                );
            } else {
                tracing::info!(
                    "POST succeeded: endpoint={} job_id={} pages={}",
                    req.endpoint,
                    req.job_id,
                    page_label,
                );
            }
        }
        Err(e) => {
            // Never crash the server on upload failures.
            tracing::error!(
                "Upload failed: endpoint={} job_id={} pages={} error={e}",
                req.endpoint,
                req.job_id,
                page_label,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_endpoint_template;

    #[test]
    fn placeholder_styles() {
        assert_eq!(
            resolve_endpoint_template("https://x/api/<paperId>/img", "42"),
            "https://x/api/42/img"
        );
        assert_eq!(
            resolve_endpoint_template("https://x/api/{PAPER_ID}", "42"),
            "https://x/api/42"
        );
        assert_eq!(
            resolve_endpoint_template("https://x/api/{paper_id}", "42"),
            "https://x/api/42"
        );
    }

    #[test]
    fn appends_as_final_segment() {
        assert_eq!(
            resolve_endpoint_template("https://x/api/upload", "42"),
            "https://x/api/upload/42"
        );
        assert_eq!(
            resolve_endpoint_template("https://x/api/upload/", "42"),
            "https://x/api/upload/42"
        );
    }

    #[test]
    fn avoids_double_append() {
        assert_eq!(
            resolve_endpoint_template("https://x/api/upload/42", "42"),
            "https://x/api/upload/42"
        );
    }

    #[test]
    fn empty_inputs_pass_through() {
        assert_eq!(resolve_endpoint_template("", "42"), "");
        assert_eq!(resolve_endpoint_template("https://x/u", ""), "https://x/u");
    }
}
