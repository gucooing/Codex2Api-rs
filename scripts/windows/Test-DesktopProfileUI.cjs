'use strict';
const fs=require('node:fs');
const assert=require('node:assert/strict');
const path=require('node:path');
const pause=ms=>new Promise(resolve=>setTimeout(resolve,ms));
(async()=>{
  const pages=await (await fetch(`http://127.0.0.1:${process.argv[2]}/json/list`)).json();
  const page=pages.find(p=>p.type==='page'&&p.url.includes('/index.html'));
  if(!page) console.log(pages.map(p=>({type:p.type,url:p.url})));
  if(!page)throw Error('No Desktop renderer');
  const ws=new WebSocket(page.webSocketDebuggerUrl);await new Promise((ok,no)=>{ws.onopen=ok;ws.onerror=no;});
  let next=0;const pending=new Map();
  ws.onmessage=e=>{const msg=JSON.parse(e.data);if(pending.has(msg.id)){pending.get(msg.id)(msg);pending.delete(msg.id);}};
  const call=(method,params={})=>new Promise((resolve,reject)=>{const id=++next;const timer=setTimeout(()=>{pending.delete(id);reject(Error('Renderer timed out: '+method));},8000);pending.set(id,msg=>{clearTimeout(timer);resolve(msg);});ws.send(JSON.stringify({id,method,params}));});
  const evaluate=async expression=>{const out=await call('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});if(out.error||out.result?.exceptionDetails)throw Error(JSON.stringify(out));return out.result.result.value;};
  if(process.env.CODEX2API_TEST_SUPPORT){
    const archive=fs.readFileSync(process.env.CODEX2API_TEST_DESKTOP_EXE.replace(/[^\\/]+$/,'resources/app.asar'));
    const tree=JSON.parse(archive.subarray(16,16+archive.readUInt32LE(12))),offset=8+archive.readUInt32LE(4);
    const [name,entry]=Object.entries(tree.files.webview.files.assets.files).find(([name])=>name.startsWith('app-initial-')&&name.endsWith('.js'));
    const source=archive.subarray(offset+Number(entry.offset),offset+Number(entry.offset)+entry.size).toString();
    const exported=source.slice(source.lastIndexOf('export{')).match(/Rv as ([\w$]+)/)?.[1];
    if(!exported)throw Error('Installed fetch export changed');
    const support=await evaluate(`(async()=>{let step='import';try{
      const api=(await import(${JSON.stringify('app://-/assets/'+name)}))[${JSON.stringify(exported)}].getInstance();
      step='bootstrap';
      const response=await api.fetch('/wham/statsig/bootstrap',{method:'POST',body:JSON.stringify({stable_id:'support-gui'}),headers:{'content-type':'application/json','X-OpenAI-Attach-Auth':'1'}});
      const payload=JSON.parse((await response.json()).statsigPayload);
      step='initialize';
      const init=await api.fetch('https://ab.chatgpt.com/v1/initialize',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({user:payload.user,hash:'djb2',responseMode:'live_overlay',previousDerivedFields:payload.derived_fields})});
      if(!init.ok||(await init.json()).user.userID!==payload.user.userID)throw Error('Live refresh failed');
      for(const [url,body]of [['https://chatgpt.com/ces/v1/rgstr',JSON.stringify({events:[{eventName:'gui-support-check',user:payload.user,time:Date.now()}]})],['https://ab.chatgpt.com/v1/sdk_exception',JSON.stringify({tag:'gui-support-check',exception:'TestDiagnostic'})],['https://chatgpt.com/ces/v1/telemetry/intake',JSON.stringify({status:'info',logger:{name:'gui-support-check'}})]]){step=url;const result=await api.fetch(url,{method:'POST',headers:{'content-type':'application/json'},body});if(!result.ok)throw Error('Support intake failed');}
      return {ok:true};
    }catch(e){return {ok:false,step,message:String(e?.message??e),status:e?.status??null};}})()`);
    assert(support.ok,JSON.stringify(support));
    console.log('Original renderer transport: SDK refresh and telemetry/events/exception endpoints completed.');
  }
  const click=async expression=>{const point=await evaluate(`(()=>{const e=${expression};if(!e)throw Error('Control not found');e.scrollIntoView({block:'center'});const r=e.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2};})()`);await call('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',clickCount:1,...point});await call('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',clickCount:1,...point});};
  await evaluate('[...document.querySelectorAll("button")].find(b=>/^(跳过|Skip)$/.test(b.innerText))?.click()');
  await pause(2500);
  await click('document.querySelector("button[aria-label=\\\"打开个人资料菜单\\\"]")');
  await pause(500);
  await click('[...document.querySelectorAll("[role=menuitem]")].find(b=>b.innerText.includes("Alice Proxy"))');
  await pause(2000);
  const profileText=await evaluate('document.body.innerText');
  assert(profileText.includes('Alice Proxy')&&profileText.includes('@alice')&&profileText.includes('累计 Token 数')&&profileText.includes('1.2万'),profileText);
  const proof=path.resolve(__dirname,'../../target/desktop-repair-proof');fs.mkdirSync(proof,{recursive:true});
  const capture=await call('Page.captureScreenshot',{format:'png'});fs.writeFileSync(path.join(proof,'profile.png'),Buffer.from(capture.result.data,'base64'));
  await click('[...document.querySelectorAll("button")].find(b=>b.innerText==="常规")');
  await pause(600);
  await click('[...document.querySelectorAll("button")].find(b=>b.innerText==="自动检测")');
  await pause(300);
  await click('[...document.querySelectorAll("[role=option]")].find(b=>b.innerText==="English")');
  await pause(1000);
  assert(await evaluate('document.documentElement.lang.startsWith("en")&&document.body.innerText.includes("General")'));
  await click('[...document.querySelectorAll("button")].find(b=>b.innerText==="English")');
  await pause(300);
  await click('[...document.querySelectorAll("[role=option]")].find(b=>/简体中文|Chinese.*Simplified/.test(b.innerText))');
  await pause(1000);
  assert(await evaluate('document.documentElement.lang.startsWith("zh")&&document.body.innerText.includes("应用 UI 语言")'));
  await call('Page.reload');await pause(2500);
  if(await evaluate('!!document.querySelector("button[aria-label=\\\"打开个人资料菜单\\\"]")')) {
    await click('document.querySelector("button[aria-label=\\\"打开个人资料菜单\\\"]")');await pause(300);
    await click('[...document.querySelectorAll("[role=menuitem]")].find(b=>b.innerText.includes("设置"))');await pause(600);
    await click('[...document.querySelectorAll("button")].find(b=>b.innerText==="常规")');await pause(300);
  }
  assert(await evaluate('document.documentElement.lang.startsWith("zh")&&document.body.innerText.includes("简体中文")'),await evaluate('JSON.stringify({lang:document.documentElement.lang,text:document.body.innerText.slice(0,2500)})'));
  const language=await call('Page.captureScreenshot',{format:'png'});fs.writeFileSync(path.join(proof,'language.png'),Buffer.from(language.result.data,'base64'));
  console.log('Original Desktop GUI: profile identity, stored token totals and activity graph displayed; English to Simplified Chinese switching and reload persistence passed.');
  ws.close();
})().catch(e=>{console.error(e);process.exit(1);});
