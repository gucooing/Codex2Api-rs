import test from "node:test";
import assert from "node:assert/strict";
import { request, setCsrf as setCsrf } from "../src/lib/http.ts";
import { desktopViewportScript } from "../src/lib/viewport.ts";
import { keepSessionAlive } from "../src/lib/session-refresh.ts";
import { runInNewContext } from "node:vm";

const json = (status, value) => new Response(JSON.stringify(value), { status });
const unauthorized = () => json(401, { error: { code: "unauthorized", message: "登录已失效" } });

test("expired access refreshes once for concurrent requests and replays each request once", async () => {
  const original = {
    fetch: globalThis.fetch,
    window: globalThis.window,
    document: globalThis.document,
  };
  let refreshes = 0;
  let expiredEvents = 0;
  let fresh = false;
  const calls = [];
  let finishRefresh;
  const gate = new Promise((resolve) => {
    finishRefresh = resolve;
  });
  globalThis.window = {
    dispatchEvent: () => {
      expiredEvents++;
    },
  };
  globalThis.document = { cookie: "c2a_admin_csrf=cookie-csrf" };
  setCsrf("");
  globalThis.fetch = async (path, options) => {
    calls.push({ path, options });
    if (path.endsWith("/session/refresh")) {
      refreshes++;
      assert.equal(options.headers["X-CSRF-Token"], "cookie-csrf");
      await gate;
      fresh = true;
      return json(200, { ok: true });
    }
    return fresh ? json(200, { ok: true }) : unauthorized();
  };
  try {
    const pending = Promise.all([
      request("/session"),
      request("/orders", { method: "POST", body: { order: "same-order" } }),
    ]);
    await new Promise((resolve) => setImmediate(resolve));
    assert.equal(refreshes, 1);
    finishRefresh();
    await pending;
    assert.equal(expiredEvents, 0);
    const writes = calls.filter((call) => call.path.endsWith("/orders"));
    assert.equal(writes.length, 2);
    assert.equal(writes[0].options.body, writes[1].options.body);
    assert.equal(
      calls.every((call) => call.options.credentials === "same-origin"),
      true,
    );
  } finally {
    Object.assign(globalThis, original);
    setCsrf("");
  }
});

test("only a rejected refresh logs out; outages, foreign 401s and canceled reads do not", async () => {
  const original = {
    fetch: globalThis.fetch,
    window: globalThis.window,
    document: globalThis.document,
  };
  let events = 0;
  globalThis.window = {
    dispatchEvent: () => {
      events++;
    },
  };
  globalThis.document = { cookie: "c2a_admin_csrf=csrf" };
  setCsrf("csrf");
  try {
    for (const status of [500, 503, 403]) {
      globalThis.fetch = async (path) =>
        path.endsWith("/session/refresh")
          ? json(status, { error: { code: "service_unavailable", message: "暂时不可用" } })
          : unauthorized();
      await assert.rejects(request("/orders"));
      assert.equal(events, 0);
    }
    let calls = 0;
    globalThis.fetch = async () => {
      calls++;
      return json(401, { error: { code: "upstream_auth", message: "上游错误" } });
    };
    await assert.rejects(request("/orders"));
    assert.equal(calls, 1);
    assert.equal(events, 0);
    const controller = new AbortController();
    globalThis.fetch = async () => {
      controller.abort();
      return unauthorized();
    };
    await assert.rejects(request("/orders", { signal: controller.signal }));
    assert.equal(events, 0);
    globalThis.fetch = async () => unauthorized();
    await assert.rejects(request("/orders"));
    assert.equal(events, 1);
  } finally {
    Object.assign(globalThis, original);
    setCsrf("");
  }
});

test("session checks pause when hidden and resume after visibility returns", async () => {
  const original = { window: globalThis.window, document: globalThis.document, now: Date.now };
  const win = new EventTarget();
  const doc = new EventTarget();
  let timer;
  win.setInterval = (callback) => {
    timer = callback;
    return 1;
  };
  win.clearInterval = () => {
    timer = undefined;
  };
  doc.visibilityState = "visible";
  globalThis.window = win;
  globalThis.document = doc;
  let now = 0;
  Date.now = () => now;
  let calls = 0;
  let signal;
  const stop = keepSessionAlive(
    async (value) => {
      calls++;
      signal = value;
    },
    () => assert.fail("unexpected error"),
  );
  try {
    await new Promise((resolve) => setImmediate(resolve));
    assert.equal(calls, 1);
    now += 10 * 60 * 1000;
    doc.visibilityState = "hidden";
    timer();
    assert.equal(calls, 1);
    doc.visibilityState = "visible";
    doc.dispatchEvent(new Event("visibilitychange"));
    win.dispatchEvent(new Event("focus"));
    await new Promise((resolve) => setImmediate(resolve));
    assert.equal(calls, 2);
    stop();
    assert.equal(signal.aborted, true);
    now += 10 * 60 * 1000;
    win.dispatchEvent(new Event("online"));
    assert.equal(calls, 2);
  } finally {
    stop();
    globalThis.window = original.window;
    globalThis.document = original.document;
    Date.now = original.now;
  }
});

test("phone desktop-site identities select one desktop viewport for CSS and JS", () => {
  for (const [userAgent, width, height, touches, expected] of [
    ["Mozilla/5.0 (iPhone) Mobile Safari", 390, 844, 5, "width=device-width, initial-scale=1"],
    [
      "Mozilla/5.0 (Linux; Android 15) Mobile Chrome",
      390,
      844,
      5,
      "width=device-width, initial-scale=1",
    ],
    ["Mozilla/5.0 (Macintosh; Intel Mac OS X) Safari", 390, 844, 5, "width=1280"],
    ["Mozilla/5.0 (X11; Linux x86_64) Chrome", 844, 390, 5, "width=1280"],
    ["Mozilla/5.0 (Windows NT 10.0) Chrome", 1440, 900, 10, "width=device-width, initial-scale=1"],
    ["Mozilla/5.0 (Macintosh) Safari", 390, 844, 0, "width=device-width, initial-scale=1"],
  ]) {
    let content = "width=device-width, initial-scale=1";
    runInNewContext(desktopViewportScript, {
      navigator: { userAgent, maxTouchPoints: touches },
      screen: { width, height },
      document: {
        querySelector: () => ({
          setAttribute: (name, value) => {
            assert.equal(name, "content");
            content = value;
          },
        }),
      },
    });
    assert.equal(content, expected, userAgent);
  }
});
