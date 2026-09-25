'use strict';
const fs = require('node:fs'), vm = require('node:vm'), assert = require('node:assert/strict');
const {renderer, between} = require('./desktop-contract.cjs');

(async () => {
  const installed = await renderer(process.argv[2]);
  const sample = process.argv.includes('--requests') ? {} : JSON.parse(fs.readFileSync(0, 'utf8'));
  installed.bindings.za();
  const schema = between(installed.text, 'CFc=$().regex(', '})))()}var EFc');
  const context = {
    $: installed.bindings.$, TextEncoder, Zr: {info() {}},
    Dg: () => ({'fixture-header': 'preserved'}),
  };
  const requests = [];
  context.Cg = {getInstance: () => ({fetch: async (path, request) => {
    requests.push({path, headers: request.headers, body: JSON.parse(request.body)});
    assert.equal(request.method, 'POST');
    assert(request.signal instanceof AbortSignal);
    return new Response(sample.body ?? 'v=0\r\ns=fixture\r\n', {
      status: sample.status ?? 201,
      headers: {location: sample.location ?? '/backend-api/wham/realtime/calls/rtc_fixture'},
    });
  }})};
  const create = vm.runInNewContext('var ' + schema + ';' +
    between(installed.text, 'async function SFc(', 'var CFc,wFc;') + ';SFc', context);
  for (const version of ['v1', 'v3']) {
    const result = await create({
      codexSessionId: 'session-fixture', conversationId: 'thread-fixture',
      initialItems: [{role: 'user', text: 'fixture'}], offerSdp: 'v=0\r\ns=offer\r\n',
      prompt: 'fixture instruction', realtimeSessionId: 'realtime-fixture',
      realtimeSessionOverrides: {version}, signal: new AbortController().signal,
      threadSource: 'fixture', voice: 'marin',
    });
    assert.equal(result.answerSdp, sample.body ?? 'v=0\r\ns=fixture\r\n');
    assert.equal(result.callId, sample.call_id ?? 'rtc_fixture');
    const request = requests.at(-1);
    assert.equal(request.path, '/wham/realtime/calls?intent=quicksilver&architecture=avas');
    assert.equal(request.headers['Thread-Id'], 'thread-fixture');
    assert.equal(request.headers['Session-Id'], 'session-fixture');
    assert.equal(request.headers['OpenAI-Alpha'], version === 'v3' ? 'quicksilver=v2' : 'quicksilver=v1');
    assert.equal(request.body.session.model, version === 'v3' ? 'gpt-live-1-codex' : 'gpt-realtime-1.5');
    assert.equal(request.body.sdp, 'v=0\r\ns=offer\r\n');
  }
  console.log(JSON.stringify({source: installed.name, requests, reader: 'actual SFc and embedded call-ID schema'}));
})().catch(error => {console.error(error.name + ': ' + String(error.message).slice(0, 700)); process.exitCode = 1;});
