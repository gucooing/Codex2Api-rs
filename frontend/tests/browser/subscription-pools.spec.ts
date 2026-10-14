import { grantSubscription } from "./subscription-fixtures";
import { test, expect } from "@playwright/test";

test("subscriptions inherit plan pools and support independent limits", async ({
  page,
  context,
}, info) => {
  const suffix = Date.now().toString(36);
  const errors: string[] = [];
  const requests: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("request", (request) => requests.push(new URL(request.url()).pathname));
  await page.goto("/admin/");
  await page.getByLabel("用户名", { exact: true }).fill("admin");
  await page.getByLabel("密码", { exact: true }).fill("admin");
  await page.getByRole("button", { name: "登录", exact: true }).click();
  await expect(page.locator("[data-slot=breadcrumb-page]")).toHaveText("概览");
  const session = await (await context.request.get("/admin/api/session")).json();
  const headers = { "X-CSRF-Token": session.csrf_token };
  const poolName = `套餐号池-${suffix}`;
  const createPool = await context.request.post("/admin/api/supplier-tags", {
    headers,
    data: { provider_id: "chatgpt", name: poolName },
  });
  expect(createPool.ok()).toBe(true);
  const tags = await (await context.request.get("/admin/api/supplier-tags/options")).json();
  const pool = tags.items.find((tag: { name: string }) => tag.name === poolName);
  const planName = `订阅套餐-${suffix}`;
  const createPlan = await context.request.post("/admin/api/plans", {
    headers,
    data: {
      name: planName,
      description: "",
      provider_id: "chatgpt",
      plan_type: "plus",
      model_access: "all",
      models: [],
      spending_windows: [],
      allow_purchase: true,
      rpm_limit: 40,
      concurrency_limit: 10,
      sale_price_usd: "10",
      duration_days: 30,
      supplier_tag_id: null,
      revision: null,
    },
  });
  expect(createPlan.ok()).toBe(true);
  const plan = await createPlan.json();
  const createUser = await context.request.post("/admin/api/users", {
    headers,
    data: {
      username: `pool-user-${suffix}`,
      name: "号池测试用户",
      email: `${suffix}@example.test`,
      password: "local-fixture-password",
      enabled: true,
      revision: null,
    },
  });
  expect(createUser.ok()).toBe(true);
  const user = await createUser.json();
  const subscriptions = await (
    await context.request.get(`/admin/api/subscriptions?user_id=${user.id}`)
  ).json();
  expect(subscriptions.items).toHaveLength(0);
  const subscription = await grantSubscription(context.request, headers, user.id, plan.id);

  await page.goto("/admin/plans/");
  await page.getByLabel("搜索套餐", { exact: true }).fill(planName);
  await page.getByRole("button", { name: "查询", exact: true }).click();
  await page
    .getByRole("row")
    .filter({ hasText: planName })
    .getByRole("button", { name: "编辑", exact: true })
    .click();
  await page.getByLabel("套餐供应号池", { exact: true }).click();
  await page.getByRole("option", { name: poolName, exact: true }).click();
  await page.getByRole("dialog").getByRole("button", { name: "保存", exact: true }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  const plans = await (
    await context.request.get(`/admin/api/plans?search=${encodeURIComponent(planName)}`)
  ).json();
  expect(plans.items[0].supplier_tag_id).toBe(pool.id);
  const boundTags = await (await context.request.get("/admin/api/supplier-tags/options")).json();
  expect(boundTags.items.find((tag: { id: string }) => tag.id === pool.id).binding_count).toBe(1);

  const optionsLoaded = page.waitForResponse((response) =>
    new URL(response.url()).pathname.endsWith("/plans/options"),
  );
  await page.route(`**/admin/api/subscriptions/${subscription.id}`, async (route) => {
    await optionsLoaded;
    await route.continue();
  });
  await page.goto(`/admin/subscriptions/detail/?id=${subscription.id}`);
  await expect(page.getByLabel("套餐", { exact: true })).toHaveText(planName);
  await expect(page.getByRole("heading", { name: "供应绑定", exact: true })).toHaveCount(0);
  await expect(page.getByLabel("标签号池", { exact: true })).toHaveText("继承套餐号池");
  await expect(page.getByLabel("分配账户", { exact: true })).toHaveCount(0);
  await expect(page.getByLabel("RPM 限制", { exact: true })).toBeEnabled();
  await page.getByLabel("RPM 限制", { exact: true }).fill("23");
  const saved = page.waitForResponse(
    (response) => response.url().endsWith("/rate-limit") && response.request().method() === "PUT",
  );
  await page.getByRole("button", { name: "保存限制", exact: true }).click();
  expect((await saved).ok()).toBe(true);
  await page.reload();
  await expect(page.getByLabel("RPM 限制", { exact: true })).toHaveValue("23");
  await page.screenshot({ path: info.outputPath("subscription-settings.png") });
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(page.getByLabel("RPM 限制", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "保存限制", exact: true })).toBeVisible();
  expect(requests.some((path) => /^\/admin\/api\/subscriptions\/[^/]+\/routing$/.test(path))).toBe(
    true,
  );
  expect(errors).toEqual([]);
});
