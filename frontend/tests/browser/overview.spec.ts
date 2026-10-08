import { test, expect, type Page } from "@playwright/test";
import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";

test("overview statistics aggregate real records, filter dimensions, and retain data on failure", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (
      ["error", "warning"].includes(message.type()) &&
      !/status of (401|500)/.test(message.text())
    )
      errors.push(message.text());
  });
  let release: () => void = () => {};
  const delayed = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route("**/admin/api/overview/usage?**", async (route) => {
    await delayed;
    await route.continue();
  });
  await page.goto("/admin/");
  await page.getByLabel("用户名", { exact: true }).fill("admin");
  await page.getByLabel("密码", { exact: true }).fill("admin");
  await page.getByRole("button", { name: "登录", exact: true }).click();
  const form = page.getByRole("form", { name: "概览统计筛选" });
  await expect(form).toBeVisible();
  await expect(page.getByLabel("用量统计组合图")).toBeVisible();
  await expect(page.locator('[data-slot="chart"]')).toHaveCount(1);
  await expect(page.getByLabel("图表指标", { exact: true })).toHaveCount(0);
  await expect(page.getByRole("table")).toHaveCount(0);
  await expect(form.getByRole("button", { name: "查询", exact: true })).toBeDisabled();
  const initial = page.waitForResponse((response) =>
    response.url().includes("/admin/api/overview/usage?"),
  );
  release();
  const data = await (await initial).json();
  expect(data.summary.request_count).toBeGreaterThan(100);
  expect(data.summary.missing_cache_requests).toBeGreaterThan(0);
  expect(data.summary.cache_rate).toBeCloseTo(
    (100 * data.summary.cached_tokens) / data.summary.input_tokens,
    8,
  );
  await page.unroute("**/admin/api/overview/usage?**");
  await expect(page.getByText(/缓存信息缺失 105 个请求/)).toHaveCount(0);
  const modelChart = page.getByLabel("用量统计组合图");
  await expect(modelChart.locator(".recharts-yAxis")).toHaveCount(0);
  await expect(modelChart.locator(".recharts-bar")).toHaveCount(3);
  await expect(modelChart.locator(".recharts-line")).toHaveCount(4);
  const strokes = await modelChart
    .locator(".recharts-line")
    .evaluateAll((lines) =>
      lines.map((line) => line.querySelector("path, circle")?.getAttribute("stroke")),
    );
  expect(new Set(strokes).size).toBe(4);
  for (const label of ["Token 用量", "请求数", "缓存率", "费用（USD）"])
    await expect(
      modelChart.locator(".recharts-legend-wrapper").getByText(label, { exact: true }),
    ).toBeVisible();
  await expect(modelChart.locator(".recharts-legend-wrapper").getByText(/^模型：/)).toHaveCount(0);
  expect(
    data.model_usage.reduce(
      (sum: number, item: { total_tokens: number }) => sum + item.total_tokens,
      0,
    ),
  ).toBe(data.summary.total_tokens);

  await form.getByLabel("模型", { exact: true }).fill("review-cycle-model");
  const submit = async () => {
    const response = page.waitForResponse((response) =>
      response.url().includes("/admin/api/overview/usage?"),
    );
    await form.getByRole("button", { name: "查询", exact: true }).click();
    return (await response).json();
  };
  const filtered = await submit();
  expect(filtered.summary.request_count).toBe(1);
  expect(filtered.summary.total_tokens).toBe(3_800_000);
  const summary = page.getByLabel("统计摘要");
  await expect(summary.getByText("3.8M", { exact: true })).toBeVisible();
  await expect(summary.getByText("33.3%", { exact: true })).toBeVisible();
  await submit(); // The same query still refreshes.
  await choose(page, "统计维度", "按模型");
  const model = await submit();
  expect(model.rows[0].key).toBe("review-cycle-model");
  await expect(
    modelChart
      .locator(".recharts-legend-wrapper")
      .getByText("模型：review-cycle-model", { exact: true }),
  ).toHaveCount(0);
  const subject = form.getByLabel("用户/账户", { exact: true });
  const supplier = form.getByLabel("供应账户", { exact: true });
  await subject.fill("review-consumer");
  await page.getByRole("option", { name: "账户 · review-consumer", exact: true }).click();
  await choose(page, "统计维度", "按虚拟账户");
  const account = await submit();
  expect(account.summary.request_count).toBe(1);
  expect(account.rows[0].key).toBe("review-consumer");
  await expect(modelChart.getByText("review-consumer", { exact: true })).toBeVisible();

  const session = await (await page.request.get("/admin/api/session")).json();
  const username = `overview-${Date.now().toString(36)}`;
  const created = await page.request.post("/admin/api/users", {
    headers: { "x-csrf-token": session.csrf_token },
    data: {
      username,
      name: username,
      email: `${username}@example.test`,
      password: "overview-test-password",
      enabled: true,
    },
  });
  expect(created.ok()).toBeTruthy();
  const user = await created.json();
  await subject.fill(username);
  await page.getByRole("option", { name: `用户 · ${username}`, exact: true }).click();
  const userResponse = page.waitForResponse((response) => {
    const url = new URL(response.url());
    return (
      url.pathname === "/admin/api/overview/usage" && url.searchParams.get("user_id") === user.id
    );
  });
  expect((await submit()).summary.request_count).toBe(0);
  expect(new URL((await userResponse).url()).searchParams.has("virtual_account")).toBe(false);

  await supplier.fill("review-supplier-disabled@example.test");
  await page.getByRole("option", { name: /^本地验收停用账户/ }).click();
  expect((await submit()).summary.request_count).toBe(0);
  const restored = page.waitForResponse((response) =>
    response.url().includes("/admin/api/overview/usage?"),
  );
  await page.reload();
  const restoredQuery = new URL((await restored).url()).searchParams;
  expect(restoredQuery.get("user_id")).toBe(user.id);
  expect(restoredQuery.get("supplier_id")).toBe("review-supplier-disabled");
  expect(restoredQuery.has("virtual_account")).toBe(false);
  await expect(subject).toHaveValue(`用户 · ${username}`);
  await expect(supplier).toHaveValue("本地验收停用账户");

  await subject.fill("review-consumer");
  await page.getByRole("option", { name: "账户 · review-consumer", exact: true }).click();
  expect((await submit()).summary.request_count).toBe(1);
  await supplier.fill("review-supplier-active@example.test");
  await page.getByRole("option", { name: /^本地验收供应账户/ }).click();
  expect((await submit()).summary.request_count).toBe(0);
  await supplier.fill("review-supplier-disabled@example.test");
  await page.getByRole("option", { name: /^本地验收停用账户/ }).click();
  expect((await submit()).summary.request_count).toBe(1);

  let statisticsRequests = 0;
  page.on("request", (request) => {
    if (request.url().includes("/overview/usage?")) statisticsRequests++;
  });
  await expect(modelChart.locator(".recharts-line")).toHaveCount(4);
  await expect(modelChart.locator(".recharts-bar")).toHaveCount(1);
  await expect(page.getByRole("checkbox")).toHaveCount(5);
  await modelChart.locator(".recharts-bar .recharts-rectangle").hover();
  const tooltip = modelChart.locator(".recharts-tooltip-wrapper");
  await expect(tooltip).toContainText("Token 用量：3.8M Token");
  await expect(tooltip).toContainText("模型：review-cycle-model：3.8M Token");
  await expect(tooltip).not.toContainText("3,800,000");
  await form.getByLabel("模型", { exact: true }).hover();
  for (const [index, label] of ["Token 用量", "请求数", "缓存率", "费用"].entries()) {
    await page.getByRole("checkbox", { name: label, exact: true }).uncheck();
    await expect(modelChart.locator(".recharts-line")).toHaveCount(3 - index);
  }
  await page.getByRole("checkbox", { name: "模型用量（柱体）", exact: true }).uncheck();
  await expect(modelChart.locator(".recharts-bar")).toHaveCount(0);
  for (const label of ["Token 用量", "请求数", "缓存率", "费用", "模型用量（柱体）"]) {
    await page.getByRole("checkbox", { name: label, exact: true }).check();
  }
  await expect(modelChart.locator(".recharts-line")).toHaveCount(4);
  await expect(modelChart.locator(".recharts-bar")).toHaveCount(1);
  expect(statisticsRequests).toBe(0);

  await choose(page, "时间范围", "自定义");
  await form.getByLabel("开始时间", { exact: true }).fill("2026-09-22T00:00");
  await form.getByLabel("结束时间（不含）", { exact: true }).fill("2026-09-21T00:00");
  await form.getByRole("button", { name: "查询", exact: true }).click();
  await expect(
    page.locator("[data-sonner-toast]").filter({ hasText: "结束时间需晚于开始时间" }),
  ).toBeVisible();
  expect(statisticsRequests).toBe(0);
  const local = (ms: number) =>
    new Date(ms - new Date(ms).getTimezoneOffset() * 60000).toISOString().slice(0, 16);
  await form.getByLabel("开始时间", { exact: true }).fill(local(Date.now() - 7 * 86400000));
  await form.getByLabel("结束时间（不含）", { exact: true }).fill(local(Date.now() + 86400000));
  await submit();
  await page.route("**/admin/api/overview/usage?**", (route) =>
    route.fulfill({
      status: 500,
      contentType: "application/json",
      body: JSON.stringify({ error: { code: "fixture_error", message: "统计刷新验收失败" } }),
    }),
  );
  await form.getByRole("button", { name: "刷新", exact: true }).click();
  await expect(
    page.locator("[data-sonner-toast]").filter({ hasText: "统计刷新验收失败" }),
  ).toBeVisible();
  await expect(summary.getByText("3.8M", { exact: true })).toBeVisible();
  await page.unroute("**/admin/api/overview/usage?**");
  await form.getByLabel("模型", { exact: true }).fill("no-such-statistics-model");
  const empty = await submit();
  expect(empty.summary.request_count).toBe(0);
  await expect(page.getByText("当前筛选范围暂无用量记录", { exact: true })).toBeVisible();
  const reset = page.waitForResponse((response) =>
    response.url().includes("/admin/api/overview/usage?"),
  );
  await form.getByRole("button", { name: "重置", exact: true }).click();
  await reset;
  await expect(form.getByLabel("模型", { exact: true })).toHaveValue("");
  await expect(subject).toHaveValue("");
  await expect(supplier).toHaveValue("");
  const evidence = resolve(process.env.CODEX2API_TEST_OUTPUT_DIR!, "screenshots");
  await mkdir(evidence, { recursive: true });
  await expect(page.locator("[data-sonner-toast]")).toHaveCount(0);
  await page.screenshot({ path: resolve(evidence, "overview-statistics.png"), fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(form.getByRole("button", { name: "查询", exact: true })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
  await page.screenshot({
    path: resolve(evidence, "overview-statistics-mobile.png"),
    fullPage: true,
  });
  expect(errors).toEqual([]);
});

async function choose(page: Page, label: string, option: string) {
  await page.getByLabel(label, { exact: true }).click();
  await page.getByRole("option", { name: option, exact: true }).click();
}
