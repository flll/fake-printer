#!/usr/bin/env python3
"""Post-process captured print jobs for agent-friendly storage (PDF primary, PNG fallback)."""

from __future__ import annotations

import json
import re
import shutil
from datetime import datetime
from pathlib import Path

INSTALL_ROOT = Path(__file__).resolve().parent.parent
INBOX_DIR = INSTALL_ROOT / "inbox"

MIN_TEXT_CHARS = 20
PDF_MAGIC = b"%PDF"


def _read_meta(job_dir: Path) -> dict:
    meta_path = job_dir / "meta.json"
    if not meta_path.exists():
        return {}
    try:
        return json.loads(meta_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {}


def _is_pdf(data: bytes) -> bool:
    return data[:4] == PDF_MAGIC


def _safe_stem(name: str, fallback: str = "job") -> str:
    stem = Path(name).stem if name else fallback
    stem = re.sub(r'[<>:"/\\|?*\x00-\x1f]', "_", stem).strip(" .")
    return stem or fallback


def _inbox_stamp(job_dir: Path) -> str:
    folder = job_dir.name
    m = re.match(r"^(\d{8})T(\d{6})Z", folder)
    if m:
        return f"{m.group(1)}_{m.group(2)[:4]}"
    return datetime.now().strftime("%Y%m%d_%H%M")


def _pdf_text_char_count(pdf_path: Path) -> int:
    import fitz  # PyMuPDF

    doc = fitz.open(pdf_path)
    try:
        chars = 0
        for i in range(doc.page_count):
            chars += len(re.sub(r"\s+", "", doc.load_page(i).get_text("text")))
        return chars
    finally:
        doc.close()


def _list_pngs(job_dir: Path) -> list[Path]:
    return sorted(job_dir.glob("page_*.png"))


def _remove_legacy_text_md(job_dir: Path) -> None:
    legacy = job_dir / "text.md"
    if legacy.exists():
        try:
            legacy.unlink()
        except OSError:
            pass


def _copy_to_inbox(
    job_dir: Path, job_name: str, mode: str, pngs: list[Path]
) -> dict[str, Path]:
    INBOX_DIR.mkdir(parents=True, exist_ok=True)
    stamp = _inbox_stamp(job_dir)
    stem = _safe_stem(job_name)
    out: dict[str, Path] = {}

    if mode == "text":
        pdf_src = job_dir / "document.pdf"
        if pdf_src.exists():
            pdf_dst = INBOX_DIR / f"{stamp}_{stem}.pdf"
            shutil.copy2(pdf_src, pdf_dst)
            out["pdf"] = pdf_dst
    else:
        for i, png in enumerate(pngs, 1):
            png_dst = INBOX_DIR / f"{stamp}_{stem}_p{i:03d}.png"
            shutil.copy2(png, png_dst)
            out[f"png{i:03d}"] = png_dst
    return out


def process_job(job_dir: Path, inbox_dir: Path | None = None) -> dict:
    """Convert a spooled job to agent-friendly artifacts.

    Returns: {mode, pages, chars, name, job_dir, inbox, status}
    """
    global INBOX_DIR
    job_dir = job_dir.resolve()
    if inbox_dir is not None:
        INBOX_DIR = inbox_dir.resolve()

    _remove_legacy_text_md(job_dir)

    meta = _read_meta(job_dir)
    job_name = meta.get("job-name") or meta.get("job_name") or job_dir.name
    pages = len(_list_pngs(job_dir)) or 1

    doc_bin = job_dir / "document.bin"
    pngs = _list_pngs(job_dir)
    pdf_path = job_dir / "document.pdf"

    def finish(mode: str, chars: int = 0, status: str = "") -> dict:
        inbox = _copy_to_inbox(job_dir, job_name, mode, pngs)
        return {
            "mode": mode,
            "pages": pages,
            "chars": chars,
            "name": job_name,
            "job_dir": str(job_dir),
            "inbox": {k: str(v) for k, v in inbox.items()},
            "status": status,
        }

    # ── not PDF (raster etc.) ────────────────────────────────────────────────
    if not doc_bin.exists():
        refs = ", ".join(p.name for p in pngs) if pngs else "(なし)"
        status = f"テキスト抽出不可 → PNG にフォールバックしました（{refs} を参照）"
        return finish("png", status=status)

    raw = doc_bin.read_bytes()
    if not _is_pdf(raw):
        refs = ", ".join(p.name for p in pngs) if pngs else "(なし)"
        status = f"テキスト抽出不可 → PNG にフォールバックしました（{refs} を参照）"
        return finish("png", status=status)

    # ── PDF: check text layer ────────────────────────────────────────────────
    if not pdf_path.exists():
        doc_bin.rename(pdf_path)

    try:
        chars = _pdf_text_char_count(pdf_path)
    except Exception:
        chars = 0

    if chars >= MIN_TEXT_CHARS:
        for png in pngs:
            try:
                png.unlink()
            except OSError:
                pass
        return finish("text", chars=chars, status="PDF保存（テキスト層あり）")

    # ── image-only PDF → PNG fallback ────────────────────────────────────────
    refs = ", ".join(p.name for p in pngs) if pngs else "(なし)"
    status = f"テキスト抽出不可 → PDF→PNG にフォールバックしました（{refs} を参照）"
    return finish("png", status=status)


if __name__ == "__main__":
    import sys

    if len(sys.argv) < 2:
        print("usage: postprocess.py <job_dir>")
        raise SystemExit(2)
    result = process_job(Path(sys.argv[1]))
    print(json.dumps(result, ensure_ascii=False, indent=2))
