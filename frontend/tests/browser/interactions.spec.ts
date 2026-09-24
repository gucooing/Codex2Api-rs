import { test, expect, type Page } from "@playwright/test";
import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";

const evidence = resolve(
  process.env.CODEX2API_TEST_OUTPUT_DIR ?? "../target/review-next-ui-redesign-20260923",
  "screenshots",
);
const suffix = Date.now().toString(36);

test("supplier quota stays compact and cached while toolbar actions stay in the filter card", async ({
  page,
}) => {
  await mkdir(evidence, { recursive: true });
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await login(page);
  page.on("console", (message) => {
    if (["error", "warning"].includes(message.type())) errors.push(message.text());
  });
  await page.getByRole("link", { name: "供应账户", exact: true }).click();
  const row = page
    .getByRole("row")
    .filter({ has: page.getByRole("link", { name: "本地验收供应账户", exact: true }) });
  await expect(row.getByText("启用", { exact: true })).toBeVisible();
  const quotas = row.getByLabel("官方额度", { exact: true });
  await expect(quotas.getByText("18%", { exact: true })).toBeVisible();
  await expect(quotas.getByText("47%", { exact: true })).toBeVisible();
  await expect(quotas).not.toContainText("已用");
  await expect(quotas).toContainText("5h：");
  await expect(quotas).toContainText("7天：");
  const monthlyRow = page
    .getByRole("row")
    .filter({ has: page.getByRole("link", { name: "本地验收停用账户", exact: true }) });
  const monthlyQuota = monthlyRow.getByLabel("官方额度", { exact: true });
  await expect(monthlyQuota).toContainText("30天：");
  await expect(monthlyQuota.getByText("0%", { exact: true })).toBeVisible();
  await expect(monthlyQuota.getByRole("progressbar")).toHaveCount(1);
  await expect(monthlyQuota).not.toContainText("5h");
  await expect(monthlyQuota).not.toContainText("7天：");
  await expect(monthlyQuota).not.toContainText("未提供");
  expect((await row.boundingBox())!.height).toBeLessThan(85);
  const filter = page
    .locator("[data-slot=card]")
    .filter({ has: page.getByLabel("搜索账户", { exact: true }) });
  for (const name of ["刷新", "添加供应账户"])
    await expect(filter.getByRole("button", { name, exact: true })).toBeVisible();
  await expect(filter.getByRole("radio", { name: "卡片视图", exact: true })).toBeVisible();
  await page.screenshot({
    path: resolve(evidence, "22-supplier-compact-quotas.png"),
    fullPage: true,
  });
  let quotaReads = 0;
  page.on("request", (r) => {
    if (/\/suppliers\/[^/]+\/quota/.test(new URL(r.url()).pathname)) quotaReads++;
  });
  await filter.getByRole("radio", { name: "卡片视图", exact: true }).click();
  const monthlyCard = page
    .locator("[data-slot=card]")
    .filter({ has: page.getByRole("link", { name: "本地验收停用账户", exact: true }) });
  await expect(monthlyCard.getByLabel("官方额度", { exact: true })).toContainText("30天：");
  await expect(monthlyCard.getByRole("progressbar")).toHaveCount(1);
  await page.setViewportSize({ width: 1920, height: 1000 });
  await page.screenshot({
    path: resolve(evidence, "supplier-dynamic-quota-cards.png"),
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({
    path: resolve(evidence, "supplier-compact-cards-mobile.png"),
    fullPage: true,
  });
  await page.setViewportSize({ width: 1440, height: 1100 });
  await filter.getByRole("radio", { name: "表格视图", exact: true }).click();
  await page.getByLabel("搜索账户", { exact: true }).fill("本地验收供应账户");
  await page.getByRole("button", { name: "查询", exact: true }).click();
  await expect(page.getByRole("row")).toHaveCount(2);
  expect(quotaReads).toBe(0);
  await page.getByRole("button", { name: "重置", exact: true }).click();
  const errorRow = page
    .getByRole("row")
    .filter({ has: page.getByRole("link", { name: "本地验收错误账户", exact: true }) });
  await expect(errorRow.getByText("错误", { exact: true })).toBeVisible();
  await errorRow.getByRole("button", { name: "更多操作", exact: true }).click();
  await expect(page.getByRole("menuitem", { name: "恢复账户", exact: true })).toBeVisible();
  await page.keyboard.press("Escape");
  await monthlyRow.getByRole("link", { name: "本地验收停用账户", exact: true }).click();
  await page.getByRole("tab", { name: "官方额度", exact: true }).click();
  await expect(page.getByRole("heading", { name: "30天", exact: true })).toBeVisible();
  await expect(page.getByRole("progressbar")).toHaveCount(1);
  await expect(page.getByRole("progressbar")).toHaveAttribute("aria-label", "官方额度已用 0%");
  await expect(page.getByText("官方未提供此窗口", { exact: true })).toHaveCount(0);
  await page.screenshot({
    path: resolve(evidence, "supplier-dynamic-quota-detail.png"),
    fullPage: true,
  });
  expect(errors).toEqual([]);
});

test("backend failures preserve the admin session and unsaved form until a real 401", async ({
  page,
  context,
}) => {
  await login(page);
  await page.goto("/admin/settings/");
  const rules = page.getByLabel("UA 规则", { exact: true });
  await expect(rules).toBeEnabled();
  const draft = "codex*\nkeep-unsaved-rule";
  await rules.fill(draft);
  const cookie = (await context.cookies()).find((item) => item.name === "c2a_admin_session")!;
  const session = await (await context.request.get("/admin/api/session")).json();
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  for (const status of [0, 500, 502, 503, 504, 403, 401]) {
    await page.route("**/admin/api/settings/gateway", async (route) => {
      if (status === 0) return route.abort("connectionrefused");
      await route.fulfill({
        status,
        contentType: "application/json",
        body: JSON.stringify({
          error: {
            code: status === 401 ? "upstream_unauthorized" : "backend_unavailable",
            message: `本地验收：后端错误 ${status}`,
          },
        }),
      });
    });
    await page.getByRole("button", { name: "保存", exact: true }).click();
    await expect(
      page.getByText(
        status === 0 ? "无法连接后端服务，请检查网络后重试。" : `本地验收：后端错误 ${status}`,
        { exact: true },
      ),
    ).toBeVisible();
    await expect(rules).toHaveValue(draft);
    await expect(page.getByRole("heading", { name: "管理登录", exact: true })).toHaveCount(0);
    expect((await context.cookies()).find((item) => item.name === cookie.name)?.value).toBe(
      cookie.value,
    );
    await page.unroute("**/admin/api/settings/gateway");
  }
  await expect(page.getByRole("button", { name: "保存", exact: true })).toBeEnabled();
  const saved = page.waitForResponse(
    (response) =>
      response.url().endsWith("/admin/api/settings/gateway") &&
      response.request().method() === "PUT",
  );
  await page.getByRole("button", { name: "保存", exact: true }).click();
  expect((await saved).status()).toBe(200);
  const logout = await context.request.post("/admin/api/logout", {
    headers: { "X-CSRF-Token": session.csrf_token },
  });
  expect(logout.status()).toBe(200);
  const expired = page.waitForResponse(
    (response) =>
      response.url().endsWith("/admin/api/settings/gateway") && response.status() === 401,
  );
  await page.getByRole("button", { name: "保存", exact: true }).click();
  await expired;
  await expect(page.getByRole("heading", { name: "管理登录", exact: true })).toBeVisible();
  expect(errors).toEqual([]);
});

test("failed initial session checks offer reconnect and recover without logging in again", async ({
  page,
  context,
}) => {
  await login(page);
  await page.goto("/admin/settings/");
  await expect(page.getByLabel("UA 规则", { exact: true })).toBeEnabled();
  const cookie = (await context.cookies()).find((item) => item.name === "c2a_admin_session")!;
  const url = page.url();
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  for (const status of [0, 503]) {
    await page.route("**/admin/api/session", async (route) => {
      if (status === 0) return route.abort("connectionrefused");
      await route.fulfill({
        status,
        contentType: "application/json",
        body: JSON.stringify({
          error: { code: "session_unavailable", message: "本地验收：会话服务不可用" },
        }),
      });
    });
    await page.reload();
    await expect(page.getByRole("button", { name: "重新连接", exact: true })).toBeVisible();
    await expect(page.getByRole("heading", { name: "管理登录", exact: true })).toHaveCount(0);
    expect(page.url()).toBe(url);
    expect((await context.cookies()).find((item) => item.name === cookie.name)?.value).toBe(
      cookie.value,
    );
    await page.unroute("**/admin/api/session");
    await page.getByRole("button", { name: "重新连接", exact: true }).click();
    await expect(page.getByLabel("UA 规则", { exact: true })).toBeEnabled();
    await expect(page.getByRole("button", { name: "重新连接", exact: true })).toHaveCount(0);
  }
  expect(errors).toEqual([]);
});

test("failed reads keep all pages visible and settings disabled until retry succeeds", async ({
  page,
}) => {
  await mkdir(evidence, { recursive: true });
  await login(page);
  // Failure-only injection into this isolated browser; all successful reads/writes use the real service.
  await page.route("**/admin/api/**", async (route) => {
    const path = new URL(route.request().url()).pathname;
    if (route.request().method() === "GET" && !path.endsWith("/session"))
      await route.fulfill({
        status: 500,
        contentType: "application/json",
        body: JSON.stringify({ error: { message: "本地验收：读取失败" } }),
      });
    else await route.continue();
  });
  for (const [label, expected] of [
    ["供应账户", "搜索账户"],
    ["虚拟账户", "搜索账户"],
    ["套餐管理", "搜索套餐"],
    ["模型配置", "搜索模型"],
    ["出站代理", "搜索代理"],
    ["用量记录", "模型"],
  ]) {
    await page.getByRole("link", { name: label, exact: true }).click();
    await expect(page.locator("[data-slot=breadcrumb-page]")).toHaveText(label);
    await expect(page.getByLabel(expected, { exact: true })).toBeVisible();
    await expect(page.getByRole("table")).toBeVisible();
    expect(await page.getByRole("columnheader").count()).toBeGreaterThan(0);
    await expect(
      page.getByRole("main").locator("[data-slot=empty-description]").first(),
    ).toBeVisible();
  }
  await page.getByRole("link", { name: "系统设置", exact: true }).click();
  await expect(page.getByLabel("UA 规则", { exact: true })).toBeVisible();
  await expect(page.getByLabel("UA 规则", { exact: true })).toBeDisabled();
  await expect(page.getByRole("button", { name: "保存", exact: true })).toBeDisabled();
  await page.screenshot({ path: resolve(evidence, "23-settings-failed-read.png"), fullPage: true });
  await page.getByRole("tab", { name: "管理员凭据", exact: true }).click();
  await expect(page.getByLabel("当前用户名", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "保存", exact: true })).toBeDisabled();
  await page.getByRole("tab", { name: "Desktop 支持", exact: true }).click();
  await expect(page.getByLabel("公开资源缓存时间（分钟）", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "保存", exact: true })).toBeDisabled();
  for (const name of ["诊断记录", "公开资源", "端点诊断"]) {
    await page.getByRole("tab", { name, exact: true }).click();
    await expect(page.getByRole("table")).toBeVisible();
  }
  await page.getByRole("link", { name: "概览", exact: true }).click();
  await expect(page.getByRole("button", { name: "重新加载", exact: true })).toBeVisible();
  await expect(page.locator("[data-slot=card-title]").filter({ hasText: "—" })).toHaveCount(4);
  for (const kind of ["suppliers", "consumers"]) {
    await page.goto(`/admin/${kind}/detail/?id=missing-review-record`);
    await expect(page.getByRole("tablist")).toBeVisible();
    if (kind === "consumers") {
      await expect(page.getByLabel("账户名称", { exact: true })).toBeVisible();
      await expect(page.getByLabel("账户名称", { exact: true })).toBeDisabled();
    } else {
      await expect(page.getByRole("heading", { name: "供应账户资料", exact: true })).toBeVisible();
      await page.getByRole("tab", { name: "指纹与网络", exact: true }).click();
      await expect(page.getByLabel("操作系统", { exact: true })).toBeVisible();
      await expect(page.getByLabel("操作系统", { exact: true })).toBeDisabled();
    }
  }
  await page.getByRole("link", { name: "系统设置", exact: true }).click();
  await page.getByRole("tab", { name: "网关 UA", exact: true }).click();
  await page.unroute("**/admin/api/**");
  await page.getByRole("button", { name: "重新加载", exact: true }).click();
  await expect(page.getByLabel("UA 规则", { exact: true })).toBeEnabled();
  await expect(page.getByRole("button", { name: "保存", exact: true })).toBeEnabled();
  await page.goto("/admin/authorize/");
  await expect(page.getByLabel("虚拟账户用户名", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "登录并授权", exact: true })).toBeDisabled();
  await expect(page.getByRole("button", { name: "重新加载", exact: true })).toBeVisible();
});

test("page controls exist before the data arrives and are populated without rebuilding the form", async ({
  page,
}) => {
  await login(page);
  let release: () => void = () => {};
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route("**/admin/api/settings/gateway", async (route) => {
    await gate;
    await route.continue();
  });
  try {
    await page.getByRole("link", { name: "系统设置", exact: true }).click();
    const rules = page.getByLabel("UA 规则", { exact: true });
    await expect(rules).toBeVisible();
    await expect(rules).toBeDisabled();
    await rules.evaluate((element) => element.setAttribute("data-same-control", "yes"));
    release();
    await expect(rules).toBeEnabled();
    await expect(rules).toHaveAttribute("data-same-control", "yes");
  } finally {
    release();
    await page.unroute("**/admin/api/settings/gateway");
  }
});

async function login(page: Page) {
  await page.goto("/admin/");
  await page.getByLabel("用户名", { exact: true }).fill("admin");
  await page.getByLabel("密码", { exact: true }).fill("admin");
  await page.getByRole("button", { name: "登录", exact: true }).click();
  await expect(
    page.locator("[data-slot=breadcrumb-page]").filter({ hasText: "概览" }),
  ).toBeVisible();
}

async function noPageOverflow(page: Page) {
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(overflow).toBeLessThanOrEqual(1);
}

test("account forms, filters, view controls and destructive confirmation use real writes", async ({
  page,
  context,
}) => {
  await mkdir(evidence, { recursive: true });
  await login(page);
  await page.getByRole("link", { name: "虚拟账户", exact: true }).click();
  await page.getByRole("button", { name: "创建虚拟账户", exact: true }).click();
  const dialog = page.getByRole("dialog");
  const accountName = `交互验收 ${suffix}`;
  await dialog.getByLabel("账户名称", { exact: true }).fill(accountName);
  await dialog.getByLabel("登录用户名", { exact: true }).fill("review-consumer");
  await dialog.getByLabel("登录密码", { exact: true }).fill("test-only-password");
  await dialog.getByLabel("邮箱", { exact: true }).fill(`interaction-${suffix}@example.test`);
  const plans = await (await context.request.get("/admin/api/plans")).json();
  const plan = plans.items.find((item: { enabled: boolean }) => item.enabled);
  await dialog.getByLabel("订阅套餐", { exact: true }).click();
  await page.getByRole("option", { name: plan.name, exact: true }).click();
  const loginSwitch = dialog.getByRole("switch", { name: "允许账户登录", exact: true });
  await expect(loginSwitch).toBeChecked();
  await loginSwitch.click();
  await expect(loginSwitch).not.toBeChecked();
  const beforeError = await dialog.locator("form").boundingBox();
  let releaseMutation: () => void = () => {};
  const mutationGate = new Promise<void>((resolve) => {
    releaseMutation = resolve;
  });
  await page.route("**/admin/api/consumers", async (route) => {
    if (route.request().method() === "POST") await mutationGate;
    await route.continue();
  });
  const pendingCheck = page
    .waitForRequest(
      (request) =>
        request.method() === "POST" && new URL(request.url()).pathname === "/admin/api/consumers",
    )
    .then(async () => {
      await page.keyboard.press("Escape");
      await expect(dialog).toBeVisible();
      await expect(dialog.getByRole("button", { name: "关闭", exact: true })).toBeDisabled();
    });
  await dialog.getByRole("button", { name: "创建账户", exact: true }).click();
  try {
    await pendingCheck;
  } finally {
    releaseMutation();
  }
  const errorToast = page.locator('[data-sonner-toast][data-type="error"]').last();
  await expect(errorToast).toBeVisible();
  await expect(dialog.getByRole("alert")).toHaveCount(0);
  const afterError = await dialog.locator("form").boundingBox();
  expect(Math.abs(afterError!.height - beforeError!.height)).toBeLessThanOrEqual(1);
  expect(Math.abs(afterError!.y - beforeError!.y)).toBeLessThanOrEqual(1);
  expect(
    await page.evaluate(() =>
      Boolean(document.activeElement?.closest("[data-sonner-toast], .form-error")),
    ),
  ).toBe(false);
  await expect(dialog.getByLabel("账户名称", { exact: true })).toHaveValue(accountName);
  await dialog.getByLabel("登录用户名", { exact: true }).fill(`interaction-${suffix}`);
  let creates = 0;
  page.on("request", (request) => {
    if (request.method() === "POST" && new URL(request.url()).pathname === "/admin/api/consumers")
      creates++;
  });
  await dialog.getByRole("button", { name: "创建账户", exact: true }).click({ clickCount: 2 });
  await expect(dialog).not.toBeVisible();
  await expect(page.getByRole("link", { name: accountName, exact: true })).toBeVisible();
  expect(creates).toBe(1);
  const consumers = await (await context.request.get("/admin/api/consumers")).json();
  const account = consumers.items.find((item: { name: string }) => item.name === accountName);
  expect(account.enabled).toBe(false);
  await expect(errorToast).not.toBeVisible();

  await page.screenshot({ path: resolve(evidence, "09-accounts-table-1440.png"), fullPage: true });
  await page.getByRole("radio", { name: "卡片视图", exact: true }).click();
  await expect(page.getByRole("table")).toHaveCount(0);
  await page.screenshot({ path: resolve(evidence, "10-accounts-cards-1440.png"), fullPage: true });
  await page.getByRole("radio", { name: "表格视图", exact: true }).click();
  await page.getByLabel("搜索账户", { exact: true }).fill(accountName);
  await page.getByRole("button", { name: "查询", exact: true }).click();
  await expect(page.getByRole("row")).toHaveCount(2);
  await expect(page.getByRole("link", { name: "本地验收账户", exact: true })).toHaveCount(0);
  await page.getByRole("button", { name: "重置", exact: true }).click();
  await expect(page.getByLabel("搜索账户", { exact: true })).toHaveValue("");
  await expect(page.getByRole("link", { name: "本地验收账户", exact: true })).toBeVisible();
  await page.getByLabel("搜索账户", { exact: true }).fill(accountName);
  await page.getByRole("button", { name: "查询", exact: true }).click();
  const row = page.getByRole("row").filter({ has: page.getByText(accountName, { exact: true }) });
  const menuTrigger = row.getByRole("button", { name: "更多操作", exact: true });
  await menuTrigger.click();
  await expect(page.getByRole("menu")).toBeVisible();
  const heading = await page
    .locator("[data-slot=breadcrumb-page]")
    .filter({ hasText: "虚拟账户" })
    .boundingBox();
  await page.mouse.click(heading!.x + 4, heading!.y + 4);
  await expect(page.getByRole("menu")).not.toBeVisible();
  await menuTrigger.focus();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("menuitem", { name: "删除账户", exact: true })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("menu")).not.toBeVisible();
  await expect(menuTrigger).toBeFocused();

  let deletes = 0;
  page.on("request", (request) => {
    if (request.method() === "DELETE" && new URL(request.url()).pathname.endsWith(account.id))
      deletes++;
  });
  await menuTrigger.click();
  await page.getByRole("menuitem", { name: "删除账户", exact: true }).click();
  const confirmation = page.getByRole("alertdialog");
  await expect(confirmation).toContainText(accountName);
  await confirmation.getByRole("button", { name: "取消", exact: true }).click();
  await expect(confirmation).not.toBeVisible();
  expect(deletes).toBe(0);
  expect((await context.request.get(`/admin/api/consumers/${account.id}`)).ok()).toBe(true);
  await menuTrigger.click();
  await page.getByRole("menuitem", { name: "删除账户", exact: true }).click();
  await confirmation.getByRole("button", { name: /^确认/ }).click({ clickCount: 2 });
  await expect(confirmation).not.toBeVisible();
  await expect(page.getByRole("link", { name: accountName, exact: true })).toHaveCount(0);
  expect(deletes).toBe(1);
  expect((await context.request.get(`/admin/api/consumers/${account.id}`)).status()).toBe(404);
});

test("usage filters are applied to actual server reads and reset cleanly", async ({ page }) => {
  await mkdir(evidence, { recursive: true });
  await login(page);
  await page.getByRole("link", { name: "用量记录", exact: true }).click();
  await page.getByLabel("模型", { exact: true }).fill("ui-regression-no-match");
  const filtered = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === "/admin/api/usage" &&
      new URL(response.url()).searchParams.get("model") === "ui-regression-no-match",
  );
  await page.getByRole("button", { name: "查询", exact: true }).click();
  expect((await filtered).ok()).toBe(true);
  await expect(page.getByText("暂无符合条件的用量记录", { exact: true })).toBeVisible();
  await noPageOverflow(page);
  await page.screenshot({ path: resolve(evidence, "11-usage-filters-1440.png"), fullPage: true });
  const reset = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === "/admin/api/usage" &&
      !new URL(response.url()).searchParams.has("model"),
  );
  await page.getByRole("button", { name: "重置", exact: true }).click();
  expect((await reset).ok()).toBe(true);
  await expect(page.getByLabel("模型", { exact: true })).toHaveValue("");
  await page.goto("/admin/consumers/detail/?id=review-consumer");
  await page.getByRole("tab", { name: "记录查询", exact: true }).click();
  await expect(page.getByRole("tabpanel", { name: "记录查询", exact: true })).toBeVisible();
  await noPageOverflow(page);
  await page.screenshot({ path: resolve(evidence, "12-consumer-log-filters.png"), fullPage: true });
});

test("desktop navigation, mobile drawer and long forms respect viewport and reduced motion", async ({
  page,
}) => {
  await mkdir(evidence, { recursive: true });
  await login(page);
  const toggle = page.locator("[data-slot=sidebar-trigger]");
  await expect(toggle).toHaveAccessibleName(/导航|侧栏|Sidebar/);
  const expanded = await page.getByRole("main").boundingBox();
  await toggle.click();
  await expect
    .poll(async () => (await page.getByRole("main").boundingBox())!.x)
    .toBeLessThan(expanded!.x - 50);
  await toggle.click();
  await page.keyboard.press("Control+k");
  const search = page.getByRole("dialog");
  await search.getByRole("combobox").fill("模型");
  await search.getByRole("option", { name: /模型配置/ }).click();
  await expect(
    page.locator("[data-slot=breadcrumb-page]").filter({ hasText: "模型配置" }),
  ).toBeVisible();
  await expect(search).not.toBeVisible();

  await page.setViewportSize({ width: 1920, height: 1080 });
  await page.getByRole("link", { name: "虚拟账户", exact: true }).click();
  await expect(page.getByRole("link", { name: "本地验收账户", exact: true })).toBeVisible();
  await noPageOverflow(page);
  await page.screenshot({ path: resolve(evidence, "13-accounts-table-1920.png"), fullPage: true });
  await page.getByRole("radio", { name: "卡片视图", exact: true }).click();
  await noPageOverflow(page);
  await page.screenshot({ path: resolve(evidence, "14-accounts-cards-1920.png"), fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  await noPageOverflow(page);
  await page.screenshot({ path: resolve(evidence, "15-accounts-mobile.png"), fullPage: true });
  await toggle.click();
  const drawer = page.getByRole("dialog");
  await expect(drawer.getByRole("link", { name: "套餐管理", exact: true })).toBeVisible();
  await page.screenshot({ path: resolve(evidence, "16-navigation-mobile.png"), fullPage: true });
  await drawer.getByRole("link", { name: "套餐管理", exact: true }).click();
  await expect(drawer).not.toBeVisible();
  await expect(
    page.locator("[data-slot=breadcrumb-page]").filter({ hasText: "套餐管理" }),
  ).toBeVisible();
  await noPageOverflow(page);
  await page.emulateMedia({ reducedMotion: "reduce" });
  const add = page.getByRole("button", { name: "添加套餐", exact: true });
  await add.click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByRole("button", { name: "保存", exact: true })).toBeInViewport();
  await expect(dialog.getByRole("button", { name: "关闭", exact: true })).toBeInViewport();
  const duration = await dialog.evaluate((element) => {
    const style = getComputedStyle(element);
    return Math.max(...style.animationDuration.split(",").map((value) => parseFloat(value)));
  });
  expect(duration).toBeLessThanOrEqual(0.01);
  const bounds = await dialog.boundingBox();
  expect(bounds!.x).toBeGreaterThanOrEqual(0);
  expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(391);
  expect(bounds!.y).toBeGreaterThanOrEqual(0);
  expect(bounds!.y + bounds!.height).toBeLessThanOrEqual(845);
  const body = dialog.locator("form > [data-slot=scroll-area] [data-slot=scroll-area-viewport]");
  await body.evaluate((element) => {
    element.scrollTop = element.scrollHeight;
  });
  const header = await dialog.locator("[data-slot=dialog-header]").boundingBox();
  const scroll = await body.boundingBox();
  const footer = await dialog.locator("form > [data-slot=field-group]").boundingBox();
  expect(header!.y + header!.height).toBeLessThanOrEqual(scroll!.y + 1);
  expect(scroll!.y + scroll!.height).toBeLessThanOrEqual(footer!.y + 1);
  await page.screenshot({ path: resolve(evidence, "17-plan-form-mobile.png"), fullPage: true });
  await dialog.getByRole("button", { name: "关闭", exact: true }).click();
  await expect(dialog).not.toBeVisible();
  await expect(add).toBeFocused();
});

test("every query and reset submits a fresh read even with unchanged conditions", async ({
  page,
}) => {
  await login(page);
  for (const resource of ["usage", "suppliers", "consumers", "plans", "models", "proxies"]) {
    const matches = (response: import("@playwright/test").Response) =>
      new URL(response.url()).pathname === `/admin/api/${resource}` &&
      response.request().method() === "GET";
    await Promise.all([page.waitForResponse(matches), page.goto(`/admin/${resource}/`)]);
    for (const action of ["查询", "查询", "重置", "重置"]) {
      const refreshed = page.waitForResponse(matches);
      await page.getByRole("button", { name: action, exact: true }).click();
      expect((await refreshed).ok()).toBe(true);
    }
  }
  await page.goto("/admin/consumers/detail/?id=review-consumer");
  const logs = (response: import("@playwright/test").Response) =>
    new URL(response.url()).pathname === "/admin/api/consumers/review-consumer/logs";
  await Promise.all([
    page.waitForResponse(logs),
    page.getByRole("tab", { name: "记录查询", exact: true }).click(),
  ]);
  for (const action of ["查询", "查询", "重置", "重置"]) {
    const refreshed = page.waitForResponse(logs);
    await page.getByRole("button", { name: action, exact: true }).click();
    expect((await refreshed).ok()).toBe(true);
  }
});

test("supplier authorization keeps fingerprint, method and authorization in three steps", async ({
  page,
}) => {
  await login(page);
  await page.getByRole("link", { name: "供应账户", exact: true }).click();
  const before = await (await page.request.get("/admin/api/suppliers")).json();
  const starts: Record<string, unknown>[] = [];
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("request", (request) => {
    if (new URL(request.url()).pathname === "/admin/api/suppliers/oauth/start")
      starts.push(request.postDataJSON());
  });
  await page.getByRole("button", { name: "添加供应账户", exact: true }).click();
  const dialog = page.getByRole("dialog");
  const steps = dialog.getByRole("tablist", { name: "添加账户步骤", exact: true });
  await expect(steps.getByRole("tab")).toHaveCount(3);
  await expect(steps.getByRole("tab", { name: "1 指纹配置", exact: true })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(steps.getByRole("tab", { name: "2 授权方式", exact: true })).toBeDisabled();
  const os = dialog.getByLabel("操作系统", { exact: true });
  await expect(os).toBeEnabled();
  const osValue = await os.inputValue();
  await os.fill("");
  await dialog.getByRole("button", { name: "下一步", exact: true }).click();
  await expect(page.getByText("请填写操作系统", { exact: true })).toBeVisible();
  await expect(os).toBeVisible();
  await os.fill(osValue);
  await dialog.getByLabel("终端标识", { exact: true }).fill("review-terminal");
  await dialog.getByRole("button", { name: "下一步", exact: true }).click();
  await expect(dialog.getByRole("radiogroup", { name: "授权方式", exact: true })).toBeVisible();
  await expect(dialog.getByRole("radio")).toHaveCount(3);
  await expect(dialog.getByRole("radio", { name: "回调链接", exact: true })).toBeChecked();
  const output = resolve(
    process.env.CODEX2API_TEST_OUTPUT_DIR ?? "../target/review-next-ui-redesign-20260923",
    "screenshots",
  );
  await mkdir(output, { recursive: true });
  await page.screenshot({ path: resolve(output, "supplier-oauth-step-2.png"), fullPage: true });
  await dialog.getByRole("radio", { name: "RT 授权", exact: true }).check();
  await dialog.getByRole("button", { name: "下一步", exact: true }).click();
  await expect(dialog.getByLabel("Refresh Token", { exact: true })).toBeVisible();
  expect(starts).toHaveLength(0);
  await dialog.getByRole("button", { name: "开始授权", exact: true }).click();
  expect(starts).toHaveLength(0);
  await expect(page.getByText("请填写Refresh Token", { exact: true })).toBeVisible();
  await dialog.getByRole("button", { name: "上一步", exact: true }).click();
  await expect(dialog.getByRole("radio", { name: "RT 授权", exact: true })).toBeChecked();
  await dialog.getByRole("button", { name: "上一步", exact: true }).click();
  await expect(dialog.getByLabel("终端标识", { exact: true })).toHaveValue("review-terminal");
  await dialog.getByRole("button", { name: "下一步", exact: true }).click();
  await dialog.getByRole("radio", { name: "回调链接", exact: true }).check();
  const started = page.waitForResponse(
    (response) => new URL(response.url()).pathname === "/admin/api/suppliers/oauth/start",
  );
  await dialog.getByRole("button", { name: "下一步", exact: true }).click();
  const pending = await (await started).json();
  expect(pending.status).toBe("pending");
  expect(starts).toHaveLength(1);
  expect(starts[0]).toMatchObject({
    method: "callback",
    fingerprint: { terminal: "review-terminal" },
  });
  await expect(dialog.getByRole("tab", { name: "3 开始授权", exact: true })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(dialog.getByRole("link", { name: "打开官方授权页面", exact: true })).toHaveAttribute(
    "href",
    pending.authorize_url,
  );
  await dialog.getByLabel("授权后的完整回调地址", { exact: true }).fill("invalid-callback");
  const rejected = page.waitForResponse(
    (response) => new URL(response.url()).pathname === "/admin/api/suppliers/oauth/callback",
  );
  await dialog.getByRole("button", { name: "提交回调", exact: true }).click();
  expect((await rejected).status()).toBe(400);
  await expect(dialog.getByLabel("授权后的完整回调地址", { exact: true })).toHaveValue(
    "invalid-callback",
  );
  const canceled = page.waitForResponse(
    (response) => new URL(response.url()).pathname === "/admin/api/suppliers/oauth/cancel",
  );
  await dialog.getByRole("button", { name: "上一步", exact: true }).click();
  expect((await canceled).request().postDataJSON().state).toBe(pending.state);
  await expect(dialog.getByRole("radio", { name: "回调链接", exact: true })).toBeChecked();
  // Failure-only injection avoids contacting the official device or refresh-token service.
  await page.route("**/admin/api/suppliers/oauth/start", (route) =>
    route.fulfill({
      status: 503,
      contentType: "application/json",
      body: JSON.stringify({
        error: { code: "upstream_error", message: "本地验收：授权服务不可用" },
      }),
    }),
  );
  await dialog.getByRole("radio", { name: "设备码", exact: true }).check();
  await dialog.getByRole("button", { name: "下一步", exact: true }).click();
  await expect(page.getByText("本地验收：授权服务不可用", { exact: true })).toBeVisible();
  await expect(dialog.getByRole("button", { name: "开始授权", exact: true })).toBeEnabled();
  await dialog.getByRole("button", { name: "上一步", exact: true }).click();
  await expect(dialog.getByRole("radio", { name: "设备码", exact: true })).toBeChecked();
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({
    path: resolve(output, "supplier-oauth-step-2-mobile.png"),
    fullPage: true,
  });
  await expect(dialog.getByRole("button", { name: "下一步", exact: true })).toBeInViewport();
  await dialog.getByRole("button", { name: "关闭", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  const after = await (await page.request.get("/admin/api/suppliers")).json();
  expect(after.items.length).toBe(before.items.length);
  expect(errors).toEqual([]);
});

test("virtual account settings stay compact, grouped and editable without losing client ownership", async ({
  page,
}) => {
  await mkdir(evidence, { recursive: true });
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await login(page);
  page.on("console", (message) => {
    if (["error", "warning"].includes(message.type()) && !message.text().includes("500"))
      errors.push(message.text());
  });
  await page.goto("/admin/consumers/detail/?id=review-consumer");
  await expect(page.getByLabel("账户名称", { exact: true })).toHaveValue("本地验收账户");
  const tabs = page.getByRole("tablist", { name: "虚拟账户详情" });
  await expect(tabs.getByRole("tab")).toHaveText(["账户设置", "用量统计", "记录查询", "登录设备"]);
  const firstTab = await tabs.getByRole("tab", { name: "账户设置" }).boundingBox();
  const refresh = await page.getByRole("button", { name: "刷新", exact: true }).boundingBox();
  expect(
    Math.abs(firstTab!.y + firstTab!.height / 2 - refresh!.y - refresh!.height / 2),
  ).toBeLessThan(8);
  const accountCard = page
    .locator('[data-slot="card"]')
    .filter({ has: page.getByRole("heading", { name: "账户与订阅", exact: true }) });
  const routingCard = page
    .locator('[data-slot="card"]')
    .filter({ has: page.getByRole("heading", { name: "供应绑定", exact: true }) });
  const accountBox = await accountCard.boundingBox();
  const routingBox = await routingCard.boundingBox();
  expect(Math.abs(accountBox!.y - routingBox!.y)).toBeLessThan(2);
  expect(routingBox!.x).toBeGreaterThan(accountBox!.x + accountBox!.width);
  await expect(page.getByRole("button", { name: "保存账户与订阅", exact: true })).toBeInViewport();
  await noPageOverflow(page);
  await page.screenshot({
    path: resolve(evidence, "consumer-settings-desktop.png"),
    fullPage: true,
    animations: "disabled",
  });

  async function select(label: string, value: string) {
    await page.getByLabel(label, { exact: true }).click();
    await page.getByRole("option", { name: value, exact: true }).click();
    await expect(page.getByRole("listbox")).toHaveCount(0);
  }
  for (const group of ["个人资料", "功能开关", "会话与附件", "提示与公告", "优惠与定价"]) {
    await select("设置分类", group);
    await expect(page.getByRole("heading", { name: group, exact: true })).toBeVisible();
    await noPageOverflow(page);
  }
  await select("设置分类", "个人资料");
  const bio = page.getByLabel("个人简介", { exact: true });
  const saveProfile = page.getByRole("button", { name: "保存设置", exact: true });
  await expect(bio).toBeEnabled();
  const original = await bio.inputValue();
  const draft = `布局验收 ${suffix}`;
  await bio.fill(draft);
  // A failed refresh must preserve the same input and its unsaved draft, while blocking writes.
  await page.route("**/admin/api/consumers/review-consumer/configs", (route) =>
    route.fulfill({
      status: 500,
      contentType: "application/json",
      body: JSON.stringify({ error: { code: "test_failure", message: "布局验收读取失败" } }),
    }),
  );
  await page.getByRole("button", { name: "刷新", exact: true }).click();
  await expect(saveProfile).toBeDisabled();
  await expect(bio).toHaveValue(draft);
  await page.unroute("**/admin/api/consumers/review-consumer/configs");
  await page.getByRole("button", { name: "刷新", exact: true }).click();
  await expect(saveProfile).toBeEnabled();
  await expect(bio).toHaveValue(draft);
  const saved = page.waitForResponse(
    (response) =>
      response.request().method() === "PUT" &&
      new URL(response.url()).pathname === "/admin/api/consumers/review-consumer/config/profile",
  );
  await saveProfile.click();
  expect((await saved).ok()).toBe(true);
  await expect(bio).toBeEnabled();
  await expect(saveProfile).toBeDisabled();
  const configs = await (
    await page.request.get("/admin/api/consumers/review-consumer/configs")
  ).json();
  expect(configs.items.find((item: { key: string }) => item.key === "profile").value.bio).toBe(
    draft,
  );
  await bio.fill(original);
  await saveProfile.click();
  await expect(bio).toBeEnabled();
  await expect(saveProfile).toBeDisabled();
  await select("设置分类", "功能开关");
  await expect(page.locator("[data-sonner-toast]")).toHaveCount(0, { timeout: 10000 });
  await page.screenshot({
    path: resolve(evidence, "consumer-features-desktop.png"),
    fullPage: true,
    animations: "disabled",
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await noPageOverflow(page);
  await page.screenshot({
    path: resolve(evidence, "consumer-features-mobile.png"),
    fullPage: true,
    animations: "disabled",
  });
  await select("设置分类", "账户与订阅");
  await expect(page.getByRole("button", { name: "保存账户与订阅", exact: true })).toBeEnabled();
  await noPageOverflow(page);
  await page.screenshot({
    path: resolve(evidence, "consumer-settings-mobile.png"),
    fullPage: true,
    animations: "disabled",
  });
  await page.getByRole("tab", { name: "记录查询", exact: true }).click();
  await select("记录类别", "订阅操作记录");
  await expect(page.getByRole("heading", { name: "订阅操作记录", exact: true })).toBeVisible();
  await select("记录类别", "客户端偏好与安装状态");
  await select("状态类别", "浏览器设置");
  await expect(page.getByRole("heading", { name: "审批偏好", exact: true })).toBeVisible();
  await expect(page.getByRole("main").getByRole("button", { name: /^保存/ })).toHaveCount(0);
  await noPageOverflow(page);
  expect(errors).toEqual([]);
});

test("feature switches save only changes, retain defaults and retry only failed updates", async ({
  page,
}) => {
  await login(page);
  await page.goto("/admin/consumers/detail/?id=review-consumer");
  await page.getByLabel("设置分类", { exact: true }).click();
  await page.getByRole("option", { name: "功能开关", exact: true }).click();
  const read = async () =>
    (await (await page.request.get("/admin/api/consumers/review-consumer/configs")).json())
      .items as {
      key: string;
      revision: number;
      value: {
        enabled?: boolean;
        finances?: boolean;
        health_eligibility?: unknown;
        feature_gates?: Record<string, { value?: boolean }>;
      };
    }[];
  const before = await read();
  const old = (key: string) => before.find((item) => item.key === key)!;
  const trust = page.getByRole("switch", { name: "提供信任联系人功能", exact: true });
  const sites = page.getByRole("switch", { name: "开放站点功能", exact: true });
  const finances = page.getByRole("switch", { name: "显示财务入口", exact: true });
  const save = page.getByRole("button", { name: "保存设置", exact: true });
  await expect(trust).toBeEnabled();
  await expect(save).toBeDisabled();
  await expect(
    page.getByRole("button", { name: "开放站点功能：恢复默认", exact: true }),
  ).toHaveCount(0);
  await expect(page.getByRole("tabpanel").locator('[data-slot="card"]')).toHaveCount(1);
  await expect(page.getByRole("tabpanel").getByRole("combobox")).toHaveCount(1);
  await trust.setChecked(!old("trusted_contact").value.enabled);
  await sites.setChecked(!old("sites").value.enabled);
  await finances.setChecked(!old("first_party").value.finances);
  const writes: string[] = [];
  page.on("request", (request) => {
    const path = new URL(request.url()).pathname;
    if (request.method() === "PUT" && path.includes("/config/"))
      writes.push(path.split("/").pop()!);
  });
  await page.route("**/admin/api/consumers/review-consumer/config/sites", (route) =>
    route.fulfill({
      status: 500,
      contentType: "application/json",
      body: JSON.stringify({ error: { code: "test_failure", message: "本地验收：站点保存失败" } }),
    }),
  );
  await save.click();
  await expect(
    page
      .locator('[data-sonner-toast][data-type="error"]')
      .filter({ hasText: "未保存的修改已保留" }),
  ).toBeVisible();
  await expect(save).toBeEnabled();
  const partial = await read();
  expect(writes.sort()).toEqual(["first_party", "sites", "trusted_contact"]);
  expect(partial.find((item) => item.key === "trusted_contact")!.value.enabled).toBe(
    !old("trusted_contact").value.enabled,
  );
  expect(partial.find((item) => item.key === "first_party")!.value.finances).toBe(
    !old("first_party").value.finances,
  );
  expect(partial.find((item) => item.key === "first_party")!.value.health_eligibility).toEqual(
    old("first_party").value.health_eligibility,
  );
  expect(partial.find((item) => item.key === "sites")!.value).toEqual(old("sites").value);
  await expect(sites).toBeChecked({ checked: !old("sites").value.enabled });
  await page.unroute("**/admin/api/consumers/review-consumer/config/sites");
  writes.length = 0;
  const retry = page.waitForResponse(
    (response) =>
      response.request().method() === "PUT" &&
      new URL(response.url()).pathname.endsWith("/config/sites"),
  );
  await save.click();
  expect((await retry).ok()).toBe(true);
  await expect(sites).toBeEnabled();
  await expect(save).toBeDisabled();
  expect(writes).toEqual(["sites"]);
  const after = await read();
  expect(after.find((item) => item.key === "sites")!.value.enabled).toBe(
    !old("sites").value.enabled,
  );
  expect(after.find((item) => item.key === "user_settings")).toEqual(old("user_settings"));

  const personality = page.getByRole("switch", { name: "个性风格", exact: true });
  await expect(
    personality.locator('xpath=ancestor::*[@data-slot="field"]').getByText("默认", { exact: true }),
  ).toBeVisible();
  await personality.check();
  const enabled = page.waitForResponse(
    (response) =>
      response.request().method() === "PUT" &&
      new URL(response.url()).pathname.endsWith("/config/feature_bootstrap"),
  );
  await save.click();
  expect((await enabled).ok()).toBe(true);
  await expect(personality).toBeEnabled();
  expect(
    (await read()).find((item) => item.key === "feature_bootstrap")!.value.feature_gates[
      "3522875721"
    ].value,
  ).toBe(true);
  await page.getByRole("button", { name: "个性风格：恢复默认", exact: true }).click();
  const reset = page.waitForResponse(
    (response) =>
      response.request().method() === "PUT" &&
      new URL(response.url()).pathname.endsWith("/config/feature_bootstrap"),
  );
  await save.click();
  expect((await reset).ok()).toBe(true);
  await expect(personality).toBeEnabled();
  expect(
    (await read()).find((item) => item.key === "feature_bootstrap")!.value.feature_gates[
      "3522875721"
    ].value,
  ).toBeUndefined();
  await page.getByRole("button", { name: "更多功能选项", exact: true }).click();
  await expect(page.getByLabel("申请说明", { exact: true })).toHaveCount(0);
  await expect(page.getByLabel("申请链接", { exact: true })).toHaveCount(0);
  await page.getByLabel("额度申请入口", { exact: true }).click();
  await page.getByRole("option", { name: "自定义申请入口", exact: true }).click();
  await expect(page.getByLabel("申请说明", { exact: true })).toBeVisible();
  await expect(page.getByLabel("申请链接", { exact: true })).toBeVisible();
  await page.getByLabel("额度申请入口", { exact: true }).click();
  await page.getByRole("option", { name: "不显示", exact: true }).click();
  await expect(page.getByLabel("申请说明", { exact: true })).toHaveCount(0);
  await expect(page.getByLabel("申请链接", { exact: true })).toHaveCount(0);
  await noPageOverflow(page);
});
