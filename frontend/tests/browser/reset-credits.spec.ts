import { test, expect } from "@playwright/test";
import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";

test("reset cards clear real fixture usage without changing subscription or bills", async ({
  page,
  context,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
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
  const updated = await context.request.put("/admin/api/plans/plus", {
    headers,
    data: {
      name: plan.name,
      provider_id: "chatgpt",
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
      enabled: true,
      revision: plan.revision,
    },
  });
  expect(updated.ok()).toBe(true);
  const accountPath = "/admin/api/consumers/review-consumer";
  const account = await (await context.request.get(accountPath)).json();
  const expires = new Date(Date.now() + 14 * 86400000).toISOString();
  expect(
    (
      await context.request.put(accountPath, {
        headers,
        data: {
          username: account.username,
          password: "",
          name: account.name,
          email: account.email,
          provider_id: "chatgpt",
          plan_id: account.plan_id,
          subscription_expires_at: expires,
          enabled: true,
        },
      })
    ).ok(),
  ).toBe(true);
  const before = await (await context.request.get(`${accountPath}/usage`)).json();
  const subscription = await (await context.request.get(accountPath)).json();
  expect(Number(before.quota.rate_limit.primary_window.used_usd)).toBeGreaterThan(0);
  await page.goto("/admin/consumers/detail/?id=review-consumer");
  await page.getByRole("tab", { name: "重置卡", exact: true }).click();
  await expect(page.getByRole("button", { name: "发放重置卡", exact: true })).toBeEnabled();
  await expect(page.getByLabel("发放数量", { exact: true })).toHaveCount(0);
  await page.getByRole("button", { name: "发放重置卡", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "发放重置卡", exact: true });
  await expect(dialog.getByLabel("有效时长（天）", { exact: true })).toHaveValue("30");
  await dialog.getByLabel("发放数量", { exact: true }).fill("2");
  await dialog.getByLabel("管理备注", { exact: true }).fill("本地验收发卡");
  await dialog.getByRole("button", { name: "确认发放", exact: true }).click();
  await expect(page.getByText("可用 2 张", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "使用", exact: true }).first().click();
  await page.getByRole("alertdialog").getByRole("button", { name: "确认", exact: true }).click();
  await expect(page.getByText("可用 1 张", { exact: true })).toBeVisible();
  await expect(page.getByRole("cell", { name: "已使用", exact: true })).toBeVisible();
  await expect(page.getByRole("cell", { name: "管理员", exact: true })).toBeVisible();
  const after = await (await context.request.get(`${accountPath}/usage`)).json();
  expect(after.quota.rate_limit.primary_window.used_usd).toBe("0");
  expect(after.quota.rate_limit.secondary_window).toBeNull();
  expect(after.quota.rate_limit.primary_window.reset_after_seconds).toBeGreaterThan(2591990);
  expect(after.quota.billing).toEqual(before.quota.billing);
  expect(after.summary).toEqual(before.summary);
  expect((await (await context.request.get(accountPath)).json()).subscription_expires_at).toBe(
    subscription.subscription_expires_at,
  );
  // No-op feedback stays in a toast and preserves the remaining card.
  await page
    .getByRole("button", { name: "使用", exact: true })
    .and(page.locator(":enabled"))
    .first()
    .click();
  await page.getByRole("alertdialog").getByRole("button", { name: "确认", exact: true }).click();
  await expect(
    page.locator("[data-sonner-toast]").filter({ hasText: "没有可重置的当前用量" }),
  ).toBeVisible();
  await page.getByRole("alertdialog").getByRole("button", { name: "取消", exact: true }).click();
  await expect(page.locator('[data-slot="alert-dialog-overlay"]')).toHaveCount(0);
  await expect(page.getByText("可用 1 张", { exact: true })).toBeVisible();
  const output = resolve(
    process.env.CODEX2API_TEST_OUTPUT_DIR ?? "../target/review-next-reset-credits",
    "screenshots",
  );
  await mkdir(output, { recursive: true });
  await page.screenshot({ path: resolve(output, "reset-credits-desktop.png"), fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(page.getByRole("button", { name: "发放重置卡", exact: true })).toBeVisible();
  await page.screenshot({ path: resolve(output, "reset-credits-mobile.png"), fullPage: true });
  expect(errors).toEqual([]);
});
