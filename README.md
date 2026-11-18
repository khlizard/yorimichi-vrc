# VrcLogger

VRChat のログファイル (`output_log_*.txt`) を解析して SQLite に蓄積し、
「いつ・どのワールドで・誰と会ったか」「どんな URL が読み込まれたか」を検索できる Windows 用デスクトップアプリです。

## 主な機能

- **ログ自動取り込み** — VRChat のログフォルダを定期スキャン (既定 5 分間隔) し、新規・追記されたファイルだけを差分取り込み
- **プレイヤー検索** — 表示名 / ユーザー ID で検索し、そのプレイヤーと同席したセッション一覧を表示。表示名の変更履歴も記録
- **セッション一覧** — 訪問したワールド・インスタンスごとに、滞在時間と同席プレイヤーを表示
- **URL 検索** — 動画再生 (`[Video Playback]`)・文字列ダウンロード・画像ダウンロードで読み込まれた URL を種別ごとに検索
- **アバター履歴** — 誰がどのアバターを着ていたか (アバター名) と、ダウンロード時のサイズ (MB)・所要秒数。セッション / プレイヤーの一覧にも表示
- **通知・モデレーション履歴** — 招待 / 招待リクエスト / フレンド申請 / boop などの通知と、ミュート・ブロック・Vote Kick の履歴 (グループ告知は保存しない)
- **生ログ全文検索** — SQLite FTS5 によるログ行の全文検索 (ログレベルで絞り込み可)。定型ノイズ行は取り込み時に除外し、既定で 30 日より古い行は自動削除
- **日付範囲フィルタ** — 全タブ共通で期間を絞り込み
- **タスクトレイ常駐** — 閉じるボタンでトレイに格納して常駐 (設定で無効化可)
- **多重起動防止** — 二重に起動した場合は既存のウィンドウを前面に表示して終了

### 別PCから見る

DB (`vrclog.db`) は Dropbox などで共有された別PCから **アプリで開かないでください**。
SQLite の WAL ファイルが別々に同期されたり、2台から同時に書き込まれたりして壊れる恐れがあります。

代わりに、メインPCのアプリが取り込みのたびに `vrclog-viewer.html` を書き出します (設定 `ViewerExportPath`)。
これは DB に触れない自己完結の HTML なので、別PCではダブルクリックしてブラウザで開くだけで、
プレイヤー検索・セッションの同席者・通知を閲覧できます (メインPCの電源が切れていても可)。

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
  "MinimizeToTray": true,
  "RawLogRetentionDays": 30,
  "RawLogNoisePatterns": [ "[Behaviour] Using default gesture mask", "..." ]
}
```

| キー | 説明 |
| --- | --- |
| `DbPath` | SQLite データベースのパス。相対パスは設定ファイルのあるディレクトリ基準 |
| `LogFolders` | `output_log_*.txt` を探すフォルダ一覧。環境変数を展開可。複数フォルダに同名ファイルがある場合はサイズが大きい方を採用 |
| `AutoImportIntervalMinutes` | 自動取り込みの間隔 (分)。`0` で無効 |
| `MinimizeToTray` | `true` の場合、閉じるボタンでタスクトレイに常駐 |
| `RawLogRetentionDays` | 生ログ (`raw_events`) の保持日数。これより古い行は取り込み後に削除。`0` で無制限。セッション・プレイヤー・URL・通知は削除されない |
| `ViewerExportPath` | 別PC閲覧用 HTML の書き出し先 (既定 `vrclog-viewer.html`)。取り込みで変更があるたびに更新。空文字で無効 |
| `RawLogNoisePatterns` | 生ログに保存しない行のパターン (前方一致、先頭 `*` で部分一致)。既定はアバター読み込みなどの定型行。設定タブで編集可 |

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
| アバター | `[Behaviour] Switching NAME to avatar AVATAR` / `Download for avatar (…) (44.2 MB) …` + `Unpacking Avatar (AVATAR by NAME)` |
| 通知 | `Received Notification: <Notification from username:... of type: invite, ...>` / `AcceptNotification ...` |
| モデレーション | `[ModerationManager] NAME is now Muted` / `is no longer Blocked` / `A vote kick has been initiated against ...` |

### データベース

`vrclog.db` (WAL モード) に以下のテーブルを作成します。

- `log_files` — 取り込み済みファイルとサイズ (差分取り込みの判定に使用)
- `worlds` / `players` / `player_names` — ワールド、プレイヤー、表示名の履歴
- `visits` — ワールド訪問 (入室〜退室)
- `encounters` — 訪問中に同席したプレイヤー
- `url_events` — 読み込まれた URL
- `avatar_events` — プレイヤーごとのアバター切替とダウンロードサイズ (サイズはダウンロード時のみ)
- `notifications` — 通知 (通知 ID で重複排除。ログイン毎に再出力されるためファイルには紐付けない)
- `moderation_events` — ミュート / ブロック / Vote Kick
- `raw_events` + `raw_events_fts` — ログ行と FTS5 全文検索インデックス (ノイズ除外・保持期間あり)

ファイルが追記された場合は、そのファイル由来の行を `ON DELETE CASCADE` で削除してから再解析します。
取り込み後に期限切れ・ノイズ行を削除し、削除行数が多い場合は自動で `VACUUM` します (設定タブから手動実行も可)。

パーサの対応項目が増えた場合 (例: 通知の追加) は、設定タブの「全ログを再取り込み」で既存ファイルを解析し直すと過去分にも反映されます。

## 使用ライブラリ

- [Microsoft.Data.Sqlite](https://www.nuget.org/packages/Microsoft.Data.Sqlite) / [Dapper](https://www.nuget.org/packages/Dapper)
- [CommunityToolkit.Mvvm](https://www.nuget.org/packages/CommunityToolkit.Mvvm)
- [xUnit](https://xunit.net/)
