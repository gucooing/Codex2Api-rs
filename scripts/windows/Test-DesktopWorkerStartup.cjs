'use strict';
// Reproduce the launcher's inherited breakpoint using a real Node Worker. This
// exercises our startup environment; it does not patch Desktop application code.
const { spawn } = require('node:child_process');
const net = require('node:net');
const assert = require('node:assert/strict');
const { isolateLauncherInspector } = require('../../tools/desktop-proxy/AddressHook.cjs');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));

async function run(cleanup) {
  const listener = net.createServer();
  await new Promise(resolve => listener.listen(0, '127.0.0.1', resolve));
  const port = listener.address().port;
  await new Promise(resolve => listener.close(resolve));
  const arg = `--inspect-brk=127.0.0.1:${port}`;
  const program = `const {Worker}=require('node:worker_threads');console.log('MAIN');const w=new Worker("require('node:worker_threads').parentPort.postMessage('WORKER_READY')",{eval:true});w.on('message',message=>console.log(message));setTimeout(()=>process.exit(0),2000);`;
  const child = spawn(process.execPath, [arg, '-e', program], { windowsHide: true, stdio: ['ignore', 'pipe', 'ignore'] });
  let output = '', socket;
  child.stdout.on('data', bytes => { output += bytes; });
  const exited = new Promise(resolve => child.once('exit', resolve));
  const pending = new Map(); let sequence = 0;
  const send = (method, params = {}) => new Promise((resolve, reject) => {
    const id = ++sequence;
    const timer = setTimeout(() => { pending.delete(id); reject(Error(`Timed out: ${method}`)); }, 5000);
    pending.set(id, { resolve, reject, timer });socket.send(JSON.stringify({ id, method, params }));
  });
  try {
    let target;
    for (let i = 0; i < 100; i++) {
      try { target = (await (await fetch(`http://127.0.0.1:${port}/json/list`, { signal: AbortSignal.timeout(200) })).json())[0]; if (target) break; } catch {}
      await delay(30);
    }
    assert(target, 'Test inspector did not start');
    socket = new WebSocket(target.webSocketDebuggerUrl);
    await new Promise((resolve, reject) => { socket.onopen = resolve;socket.onerror = reject; });
    let onPause;const paused = new Promise(resolve => { onPause = resolve; });
    socket.onmessage = event => {
      const message = JSON.parse(event.data);
      if (message.id && pending.has(message.id)) {
        const call = pending.get(message.id);pending.delete(message.id);clearTimeout(call.timer);
        message.error ? call.reject(Error(message.error.message)) : call.resolve(message.result);
      } else if (message.method === 'Debugger.paused') onPause(message.params);
    };
    await send('Debugger.enable');await send('Runtime.runIfWaitingForDebugger');const pause = await paused;
    if (cleanup) {
      const result = await send('Debugger.evaluateOnCallFrame', { callFrameId: pause.callFrames[0].callFrameId, expression: `(${isolateLauncherInspector.toString()})(require('node:worker_threads'),process.execArgv,${JSON.stringify(arg)})`, returnByValue: true });
      assert.equal(result.result.value, true);
    }
    await send('Debugger.resume');await delay(200);
    await send('Runtime.evaluate', { expression: "setTimeout(()=>require('node:inspector').close(),50);true" });
    await send('Debugger.disable');socket.close();await exited;
    return output;
  } finally {
    socket?.close();for (const call of pending.values()) clearTimeout(call.timer);
    if (child.exitCode === null) child.kill();
  }
}
(async () => {
  const args = ['--trace-warnings', '--inspect-brk=127.0.0.1:12345'];
  let observed;
  const workerModule={Worker:class {constructor(file,options){observed=options;}}};
  isolateLauncherInspector(workerModule,args,'--inspect-brk=127.0.0.1:12345');assert.deepEqual(args, ['--trace-warnings']);
  new workerModule.Worker('fixture',{workerData:{fixture:true}});assert.deepEqual(observed.execArgv,['--trace-warnings']);assert.equal(observed.workerData.fixture,true);
  new workerModule.Worker('fixture',{execArgv:['--trace-deprecation']});assert.deepEqual(observed.execArgv,['--trace-deprecation']);
  const electronArgs=[];const electronWorkers={Worker:class{constructor(file,options){observed=options;}}};
  isolateLauncherInspector(electronWorkers,electronArgs,'--inspect-brk=127.0.0.1:12345');new electronWorkers.Worker('fixture');assert.deepEqual(observed.execArgv,[]);
  const old = await run(false);assert(old.includes('MAIN'));assert(!old.includes('WORKER_READY'));
  const fixed = await run(true);assert(fixed.includes('WORKER_READY'));
  console.log('Launcher regression: inherited breakpoint stalls Worker; removing only launcher breakpoint allows Worker to respond.');
})().catch(error => { console.error(error);process.exitCode = 1; });
