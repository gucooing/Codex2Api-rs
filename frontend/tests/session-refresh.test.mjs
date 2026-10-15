import test from "node:test";
import assert from "node:assert/strict";
import { request, setCsrf as setCsrf } from "../src/lib/http.ts";

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
