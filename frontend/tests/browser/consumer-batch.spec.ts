import { test, expect } from "@playwright/test";
import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";

test("server pages, cross-page selection, grant retry, quota and pricing dialogs", async ({
  page,
  context,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto("/admin/");
  await page.getByLabel("用户名", { exact: true }).fill("admin");
  await page.getByLabel("密码", { exact: true }).fill("admin");
  await page.getByRole("button", { name: "登录", exact: true }).click();
  await expect(
    page.locator("[data-slot=breadcrumb-page]").filter({ hasText: "概览" }),
  ).toBeVisible();
  const session = await (await context.request.get("/admin/api/session")).json();
  const headers = { "X-CSRF-Token": session.csrf_token };
  const plans = await (await context.request.get("/admin/api/plans")).json();
  const plan = plans.items.find((item: { id: string }) => item.id === "plus");
  expect(
    (
      await context.request.put("/admin/api/plans/plus", {
        headers,
        data: {
          name: plan.name,
          provider_id: "chatgpt",
          enabled: true,
          revision: plan.revision,
          model_access: "all",
          models: [],
          free_model_access: "none",
          free_models: [],
          free_access_enabled: false,
          spending_windows: [
            { duration_seconds: 2592000, cost_limit_usd: "10" },
            { duration_seconds: 18000, cost_limit_usd: "2" },
          ],
          free_spending_windows: [],
        },
      })
    ).ok(),
  ).toBeTruthy();
  const prefix = `batch-ui-${Date.now()}`;
  const accounts: { id: string; name: string }[] = [];
  for (let index = 0; index < 23; index++) {
    const name = `${prefix}-${index.toString().padStart(2, "0")}`;
    const response = await context.request.post("/admin/api/consumers", {
      headers,
      data: {
        username: name,
        name,
        email: `${name}@example.test`,
        password: "test-only",
        provider_id: "chatgpt",
        plan_id: "plus",
        subscription_expires_at: null,
        enabled: true,
      },
    });
    expect(response.ok()).toBeTruthy();
    accounts.push(await response.json());
  }
  await page.goto("/admin/consumers/");
  await page.getByLabel("搜索账户", { exact: true }).fill(prefix);
  await page.getByRole("button", { name: "查询", exact: true }).click();
  await expect(page.getByText("共 23 条 · 2 页", { exact: true })).toBeVisible();
  await expect(page.getByRole("table").getByRole("row")).toHaveCount(21);
  await expect(page.getByRole("columnheader", { name: "额度", exact: true })).toBeVisible();
  await expect(page.getByRole("progressbar")).not.toHaveCount(0);
  await page.getByRole("checkbox", { name: "选择当前页账户", exact: true }).check();
  await page.getByRole("button", { name: "选择当前筛选的全部 23 个", exact: true }).click();
  const excluded = page.getByRole("table").getByRole("checkbox").first();
  const excludedName = (await excluded.getAttribute("aria-label"))!.replace(/^选择 /, "");
  const excludedId = accounts.find((a) => a.name === excludedName)!.id;
  await excluded.uncheck();
  await expect(page.getByText("已选择 22 个（全部筛选结果）", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "下一页", exact: true }).click();
  await expect(page.getByRole("table").getByRole("row")).toHaveCount(4);
  for (const checkbox of await page.getByRole("table").getByRole("checkbox").all())
    await expect(checkbox).toBeChecked();
  await page.getByRole("button", { name: "发放重置卡", exact: true }).click();
  const grant = page.getByRole("dialog", { name: "发放重置卡", exact: true });
  await expect(grant.getByLabel("有效时长（天）", { exact: true })).toHaveValue("30");
  await grant.getByLabel("启用方式", { exact: true }).click();
  await page.getByRole("option", { name: "定时启用", exact: true }).click();
  await grant.getByLabel("启用时间", { exact: true }).fill("2030-01-02T12:00");
  await grant.getByLabel("发放数量（每个账户）", { exact: true }).fill("2");
  const requests: Record<string, unknown>[] = [];
  let drop = true;
  await page.route("**/admin/api/consumers/batch", async (route) => {
    requests.push(route.request().postDataJSON());
    const response = await route.fetch();
    if (drop) {
      drop = false;
      await route.abort("failed");
    } else await route.fulfill({ response });
  });
  await grant.getByRole("button", { name: "确认", exact: true }).click();
  await expect(page.locator("[data-sonner-toast]").filter({ hasText: "无法连接" })).toBeVisible();
  await grant.getByRole("button", { name: "确认", exact: true }).click();
  await expect(grant).toHaveCount(0);
  expect(requests).toHaveLength(2);
  expect(requests[0]).toEqual(requests[1]);
  expect(requests[0].ids).toEqual([]);
  expect(requests[0].excluded_ids).toEqual([excludedId]);
  const sample = accounts.find((a) => a.id !== excludedId)!;
  const cards = await (
    await context.request.get(`/admin/api/consumers/${sample.id}/reset-credits`)
  ).json();
  expect(cards.items).toHaveLength(2);
  expect(cards.items[0].status).toBe("pending");
  const none = await (
    await context.request.get(`/admin/api/consumers/${excludedId}/reset-credits`)
  ).json();
  expect(none.items).toHaveLength(0);
  // Single-account menus open a form without immediately granting anything.
  await page.getByRole("button", { name: "更多操作", exact: true }).first().click();
  await page.getByRole("menuitem", { name: "发放重置卡", exact: true }).click();
  await expect(grant).toBeVisible();
  await grant.getByRole("button", { name: "取消", exact: true }).click();
  await expect(page.locator('[data-slot="dialog-overlay"]')).toHaveCount(0);
  expect(requests).toHaveLength(2);
  const output = resolve(process.env.CODEX2API_TEST_OUTPUT_DIR!, "screenshots");
  await mkdir(output, { recursive: true });
  await page.screenshot({ path: resolve(output, "consumer-quota.png"), fullPage: true });
  await page.goto("/admin/models/");
  const model = page
    .getByRole("row")
    .filter({ has: page.getByText("gpt-6-astra", { exact: true }) });
  await model.getByRole("button", { name: "编辑", exact: true }).click();
  const pricing = page.getByRole("dialog", { name: "编辑模型", exact: true });
  await expect(pricing.getByLabel("Fast 倍率", { exact: true })).toHaveValue("2");
  await expect(pricing.getByLabel("Flex 倍率", { exact: true })).toHaveValue("0.5");
  await expect(pricing.getByLabel("上下文起点（Token）", { exact: true })).toHaveValue("272001");
  await page.screenshot({ path: resolve(output, "model-pricing.png"), fullPage: true });
  await pricing.getByLabel("Fast 倍率", { exact: true }).fill("3");
  await pricing.getByRole("button", { name: "保存", exact: true }).click();
  await expect(pricing).toHaveCount(0);
  const saved = (await (await context.request.get("/admin/api/models")).json()).items.find(
    (item: { model: string }) => item.model === "gpt-6-astra",
  );
  expect(
    saved.token_prices.find(
      (row: { tier: string; min_input_tokens: number }) =>
        row.tier === "fast" && row.min_input_tokens === 272001,
    ).output_rate,
  ).toBe("225");
  await model.getByRole("button", { name: "编辑", exact: true }).click();
  await expect(pricing.getByLabel("Fast 倍率", { exact: true })).toHaveValue("3");
  await pricing.getByLabel("Fast 倍率", { exact: true }).fill("2");
  await pricing.getByRole("button", { name: "保存", exact: true }).click();
  await expect(pricing).toHaveCount(0);
  expect(errors).toEqual([]);
});

test("reset card detail has a compact toolbar and opens the grant form before any write", async ({
  page,
}) => {
  await page.goto("/admin/");
  await page.getByLabel("用户名", { exact: true }).fill("admin");
  await page.getByLabel("密码", { exact: true }).fill("admin");
  await page.getByRole("button", { name: "登录", exact: true }).click();
  await expect(
    page.locator("[data-slot=breadcrumb-page]").filter({ hasText: "概览" }),
  ).toBeVisible();
  await page.goto("/admin/consumers/detail/?id=review-consumer");
  await page.getByRole("tab", { name: "重置卡", exact: true }).click();
  await expect(page.getByRole("button", { name: "发放重置卡", exact: true })).toBeEnabled();
  await expect(page.getByLabel("发放数量", { exact: true })).toHaveCount(0);
  await expect(page.getByRole("columnheader", { name: "启用时间", exact: true })).toBeVisible();
  await expect(page.getByRole("columnheader", { name: "到期时间", exact: true })).toBeVisible();
  const output = resolve(process.env.CODEX2API_TEST_OUTPUT_DIR!, "screenshots");
  await page.screenshot({ path: resolve(output, "reset-credits-desktop.png"), fullPage: true });
  let writes = 0;
  page.on("request", (request) => {
    if (request.method() === "POST" && request.url().includes("reset-credits")) writes++;
  });
  await page.getByRole("button", { name: "发放重置卡", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "发放重置卡", exact: true });
  await expect(dialog.getByLabel("有效时长（天）", { exact: true })).toHaveValue("30");
  await expect(dialog.getByLabel("启用方式", { exact: true })).toHaveText("立即启用");
  await page.screenshot({ path: resolve(output, "reset-card-grant-dialog.png"), fullPage: true });
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  await expect(page.locator('[data-slot="dialog-overlay"]')).toHaveCount(0);
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({ path: resolve(output, "reset-credits-mobile.png"), fullPage: true });
  expect(writes).toBe(0);
});

test("callback authorization shows a clickable URL and supports copying on HTTP", async ({
  page,
}) => {
  const url = "https://auth.openai.com/oauth/authorize?client_id=test&state=fixture";
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: undefined });
    document.execCommand = (command: string) => {
      if (command === "copy")
        document.documentElement.dataset.copied = window.getSelection()?.toString();
      return command === "copy";
    };
  });
  await page.route("**/admin/api/suppliers/oauth/start", (route) =>
    route.fulfill({
      json: { status: "pending", method: "callback", state: "fixture", authorize_url: url },
    }),
  );
  await page.route("**/admin/api/suppliers/oauth/cancel", (route) =>
    route.fulfill({ json: { ok: true } }),
  );
  await page.goto("/admin/");
  await page.getByLabel("用户名", { exact: true }).fill("admin");
  await page.getByLabel("密码", { exact: true }).fill("admin");
  await page.getByRole("button", { name: "登录", exact: true }).click();
  await expect(
    page.locator("[data-slot=breadcrumb-page]").filter({ hasText: "概览" }),
  ).toBeVisible();
  await page.goto("/admin/suppliers/");
  await page.getByRole("button", { name: "添加供应账户", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "添加供应账户", exact: true });
  await dialog.getByRole("button", { name: "下一步", exact: true }).click();
  await dialog.getByRole("radio", { name: "回调链接", exact: true }).check();
  await dialog.getByRole("button", { name: "下一步", exact: true }).click();
  const link = dialog.getByRole("link", { name: url, exact: true });
  await expect(link).toHaveAttribute("href", url);
  await expect(link).toHaveAttribute("target", "_blank");
  await dialog.getByRole("button", { name: "复制", exact: true }).click();
  await expect(page.locator("html")).toHaveAttribute("data-copied", url);
  await expect(
    page.locator("[data-sonner-toast]").filter({ hasText: "授权地址已复制" }),
  ).toBeVisible();
  await page.screenshot({
    path: resolve(process.env.CODEX2API_TEST_OUTPUT_DIR!, "screenshots", "oauth-callback.png"),
    fullPage: true,
  });
  await dialog.getByRole("button", { name: "关闭", exact: true }).click();
});
