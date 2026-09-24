'use strict';
const assert=require('node:assert/strict'),http=require('node:http'),https=require('node:https'),{createHash}=require('node:crypto');
const {proxyPolicy,installRequestRouting}=require('../../tools/desktop-proxy/AddressHook.cjs');
const server=http.createServer(async(req,res)=>{
  let body='';for await(const chunk of req)body+=chunk;
  if(req.url.endsWith('/redirect')){res.writeHead(307,{location:'https://chatgpt.com/backend-api/final'});return res.end();}
  if(req.url.endsWith('/redirect-get')){res.writeHead(303,{location:'https://api.openai.com/v1/final'});return res.end();}
  res.setHeader('content-type','application/json');res.end(JSON.stringify({path:req.url,method:req.method,body,auth:req.headers.authorization,host:req.headers.host,routing:req.headers['x-openai-account-routing-override']}));
});
let socketPath;
server.on('upgrade',(req,socket)=>{socketPath=req.url;const accept=createHash('sha1').update(req.headers['sec-websocket-key']+'258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest('base64');socket.write('HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: '+accept+'\r\n\r\n');socket.on('data',()=>{socket.end(Buffer.from([0x88,0]));});});
(async()=>{
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  const root=`http://127.0.0.1:${server.address().port}/custom/proxy`,policy=proxyPolicy(root);
  let beforeRequest;
  const nativeCalls=[];
  const nativeFetch=(input,init)=>{nativeCalls.push({input,init});return Promise.resolve(new Response('native'));};
  const session={fetch:nativeFetch,webRequest:{onBeforeRequest:(_,listener)=>{beforeRequest=listener;}}};
  const electron={net:{fetch:nativeFetch,request:options=>options},app:{on(){},whenReady:()=>Promise.resolve()},session:{defaultSession:session}};
  installRequestRouting(policy,name=>name==='electron'?electron:require(name));
  for (const internal of [{hostname:'::1',port:32123,path:'/rpc'}, {socketPath:'fixture-local-pipe',path:'/rpc'}]) assert.equal(electron.net.request(internal),internal);
  await Promise.resolve();
  for(const nativeUrl of ['http://127.0.0.1:32123/browser/rpc','http://localhost:32123/browser/rpc','http://[::1]:32123/browser/rpc','chrome-extension://fixture/internal']){
    const init={method:'POST',body:new ReadableStream({start(c){c.enqueue(new Uint8Array([1]));c.close();}}),nativeOption:{keep:true}};
    await electron.net.fetch(nativeUrl,init);await session.fetch(nativeUrl,init);
    for(const call of nativeCalls.splice(0)){assert.equal(call.input,nativeUrl);assert.equal(call.init,init);}
  }
  const options={method:'POST',headers:{authorization:'Bearer fixture','x-openai-account-routing-override':'us','content-type':'application/json'},body:'{"data":"unchanged"}'};
  for(const source of ['https://chatgpt.com/backend-api/new-endpoint?x=1','https://auth.openai.com/oauth/revoke','https://api.openai.com/v1/responses','https://cdn.oaistatic.com/new-asset']){
    const response=await fetch(source,options),result=await response.json();
    assert.equal(result.path,new URL(policy.requestUrl(source)).pathname+new URL(source).search);
    assert.equal(result.body,options.body);assert.equal(result.auth,'Bearer fixture');assert.equal(result.routing,undefined);
  }
  const redirected=await(await fetch('https://chatgpt.com/backend-api/redirect',options)).json();
  assert.equal(redirected.path,'/custom/proxy/backend-api/final');assert.equal(redirected.body,options.body);
  const get=await(await fetch('https://chatgpt.com/backend-api/redirect-get',options)).json();assert.equal(get.method,'GET');assert.equal(get.body,'');
  const local=await(await fetch(`http://127.0.0.1:${server.address().port}/internal-rpc`,options)).json();assert.equal(local.path,'/internal-rpc');assert.equal(local.body,options.body);
  const proxyRedirect=await(await fetch(root+'/backend-api/redirect',options)).json();assert.equal(proxyRedirect.path,'/custom/proxy/backend-api/final');
  const post=await new Promise((resolve,reject)=>{const req=https.request('https://chatgpt.com/backend-api/https-request',{method:'POST',headers:{authorization:'Bearer fixture'}},res=>{let text='';res.on('data',c=>text+=c);res.on('end',()=>resolve(JSON.parse(text)));});req.on('error',reject);req.end('stream-body');});
  assert.equal(post.path,'/custom/proxy/backend-api/https-request');assert.equal(post.body,'stream-body');assert.equal(post.auth,'Bearer fixture');
  const {Worker}=require('node:worker_threads');
  const worker=new Worker("fetch('https://chatgpt.com/backend-api/worker').then(r=>r.json()).then(v=>require('node:worker_threads').parentPort.postMessage({v,explicitOption:process.execArgv.includes('--no-warnings')}));",{eval:true,execArgv:['--no-warnings']});
  const fromWorker=await new Promise((resolve,reject)=>{worker.once('message',resolve);worker.once('error',reject);});await worker.terminate();assert.equal(fromWorker.v.path,'/custom/proxy/backend-api/worker');assert.equal(fromWorker.explicitOption,true);
  const ws=new WebSocket('wss://chatgpt.com/backend-api/codex/responses');await new Promise((resolve,reject)=>{ws.onopen=resolve;ws.onerror=reject;});assert.equal(socketPath,'/custom/proxy/backend-api/codex/responses');ws.close();
  const electronRequest=electron.net.request({url:'https://chatgpt.com/backend-api/test',method:'POST'});assert.equal(electronRequest.hostname,'127.0.0.1');assert.equal(electronRequest.path,'/custom/proxy/backend-api/test');
  const redirect=await new Promise(resolve=>beforeRequest({url:'https://chatgpt.com/ces/v1/rgstr'},resolve));assert.equal(redirect.redirectURL,root+'/ces/v1/rgstr');
  let calls=0;session.webRequest.onBeforeRequest({urls:['https://example.test/*']},(_,done)=>{calls++;done({cancel:true});});
  assert.equal((await new Promise(resolve=>beforeRequest({url:'https://example.test/page'},resolve))).cancel,true);
  assert.equal((await new Promise(resolve=>beforeRequest({url:'https://chatgpt.com/backend-api/x'},resolve))).redirectURL,root+'/backend-api/x');assert.equal(calls,1);
  server.closeAllConnections();server.close();
  console.log('Actual fetch, redirects, Node HTTPS request, WebSocket and Electron session routing passed; method/body/auth preserved and unrelated cancellation retained.');
})().catch(error=>{console.error(error);server.closeAllConnections();server.close();process.exit(1);});
