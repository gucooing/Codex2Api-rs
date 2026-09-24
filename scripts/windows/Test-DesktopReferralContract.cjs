'use strict';
const fs = require('node:fs'), vm = require('node:vm'), assert = require('node:assert/strict');
const archive = fs.readFileSync(process.argv[2]);
const size = archive.readUInt32LE(4), header = JSON.parse(archive.subarray(16,16+archive.readUInt32LE(12)));
const entries = Object.entries(header.files.webview.files.assets.files).filter(([name]) => name.startsWith('app-initial-') && name.endsWith('.js'));
assert.equal(entries.length,1);
const entry = entries[0][1], start = 8 + size + Number(entry.offset);
const source = archive.subarray(start,start+entry.size).toString('utf8');
const from = source.indexOf('function shs('), to = source.indexOf('var lhs,',from);
assert(from>=0&&to>from,'Installed Desktop referral reader changed');
const fixture = JSON.parse(fs.readFileSync(0,'utf8'));
const calls = [];
const readers = vm.runInNewContext(source.slice(from,to)+';({list:shs,count:chs})',{
  gSt:options=>options,Xf:options=>options,kd:undefined,kv:{FIVE_SECONDS:5000},
  oy:{safeGet:async(path,options)=>{
    assert.equal(path,'/referrals/invite/tracking');
    const query=options.parameters.query;
    assert.equal(query.program_id,'codex_referral_consumer');
    assert.equal(query.period,'past_90_days');
    assert.equal(query.limit,100);
    calls.push(query.cursor??null);assert(calls.length<=4,'Pagination did not terminate');
    return query.cursor==null?fixture.first:fixture.second;
  }},
});
(async()=>{
  const options=readers.list('past_90_days','user-fixture','account-fixture','codex_referral_consumer');
  const first=await options.queryFn({pageParam:null});
  assert.equal(first.items[0].email,fixture.first.items[0].email);
  assert.equal(options.getNextPageParam(first),'1');
  const second=await options.queryFn({pageParam:options.getNextPageParam(first)});
  assert.equal(options.getNextPageParam(second),null);
  const count=await readers.count('user-fixture','account-fixture','codex_referral_consumer').queryFn({});
  assert.equal(count,2);assert.deepEqual(calls,[null,'1',null,'1']);
  console.log('Installed Desktop: referral list, email filtering, pagination and invite counter accepted the proxy records.');
})().catch(error=>{console.error(error.message);process.exitCode=1;});
