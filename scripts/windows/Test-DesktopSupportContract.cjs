'use strict';
const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');
const archive=fs.readFileSync(process.argv[2]),offset=8+archive.readUInt32LE(4),tree=JSON.parse(archive.subarray(16,16+archive.readUInt32LE(12)));
function source(prefix){const entries=Object.entries(tree.files['.vite'].files.build.files).filter(([name])=>name.startsWith(prefix)&&name.endsWith('.js'));assert.equal(entries.length,1);const e=entries[0][1],start=offset+Number(e.offset);return archive.subarray(start,start+e.size).toString('utf8');}
const main=source('main-'),bootstrap=source('bootstrap-');
const srcEntries=Object.entries(tree.files['.vite'].files.build.files).filter(([name,e])=>name.startsWith('src-')&&name.endsWith('.js')&&archive.subarray(offset+Number(e.offset),offset+Number(e.offset)+e.size).includes('/telemetry/intake'));
assert.equal(srcEntries.length,1);const entry=srcEntries[0][1],shared=archive.subarray(offset+Number(entry.offset),offset+Number(entry.offset)+entry.size).toString('utf8');
function slice(s,a,b){const i=s.indexOf(a),j=s.indexOf(b,i+a.length);assert(i>=0&&j>i,a);return s.slice(i,j);}
const context={
  e:{t(factory){let module;return()=>{if(!module){module={exports:{}};factory(module.exports,module);}return module.exports;};}},
  console,setTimeout,clearTimeout,setInterval,clearInterval,URL,TextEncoder,TextDecoder,AbortController,performance,fetch,CompressionStream,
};
const sdk=vm.runInNewContext('var '+slice(main,'Ew=e.t(','nT={gates:').replace(/,\s*$/,'')+';tT',context);
const sample=JSON.parse(fs.readFileSync(0,'utf8')),requests=[];
function capture(url,init){requests.push({path:new URL(url).pathname+new URL(url).search,headers:Object.fromEntries(new Headers(init.headers)),body:Buffer.from(typeof init.body==='string'?init.body:init.body??[]).toString('base64')});}
(async()=>{
  const options={disableStorage:true,loggingEnabled:'always',enableLiveValuesAutoRefresh:true,networkConfig:{api:'https://ab.chatgpt.com/v1',logEventUrl:'https://chatgpt.com/ces/v1/rgstr',sdkExceptionUrl:'https://ab.chatgpt.com/v1/sdk_exception',networkOverrideFunc:async(url,init)=>{capture(url,init);let value=url.includes('initialize')?(sample.refresh??sample.bootstrap):{success:true};if(typeof init.body==='string'&&JSON.parse(init.body).responseMode==='live_overlay')value={...value,response_mode:'live_overlay'};return new Response(JSON.stringify(value),{status:200,headers:{'content-type':'application/json'}});}}};
  const client=new sdk.StatsigClient('client-fixture',sample.bootstrap.user,options);
  client.dataAdapter.setData(JSON.stringify(sample.bootstrap));
  assert.equal(client.initializeSync({disableBackgroundCacheRefresh:true}).success,true);
  assert.equal(client._store.getLiveCursor().previousDerivedFields.codex2api_snapshot,sample.bootstrap.full_checksum);
  const initialized=await client.refreshValuesAsync();assert.equal(initialized.success,true);
  await client._refreshLiveValuesAsyncImpl();
  client.logEvent('codex_fixture_event',1,{private:'NOT_PERSISTED'});await client.flush();
  const boundary=new sdk.ErrorBoundary('client-fixture',options,()=>{});boundary.logError('fixture',new Error('PRIVATE_STACK'));
  await new Promise(resolve=>setImmediate(resolve));
  const init=requests.find(r=>r.path.startsWith('/v1/initialize'));assert(init,'SDK did not initialize');
  const initBody=JSON.parse(Buffer.from(init.body,'base64').toString());
  assert.equal(initBody.full_checksum,sample.bootstrap.full_checksum,'SDK must carry authenticated snapshot checksum');
  assert(requests.some(r=>r.path.startsWith('/ces/v1/rgstr')));
  assert(requests.some(r=>r.path.startsWith('/v1/sdk_exception')));
  if(sample.refresh)assert.equal(client.checkGate('1867347216'),false);
  context.window={btoa:value=>Buffer.from(value,'binary').toString('base64')};
  const encoded={isStatsigEncodable:true,body:JSON.stringify(initBody),params:{},urlConfig:{getUrl:()=>'/v1/initialize'}};
  client._network._tryEncodeBody(encoded);delete context.window;
  assert.equal(encoded.params.se,'1');capture('https://ab.chatgpt.com/v1/initialize?se=1',{headers:{'content-type':'application/json'},body:encoded.body});
  const event=requests.find(r=>r.path.startsWith('/ces/v1/rgstr'));
  client._network.setLogEventCompressionMode(sdk.LogEventCompressionMode.Forced);
  const compressed={sdkKey:'client-fixture',isCompressable:true,body:Buffer.from(event.body,'base64').toString(),params:{},urlConfig:{customUrl:'https://chatgpt.com/ces/v1/rgstr',getUrl:()=>'/ces/v1/rgstr'}};
  await client._network._tryToCompressBody(compressed);assert.equal(compressed.params.gz,'1');capture('https://chatgpt.com/ces/v1/rgstr?gz=1',{headers:{'content-type':'application/json'},body:compressed.body});
  await client.shutdown();
  // Execute the installed telemetry sender and its exact NDJSON/query construction.
  const makeUrl=vm.runInNewContext(slice(shared,'function I8(','var xre=')+';I8',{URLSearchParams,T8:'https://chat.openai.com/ces/v1/telemetry/intake',E8:'dummy-token'});
  const Sender=vm.runInNewContext('('+slice(shared,'F8=class','function ').slice(3).replace(/;\s*$/,'')+')',{
    w8:class{clear(){}},fetch,AbortController,setTimeout,clearTimeout,I8:makeUrl,T8:'https://chat.openai.com/ces/v1/telemetry/intake',E8:'dummy-token',j8:30000,P8:false,F$: 'web-sandbox.oaiusercontent.com',
  });
  const telemetry=new Sender({reportFailure:()=>{},fetchImpl:async(url,init)=>{capture(url,init);return new Response(null,{status:204});}});
  await telemetry.send(JSON.stringify({status:'info',logger:{name:'fixture'},message:'PRIVATE_MESSAGE',usr:{user_id:'untrusted-user'}}),'fixture-request');
  telemetry.dispose();
  const shell=sample.shell??'<script src="/assets/main-BFDC70j-.js"></script>';
  const assets=[];
  const prewarm=vm.runInNewContext(slice(main,'async function B$(','function V$(')+';B$',{G0e:8,F$:'web-sandbox.oaiusercontent.com',H$:async url=>{assets.push(url);return {ok:true};}});
  await prewarm(new Response(shell));assert(assets.length>0);assert(assets.every(url=>url.startsWith('https://web-sandbox.oaiusercontent.com/assets/')));
  // The real update reader rejects an HTTP transport before parsing the manifest.
  const update=vm.runInNewContext(slice(bootstrap,'function MT(','function PT(')+';NT',{URL,jT:{parse:value=>value}});
  await assert.rejects(()=>update({storeUpdateManifestUrl:'https://persistent.oaistatic.com/codex-app-prod/windows-store-update.json',fetch:async()=>({status:200,ok:true,url:'http://127.0.0.1/manifest',text:async()=>JSON.stringify(sample.manifest??{})})}),/must use https/);
  assert.equal(await update({storeUpdateManifestUrl:'https://persistent.oaistatic.com/codex-app-prod/windows-store-update.json',fetch:async()=>({status:404})}),null);
  if(process.argv[3]==='emit')console.log(JSON.stringify(requests));
  else console.log('Installed SDK checksum refresh, event/exception sender, NDJSON telemetry, MCP asset prewarm and HTTPS update check branches passed.');
})().catch(error=>{console.error(error);process.exit(1);});
