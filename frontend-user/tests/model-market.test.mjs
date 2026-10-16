import test from "node:test";
import assert from "node:assert/strict";
import { modelMarketPricing } from "../src/app/data/model-market/pricing.ts";
import { apiRange } from "../src/lib/api-access.ts";

const price = (tier, start, end, multiplier, input, output) => ({
  tier,
  min_input_tokens: start,
  max_input_tokens: end,
  standard_multiplier: multiplier,
  base_input_usd_per_million: input,
  input_usd_per_million: String(Number(input) * 0.2),
  base_output_usd_per_million: output,
  output_usd_per_million: String(Number(output) * 0.2),
  base_cached_usd_per_million: "0.5",
  cached_usd_per_million: "0.1",
  base_cache_write_usd_per_million: input,
  cache_write_usd_per_million: String(Number(input) * 0.2),
});
const model = {
  provider_id: "chatgpt",
  model: "gpt-5.5",
  kind: "text",
  multiplier: "0.2",
  prices: [
    price("fast", 0, 272000, "2.5", "12.5", "75"),
    price("fast", 272001, null, "2.5", "25", "112.5"),
    price("flex", 272001, null, "0.5", "5", "22.5"),
    price("standard", 0, 272000, null, "5", "30"),
    price("standard", 272001, null, null, "10", "45"),
    price("flex", 0, 272000, "0.5", "2.5", "15"),
  ],
  image_prices: [],
};

test("market uses disjoint standard ranges and global Fast and Flex multipliers", () => {
  const pricing = modelMarketPricing(model, true);
  assert.deepEqual(pricing.standard.map(apiRange), ["0–272,000", "≥ 272,001"]);
  assert.equal(pricing.startingToken.input, "1");
  assert.equal(pricing.startingToken.output, "6");
  assert.equal(pricing.services[0].tier, "fast");
  assert.equal(pricing.services[0].multiplier, "2.5");
  assert.equal(pricing.services[0].followsStandard, true);
  assert.deepEqual(pricing.services[0].ranges, [{ min_input_tokens: 0, max_input_tokens: null }]);
  assert.equal(pricing.services[1].tier, "flex");
  assert.equal(pricing.services[1].multiplier, "0.5");
  assert.equal(pricing.services[1].followsStandard, true);
  assert.equal(modelMarketPricing(model, false).startingToken.input, "5");
  assert.equal(model.prices[0].tier, "fast");
});

test("non-proportional, varying and unknown service prices are not fabricated as multipliers", () => {
  const custom = structuredClone(model);
  custom.prices[0].standard_multiplier = null;
  custom.prices.find((price) => price.tier === "flex").standard_multiplier = "0.75";
  const pricing = modelMarketPricing(custom, false);
  assert.equal(pricing.services[0].multiplier, null);
  assert.equal(pricing.services[1].multiplier, null);
  assert.equal(pricing.services[0].prices[0].input, "12.5");
  assert.equal(modelMarketPricing({ ...model, prices: [] }, false).startingToken, undefined);
});

test("zero multipliers remain valid and gaps never become unrestricted service pricing", () => {
  const custom = structuredClone(model);
  custom.prices
    .filter((price) => price.tier === "fast")
    .forEach((price) => {
      price.standard_multiplier = "0";
    });
  custom.prices.find((p) => p.tier === "flex" && p.min_input_tokens === 0).max_input_tokens = 100;
  const pricing = modelMarketPricing(custom, true);
  assert.equal(pricing.services[0].multiplier, "0");
  assert.equal(pricing.services[1].followsStandard, false);
  assert.equal(pricing.services[1].ranges.length, 2);
});

test("uniform images retain the configured amount and all-resolution scope", () => {
  const uniform = {
    ...model,
    kind: "image",
    prices: [],
    image_prices: [{ resolution: "*", base_price_usd: "0.03", price_usd: "0.006" }],
  };
  assert.deepEqual(modelMarketPricing(uniform, true).startingImage, {
    resolution: "*",
    price: "0.006",
  });
  assert.deepEqual(modelMarketPricing(uniform, false).startingImage, {
    resolution: "*",
    price: "0.03",
  });
  assert.equal(apiRange({ min_input_tokens: 0, max_input_tokens: null }), "不限");
});
