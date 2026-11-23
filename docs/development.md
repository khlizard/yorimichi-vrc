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
- `&open=period` を付けると「表示 → 期間で絞り込む…」、`&open=about` を付けると「ヘルプ → Yorimichi について…」と同じダイアログを開きます。

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

- `pack.ps1` は exe のビルドから行います。`releases\` に同じバージョンがあると止まります。
- インストーラーは、アプリを `%LOCALAPPDATA%\Yorimichi` に入れます。
- 含まれるライブラリのライセンスの一覧 (`THIRD-PARTY-LICENSES.html`) は、`pack.ps1` が作って同梱します。単独で作るには `pnpm third-party` を実行します。`about.toml` に無いライセンスの依存が増えると、作成が止まります。
- `releases\` は git の管理外です。前の版のパッケージを消さずに残すと、`pack.ps1` が前の版からの差分 (`-delta.nupkg`) も作ります。

## リリースを出す

細かい修正はコミットだけしておき、ある程度まとまってから版を上げて出します。使う人が困る不具合の修正は、1 件でもすぐ出します。

1. 前の版からの変更を見る: `git log v<前の版>..main --oneline`。`feat:` と `fix:` のコミットが、リリースノートの材料になります。
2. バージョンを上げる。書かれている所は次の 4 つです (`git grep -n '<今の版>'` で探せます)。
   - `Cargo.toml` (`[workspace.package]` の `version`)
   - `Cargo.lock` (`yorimichi` と `yorimichi-core` の 2 か所)
   - `src-tauri/tauri.conf.json`
   - `package.json`
3. `chore: バージョンを <版> にする` としてコミットし、`pwsh scripts/pack.ps1` で配布物を作る。
4. できた `Yorimichi-win-Setup.exe` を、インストール済みのアプリの上から入れ、起動して確かめる。
5. `main` を push し、版を上げたコミットにタグ `v<版>` を付けて push する。
6. GitHub の Release を作り、`releases\` にできた次の 4 つを添付する。アプリは GitHub の最新リリースから更新を探します。
   - `Yorimichi-win-Setup.exe`
   - `Yorimichi-<版>-full.nupkg`
   - `Yorimichi-<版>-delta.nupkg` (前の版があるときだけできます。付けると更新のダウンロードが小さくなります)
   - `releases.win.json`

   ```powershell
   gh release create v<版> --verify-tag --title "Yorimichi <版>" --notes-file <ノート.md> `
     releases/Yorimichi-win-Setup.exe releases/Yorimichi-<版>-full.nupkg `
     releases/Yorimichi-<版>-delta.nupkg releases/releases.win.json
   ```

- `v*` のタグは GitHub の設定で保護しており、消したり付け替えたりできません。間違えたら、版を上げて出し直します。
- リリースノートには、インストールの仕方 (署名が無いので Windows の警告が出ること)、その版の変更、VRChat の非公式ツールであることを書きます。

## 構成

```
crates/core/   ログの解析、データベース、検索、写真、HTML の書き出し (Tauri に依存しない。テストはここ)
src-tauri/     Tauri のアプリ (コマンド、メニューとトレイ、自動取り込み、設定、更新)
ui/            画面 (React + TypeScript、Vite)。ui/src/features/ に画面ごと、ui/src/components/ に共通部品
scripts/       pack.ps1 (配布物の作成)、licenses.mjs (ライセンスの一覧)、with-target.mjs (cargo の出力先)
docs/          設定、メタデータ、開発の説明
```

画面の配色と見た目の方針は [DESIGN.md](../DESIGN.md) にあります。
