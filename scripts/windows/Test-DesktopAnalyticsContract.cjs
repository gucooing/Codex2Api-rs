'use strict';
const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');
const archive=fs.readFileSync(process.argv[2]);
const size=archive.readUInt32LE(4),header=JSON.parse(archive.subarray(16,16+archive.readUInt32LE(12)));
const entries=Object.entries(header.files.webview.files.assets.files).filter(([key])=>key.startsWith('app-initial-')&&key.endsWith('.js'));
assert.equal(entries.length,1);
const entry=entries[0][1],start=8+size+Number(entry.offset),source=archive.subarray(start,start+entry.size).toString('utf8');
const a=source.indexOf('async function o0c('),b=source.indexOf('var d0c,j7;',a);
assert(a>=0&&b>a);
const requests=[],warnings=[];
const submit=vm.runInNewContext(source.slice(a,b)+';o0c',{
  d0c:'/wham/analytics-events/events',j7:1048576,TextEncoder,Date,
  Rv:{getInstance:()=>({post:async(path,body)=>{requests.push({path,body:JSON.parse(body)});return {};}})},
  iy:()=>({}),$t:{warning:(...args)=>warnings.push(args)},
});
(async()=>{
  for(const event of [
    {eventKind:'appgen_my_apps_view'},
    {eventKind:'turn_rating',threadId:'thread-a',turnId:'turn-a',sessionId:'session-a',rating:'positive'},
    {eventKind:'action',threadId:'thread-a',turnId:'turn-a',action:'copy',metadata:{private:'not retained'}},
    {eventKind:'turn_diff',threadId:'thread-a',turnId:'turn-a',sessionId:'session-a',completedAtMs:Date.now(),status:'completed',diff:'PRIVATE DIFF',diffFormat:'unified',captureStatus:'captured'},
    {eventKind:'turn_environment'},
  ])await submit(event);
  assert.equal(warnings.length,0);assert.equal(requests.length,4);
  assert(requests.every(request=>request.path==='/wham/analytics-events/events'));
  process.stdout.write(JSON.stringify(requests));
})().catch(error=>{console.error(error);process.exitCode=1;});
