# fake-printer

LAN 上に仮想 IPP / AirPrint プリンターを立て、印刷ジョブをローカルに保存する Windows 向けツールです。

- **エンジン**: [paperlessprinter](https://github.com/paperlesspaper/paperlessprinter)（IPP サーバー、セットアップ時に clone）
- **UI**: rich の枠線ダッシュボード（窓を閉じると停止）
- **成果物**: `inbox/` に PDF（テキスト層あり）または PNG フォールバック

## 要件

- Windows 10 以降
- [PowerShell 7+](https://github.com/PowerShell/PowerShell)
- Python 3.12+（`setup.ps1` が winget で導入を試みます）
- Git
- Ghostscript（PostScript ジョブのみ・任意）

## クイックスタート

```powershell
git clone https://github.com/flll/fake-printer.git
cd fake-printer
pwsh scripts/setup.ps1
.\start-fake-printer.bat
```

`start-fake-printer.bat` のウィンドウを閉じるとサーバーと mDNS が停止します。

## クライアント接続

| 端末 | 手順 |
|------|------|
| iPhone / iPad / Mac | 同一 LAN で **Fake Printer** が AirPrint 一覧に表示 |
| Windows | 設定 → プリンター → 手動追加 → IPP → `ipp://<ホストIP>:8631/ipp/print` |

ホスト IP はダッシュボード左パネルに表示されます。

## エージェント連携

ジョブ完了後、`postprocess.py` が自動で `inbox/` にコピーします。

| 条件 | inbox のファイル |
|------|------------------|
| テキスト付き PDF | `<YYYYMMDD_HHMM>_<jobname>.pdf` |
| 画像のみ / フォールバック | `<YYYYMMDD_HHMM>_<jobname>_p001.png` … |

生ログは `logs/server.log` と `logs/mdns.log` です。

## 運用コマンド

```powershell
pwsh scripts/doctor.ps1          # 診断
pwsh scripts/start.ps1           # bat を別窓で起動
pwsh scripts/stop.ps1            # 残骸プロセスを停止
pwsh scripts/setup.ps1 -Force    # paperlessprinter を再 clone
```

### ファイアウォール

管理者 PowerShell で:

```powershell
pwsh scripts/open-firewall.ps1
```

TCP **8631**（IPP）と UDP **5353**（mDNS）を許可します。

### カスタムインストール先

デフォルトは clone したリポジトリのルートです。別フォルダに置きたい場合:

```powershell
pwsh scripts/setup.ps1 -InstallRoot 'D:\my-fake-printer'
```

## リポジトリ構成

```
fake-printer/
  start-fake-printer.bat   # 起動
  scripts/                 # 正本スクリプト
  config/env.example       # .env テンプレ
  .venv/                   # setup で作成（gitignore）
  paperlessprinter/        # setup で clone（gitignore）
  spool/ inbox/ logs/      # ランタイム（gitignore）
```

詳細は [docs/reference.md](docs/reference.md) を参照してください。

## ライセンス

- 本リポジトリのラッパーコード: **MIT**（[LICENSE](LICENSE)）
- paperlessprinter: **AGPL-3.0**（[NOTICE](NOTICE)）

LAN / プライベートネットワーク内での利用を想定しています。インターネット公開はしないでください。
