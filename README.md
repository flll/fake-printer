# fake-printer

**Virtual IPP / AirPrint printer for Windows** — capture print jobs from LAN clients into a local **agent-friendly inbox** (PDF with text layer, or PNG fallback).

Keywords: virtual printer, IPP, AirPrint, mDNS, print capture, agent inbox, Windows, Rust.

- **Engine**: single Rust binary ([`rust/`](rust/), AGPL-3.0) — no Python, no venv, pdfium embedded
- **UI**: ratatui bordered dashboard (close the window to stop); `--headless` for logs only
- **Output**: `inbox/` — PDF when a readable text layer exists, otherwise per-page PNG

For AI coding agents, see [AGENTS.md](AGENTS.md). Security model: [SECURITY.md](SECURITY.md).

## Requirements

- Windows 10 or later (x64)
- [Rust](https://rustup.rs) (build once; the exe is then copy-anywhere)
- [PowerShell 7+](https://github.com/PowerShell/PowerShell) for the helper scripts
- Ghostscript (optional — PostScript jobs only)

## Quick start

```powershell
git clone https://github.com/flll/fake-printer.git
cd fake-printer
pwsh scripts/setup.ps1     # cargo build --release + .env + dirs + firewall
.\start-fake-printer.bat
```

Closing the `start-fake-printer.bat` window stops everything (single process).

Already have a built `fake-printer.exe`? Just copy it to any folder and run it —
`spool/`, `inbox/`, `logs/`, `temp/` are created next to the exe.

## Connect clients

| Client | How |
|--------|-----|
| iPhone / iPad / Mac | Same LAN — **Fake Printer** appears in the AirPrint list |
| Windows | Settings → Printers → Add manually → IPP → `ipp://<host-ip>:8631/ipp/print` |

The host IP is shown in the dashboard left panel.

## Agent integration

After each job, built-in postprocessing copies artifacts into `inbox/`:

| Condition | Inbox file |
|-----------|------------|
| PDF with readable text layer | `<YYYYMMDD_HHMM>_<jobname>.pdf` |
| Image-only / garbled text / fallback | `<YYYYMMDD_HHMM>_<jobname>_p001.png` … |

Raw log: `logs/server.log`.

## Operations

```powershell
pwsh scripts/doctor.ps1          # health check
pwsh scripts/start.ps1           # launch bat in a new window
pwsh scripts/stop.ps1            # stop stray processes
pwsh scripts/setup.ps1 -Force    # rebuild exe + rewrite .env
```

### Firewall

Run as Administrator:

```powershell
pwsh scripts/open-firewall.ps1
```

Allows TCP **8631** (IPP) and UDP **5353** (mDNS).

### Custom install root

Default is the cloned repository root. For another path:

```powershell
pwsh scripts/setup.ps1 -InstallRoot 'D:\my-fake-printer'
```

## Repository layout

```
fake-printer/
  start-fake-printer.bat   # launch (runs fake-printer.exe)
  rust/                    # the engine — single-binary Rust crate
  scripts/                 # PowerShell setup/ops helpers
  config/env.example       # .env template (written to <install root>\.env)
  spool/ inbox/ logs/      # runtime (gitignored)
```

Engine internals, config keys, and known deviations: [rust/README.md](rust/README.md).
Architecture and troubleshooting: [docs/reference.md](docs/reference.md).

## License

- Helper scripts (`scripts/`, bat): **MIT** ([LICENSE](LICENSE))
- Engine (`rust/`): **AGPL-3.0** ([rust/LICENSE](rust/LICENSE)) — port of
  [paperlessprinter](https://github.com/paperlesspaper/paperlessprinter);
  embedded PDFium is BSD-3-Clause. Details: [NOTICE](NOTICE)

Intended for **LAN / private networks only**. Do not expose to the public internet.
