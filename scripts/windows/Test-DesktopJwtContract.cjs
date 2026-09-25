'use strict';
// Execute the installed token readers. Only local fixture JWTs enter this test.
const fs=require('node:fs'),assert=require('node:assert/strict');
const {renderer,between}=require('./desktop-contract.cjs');
(async()=>{
  const sample=JSON.parse(fs.readFileSync(0,'utf8'));
  const installed=await renderer(process.argv[2]);
  installed.bindings.za();
  const enumPrimitive=Object.values(installed.bindings).find(value=>typeof value==='function'&&value.toString().includes('type:`enum`,entries:'));
  const initializer=installed.text.match(/BRt=(Es\(`[^`]+`\.split\(`\.`\)\))/)?.[1];
  assert(initializer&&enumPrimitive,'Installed plan reader changed');
  const planSchema=new Function('Es',`return ${initializer};`)(enumPrimitive);
  const read=new Function('BRt',between(installed.text,'function zg(','function DRt(')+';return zg;')(planSchema);
  const main=installed.read(installed.unique('.vite/build','main-'));
  const readMain=new Function('J','jTe',between(main,'async function cT(','var lT=')+';return cT;')('local',()=>({error:()=>{}}));
  for(const token of [sample.access_token,sample.refreshed_access_token]) {
    const value=read(token);
    assert.equal(value.accountId,sample.account_id);assert.equal(value.userId,sample.user_id);
    assert.equal(value.accountUserId,sample.user_id);assert.equal(value.email,sample.email);
    assert.equal(value.planType,sample.plan_type);assert.equal(value.computeResidency,'no_constraint');
    const backend=await readMain({getConnection:()=>({getAuthToken:async()=>token})});
    assert.equal(backend.accountId,sample.account_id);assert.equal(backend.userId,sample.user_id);
    assert.equal(backend.authenticatedUserId,sample.user_id);assert.equal(backend.email,sample.email);
    assert.equal(backend.hasChatGptToken,true);
  }
  console.log('Installed Desktop renderer and main process accepted local JWT identity and refresh claims.');
})().catch(error=>{console.error(error.message);process.exitCode=1;});
