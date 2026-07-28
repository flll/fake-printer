# fake-printer — reference

## Architecture

```
[Client: Win/Mac/iOS]
        | IPP (TCP 8631)
        v
[fake-printer.exe]  — axum HTTP + IPP 1.1 codec
        | pdfium render + postprocess (in-process)
        v
[spool/<job>/document.pdf]  or  [page_*.png fallback]
        |
        v
[inbox/<stamp>_<name>.pdf  or  _pNNN.png]
```

mDNS (UDP 5353) advertises `_ipp._tcp` plus the AirPrint `_universal` subtype
so iPhones can auto-discover the printer. `_ipps` is deliberately not
advertised — the server speaks plain HTTP, and clients that see IPPS attempt
TLS and mark the printer "offline".

## Paths (self-contained in the repo)

| Purpose | Path |
|---------|------|
| Repository root | clone destination (= default install root) |
| Launch | `start-fake-printer.bat` → `fake-printer.exe` |
| Engine source | `rust/` |
| .env | `<install root>\.env` |
| spool | `spool/` |
| inbox | `inbox/` |
| Logs | `logs/server.log` |
| pdfium (runtime) | `%LOCALAPPDATA%\fake-printer\pdfium\<hash>\pdfium.dll` |

## Post-processing (rust/src/postprocess.rs)

Runs in-process right after each job renders:

1. Check whether the payload starts with `%PDF`
2. Extract text via pdfium — TEXT requires ≥ 20 non-whitespace chars **and**
   a readable-character ratio ≥ 70 % (`looks_readable`, guards against
   missing-ToUnicode garbage)
3. **TEXT**: keep `document.pdf`, delete PNGs, copy `.pdf` to inbox only
4. **Fallback**: keep PNGs, copy `_p001.png` … to inbox

## Dashboard

`start-fake-printer.bat` → `fake-printer.exe` — a single process running the
IPP server, mDNS advertiser, and ratatui dashboard (event channels, no log
tailing). Close the window / `q` / Ctrl+C to stop. `--headless` skips the UI.

## Key .env variables

| Variable | Default | Description |
|----------|---------|-------------|
| `IPP_LISTEN_HOST` | `0.0.0.0` | Bind address |
| `IPP_LISTEN_PORT` | `8631` | IPP port |
| `IPP_SPOOL_DIR` | `spool/` | Job storage |
| `IPP_RENDER_DPI` | `200` | Render DPI |
| `PRINTER_NAME` | `Fake Printer` | mDNS instance name (quote: contains space) |
| `POST_ENDPOINT` | (empty) | Empty = store-only (no outbound POST) |
| `IPP_SHARED_TOKEN` | (empty) | If set, clients must send `X-IPP-Token` |

Full list and known deviations from the retired Python stack:
[../rust/README.md](../rust/README.md).

**Quoting rule**: values containing spaces must be quoted (`KEY="a b"`).
An unquoted space aborts .env parsing at that line and the rest of the file
is ignored (a warning is printed at startup).

## Troubleshooting

### doctor reports `MISSING: fake-printer.exe`

Install Rust (https://rustup.rs), then:

```powershell
pwsh scripts/setup.ps1
```

### iPhone does not show the printer

1. Run `pwsh scripts/doctor.ps1` — check firewall and exe
2. Confirm same Wi‑Fi / subnet (guest networks often block mDNS)
3. Run `pwsh scripts/open-firewall.ps1` as Administrator

### Print job produces no files

1. Check `spool/` and `logs/server.log`
2. Run `pwsh scripts/stop.ps1`, then restart `start-fake-printer.bat`

### Port 8631 already in use

```powershell
Get-NetTCPConnection -LocalPort 8631 -ErrorAction SilentlyContinue
pwsh scripts/stop.ps1
```

### Settings in .env seem ignored

Almost always the quoting rule above — look for the startup warning in
`logs/server.log`.

## License

- Engine `rust/`: AGPL-3.0 ([NOTICE](../NOTICE))
- Helper scripts: MIT ([LICENSE](../LICENSE))
