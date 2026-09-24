import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { planWrite, modelWrite, canEditConfig } from "../src/lib/domain.ts";
import { taskRecord, subscriptionRecord } from "../src/lib/record-selectors.ts";

// This fixture is emitted by the real Rust REST handlers over a temporary SQLite database.
const fixture = JSON.parse(
  await readFile(
    new URL("../../crates/codex2api-admin/tests/contracts.json", import.meta.url),
    "utf8",
  ),
);
const string = (value) => assert.equal(typeof value, "string");
const number = (value) => assert.equal(typeof value, "number");
const decimal = (value) => {
  string(value);
  assert.match(value, /^\d+(\.\d+)?$/);
};
const modelRef = (value) => {
  string(value.provider_id);
  string(value.model);
};

test("actual session, overview, consumer and route DTOs match their consumers", () => {
  assert.equal(fixture.session.authenticated, true);
  string(fixture.session.username);
  string(fixture.session.csrf_token);
  string(fixture.session.app_version);
  string(fixture.session.codex_cli_version);
  for (const key of ["supplier_count", "consumer_count", "enabled_consumers", "models_count"])
    number(fixture.overview[key]);
  for (const key of [
    "id",
    "username",
    "name",
    "email",
    "provider_id",
    "plan_id",
    "plan_name",
    "effective_plan",
  ])
    string(fixture.consumer[key]);
  assert.ok(["active", "expired", "free"].includes(fixture.consumer.subscription_status));
  assert.equal(typeof fixture.consumer.enabled, "boolean");
  assert.equal(fixture.routing.items[0].supplier_account_id, null);
  number(fixture.routing.items[0].revision);
  assert.equal(fixture.routing.items[0].virtual_account_id, fixture.consumer.id);
});
test("actual plans expose provider-qualified models and writable DTOs", () => {
  assert.equal("model_choices" in fixture.plans, false);
  for (const item of fixture.plans.items) {
    assert.ok(["all", "selected"].includes(item.model_access));
    assert.ok(["none", "all", "selected"].includes(item.free_model_access));
    item.models.forEach(modelRef);
    item.free_models.forEach(modelRef);
    for (const windows of [item.spending_windows, item.free_spending_windows]) {
      assert.ok(Array.isArray(windows));
      assert.ok(windows.length <= 2);
      if (windows[0]) assert.ok([604800, 2592000].includes(windows[0].duration_seconds));
      if (windows[1]) assert.equal(windows[1].duration_seconds, 18000);
    }
    for (const key of [
      "primary_cost_limit_usd",
      "weekly_cost_limit_usd",
      "free_primary_cost_limit_usd",
      "free_weekly_cost_limit_usd",
    ])
      if (item[key] !== null) decimal(item[key]);
    assert.equal("id" in planWrite(item), false);
    assert.equal("updated_at_ms" in planWrite(item), false);
  }
});
test("plan writes select model identity fields from independent model list DTOs", () => {
  const model = fixture.models.items[0];
  const plan = { ...fixture.plans.items[0], models: [model], free_models: [model] };
  const expected = [{ provider_id: model.provider_id, model: model.model }];
  assert.deepEqual(planWrite(plan).models, expected);
  assert.deepEqual(planWrite(plan).free_models, expected);
});

test("actual model metadata does not leak into write requests", () => {
  for (const item of fixture.models.items) {
    modelRef(item);
    assert.ok(["verified", "unavailable", "not_applicable"].includes(item.codex_metadata_status));
    for (const price of item.token_prices)
      for (const key of ["input_rate", "cached_rate", "cache_write_rate", "output_rate"])
        decimal(price[key]);
    assert.equal("codex_metadata_status" in modelWrite(item), false);
    assert.equal("codex_metadata_source" in modelWrite(item), false);
  }
});
test("actual usage distinguishes billed cost, unknown billing, and both windows", () => {
  const usage = fixture.usage;
  assert.equal("consumers" in usage, false);
  assert.equal("suppliers" in usage, false);
  number(usage.page);
  number(usage.page_size);
  number(usage.total);
  assert.equal(usage.records[0].subject_kind, "virtual_account");
  assert.equal(usage.records[0].cost_nano_usd, 14100000);
  const { quota, summary } = fixture.consumer_usage;
  assert.equal(quota.billing.used_usd, "0.0141");
  for (const key of ["pending_requests", "unpriced_requests", "legacy_requests"])
    number(quota.billing[key]);
  assert.equal(quota.rate_limit.primary_window.limit_window_seconds, 18000);
  assert.equal(quota.rate_limit.secondary_window.limit_window_seconds, 604800);
  for (const window of [quota.rate_limit.primary_window, quota.rate_limit.secondary_window]) {
    assert.ok(Number.isInteger(window.used_percent));
    decimal(window.used_usd);
    decimal(window.limit_usd);
    number(window.reset_at);
  }
  assert.equal(summary.daily_usage_buckets[0].tokens, 1100);
});
test("actual nested task and subscription records preserve nonempty business details", () => {
  const task = taskRecord(fixture.task_records.items[0]);
  assert.equal(task.title, "Recorded test task");
  assert.equal(task.status, "completed");
  string(task.source);
  const rows = fixture.subscription_records.items.map(subscriptionRecord);
  for (const operation of ["grant", "renew", "change_plan", "change_expiry"]) {
    const row = rows.find((row) => row.operation === operation && row.origin === "admin");
    assert.ok(row);
    assert.equal(row.expires_at, "2027-09-22T00:00:00Z");
    assert.equal(row.plan_name, "Plus");
  }
});
test("actual client state remains read only and diagnostics expose actual counters", () => {
  const state = fixture.client_state;
  assert.equal(canEditConfig(state.key, false), false);
  assert.equal(state.value.branch_format, "codex/{task_id}");
  assert.equal(state.write_origin, "client");
  assert.ok(Array.isArray(state.fields));
  const diagnostic = fixture.diagnostics.items[0];
  assert.equal(diagnostic.record_count, 1);
  assert.equal(diagnostic.attempts, 1);
  assert.ok(Array.isArray(diagnostic.summaries));
});
test("actual task execution and operation records include model and rejection reason", () => {
  const execution = fixture.task_execution_records.items[0];
  string(execution.value.task_id);
  string(execution.value.model);
  string(execution.source);
  const operation = fixture.task_operation_records.items[0];
  assert.equal(operation.value.operation, "follow_up");
  assert.equal(operation.value.reason, "task_model_unavailable");
  string(operation.value.task_id);
  string(operation.value.status);
});
