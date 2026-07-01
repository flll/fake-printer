#!/usr/bin/env python3
"""Advertise paperlessprinter as IPP / AirPrint via mDNS (zeroconf)."""

from __future__ import annotations

import json
import os
import socket
import sys
import time
from pathlib import Path

from zeroconf import ServiceInfo, Zeroconf

INSTALL_ROOT = Path(__file__).resolve().parent.parent
ENV_FILE = INSTALL_ROOT / "paperlessprinter" / ".env"
MANIFEST = INSTALL_ROOT / "install.json"

PRINTER_NAME = "Fake Printer"
SERVICE_TYPES = [
    "_ipp._tcp.local.",
    "_ipps._tcp.local.",
]


def load_port() -> int:
    if MANIFEST.exists():
        try:
            data = json.loads(MANIFEST.read_text(encoding="utf-8"))
            return int(data.get("listen_port", 8631))
        except (json.JSONDecodeError, TypeError, ValueError):
            pass
    if ENV_FILE.exists():
        for line in ENV_FILE.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if line.startswith("IPP_LISTEN_PORT="):
                return int(line.split("=", 1)[1].strip())
    return 8631


def local_ipv4() -> str:
    try:
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
            sock.connect(("8.8.8.8", 80))
            return sock.getsockname()[0]
    except OSError:
        return socket.gethostbyname(socket.gethostname())


def build_txt_records(port: int) -> dict[bytes, bytes]:
    # AirPrint-compatible minimal TXT set
    fields = {
        "txtvers": "1",
        "qtotal": "1",
        "rp": "ipp/print",
        "ty": PRINTER_NAME,
        "product": "(Virtual IPP PNG)",
        "note": "fake-printer — virtual IPP capture",
        "pdl": "application/pdf,image/pwg-raster,image/urf",
        "URF": "W8,SRGB24,CP255,DM1,FN3,IS0-0,MT1-8-11,OB10,PQ4-5,RS300,ST13,V1.4,W8",
        "Color": "T",
        "Duplex": "F",
        "TLS": "1.3",
    }
    return {k.encode("utf-8"): v.encode("utf-8") for k, v in fields.items()}


def main() -> int:
    port = load_port()
    host_ip = local_ipv4()
    hostname = socket.gethostname().split(".")[0]
    server_name = f"{hostname}.local."
    address = socket.inet_aton(host_ip)
    properties = build_txt_records(port)

    zeroconf = Zeroconf()
    infos: list[ServiceInfo] = []

    for service_type in SERVICE_TYPES:
        safe_name = PRINTER_NAME.replace(" ", "-")
        service_name = f"{safe_name}.{service_type}"
        info = ServiceInfo(
            service_type,
            service_name,
            addresses=[address],
            port=port,
            properties=properties,
            server=server_name,
        )
        zeroconf.register_service(info)
        infos.append(info)
        print(f"registered {service_name} on {host_ip}:{port}", flush=True)

    # AirPrint subtype registration
    airprint_type = "_universal._sub._ipp._tcp.local."
    airprint_name = f"{PRINTER_NAME.replace(' ', '-')}.{airprint_type}"
    airprint_info = ServiceInfo(
        airprint_type,
        airprint_name,
        addresses=[address],
        port=port,
        properties=properties,
        server=server_name,
    )
    zeroconf.register_service(airprint_info)
    infos.append(airprint_info)
    print(f"registered AirPrint subtype on {host_ip}:{port}", flush=True)

    try:
        while True:
            time.sleep(60)
    except KeyboardInterrupt:
        pass
    finally:
        for info in infos:
            zeroconf.unregister_service(info)
        zeroconf.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
