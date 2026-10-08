import test from "node:test";
import assert from "node:assert/strict";
import { requestSpeed } from "../src/lib/usage-display.ts";

test("speed displays the request tier independently of billing results", () => {
  for (const [service_tier, expected] of [
    ["priority", "fast"],
    ["fast", "fast"],
    ["flex", "flex"],
    ["default", "standard"],
    [null, "standard"],
    ["custom-speed", "custom-speed"],
  ]) {
    assert.equal(requestSpeed({ service_tier, billing_tier: "standard" }), expected);
    assert.equal(requestSpeed({ service_tier, billing_tier: null }), expected);
  }
});
