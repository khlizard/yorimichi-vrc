// 配布物に入れる「第三者のライセンス」の HTML を作る。
// Rust のライブラリは cargo-about (cargo install cargo-about --locked --features cli)、
// 画面側のライブラリは pnpm が持つ情報と、各パッケージに入っているライセンスの文面から集める。
// 使い方（プロジェクトのルートで）: node scripts/licenses.mjs [出力先]
//   出力先を省くと THIRD-PARTY-LICENSES.html に書く。
import { spawnSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const out = resolve(process.argv[2] ?? "THIRD-PARTY-LICENSES.html");
const MARK = "<!--JS_LICENSES-->";

function run(command, args, options = {}) {
  const r = spawnSync(command, args, { shell: true, encoding: "utf8", maxBuffer: 64 * 1024 * 1024, ...options });
  if (r.status !== 0) {
    console.error(r.stderr || r.stdout);
    throw new Error(`${command} ${args.join(" ")} が失敗しました (exit ${r.status})`);
  }
  return r.stdout;
}

const escapeHtml = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

// Rust: about.toml に無いライセンスの依存があると、ここで失敗する。
run("node", [
  "scripts/with-target.mjs",
  "cargo",
  "about",
  "generate",
  "--manifest-path",
  "src-tauri/Cargo.toml",
  "-c",
  "about.toml",
  "about.hbs",
  "-o",
  `"${out}"`,
]);

// 画面側: exe に埋め込まれるのは dependencies だけ (devDependencies は入らない)。
const byLicense = JSON.parse(run("pnpm", ["licenses", "list", "--prod", "--json"]));
const packages = Object.entries(byLicense)
  .flatMap(([license, list]) => list.map((p) => ({ license, ...p })))
  .sort((a, b) => a.name.localeCompare(b.name));

let js = "<h2>画面側のライブラリ</h2>\n";
for (const p of packages) {
  const dir = p.paths?.[0];
  const file = dir && existsSync(dir) ? readdirSync(dir).find((f) => /^licen[sc]e/i.test(f)) : undefined;
  if (!file) throw new Error(`${p.name} のライセンスの文面が見つかりません`);
  const text = readFileSync(join(dir, file), "utf8");
  js += `<h3>${escapeHtml(p.name)} ${escapeHtml((p.versions ?? []).join(", "))} (${escapeHtml(p.license)})</h3>\n`;
  js += `<pre>${escapeHtml(text.trim())}</pre>\n`;
}

const html = readFileSync(out, "utf8");
if (!html.includes(MARK)) throw new Error(`about.hbs に ${MARK} がありません`);
writeFileSync(out, html.replace(MARK, js));
console.log(`作成しました: ${out} (画面側 ${packages.length} 件)`);
