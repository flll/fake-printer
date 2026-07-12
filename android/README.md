# Fake Printer — Android PrintService アプリ

Android の印刷ダイアログに「Fake Printer」としてプリンタを追加する印刷サービス。
選んで印刷すると、ドキュメント（OS が PDF にレンダリング済み）を IPP Print-Job で
fake-printer サーバへ直接送信し、数秒でジョブ完了になる。

標準の「デフォルト印刷サービス」(Mopria) は printer-state のポーリングで完了判定するため
1ジョブ約47秒かかる問題があった。本アプリはジョブ完了を自前で確定するため構造的に速い。

## ビルド

要件: JDK 17+、Android SDK（`local.properties` の `sdk.dir` を自機に合わせる）

```
cd android
gradlew.bat assembleDebug      # APK: app/build/outputs/apk/debug/app-debug.apk
gradlew.bat test               # fake-printer が 127.0.0.1:8631 で稼働中なら実サーバ送信テストも実行
```

## タブレットへの導入

1. `app-debug.apk` をタブレットへ転送（OneDrive / ブラウザ DL など）してインストール
   （「提供元不明のアプリ」の許可が必要）
2. **設定 > 接続済みのデバイス > 印刷**（機種により 設定 > 印刷）で
   「Fake Printer」サービスを **ON** にする
3. 必要ならアプリ「Fake Printer」を開き、サーバ IP / ポート / パスを変更
   （初期値 `192.168.30.11:8631` `/ipp/print`）
4. 任意のアプリで 印刷 → プリンタ一覧から「Fake Printer」を選択

## 構成

| ファイル | 役割 |
|---|---|
| `FakePrintService.kt` | ジョブ受信 → PDF 読み出し → IPP 送信 → complete/fail |
| `FakeDiscoverySession.kt` | プリンタ1台を静的に公開（A4/Letter, 200dpi, カラー） |
| `IppClient.kt` | IPP 2.0 Print-Job クライアント（純 JVM、単体テスト可能） |
| `SettingsActivity.kt` | 送信先設定（SharedPreferences） |

- 署名は debug 署名。ビルドマシンが変わると署名が変わり上書き更新できない
  （その場合は一度アンインストールしてから入れ直す）
- 平文 HTTP（LAN 内前提）のため `usesCleartextTraffic="true"`
