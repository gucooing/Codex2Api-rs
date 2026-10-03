import test from "node:test";
import assert from "node:assert/strict";
import {
  commonSupplierTags,
  selectedSupplierProvider,
  supplierTagChecked,
  toggleSupplierSelection,
} from "../src/lib/supplier-selection.ts";

test("list selection spans pages and removing one page preserves other selected accounts", () => {
  let selected = toggleSupplierSelection([], ["a", "b"], true);
  selected = toggleSupplierSelection(selected, ["c", "d"], true);
  selected = toggleSupplierSelection(selected, ["c", "d"], true);
  assert.deepEqual(selected, ["a", "b", "c", "d"]);
  assert.deepEqual(toggleSupplierSelection(selected, ["a", "b"], false), ["c", "d"]);
});

test("mixed account tags stay distinct until an explicit replacement is chosen", () => {
  const accounts = [
    { provider_id: "chatgpt", tag_ids: ["common", "a"] },
    { provider_id: "chatgpt", tag_ids: ["common", "b"] },
  ];
  assert.deepEqual(commonSupplierTags(accounts), ["common"]);
  assert.equal(supplierTagChecked(accounts, "a"), "indeterminate");
  assert.equal(supplierTagChecked(accounts, "common"), true);
  assert.equal(supplierTagChecked(accounts, "missing"), false);
  assert.equal(selectedSupplierProvider(accounts), "chatgpt");
  assert.equal(
    selectedSupplierProvider([...accounts, { provider_id: "other", tag_ids: [] }]),
    undefined,
  );
});
