import test from "node:test";
import assert from "node:assert/strict";
import {
  findModelPreset,
  withModelPreset,
  pricingDraft,
  pricingRows,
} from "../src/lib/model-pricing.ts";
import { modelWrite } from "../src/lib/domain.ts";

const preset = {
  provider_id: "chatgpt",
  model: "gpt-5.5",
  kind: "text",
  version: "fixture",
  source_url: "https://developers.openai.com/api/docs/models/gpt-5.5",
  verified_at: "2026-10-03",
  token_prices: [
    {
      tier: "standard",
      min_input_tokens: 0,
      input_rate: "5",
      cached_rate: "0.5",
      cache_write_rate: "5",
      output_rate: "30",
    },
    {
      tier: "standard",
      min_input_tokens: 272001,
      input_rate: "10",
      cached_rate: "1",
      cache_write_rate: "10",
      output_rate: "45",
    },
    {
      tier: "fast",
      min_input_tokens: 0,
      max_input_tokens: 272000,
      input_rate: "12.5",
      cached_rate: "1.25",
      cache_write_rate: "12.5",
      output_rate: "75",
    },
  ],
};
const model = {
  provider_id: "chatgpt",
  model: " gpt-5.5 ",
  kind: "text",
  enabled: false,
  revision: 4,
  token_prices: [],
  image_prices: [],
};

test("exact provider/model matching applies a copy of the preset without changing account policy", () => {
  assert.equal(findModelPreset([preset], "chatgpt", " gpt-5.5 "), preset);
  assert.equal(findModelPreset([preset], "other", "gpt-5.5"), undefined);
  assert.equal(findModelPreset([preset], "chatgpt", "gpt-5.5-custom"), undefined);
  assert.equal(findModelPreset(undefined, "chatgpt", "gpt-5.5"), undefined);
  const draft = withModelPreset(model, preset);
  assert.equal(draft.enabled, false);
  assert.equal(draft.revision, 4);
  assert.equal(draft.model, "gpt-5.5");
  draft.token_prices[0].input_rate = "99";
  assert.equal(preset.token_prices[0].input_rate, "5");
  assert.deepEqual(model.token_prices, []);
  assert.throws(() => withModelPreset(model, { ...preset, token_prices: [] }));
});

test("preset writes identify the version while manual edits remain explicit prices", () => {
  const value = withModelPreset(model, preset);
  const automatic = modelWrite(value, preset.version);
  assert.equal(automatic.pricing_preset, "fixture");
  assert.deepEqual(automatic.token_prices, []);
  assert.equal("source_url" in automatic, false);
  const manual = modelWrite(value);
  assert.equal("pricing_preset" in manual, false);
  assert.deepEqual(manual.token_prices, preset.token_prices);
});

test("bounded Fast pricing survives editor round trips and rejects inverted bounds", () => {
  const draft = pricingDraft(preset.token_prices);
  assert.equal(draft.fast.mode, "custom");
  assert.deepEqual(pricingRows(draft), preset.token_prices);
  draft.fast.rows[0].max_input_tokens = -1;
  assert.throws(() => pricingRows(draft));
});
