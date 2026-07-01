# fake-printer — reference

## Architecture

```
[Client: Win/Mac/iOS]
        | IPP (TCP 8631)
        v
[paperlessprinter]
        | PyMuPDF render + postprocess
        v
[spool/<job>/document.pdf]  or  [page_*.png fallback]
        |
        v
[inbox/<stamp>_<name>.pdf  or  _pNNN.png]
```

mDNS (UDP 5353) advertises `_ipp._tcp` plus an AirPrint subtype so iPhones can auto-discover the printer.

## Paths (self-contained in the repo)

| Purpose | Path |
|---------|------|
| Repository root | clone destination (= default install root) |
| Launch | `start-fake-printer.bat` |
| Dashboard | `scripts/dashboard.py` |
| paperlessprinter | `paperlessprinter/` (cloned by setup) |
| venv | `.venv/` |
| spool | `spool/` |
| inbox | `inbox/` |
| Logs | `logs/server.log` / `logs/mdns.log` |

## Post-processing (postprocess.py)

`dashboard.py` runs this in the background when a job completes:

1. Check whether `document.bin` starts with `%PDF`
2. Use PyMuPDF to detect a text layer (success if ≥ 20 non-whitespace characters)
3. **Success**: keep `document.pdf`, delete PNGs, copy `.pdf` to inbox only
4. **Fallback**: keep PNGs, copy `_p001.png` … to inbox

Manual run: `.venv\Scripts\python.exe scripts\postprocess.py <job_dir>`

## Dashboard

`start-fake-printer.bat` → `scripts/dashboard.py` spawns **server.py** and **advertise-ipp-mdns.py** as children, bound to a Windows Job Object (`KILL_ON_JOB_CLOSE`).

- rich bordered panels (IPP URL / Spool / Inbox / mDNS / Server / Uptime / Captured / progress)
- Raw logs go to `logs/` only (not the console)

## Key .env variables

| Variable | Default | Description |
|----------|---------|-------------|
| `IPP_LISTEN_HOST` | `0.0.0.0` | Bind address |
| `IPP_LISTEN_PORT` | `8631` | IPP port |
| `IPP_SPOOL_DIR` | `spool/` | Job storage |
| `IPP_RENDER_DPI` | `200` | Render DPI |
| `POST_ENDPOINT` | (empty) | Empty = store-only (no outbound POST) |
| `IPP_SHARED_TOKEN` | (empty) | If set, clients must send `X-IPP-Token` |

## Troubleshooting

### doctor reports `MISSING: python`

```powershell
winget install Python.Python.3.12 --accept-package-agreements --accept-source-agreements
```

Open a new terminal and run `pwsh scripts/setup.ps1` again.

### iPhone does not show the printer

1. Run `pwsh scripts/doctor.ps1` — check firewall and venv
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

## License

- paperlessprinter: AGPL-3.0 ([NOTICE](../NOTICE))
- Wrapper in this repo: MIT ([LICENSE](../LICENSE))
