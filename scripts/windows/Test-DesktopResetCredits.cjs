'use strict';
// Execute the installed Desktop request builders and redemption state machine.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const {archive, between} = require('./desktop-contract.cjs');
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const asar = archive(process.argv[2]);
const source = asar.read(asar.unique('webview/assets', 'app-initial-'));
const calls = [];
const kg = {
  safeGet: async path => { calls.push({method:'GET',path}); return input.credits_before_reset; },
  safePost: async (path,options) => { calls.push({method:'POST',path,...options}); return input.reset_cases[options.requestBody.redeem_request_id]; },
};
const api = new Function('kg',
  between(source,'function Tai(','function Dai(') +
  between(source,'function Oai(','function kai(') +
  between(source,'function Yii(','function Xii(') +
  between(source,'function tIo(','var nIo,') +
  between(source,'function aIo(','function oIo(') +
  ';return {Tai,Eai,Oai,Yii,tIo,aIo};')(kg);
(async () => {
  const list = api.Tai(await api.Eai());
  assert.equal(calls[0].path,'/wham/rate-limit-reset-credits');
  assert.equal(list.credits.filter(api.tIo).length,list.available_count);
  assert.equal(list.available_count,2);
  for (const card of list.credits) {
    assert.equal(card.reset_type,'codex_rate_limits');
    assert(Number.isFinite(Date.parse(card.granted_at)));
    assert(Number.isFinite(Date.parse(card.expires_at)));
    assert(Date.parse(card.expires_at)>Date.parse(card.granted_at));
    assert(!('note' in card));
  }
  const selected = list.credits[0].id;
  const consumed = await api.Oai({creditId:selected,redeemRequestId:'reset-use'});
  assert.deepEqual(calls[1],{method:'POST',path:'/wham/rate-limit-reset-credits/consume',requestBody:{credit_id:selected,redeem_request_id:'reset-use'}});
  const cached = api.Yii(list,consumed.code,consumed.credit.id);
  assert.equal(cached.available_count,1);
  assert(!cached.credits.some(c=>c.id===consumed.credit.id));
  assert.equal(api.Yii(cached,'already_redeemed',consumed.credit.id).available_count,1);
  const state = api.aIo();
  let key;
  const first = await state.redeem({availableCount:2,creditId:selected,consume:async request=>{key=request.redeemRequestId;throw new Error('lost response');}});
  assert.equal(first.status,'transport_error');
  const retry = await state.redeem({availableCount:2,creditId:selected,consume:async request=>{
    assert.equal(request.redeemRequestId,key);
    return input.reset_cases['reset-retry'];
  }});
  assert.equal(retry.status,'reset');
  assert.equal(retry.resetType,'codex_rate_limits');
  for (const key of ['reset-empty','reset-missing']) {
    const result = await api.aIo().redeem({availableCount:1,consume:async()=>input.reset_cases[key]});
    assert.equal(result.status,'rejected');
    assert.equal(result.code,input.reset_cases[key].code);
  }
  console.log('Installed Desktop reset list, selected request, cache update and lost-response retry accepted actual proxy responses.');
})().catch(error=>{console.error(error);process.exitCode=1;});
