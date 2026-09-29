'use strict';
// Read code from the installed archive. No client files or business logic change.
const fs = require('node:fs');
const assert = require('node:assert/strict');

function archive(file) {
  const bytes = fs.readFileSync(file);
  const tree = JSON.parse(bytes.subarray(16, 16 + bytes.readUInt32LE(12)));
  const offset = 8 + bytes.readUInt32LE(4);
  function read(name) {
    let entry = tree;
    for (const part of name.split('/')) entry = entry.files[part];
    assert(entry && !entry.unpacked, `Missing archive source: ${name}`);
    const start = offset + Number(entry.offset);
    return bytes.subarray(start, start + entry.size).toString('utf8');
  }
  function unique(directory, prefix) {
    let entry = tree;
    for (const part of directory.split('/')) entry = entry.files[part];
    const names = Object.keys(entry.files).filter(name => name.startsWith(prefix) && name.endsWith('.js'));
    assert.equal(names.length, 1, `Installed module changed: ${prefix}`);
    return `${directory}/${names[0]}`;
  }
  return {read, unique};
}

function between(source, start, end) {
  const a = source.indexOf(start), b = source.indexOf(end, a + start.length);
  assert(a >= 0 && b > a, `Installed contract changed: ${start}`);
  return source.slice(a, b);
}

function statsigSdk(source) {
  const vm = require('node:vm');
  const main = source.read(source.unique('.vite/build', 'main-'));
  const logger = /([\w$]+)=e\.t\(\(e=>\{Object\.defineProperty\(e,"__esModule",\{value:!0\}\),e\.Log=e\.LogLevel=void 0/.exec(main);
  assert(logger, 'Installed Statsig SDK module start changed');
  const exportAt = main.indexOf('e.StatsigClient=void 0', logger.index);
  const modules = [...main.slice(logger.index, exportAt).matchAll(/([\w$]+)=e\.t\(/g)];
  const name = modules.at(-1)?.[1];
  const end = main.indexOf('}))(),', exportAt);
  assert(name && end > exportAt, 'Installed Statsig SDK exports changed');
  return vm.runInNewContext('var ' + main.slice(logger.index, end + 5) + ';' + name, {
    e: { t(factory) { let module; return () => { if (!module) { module = {exports:{}}; factory(module.exports, module); } return module.exports; }; } },
    console, setTimeout, clearTimeout, setInterval, clearInterval, URL, TextEncoder, TextDecoder, AbortController, performance,
  });
}

async function renderer(file) {
  const source = archive(file), directory = 'webview/assets';
  const name = source.unique(directory, 'app-initial-');
  const text = source.read(name);
  const sharedName = source.unique(directory, 'app-shared-');
  const sharedText = source.read(sharedName);
  const runtime = sharedText.match(/from["']\.\/(rolldown-runtime-[^"']+)["']/)?.[1];
  assert(runtime, 'Installed renderer runtime import changed');
  const data = value => 'data:text/javascript;base64,' + Buffer.from(value).toString('base64');
  const runtimeUrl = data(source.read(`${directory}/${runtime}`));
  const shared = await import(data(sharedText.replaceAll('./' + runtime, runtimeUrl)));
  const bindings = {};
  for (const match of text.matchAll(/import\{([^}]+)\}from["']\.\/([^"']+)["']/g)) {
    const imported = match[2] === sharedName.split('/').pop() ? shared
      : match[2] === runtime ? await import(runtimeUrl) : null;
    if (!imported) continue;
    for (const entry of match[1].split(',')) {
      const [original, local = original] = entry.trim().split(/\s+as\s+/);
      bindings[local] = imported[original];
    }
  }
  return {...source, name, text, bindings};
}

module.exports = {archive, between, renderer, statsigSdk};
