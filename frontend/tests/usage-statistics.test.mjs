import test from "node:test";
import assert from "node:assert/strict";
import { statisticsRows, overviewChart } from "../src/lib/usage-statistics.ts";

test("time series fills only absent intervals and preserves unknown actual usage", () => {
  const unknown = {
    key: "2026-09-21",
    label: "2026-09-21",
    total_tokens: null,
    input_tokens: null,
    cache_rate: null,
  };
  const rows = statisticsRows({
    group_by: "day",
    tz_offset: -480,
    from_ms: Date.parse("2026-09-20T16:00:00Z"),
    until_ms: Date.parse("2026-09-23T16:00:00Z"),
    summary: { request_count: 1 },
    rows: [unknown],
  });
  assert.equal(rows.length, 3);
  assert.equal(rows[0].total_tokens, null);
  assert.equal(rows[1].total_tokens, 0);
  assert.equal(rows[1].cache_rate, null);
  assert.equal(rows[2].key, "2026-09-23");
});

test("model stacks use tokens rather than request counts and keep unknown model usage", () => {
  const result = overviewChart({
    group_by: "day",
    tz_offset: -480,
    from_ms: Date.parse("2026-09-20T16:00:00Z"),
    until_ms: Date.parse("2026-09-23T16:00:00Z"),
    summary: { request_count: 8 },
    rows: [
      {
        key: "2026-09-22",
        label: "2026-09-22",
        total_tokens: null,
        request_count: 1,
        cache_rate: null,
        cost_nano_usd: null,
      },
      {
        key: "2026-09-21",
        label: "2026-09-21",
        total_tokens: 400,
        request_count: 7,
        cache_rate: 62.5,
        cost_nano_usd: 123456789,
      },
    ],
    model_usage: [
      { bucket: "2026-09-21", model: "a", total_tokens: 100, missing_token_requests: 0 },
      { bucket: "2026-09-21", model: "b", total_tokens: 300, missing_token_requests: 0 },
      { bucket: "2026-09-22", model: "a", total_tokens: null, missing_token_requests: 1 },
    ],
  });
  assert.deepEqual(
    result.models.map((model) => model.model),
    ["b", "a"],
  );
  assert.equal(result.rows[0].model_0, 300);
  assert.equal(result.rows[0].model_1, 100);
  assert.equal(result.rows[0].total_tokens, 400);
  assert.equal(result.rows[0].request_count, 7);
  assert.equal(result.rows[0].cache_rate, 62.5);
  assert.equal(result.rows[0].cost_usd, 0.123456789);
  assert.equal(result.rows[1].model_1, null);
  assert.equal(result.rows[1].total_tokens, null);
  assert.equal(result.rows[1].cost_usd, null);
  assert.equal(result.rows[1].cache_rate, null);
  assert.equal(result.rows[2].total_tokens, 0);
  assert.equal(result.rows[2].request_count, 0);
});

test("dimension ranking keeps all rows and separates identities with the same label", () => {
  const rows = statisticsRows({
    group_by: "virtual_account",
    rows: [
      { key: "a", label: "same", total_tokens: null },
      { key: "b", label: "same", total_tokens: 0 },
      { key: "c", label: "same", total_tokens: 100 },
    ],
  });
  assert.deepEqual(
    rows.map((row) => row.key),
    ["c", "b", "a"],
  );
});

test("account model stacks join on account identity alongside the four metrics", () => {
  const result = overviewChart({
    group_by: "virtual_account",
    rows: [
      {
        key: "a",
        label: "same",
        total_tokens: 100,
        request_count: 4,
        cache_rate: 10,
        cost_nano_usd: 1000000000,
      },
      {
        key: "b",
        label: "same",
        total_tokens: 300,
        request_count: 1,
        cache_rate: 90,
        cost_nano_usd: 2000000000,
      },
    ],
    model_usage: [
      { bucket: "a", model: "m", total_tokens: 100 },
      { bucket: "b", model: "m", total_tokens: 300 },
    ],
  });
  assert.deepEqual(
    result.rows.map((row) => [
      row.key,
      row.model_0,
      row.request_count,
      row.cache_rate,
      row.cost_usd,
    ]),
    [
      ["b", 300, 1, 90, 2],
      ["a", 100, 4, 10, 1],
    ],
  );
});
