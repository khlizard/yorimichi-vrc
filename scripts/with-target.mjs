// cargo を動かすコマンドを、次の 2 つを設定してから実行する。
// 1. ビルド出力先（target）を Dropbox の外へ向ける。
//    Dropbox 内に target を置くと、ビルド中の一時ファイルを掴まれてビルドが失敗するため。
//    出力先は %LOCALAPPDATA%\<package.json の name>\target。CARGO_TARGET_DIR が設定済みならそれを使う。
// 2. 実行ファイルに埋め込まれるパスから、ホームフォルダ（Windows のユーザー名を含む）を取り除く。
//    Rust はエラー表示用にライブラリのソースの場所（C:\Users\<名前>\.cargo\...）を埋め込むため。
// 使い方（プロジェクトのルートで）: node scripts/with-target.mjs <コマンド> [引数...]
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { basename, join } from "node:path";

function projectName() {
  try {
    const name = JSON.parse(readFileSync("package.json", "utf8")).name;
    if (typeof name === "string" && name !== "") return name.replace(/[^\w.-]+/g, "-");
  } catch {
    // package.json がなければフォルダ名を使う。
  }
  return basename(process.cwd());
}

const base = process.env.LOCALAPPDATA ?? join(homedir(), ".cache");
// 後に書いた置き換えが優先される。プロジェクトのフォルダは「.」、それ以外のホーム以下は「~」にする。
// 既に指定されているフラグは残し、その後ろに置き換えの指定を足す。
// CARGO_ENCODED_RUSTFLAGS は区切りに U+001F を使う形式で、パスに空白があっても 1 つの引数として渡る。
// cargo は CARGO_ENCODED_RUSTFLAGS があると RUSTFLAGS を見ないので、あるほうを引き継ぐ。
const given = process.env.CARGO_ENCODED_RUSTFLAGS
  ? process.env.CARGO_ENCODED_RUSTFLAGS.split("\x1f").filter(Boolean)
  : (process.env.RUSTFLAGS ?? "").split(/\s+/).filter(Boolean);
const flags = [...given, `--remap-path-prefix=${homedir()}=~`, `--remap-path-prefix=${process.cwd()}=.`];
const env = {
  ...process.env,
  CARGO_TARGET_DIR: process.env.CARGO_TARGET_DIR ?? join(base, projectName(), "target"),
  CARGO_ENCODED_RUSTFLAGS: flags.join("\x1f"),
};
const [command, ...args] = process.argv.slice(2);
if (!command) {
  console.error("使い方: node scripts/with-target.mjs <コマンド> [引数...]");
  process.exit(2);
}
const result = spawnSync(command, args, { stdio: "inherit", shell: true, env });
process.exit(result.status ?? 1);
