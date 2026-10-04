import assert from "node:assert/strict";
import test from "node:test";
import ts from "typescript";
import { readFileSync } from "node:fs";
const code = ts.transpileModule(
  readFileSync(new URL("../src/lib/refresh-tokens.ts", import.meta.url), "utf8"),
  { compilerOptions: { module: ts.ModuleKind.ESNext } },
).outputText;
const { parseRefreshTokenLines } = await import(
  `data:text/javascript;base64,${Buffer.from(code).toString("base64")}`
);
test("RT batch retains source lines, trims whitespace, and marks duplicate credentials", () => {
  assert.deepEqual(parseRefreshTokenLines(" first \r\n\r\nsecond\nfirst "), [
    { token: "first", line: 1, duplicateOf: undefined },
    { token: "second", line: 3, duplicateOf: undefined },
    { token: "first", line: 4, duplicateOf: 1 },
  ]);
});
test("RT batch rejects blanks, controls and oversized batches without echoing credentials", () => {
  assert.throws(() => parseRefreshTokenLines(" \n"));
  assert.throws(
    () => parseRefreshTokenLines("private token"),
    (e) => !e.message.includes("private"),
  );
  assert.throws(() =>
    parseRefreshTokenLines(Array.from({ length: 51 }, (_, i) => `token-${i}`).join("\n")),
  );
});
