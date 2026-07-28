# fake-printer (Rust)

Single-binary Rust port of fake-printer: a virtual **IPP / AirPrint** printer
for Windows that captures LAN print jobs into an agent-friendly `inbox/`
(PDF with text layer, or per-page PNG fallback).

Replaces the Python stack (venv + paperlessprinter + wrapper scripts) with one
`fake-printer.exe` — no runtime dependencies. pdfium is embedded in the exe and
extracted to `%LOCALAPPDATA%\fake-printer\pdfium\` on first run.

## Build

```powershell
cd rust
cargo build --release   # → target/release/fake-printer.exe (~18 MB)
```

`build.rs` downloads `pdfium.dll` (pinned bblanchon/pdfium-binaries release
`chromium/7881`, SHA-256 verified) using `curl` + `tar`, both shipped with
Windows 10+. Offline builds: set `PDFIUM_DLL_PATH` to a local dll, or place it
at `third_party/pdfium/pdfium.dll`.

## Run

```powershell
.\fake-printer.exe              # ratatui dashboard (close window / q to stop)
.\fake-printer.exe --headless   # logs only (Ctrl+C to stop)
```

Deployment is copy-the-exe: put `fake-printer.exe` in any directory and run
it. `spool/`, `inbox/`, `temp/`, `logs/` are created next to the exe.

## Configuration

Drop-in compatible with the Python deployment: reads `paperlessprinter/.env`
(legacy layout) or `.env` next to the exe, plus `install.json`. Same keys
(`IPP_LISTEN_PORT`, `IPP_SPOOL_DIR`, `IPP_RENDER_DPI`, `POST_ENDPOINT`, …).

New keys in the Rust port:

| Key | Default | Meaning |
|-----|---------|---------|
| `PRINTER_NAME` | `Fake Printer` | mDNS instance name / TXT `ty` |
| `INBOX_DIR` | `<exe dir>\inbox` | overrides the inbox location |

## Architecture

One process, one tokio runtime:

- `server.rs` + `ipp/` — axum HTTP layer and hand-rolled IPP 1.1 codec
  (Print-Job, Validate-Job, Create-Job, Send-Document, Get-Job-Attributes,
  Get-Jobs, Get-Printer-Attributes)
- `render.rs` — PDF→PNG via embedded pdfium; PostScript via Ghostscript when
  installed (`gs` / `ghostscript` / `gswin64c` on PATH)
- `postprocess.rs` — text-layer check (≥20 non-space chars) → inbox PDF,
  otherwise PNG fallback
- `upload.rs` — optional multipart POST of rendered pages (paperlesspaper)
- `mdns.rs` — `_ipp._tcp` + `_universal` subtype advertisement (mdns-sd);
  `_ipps` is deliberately NOT advertised (clients would try TLS and fail)
- `tui.rs` — ratatui dashboard fed by direct event channels (no log tailing)

Client-specific behaviors carried over from production tuning:

- printer-state 4→3 transition with a busy grace window (Android's print
  service polls it to decide a job finished; always-idle costs ~46 s/job)
- job-id/job-uri/job-state in every Print-Job response (Mopria resends
  without it)
- Create-Job override caching for macOS, which drops query params on the
  follow-up Send-Document

Known deviation: `Expect: 100-continue` is answered automatically by hyper
(the Python default suppressed it as a reverse-proxy workaround; irrelevant
on a direct LAN connection). `meta.json` is UTF-8 instead of `\uXXXX`-escaped.

## Verification

```powershell
cd rust
cargo test          # includes local golden tests against real captures when
                    # a deployment spool exists (never committed — privacy)
cargo run --example browse   # watch LAN _ipp._tcp advertisements
```

## License

`rust/` is **AGPL-3.0** (see [LICENSE](LICENSE)): the IPP engine is a port of
[paperlessprinter](https://github.com/paperlesspaper/paperlessprinter)
(AGPL-3.0). The embedded pdfium is BSD-3-Clause
([third_party/pdfium/LICENSE](third_party/pdfium/LICENSE)).
