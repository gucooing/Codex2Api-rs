import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import assert from 'node:assert/strict';

const assets=path.join(process.argv[2],'readable','webview','assets');
const settings=fs.readFileSync(path.join(assets,'settings-8d6c2878e79a.js'),'utf8');
const presentation=fs.readFileSync(path.join(assets,'presentation-7092c5138f20.js'),'utf8');
const input=JSON.parse(fs.readFileSync(0,'utf8'));
function slice(text,start,end){const a=text.indexOf(start),b=text.indexOf(end,a);assert(a>=0&&b>a,`${start} changed`);return text.slice(a,b);}
const endpoint='/amphora/u18_graduation_unlink_setting_notices';
let postedIds;
async function request(method,route,options={}){
 assert.equal(options.additionalHeaders['ChatGPT-Account-ID'],input.account_id);
 if(method==='POST')postedIds=Array.from(options.requestBody.notice_ids);
 const response=await fetch(input.base_url+route,{method,headers:{authorization:`Bearer ${input.token}`,'content-type':'application/json',...options.additionalHeaders},...(method==='POST'?{body:JSON.stringify(options.requestBody)}:{})});
 assert(response.ok,`${method} ${route}: ${response.status}`);return response.json();
}
const account={accountId:input.account_id,userId:input.user_id};
const scope={get:()=>({status:'allowed',...account})};
const accountMatches=vm.runInNewContext(slice(presentation,'function A(','function j(')+';A',{s:'account-state'});
let queryOptions,cache=input.captured,finished;
const done=new Promise(resolve=>finished=resolve);
const stateValues=[];
let stateSlot=0;
const client={cancelQueries:async()=>{},setQueryData:(_key,update)=>{cache=update(cache);},invalidateQueries:async()=>{
 cache=await queryOptions.queryFn({signal:new AbortController().signal});finished();
}};
const elements=(type,props,key)=>({type,props,key});
const context=vm.createContext({
 rt:{c:n=>Array(n).fill(Symbol.for('react.memo_cache_sentinel'))},oe:()=>scope,C:'store',ge:()=>client,
 de:options=>{queryOptions=options;return{data:input.captured,isPending:false,isError:false,isFetching:false,refetch:()=>{}};},ce:{ONE_MINUTE:60000},
 it:{useState:initial=>{const index=stateSlot++;return[initial,value=>stateValues[index]=value];},useRef:initial=>({current:initial}),useEffect:()=>{}},
 M:{safeGet:(r,o)=>request('GET',r,o),safePost:(r,o)=>request('POST',r,o)},ke:accountMatches,AbortController,
 Y:{jsx:elements,jsxs:elements,Fragment:'fragment'},n:'text',Ce:'loading',W:'row',v:'button',i:'close-icon',O:'callout',j:'link',
 d:()=>({formatList:names=>names.join(', '),formatMessage:value=>value.defaultMessage}),
});
const functions=vm.runInContext(slice(settings,'function $e(','var rt,')+';({read:$e,group:et})',context);
const tree=functions.read({account});
assert.equal(functions.group({notices:[],isPending:false,onDismiss:()=>{}}),null);
const groups=tree.props.children[2];
assert.equal(groups.length,2);
const parent=groups.find(g=>g.key==='parent');
assert(parent.props.notices.length>0);
assert(groups.find(g=>g.key==='teen').props.notices.length>0);
const fresh=await queryOptions.queryFn({signal:new AbortController().signal});
assert(fresh.notices.length>input.captured.notices.length,'A newer notice must exist after the captured render');
cache=fresh;
const card=functions.group(parent.props);
assert.equal(card.type,'callout');assert.equal(card.props.trailingAction.props.disabled,false);
card.props.trailingAction.props.onClick();
await Promise.race([done,new Promise((_,reject)=>setTimeout(()=>reject(Error('Dismiss did not complete')),5000))]);
assert.deepEqual(postedIds,parent.props.notices.map(n=>n.id));
assert.equal(stateValues[1],null,'Original client must not enter its dismissal error branch');
assert(cache.notices.some(n=>n.id===input.newer_id));
assert(cache.notices.some(n=>n.recipient_type==='teen'));
assert(!cache.notices.some(n=>postedIds.includes(n.id)));
console.log('Actual Desktop query, recipient grouping, close callback and refresh preserved newer notices and dismissed only the captured IDs.');
