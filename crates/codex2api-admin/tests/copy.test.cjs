const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { join } = require('node:path');
const { test } = require('node:test');
const { runInNewContext } = require('node:vm');

const script = readFileSync(join(__dirname, '../static/copy.js'), 'utf8');

function setup({ clipboard, modal = false, key = false, copyAllowed = true, fetchOk = true, input = false } = {}) {
  const copied = [];
  let active;
  let selected;
  let fallbackCount = 0;
  const elements = [];
  const element = () => {
    const attrs = new Map();
    const classes = new Set();
    const result = {
      dataset: {}, childNodes: [], style: {}, disabled: false,
      classList: { add: (...names) => names.forEach((name) => classes.add(name)), remove: (...names) => names.forEach((name) => classes.delete(name)), contains: (name) => classes.has(name) },
      getAttribute: (name) => attrs.get(name) ?? null,
      setAttribute: (name, value) => attrs.set(name, value),
      removeAttribute: (name) => attrs.delete(name),
      append(...children) { children.forEach((child) => { child.parent = result; result.childNodes.push(child); }); },
      querySelector() { return result.message ||= element(); },
      addEventListener(name, handler) { result[name] = handler; },
      focus() { active = result; },
      select() { selected = result; },
      setSelectionRange() {},
      remove() { result.removed = true; },
    };
    elements.push(result);
    return result;
  };
  const body = element();
  const dialog = element();
  const button = element();
  active = button;
  const manager = { dataset: { keyCsrf: 'csrf-fixture' } };
  button.closest = (selector) => selector === '[data-key-manager]' ? manager : modal ? dialog : null;
  if (key) button.dataset.keyCopy = '/admin/keys/test/copy';
  else button.dataset.copyTarget = 'source';
  const target = input ? { value: 'input-value', textContent: '' } : { textContent: '授权链接-fixture' };
  const document = {
    body,
    get activeElement() { return active; },
    querySelectorAll: () => [button],
    createElement: element,
    getElementById: () => target,
    execCommand(command) {
      assert.equal(command, 'copy');
      assert.equal(selected.parent, modal ? dialog : body);
      assert.equal(active, selected);
      fallbackCount++;
      if (copyAllowed) copied.push(selected.value);
      return copyAllowed;
    },
  };
  const window = { getSelection: () => null, clearTimeout() {}, setTimeout() {} };
  const navigator = {};
  if (clipboard === 'available') navigator.clipboard = { writeText: async (text) => copied.push(text) };
  if (clipboard === 'denied') navigator.clipboard = { writeText: async () => { throw new Error('denied'); } };
  runInNewContext(script, {
    window, document, navigator, URLSearchParams,
    fetch: async (path, options) => {
      assert.equal(path, '/admin/keys/test/copy');
      assert.equal(options.method, 'POST');
      assert.equal(options.body.get('csrf'), 'csrf-fixture');
      return { ok: fetchOk, status: fetchOk ? 200 : 403, json: async () => fetchOk ? { token: 'c2a_test-fixture' } : { error: 'denied' } };
    },
  });
  return { button, copied, elements, window, get fallbackCount() { return fallbackCount; } };
}

test('secure-context clipboard succeeds without selection fallback', async () => {
  const fixture = setup({ clipboard: 'available' });
  await fixture.button.click();
  assert.deepEqual(fixture.copied, ['授权链接-fixture']);
  assert.equal(fixture.fallbackCount, 0);
  assert(fixture.button.classList.contains('copy-success'));
});

for (const clipboard of [undefined, 'denied']) {
  for (const modal of [false, true]) {
    test(`copy works when clipboard is ${clipboard || 'unavailable'} and modal=${modal}`, async () => {
      const fixture = setup({ clipboard, modal });
      await fixture.button.click();
      assert.deepEqual(fixture.copied, ['授权链接-fixture']);
      assert.equal(fixture.fallbackCount, 1);
      assert(fixture.elements.find((element) => element.value === '授权链接-fixture').removed);
      assert(fixture.button.classList.contains('copy-success'));
      assert.equal(fixture.button.disabled, false);
    });
  }
}

test('API key fetched with CSRF can be copied without Clipboard API', async () => {
  const fixture = setup({ key: true });
  await fixture.button.click();
  assert.deepEqual(fixture.copied, ['c2a_test-fixture']);
});

test('input copy reads its value and repeated binding does not duplicate controls', async () => {
  const fixture = setup({ input: true });
  const children = fixture.button.childNodes.length;
  fixture.window.bindCopyButtons();
  assert.equal(fixture.button.childNodes.length, children);
  await fixture.button.click();
  assert.deepEqual(fixture.copied, ['input-value']);
});

test('refused copy reports failure and removes the temporary textarea', async () => {
  const fixture = setup({ copyAllowed: false });
  await fixture.button.click();
  assert.deepEqual(fixture.copied, []);
  assert(fixture.button.classList.contains('copy-error'));
  assert(!fixture.button.classList.contains('copy-success'));
  assert(fixture.elements.find((element) => element.value === '授权链接-fixture').removed);
  assert.equal(fixture.button.disabled, false);
});

test('failed API key request does not copy or claim success', async () => {
  const fixture = setup({ key: true, fetchOk: false });
  await fixture.button.click();
  assert.deepEqual(fixture.copied, []);
  assert.equal(fixture.fallbackCount, 0);
  assert(fixture.button.classList.contains('copy-error'));
});
