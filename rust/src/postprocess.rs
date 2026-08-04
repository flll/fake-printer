//! Post-process captured print jobs for agent-friendly storage
//! (PDF primary, PNG fallback). Port of `scripts/postprocess.py`.

use std::path::{Path, PathBuf};

use crate::config::MIN_TEXT_CHARS;
use crate::render;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Text,
    Png,
}

impl Mode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Mode::Text => "text",
            Mode::Png => "png",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PostprocessResult {
    pub mode: Mode,
    pub pages: usize,
    pub chars: usize,
    pub name: String,
    pub job_dir: PathBuf,
    pub inbox: Vec<PathBuf>,
    pub status: String,
}

fn read_meta_job_name(job_dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(job_dir.join("meta.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value
        .get("job-name")
        .or_else(|| value.get("job_name"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Sanitize a job name into a filesystem-safe stem (Python `_safe_stem`).
fn safe_stem(name: &str) -> String {
    let stem = Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("job");
    let cleaned: String = stem
        .chars()
        .map(|c| {
            if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
                || (c as u32) < 0x20
            {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches(|c| c == ' ' || c == '.');
    if trimmed.is_empty() { "job".to_string() } else { trimmed.to_string() }
}

/// Derive the inbox stamp `YYYYMMDD_HHMM` from the spool folder name
/// (`YYYYMMDDTHHMMSSZ_...`), falling back to local now.
fn inbox_stamp(job_dir: &Path) -> String {
    let folder = job_dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let bytes = folder.as_bytes();
    if bytes.len() >= 16
        && bytes[..8].iter().all(u8::is_ascii_digit)
        && bytes[8] == b'T'
        && bytes[9..15].iter().all(u8::is_ascii_digit)
        && bytes[15] == b'Z'
    {
        return format!("{}_{}", &folder[..8], &folder[9..13]);
    }
    chrono::Local::now().format("%Y%m%d_%H%M").to_string()
}

fn list_pngs(job_dir: &Path) -> Vec<PathBuf> {
    let mut pngs: Vec<PathBuf> = std::fs::read_dir(job_dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("page_") && n.ends_with(".png"))
        })
        .collect();
    pngs.sort();
    pngs
}

fn copy_to_inbox(
    job_dir: &Path,
    inbox_dir: &Path,
    job_name: &str,
    mode: &Mode,
    pngs: &[PathBuf],
) -> Vec<PathBuf> {
    if std::fs::create_dir_all(inbox_dir).is_err() {
        return Vec::new();
    }
    let stamp = inbox_stamp(job_dir);
    let stem = safe_stem(job_name);
    let mut out = Vec::new();

    match mode {
        Mode::Text => {
            let pdf_src = job_dir.join("document.pdf");
            if pdf_src.exists() {
                let pdf_dst = inbox_dir.join(format!("{stamp}_{stem}.pdf"));
                if std::fs::copy(&pdf_src, &pdf_dst).is_ok() {
                    out.push(pdf_dst);
                }
            }
        }
        Mode::Png => {
            for (i, png) in pngs.iter().enumerate() {
                let png_dst = inbox_dir.join(format!("{stamp}_{stem}_p{:03}.png", i + 1));
                if std::fs::copy(png, &png_dst).is_ok() {
                    out.push(png_dst);
                }
            }
        }
    }
    out
}

/// Guard against garbled "text layers": a PDF whose fonts omit a ToUnicode
/// CMap (e.g. iText Identity-H) renders fine but extracts as control-char
/// garbage. Character count alone would misclassify it as TEXT (see
/// AGENTS.md gotcha), so require that most non-whitespace chars are readable.
fn looks_readable(text: &str) -> bool {
    let mut total = 0usize;
    let mut readable = 0usize;
    for c in text.chars().filter(|c| !c.is_whitespace()) {
        total += 1;
        let garbage = c.is_control()
            || c == '\u{FFFD}'
            || ('\u{E000}'..='\u{F8FF}').contains(&c); // private use area
        if !garbage {
            readable += 1;
        }
    }
    if total == 0 {
        return false;
    }
    readable * 10 >= total * 7 // >= 70% readable
}

/// Convert a spooled job into agent-friendly artifacts in the inbox.
pub fn process_job(job_dir: &Path, inbox_dir: &Path) -> PostprocessResult {
    // Remove legacy text.md if present.
    let _ = std::fs::remove_file(job_dir.join("text.md"));

    let job_name = read_meta_job_name(job_dir).unwrap_or_else(|| {
        job_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("job")
            .to_string()
    });

    let pngs = list_pngs(job_dir);
    let pages = if pngs.is_empty() { 1 } else { pngs.len() };
    let doc_bin = job_dir.join("document.bin");
    let pdf_path = job_dir.join("document.pdf");

    let finish = |mode: Mode, chars: usize, status: String| {
        let inbox = copy_to_inbox(job_dir, inbox_dir, &job_name, &mode, &pngs);
        PostprocessResult {
            mode,
            pages,
            chars,
            name: job_name.clone(),
            job_dir: job_dir.to_path_buf(),
            inbox,
            status,
        }
    };

    let png_refs = || {
        if pngs.is_empty() {
            "(none)".to_string()
        } else {
            pngs.iter()
                .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
                .collect::<Vec<_>>()
                .join(", ")
        }
    };

    // ── not PDF (raster etc.) ────────────────────────────────────────────
    let raw = match std::fs::read(&doc_bin) {
        Ok(raw) => raw,
        Err(_) => {
            let status = format!("No text layer — fell back to PNG (see {})", png_refs());
            return finish(Mode::Png, 0, status);
        }
    };
    if !raw.starts_with(b"%PDF") {
        let status = format!("No text layer — fell back to PNG (see {})", png_refs());
        return finish(Mode::Png, 0, status);
    }

    // ── PDF: check text layer ────────────────────────────────────────────
    if !pdf_path.exists() {
        let _ = std::fs::rename(&doc_bin, &pdf_path);
    }

    let extracted = std::fs::read(&pdf_path)
        .ok()
        .and_then(|bytes| render::pdf_text_all(&bytes).ok())
        .unwrap_or_default();
    let chars = extracted.chars().filter(|c| !c.is_whitespace()).count();

    if chars >= MIN_TEXT_CHARS && looks_readable(&extracted) {
        for png in &pngs {
            let _ = std::fs::remove_file(png);
        }
        // Note: pngs list already captured; Text mode ignores it for inbox.
        let inbox = copy_to_inbox(job_dir, inbox_dir, &job_name, &Mode::Text, &pngs);
        return PostprocessResult {
            mode: Mode::Text,
            pages,
            chars,
            name: job_name,
            job_dir: job_dir.to_path_buf(),
            inbox,
            status: "Saved PDF with text layer".to_string(),
        };
    }

    // ── image-only PDF → PNG fallback ────────────────────────────────────
    let status = format!("No text layer — fell back to PNG from PDF (see {})", png_refs());
    finish(Mode::Png, 0, status)
}

#[cfg(test)]
mod tests {
    use super::{looks_readable, safe_stem};

    #[test]
    fn readable_text_passes() {
        assert!(looks_readable("普通の日本語テキストと English words 12345."));
    }

    #[test]
    fn control_char_garbage_fails() {
        let garbled: String = (1u8..=20).map(|b| b as char).cycle().take(100).collect();
        assert!(!looks_readable(&garbled));
        assert!(!looks_readable(""));
    }

    #[test]
    fn private_use_area_fails() {
        let pua: String = std::iter::repeat('\u{E123}').take(50).collect();
        assert!(!looks_readable(&pua));
    }

    #[test]
    fn safe_stem_sanitizes() {
        assert_eq!(safe_stem("報告書: 最終<版>.pdf"), "報告書_ 最終_版_");
        assert_eq!(safe_stem(""), "job");
        assert_eq!(safe_stem("..."), "job");
    }
}
