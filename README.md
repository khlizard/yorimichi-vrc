# VrcLogger

VRChat のログファイル (`output_log_*.txt`) を解析して SQLite に蓄積し、
「いつ・どのワールドで・誰と会ったか」「どんな URL が読み込まれたか」を検索できる Windows 用デスクトップアプリです。

## 主な機能

- **ログ自動取り込み** — VRChat のログフォルダを定期スキャン (既定 5 分間隔) し、新規・追記されたファイルだけを差分取り込み
- **プレイヤー検索** — 表示名 / ユーザー ID で検索し、そのプレイヤーと同席したセッション一覧を表示。表示名の変更履歴も記録
- **セッション一覧** — 訪問したワールド・インスタンスごとに、滞在時間と同席プレイヤーを表示
- **URL 検索** — 動画再生 (`[Video Playback]`)・文字列ダウンロード・画像ダウンロードで読み込まれた URL を種別ごとに検索
- **生ログ全文検索** — SQLite FTS5 によるログ行の全文検索 (ログレベルで絞り込み可)
- **日付範囲フィルタ** — 全タブ共通で期間を絞り込み
- **タスクトレイ常駐** — 閉じるボタンでトレイに格納して常駐 (設定で無効化可)
- **多重起動防止** — 二重に起動した場合は既存のウィンドウを前面に表示して終了

## 動作環境

- Windows 10 / 11
- [.NET 9 SDK](https://dotnet.microsoft.com/download/dotnet/9.0) (ビルド用)

## ビルドと実行

```powershell
# アプリを起動
dotnet run --project src/VrcLogger.App

# テスト
dotnet test

# リリースビルド
dotnet publish src/VrcLogger.App -c Release
```

VS Code を使う場合は `.vscode/tasks.json` の `build` / `watch` タスク、`launch.json` のデバッグ構成が利用できます。

## 設定

初回起動時にリポジトリルート (`VrcLogger.slnx` のある場所) に `vrclogger.settings.json` が生成されます。
「設定」タブから GUI で編集するか、直接ファイルを編集してください。

```json
{
  "DbPath": "vrclog.db",
  "LogFolders": [
    "%USERPROFILE%\AppData\LocalLow\VRChat\VRChat"
  ],
  "AutoImportIntervalMinutes": 5,
  "MinimizeToTray": true
}
```

| キー | 説明 |
| --- | --- |
| `DbPath` | SQLite データベースのパス。相対パスは設定ファイルのあるディレクトリ基準 |
| `LogFolders` | `output_log_*.txt` を探すフォルダ一覧。環境変数を展開可。複数フォルダに同名ファイルがある場合はサイズが大きい方を採用 |
| `AutoImportIntervalMinutes` | 自動取り込みの間隔 (分)。`0` で無効 |
| `MinimizeToTray` | `true` の場合、閉じるボタンでタスクトレイに常駐 |

## プロジェクト構成

```
VrcLogger.slnx
├── src/
│   ├── VrcLogger.Core/            # UI 非依存のコアライブラリ (net9.0)
│   │   ├── Parsing/               # ログ行の正規表現・パーサ・解析結果モデル
│   │   └── Data/                  # SQLite スキーマ・取り込み・検索クエリ (Dapper)
│   └── VrcLogger.App/             # WPF アプリ (net9.0-windows, CommunityToolkit.Mvvm)
│       ├── ViewModels/            # タブごとの ViewModel
│       ├── Views/                 # タブごとの View (XAML)
│       └── Services/              # 設定ファイルの読み書き
├── tests/
│   └── VrcLogger.Core.Tests/      # xUnit テスト (パーサ・取り込み・実ログ統合テスト)
└── logs/                          # (Git 管理外) 実ログを置くと RealLog* テストが実行される
```

### 解析するログ行

| 種別 | パターン例 |
| --- | --- |
| ワールド入室 | `[Behaviour] Entering Room: ...` / `[Behaviour] Joining wrld_...:instance` |
| ワールド退室 | `[Behaviour] OnLeftRoom` |
| プレイヤー入退室 | `[Behaviour] OnPlayerJoined 名前 (usr_...)` / `OnPlayerLeft ...` |
| 自分自身の認証 | `User Authenticated: 名前 (usr_...)` |
| URL 読み込み | `[Video Playback] Resolving URL '...'`, `[String Download] ...`, `[Image Download] ...` |

### データベース

`vrclog.db` (WAL モード) に以下のテーブルを作成します。

- `log_files` — 取り込み済みファイルとサイズ (差分取り込みの判定に使用)
- `worlds` / `players` / `player_names` — ワールド、プレイヤー、表示名の履歴
- `visits` — ワールド訪問 (入室〜退室)
- `encounters` — 訪問中に同席したプレイヤー
- `url_events` — 読み込まれた URL
- `raw_events` + `raw_events_fts` — 全ログ行と FTS5 全文検索インデックス

ファイルが追記された場合は、そのファイル由来の行を `ON DELETE CASCADE` で削除してから再解析します。

## 使用ライブラリ

- [Microsoft.Data.Sqlite](https://www.nuget.org/packages/Microsoft.Data.Sqlite) / [Dapper](https://www.nuget.org/packages/Dapper)
- [CommunityToolkit.Mvvm](https://www.nuget.org/packages/CommunityToolkit.Mvvm)
- [xUnit](https://xunit.net/)
