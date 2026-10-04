import test from "node:test";
import assert from "node:assert/strict";
import { usdCents } from "../src/lib/wallet.ts";
test("wallet adjustments parse exact cents and reject rounding or unsafe amounts", () => {
  for (const [value, cents] of [
    ["0", 0],
    ["0.29", 29],
    ["12.3", 1230],
    [" 10.25 ", 1025],
  ])
    assert.equal(usdCents(value), cents);
  for (const value of ["", "-2", "1e4", "1.234", "NaN", "90071992547409.92"])
    assert.equal(usdCents(value), null);
});
