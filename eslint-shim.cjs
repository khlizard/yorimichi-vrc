// typescript-eslint does not support TypeScript 7 yet. Route its `require("typescript")` to the
// TS 6 copy (devDependency `typescript6`); `pnpm build` still type-checks with TS 7.
const Module = require("node:module");
const orig = Module._resolveFilename;
let ts6;
Module._resolveFilename = function (request, parent, ...rest) {
  if (request === "typescript" && parent && /typescript-eslint|typescript-estree|ts-api-utils/.test(parent.filename ?? "")) {
    ts6 ??= require.resolve("typescript6", { paths: [process.cwd()] });
    return ts6;
  }
  return orig.call(this, request, parent, ...rest);
};
