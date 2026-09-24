'use strict';
// Run the installed renderer's actual selectors against responses from the test proxy.
const fs = require('node:fs'), vm = require('node:vm'), assert = require('node:assert/strict');
const archive = fs.readFileSync(process.argv[2]);
const hs = archive.readUInt32LE(4), js = archive.readUInt32LE(12);
const header = JSON.parse(archive.subarray(16, 16 + js));
const assets = header.files.webview.files.assets.files;
const candidates = Object.entries(assets).filter(([name, entry]) => {
  if (!name.startsWith('queries-') || !name.endsWith('.js') || entry.unpacked) return false;
  return read(entry).includes('/wham/usage/daily-token-usage-breakdown');
});
function read(entry) {
  return archive.subarray(8 + hs + Number(entry.offset), 8 + hs + Number(entry.offset) + entry.size).toString('utf8');
}
assert.equal(candidates.length, 1, 'Installed usage queries changed');
const source = read(candidates[0][1]);
const start = source.indexOf('function S('), end = source.indexOf('var L,R,z,B;', start);
assert(start >= 0 && end > start, 'Installed usage selectors changed');
const selectors = vm.runInNewContext(source.slice(start, end) + ';({tokens:C,credits:S,counts:k,metrics:E})', {
  B: {desktop:['CODEX_DESKTOP_APP','CODEX_WORK_DESKTOP'],cli:['CODEX_CLI']},
});
const values = JSON.parse(fs.readFileSync(0, 'utf8'));
const from = '2026-09-12', until = '2026-09-18';
const tokenView = selectors.tokens(values.tokens);
assert.equal(tokenView.units, 'tokens');
const reported = values.tokens.data.reduce((sum, day) => sum + day.models.reduce((sum, model) => sum + model.credits, 0), 0);
assert.equal(tokenView.total, reported);
assert.equal(tokenView.byModel.total, reported);
assert.equal(selectors.counts(values.counts, from, until).byModel.total, 0);
assert.equal(selectors.credits(values.credits, from, until, 'product').total, 0);
assert.equal(selectors.metrics(values.plugins, from, until, 'consumer').total, 0);
assert.equal(selectors.metrics(values.skills, from, until, 'consumer').total, 0);
assert.equal(values.plan.coverage_complete, false);
assert.deepEqual(values.plan.periods, []);
console.log('Installed desktop usage selectors accepted all responses; displayed tokens: ' + tokenView.total);
