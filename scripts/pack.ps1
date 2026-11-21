# Yorimichi for VRChat の Velopack パッケージ (Setup.exe と更新用 nupkg) を作成する。
# 使い方: pwsh scripts/pack.ps1
# 必要: Rust, Node.js + pnpm, vpk (dotnet tool install -g vpk),
#       cargo-about (cargo install cargo-about --locked --features cli)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

# cargo は with-target.mjs 経由で動かす (ビルド出力を Dropbox の外に置くため)。
# バージョンは workspace.package.version、出力先は target_directory から取る。
$meta = node scripts/with-target.mjs cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw "cargo metadata に失敗しました (exit $LASTEXITCODE)" }
$version = $meta.packages | Where-Object { $_.name -eq 'yorimichi' } | Select-Object -ExpandProperty version
if (-not $version) { throw 'yorimichi パッケージのバージョンを取得できませんでした' }
$targetDir = $meta.target_directory

# Velopack は既存と同じか古いバージョンを作り直さないので、先に分かりやすく止める
if (Test-Path (Join-Path $root "releases\Yorimichi-$version-full.nupkg")) {
    throw "releases\ に $version が既にあります。Cargo.toml、src-tauri\tauri.conf.json、package.json のバージョンを上げてください"
}

# フロントエンドを埋め込んだ exe をビルド (インストーラーは Tauri ではなく Velopack で作る)
pnpm tauri build --no-bundle
if ($LASTEXITCODE -ne 0) { throw "tauri build に失敗しました (exit $LASTEXITCODE)" }

# 配布物は exe 1 つだけなので、ビルド出力の中間ファイルを含めないよう専用フォルダに置く
$stage = Join-Path $targetDir 'pack'
Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item (Join-Path $targetDir 'release\yorimichi.exe') $stage

# 含まれるライブラリのライセンスの一覧を、exe と同じフォルダに入れる
node scripts/licenses.mjs (Join-Path $stage 'THIRD-PARTY-LICENSES.html')
if ($LASTEXITCODE -ne 0) { throw "ライセンスの一覧の作成に失敗しました (exit $LASTEXITCODE)" }

vpk pack `
    --packId Yorimichi `
    --packVersion $version `
    --packTitle "Yorimichi for VRChat" `
    --packAuthors khlizard `
    --runtime win-x64 `
    --packDir $stage `
    --mainExe yorimichi.exe `
    --icon (Join-Path $root 'src-tauri\icons\icon.ico') `
    --outputDir (Join-Path $root 'releases')
if ($LASTEXITCODE -ne 0) { throw "vpk pack に失敗しました (exit $LASTEXITCODE)" }

Write-Host "作成しました: releases\ (Yorimichi-win-Setup.exe ほか)"
