'use strict';
const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');
const archive=fs.readFileSync(process.argv[2]);
const header=JSON.parse(archive.subarray(16,16+archive.readUInt32LE(12))),offset=8+archive.readUInt32LE(4);
function source(entries,prefix){const matches=Object.entries(entries).filter(([name])=>name.startsWith(prefix)&&name.endsWith('.js'));assert.equal(matches.length,1);const e=matches[0][1],a=offset+Number(e.offset);return archive.subarray(a,a+e.size).toString();}
function slice(text,start,end){const a=text.indexOf(start),b=text.indexOf(end,a);assert(a>=0&&b>a,`Installed contract changed: ${start}`);return text.slice(a,b);}
const renderer=source(header.files.webview.files.assets.files,'app-initial-');
const main=source(header.files['.vite'].files.build.files,'main-');
const sdk=vm.runInNewContext('var '+slice(main,'Ew=e.t(','nT={gates:').replace(/,\s*$/,'')+';tT',{
 e:{t(factory){let m;return()=>{if(!m){m={exports:{}};factory(m.exports,m);}return m.exports;};}},console,setTimeout,clearTimeout,setInterval,clearInterval,URL,TextEncoder,TextDecoder,AbortController,performance,
});
const browser=vm.runInNewContext(slice(renderer,'function jWn(','var MWn')+';jWn');
const computer=vm.runInNewContext(slice(renderer,'function fHn(','var pHn')+';fHn');
const selectModels=vm.runInNewContext(slice(renderer,'function UXn(','var WXn')+slice(renderer,'function O9n(','function A9n(')+';O9n');
const agentSettings=source(header.files.webview.files.assets.files,'agent-settings-');
function hasReasoningSettings(gate){
 const jsx=(type,props)=>({type,props});
 const component=vm.runInNewContext(slice(agentSettings,'function br(','function xr(')+';br',{
  Rr:{c:n=>Array(n).fill(Symbol.for('react.memo_cache_sentinel'))},je:()=>({selectedHostId:'local'}),ne:()=>({kind:'local'}),se:()=>[],Ot:'config',de:()=>false,vt:'notices',Lt:name=>name==='3693343337'&&gate,gn:'runtime',
  $:{jsx,jsxs:jsx,Fragment:'fragment'},Wt:'title',r:'text',xr:()=>{},pr:'reasoning-settings',sr:'experimental',Oe:'settings',
 });
 const tree=component();return JSON.stringify(tree).includes('reasoning-settings');
}
const sample=JSON.parse(fs.readFileSync(0,'utf8'));
const browserInputs={areRequirementsPending:false,isBrowserAndComputerUseAllowed:true,isExternalBrowserUseFeatureEnabled:true,isExternalBrowserUseFeatureLoading:false,runCodexInWsl:false,windowType:'electron'};
const computerInputs={areRequirementsPending:false,areRequiredFeaturesEnabled:true,enabled:true,isBrowserAndComputerUseAllowed:true,isAnyFeatureLoading:false,isHostCompatiblePlatform:true,isPlatformLoading:false,windowType:'electron'};
for(const [response,enabled] of [[sample.enabled,true],[sample.disabled,false]]){
 const payload=JSON.parse(response.statsigPayload),client=new sdk.StatsigClient('test-client',payload.user,{disableStorage:true,loggingEnabled:'disabled',networkConfig:{preventAllNetworkTraffic:true}});
 client.dataAdapter.setData(JSON.stringify(payload));assert(client.initializeSync({disableBackgroundCacheRefresh:true}).success);
 try{
  const browserGate=client.checkGate('410065390'),computerGate=client.checkGate('1506311413');assert.equal(browserGate,enabled);assert.equal(computerGate,enabled);
  assert.equal(browser({...browserInputs,isExternalBrowserUseGateEnabled:browserGate}),enabled?'available':'statsig-disabled');
  assert.equal(computer({...computerInputs,isComputerUseGateEnabled:computerGate}),enabled?'available':'statsig-disabled');
  assert.equal(browser({...browserInputs,isExternalBrowserUseGateEnabled:true,isBrowserAndComputerUseAllowed:false}),'config-requirement-disabled');
  assert.equal(computer({...computerInputs,isComputerUseGateEnabled:true,areRequiredFeaturesEnabled:false}),'config-requirement-disabled');
  assert.equal(client.checkGate('3693343337'),enabled);
  assert.equal(client.checkGate('1186680773'),enabled);
  assert.equal(hasReasoningSettings(client.checkGate('3693343337')),enabled);
  const models=process.env.CODEX2API_TEST_NATIVE_MODELS?JSON.parse(fs.readFileSync(process.env.CODEX2API_TEST_NATIVE_MODELS,'utf8')):[{model:'fixture-model',hidden:false,isDefault:true,supportedReasoningEfforts:['low','max','ultra'].map(reasoningEffort=>({reasoningEffort}))}];
  const filtered=selectModels({authMethod:'chatgpt',availableModels:new Set(),defaultModel:models[0].model,enabledReasoningEfforts:new Set(['none','minimal','low','medium','high','xhigh','max','ultra','persistent']),hasConfiguredModelCatalog:false,includeUltraReasoningEffort:client.checkGate('1186680773'),models,useHiddenModels:false});
  assert.equal(filtered.hasModelSupportingMaxReasoningEffort,true);
  assert.equal(filtered.hasModelSupportingUltraReasoningEffort,enabled);
  assert.equal(filtered.models.some(m=>m.supportedReasoningEfforts.some(e=>e.reasoningEffort==='ultra')),enabled);
 }finally{client.shutdown();}
}
// Execute the installed parent-controls component's real no-membership/error branches.
const settings=source(header.files.webview.files.assets.files,'settings-8d6c2878e79a');
function renderFamily(data,isError){
 const jsx=(type,props)=>({type,props});
 const component=vm.runInNewContext(slice(settings,'function Zt(','function Qt(')+';Zt',{
  on:{c:n=>Array(n).fill(Symbol.for('react.memo_cache_sentinel'))},Ee:()=>({current:{}}),de:()=>({data,isPending:false,isError,refetch:()=>{}}),$: {jsx,jsxs:jsx,Fragment:'fragment'},an:'loading',Ge:'load-error',Qt:'no-membership',en:'members',
 });return component({userId:'test-user',accountId:'test-account'});
}
assert.equal(renderFamily(undefined,true).type,'load-error');
assert.equal(renderFamily(sample.family,false).props.children[1].type,'no-membership');
assert.equal(renderFamily(sample.populated_family,false).props.children[1].type,'members');
console.log('Actual Desktop SDK and availability selectors: enabled controls available, explicit denials preserved; family response reaches the normal empty or member branch.');
(async()=>{
 let preferences=['low','medium','high','xhigh'];
 const toggle=vm.runInNewContext(slice(renderer,'async function TLa(','async function ELa(')+';TLa',{
  Ky:()=>preferences,Ar:{enabledReasoningEfforts:'enabledReasoningEfforts'},qy:async(_scope,key,value)=>{assert.equal(key,'enabledReasoningEfforts');preferences=Array.from(value);},
 });
 await toggle({get:()=>{}},{enabled:true,hostId:'local',reasoningEffort:'ultra'});
 assert.deepEqual(preferences,['low','medium','high','xhigh','ultra']);
 console.log('Actual Desktop settings component renders reasoning controls; native model capabilities survive filtering and the original toggle writes the client preference.');
})().catch(error=>{console.error(error);process.exitCode=1;});
