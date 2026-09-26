import test from "node:test";
import assert from "node:assert/strict";
import {
  cacheRate,
  tokenCount,
  duration,
  failedUsage,
  usageStatus,
  usageStatuses,
} from "../src/lib/usage-display.ts";

test("usage display preserves missing values and formats counts, cache rate and duration", () => {
  assert.equal(tokenCount(150800), "150.8K");
  assert.equal(tokenCount(1250000), "1.3M");
  assert.equal(tokenCount(null), "-");
  assert.equal(tokenCount(0), "0");
  assert.equal(cacheRate({ input_tokens: 24830, cached_tokens: 24320 }), "97.9%");
  assert.equal(cacheRate({ input_tokens: 0, cached_tokens: 0 }), "-");
  assert.equal(cacheRate({ input_tokens: 100, cached_tokens: null }), "-");
  assert.equal(duration(951), "951 ms");
  assert.equal(duration(1569), "1.57秒");
  assert.equal(duration(145383), "2分25秒");
  assert.equal(duration(3900000), "1小时5分");
  assert.equal(duration(null), "-");
  assert.equal(failedUsage({ status: "failed" }), true);
});
test("usage filters and labels group all unsuccessful terminal states as failures", () => {
  assert.deepEqual(
    usageStatuses.map(({ value }) => value),
    ["in_progress", "completed", "failed"],
  );
  assert.equal(usageStatus("completed").label, "成功");
  assert.equal(usageStatus("in_progress").label, "进行中");
  assert.equal(usageStatus("client_stopped").label, "成功");
  assert.equal(failedUsage({ status: "client_stopped" }), false);
  for (const status of ["failed", "incomplete", "interrupted"]) {
    assert.equal(usageStatus(status).label, "失败");
    assert.equal(usageStatus(status).className, usageStatus("failed").className);
    assert.equal(failedUsage({ status }), true);
  }
});

import {
  quotaWindowLabel,
  quotaResetLabel,
  supplierStatusLabel,
  percentLabel,
} from "../src/lib/supplier-state.ts";

test("supplier quota labels use the reported duration instead of assuming fixed windows", () => {
  for (const [seconds, label] of [
    [18000, "5h"],
    [604800, "7天"],
    [2592000, "30天"],
    [86400, "1天"],
    [5400, "90分钟"],
    [45, "45秒"],
    [null, "主额度"],
  ]) {
    assert.equal(quotaWindowLabel({ id: "primary_window", limit_window_seconds: seconds }), label);
  }
  assert.equal(quotaWindowLabel({ id: "secondary_window", limit_window_seconds: null }), "次额度");
});

test("quota countdown keeps missing and expired cache explicit without inventing usage", () => {
  const now = 2000000000000;
  assert.equal(quotaResetLabel(now / 1000 + 266400, now), "3天 2小时");
  assert.equal(quotaResetLabel(now / 1000 - 1, now), "已到重置时间，待更新");
  assert.equal(quotaResetLabel(null, now), "重置时间未提供");
  assert.equal(percentLabel(18), "18%");
  assert.equal(supplierStatusLabel("error"), "错误");
});
import {
  allowedFields,
  canEditConfig,
  clientOwnedKeys,
  modelWrite,
  planWrite,
  mergeOAuth,
  sameProviderModels,
  withPath,
} from "../src/lib/domain.ts";

test("client state cannot become an admin form even when server marks it editable", () => {
  for (const key of clientOwnedKeys) assert.equal(canEditConfig(key, false), false, key);
  assert.equal(canEditConfig("models", false), false);
  assert.equal(canEditConfig("profile", true), false);
  assert.equal(canEditConfig("profile", false), true);
});
test("mixed user settings only expose service flags and preserve preferences", () => {
  const preferences = { path: ["settings", "training_allowed"] };
  const flags = { path: ["flags", "file_library_enabled_for_registration_country"] };
  assert.deepEqual(allowedFields("user_settings", [preferences, flags]), [flags]);
  const before = { settings: { training_allowed: false }, flags: {} };
  const after = withPath(before, flags.path, true);
  assert.deepEqual(after.settings, before.settings);
  assert.deepEqual(before.flags, {});
  assert.throws(() => withPath(before, ["__proto__", "admin"], true));
});
test("provider model selection remains isolated", () => {
  assert.deepEqual(
    sameProviderModels(
      [
        { provider_id: "chatgpt", model: "gpt-5" },
        { provider_id: "grok", model: "gpt-5" },
      ],
      "chatgpt",
    ),
    [{ provider_id: "chatgpt", model: "gpt-5" }],
  );
});
test("mutation DTOs exclude read only metadata", () => {
  const model = {
    provider_id: "chatgpt",
    model: "gpt-5",
    kind: "text",
    enabled: true,
    revision: 5,
    token_prices: [],
    image_prices: [{ resolution: "2K", price: "1" }],
    codex_metadata_source: "reference",
    codex_metadata_status: "verified",
  };
  assert.deepEqual(
    Object.keys(modelWrite(model)).sort(),
    ["provider_id", "model", "kind", "enabled", "revision", "token_prices", "image_prices"].sort(),
  );
  assert.deepEqual(modelWrite(model).image_prices, []);
  const plan = planWrite({
    id: "plan",
    revision: 2,
    updated_at_ms: 23,
    models: [],
    free_models: [],
  });
  assert.equal("id" in plan, false);
  assert.equal("updated_at_ms" in plan, false);
  assert.equal(plan.revision, 2);
});
test("device polling preserves pending authorization context", () => {
  const pending = {
    status: "pending",
    method: "device",
    state: "state",
    user_code: "CODE",
    verification_url: "https://auth.openai.com/device",
    interval: 5,
  };
  assert.deepEqual(mergeOAuth(pending, { status: "pending" }), pending);
  assert.deepEqual(mergeOAuth(pending, { status: "complete", supplier_id: "supplier" }), {
    status: "complete",
    supplier_id: "supplier",
  });
  assert.throws(() => mergeOAuth(undefined, { status: "pending" }));
});
