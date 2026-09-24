'use strict';
const assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const {proxyPolicy,transform,readTargets,bindNativeRouting}=require('../../tools/desktop-proxy/AddressHook.cjs');
const archive=process.argv[2];if(!archive)throw Error('Pass the installed app.asar.');
const proxy='http://127.0.0.1:8080/api/oauth/chatgpt',policy=proxyPolicy(proxy);
const targets=readTargets(archive),bytes=fs.readFileSync(archive),offset=8+bytes.readUInt32LE(4),tree=JSON.parse(bytes.subarray(16,16+bytes.readUInt32LE(12)));
const sources=Object.fromEntries(Object.entries(targets).map(([kind,file])=>{const entry=tree.files['.vite'].files.build.files[file.replaceAll('\\','/').split('/').pop()];const start=offset+Number(entry.offset);return[kind,bytes.subarray(start,start+entry.size).toString('utf8')];}));
for(const kind of ['main','native'])assert.throws(()=>transform('unrecognized client',kind));
const originalStart=sources.main.indexOf('isDesktopAuthAllowedUrl(e){'),originalEnd=sources.main.indexOf('isVsCodeFetchRequest(',originalStart);
const context={URL,a:{_t:()=>proxy},__codex2apiDesktopHook:{policy}};
const original=vm.runInNewContext('({'+sources.main.slice(originalStart,originalEnd)+'})',context);
const hooked=transform(sources.main,'main'),start=hooked.indexOf('isDesktopAuthAllowedUrl(e){'),end=hooked.indexOf('isVsCodeFetchRequest(',start);
const guard=vm.runInNewContext('({'+hooked.slice(start,end)+'})',context);
assert.equal(original.isDesktopAuthAllowedUrl(proxy+'/backend-api/wham/usage'),false);
assert.equal(guard.isDesktopAuthAllowedUrl(proxy+'/backend-api/wham/usage'),true);
for(const value of ['https://evil.test','http://127.0.0.1:8080/admin'])assert.equal(guard.isDesktopAuthAllowedUrl(value),original.isDesktopAuthAllowedUrl(value));
assert(!hooked.includes('policy.loginResponse'));
assert(!hooked.includes('__codex2apiLoginUrl'));
assert.equal(Object.hasOwn(targets,'bootstrap'),false);
const native=transform(sources.native,'native');
assert(native.includes('bindNative(this,this.messageDelivery)'));
assert(native.includes('nativeLaunch(await '));
const launchStart=native.indexOf('async connect(){let e=this.options.hostConfig.kind'),launchEnd=native.indexOf('getIoStatsSnapshot()',launchStart);
assert(launchStart>=0&&launchEnd>launchStart);
const launch=vm.runInNewContext('({'+native.slice(launchStart,launchEnd)+'})',{
  __codex2apiDesktopHook:{nativeLaunch:async options=>({...options,args:[...options.args,'mapped-addresses']})},
  vQ:async()=>({executablePath:'original-native',args:['app-server']}),s:{join:(...s)=>s.join('/')},gM:()=>'/default-home',process:{platform:'win32',env:{}},mQ:class{constructor(options){this.options=options;}}
});
(async()=>{
  const launched=await launch.connect.call({options:{hostConfig:{kind:'local'}},ioStatsTracker:{}});
  assert.equal(launched.options.executablePath,'original-native');
  assert.deepEqual(Array.from(launched.options.args),['app-server','mapped-addresses']);
  const sent=[],transport={};
  const saved={model_provider:'custom',model_providers:{custom:{name:'OpenAI',requires_openai_auth:true},external:{base_url:'https://third-party.test/v1'}}};
  const delivery={sendMessage:(message)=>sent.push(message)};
  const connection={options:{transport:{kind:'stdio'}},connection:transport,getUserSavedConfiguration:async()=>saved,routeIncomingMessage:e=>{throw Error(JSON.stringify(e));}};
  bindNativeRouting(connection,delivery,policy);
  const request={id:'foreground',method:'thread/start',params:{modelProvider:null,model:'chosen-model',config:{approval_policy:'on-request'},input:[{text:'https://chatgpt.com/not-an-address-setting'}]}};
  delivery.sendMessage(request);delivery.sendMessage({...request,id:'background',params:{...request.params,ephemeral:true}});
  delivery.sendMessage({id:'resume',method:'thread/resume',params:{config:null}});
  delivery.sendMessage({id:'login',method:'account/login/start',params:{type:'chatgpt',codexStreamlinedLogin:true,useHostedLoginSuccessPage:true}});
  await new Promise(resolve=>setImmediate(resolve));
  for(const id of ['foreground','background','resume']){
    const message=sent.find(v=>v.id===id);
    assert.equal(message.params.config['model_providers.custom.base_url'],proxy+'/backend-api/codex');
    assert.equal(Object.hasOwn(message.params.config,'model_providers.external.base_url'),false);
  }
  assert.equal(sent.find(v=>v.id==='foreground').params.model,'chosen-model');
  assert.equal(sent.find(v=>v.id==='foreground').params.config.approval_policy,'on-request');
  assert.equal(request.params.config['model_providers.custom.base_url'],undefined);
  assert.equal(sent.find(v=>v.id==='login').params.useHostedLoginSuccessPage,false);
  assert.equal(sent.find(v=>v.id==='login').params.codexStreamlinedLogin,true);
  for(const root of [proxy,'https://another.example:9443/custom/path']){
    const p=proxyPolicy(root);
    assert.equal(p.requestUrl('https://chatgpt.com/backend-api/new/unknown?x=1'),root+'/backend-api/new/unknown?x=1');
    assert.equal(p.requestUrl('https://api.openai.com/v1/responses'),root+'/v1/responses');
    assert.equal(p.requestUrl('https://auth.openai.com/oauth/token'),root+'/oauth/token');
    for(const host of ['api.oaistatsig.com','chatgpt-staging.com','client-events-service-analytics.openai.internal','oaisidekickupdates.blob.core.windows.net'])assert.equal(p.requestUrl('https://'+host+'/future-endpoint'),root+'/future-endpoint');
    assert.equal(p.requestUrl('wss://chatgpt.com/backend-api/codex/responses'),root.replace(/^http/,'ws')+'/backend-api/codex/responses');
    const url=new URL(p.requestUrl('https://chatgpt.com/codex/desktop-auth?authorize_url='+encodeURIComponent('https://auth.openai.com/oauth/authorize?state=s&code_challenge=p&redirect_uri=http%3A%2F%2Flocalhost%3A1455%2Fauth%2Fcallback')));
    assert.equal(url.origin,new URL(root).origin);const nested=new URL(url.searchParams.get('authorize_url'));
    assert.equal(nested.origin,new URL(root).origin);assert.equal(nested.searchParams.get('state'),'s');assert.equal(nested.searchParams.get('code_challenge'),'p');
    for(const unrelated of ['https://chatgpt.com.evil.test/x','https://example.test/x','http://localhost:1455/auth/callback?code=x'])assert.equal(p.requestUrl(unrelated),unrelated);
  }
  console.log('Installed native launch and shared RPC transport routing passed: custom providers, foreground/background/resume, original runtime and login options.');
})().catch(error=>{console.error(error);process.exit(1);});
