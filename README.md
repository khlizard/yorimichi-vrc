# VrcLogger

VRChat のログファイル (`output_log_*.txt`) を解析して SQLite に蓄積し、
「いつ・どのワールドで・誰と会ったか」「どんな URL が読み込まれたか」を検索できる Windows 用デスクトップアプリです。
Tauri (Rust + React) で作られています。

## 主な機能

- **ログ自動取り込み** — VRChat のログフォルダを定期スキャン (既定 1 分間隔) し、新規・追記されたファイルだけを差分取り込み
- **プレイヤー** — 最近会った順に表示。表示名で検索すると、同席したセッション・当時のアバターを表示。表示名の変更履歴も記録
- **セッション** — 訪問したワールド・インスタンスごとに、滞在時間・同席プレイヤー・URL を表示
- **URL / アバター / 通知** — 動画・文字列・画像の URL、アバター名とダウンロードサイズ、招待・フレンド申請・boop などの通知とミュート・ブロック・Vote Kick の履歴
- **生ログ全文検索** — SQLite FTS5 による全文検索。定型ノイズ行は取り込み時に除外し、既定で 30 日より古い行は自動削除
- **スクロールで続きを読み込み** — プレイヤーと生ログは下までスクロールすると次の分を追加。自動取り込み後、続きを読み込んだ一覧は「再取り込み」で更新
- **日付範囲フィルタ** — 全タブ共通で期間を絞り込み
- **常駐と自動起動** — 閉じるボタンでタスクトレイに格納。設定で ON にすると Windows のサインイン時にトレイに格納した状態で起動
- **多重起動防止** — 二重に起動した場合は既存のウィンドウを前面に表示

URL の一覧では Ctrl+C で URL だけをコピーでき、右クリックで行ごとコピーできます。

### 別PCから見る

DB (`vrclog.db`) は Dropbox などで共有された別PCから **アプリで開かないでください**。
SQLite の WAL ファイルが別々に同期されたり、2 台から同時に書き込まれたりして壊れる恐れがあります。

代わりに、メニューの「ファイル → 閲覧用 HTML を書き出す…」(トレイメニューからも可) で、
DB に触れない自己完結の HTML を保存できます。Dropbox 内に保存すれば、別PCではブラウザで開くだけで
プレイヤー検索・セッションの同席者・通知を閲覧できます。書き出した時点のスナップショットなので、最新にするには書き出し直してください。

## 必要なもの

- Windows 10 / 11 (WebView2 ランタイム。Windows 11 は標準搭載)
- 開発・ビルド: [Rust](https://rustup.rs/) (MSVC ツールチェーン)、Node.js、[pnpm](https://pnpm.io/)
- インストーラー作成: [Velopack](https://velopack.io/) の `vpk` (`dotnet tool install -g vpk`)

## 開発

```powershell
pnpm install          # 依存パッケージの取得
pnpm tauri dev        # 開発モードで起動
pnpm test:rust        # テスト (logs/ に実ログを置くと実ログのテストも実行)
pnpm lint:rust        # clippy
```

cargo は直接実行せず、上の `pnpm` スクリプト経由で実行してください。
`scripts/with-target.mjs` がビルド出力先を Dropbox の外 (`%LOCALAPPDATA%\vrclogger-dev\target`) に向け、
exe に埋め込まれるパスからユーザー名を取り除きます。環境変数 `CARGO_TARGET_DIR` があればそちらを使います。

## ビルドと配布

```powershell
pnpm tauri build --no-bundle   # %LOCALAPPDATA%\vrclogger-dev\target\release\vrclogger.exe を作成
pwsh scripts/pack.ps1          # releases\ に Velopack のインストーラー (Setup.exe) を作成
```

`pack.ps1` は `releases\` に同じバージョンがあると止まります。配布のたびに `Cargo.toml` と `src-tauri/tauri.conf.json` のバージョンを上げてください。
インストーラーは `%LOCALAPPDATA%\VrcLogger` にアプリを入れます。

## 設定

設定ファイルは `%APPDATA%\VrcLogger\vrclogger.settings.json` です (初回起動時に作成)。
「設定」タブから編集できます。

| キー | 説明 |
| --- | --- |
| `DbPath` | SQLite データベースのパス (既定 `vrclog.db`)。相対パスは設定ファイルのあるフォルダ基準。`%VAR%` 展開可 |
| `LogFolders` | `output_log_*.txt` を探すフォルダ一覧 (既定は VRChat のログフォルダ)。同名ファイルはサイズが大きい方を採用 |
| `AutoImportIntervalMinutes` | 自動取り込みの間隔 (分)。`0` で無効 |
| `MinimizeToTray` | `true` の場合、閉じるボタンでタスクトレイに常駐 |
| `RawLogRetentionDays` | 生ログの保持日数。`0` で無制限。セッション・プレイヤー・URL・通知は削除されない |
| `RawLogNoisePatterns` | 生ログに保存しない行 (前方一致、先頭 `*` で部分一致) |

自動起動の ON/OFF は設定ファイルではなく、Windows のレジストリ (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`) に保存されます。

パーサの対応項目が増えたときは、設定タブの「全ログを再取り込み」で過去のログも解析し直せます。

## プロジェクト構成

```
crates/core/   # ログ解析・SQLite 取り込み・検索・HTML 書き出し (Tauri 非依存、テストはここ)
src-tauri/     # Tauri アプリ (コマンド、トレイ、自動取り込み、設定)
ui/            # 画面 (React + TypeScript、Vite)
scripts/       # pack.ps1 (Velopack パッケージ作成)、with-target.mjs (cargo の出力先とパスの置き換え)
logs/          # (Git 管理外) 実ログを置くと実ログのテストが実行される
```
