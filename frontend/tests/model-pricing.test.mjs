import test from "node:test";
import assert from "node:assert/strict";
import { pricingDraft, pricingRows, multiplyPrice } from "../src/lib/model-pricing.ts";

const sort = (rows) =>
  rows.toSorted((a, b) => a.tier.localeCompare(b.tier) || a.min_input_tokens - b.min_input_tokens);
const base = (start = 0) => ({
  tier: "standard",
  min_input_tokens: start,
  input_rate: "2.5",
  cached_rate: "0.25",
  cache_write_rate: "3.125",
  output_rate: "10",
});
const scale = (row, tier, multiplier) => ({
  ...row,
  tier,
  input_rate: multiplyPrice(row.input_rate, multiplier),
  output_rate: multiplyPrice(row.output_rate, multiplier),
  cached_rate: multiplyPrice(row.cached_rate, multiplier),
  cache_write_rate: multiplyPrice(row.cache_write_rate, multiplier),
});
test("base and context prices round trip through tier multipliers without changing charges", () => {
  const ordinary = base(),
    long = {
      ...base(272001),
      input_rate: "5",
      cached_rate: "0.5",
      cache_write_rate: "6.25",
      output_rate: "15",
    };
  const prices = [
    ordinary,
    long,
    ...[ordinary, long].map((r) => scale(r, "fast", "2")),
    ...[ordinary, long].map((r) => scale(r, "flex", "0.5")),
  ];
  const draft = pricingDraft(prices);
  assert.equal(draft.fast.mode, "multiplier");
  assert.equal(draft.fast.multiplier, "2");
  assert.equal(draft.flex.multiplier, "0.5");
  assert.deepEqual(sort(pricingRows(draft)), sort(prices));
  draft.fast.multiplier = "3";
  assert.equal(
    pricingRows(draft).find((r) => r.tier === "fast" && r.min_input_tokens === 272001).output_rate,
    "45",
  );
  assert.equal(
    prices.find((r) => r.tier === "fast" && r.min_input_tokens === 272001).output_rate,
    "30",
  );
});
test("non-proportional legacy tiers and missing tiers stay exact and explicit", () => {
  const prices = [base(), { ...base(), tier: "fast", input_rate: "3", output_rate: "17" }];
  const draft = pricingDraft(prices);
  assert.equal(draft.fast.mode, "custom");
  assert.equal(draft.flex.mode, "off");
  assert.deepEqual(sort(pricingRows(draft)), sort(prices));
  assert.deepEqual(pricingRows(pricingDraft([])), []);
});
test("prices use exact decimals and reject lost precision or ambiguous intervals", () => {
  assert.equal(multiplyPrice("0.000001", "2"), "0.000002");
  assert.equal(multiplyPrice("2.5", "0"), "0");
  assert.throws(() => multiplyPrice("0.000001", "0.5"));
  assert.throws(() => multiplyPrice("-1", "2"));
  const draft = pricingDraft([base(), base(100)]);
  draft.ranges[1].min_input_tokens = 0;
  assert.throws(() => pricingRows(draft));
});
