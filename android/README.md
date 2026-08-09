# Fake Printer — Android 受信サーバアプリ

**Android 端末をフェイクプリンターにする**アプリ（Windows 版 `server.py` の Android 移植）。
起動すると LAN 上の他の端末（Android / iPhone / Windows / macOS）の印刷画面に
プリンタとして表示され、印刷されたドキュメントを **PDF ファイルとして保存**する。
保存した PDF はアプリ内の一覧から開ける（タップで表示 / 長押しで削除）。

## 仕組み

- `IppServerCore.kt` — IPP 1.1/2.0 サーバ（純 JVM・単体テスト可能）。
  Windows 版で解決した互換性ノウハウを移植済み:
  - Print-Job 応答に job-id / job-uri / job-state / charset を必ず含める（RFC 8011）
  - ジョブ処理中〜終了後 4 秒は printer-state=processing を返す
    （Android Mopria は processing→idle 遷移でジョブ完了を判定する）
  - Expect: 100-continue と chunked 転送に対応
- `PrinterForegroundService.kt` — フォアグラウンドサービス + jmDNS で
  `_ipp._tcp`（+ `_universal` サブタイプ = AirPrint）を広告
- `MainActivity.kt` — 開始/停止・受信 PDF 一覧・設定
- 保存先: `Android/data/jp.flll.fakeprinter/files/inbox/`

## ビルド

要件: JDK 17+、Android SDK（`local.properties` の `sdk.dir` を自機に合わせる）

```
cd android
gradlew.bat assembleDebug   # APK: app/build/outputs/apk/debug/app-debug.apk
gradlew.bat test            # IPP サーバの単体テスト（自己完結・外部サーバ不要）
```

## 使い方

1. APK をインストール（提供元不明アプリの許可が必要）
2. アプリを開く → プリンタが自動起動（通知に ipp:// アドレスが出る）
3. 同じ Wi-Fi の他の端末で印刷 → プリンタ一覧から「Fake Printer (機種名)」を選択
4. 受信した PDF はアプリの一覧に数秒で現れる

- プリンタ名・ポート（既定 8631）は設定画面で変更可（停止→開始で反映）
- 署名は debug 署名。ビルドマシンが変わると上書き更新不可
  （一度アンインストールして入れ直す）
- mDNS が届く同一セグメントの LAN が前提（VLAN 越えは不可）
