'use strict';
const fs = require('node:fs'), vm = require('node:vm'), assert = require('node:assert/strict');
const {renderer, between} = require('./desktop-contract.cjs');

(async () => {
  const installed = await renderer(process.argv[2]);
  installed.bindings.za();
  const mainName = installed.unique('.vite/build', 'main-');
  const main = installed.read(mainName);
  // The main-process schema uses the same bundled Zod primitives. Bind real
  // embedded primitives, never a permissive parser stub.
  const b = installed.bindings;
  const enumPrimitive = Object.values(b).find(value => typeof value === 'function' &&
    value.toString().includes('type:`enum`,entries:'));
  assert(enumPrimitive && b.G && b.an && b.$ && b.po, 'Installed Zod primitives changed');
  const schema = vm.runInNewContext('(' + between(main, 'var $Te=', ',vT=').slice('var $Te='.length) + ')', {
    n: {wf: b.G, hf: b.an, Of: b.$, gf: b.po, df: enumPrimitive},
  });
  const input = JSON.parse(fs.readFileSync(0, 'utf8'));
  const parsed = schema.parse(input);
  assert.equal(parsed.accounts.length, 1);
  assert.equal(parsed.accounts[0].workspace_backend_origin, 'NO_CONSTRAINT');
  assert.equal(parsed.default_account_id, parsed.accounts[0].id);
  assert.equal(parsed.accounts[0].account_routing_override, 'NO_CONSTRAINT');
  for (const origin of ['https://chatgpt.com', 'https://proxy.invalid']) {
    assert.equal(schema.safeParse({...input, accounts: [{...input.accounts[0], workspace_backend_origin: origin}]}).success, false);
  }
  assert.equal(schema.safeParse({...input, accounts: [{...input.accounts[0], account_routing_override: 'invented'}]}).success, false);
  console.log(JSON.stringify({source: mainName, reader: 'actual Desktop workspace schema', identity: 'preserved'}));
})().catch(error => {console.error(error.name + ': ' + String(error.message).slice(0, 700)); process.exitCode = 1;});
