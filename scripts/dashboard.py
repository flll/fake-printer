#!/usr/bin/env python3
"""Foreground console dashboard for the fake IPP printer.

Spawns server.py + advertise-ipp-mdns.py (output redirected to log files),
binds them to a Windows Job Object so closing this window kills everything,
then renders a boxed status UI (rich) with live capture progress.
"""

from __future__ import annotations

import json
import os
import re
import socket
import subprocess
import sys
import threading
import time
from datetime import datetime
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
INSTALL_ROOT = SCRIPTS_DIR.parent
if str(SCRIPTS_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPTS_DIR))

REPO_DIR = INSTALL_ROOT / "paperlessprinter"
VENV_PY = INSTALL_ROOT / ".venv" / "Scripts" / "python.exe"
LOGS_DIR = INSTALL_ROOT / "logs"
INBOX_DIR = INSTALL_ROOT / "inbox"
MANIFEST = INSTALL_ROOT / "install.json"

SERVER_LOG = LOGS_DIR / "server.log"
MDNS_LOG = LOGS_DIR / "mdns.log"

PRINTER_NAME = "Fake Printer"
MAX_RECENT = 8

# ── log line patterns (ignore Get-Printer-Attributes / access noise) ─────────
RE_PRINT_JOB = re.compile(
    r"op=Print-Job .*?job_name=(?P<name>.*?) document_format=(?P<fmt>\S*) "
    r"document_bytes=(?P<bytes>\d+)"
)
RE_SPOOLING = re.compile(r"Spooling job (?P<job>\w+) to (?P<dir>.+)$")
RE_DONE = re.compile(
    r"PNG generation succeeded: job_id=(?P<job>\w+) total_pages=(?P<pages>\d+)"
)


def read_manifest() -> dict:
    try:
        return json.loads(MANIFEST.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {}


def python_exe() -> str:
    return str(VENV_PY) if VENV_PY.exists() else sys.executable


# ── Windows Job Object: children die when this process exits ─────────────────
def make_kill_on_close_job():
    if os.name != "nt":
        return None
    try:
        import ctypes
        from ctypes import wintypes

        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)

        class JOBOBJECT_BASIC_LIMIT_INFORMATION(ctypes.Structure):
            _fields_ = [
                ("PerProcessUserTimeLimit", wintypes.LARGE_INTEGER),
                ("PerJobUserTimeLimit", wintypes.LARGE_INTEGER),
                ("LimitFlags", wintypes.DWORD),
                ("MinimumWorkingSetSize", ctypes.c_size_t),
                ("MaximumWorkingSetSize", ctypes.c_size_t),
                ("ActiveProcessLimit", wintypes.DWORD),
                ("Affinity", ctypes.POINTER(wintypes.ULONG)),
                ("PriorityClass", wintypes.DWORD),
                ("SchedulingClass", wintypes.DWORD),
            ]

        class IO_COUNTERS(ctypes.Structure):
            _fields_ = [
                ("ReadOperationCount", ctypes.c_ulonglong),
                ("WriteOperationCount", ctypes.c_ulonglong),
                ("OtherOperationCount", ctypes.c_ulonglong),
                ("ReadTransferCount", ctypes.c_ulonglong),
                ("WriteTransferCount", ctypes.c_ulonglong),
                ("OtherTransferCount", ctypes.c_ulonglong),
            ]

        class JOBOBJECT_EXTENDED_LIMIT_INFORMATION(ctypes.Structure):
            _fields_ = [
                ("BasicLimitInformation", JOBOBJECT_BASIC_LIMIT_INFORMATION),
                ("IoInfo", IO_COUNTERS),
                ("ProcessMemoryLimit", ctypes.c_size_t),
                ("JobMemoryLimit", ctypes.c_size_t),
                ("PeakProcessMemoryUsed", ctypes.c_size_t),
                ("PeakJobMemoryUsed", ctypes.c_size_t),
            ]

        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x2000
        JobObjectExtendedLimitInformation = 9

        job = kernel32.CreateJobObjectW(None, None)
        if not job:
            return None
        info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION()
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        kernel32.SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            ctypes.byref(info),
            ctypes.sizeof(info),
        )
        return (kernel32, job)
    except Exception:
        return None


def assign_to_job(job, proc: subprocess.Popen) -> None:
    if not job:
        return
    try:
        import ctypes

        kernel32, handle = job
        PROCESS_SET_QUOTA = 0x0100
        PROCESS_TERMINATE = 0x0001
        hproc = kernel32.OpenProcess(
            PROCESS_SET_QUOTA | PROCESS_TERMINATE, False, proc.pid
        )
        if hproc:
            kernel32.AssignProcessToJobObject(handle, hproc)
            kernel32.CloseHandle(hproc)
    except Exception:
        pass


def spawn_children(job):
    LOGS_DIR.mkdir(parents=True, exist_ok=True)
    py = python_exe()
    creationflags = 0
    if os.name == "nt":
        creationflags = subprocess.CREATE_NO_WINDOW  # type: ignore[attr-defined]

    server_out = open(SERVER_LOG, "w", encoding="utf-8", buffering=1)
    mdns_out = open(MDNS_LOG, "w", encoding="utf-8", buffering=1)

    # -u / PYTHONUNBUFFERED so child logs hit the file immediately (live tail).
    child_env = dict(os.environ, PYTHONUNBUFFERED="1")

    server = subprocess.Popen(
        [py, "-u", "server.py"],
        cwd=str(REPO_DIR),
        stdout=server_out,
        stderr=subprocess.STDOUT,
        env=child_env,
        creationflags=creationflags,
    )
    assign_to_job(job, server)

    mdns_script = SCRIPTS_DIR / "advertise-ipp-mdns.py"
    mdns = subprocess.Popen(
        [py, "-u", str(mdns_script)],
        cwd=str(SCRIPTS_DIR),
        stdout=mdns_out,
        stderr=subprocess.STDOUT,
        env=child_env,
        creationflags=creationflags,
    )
    assign_to_job(job, mdns)
    return server, mdns, (server_out, mdns_out)


def port_is_up(port: int) -> bool:
    try:
        with socket.create_connection(("127.0.0.1", port), timeout=0.4):
            return True
    except OSError:
        return False


# ── state built from tailing the server log ──────────────────────────────────
class State:
    def __init__(self, spool_dir: Path):
        self.spool_dir = spool_dir
        self.jobs_captured = 0
        self.pages_total = 0
        self.recent: list[dict] = []  # newest first; mode filled by postprocess
        self.current: dict | None = None
        self._pending_name: str | None = None
        self._processed_dirs: set[str] = set()

    def _count_pngs(self, job_dir: Path) -> int:
        try:
            return len(list(job_dir.glob("page_*.png")))
        except OSError:
            return 0

    def on_print_job(self, name: str):
        self._pending_name = name or "(no name)"
        self.current = {
            "name": self._pending_name,
            "job": None,
            "dir": None,
            "pages_seen": 0,
            "started": time.time(),
        }

    def on_spooling(self, job: str, spool_dir: str):
        if self.current is not None:
            self.current["job"] = job
            self.current["dir"] = Path(spool_dir)

    def _run_postprocess(self, job_dir: Path, entry: dict) -> None:
        try:
            import postprocess

            result = postprocess.process_job(job_dir, INBOX_DIR)
            entry["mode"] = result.get("mode", "png")
        except Exception:
            entry["mode"] = "png"

    def on_done(self, job: str, pages: int):
        name = self.current["name"] if self.current else "(no name)"
        job_dir = self.current.get("dir") if self.current else None
        if job_dir is None and job:
            matches = sorted(self.spool_dir.glob(f"*_{job}"))
            if matches:
                job_dir = matches[-1]

        self.jobs_captured += 1
        self.pages_total += pages
        entry = {
            "time": datetime.now().strftime("%H:%M:%S"),
            "name": name,
            "pages": pages,
            "mode": "...",
        }
        self.recent.insert(0, entry)
        del self.recent[MAX_RECENT:]
        self.current = None

        if job_dir is not None:
            key = str(job_dir)
            if key not in self._processed_dirs:
                self._processed_dirs.add(key)
                threading.Thread(
                    target=self._run_postprocess,
                    args=(Path(job_dir), entry),
                    daemon=True,
                ).start()
        else:
            entry["mode"] = "png"

    def refresh_progress(self):
        if self.current and self.current.get("dir"):
            self.current["pages_seen"] = self._count_pngs(self.current["dir"])


def follow_log(state: State, path: Path):
    """Generator-free tail: called periodically to drain new lines."""
    pos = getattr(follow_log, "_pos", {})
    key = str(path)
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as fh:
            fh.seek(pos.get(key, 0))
            for line in fh:
                m = RE_PRINT_JOB.search(line)
                if m:
                    state.on_print_job(m.group("name").strip())
                    continue
                m = RE_SPOOLING.search(line)
                if m:
                    state.on_spooling(m.group("job"), m.group("dir").strip())
                    continue
                m = RE_DONE.search(line)
                if m:
                    state.on_done(m.group("job"), int(m.group("pages")))
                    continue
            pos[key] = fh.tell()
    except OSError:
        pass
    follow_log._pos = pos  # type: ignore[attr-defined]


def fmt_uptime(seconds: float) -> str:
    s = int(seconds)
    return f"{s // 3600:02d}:{(s % 3600) // 60:02d}:{s % 60:02d}"


# ── rich rendering (with plain fallback) ─────────────────────────────────────
def run_rich(state: State, manifest: dict, started_at: float, procs) -> None:
    from rich.align import Align
    from rich.console import Console, Group
    from rich.layout import Layout
    from rich.live import Live
    from rich.panel import Panel
    from rich.progress import BarColumn, Progress, TextColumn
    from rich.table import Table
    from rich.text import Text

    console = Console()
    ipp_url = manifest.get("ipp_url", "ipp://127.0.0.1:8631/ipp/print")
    port = int(manifest.get("listen_port", 8631))
    spool = manifest.get("spool_dir", str(state.spool_dir))
    inbox = str(INBOX_DIR)
    accent = "bright_red"

    def badge(ok: bool, up="UP", down="DOWN") -> Text:
        return Text(f"● {up}", style="bold green") if ok else Text(
            f"● {down}", style="bold red"
        )

    def build_main() -> Panel:
        up = port_is_up(port)
        mdns_ok = MDNS_LOG.exists() and "registered" in _safe_tail(MDNS_LOG)

        info = Table.grid(padding=(0, 2))
        info.add_column(justify="right", style="dim", no_wrap=True)
        info.add_column(style="white")
        info.add_row("IPP", Text(ipp_url, style="bold cyan"))
        info.add_row("Spool", spool)
        info.add_row("Inbox", inbox)
        info.add_row("mDNS", Text.assemble(f"{PRINTER_NAME}  ", badge(mdns_ok, "advertised", "off")))
        info.add_row(
            "Server",
            Text.assemble(f":{port}  ", badge(up), f"    Uptime {fmt_uptime(time.time() - started_at)}"),
        )

        counts = Text.assemble(
            ("Captured  ", "dim"),
            (f"{state.jobs_captured}", "bold green"),
            (" jobs    ", "dim"),
            (f"{state.pages_total}", "bold green"),
            (" pages", "dim"),
        )

        if state.current:
            cur = state.current
            seen = cur.get("pages_seen", 0)
            prog = Progress(
                TextColumn("[yellow]now[/]  rendering {task.description}"),
                BarColumn(bar_width=24, complete_style="yellow"),
                TextColumn("p{task.completed}"),
                expand=False,
            )
            total = max(seen, 1)
            prog.add_task(cur["name"], total=total, completed=seen)
            current_block = prog
        else:
            current_block = Text("idle — waiting for a print job", style="dim italic")

        body = Group(info, Text(""), counts, Text(""), current_block)
        return Panel(
            body,
            title=Text("FAKE PRINTER", style=f"bold {accent}"),
            subtitle=Text("close this window to STOP", style="dim"),
            border_style=accent,
            padding=(1, 2),
        )

    def build_connect() -> Panel:
        t = Table.grid(padding=(0, 1))
        t.add_column()
        t.add_row(Text("iPhone / Mac", style="bold"))
        t.add_row(Text("  Shows in AirPrint list", style="dim"))
        t.add_row("")
        t.add_row(Text("Windows", style="bold"))
        t.add_row(Text("  Add printer manually → IPP", style="dim"))
        t.add_row(Text("  URL in left panel", style="dim"))
        return Panel(t, title="Connect", border_style="grey50", padding=(1, 1))

    def build_recent() -> Panel:
        if not state.recent:
            body: object = Text("no jobs yet", style="dim italic")
        else:
            t = Table.grid(padding=(0, 1))
            t.add_column(style="dim", no_wrap=True)
            t.add_column(no_wrap=True)
            t.add_column(justify="right", no_wrap=True)
            for r in state.recent:
                mode = r.get("mode", "?")
                if mode == "text":
                    badge_txt = Text("TEXT", style="bold green")
                elif mode == "png":
                    badge_txt = Text("PNG", style="bold yellow")
                else:
                    badge_txt = Text("...", style="dim")
                t.add_row(r["time"], _ellipsis(r["name"], 16), badge_txt, f"{r['pages']}p")
            body = t
        return Panel(body, title="Recent activity", border_style="grey50", padding=(1, 1))

    def build() -> Layout:
        layout = Layout()
        layout.split_column(
            Layout(name="top", ratio=1),
            Layout(name="foot", size=1),
        )
        layout["top"].split_row(
            Layout(build_main(), name="left", ratio=2),
            Layout(name="right", ratio=1),
        )
        layout["top"]["right"].split_column(
            Layout(build_connect(), name="connect"),
            Layout(build_recent(), name="recent"),
        )
        layout["foot"].update(
            Align.center(Text(f"inbox\\  ·  logs\\server.log   ·   {ipp_url}", style="dim"))
        )
        return layout

    with Live(build(), console=console, screen=True, refresh_per_second=4) as live:
        while True:
            server, mdns = procs
            if server.poll() is not None:
                break
            follow_log(state, SERVER_LOG)
            state.refresh_progress()
            live.update(build())
            time.sleep(1.0)


def run_plain(state: State, manifest: dict, started_at: float, procs) -> None:
    ipp_url = manifest.get("ipp_url", "ipp://127.0.0.1:8631/ipp/print")
    port = int(manifest.get("listen_port", 8631))
    while True:
        server, _ = procs
        if server.poll() is not None:
            break
        follow_log(state, SERVER_LOG)
        state.refresh_progress()
        os.system("cls" if os.name == "nt" else "clear")
        up = "UP" if port_is_up(port) else "DOWN"
        print("=== FAKE PRINTER ===")
        print(f" IPP    {ipp_url}")
        print(f" Server :{port} {up}   Uptime {fmt_uptime(time.time() - started_at)}")
        print(f" Captured {state.jobs_captured} jobs  {state.pages_total} pages")
        if state.current:
            print(f" now> rendering {state.current['name']}  p{state.current.get('pages_seen', 0)}")
        print(" Recent:")
        for r in state.recent:
            mode = r.get("mode", "?")
            print(f"   {r['time']}  [{mode}]  {r['name']}  {r['pages']}p")
        print(" Close this window to STOP")
        time.sleep(1.5)


def _safe_tail(path: Path, n: int = 4000) -> str:
    try:
        data = path.read_text(encoding="utf-8", errors="replace")
        return data[-n:]
    except OSError:
        return ""


def _ellipsis(s: str, width: int) -> str:
    return s if len(s) <= width else s[: width - 1] + "…"


def main() -> int:
    if not REPO_DIR.exists():
        print("[ERROR] Not set up. Run: pwsh scripts/setup.ps1")
        input("Press Enter to exit...")
        return 1

    manifest = read_manifest()
    spool_dir = Path(manifest.get("spool_dir", str(INSTALL_ROOT / "spool")))
    INBOX_DIR.mkdir(parents=True, exist_ok=True)

    job = make_kill_on_close_job()
    server, mdns, files = spawn_children(job)
    started_at = time.time()
    state = State(spool_dir)

    try:
        try:
            import rich  # noqa: F401

            run_rich(state, manifest, started_at, (server, mdns))
        except ImportError:
            run_plain(state, manifest, started_at, (server, mdns))
    except KeyboardInterrupt:
        pass
    finally:
        for p in (server, mdns):
            try:
                p.terminate()
            except Exception:
                pass
        for f in files:
            try:
                f.close()
            except Exception:
                pass
    return 0


if __name__ == "__main__":
    sys.exit(main())
