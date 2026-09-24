import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {pathToFileURL} from 'node:url';

const assets=path.join(process.argv[2],'readable','webview','assets');
const source=fs.readFileSync(path.join(assets,fs.readdirSync(assets).find(n=>/^app-initial-.*\.js$/.test(n))),'utf8');
const agent=fs.readFileSync(path.join(assets,fs.readdirSync(assets).find(n=>/^agent-settings-.*\.js$/.test(n))),'utf8');
const shared=await import(pathToFileURL(path.join(assets,fs.readdirSync(assets).find(n=>/^app-shared-.*\.js$/.test(n)))));shared.RL();
const sharedSource=fs.readFileSync(path.join(assets,fs.readdirSync(assets).find(n=>/^app-shared-.*\.js$/.test(n))),'utf8');
const input=JSON.parse(fs.readFileSync(0,'utf8'));
function slice(text,start,end){const a=text.indexOf(start),b=text.indexOf(end,a);assert(a>=0&&b>a,`Client contract changed: ${start}`);return text.slice(a,b);}
const symbols={};
for(const local of ['J','Z','Fr','Zr','$n','Ui','ca','ea','Mi','Ji']){const m=source.match(new RegExp('\\b(\\w+) as '+local.replace('$','\\$')+','));assert(m,local);symbols[local]=shared[m[1]];}
const context=vm.createContext(symbols);
vm.runInContext(slice(source,'(zy = J().trim()','(pln = {').trim().replace(/,$/,'')+';',context);
vm.runInContext(slice(source,'(gfr = Z({','(vfr = cf(').trim().replace(/,$/,'')+';',context);
vm.runInContext('(mfr=Fr([`default`,`blue`,`green`,`yellow`,`pink`,`orange`,`purple`,`black`]));'+slice(source,'(kfr = Z({','(dk = sf(').trim().replace(/,$/,'')+';',context);
const schema=context.kfr;
const old=schema.safeParse({settings:{voice_name:null},flags:{}});
assert.equal(old.success,false);
assert(old.error.issues.some(e=>e.path.join('.')==='settings.voice_name'));

const requests=[];
async function request(method,route,options={}){
 const url=new URL(input.base_url+route);
 for(const [key,value]of Object.entries(options.parameters?.query??{}))url.searchParams.set(key,String(value));
 requests.push({method,path:url.pathname+url.search});
 const response=await fetch(url,{method,headers:{authorization:`Bearer ${input.token}`,'content-type':'application/json',...options.additionalHeaders},...(options.requestBody===undefined?{}:{body:JSON.stringify(options.requestBody)})});
 assert(response.ok,`${method} ${route}: ${response.status}`);
 const value=await response.json();
 if(route==='/tpp/models/')context.fln.parse(value);
 return value;
}
const ids={dk:'account',yv:'user',yfr:'revision',vfr:'count',pk:'user-settings',fk:'identity',Ofr:'announcement',Pj:'models'};
let cached;
const snapshot={getData:()=>cached,setData:next=>cached=typeof next==='function'?next(cached):next,invalidate:async()=>{cached=await read(scope);}};
const scope={get:key=>key===ids.dk?input.account_id:key===ids.yv?input.user_id:key===ids.yfr?0:key===ids.Ofr?false:{},set:()=>{},query:{snapshot:key=>key===ids.pk?snapshot:{invalidate:()=>request('GET','/tpp/models/')}}};
const expectedHeader=sharedSource.match(/rL = `([^`]+)`/)[1];
Object.assign(context,ids,{it:expectedHeader,ik:{safeGet:(route,options)=>request('GET',route,options),safePatch:(route,options)=>request('PATCH',route,options)}});
vm.runInContext(slice(source,'function lfr(','function ufr(')+slice(source,'async function xfr(','async function Sfr(')+slice(source,'async function tjr(','function njr(')+';globalThis.functions={read:xfr,toggle:tjr};',context);
const {read,toggle}=context.functions;

// Real compiled React component; only hooks/element construction are supplied.
function slider(data){
 const jsx=(type,props)=>({type,props});
 const levels=new Set(['low','medium','high','xhigh','max','ultra']);
 const view=vm.runInNewContext(slice(agent,'function pr(','var _r,')+';pr',{
  _r:{c:n=>Array(n).fill(Symbol.for('react.memo_cache_sentinel'))},nt:()=>scope,ee:'scope',d:()=>({formatMessage:v=>v.defaultMessage}),de:key=>key==='levels'?levels:key==='user'?{data}:{isPending:false,mutate:()=>{}},Ke:'levels',Ut:'user',Et:'mutation',jt:()=>({data:{hasModelSupportingMaxReasoningEffort:true,hasModelSupportingUltraReasoningEffort:true,models:[]}}),
  Q:{jsx,jsxs:jsx},q:{Header:'header',Content:'content'},ot:'layout',Ze:'row',R:'menu',B:'button',W:{CheckboxItem:'checkbox'},r:'translation',Dt:'switch',$t:'effort',ct:()=>{},vr:['max','ultra'],Ve:['low','medium','high','xhigh'],
 });
 function find(node){if(!node||typeof node!=='object')return;if(node.type==='switch')return node.props;for(const value of Object.values(node)){if(Array.isArray(value)){for(const child of value){const result=find(child);if(result)return result;}}else{const result=find(value);if(result)return result;}}}
 return find(view({hostId:'local'}));
}
assert.equal(slider(undefined).disabled,true,'Failed settings reads disable the actual switch');
cached=await read(scope);
assert.equal(slider(cached).disabled,false);
assert.equal(cached.ultraEffortEnabled,false);
for(const enabled of [true,false,true]){
 await toggle(scope,enabled);
 assert.equal(cached.ultraEffortEnabled,enabled);
 assert.equal(slider(cached).checked,enabled);
 assert.equal(slider(cached).disabled,false);
 assert.equal((await read(scope)).ultraEffortEnabled,enabled);
}
assert.equal(requests.filter(r=>r.method==='PATCH'&&r.path.includes('feature=model_picker_persists_ultra_effort')).length,3);
assert.equal(requests.filter(r=>r.method==='GET'&&r.path.endsWith('/tpp/models/')).length,3);
console.log('Actual Desktop schema, Ultra switch props and original mutation: default read enabled; on/off/on persisted over HTTP and both invalidation reads succeeded.');
