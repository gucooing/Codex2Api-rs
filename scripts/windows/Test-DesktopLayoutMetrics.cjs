'use strict';
const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const { renderer, between, statsigSdk } = require('./desktop-contract.cjs');

(async () => {
  const source = await renderer(process.argv[2]);
  source.bindings.Zt();
  const sample = JSON.parse(fs.readFileSync(0, 'utf8'));
  const shared = source.read(source.unique('webview/assets', 'app-shared-'));
  const selector = shared.match(/\(\{get:([\w$]+)\}\)=>\1\(([\w$]+),`3528415127`\)\|\|!1/);
  assert(selector, 'Installed unified tab selector changed');
  const chooseLayout = vm.runInNewContext('(' + selector[0] + ')', { [selector[2]]: {} });
  const sdk = statsigSdk(source);
  const primary = source.read(source.unique('webview/assets', 'app-primary-'));
  const usage = source.read('webview/assets/page-e25392cd6e63.js');
  const composerSource = between(primary, 'function cbt(', 'function TX(');
  const navigationSource = between(source.text, 'Kv=Di(X,', '})})))()').slice('Kv=Di(X,'.length) + '}';
  const resetSectionSource = between(usage, 'function zs(', 'function Bs(');
  for (const [response, expected] of [[sample.enabled, true], [sample.disabled, false]]) {
    const payload = JSON.parse(response.statsigPayload);
    const client = new sdk.StatsigClient('client-layout-fixture', payload.user, {
      disableStorage:true, loggingEnabled:'disabled', networkConfig:{preventAllNetworkTraffic:true},
    });
    client.dataAdapter.setData(JSON.stringify(payload));
    assert(client.initializeSync({disableBackgroundCacheRefresh:true}).success);
    try {
      const gate = value => client.checkGate(value);
      const navigation = vm.runInNewContext('(' + navigationSource + ')', {Xf:'gate',Wr:'access'});
      const chooseNavigation = access => navigation({get:(key, name)=>key==='gate'?gate(name):access});
      assert.equal(chooseNavigation({status:'allowed'}), expected);
      assert.equal(chooseNavigation({status:'denied',reason:'unsupported-auth'}), false);
      assert.equal(chooseLayout({ get: (_, name) => gate(name) }), expected);
      const composer = vm.runInNewContext(composerSource + ';cbt', {ci:gate});
      assert.equal(composer({legacy:'old',unified:'new'}), expected ? 'new' : 'old');
      const jsx = (type, props) => ({type, props});
      const resets = vm.runInNewContext(resetSectionSource + ';zs', {
        A:gate, Vs:{c:n=>Array(n).fill(Symbol.for('react.memo_cache_sentinel'))},
        Hs:{jsx,jsxs:jsx,Fragment:'fragment'}, Bs:'agent-setting', Ms:'reset-cards',
      });
      assert.equal(resets({}).props.children[1]?.type === 'reset-cards', expected);
    } finally { client.shutdown(); }
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
  console.log('PASS: installed Statsig SDK, navigation/tab/composer selection with auth checks, reset-card rendering branch, metrics and environment readers.');
})().catch(error => { console.error(error.message); process.exitCode = 1; });
