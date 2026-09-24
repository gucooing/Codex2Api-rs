'use strict';
const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');
const archive=fs.readFileSync(process.argv[2]);
const size=archive.readUInt32LE(4),header=JSON.parse(archive.subarray(16,16+archive.readUInt32LE(12)));
const entries=Object.entries(header.files.webview.files.assets.files).filter(([key])=>key.startsWith('app-initial-')&&key.endsWith('.js'));
assert.equal(entries.length,1);
const entry=entries[0][1],start=8+size+Number(entry.offset),source=archive.subarray(start,start+entry.size).toString('utf8');
function slice(a,b){const from=source.indexOf(a),to=source.indexOf(b,from);assert(from>=0&&to>from);return source.slice(from,to);}
const readers=vm.runInNewContext(slice('function kan(','var pv;')+slice('function lFa(','function MFa(')
  +slice('function NFa(','var PFa;')+slice('function IFa(','function LFa(')
  +slice('function JFa(','function YFa(')+slice('function YFa(','function XFa(')+slice('function XFa(','var ZFa;')
  +';({blocked:SFa,windows:dFa,reset:pFa,limitReached:jFa,snapshot:IFa,identity:wFa})',{PFa:60});
const samples=JSON.parse(fs.readFileSync(0,'utf8'));
for(const [value,blocked] of [[samples.allowed,false],[samples.exhausted,true]]){
  assert.equal(Object.hasOwn(value,'billing'),false);
  assert.equal(JSON.stringify(value).includes('total_cost_limit_usd'),false);
  assert.equal(readers.blocked(value),blocked);
  assert.equal(readers.limitReached(value),blocked);
  const windows=[value.rate_limit.primary_window,value.rate_limit.secondary_window].filter(Boolean);
  assert.equal(readers.windows(value).length,windows.length);
  assert.equal(readers.identity({authAccountId:value.account_id,authUserId:value.user_id,currentAccountId:value.account_id,isCurrentAccountError:false,isCurrentAccountLoading:false,rateLimitStatus:value}),true);
  const snapshot=readers.snapshot(value.rate_limit,value.credits,value.plan_type);
  for(const [key,seconds] of [['primary',18000],['secondary',604800]]){
    const raw=value.rate_limit[key==='primary'?'primary_window':'secondary_window'];
    if(!raw){assert.equal(snapshot[key],null);continue;}
    assert.deepEqual(Object.keys(raw).sort(),['limit_window_seconds','reset_after_seconds','reset_at','used_percent']);
    assert(Number.isInteger(raw.used_percent));
    assert.equal(raw.limit_window_seconds,seconds);
    assert.equal(snapshot[key].windowDurationMins,seconds/60);
    assert.equal(snapshot[key].usedPercent,raw.used_percent);
    assert.equal(snapshot[key].resetsAt,raw.reset_at);
    assert(Number.isInteger(raw.reset_at));assert(raw.reset_after_seconds>0&&raw.reset_after_seconds<=seconds);
  }
  if(windows.length){assert(windows.some(w=>w.reset_at===readers.reset(value)));}else assert.equal(readers.reset(value),null);
}
console.log('Installed Desktop reads identity, 5h/week window snapshots, reset times and allowed/exhausted states from actual proxy responses.');
