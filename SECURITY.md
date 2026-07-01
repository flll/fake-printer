# Security policy — fake-printer

## Scope

fake-printer is a **local LAN utility**. It is not a multi-tenant cloud service.

## Design choices (intentional)

### No SSO / OAuth / enterprise identity

This project does **not** implement and will not add:

- Single Sign-On (SSO)
- OAuth / OpenID Connect
- Azure AD, Google Workspace, or similar identity providers

Printing is authenticated only by **network reachability** on a trusted LAN (and optionally a shared IPP token — see below).

### LAN-only deployment

- Bind: `0.0.0.0:8631` on the host — reachable from the local network
- **Do not** port-forward 8631 to the public internet
- Tailscale or other private overlays are acceptable; public exposure is not

### Optional LAN token (not SSO)

`IPP_SHARED_TOKEN` in `paperlessprinter/.env` (generated from `config/env.example`) requires clients to send `X-IPP-Token`. This is a **shared secret for LAN hardening**, not user identity or SSO.

### Store-only by default

`POST_ENDPOINT` is empty by default. Print jobs are written to local `spool/` and `inbox/` only — no outbound HTTP POST with files or credentials.

## Sensitive data

- `spool/` and `inbox/` may contain **confidential documents** from print jobs
- These paths are **gitignored** — never commit them
- `install.json` and `paperlessprinter/.env` are gitignored (machine-specific)

## Dependencies

Setup clones [paperlessprinter](https://github.com/paperlesspaper/paperlessprinter) (AGPL-3.0). Review that project's security model separately.

## Reporting vulnerabilities

Open a private security advisory on GitHub or contact the repository owner. Do not open public issues for undisclosed vulnerabilities.

## Firewall

For LAN clients to connect, allow inbound:

- TCP **8631** (IPP)
- UDP **5353** (mDNS)

Run `pwsh scripts/open-firewall.ps1` as Administrator on Windows.
