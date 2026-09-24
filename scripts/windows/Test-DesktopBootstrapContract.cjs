'use strict';
// Execute selectors from the installed Desktop, not the reference CLI sources.
const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const { isDeepStrictEqual } = require('node:util');
const archive = fs.readFileSync(process.argv[2]);
const size = archive.readUInt32LE(4), header = JSON.parse(archive.subarray(16, 16 + archive.readUInt32LE(12)));
function source(entries, prefix) {
  const candidates = Object.entries(entries).filter(([name]) => name.startsWith(prefix) && name.endsWith('.js'));
  assert.equal(candidates.length, 1, 'Installed Desktop module layout changed');
  const entry = candidates[0][1], start = 8 + size + Number(entry.offset);
  return archive.subarray(start, start + entry.size).toString('utf8');
}
function slice(text, start, end) {
  const a = text.indexOf(start), b = text.indexOf(end, a);
  assert(a >= 0 && b > a, `Installed Desktop contract changed: ${start}`);
  return text.slice(a, b);
}
const main = source(header.files['.vite'].files.build.files, 'main-');
const renderer = source(header.files.webview.files.assets.files, 'app-initial-');
// Load the SDK actually embedded in Desktop, rather than assuming every 200
// bootstrap response becomes a ready client.
const sdkSource = slice(main, 'Ew=e.t(', 'nT={gates:').replace(/,\s*$/, '');
const sdk = vm.runInNewContext('var ' + sdkSource + ';tT', {
  e: { t(factory) { let module; return () => { if (!module) { module = { exports: {} }; factory(module.exports, module); } return module.exports; }; } },
  console, setTimeout, clearTimeout, setInterval, clearInterval, URL, TextEncoder, TextDecoder, AbortController, performance,
});
const identityMatches = vm.runInNewContext(slice(main, 'function cT(', 'var uT=') + ';cT');
const gateNames=vm.runInNewContext('('+slice(renderer,'j_={','}})))()').slice(3)+'})');
const publish = vm.runInNewContext(slice(renderer, 'function r6c(', 'var a6c,') + ';r6c', {
  // SDK dependencies: empty test feature configuration has no custom overrides.
  V3c: () => ({}), H3c: () => undefined, R3c: () => ({ gates: {}, configs: {} }),
  j_: gateNames, o6c: { default: isDeepStrictEqual },
});
const readSettings = vm.runInNewContext(slice(renderer, 'async function ydn(', 'function bdn(') + ';ydn', {
  Ar: { defaultModeRequestUserInput: { key: 'ask' } }, _se: { conversationDetailMode: { key: 'detail' } },
  Zme: values => values, ra: value => value, cdn: value => value,
});
const sample = JSON.parse(fs.readFileSync(0, 'utf8'));
const payload = JSON.parse(sample.response.statsigPayload);
const identity = { principal: { userId: sample.userId, accountId: sample.accountId }, authMethod: 'chatgpt' };
function publication(value) {
  const client = new sdk.StatsigClient('client-fixture', value.user, {disableStorage:true,loggingEnabled:'disabled',networkConfig:{preventAllNetworkTraffic:true}});
  client.dataAdapter.setData(JSON.stringify(value));
  assert.equal(client.initializeSync({disableBackgroundCacheRefresh:true}).success,true);
  assert.equal(client.getLayer('72216192').get('enable_i18n',false),true);
  assert.equal(client.checkGate('2478676115'),true);
  assert.equal(client.getLayer('3503973010').get('show_dropdown_entry_point',false),true);
  try { return publish(client); } finally { client.shutdown(); }
}
async function settings(value) {
  const published = publication(value), matches = identityMatches(published, value.user, identity);
  return readSettings({ read: async key => ({ effective: key === 'ask' ? false : 'STEPS_COMMANDS' }) }, {
    readExecutionAssignments: async () => matches ? { ...published, identity, values: published.executionValues } : null,
  }, { model: null, includeDeveloperInstructions: true });
}
(async () => {
  const old = { ...payload, user: { userID: sample.userId, email: payload.user.email } };
  assert.equal(identityMatches(publication(old), old.user, identity), false);
  assert.equal((await settings(old)).reason, 'execution-config-loading');
  assert.equal(identityMatches(publication(payload), payload.user, identity), true);
  assert.equal((await settings(payload)).status, undefined, 'Desktop must leave the not-ready creation state');
  assert.equal(payload.user.customIDs.stableID, sample.stableId);
  const effect = slice(renderer, 'h=()=>{let e=i.getContext().user;e.custom?.desktop_app_beta_enabled', ',g=[i,d]').slice(2);
  let refreshes=0;
  const runBetaEffect=user=>vm.runInNewContext('('+effect+')()', {i:{getContext:()=>({user}),updateUserAsync:()=>{refreshes++;return Promise.resolve({success:true});}},d:false,A6c:()=>{}});
  runBetaEffect(payload.user);assert.equal(refreshes,0,'Bootstrap must not immediately force a second identity fetch');
  const previous=structuredClone(payload.user);delete previous.custom.desktop_app_beta_enabled;
  runBetaEffect(previous);assert.equal(refreshes,1,'Old response reproduces the unnecessary desktop beta identity refresh');
  const foreign = { ...identity, principal: { ...identity.principal, accountId: 'another-account' } };
  assert.equal(identityMatches(publication(payload), payload.user, foreign), false);
  console.log('Installed Desktop: old bootstrap reproduces execution-config-loading; corrected virtual identity passes creation-settings checks.');
})().catch(error => { console.error(error.message); process.exitCode = 1; });
