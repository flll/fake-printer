# fake-printer — 参照

## アーキテクチャ

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

mDNS（UDP 5353）で `_ipp._tcp` + AirPrint サブタイプを広告 → iPhone が自動発見。

## パス一覧（リポジトリ内完結）

| 用途 | パス |
|------|------|
| リポジトリルート | clone 先（= 既定の install root） |
| 起動 | `start-fake-printer.bat` |
| ダッシュボード | `scripts/dashboard.py` |
| paperlessprinter | `paperlessprinter/`（setup で clone） |
| venv | `.venv/` |
| spool | `spool/` |
| inbox | `inbox/` |
| ログ | `logs/server.log` / `logs/mdns.log` |

## 後処理（postprocess.py）

ジョブ完了時に `dashboard.py` がバックグラウンドで実行:

1. `document.bin` が PDF か判定（先頭 `%PDF`）
2. PyMuPDF でテキスト層の有無を判定（20 文字以上で成功）
3. **成功**: `document.pdf`、PNG 削除、inbox に `.pdf` のみ
4. **フォールバック**: PNG 保持、inbox に `_p001.png` … をコピー

手動実行: `.venv\Scripts\python.exe scripts\postprocess.py <job_dir>`

## ダッシュボード

`start-fake-printer.bat` → `scripts/dashboard.py` が **server.py + advertise-ipp-mdns.py を子プロセスとして起動**し、Windows Job Object（`KILL_ON_JOB_CLOSE`）に束ねる。

- rich の枠線パネル（IPP URL / Spool / Inbox / mDNS / Server / Uptime / Captured / 進捗）
- 生ログは `logs/` のみ（コンソールには出さない）

## 主要 .env 変数

| 変数 | 既定 | 説明 |
|------|------|------|
| `IPP_LISTEN_HOST` | `0.0.0.0` | バインドアドレス |
| `IPP_LISTEN_PORT` | `8631` | IPP ポート |
| `IPP_SPOOL_DIR` | `spool/` | ジョブ保存先 |
| `IPP_RENDER_DPI` | `200` | レンダリング解像度 |
| `POST_ENDPOINT` | （空） | 空 = store-only |
| `IPP_SHARED_TOKEN` | （空） | 設定時は `X-IPP-Token` 必須 |

## トラブルシュート

### doctor が `MISSING: python`

```powershell
winget install Python.Python.3.12 --accept-package-agreements --accept-source-agreements
```

新しいターミナルで `pwsh scripts/setup.ps1` を再実行。

### iPhone にプリンターが出ない

1. `pwsh scripts/doctor.ps1` でファイアウォールと venv を確認
2. 同一 Wi‑Fi / 同一サブネットか確認
3. 管理者で `pwsh scripts/open-firewall.ps1`

### 印刷してもファイルが出ない

1. `spool/` と `logs/server.log` を確認
2. `pwsh scripts/stop.ps1` 後に `start-fake-printer.bat` を再起動

### ポート 8631 が使用中

```powershell
Get-NetTCPConnection -LocalPort 8631 -ErrorAction SilentlyContinue
pwsh scripts/stop.ps1
```

## ライセンス

- paperlessprinter: AGPL-3.0（[NOTICE](../NOTICE)）
- 本リポジトリのラッパー: MIT（[LICENSE](../LICENSE)）
