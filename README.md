# fake-printer

**Virtual IPP / AirPrint printer for Windows** — capture print jobs from LAN clients into a local **agent-friendly inbox** (PDF with text layer, or PNG fallback).

Keywords: virtual printer, IPP, AirPrint, mDNS, print capture, agent inbox, Windows.

- **Engine**: [paperlessprinter](https://github.com/paperlesspaper/paperlessprinter) (cloned at setup, AGPL-3.0)
- **UI**: rich bordered dashboard (close the window to stop)
- **Output**: `inbox/` — PDF when text is extractable, otherwise per-page PNG

For AI coding agents, see [AGENTS.md](AGENTS.md). Security model: [SECURITY.md](SECURITY.md).

## Requirements

- Windows 10 or later
- [PowerShell 7+](https://github.com/PowerShell/PowerShell)
- Python 3.12+ (`setup.ps1` tries winget if missing)
- Git
- Ghostscript (optional — PostScript jobs only)

## Quick start

```powershell
git clone https://github.com/flll/fake-printer.git
cd fake-printer
pwsh scripts/setup.ps1
.\start-fake-printer.bat
```

Closing the `start-fake-printer.bat` window stops the server and mDNS advertiser.

## Connect clients

| Client | How |
|--------|-----|
| iPhone / iPad / Mac | Same LAN — **Fake Printer** appears in the AirPrint list |
| Windows | Settings → Printers → Add manually → IPP → `ipp://<host-ip>:8631/ipp/print` |

The host IP is shown in the dashboard left panel.

## Agent integration

After each job, `postprocess.py` copies artifacts into `inbox/`:

| Condition | Inbox file |
|-----------|------------|
| PDF with text layer | `<YYYYMMDD_HHMM>_<jobname>.pdf` |
| Image-only / fallback | `<YYYYMMDD_HHMM>_<jobname>_p001.png` … |

Raw logs: `logs/server.log`, `logs/mdns.log`.

## Operations

```powershell
pwsh scripts/doctor.ps1          # health check
pwsh scripts/start.ps1           # launch bat in a new window
pwsh scripts/stop.ps1            # stop stray processes
pwsh scripts/setup.ps1 -Force    # re-clone paperlessprinter
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
  start-fake-printer.bat   # launch
  scripts/                 # canonical scripts
  config/env.example       # .env template
  .venv/                   # created by setup (gitignored)
  paperlessprinter/        # cloned by setup (gitignored)
  spool/ inbox/ logs/      # runtime (gitignored)
```

See [docs/reference.md](docs/reference.md) for architecture and troubleshooting.

## License

- Wrapper code in this repo: **MIT** ([LICENSE](LICENSE))
- paperlessprinter: **AGPL-3.0** ([NOTICE](NOTICE))

Intended for **LAN / private networks only**. Do not expose to the public internet.
