import { test, expect } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

test("plans without a description remain readable during a backend upgrade", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.route("**/user/api/**", async (route) => {
    const path = new URL(route.request().url()).pathname.replace("/user/api", "");
    let value: unknown = { items: [] };
    if (path === "/session")
      value = {
        csrf_token: "fixture",
        user: { name: "测试用户", username: "test", email: "test@example.test", enabled: true },
      };
    if (path === "/wallet") value = { balance_usd: "0" };
    if (path === "/plans")
      value = {
        items: [
          {
            id: "existing",
            name: "原有套餐",
            provider_id: "grok",
            plan_type: "supergrok",
            sale_price_usd: "10",
            duration_days: 30,
            revision: 0,
            model_access: "all",
            models: [],
            spending_windows: [],
          },
        ],
      };
    await route.fulfill({ json: value });
  });
  await page.goto("/user/plans/");
  await page.getByRole("button", { name: "原有套餐", exact: true }).click();
  await expect(page.getByRole("dialog").getByText("暂无描述", { exact: true })).toBeVisible();
  expect(errors).toEqual([]);
});

test("plan descriptions save as Markdown and user details show complete benefits", async ({
  page,
}, info) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  const contracts = JSON.parse(
    await readFile(resolve("../crates/codex2api-admin/tests/contracts.json"), "utf8"),
  );
  const description =
    "## 开发者权益\n\n**完整 Markdown 介绍**\n\n" +
    "支持日常开发与长文本分析。".repeat(100) +
    "\n\n| 权益 | 内容 |\n| --- | --- |\n| 服务 | 编程协作 |\n\n描述末尾标记\n\n<script>alert('unsafe')</script>";
  const plan = {
    ...contracts.plans.items.find((p: { plan_type: string }) => p.plan_type === "plus"),
    id: "local-grok",
    name: "本地开发套餐",
    provider_id: "grok",
    plan_type: "supergrok_plus",
    description: "",
    sale_price_usd: "18.00",
    duration_days: 30,
    model_access: "all",
    models: [],
    spending_windows: [
      { duration_seconds: 604800, cost_limit_usd: "100" },
      { duration_seconds: 18000, cost_limit_usd: "20" },
    ],
  };
  let saves = 0;
  await page.route("**/admin/api/**", async (route) => {
    const path = new URL(route.request().url()).pathname.replace("/admin/api", "");
    let value: unknown = { items: [] };
    if (path === "/session") value = contracts.session;
    if (path === "/plans") value = { items: [plan] };
    if (path === `/plans/${plan.id}` && route.request().method() === "PUT") {
      const input = route.request().postDataJSON();
      expect(input.description).toBe(description);
      Object.assign(plan, input);
      saves += 1;
      value = plan;
    }
    await route.fulfill({ json: value });
  });
  await page.goto("/admin/plans/");
  await page.getByRole("button", { name: "编辑", exact: true }).click();
  await page.getByLabel("套餐描述", { exact: true }).fill(description);
  await page.getByRole("dialog").getByRole("button", { name: "保存", exact: true }).click();
  await expect.poll(() => saves).toBe(1);

  await page.route("**/user/api/**", async (route) => {
    const path = new URL(route.request().url()).pathname.replace("/user/api", "");
    let value: unknown = { items: [] };
    if (path === "/session")
      value = {
        csrf_token: "fixture-csrf",
        user: {
          id: "user",
          username: "user",
          name: "测试用户",
          email: "user@example.test",
          enabled: true,
          created_at: "2026-01-01T00:00:00Z",
          wallet_balance_usd: "100",
          revision: 1,
        },
      };
    if (path === "/plans")
      value = {
        items: [
          {
            ...plan,
            models: [
              { provider_id: "grok", model: "grok-4.7", kind: "text" },
              { provider_id: "grok", model: "my-custom-model", kind: "text" },
            ],
          },
        ],
      };
    if (path === "/wallet") value = { balance_usd: "100" };
    await route.fulfill({ json: value });
  });
  await page.goto("/user/plans/");
  const summary = page.getByLabel("套餐描述摘要", { exact: true });
  await expect(summary.getByText("完整 Markdown 介绍", { exact: true })).toBeVisible();
  expect(
    await summary.evaluate((element) => element.getBoundingClientRect().height),
  ).toBeLessThanOrEqual(65);
  await expect(page.getByText("全部已启用模型")).toHaveCount(0);
  await page.getByRole("button", { name: "本地开发套餐", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByRole("heading", { name: "本地开发套餐" })).toBeVisible();
  await expect(dialog.getByText("$18.00 USD", { exact: true })).toBeVisible();
  await expect(dialog.getByText("30 天", { exact: true })).toBeVisible();
  await expect(dialog.getByText("描述末尾标记", { exact: true })).toBeAttached();
  await expect(dialog.getByRole("cell", { name: "编程协作", exact: true })).toBeAttached();
  await expect(dialog.getByRole("cell", { name: "grok-4.7", exact: true })).toBeAttached();
  await expect(dialog.getByRole("cell", { name: "my-custom-model", exact: true })).toBeAttached();
  await expect(dialog.getByRole("cell", { name: "$100.00", exact: true })).toBeAttached();
  await expect(dialog.locator("script")).toHaveCount(0);
  await page.screenshot({ path: info.outputPath("plan-details.png") });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(
    await dialog.evaluate((element) => element.getBoundingClientRect().width),
  ).toBeLessThanOrEqual(390);
  await expect(dialog.getByRole("button", { name: "下单", exact: true })).toBeVisible();
  expect(errors).toEqual([]);
});
