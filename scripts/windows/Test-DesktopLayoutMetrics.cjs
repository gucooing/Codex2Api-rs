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
  const environmentReader = vm.runInNewContext(
    between(source.text, 'async function spn()', 'function cpn(') + '; spn',
    { Gc: { safeGet: async path => { assert.equal(path, '/wham/environments'); return sample.environments; } } }
  );
  assert.deepEqual(await environmentReader(), sample.environments);
  assert.equal(sample.environments[0].repo_map[sample.environments[0].repos[0]].clone_url, 'https://github.com/fixture/project.git');
  console.log('PASS: installed Statsig SDK, navigation/tab/composer selection with auth checks, reset-card rendering branch and environment readers.');
})().catch(error => { console.error(error.message); process.exitCode = 1; });
