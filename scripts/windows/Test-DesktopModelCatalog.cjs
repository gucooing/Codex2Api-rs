"use strict";
// Execute the installed Desktop schema and picker without modifying the app.
const fs = require('node:fs'), vm = require('node:vm'), assert = require('node:assert/strict');
const {renderer, between} = require('./desktop-contract.cjs');
(async () => {
  const installed = await renderer(process.argv[2]);
  const source = installed.text, symbols = installed.bindings;
  assert(source.includes('RVt=`work_dogfood_default:`'), 'Installed model contract changed');
  symbols.za();
  const schema = between(source, 'RVt=`work_dogfood_default:`', '})))()}');
  const selectors = between(source, 'function yVt(', 'function X_(');
  const reader = vm.runInNewContext(schema + ';' + selectors +
    ';({parse:value=>YVt.parse(value),select:yVt})', {...symbols});
  const input = JSON.parse(fs.readFileSync(0, 'utf8')), sample = input.catalog ?? input;
  const selected = reader.select(reader.parse(sample)), slugs = sample.models.map(model => model.slug);
  assert.deepEqual(Array.from(new Set(Array.from(selected.options, option => option.slug))).sort(), [...slugs].sort());
  assert.equal(selected.defaultModelSlug, sample.default_model_slug);
  for (const slug of input.basic_models ?? slugs) {
    const config = selected.modelConfigBySlug[slug];
    assert.equal(config.thinkingEfforts, undefined);
    assert.equal(config.configurableThinkingEffort, undefined);
  }
  const empty = reader.select(reader.parse({...sample, models: [], versions: [], categories: [], internal_groups: [], slider_settings: [], default_model_slug: null}));
  assert.equal(empty.options.length, 0);
  console.log(JSON.stringify({source: installed.name, options: slugs, defaultModelSlug: selected.defaultModelSlug, emptyOptions: 0, reader: 'actual Desktop schema and selector'}));
})().catch(error => {console.error(error.name + ': ' + String(error.message).slice(0, 700)); process.exitCode = 1;});
