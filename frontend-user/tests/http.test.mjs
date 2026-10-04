import test from "node:test";
import assert from "node:assert/strict";
import { request, setSessionCsrf, ApiError } from "../src/lib/api.ts";

test("user browser requests stay on their own credential audience and clear CSRF on logout", async () => {
  const original = globalThis.fetch;
  const calls = [];
  globalThis.fetch = async (path, options) => {
    calls.push({ path, options });
    return new Response(JSON.stringify({ ok: true }), {
      headers: { "Content-Type": "application/json" },
    });
  };
  try {
    setSessionCsrf("user-csrf");
    await request("/orders", { method: "POST", body: { preview_token: "confirmed-preview" } });
    assert.equal(calls[0].path, "/user/api/orders");
    assert.equal(calls[0].options.headers["X-CSRF-Token"], "user-csrf");
    assert.equal(calls[0].options.credentials, "same-origin");
    assert.equal("Authorization" in calls[0].options.headers, false);
    setSessionCsrf("");
    await request("/login", { method: "POST", body: { username: "user", password: "fixture" } });
    assert.equal("X-CSRF-Token" in calls[1].options.headers, false);
    globalThis.fetch = async () =>
      new Response(JSON.stringify({ error: { message: "钱包余额不足" } }), { status: 400 });
    await assert.rejects(
      request("/orders", { method: "POST", body: {} }),
      (error) =>
        error instanceof ApiError && error.status === 400 && error.message === "钱包余额不足",
    );
  } finally {
    globalThis.fetch = original;
    setSessionCsrf("");
  }
});

test("payment errors retain their specific message", async () => {
  const original = globalThis.fetch;
  try {
    for (const message of [
      "钱包余额不足，订单保留待支付状态",
      "订单已过期，请重新预览并确认订单",
      "该优惠券已过期",
      "套餐已关闭购买，订单已取消",
    ]) {
      globalThis.fetch = async () =>
        new Response(JSON.stringify({ error: { code: "checkout_error", message } }), {
          status: 422,
        });
      await assert.rejects(
        request("/orders/order/pay", { method: "POST" }),
        (error) => error instanceof ApiError && error.message === message,
      );
    }
  } finally {
    globalThis.fetch = original;
  }
});
