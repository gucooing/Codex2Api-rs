import test from "node:test";
import assert from "node:assert/strict";
import { usageSeries, usageCost, tokens } from "../src/lib/usage.ts";

test("loading and a successful empty ledger remain distinct", () => {
  assert.deepEqual(usageSeries(undefined), []);
  const empty = usageSeries({
    from_ms: Date.parse("2026-10-01T00:00:00+08:00"),
    until_ms: Date.parse("2026-10-03T00:00:00+08:00"),
    tz_offset: -480,
    rows: [],
  });
  assert.equal(empty.length, 2);
  assert.ok(
    empty.every((row) => row.request_count === 0 && row.total_tokens === 0 && row.cost_usd === 0),
  );
});

test("usage charts preserve unknown costs and tokens and fill only empty dates", () => {
  const data = {
    from_ms: Date.parse("2026-10-01T00:00:00+08:00"),
    until_ms: Date.parse("2026-10-04T00:00:00+08:00"),
    tz_offset: -480,
    rows: [
      {
        key: "2026-10-01",
        request_count: 1,
        failed_requests: 1,
        total_tokens: null,
        cost_nano_usd: null,
        missing_token_requests: 1,
        unpriced_requests: 1,
      },
      {
        key: "2026-10-03",
        request_count: 2,
        failed_requests: 0,
        total_tokens: 300,
        cost_nano_usd: 1250000000,
        missing_token_requests: 0,
        unpriced_requests: 0,
      },
    ],
  };
  const rows = usageSeries(data);
  assert.deepEqual(
    rows.map((row) => row.date),
    ["2026-10-01", "2026-10-02", "2026-10-03"],
  );
  assert.equal(rows[0].total_tokens, null);
  assert.equal(rows[0].cost_usd, null);
  assert.equal(rows[1].request_count, 0);
  assert.equal(rows[1].total_tokens, 0);
  assert.equal(rows[2].cost_usd, 1.25);
  assert.equal(usageCost(null), "—");
  assert.equal(usageCost(0), "$0.000000");
  assert.equal(tokens(null), "—");
});
