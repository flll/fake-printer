//! Configuration loading: `.env` (legacy layout compatible) + `install.json` + env vars.
//!
//! Key names are identical to the Python implementation so an existing
//! deployment works without editing any config file. Resolution order per
//! value: environment variable (including `.env`) > `install.json` > default.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

// Matches the canonical repo scripts; the legacy deployment sets
// PRINTER_NAME="Lenovo PNG Printer" in .env to keep saved printers working.
pub const DEFAULT_PRINTER_NAME: &str = "Fake Printer";
/// Minimum number of non-whitespace chars for a PDF to count as "has text layer".
pub const MIN_TEXT_CHARS: usize = 20;

#[derive(Debug, Clone)]
pub struct Config {
    #[allow(dead_code)]
    pub install_root: PathBuf,
    pub listen_host: String,
    pub listen_port: u16,
    pub ipp_path: String,
    pub max_bytes: usize,
    pub spool_dir: PathBuf,
    pub temp_dir: PathBuf,
    pub inbox_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub render_dpi: u16,
    pub busy_grace_seconds: u64,
    pub shared_token: String,
    pub printer_name: String,
    pub log_level: String,
    pub log_headers: bool,
    // Upload (paperlesspaper cloud) settings.
    pub post_endpoint: String,
    pub post_auth_header: String,
    /// Parsed for .env compatibility; the Python server also never reads it —
    /// the effective auth value always comes from per-request overrides.
    #[allow(dead_code)]
    pub post_auth_value: String,
    pub post_timeout_seconds: u64,
    pub post_file_field: String,
    pub post_include_meta_fields: bool,
    pub post_send_all_pages: bool,
}

fn env_str(name: &str, default: &str) -> String {
    match std::env::var(name) {
        Ok(v) if !v.is_empty() => v,
        _ => default.to_string(),
    }
}

fn env_parse<T: std::str::FromStr>(name: &str, default: T) -> T {
    match std::env::var(name) {
        Ok(v) if !v.is_empty() => v.trim().parse().unwrap_or(default),
        _ => default,
    }
}

fn env_bool(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(v) if !v.is_empty() => {
            matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
        }
        _ => default,
    }
}

/// Resolve a possibly-relative path against the install root.
fn resolve(root: &Path, value: &str) -> PathBuf {
    let p = PathBuf::from(value);
    if p.is_absolute() { p } else { root.join(p) }
}

fn read_manifest(root: &Path) -> HashMap<String, serde_json::Value> {
    let path = root.join("install.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return HashMap::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

impl Config {
    /// Load configuration. `install_root` is the directory containing the exe
    /// (falling back to the current directory when unavailable).
    pub fn load() -> Self {
        let install_root = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."));

        // Legacy layout keeps .env inside the cloned engine directory;
        // prefer it, then fall back to .env next to the exe.
        for candidate in [
            install_root.join("paperlessprinter").join(".env"),
            install_root.join(".env"),
        ] {
            if candidate.exists() {
                let _ = dotenvy::from_path(&candidate);
                break;
            }
        }

        let manifest = read_manifest(&install_root);
        let manifest_u64 = |key: &str| manifest.get(key).and_then(|v| v.as_u64());
        let manifest_str =
            |key: &str| manifest.get(key).and_then(|v| v.as_str()).map(str::to_string);

        // Port: env (incl. .env) > PORT > install.json > 8631
        let listen_port = match std::env::var("IPP_LISTEN_PORT") {
            Ok(v) if !v.is_empty() => v.trim().parse().unwrap_or(8631),
            _ => match std::env::var("PORT") {
                Ok(v) if !v.is_empty() => v.trim().parse().unwrap_or(8631),
                _ => manifest_u64("listen_port").map(|v| v as u16).unwrap_or(8631),
            },
        };

        let render_dpi = match std::env::var("IPP_RENDER_DPI") {
            Ok(v) if !v.is_empty() => v.trim().parse().unwrap_or(150),
            _ => manifest_u64("render_dpi").map(|v| v as u16).unwrap_or(150),
        };

        let spool_dir = match std::env::var("IPP_SPOOL_DIR") {
            Ok(v) if !v.is_empty() => resolve(&install_root, &v),
            _ => manifest_str("spool_dir")
                .map(|v| resolve(&install_root, &v))
                .unwrap_or_else(|| install_root.join("spool")),
        };
        let temp_dir = match std::env::var("IPP_TEMP_DIR") {
            Ok(v) if !v.is_empty() => resolve(&install_root, &v),
            _ => manifest_str("temp_dir")
                .map(|v| resolve(&install_root, &v))
                .unwrap_or_else(|| install_root.join("temp")),
        };
        // INBOX_DIR is new in the Rust port (the Python postprocess always
        // used <install_root>/inbox); install.json stays authoritative below.
        let inbox_dir = match std::env::var("INBOX_DIR") {
            Ok(v) if !v.is_empty() => resolve(&install_root, &v),
            _ => manifest_str("inbox_dir")
                .map(|v| resolve(&install_root, &v))
                .unwrap_or_else(|| install_root.join("inbox")),
        };

        Self {
            listen_host: env_str("IPP_LISTEN_HOST", "0.0.0.0"),
            listen_port,
            ipp_path: env_str("IPP_PATH", "/ipp/print"),
            max_bytes: env_parse("IPP_MAX_BYTES", 100 * 1024 * 1024),
            render_dpi,
            busy_grace_seconds: env_parse("IPP_BUSY_GRACE_SECONDS", 4u64),
            shared_token: env_str("IPP_SHARED_TOKEN", ""),
            printer_name: env_str("PRINTER_NAME", DEFAULT_PRINTER_NAME),
            log_level: env_str("LOG_LEVEL", "INFO"),
            log_headers: env_bool("LOG_HEADERS", false),
            post_endpoint: env_str("POST_ENDPOINT", ""),
            post_auth_header: env_str("POST_AUTH_HEADER", ""),
            post_auth_value: env_str("POST_AUTH_VALUE", ""),
            post_timeout_seconds: env_parse("POST_TIMEOUT_SECONDS", 30u64),
            post_file_field: env_str("POST_FILE_FIELD", "file"),
            post_include_meta_fields: env_bool("POST_INCLUDE_META_FIELDS", true),
            post_send_all_pages: env_bool("POST_SEND_ALL_PAGES", false),
            logs_dir: install_root.join("logs"),
            spool_dir,
            temp_dir,
            inbox_dir,
            install_root,
        }
    }
}
