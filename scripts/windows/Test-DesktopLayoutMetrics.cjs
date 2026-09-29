'use strict';
const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const { renderer, between } = require('./desktop-contract.cjs');

(async () => {
  const source = await renderer(process.argv[2]);
  source.bindings.Zt();
  const sample = JSON.parse(fs.readFileSync(0, 'utf8'));
  const shared = source.read(source.unique('webview/assets', 'app-shared-'));
  const selector = shared.match(/\(\{get:([\w$]+)\}\)=>\1\(([\w$]+),`3528415127`\)\|\|!1/);
  assert(selector, 'Installed unified tab selector changed');
  const chooseLayout = vm.runInNewContext('(' + selector[0] + ')', { [selector[2]]: {} });
  const hash = value => [...value].reduce((result, character) => (Math.imul(result, 31) + character.charCodeAt(0)) >>> 0, 0).toString();
  for (const [response, expected] of [[sample.enabled, true], [sample.disabled, false]]) {
    const payload = JSON.parse(response.statsigPayload);
    assert.equal(chooseLayout({ get: (_, gate) => payload.feature_gates[hash(gate)]?.value ?? false }), expected);
  }
  const metricsFactory = vm.runInNewContext(
    between(source.text, 'function WSr(', 'var KSr,') + '; WSr',
    { navigator: { doNotTrack: '0' }, window: { addEventListener() {}, removeEventListener() {} }, setTimeout, clearTimeout,
      KSr: source.bindings.q({ success: source.bindings.j() }) }
  );
  const requests = [];
  const collector = metricsFactory({ fetcher: async (path, options) => {
    requests.push({ path, request: JSON.parse(options.body) });
    return { ok: true, json: async () => sample.metrics_response };
  } });
  collector.count('desktop', 'ready', { app_version: 'fixture' }, 2);
  collector.hist('desktop', 'startup_ms', {}, 123);
  await collector.flush();
  await collector.flush();
  collector.dispose();
  assert.equal(requests.length, 1, 'Successful metrics were retried');
  assert.equal(requests[0].path, '/ces/statsc/flush');
  assert.equal(requests[0].request.counters[0].value, 2);
  assert.equal(requests[0].request.histograms[0].values[0], 123);
  const environmentReader = vm.runInNewContext(
    between(source.text, 'async function spn()', 'function cpn(') + '; spn',
    { Gc: { safeGet: async path => { assert.equal(path, '/wham/environments'); return sample.environments; } } }
  );
  assert.deepEqual(await environmentReader(), sample.environments);
  assert.equal(sample.environments[0].repo_map[sample.environments[0].repos[0]].clone_url, 'https://github.com/fixture/project.git');
  console.log('PASS: installed Desktop layout selector, metrics construction/success reader and environment list reader.');
})().catch(error => { console.error(error.message); process.exitCode = 1; });
