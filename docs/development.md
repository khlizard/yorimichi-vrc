# ビルドと開発

## 必要なもの

- [Rust](https://rustup.rs/) 1.88 以降 (MSVC ツールチェーン)
- Node.js と [pnpm](https://pnpm.io/)
- インストーラーを作る場合
  - [Velopack](https://velopack.io/) の `vpk` (`dotnet tool install -g vpk`)
  - `cargo-about` (`cargo install cargo-about --locked --features cli`)

## コマンド

```powershell
pnpm install          # 依存パッケージの取得
pnpm tauri dev        # 開発モードで起動
pnpm test:rust        # Rust のテスト
pnpm lint:rust        # clippy
pnpm test             # 画面側のテスト (vitest)
pnpm lint             # 画面側の lint (ESLint)
```

cargo は直接実行せず、上の `pnpm` スクリプトを使ってください。`scripts/with-target.mjs` が次の 2 つを行います。

- ビルド出力先を、リポジトリの外 (`%LOCALAPPDATA%\yorimichi-dev\target`) に向けます。リポジトリを同期アプリ (Dropbox など) のフォルダに置くと、ビルド中のファイルを掴まれて失敗するためです。環境変数 `CARGO_TARGET_DIR` があれば、そちらを使います。
- exe に埋め込まれるパスから、Windows のユーザー名を取り除きます。

`logs/` (Git の管理外) に VRChat の実際のログを置くと、実ログを使うテストも実行します。

## README のスクリーンショット

`docs/images/` の画像は、架空のデータで撮っています。実際のログや写真は写りません。

`pnpm dev` で開発サーバーを起動し、`http://localhost:1420/?demo` を開くと、ダミーデータの画面になります。データは `ui/src/demo/data.ts` にあります。この表示は開発サーバーでだけ動き、配布物には入りません。

- `&tab=stats` などを付けると、そのタブを開きます (タブの id は `ui/src/appTabs.ts`)。
- `&theme=dark` を付けると、ダークテーマになります。
- `&open=period` を付けると、「表示 → 期間で絞り込む…」と同じダイアログを開きます。

撮り直すときは、Edge をヘッドレスで動かします。ウィンドウは表示されません。

```powershell
$edge = "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe"
foreach ($t in "timeline", "stats") {
  & $edge --headless=new --hide-scrollbars --window-size=1200,750 --force-device-scale-factor=1 `
    --virtual-time-budget=8000 --screenshot="$PWD\docs\images\$t.png" "http://localhost:1420/?demo&tab=$t"
}
```

## 配布物を作る

```powershell
pnpm tauri build --no-bundle   # %LOCALAPPDATA%\yorimichi-dev\target\release\yorimichi.exe を作成
pwsh scripts/pack.ps1          # releases\ にインストーラーと更新用のパッケージを作成
```

- `pack.ps1` は、`releases\` に同じバージョンがあると止まります。配布のたびに `Cargo.toml`、`src-tauri/tauri.conf.json`、`package.json` のバージョンを上げてください。
- インストーラーは、アプリを `%LOCALAPPDATA%\Yorimichi` に入れます。
- 含まれるライブラリのライセンスの一覧 (`THIRD-PARTY-LICENSES.html`) は、`pack.ps1` が作って同梱します。単独で作るには `pnpm third-party` を実行します。`about.toml` に無いライセンスの依存が増えると、作成が止まります。

アプリは、GitHub の最新リリースから更新を探します。リリースには、`releases\` にできた次の 3 つを添付してください。

- `Yorimichi-win-Setup.exe`
- `Yorimichi-<バージョン>-full.nupkg`
- `releases.win.json`

## 構成

```
crates/core/   ログの解析、データベース、検索、写真、HTML の書き出し (Tauri に依存しない。テストはここ)
src-tauri/     Tauri のアプリ (コマンド、メニューとトレイ、自動取り込み、設定、更新)
ui/            画面 (React + TypeScript、Vite)。ui/src/features/ に画面ごと、ui/src/components/ に共通部品
scripts/       pack.ps1 (配布物の作成)、licenses.mjs (ライセンスの一覧)、with-target.mjs (cargo の出力先)
docs/          設定、メタデータ、開発の説明
```

画面の配色と見た目の方針は [DESIGN.md](../DESIGN.md) にあります。
