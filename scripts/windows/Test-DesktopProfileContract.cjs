'use strict';
const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');
const archive=fs.readFileSync(process.argv[2]);
const size=archive.readUInt32LE(4),header=JSON.parse(archive.subarray(16,16+archive.readUInt32LE(12)));
const entries=Object.entries(header.files.webview.files.assets.files).filter(([key])=>key.startsWith('app-initial-')&&key.endsWith('.js'));
assert.equal(entries.length,1);
const entry=entries[0][1],start=8+size+Number(entry.offset),source=archive.subarray(start,start+entry.size).toString('utf8');
function slice(a,b){const from=source.indexOf(a),to=source.indexOf(b,from);assert(from>=0&&to>from);return source.slice(from,to);}
const sample=JSON.parse(fs.readFileSync(0,'utf8'));
const calls=[];
const readers=vm.runInNewContext(slice('function T$s(','function E$s(')+slice('function O$s(','var A$s;')+';({query:T$s,read:O$s})',{
  Xf:value=>value,kd:null,kv:{TEN_MINUTES:600000},ik:{safeGet:(path,options)=>{calls.push({path,options});return Promise.resolve(sample);}}
});
(async()=>{
  const result=await readers.query(sample.profile_details.id,'fixture-user').queryFn({signal:undefined});
  assert.equal(calls[0].path,'/profiles/me/page');
  const value=readers.read(result);
  assert.equal(value.displayName,sample.profile_details.display_name);
  assert.equal(value.imageUrl,sample.profile_details.profile_picture_url);
  assert.equal(value.summary.totalTextTokens,39);
  assert.equal(value.dailyUsage.reduce((n,d)=>n+d.credits,0),39);
  assert.equal(value.activity.daily.chats.reduce((n,d)=>n+d.credits,0),1);
  assert.equal(value.activityInsights.totalThreads,1);
  assert.equal(value.activityInsights.invocations[0].usage_count,1);
  assert.equal(sample.is_self,true);
  assert.equal(sample.page.visibility.value,'private');
  console.log('Installed Desktop profile request, page reader, token/activity graphs and plugin reader passed with actual API response.');
})().catch(e=>{console.error(e);process.exitCode=1;});
