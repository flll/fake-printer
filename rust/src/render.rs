//! Document rendering: PDF via pdfium (embedded), PostScript via Ghostscript.
//!
//! Mirrors `render_document_to_pngs` and friends from the Python server.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex, OnceLock};

use pdfium_render::prelude::*;
use sha2::{Digest, Sha256};

/// Embedded pdfium library, extracted on first use.
static PDFIUM_DLL: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/pdfium.dll"));
static PDFIUM_DLL_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Errors that map to the Python `ValueError` -> HTTP 415 path.
#[derive(Debug)]
pub struct RenderError {
    pub message: String,
    /// true = client-caused (unsupported/invalid payload, HTTP 415);
    /// false = internal failure (HTTP 500).
    pub unsupported: bool,
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for RenderError {}

fn unsupported(message: impl Into<String>) -> RenderError {
    RenderError { message: message.into(), unsupported: true }
}

fn internal(message: impl Into<String>) -> RenderError {
    RenderError { message: message.into(), unsupported: false }
}

/// Extract the embedded pdfium.dll under %LOCALAPPDATA% (hash-keyed dir) and
/// return its path. Idempotent; safe across concurrent processes because the
/// final rename is atomic on the same volume.
pub fn ensure_pdfium_dll() -> std::io::Result<PathBuf> {
    if let Some(path) = PDFIUM_DLL_PATH.get() {
        return Ok(path.clone());
    }
    let mut hasher = Sha256::new();
    hasher.update(PDFIUM_DLL);
    let digest: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("fake-printer")
        .join("pdfium")
        .join(&digest[..16]);
    let dll = base.join("pdfium.dll");
    if !dll.exists() {
        std::fs::create_dir_all(&base)?;
        let tmp = base.join(format!("pdfium.dll.tmp-{}", std::process::id()));
        std::fs::write(&tmp, PDFIUM_DLL)?;
        match std::fs::rename(&tmp, &dll) {
            Ok(()) => {}
            Err(_) if dll.exists() => {
                let _ = std::fs::remove_file(&tmp);
            }
            Err(e) => return Err(e),
        }
    }
    let _ = PDFIUM_DLL_PATH.set(dll.clone());
    Ok(dll)
}

/// Process-wide pdfium instance: the library refuses to be bound twice, and
/// its FFI is not reentrant — all document work runs under RENDER_LOCK.
static PDFIUM: LazyLock<Result<Pdfium, String>> = LazyLock::new(|| {
    let path = ensure_pdfium_dll().map_err(|e| format!("pdfium extract failed: {e}"))?;
    let bindings =
        Pdfium::bind_to_library(&path).map_err(|e| format!("pdfium bind failed: {e:?}"))?;
    Ok(Pdfium::new(bindings))
});
static RENDER_LOCK: Mutex<()> = Mutex::new(());

fn pdfium() -> Result<&'static Pdfium, RenderError> {
    PDFIUM.as_ref().map_err(|e| internal(e.clone()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    Pdf,
    Postscript,
    Unknown,
}

pub fn detect_document_kind(document: &[u8], declared_format: &str) -> DocumentKind {
    if document.starts_with(b"%PDF") {
        return DocumentKind::Pdf;
    }
    if document.starts_with(b"%!PS") {
        return DocumentKind::Postscript;
    }
    match declared_format.trim().to_ascii_lowercase().as_str() {
        "application/pdf" => DocumentKind::Pdf,
        "application/postscript" | "application/vnd.cups-postscript" => DocumentKind::Postscript,
        _ => DocumentKind::Unknown,
    }
}

pub fn ghostscript_command() -> Option<String> {
    for name in ["gs", "ghostscript", "gswin64c"] {
        if which(name) {
            return Some(name.to_string());
        }
    }
    None
}

fn which(name: &str) -> bool {
    let Ok(path_var) = std::env::var("PATH") else { return false };
    let exts = ["", ".exe", ".bat", ".cmd"];
    std::env::split_paths(&path_var).any(|dir| {
        exts.iter().any(|ext| dir.join(format!("{name}{ext}")).is_file())
    })
}

pub fn supported_document_formats() -> Vec<&'static str> {
    let mut formats = vec!["application/pdf"];
    if ghostscript_command().is_some() {
        formats.extend(["application/postscript", "application/vnd.cups-postscript"]);
    }
    formats
}

/// Render a PDF to per-page PNG bytes at the given DPI.
/// Returns (page_count, pages keyed by 1-based page number).
pub fn render_pdf_to_pngs(
    pdf_bytes: &[u8],
    dpi: u16,
) -> Result<(usize, BTreeMap<usize, Vec<u8>>), RenderError> {
    let _guard = RENDER_LOCK.lock().unwrap();
    let pdfium = pdfium()?;
    let document = pdfium.load_pdf_from_byte_slice(pdf_bytes, None).map_err(|e| match e {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
            unsupported("PDF payload is encrypted and requires a password")
        }
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FormatError) => {
            unsupported(format!("PDF payload could not be parsed: {e:?}"))
        }
        other => unsupported(format!("PDF payload could not be opened: {other:?}")),
    })?;

    let total = document.pages().len() as usize;
    if total == 0 {
        return Err(unsupported("PDF payload contains zero pages"));
    }

    let scale = dpi as f32 / 72.0;
    let config = PdfRenderConfig::new().scale_page_by_factor(scale);

    let mut pages = BTreeMap::new();
    for (index, page) in document.pages().iter().enumerate() {
        let bitmap = page
            .render_with_config(&config)
            .map_err(|e| internal(format!("pdfium render failed on page {}: {e:?}", index + 1)))?;
        let image = bitmap
            .as_image()
            .map_err(|e| internal(format!("bitmap conversion failed on page {}: {e:?}", index + 1)))?;
        let mut png = Vec::new();
        image
            .into_rgb8()
            .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .map_err(|e| internal(format!("PNG encode failed on page {}: {e}", index + 1)))?;
        pages.insert(index + 1, png);
    }
    Ok((total, pages))
}

/// Extract the full text of all pages (text-layer check + readability gate).
pub fn pdf_text_all(pdf_bytes: &[u8]) -> Result<String, RenderError> {
    let _guard = RENDER_LOCK.lock().unwrap();
    let pdfium = pdfium()?;
    let document = pdfium
        .load_pdf_from_byte_slice(pdf_bytes, None)
        .map_err(|e| unsupported(format!("PDF payload could not be opened: {e:?}")))?;
    let mut out = String::new();
    for page in document.pages().iter() {
        let text = page
            .text()
            .map_err(|e| internal(format!("pdfium text extraction failed: {e:?}")))?;
        out.push_str(&text.all());
    }
    Ok(out)
}

/// Render PostScript via Ghostscript (`gs -sDEVICE=png16m`).
pub fn render_postscript_to_pngs(
    document: &[u8],
    dpi: u16,
) -> Result<(usize, BTreeMap<usize, Vec<u8>>), RenderError> {
    let Some(gs) = ghostscript_command() else {
        return Err(unsupported(
            "PostScript payload received but Ghostscript is not installed; \
             install Ghostscript to accept Generic IPP/PostScript printer output",
        ));
    };

    let temp_root = std::env::temp_dir().join(format!(
        "ipp-postscript-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&temp_root)
        .map_err(|e| internal(format!("temp dir create failed: {e}")))?;
    let result = (|| {
        let input_path = temp_root.join("input.ps");
        std::fs::write(&input_path, document)
            .map_err(|e| internal(format!("temp write failed: {e}")))?;
        let output_pattern = temp_root.join("page-%04d.png");

        let output = std::process::Command::new(&gs)
            .args(["-q", "-dSAFER", "-dBATCH", "-dNOPAUSE", "-sDEVICE=png16m"])
            .arg(format!("-r{dpi}"))
            .arg(format!("-sOutputFile={}", output_pattern.display()))
            .arg(&input_path)
            .output()
            .map_err(|e| internal(format!("failed to run Ghostscript: {e}")))?;

        let mut files: Vec<PathBuf> = std::fs::read_dir(&temp_root)
            .map_err(|e| internal(format!("temp readdir failed: {e}")))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("page-") && n.ends_with(".png"))
            })
            .collect();
        files.sort();

        if !output.status.success() || files.is_empty() {
            let details = if !output.stderr.is_empty() {
                String::from_utf8_lossy(&output.stderr).trim().to_string()
            } else if !output.stdout.is_empty() {
                String::from_utf8_lossy(&output.stdout).trim().to_string()
            } else {
                "Ghostscript conversion failed".to_string()
            };
            return Err(unsupported(format!("Failed to render PostScript payload: {details}")));
        }

        let mut pages = BTreeMap::new();
        for (index, file) in files.iter().enumerate() {
            let bytes = std::fs::read(file)
                .map_err(|e| internal(format!("read rendered page failed: {e}")))?;
            pages.insert(index + 1, bytes);
        }
        Ok((files.len(), pages))
    })();
    let _ = std::fs::remove_dir_all(&temp_root);
    result
}

/// Dispatch by detected kind; mirrors Python `render_document_to_pngs`.
/// Also returns the effective document format when the request omitted it.
pub fn render_document_to_pngs(
    document: &[u8],
    declared_format: &str,
    dpi: u16,
) -> Result<(usize, BTreeMap<usize, Vec<u8>>, Option<&'static str>), RenderError> {
    match detect_document_kind(document, declared_format) {
        DocumentKind::Pdf => {
            tracing::info!("Rendering PDF payload to PNG via pdfium");
            let (total, pages) = render_pdf_to_pngs(document, dpi)?;
            let assumed = declared_format.is_empty().then_some("application/pdf");
            Ok((total, pages, assumed))
        }
        DocumentKind::Postscript => {
            tracing::info!("Rendering PostScript payload to PNG via Ghostscript");
            let (total, pages) = render_postscript_to_pngs(document, dpi)?;
            let assumed = declared_format.is_empty().then_some("application/postscript");
            Ok((total, pages, assumed))
        }
        DocumentKind::Unknown => {
            let first: Vec<u8> = document.iter().copied().take(12).collect();
            Err(unsupported(format!("Unsupported document payload (first bytes={first:?})")))
        }
    }
}
