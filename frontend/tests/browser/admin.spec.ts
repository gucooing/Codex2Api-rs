import { test, expect, type Locator, type Page } from "@playwright/test";
import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";
import { createHash } from "node:crypto";
import { createServer } from "node:http";
import type { AddressInfo } from "node:net";

const evidence = resolve(
  process.env.CODEX2API_TEST_OUTPUT_DIR ?? "../target/review-next-ui-redesign-20260923",
  "screenshots",
);
const suffix = Date.now().toString(36);
async function selectOption(page: Page, trigger: Locator, label: string) {
  await trigger.click();
  await page.getByRole("option", { name: label, exact: true }).click();
}

async function expectActiveTab(page: Page, name: string) {
  const tab = page.getByRole("tab", { name, exact: true });
  const panel = page.getByRole("tabpanel", { name, exact: true });
  await expect(tab).toHaveAttribute("aria-selected", "true");
  await expect(panel).toBeVisible();
  expect(await tab.getAttribute("aria-controls")).toBe(await panel.getAttribute("id"));
  expect(await panel.getAttribute("aria-labelledby")).toBe(await tab.getAttribute("id"));
}

async function expectModalRegions(dialog: Locator) {
  const header = await dialog.locator("[data-slot=dialog-header]").boundingBox();
  const body = await dialog
    .locator("form > [data-slot=scroll-area] [data-slot=scroll-area-viewport]")
    .boundingBox();
  const footer = await dialog.locator("form > [data-slot=field-group]").boundingBox();
  expect(header).not.toBeNull();
  expect(body).not.toBeNull();
  expect(footer).not.toBeNull();
  expect(header!.y + header!.height).toBeLessThanOrEqual(body!.y + 1);
  expect(footer!.y).toBeGreaterThanOrEqual(body!.y + body!.height - 1);
  await expect(dialog.getByRole("button", { name: "保存", exact: true })).toBeInViewport();
  await expect(dialog.getByRole("button", { name: "关闭", exact: true })).toBeInViewport();
}

test("real admin actions, isolation and public consumer authorization", async ({
  page,
  context,
  baseURL,
}) => {
  await mkdir(evidence, { recursive: true });
  const pageErrors: string[] = [];
  page.on("pageerror", (error) => pageErrors.push(error.message));
  page.on("console", (message) => {
    if (!["error", "warning"].includes(message.type())) return;
    // This test deliberately submits invalid credentials and missing CSRF.
    if (
      /^Failed to load resource: the server responded with a status of (400|401|403)\b/.test(
        message.text(),
      )
    )
      return;
    pageErrors.push(message.text());
  });
  await page.goto("/admin/");
  await page.getByLabel("用户名", { exact: true }).fill("admin");
  await page.getByLabel("密码", { exact: true }).fill("admin");
  await page.getByRole("button", { name: "登录", exact: true }).click();
  await expect(
    page.locator("[data-slot=breadcrumb-page]").filter({ hasText: "概览" }),
  ).toBeVisible();
  await page.screenshot({ path: resolve(evidence, "01-overview.png"), fullPage: true });

  const session = await (await context.request.get("/admin/api/session")).json();
  expect(session.authenticated).toBe(true);
  const cookie = (await context.cookies()).find((item) => item.name.includes("session"));
  expect(cookie?.httpOnly).toBe(true);
  const csrf = { "X-CSRF-Token": session.csrf_token };
  const existingModels = await (await context.request.get("/admin/api/models")).json();
  const existingModel = existingModels.items.find(
    (item: { model: string }) => item.model === "gpt-6-astra",
  );
  expect(existingModel).toBeTruthy();
  // Explicitly priced local fixtures exercise a long form; no inference is performed.
  const modelSeed = await context.request.post("/admin/api/models", {
    headers: csrf,
    data: {
      provider_id: "chatgpt",
      model: "gpt-6-astra",
      kind: "text",
      enabled: true,
      revision: existingModel.revision,
      token_prices: ["standard", "fast", "flex"].flatMap((tier) =>
        [0, 200000].map((min_input_tokens) => ({
          tier,
          min_input_tokens,
          input_rate: "1",
          cached_rate: "0.5",
          cache_write_rate: "1",
          output_rate: "2",
        })),
      ),
      image_prices: [],
    },
  });
  expect(modelSeed.ok()).toBe(true);
  const denied = await context.request.post("/admin/api/models/status", {
    data: { provider_id: "chatgpt", model: "gpt-6-astra", enabled: false, revision: 1 },
  });
  expect(denied.status()).toBe(403);

  await page.getByRole("link", { name: "套餐管理", exact: true }).click();
  await page.getByRole("button", { name: "添加套餐", exact: true }).click();
  const planDialog = page.getByRole("dialog");
  await planDialog.getByLabel("套餐名称", { exact: true }).fill(`回归套餐 ${suffix}`);
  await planDialog.getByLabel("订阅额度周期费用上限（美元）", { exact: true }).fill("10");
  await planDialog.getByLabel("订阅额度 5 小时费用上限（美元）", { exact: true }).fill("2");
  await selectOption(
    page,
    planDialog
      .getByRole("group", { name: "订阅模型范围", exact: true })
      .getByLabel("访问范围", { exact: true }),
    "指定模型",
  );
  await planDialog.getByLabel("chatgpt/gpt-6-astra", { exact: true }).check();
  await planDialog.getByRole("button", { name: /到期免费访问/ }).click();
  await selectOption(
    page,
    planDialog
      .getByRole("group", { name: "免费层模型范围", exact: true })
      .getByLabel("访问范围", { exact: true }),
    "无模型",
  );
  await expect(planDialog.getByRole("button", { name: "保存", exact: true })).toBeInViewport();
  await expect(planDialog.getByRole("button", { name: "关闭", exact: true })).toBeInViewport();
  await expectModalRegions(planDialog);
  await page.screenshot({ path: resolve(evidence, "02-plan-selection.png"), fullPage: true });
  await planDialog.getByRole("button", { name: "保存", exact: true }).click();
  await expect(planDialog).not.toBeVisible();
  await expect(page.getByText(`回归套餐 ${suffix}`, { exact: true })).toBeVisible();
  const plans = await (await context.request.get("/admin/api/plans")).json();
  const plan = plans.items.find((item: { name: string }) => item.name === `回归套餐 ${suffix}`);
  expect(plan.models).toEqual([{ provider_id: "chatgpt", model: "gpt-6-astra" }]);

  await page.getByRole("link", { name: "模型配置", exact: true }).click();
  await expect(page.locator("[data-slot=breadcrumb-page]")).toHaveText("模型配置");
  await expect(page.getByRole("columnheader", { name: "计费方式", exact: true })).toBeVisible();
  const modelRow = page
    .getByRole("row")
    .filter({ has: page.getByText("gpt-6-astra", { exact: true }) });
  await modelRow.getByRole("button", { name: "编辑", exact: true }).click();
  await expect(page.getByRole("dialog").getByLabel("模型名称", { exact: true })).toHaveValue(
    "gpt-6-astra",
  );
  await page.getByRole("dialog").getByRole("button", { name: "添加计费规则", exact: true }).click();
  await page.getByRole("dialog").getByRole("button", { name: "添加计费规则", exact: true }).click();
  await expect(
    page.getByRole("dialog").getByRole("button", { name: "保存", exact: true }),
  ).toBeInViewport();
  await expect(
    page.getByRole("dialog").getByRole("button", { name: "关闭", exact: true }),
  ).toBeInViewport();
  await page
    .getByRole("dialog")
    .locator("form > [data-slot=scroll-area] [data-slot=scroll-area-viewport]")
    .evaluate((element) => {
      element.scrollTop = element.scrollHeight;
    });
  await expectModalRegions(page.getByRole("dialog"));
  await expect(page.getByRole("dialog").getByText("计费规则 8", { exact: true })).toBeInViewport();
  await expect(
    page.getByRole("dialog").getByLabel("输出", { exact: true }).last(),
  ).toBeInViewport();
  await page.screenshot({ path: resolve(evidence, "03-model-pricing.png"), fullPage: true });
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "移除规则", exact: true })
    .last()
    .click();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "移除规则", exact: true })
    .last()
    .click();
  await page.getByRole("dialog").getByRole("button", { name: "保存", exact: true }).click();
  await expect(page.getByRole("dialog")).not.toBeVisible();

  await page.getByRole("link", { name: "虚拟账户", exact: true }).click();
  await page.getByRole("button", { name: "创建虚拟账户", exact: true }).click();
  const accountDialog = page.getByRole("dialog");
  await accountDialog.getByLabel("账户名称", { exact: true }).fill(`回归账户 A ${suffix}`);
  await accountDialog.getByLabel("登录用户名", { exact: true }).fill(`test-a-${suffix}`);
  await accountDialog.getByLabel("登录密码", { exact: true }).fill("test-only-password");
  await accountDialog.getByLabel("邮箱", { exact: true }).fill(`test-a-${suffix}@example.test`);
  await selectOption(
    page,
    accountDialog.getByLabel("订阅套餐", { exact: true }),
    `回归套餐 ${suffix}`,
  );
  await accountDialog.getByRole("button", { name: "创建账户", exact: true }).click();
  await expect(accountDialog).not.toBeVisible();
  await page.getByRole("link", { name: `回归账户 A ${suffix}`, exact: true }).click();
  await expect(page.getByLabel("账户名称", { exact: true })).toBeVisible();
  await expect(page.getByLabel("提供商", { exact: true })).toBeDisabled();
  await expect(
    page.getByLabel("订阅套餐", { exact: true }).filter({ hasText: `回归套餐 ${suffix}` }),
  ).toBeVisible();
  await page.screenshot({ path: resolve(evidence, "04-consumer-detail.png"), fullPage: true });
  const firstId = new URL(page.url()).searchParams.get("id");
  const createSecond = await context.request.post("/admin/api/consumers", {
    headers: csrf,
    data: {
      username: `test-b-${suffix}`,
      password: "test-only-password",
      name: `回归账户 B ${suffix}`,
      email: `test-b-${suffix}@example.test`,
      provider_id: "chatgpt",
      plan_id: plan.id,
      subscription_expires_at: null,
      enabled: true,
    },
  });
  expect(createSecond.ok()).toBe(true);
  const second = await createSecond.json();
  await page.evaluate(
    (id) => window.history.pushState(null, "", `?id=${encodeURIComponent(id)}`),
    second.id,
  );
  await expect(page.getByLabel("账户名称", { exact: true })).toBeVisible();
  await expect(page.getByLabel("账户名称", { exact: true })).toHaveValue(`回归账户 B ${suffix}`);
  await page.getByRole("tab", { name: "账户设置", exact: true }).click();
  await expectActiveTab(page, "账户设置");
  await expect(page.getByLabel("执行供应账户", { exact: true })).toHaveAttribute(
    "placeholder",
    "不提供执行服务",
  );
  await page.getByRole("button", { name: "保存供应绑定", exact: true }).click();
  await expect(
    page.locator('[data-sonner-toast][data-type="success"]').filter({ hasText: "已保存" }).last(),
  ).toBeVisible();
  await page.getByRole("tab", { name: "账户设置", exact: true }).click();
  await expectActiveTab(page, "账户设置");
  await expect(page.getByLabel("账户名称", { exact: true })).toBeVisible();
  const first = await (await context.request.get(`/admin/api/consumers/${firstId}`)).json();
  expect(first.name).toBe(`回归账户 A ${suffix}`);

  await page.getByRole("tab", { name: "记录查询", exact: true }).click();
  await expectActiveTab(page, "记录查询");
  await selectOption(page, page.getByLabel("记录类别", { exact: true }), "客户端偏好与安装状态");
  await expect(page.getByRole("heading", { name: /只读/ })).toBeVisible();
  await expect(
    page.getByRole("main").getByRole("button", { name: "保存", exact: true }),
  ).toHaveCount(0);

  await page.goto("/admin/consumers/detail/?id=review-consumer");
  await page.getByRole("tab", { name: "记录查询", exact: true }).click();
  await selectOption(page, page.getByLabel("记录类别", { exact: true }), "任务与执行来源");
  const tasks = await (
    await context.request.get("/admin/api/consumers/review-consumer/records?kind=task")
  ).json();
  expect(tasks.items.length).toBeGreaterThan(0);
  await expect(
    page.getByRole("cell", { name: tasks.items[0].value.task.title, exact: true }),
  ).toBeVisible();
  await selectOption(page, page.getByLabel("记录类别", { exact: true }), "客户端偏好与安装状态");
  await selectOption(page, page.getByLabel("状态类别", { exact: true }), "浏览器设置");
  await expect(page.getByRole("heading", { name: "审批偏好", exact: true })).toBeVisible();
  const browserState = await (
    await context.request.get("/admin/api/consumers/review-consumer/client-state/browser_settings")
  ).json();
  const sites = Object.keys(browserState.value.rules.origin);
  expect(sites.length).toBeGreaterThan(0);
  await expect(page.getByRole("cell", { name: sites[0], exact: true })).toBeVisible();
  await expect(
    page.getByRole("main").getByRole("button", { name: "保存", exact: true }),
  ).toHaveCount(0);
  await page.screenshot({
    path: resolve(evidence, "07-readonly-browser-rules.png"),
    fullPage: true,
  });
  await selectOption(page, page.getByLabel("状态类别", { exact: true }), "已安装插件");
  const pluginState = await (
    await context.request.get("/admin/api/consumers/review-consumer/client-state/installed_plugins")
  ).json();
  expect(pluginState.value.plugins.length).toBeGreaterThan(0);
  await expect(
    page.getByRole("cell", { name: pluginState.value.plugins[0].name, exact: true }),
  ).toBeVisible();
  await page.getByText("查看详情", { exact: true }).first().click();
  await expect(
    page.getByText(pluginState.value.plugins[0].description, { exact: true }),
  ).toBeVisible();
  await page.screenshot({ path: resolve(evidence, "08-readonly-plugin.png"), fullPage: true });

  await page.getByRole("link", { name: "系统设置", exact: true }).click();
  await page.getByLabel("UA 规则", { exact: true }).fill("regression-one\nregression-two");
  await page.getByRole("button", { name: "保存", exact: true }).click();
  await expect(page.getByText("网关设置已保存", { exact: true })).toBeVisible();
  expect(
    (await (await context.request.get("/admin/api/settings/gateway")).json()).ua_rules,
  ).toEqual(["regression-one", "regression-two"]);
  await context.request.put("/admin/api/settings/gateway", {
    headers: csrf,
    data: { ua_mode: "blacklist", ua_rules: [] },
  });

  let callbackUrl: URL | undefined;
  const callbackServer = createServer((request, response) => {
    const url = new URL(request.url ?? "/", "http://localhost");
    if (url.pathname !== "/auth/callback") {
      response.writeHead(404).end();
      return;
    }
    callbackUrl = url;
    response
      .writeHead(200, { "Content-Type": "text/plain; charset=utf-8" })
      .end("本地测试授权回调已接收");
  });
  await new Promise<void>((resolve) => callbackServer.listen(0, "127.0.0.1", resolve));
  const callbackAddress = `http://localhost:${(callbackServer.address() as AddressInfo).port}/auth/callback`;
  const verifier = "a".repeat(64);
  const challenge = createHash("sha256").update(verifier).digest("base64url");
  const oauthQuery = new URLSearchParams({
    response_type: "code",
    client_id: "app_EMoamEEZ73f0CkXaXp7hrann",
    redirect_uri: callbackAddress,
    scope: "openid profile email offline_access",
    code_challenge: challenge,
    code_challenge_method: "S256",
    state: "frontend-regression",
  });
  const publicContext = await context
    .browser()!
    .newContext({ baseURL, viewport: { width: 1200, height: 1000 } });
  const publicPage = await publicContext.newPage();
  await publicPage.goto(`/admin/authorize/?${oauthQuery}`);
  await expect(
    publicPage.getByRole("heading", { name: "登录虚拟账户", exact: true }),
  ).toBeVisible();
  await expect(publicPage.getByLabel("虚拟账户用户名", { exact: true })).toBeVisible();
  await expect(publicPage.getByRole("navigation", { name: "主导航" })).toHaveCount(0);
  await page.screenshot({ path: resolve(evidence, "05-settings.png"), fullPage: true });
  await publicPage.screenshot({
    path: resolve(evidence, "06-public-authorization.png"),
    fullPage: true,
  });
  await publicPage.setViewportSize({ width: 390, height: 844 });
  expect(
    await publicPage.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    ),
  ).toBeLessThanOrEqual(1);
  await publicPage.screenshot({
    path: resolve(evidence, "18-public-authorization-mobile.png"),
    fullPage: true,
  });
  await publicPage.getByLabel("虚拟账户用户名", { exact: true }).fill(`test-a-${suffix}`);
  await publicPage.getByLabel("密码", { exact: true }).fill("wrong-password");
  await publicPage.getByRole("button", { name: "登录并授权", exact: true }).click();
  await expect(publicPage.locator('[data-sonner-toast][data-type="error"]').last()).toBeVisible();
  await expect(publicPage.getByLabel("虚拟账户用户名", { exact: true })).toHaveValue(
    `test-a-${suffix}`,
  );
  await publicPage.getByLabel("密码", { exact: true }).fill("test-only-password");
  await publicPage.getByRole("button", { name: "登录并授权", exact: true }).click();
  await expect(publicPage.getByText("本地测试授权回调已接收", { exact: true })).toBeVisible();
  expect(callbackUrl?.searchParams.get("state")).toBe("frontend-regression");
  expect(callbackUrl?.searchParams.has("code")).toBe(true);
  const tokenResponse = await publicContext.request.post("/api/oauth/chatgpt/oauth/token", {
    form: {
      grant_type: "authorization_code",
      code: callbackUrl!.searchParams.get("code")!,
      client_id: "app_EMoamEEZ73f0CkXaXp7hrann",
      redirect_uri: callbackAddress,
      code_verifier: verifier,
    },
  });
  expect(tokenResponse.ok()).toBe(true);
  const tokens = await tokenResponse.json();
  expect(typeof tokens.access_token).toBe("string");
  expect(tokens.scope.split(" ").sort()).toEqual(
    ["openid", "profile", "email", "offline_access"].sort(),
  );
  const adminAttempt = await publicContext.request.get("/admin/api/consumers", {
    headers: { Authorization: `Bearer ${tokens.access_token}` },
  });
  expect(adminAttempt.status()).toBe(401);
  await publicContext.close();
  await new Promise<void>((resolve, reject) =>
    callbackServer.close((error) => (error ? reject(error) : resolve())),
  );
  expect(pageErrors).toEqual([]);
});
