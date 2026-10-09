import { test, expect, type Page } from "@playwright/test";
import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";

async function login(page: Page) {
  await page.goto("/admin/");
  await page.getByLabel("用户名", { exact: true }).fill("admin");
  await page.getByLabel("密码", { exact: true }).fill("admin");
  await page.getByRole("button", { name: "登录", exact: true }).click();
  await expect(page.getByLabel("账户与模型概况")).toBeVisible();
}

test("column choices are selectable, local, persistent and separate for phone and desktop", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await login(page);
  await page.getByRole("link", { name: "用量记录", exact: true }).click();
  await expect(page.getByRole("columnheader")).toHaveCount(8);
  const displayColumns = page.getByRole("button", { name: "显示列", exact: true });
  let reads = 0;
  page.on("request", (request) => {
    if (new URL(request.url()).pathname === "/admin/api/usage") reads++;
  });
  await displayColumns.click();
  await expect(page.getByRole("menuitemcheckbox")).toHaveCount(8);
  await page.getByRole("menuitemcheckbox", { name: "供应账户", exact: true }).click();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("columnheader", { name: "供应账户", exact: true })).toHaveCount(0);
  expect(reads).toBe(0);
  await page.reload();
  await expect(page.getByRole("columnheader")).toHaveCount(7);
  await expect(page.getByRole("columnheader", { name: "供应账户", exact: true })).toHaveCount(0);
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(page.getByRole("columnheader")).toHaveCount(3);
  reads = 0;
  await displayColumns.click();
  await expect(page.getByText("手机显示列", { exact: true })).toBeVisible();
  await page.getByRole("menuitemcheckbox", { name: "用量", exact: true }).click();
  await page.getByRole("menuitemcheckbox", { name: "费用", exact: true }).click();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("columnheader")).toHaveCount(3);
  await expect(page.getByRole("columnheader", { name: "费用", exact: true })).toBeVisible();
  await expect(page.getByRole("columnheader", { name: "用量", exact: true })).toHaveCount(0);
  expect(reads).toBe(0);
  await page.reload();
  await expect(page.getByRole("columnheader", { name: "费用", exact: true })).toBeVisible();
  await expect(page.getByRole("columnheader", { name: "用量", exact: true })).toHaveCount(0);
  await displayColumns.click();
  await page.getByRole("menuitemcheckbox", { name: "费用", exact: true }).click();
  await page.getByRole("menuitemcheckbox", { name: "时间 / 状态", exact: true }).click();
  await expect(
    page.getByRole("menuitemcheckbox", { name: "模型 / 接口", exact: true }),
  ).toBeDisabled();
  await page.getByRole("menuitem", { name: "恢复默认列", exact: true }).click();
  await expect(page.getByRole("columnheader")).toHaveCount(3);
  await expect(page.getByRole("columnheader", { name: "用量", exact: true })).toBeVisible();
  await displayColumns.click();
  await page.getByRole("menuitem", { name: "显示全部列", exact: true }).click();
  await expect(page.getByRole("columnheader")).toHaveCount(8);
  await displayColumns.click();
  await page.getByRole("menuitem", { name: "恢复默认列", exact: true }).click();
  await page.setViewportSize({ width: 1440, height: 1100 });
  await expect(page.getByRole("columnheader")).toHaveCount(7);
  await expect(page.getByRole("columnheader", { name: "供应账户", exact: true })).toHaveCount(0);
  await page.goto("/admin/models/");
  await expect(page.getByRole("columnheader")).toHaveCount(5);
  expect(errors).toEqual([]);
});

test("filters, list views, page size and overview series survive reload locally", async ({
  page,
  browser,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await login(page);
  expect((await page.getByLabel("账户与模型概况").boundingBox())!.height).toBeLessThanOrEqual(50);
  await page.getByRole("link", { name: "供应账户", exact: true }).click();
  await page.getByLabel("搜索账户", { exact: true }).fill("本地验收");
  await page.getByRole("button", { name: "查询", exact: true }).click();
  await page.getByRole("button", { name: "切换为卡片视图", exact: true }).click();
  await page.getByRole("combobox", { name: "每页条数", exact: true }).click();
  await page.getByRole("option", { name: "30 条/页", exact: true }).click();
  await page.reload();
  await expect(page.getByLabel("搜索账户", { exact: true })).toHaveValue("本地验收");
  await expect(page.getByRole("button", { name: "切换为表格视图", exact: true })).toBeVisible();
  await expect(page.getByRole("combobox", { name: "每页条数", exact: true })).toContainText("30");
  await expect(page.getByRole("table")).toHaveCount(0);
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(page.getByRole("table")).toBeVisible();
  await expect(page.getByRole("columnheader", { name: "供应账户", exact: true })).toBeVisible();
  await page.reload();
  await expect(page.getByRole("table")).toBeVisible();
  await page.setViewportSize({ width: 1440, height: 1100 });
  await expect(page.getByRole("table")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "切换为表格视图", exact: true })).toBeVisible();
  // Unsubmitted text should not silently turn into a new query after reloading.
  await page.getByLabel("搜索账户", { exact: true }).fill("未提交的搜索");
  await page.reload();
  await expect(page.getByLabel("搜索账户", { exact: true })).toHaveValue("本地验收");
  await page.getByRole("button", { name: "重置", exact: true }).click();
  await page.reload();
  await expect(page.getByLabel("搜索账户", { exact: true })).toHaveValue("");
  await expect(page.getByRole("button", { name: "切换为表格视图", exact: true })).toBeVisible();

  const independent = await browser.newContext({
    storageState: { cookies: await page.context().cookies(), origins: [] },
  });
  const other = await independent.newPage();
  await other.goto(new URL("/admin/suppliers/", page.url()).href);
  await expect(other.getByRole("button", { name: "切换为卡片视图", exact: true })).toBeVisible();
  await independent.close();

  await page.getByRole("link", { name: "概览", exact: true }).click();
  await page.getByRole("checkbox", { name: "费用", exact: true }).uncheck();
  const overview = page.getByRole("form", { name: "概览统计筛选" });
  await overview.getByLabel("模型", { exact: true }).fill("review-cycle-model");
  await overview.getByRole("button", { name: "查询", exact: true }).click();
  await expect(page.getByLabel("统计摘要").getByText("3.8M", { exact: true })).toBeVisible();
  await page.reload();
  await expect(page.getByRole("checkbox", { name: "费用", exact: true })).not.toBeChecked();
  await expect(overview.getByLabel("模型", { exact: true })).toHaveValue("review-cycle-model");
  await expect(page.getByLabel("统计摘要").getByText("3.8M", { exact: true })).toBeVisible();

  await page.getByRole("link", { name: "用量记录", exact: true }).click();
  const usage = page.locator("form").filter({ has: page.getByLabel("消费账户", { exact: true }) });
  await usage.getByLabel("消费账户", { exact: true }).fill("review-consumer");
  await page.getByRole("option", { name: /^review-consumer\b/ }).click();
  await usage.getByLabel("模型", { exact: true }).fill("pagination-fixture");
  await usage.getByRole("button", { name: "查询", exact: true }).click();
  await expect(
    page.getByRole("cell", { name: "pagination-fixture-104", exact: true }),
  ).toBeVisible();
  await page.reload();
  await expect(usage.getByLabel("消费账户", { exact: true })).toHaveValue("review-consumer");
  await expect(usage.getByLabel("模型", { exact: true })).toHaveValue("pagination-fixture");
  await expect(
    page.getByRole("cell", { name: "pagination-fixture-104", exact: true }),
  ).toBeVisible();
  await page.goto("/admin/consumers/detail/?id=review-consumer");
  await page.getByRole("tab", { name: "用量统计", exact: true }).click();
  await expect(page.getByLabel("模型", { exact: true })).toHaveValue("");

  await page.evaluate(() => localStorage.setItem("codex2api-ui-v1:suppliers.view", '"invalid"'));
  await page.goto("/admin/suppliers/");
  await expect(page.getByRole("button", { name: "切换为卡片视图", exact: true })).toBeVisible();
  expect(errors).toEqual([]);
});

test("mobile tables keep compact aligned rows and open full fields on demand", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (["warning", "error"].includes(message.type()) && !/status of 401/.test(message.text()))
      errors.push(message.text());
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await login(page);
  expect((await page.getByLabel("账户与模型概况").boundingBox())!.height).toBeLessThanOrEqual(108);
  const session = await (await page.request.get("/admin/api/session")).json();
  const proxy = await page.request.post("/admin/api/proxies", {
    headers: { "x-csrf-token": session.csrf_token },
    data: {
      name: "移动端长名称代理".repeat(4),
      protocol: "http",
      host: "long-mobile-review-host.example.invalid",
      port: 9009,
      username: "",
      password: null,
    },
  });
  expect(proxy.ok()).toBe(true);
  const evidence = resolve(process.env.CODEX2API_TEST_OUTPUT_DIR!, "screenshots");
  await mkdir(evidence, { recursive: true });
  for (const route of ["suppliers", "consumers", "models", "proxies", "usage"]) {
    const response = page.waitForResponse(
      (response) => new URL(response.url()).pathname === `/admin/api/${route}`,
    );
    await page.goto(`/admin/${route}/`);
    await response;
    await expect(page.locator("td[data-label]:visible").first()).toBeVisible();
    await expect
      .poll(() => page.getByRole("table").evaluate((table) => getComputedStyle(table).display))
      .toBe("table");
    const heads = page.getByRole("columnheader");
    expect(await heads.count()).toBeGreaterThanOrEqual(2);
    expect(await heads.count()).toBeLessThanOrEqual(4);
    const heights = await page
      .locator("tbody > tr")
      .evaluateAll((rows) => rows.map((row) => row.getBoundingClientRect().height));
    expect(Math.max(...heights), `${route} mobile row height`).toBeLessThanOrEqual(80);
    await expect
      .poll(() =>
        page
          .locator('[data-slot="table-container"]')
          .evaluateAll((tables) =>
            tables.every((table) => table.scrollWidth <= table.clientWidth + 1),
          ),
      )
      .toBe(true);
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(
      390,
    );
    const row = page.locator("tbody > tr").first();
    const rowTop = (await row.boundingBox())!.y;
    await row.getByRole("button", { name: /^查看详情：/ }).click();
    const detail = page.getByRole("dialog");
    await expect(detail).toBeVisible();
    if (route === "usage") {
      await expect(detail.getByText("缓存读取", { exact: true })).toBeVisible();
      await expect(detail.getByText("供应账户", { exact: true })).toBeVisible();
    } else if (route === "proxies") {
      await expect(detail.getByText("移动端长名称代理".repeat(4), { exact: true })).toBeVisible();
    }
    await page.keyboard.press("Escape");
    await expect(detail).toHaveCount(0);
    expect((await row.boundingBox())!.y).toBeCloseTo(rowTop, 0);
    if (route === "usage") {
      await expect(page.locator("tbody > tr")).toHaveCount(20);
      expect((await page.locator("tbody").boundingBox())!.height).toBeLessThanOrEqual(1600);
    }
    if (route === "suppliers" || route === "usage") {
      await page.getByRole("table").scrollIntoViewIfNeeded();
      await page.screenshot({ path: resolve(evidence, `mobile-${route}.png`) });
    }
    await page.setViewportSize({ width: 320, height: 740 });
    await expect
      .poll(() =>
        page
          .locator('[data-slot="table-container"]')
          .evaluateAll((tables) =>
            tables.every((table) => table.scrollWidth <= table.clientWidth + 1),
          ),
      )
      .toBe(true);
    await page.setViewportSize({ width: 390, height: 844 });
  }
  await page.getByRole("button", { name: "查看请求详情：200", exact: true }).first().click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.keyboard.press("Escape");
  await page.goto("/admin/consumers/detail/?id=review-consumer");
  await page.getByRole("tab", { name: "记录查询", exact: true }).click();
  const logTable = page.getByRole("table").first();
  await expect(logTable.getByRole("columnheader", { name: "接口", exact: true })).toBeVisible();
  await logTable
    .getByRole("button", { name: /^查看详情：/ })
    .first()
    .click();
  await expect(page.getByRole("dialog").getByText("耗时（毫秒）", { exact: true })).toBeVisible();
  await page.keyboard.press("Escape");
  await page.goto("/admin/usage/");
  await page.setViewportSize({ width: 1440, height: 1100 });
  await expect
    .poll(() => page.getByRole("table").evaluate((table) => getComputedStyle(table).display))
    .toBe("table");
  await page.goto("/admin/");
  await expect(page.getByLabel("统计摘要").getByText("4.1M", { exact: true })).toBeVisible();
  await expect(page.locator("[data-sonner-toast]")).toHaveCount(0);
  await page.screenshot({ path: resolve(evidence, "overview-compact.png"), fullPage: true });
  expect(errors).toEqual([]);
});
