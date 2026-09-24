import { test, expect } from "@playwright/test";
import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";

test("usage text, returned models, unit conversion and failure details use persisted records", async ({
  page,
}) => {
  await page.goto("/admin/");
  await page.getByLabel("用户名", { exact: true }).fill("admin");
  await page.getByLabel("密码", { exact: true }).fill("admin");
  await page.getByRole("button", { name: "登录", exact: true }).click();
  await page.getByRole("link", { name: "用量记录", exact: true }).click();
  await expect(page.locator("[data-slot=breadcrumb-page]")).toHaveText("用量记录");
  const filter = page
    .locator("[data-slot=card]")
    .filter({ has: page.getByLabel("供应账户", { exact: true }) });
  const input = await page.getByLabel("供应账户", { exact: true }).boundingBox();
  const query = await filter.getByRole("button", { name: "查询", exact: true }).boundingBox();
  expect(Math.abs(input!.y - query!.y)).toBeLessThan(5);
  await page.getByRole("combobox", { name: "状态", exact: true }).click();
  await expect(page.getByRole("option")).toHaveText(["全部状态", "进行中", "成功", "失败"]);
  await page.getByRole("option", { name: "失败", exact: true }).click();
  const failedQuery = page.waitForResponse((response) => {
    const url = new URL(response.url());
    return url.pathname === "/admin/api/usage" && url.searchParams.get("status") === "failed";
  });
  await filter.getByRole("button", { name: "查询", exact: true }).click();
  const failures = await (await failedQuery).json();
  expect(failures.records.length).toBeGreaterThan(0);
  expect(
    failures.records.every((record: { status: string }) =>
      ["failed", "incomplete", "interrupted"].includes(record.status),
    ),
  ).toBe(true);
  const reset = page.waitForResponse((response) => {
    const url = new URL(response.url());
    return url.pathname === "/admin/api/usage" && !url.searchParams.has("status");
  });
  await filter.getByRole("button", { name: "重置", exact: true }).click();
  await reset;
  const complete = page
    .getByRole("row")
    .filter({ has: page.getByRole("cell", { name: "review-usage-model", exact: true }) });
  await expect(complete.getByText("gpt-6-astra → gpt-5.6-luna", { exact: true })).toHaveClass(
    /text-yellow/,
  );
  await expect(complete.getByText("虚拟账户", { exact: true })).toHaveCount(0);
  await expect(complete.getByText("成功", { exact: true })).toHaveClass(/text-green/);
  const cells = complete.getByRole("cell");
  await expect(
    page.getByRole("columnheader", { name: "推理强度 / 速度", exact: true }),
  ).toBeVisible();
  await expect(cells.nth(2)).toContainText("/v1/responses · http");
  await expect(cells.nth(3)).toHaveText("默认 / 标准");
  await expect(cells.nth(4).locator("svg")).toHaveCount(0);
  for (const value of [
    "输入",
    "输出",
    "思考",
    "缓存读取",
    "缓存写入",
    "缓存率",
    "24.8K",
    "10.8K",
    "2.6K",
    "24.3K",
    "256",
    "97.9%",
  ])
    await expect(cells.nth(4).getByText(value, { exact: !/^[0-9]/.test(value) })).toBeVisible();
  await expect(cells.nth(6)).toContainText("首字节 1.57秒");
  await expect(cells.nth(6)).toContainText("总计 2分25秒");
  expect((await complete.boundingBox())!.height).toBeLessThan(65);
  const evidence = resolve(
    process.env.CODEX2API_TEST_OUTPUT_DIR ?? "../target/review-next-ui-redesign-20260923",
    "screenshots",
  );
  await mkdir(evidence, { recursive: true });
  await page.screenshot({
    path: resolve(evidence, "24-usage-compact.png"),
    fullPage: true,
    animations: "disabled",
  });
  await complete.getByRole("button", { name: "查看请求详情：200", exact: true }).click();
  const detail = page.getByRole("dialog");
  await expect(detail.locator("[data-slot=dialog-description]")).toContainText(
    "请求 ID：req-review-usage-model",
  );
  await expect(detail.getByText("24.8K", { exact: true })).toBeVisible();
  await expect(detail.getByText("2.6K", { exact: true })).toBeVisible();
  await expect(detail.getByText("2分25秒", { exact: true })).toBeVisible();
  await page.keyboard.press("Escape");
  const stopped = page
    .getByRole("row")
    .filter({ has: page.getByRole("cell", { name: "review-usage-client-stop", exact: true }) });
  await expect(stopped.getByText("成功", { exact: true })).toHaveClass(/text-green/);
  await expect(stopped.getByRole("cell").nth(4)).toContainText("24.8K");
  await expect(stopped.getByRole("cell").nth(5)).not.toHaveText("-");
  await stopped.getByRole("button", { name: "查看请求详情：200", exact: true }).click();
  await expect(detail.getByText("成功（客户端停止）", { exact: true })).toBeVisible();
  await expect(detail.getByText("失败原因", { exact: true })).toHaveCount(0);
  await page.keyboard.press("Escape");
  for (const [id, code, reason] of [
    ["review-usage-failure", "429", "本地验收：已达到上游用量限制，请稍后重试。"],
    ["review-usage-stream-failure", "200", "本地验收：输入超出模型上下文限制。"],
  ]) {
    const row = page
      .getByRole("row")
      .filter({ has: page.getByRole("cell", { name: id, exact: true }) });
    await expect(row.getByRole("cell").nth(4)).toHaveText("-");
    await expect(row.getByRole("cell").nth(5)).toHaveText("-");
    await expect(row).not.toContainText(reason);
    await expect(row.getByText("失败", { exact: true })).toHaveClass(/text-red/);
    await row.getByRole("button", { name: `查看请求详情：${code}`, exact: true }).click();
    await expect(detail.getByText(reason, { exact: true })).toBeVisible();
    await expect(detail.locator("[data-slot=dialog-description]")).toContainText(
      `请求 ID：req-${id}`,
    );
    await page.screenshot({
      path: resolve(evidence, `25-usage-failure-${code}.png`),
      fullPage: true,
      animations: "disabled",
    });
    await page.keyboard.press("Escape");
  }
});

test("account filters search their own list endpoints with five results and submit IDs", async ({
  page,
}) => {
  const errors: string[] = [];
  await page.goto("/admin/");
  await expect(page.getByLabel("用户名", { exact: true })).toBeVisible();
  page.on("console", (message) => {
    if (message.type() === "error" || message.type() === "warning") errors.push(message.text());
  });
  page.on("pageerror", (error) => errors.push(error.message));
  await page.getByLabel("用户名", { exact: true }).fill("admin");
  await page.getByLabel("密码", { exact: true }).fill("admin");
  await page.getByRole("button", { name: "登录", exact: true }).click();
  await expect(page.getByRole("link", { name: "用量记录", exact: true })).toBeVisible();
  const session = await (await page.request.get("/admin/api/session")).json();
  const headers = { "x-csrf-token": session.csrf_token };
  const created: { id: string; username: string; email: string }[] = [];
  try {
    const suffix = Date.now().toString(36);
    for (let i = 0; i < 6; i++) {
      const response = await page.request.post("/admin/api/consumers", {
        headers,
        data: {
          username: `search-${suffix}-${i}`,
          email: `mail-${suffix}-${i}@example.test`,
          name: "同名账户",
          password: "search-test-password",
          provider_id: "chatgpt",
          plan_id: "plus",
          enabled: true,
          subscription_expires_at: null,
        },
      });
      expect(response.ok()).toBeTruthy();
      created.push(await response.json());
    }
    await page.getByRole("link", { name: "用量记录", exact: true }).click();
    const consumer = page.getByRole("combobox", { name: "消费账户", exact: true });
    const supplier = page.getByRole("combobox", { name: "供应账户", exact: true });
    const waitList = (path: string, search: string) =>
      page.waitForResponse((response) => {
        const url = new URL(response.url());
        return (
          url.pathname === `/admin/api/${path}` &&
          url.searchParams.get("limit") === "5" &&
          (url.searchParams.get("search") ?? "") === search
        );
      });
    let response = waitList("consumers", "");
    await consumer.click();
    expect((await (await response).json()).items).toHaveLength(5);
    await expect(page.locator("[data-slot=combobox-content] input")).toHaveCount(0);
    await expect(page.getByRole("option")).toHaveCount(5);
    const target = created[5];
    response = waitList("consumers", target.username.toUpperCase());
    await consumer.fill(target.username.toUpperCase());
    expect((await (await response).json()).items.map((item: { id: string }) => item.id)).toEqual([
      target.id,
    ]);
    await expect(page.getByRole("option")).toHaveCount(1);
    response = waitList("consumers", target.email);
    await consumer.fill(target.email);
    await response;
    await page.getByRole("option").click();
    await expect(consumer).toHaveValue(target.username);
    const filtered = page.waitForResponse(
      (response) =>
        new URL(response.url()).pathname === "/admin/api/usage" &&
        new URL(response.url()).searchParams.get("virtual_account") === target.id,
    );
    await page.getByRole("button", { name: "查询", exact: true }).click();
    expect((await (await filtered).json()).total).toBe(0);
    await expect(page.getByRole("table")).toBeVisible();
    response = waitList("consumers", "");
    await consumer.click();
    await response;
    await page.keyboard.press("Escape");
    await expect(consumer).toHaveValue(target.username);
    await page.getByRole("button", { name: "重置", exact: true }).click();
    await expect(consumer).toHaveValue("");
    response = waitList("suppliers", "");
    await supplier.click();
    expect((await (await response).json()).items.length).toBeLessThanOrEqual(5);
    response = waitList("suppliers", "review-supplier-error@example.test");
    await supplier.fill("review-supplier-error@example.test");
    expect((await (await response).json()).items.map((item: { id: string }) => item.id)).toEqual([
      "review-supplier-error",
    ]);
    await page.getByRole("option").click();
    const supplierFiltered = page.waitForResponse(
      (response) =>
        new URL(response.url()).pathname === "/admin/api/usage" &&
        new URL(response.url()).searchParams.get("supplier_id") === "review-supplier-error",
    );
    await page.getByRole("button", { name: "查询", exact: true }).click();
    await supplierFiltered;
    await expect(supplier).toHaveValue("本地验收错误账户");
    await page.getByRole("button", { name: "重置", exact: true }).click();
    await expect(supplier).toHaveValue("");
    expect(errors).toEqual([]);
  } finally {
    for (const account of created)
      await page.request.delete(`/admin/api/consumers/${account.id}`, { headers });
  }
});

test("table pagination supports totals, page input, first and last pages after filtering", async ({
  page,
}) => {
  await page.goto("/admin/");
  await page.getByLabel("用户名", { exact: true }).fill("admin");
  await page.getByLabel("密码", { exact: true }).fill("admin");
  await page.getByRole("button", { name: "登录", exact: true }).click();
  await expect(page.getByRole("link", { name: "用量记录", exact: true })).toBeVisible();
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (["warning", "error"].includes(message.type())) errors.push(message.text());
  });
  for (const [path, filter, term] of [
    ["usage", "模型", "pagination-fixture"],
    ["models", "搜索模型", "pagination-fixture-"],
  ]) {
    await page.goto(`/admin/${path}/`);
    await page.getByLabel(filter, { exact: true }).fill(term);
    await page.getByRole("button", { name: "查询", exact: true }).click();
    const pager = page.getByRole("navigation", { name: "记录分页", exact: true });
    await expect(pager).toContainText("共 105 条 · 6 页");
    await expect(pager.getByRole("combobox", { name: "每页条数", exact: true })).toHaveText(
      "20 条/页",
    );
    for (const size of [10, 30, 50]) {
      await pager.getByRole("combobox", { name: "每页条数", exact: true }).click();
      await expect(page.getByRole("option")).toHaveText([
        "10 条/页",
        "20 条/页",
        "30 条/页",
        "50 条/页",
      ]);
      await page.getByRole("option", { name: `${size} 条/页`, exact: true }).click();
      await expect(pager).toContainText(`共 105 条 · ${Math.ceil(105 / size)} 页`);
      await expect(page.locator("tbody > tr")).toHaveCount(size);
    }
    await expect(page.getByText("共 105 条 · 3 页", { exact: true })).toHaveCount(1);
    const input = pager.getByRole("textbox", { name: "页码", exact: true });
    await expect(input).toHaveValue("1");
    await expect(pager.getByRole("button", { name: "首页", exact: true })).toBeDisabled();
    await pager.getByRole("button", { name: "末页", exact: true }).click();
    await expect(input).toHaveValue("3");
    await expect(page.locator("tbody > tr")).toHaveCount(5);
    await expect(pager.getByRole("button", { name: "末页", exact: true })).toBeDisabled();
    await input.fill("2");
    await input.press("Enter");
    await expect(input).toHaveValue("2");
    await expect(page.locator("tbody > tr")).toHaveCount(50);
    await expect(input).toBeEnabled();
    await input.fill("0");
    await input.press("Enter");
    await expect(page.getByText("请输入 1 到 3 之间的页码", { exact: true })).toBeVisible();
    await expect(input).toHaveValue("2");
    await input.fill("3");
    await input.press("Tab");
    await expect(input).toHaveValue("3");
    await expect(page.locator("tbody > tr")).toHaveCount(5);
    await pager.getByRole("button", { name: "首页", exact: true }).click();
    await expect(input).toHaveValue("1");
    await expect(page.locator("tbody > tr")).toHaveCount(50);
    if (path === "usage") {
      await expect(page.locator("tbody > tr").first().getByRole("cell").nth(3)).toHaveText(
        "xhigh / default",
      );
      expect((await page.locator("tbody > tr").first().boundingBox())!.height).toBeLessThan(65);
    }
    await pager.getByRole("button", { name: "下一页", exact: true }).click();
    await expect(input).toHaveValue("2");
    await expect(input).toBeEnabled();
    await pager.getByRole("button", { name: "上一页", exact: true }).click();
    await expect(input).toHaveValue("1");
    await expect(input).toBeEnabled();
    await pager.getByRole("button", { name: "末页", exact: true }).click();
    await expect(input).toHaveValue("3");
    await expect(input).toBeEnabled();
    await pager.getByRole("combobox", { name: "每页条数", exact: true }).click();
    await page.getByRole("option", { name: "20 条/页", exact: true }).click();
    await expect(input).toHaveValue("1");
    await expect(page.locator("tbody > tr")).toHaveCount(20);
    await page.getByLabel(filter, { exact: true }).fill("no-matching-record");
    await page.getByRole("button", { name: "查询", exact: true }).click();
    await expect(pager).toContainText("共 0 条 · 1 页");
    await expect(input).toHaveValue("1");
    for (const label of ["首页", "上一页", "下一页", "末页"])
      await expect(pager.getByRole("button", { name: label, exact: true })).toBeDisabled();
  }
  for (const path of ["suppliers", "consumers", "plans", "proxies"]) {
    await page.goto(`/admin/${path}/`);
    const pager = page.locator('[data-slot="pagination"]');
    await expect(pager).toContainText(/共 \d+ 条 · \d+ 页/);
    await expect(pager.getByRole("textbox", { name: "页码", exact: true })).toBeVisible();
    await expect(pager.getByRole("button", { name: "首页", exact: true })).toBeDisabled();
  }
  await page.goto("/admin/consumers/detail/?id=review-consumer");
  await page.getByRole("tab", { name: "记录查询", exact: true }).click();
  const logs = page.getByRole("navigation", { name: "记录分页", exact: true });
  await expect(logs).toContainText("共 105 条 · 6 页");
  await logs.getByRole("button", { name: "末页", exact: true }).click();
  await expect(logs.getByRole("textbox", { name: "页码", exact: true })).toHaveValue("6");
  await page.goto("/admin/settings/");
  await page.getByRole("tab", { name: "诊断记录", exact: true }).click();
  const diagnostics = page.getByRole("navigation", { name: "记录分页", exact: true });
  await expect(diagnostics).toContainText("共 106 条 · 6 页");
  await diagnostics.getByRole("button", { name: "末页", exact: true }).click();
  await expect(diagnostics.getByRole("textbox", { name: "页码", exact: true })).toHaveValue("6");
  await expect(page.locator("tbody > tr")).toHaveCount(6);
  expect(errors).toEqual([]);
});
