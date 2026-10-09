"use strict";
// Execute the installed catalog and tier selectors in a separate test process.
const fs = require("node:fs");
const assert = require("node:assert/strict");
const { renderer } = require("./desktop-contract.cjs");

(async () => {
  const installed = await renderer(process.argv[2]);
  function exported(name) {
    const alias = installed.sharedText.match(new RegExp(`\\b${name} as ([\\w$]+)`))?.[1];
    assert(alias && typeof installed.shared[alias] === "function", `Installed selector changed: ${name}`);
    return installed.shared[alias];
  }
  exported("PW")();
  exported("JN")();
  const sample = JSON.parse(fs.readFileSync(0, "utf8"));
  const selected = exported("Uxn")(sample.desktop_models);
  assert(selected.serviceTiersByModelSlug[sample.model].some(tier => tier.id === "ultrafast"));
  const model = sample.models.models.find(model => model.slug === sample.model);
  const nativeModel = { serviceTiers: model.service_tiers, defaultServiceTier: model.default_service_tier };
  assert.equal(exported("_jt")(nativeModel, "ultrafast"), true);
  assert.equal(exported("vjt")(nativeModel, "ultrafast"), "ultrafast");
  assert.equal(exported("qN")("ultrafast", "Ultrafast"), "ultrafast");
  assert.equal(exported("yjt")(nativeModel, "ultrafast"), "ultrafast");
  const other = sample.models.models.find(model => model.slug !== sample.model);
  if (other) assert.equal(exported("_jt")({ serviceTiers: other.service_tiers }, "ultrafast"), false);
  console.log(JSON.stringify({ reader: "installed Desktop catalog and tier selectors", model: sample.model, selectedTier: "ultrafast", source: installed.name }));
})().catch(error => { console.error(error.stack); process.exitCode = 1; });
