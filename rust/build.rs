//! Obtains pdfium.dll (embedded into the exe at compile time, extracted at
//! runtime). Source order:
//! 1. `PDFIUM_DLL_PATH` env var (offline builds; hash not enforced)
//! 2. `third_party/pdfium/pdfium.dll` (verified against the pinned hash)
//! 3. Download from the pinned bblanchon/pdfium-binaries release using
//!    `curl` + `tar` (both ship with Windows 10+), then verify.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

// chromium/7881 matches pdfium-render 0.9.3's default `pdfium_7881` bindings.
const PDFIUM_URL: &str =
    "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F7881/pdfium-win-x64.tgz";
const PDFIUM_SHA256: &str = "79d4676b656cfb1abcea88f9ade3b4b0826c5200382db5f4ec72a636c598c118";

fn sha256_hex(path: &Path) -> String {
    let data = std::fs::read(path).expect("read dll for hashing");
    let mut hasher = Sha256::new();
    hasher.update(&data);
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn main() {
    println!("cargo:rerun-if-env-changed=PDFIUM_DLL_PATH");
    println!("cargo:rerun-if-changed=third_party/pdfium/pdfium.dll");

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let dest = out_dir.join("pdfium.dll");
    let vendored = PathBuf::from("third_party/pdfium/pdfium.dll");

    if let Ok(custom) = std::env::var("PDFIUM_DLL_PATH") {
        std::fs::copy(&custom, &dest).expect("copy PDFIUM_DLL_PATH dll");
        return;
    }

    if !vendored.exists() {
        let tgz = out_dir.join("pdfium-win-x64.tgz");
        let status = Command::new("curl")
            .args(["-sSL", "-o"])
            .arg(&tgz)
            .arg(PDFIUM_URL)
            .status()
            .expect("run curl (required to download pdfium.dll)");
        assert!(status.success(), "curl failed downloading {PDFIUM_URL}");

        let extract_dir = out_dir.join("pdfium-extract");
        std::fs::create_dir_all(&extract_dir).unwrap();
        let status = Command::new("tar")
            .arg("xzf")
            .arg(&tgz)
            .arg("-C")
            .arg(&extract_dir)
            .args(["bin/pdfium.dll"])
            .status()
            .expect("run tar (required to extract pdfium.dll)");
        assert!(status.success(), "tar failed extracting pdfium.dll");

        std::fs::create_dir_all(vendored.parent().unwrap()).unwrap();
        std::fs::copy(extract_dir.join("bin/pdfium.dll"), &vendored)
            .expect("stage pdfium.dll into third_party");
    }

    let actual = sha256_hex(&vendored);
    assert_eq!(
        actual, PDFIUM_SHA256,
        "third_party/pdfium/pdfium.dll hash mismatch — delete it to re-download"
    );
    std::fs::copy(&vendored, &dest).expect("copy pdfium.dll to OUT_DIR");
}
