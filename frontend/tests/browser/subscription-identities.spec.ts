import { test, expect } from "@playwright/test";
import { createHash } from "node:crypto";
import { createServer } from "node:http";

test("users manage independent subscriptions, purchase another and select their OAuth identity", async ({
  page,
  context,
  browser,
}) => {
  const login = await context.request.post("/admin/api/login", {
    data: { username: "admin", password: "admin" },
  });
  expect(login.status()).toBe(200);
  const { csrf_token } = await login.json();
  const headers = { "x-csrf-token": csrf_token };
  const addresses = await (await context.request.get("/admin/api/settings/public-urls")).json();
  for (const origin of [addresses.user_url, addresses.api_url])
    expect(new URL(origin).hostname).toBe("127.0.0.1");
  const suffix = Date.now();
  const username = `subscriptions-${suffix}`;
  const created = await context.request.post("/admin/api/users", {
    headers,
    data: {
      username,
      name: username,
      email: `${username}@example.test`,
      password: "subscription-fixture",
      enabled: true,
      revision: null,
    },
  });
  expect(created.status()).toBe(200);
  const user = await created.json();
  expect(
    (await (await context.request.get(`/admin/api/subscriptions?user_id=${user.id}`)).json()).total,
  ).toBe(0);
  expect((await context.request.get("/admin/api/consumers")).status()).toBe(404);
  await page.goto("/admin/settings/");
  await expect(page.getByLabel("默认 RPM 限制", { exact: true })).toHaveCount(0);
  await expect(page.getByLabel("默认并发限制", { exact: true })).toHaveCount(0);
  await page.goto("/admin/plans/");
  await page.getByRole("button", { name: "添加套餐", exact: true }).click();
  let dialog = page.getByRole("dialog");
  await expect(dialog.getByLabel("RPM 限制", { exact: true })).toHaveValue("40");
  await expect(dialog.getByLabel("并发限制", { exact: true })).toHaveValue("10");
  const planName = `Multi subscription ${suffix}`;
  await dialog.getByLabel("套餐名称", { exact: true }).fill(planName);
  await dialog.getByLabel("售价（USD）", { exact: true }).fill("1");
  const savedPlan = page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      new URL(response.url()).pathname === "/admin/api/plans",
  );
  await dialog.getByRole("button", { name: "保存", exact: true }).click();
  const planResponse = await savedPlan;
  expect(planResponse.status()).toBe(200);
  const plan = await planResponse.json();
  expect([plan.rpm_limit, plan.concurrency_limit]).toEqual([40, 10]);
  await expect(dialog).toBeHidden();
  const hiddenPlans: { id: string; name: string; revision: number; code: string }[] = [];
  for (const [label, price, published, code] of [
    ["Unpriced", null, true, "plan_unpriced"],
    ["Closed", "1", false, "plan_not_for_sale"],
  ] as const) {
    const response = await context.request.post("/admin/api/plans", {
      headers,
      data: {
        name: `${label} ${suffix}`,
        provider_id: "chatgpt",
        plan_type: "plus",
        description: "",
        sale_price_usd: price,
        allow_purchase: published,
        duration_days: 30,
        supplier_tag_id: null,
        rpm_limit: 40,
        concurrency_limit: 10,
        model_access: "all",
        models: [],
        spending_windows: [],
        revision: null,
      },
    });
    expect(response.status()).toBe(200);
    hiddenPlans.push({ ...(await response.json()), code });
  }
  await page.goto(`/admin/subscriptions/?user_id=${user.id}`);
  await expect(page.getByRole("link", { name: "虚拟账户", exact: true })).toHaveCount(0);
  await expect(page.getByRole("columnheader", { name: "用户", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "新增订阅", exact: true }).click();
  dialog = page.getByRole("dialog");
  await dialog.getByRole("combobox", { name: "用户", exact: true }).fill(username);
  await page.getByRole("option", { name: username, exact: true }).click();
  await dialog.getByRole("combobox", { name: "套餐", exact: true }).click();
  await page.getByRole("option", { name: planName, exact: true }).click();
  const savedSubscription = page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      new URL(response.url()).pathname === "/admin/api/subscriptions",
  );
  await dialog.getByRole("button", { name: "保存", exact: true }).click();
  const firstResponse = await savedSubscription;
  expect(firstResponse.status()).toBe(200);
  const first = await firstResponse.json();
  const secondResponse = await context.request.post("/admin/api/subscriptions", {
    headers,
    data: {
      user_id: user.id,
      plan_id: plan.id,
      expires_at: null,
      enabled: true,
      reissue: false,
      revision: null,
    },
  });
  expect(secondResponse.status()).toBe(200);
  const second = await secondResponse.json();
  expect(second.id).not.toBe(first.id);
  await page.reload();
  await expect(page.getByRole("link", { name: "管理", exact: true })).toHaveCount(2);
  await page.goto(`/admin/subscriptions/detail/?id=${first.id}`);
  await expect(page.getByLabel("用户", { exact: true })).toHaveValue(username);
  for (const tab of ["账户设置", "用量统计", "重置卡", "记录查询", "登录设备"])
    await expect(page.getByRole("tab", { name: tab, exact: true })).toBeVisible();
  await expect(page.getByLabel("标签号池", { exact: true })).toBeVisible();
  const funding = await context.request.post(`/admin/api/users/${user.id}/wallet-adjustments`, {
    headers,
    data: { request_id: `fund-${suffix}`, amount_cents: 1000, expected_revision: user.revision },
  });
  expect(funding.status()).toBe(200);

  const userContext = await browser.newContext({ baseURL: addresses.user_url });
  const userPage = await userContext.newPage();
  const errors: string[] = [];
  userPage.on("pageerror", (error) => errors.push(error.message));
  userPage.on("console", (message) => {
    if (["error", "warning"].includes(message.type())) errors.push(message.text());
  });
  let callback = "";
  const server = createServer((request, response) => {
    callback = request.url ?? "";
    response.end("Authorized");
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("Missing callback port");
  const redirect = `http://127.0.0.1:${address.port}/auth/callback`;
  try {
    const userLogin = await userContext.request.post("/user/api/login", {
      data: { username, password: "subscription-fixture" },
    });
    expect(userLogin.status()).toBe(200);
    const userSession = await userLogin.json();
    for (const hidden of hiddenPlans) {
      const catalog = await (
        await userContext.request.get(`/user/api/plans?search=${encodeURIComponent(hidden.name)}`)
      ).json();
      expect(catalog.total).toBe(0);
      expect(catalog.items).toEqual([]);
      const rejected = await userContext.request.post("/user/api/checkout/preview", {
        headers: { "x-csrf-token": userSession.csrf_token },
        data: {
          plan_id: hidden.id,
          plan_revision: hidden.revision,
          subscription_id: null,
          subscription_revision: null,
          payment_method: "wallet",
          coupon_code: "",
        },
      });
      expect(rejected.status()).toBe(422);
      expect((await rejected.json()).error.code).toBe(hidden.code);
    }
    await userPage.goto("/user/subscriptions/");
    await expect(userPage.getByRole("cell", { name: "40", exact: true })).toHaveCount(2);
    await expect(userPage.getByRole("cell", { name: "10", exact: true })).toHaveCount(2);
    await userPage.goto("/user/plans/");
    for (const hidden of hiddenPlans)
      await expect(userPage.getByRole("button", { name: hidden.name, exact: true })).toHaveCount(0);
    const card = userPage
      .locator('[data-slot="card"]')
      .filter({ has: userPage.getByRole("button", { name: planName, exact: true }) });
    await card.getByRole("button", { name: "下单", exact: true }).click();
    const checkout = userPage.getByRole("dialog");
    await expect(checkout.getByRole("combobox", { name: "目标订阅", exact: true })).toContainText(
      "购买新订阅",
    );
    await checkout.getByRole("button", { name: "确认订单", exact: true }).click();
    await checkout.getByRole("button", { name: /钱包支付/ }).click();
    await expect(checkout.getByRole("heading", { name: "订单完成", exact: true })).toBeVisible();
    const subscriptions = await (await userContext.request.get("/user/api/subscriptions")).json();
    expect(subscriptions.total).toBe(3);
    const verifier = "v".repeat(64);
    const params = new URLSearchParams({
      response_type: "code",
      client_id: "app_EMoamEEZ73f0CkXaXp7hrann",
      redirect_uri: redirect,
      state: `selection-${suffix}`,
      code_challenge: createHash("sha256").update(verifier).digest("base64url"),
      code_challenge_method: "S256",
      scope: "openid profile email offline_access",
    });
    await userPage.goto(`/user/authorize/?${params}`);
    await userPage.getByRole("combobox", { name: "目标订阅", exact: true }).click();
    await userPage.getByRole("option", { name: new RegExp(second.id.slice(0, 8)) }).click();
    await userPage.getByRole("button", { name: "确认登录", exact: true }).click();
    await expect.poll(() => callback).toContain("code=");
    const result = new URL(callback, redirect);
    expect(result.searchParams.get("state")).toBe(`selection-${suffix}`);
    const exchanged = await userContext.request.post(
      `${addresses.api_url}/api/oauth/chatgpt/oauth/token`,
      {
        form: {
          grant_type: "authorization_code",
          client_id: "app_EMoamEEZ73f0CkXaXp7hrann",
          code: result.searchParams.get("code")!,
          redirect_uri: redirect,
          code_verifier: verifier,
        },
      },
    );
    expect(exchanged.status()).toBe(200);
    const tokens = await exchanged.json();
    const claims = JSON.parse(
      Buffer.from(tokens.access_token.split(".")[1], "base64url").toString(),
    );
    expect(claims["https://api.openai.com/auth"].chatgpt_account_id).toBe(second.id);
    expect(errors).toEqual([]);
  } finally {
    await userContext.close();
    await new Promise<void>((resolve, reject) =>
      server.close((error) => (error ? reject(error) : resolve())),
    );
  }
});
