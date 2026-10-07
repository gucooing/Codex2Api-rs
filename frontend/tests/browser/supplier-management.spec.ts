import { test, expect, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

async function fixture(page: Page, includeGrok = false) {
  const contracts = JSON.parse(
    await readFile(resolve("../crates/codex2api-admin/tests/contracts.json"), "utf8"),
  );
  const tags = [
    { id: "a", provider_id: "chatgpt", name: "标准池", supplier_count: 22, binding_count: 0 },
    { id: "b", provider_id: "chatgpt", name: "高级池", supplier_count: 1, binding_count: 0 },
    {
      id: "foreign",
      provider_id: "grok",
      name: "其他平台标签",
      supplier_count: 0,
      binding_count: 0,
    },
  ];
  const suppliers = Array.from({ length: 23 }, (_, index) => ({
    id: `s${index}`,
    provider_id: "chatgpt",
    enabled: true,
    display_name: `账户${String(index).padStart(2, "0")}`,
    email: `supplier${index}@example.test`,
    username: `supplier${index}`,
    plan_type: "prolite",
    subscription_expires_at: index === 0 ? "2026-10-17T08:00:00+08:00" : null,
    status: index === 0 ? "quota_exhausted" : "active",
    authorized: true,
    authentication_invalid: false,
    error_message: null,
    error_at: null,
    cooldown_until: index === 0 ? Math.floor(Date.now() / 1000) + 600 : null,
    cooldown_code: null,
    binding_count: 0,
    tag_ids: [index === 1 ? "b" : "a"],
    created_at: "2026-10-01T00:00:00Z",
    last_used_at: null,
    quota: {
      stale: false,
      observed_at: new Date().toISOString(),
      windows: [
        {
          id: "primary_window",
          limit_window_seconds: 604800,
          used_percent: index === 0 ? 100 : 20,
          reset_at: Math.floor(Date.now() / 1000) + 600,
          local_usage: null,
        },
      ],
    },
  }));
  if (includeGrok)
    suppliers.push({
      ...suppliers[1],
      id: "g1",
      provider_id: "grok",
      email: "grok@example.test",
      display_name: "Grok account",
      plan_type: "Free",
      tag_ids: ["foreign"],
    });
  const writes: { account_ids: string[]; tag_ids: string[] }[] = [];
  const state = { assigned: "s1" as string | null, tag: "b", revision: 1 };
  const allocations: { supplier_id: string | null; tag_id: string | null; revision: number }[] = [];
  await page.emulateMedia({ colorScheme: "dark" });
  await page.route("**/admin/api/**", async (route) => {
    const path = new URL(route.request().url()).pathname.slice("/admin/api".length);
    const method = route.request().method();
    let value: unknown = { items: [] };
    if (path === "/session")
      value = {
        authenticated: true,
        username: "admin",
        csrf_token: "test-only",
        app_version: "test",
        codex_cli_version: "0.160.0",
        grok_build_version: "1.0.45",
      };
    else if (path === "/suppliers/tags" && method === "POST") {
      const input = route.request().postDataJSON();
      expect(Object.keys(input).sort()).toEqual(["account_ids", "tag_ids"]);
      writes.push(input);
      for (const supplier of suppliers)
        if (input.account_ids.includes(supplier.id)) supplier.tag_ids = [...input.tag_ids];
      for (const tag of tags)
        tag.supplier_count = suppliers.filter((supplier) =>
          supplier.tag_ids.includes(tag.id),
        ).length;
      value = { ok: true };
    } else if (path.endsWith("/reset-state") && method === "POST") {
      const supplier = suppliers.find((item) => item.id === path.split("/")[2])!;
      supplier.status = "active";
      supplier.cooldown_until = null;
      value = supplier;
    } else if (path === "/suppliers") value = { items: suppliers };
    else if (path.startsWith("/suppliers/"))
      value = suppliers.find((supplier) => supplier.id === path.split("/")[2]);
    else if (path === "/supplier-tags") {
      if (method === "POST") {
        const input = route.request().postDataJSON();
        tags.push({ id: "created", ...input, supplier_count: 0, binding_count: 0 });
      }
      value = { items: tags };
    } else if (path.startsWith("/supplier-tags/")) {
      const tag = tags.find((item) => item.id === path.split("/")[2]);
      if (tag && method === "PUT") tag.name = route.request().postDataJSON().name;
      if (tag && method === "DELETE") tags.splice(tags.indexOf(tag), 1);
      value = { ok: true };
    } else if (path === "/consumers/v1")
      value = {
        ...contracts.consumer,
        id: "v1",
        plan_id: "plus",
        provider_id: "chatgpt",
        name: "测试虚拟账户",
      };
    else if (path === "/consumers/v1/routing") {
      if (method === "PUT") {
        const input = route.request().postDataJSON();
        allocations.push(input);
        state.assigned = input.supplier_id;
        state.tag = input.tag_id;
        state.revision += 1;
      }
      value = {
        items: [
          {
            virtual_account_id: "v1",
            provider_id: "chatgpt",
            supplier_account_id: state.assigned,
            tag_id: state.tag,
            revision: state.revision,
          },
        ],
      };
    } else if (path === "/consumers/v1/rate-limit")
      value = { rpm: null, default_rpm: 20, effective_rpm: 20 };
    else if (path === "/plans") value = contracts.plans;
    await route.fulfill({ json: value });
  });
  return { writes, suppliers, tags, state, allocations };
}

test("list query retries an initial failure without requesting quotas", async ({ page }) => {
  await fixture(page);
  let listReads = 0;
  let quotaReads = 0;
  let releaseRetry: () => void = () => {};
  const retryGate = new Promise<void>((resolve) => {
    releaseRetry = resolve;
  });
  page.on("request", (request) => {
    if (new URL(request.url()).pathname.endsWith("/quota")) quotaReads++;
  });
  await page.route("**/admin/api/suppliers", async (route) => {
    listReads++;
    if (listReads === 1) {
      await route.fulfill({
        status: 503,
        json: { error: { message: "供应列表暂时不可用" } },
      });
      return;
    }
    await retryGate;
    await route.fallback();
  });
  await page.goto("/admin/suppliers/");
  const query = page.getByRole("button", { name: "查询", exact: true });
  await expect(page.getByRole("button", { name: "刷新供应账户", exact: true })).toHaveCount(0);
  await expect(page.getByText("供应列表暂时不可用", { exact: true })).toBeVisible();
  await expect(query).toBeEnabled();
  const retry = page.waitForRequest("**/admin/api/suppliers");
  await query.click();
  await retry;
  try {
    await expect(page.getByText("供应账户额度已刷新", { exact: true })).toHaveCount(0);
    expect(quotaReads).toBe(0);
  } finally {
    releaseRetry();
  }
  await expect(
    page.getByRole("link", { name: "supplier0@example.test", exact: true }),
  ).toBeVisible();
  await expect(query).toBeEnabled();
  expect(listReads).toBe(2);
});

test("failed list query preserves rows and can retry without quota requests", async ({ page }) => {
  const { suppliers } = await fixture(page);
  let failList = false;
  let quotaReads = 0;
  page.on("request", (request) => {
    if (new URL(request.url()).pathname.endsWith("/quota")) quotaReads++;
  });
  await page.route("**/admin/api/suppliers", async (route) => {
    if (failList) {
      await route.abort("failed");
      return;
    }
    await route.fallback();
  });
  await page.goto("/admin/suppliers/");
  const original = page.getByRole("link", { name: "supplier0@example.test", exact: true });
  await expect(original).toBeVisible();
  const query = page.getByRole("button", { name: "查询", exact: true });
  failList = true;
  await query.click();
  await expect(
    page.getByText("无法连接后端服务，请检查网络后重试。", { exact: true }),
  ).toBeVisible();
  await expect(original).toBeVisible();
  await expect(query).toBeEnabled();
  expect(quotaReads).toBe(0);
  const row = page.getByRole("row").filter({ has: original });
  await row.getByRole("button", { name: "更多操作", exact: true }).click();
  await expect(page.getByRole("menuitem", { name: "重置状态", exact: true })).toBeDisabled();
  await page.keyboard.press("Escape");
  failList = false;
  suppliers[0].email = "recovered@example.test";
  await query.click();
  await expect(
    page.getByRole("link", { name: "recovered@example.test", exact: true }),
  ).toBeVisible();
  await expect(query).toBeEnabled();
  expect(quotaReads).toBe(0);
});

test("list state reset is offline and keeps the persisted quota snapshot", async ({ page }) => {
  const { suppliers } = await fixture(page);
  suppliers[0].quota.stale = true;
  const quotaRequests: string[] = [];
  page.on("request", (request) => {
    if (new URL(request.url()).pathname.endsWith("/quota")) quotaRequests.push(request.url());
  });
  await page.goto("/admin/suppliers/");
  const row = page
    .getByRole("row")
    .filter({ has: page.getByRole("link", { name: "supplier0@example.test", exact: true }) });
  await expect(row.getByText("配额耗尽", { exact: true })).toBeVisible();
  await row.getByRole("button", { name: "更多操作", exact: true }).click();
  await page.getByRole("menuitem", { name: "重置状态", exact: true }).click();
  await expect(row.getByText("正常", { exact: true })).toBeVisible();
  await expect(row.getByText("100%", { exact: true })).toBeVisible();
  expect(quotaRequests).toHaveLength(0);
});

test("supplier subscription expiry is visible in table, cards, mobile and account details", async ({
  page,
}, info) => {
  const { suppliers, tags } = await fixture(page);
  suppliers[0].binding_count = 3;
  suppliers[0].tag_ids = ["a", "b", "extra"];
  suppliers[1].tag_ids = ["a", "b"];
  tags.push({
    id: "extra",
    provider_id: "chatgpt",
    name: "备用池",
    supplier_count: 1,
    binding_count: 0,
  });
  await page.goto("/admin/suppliers/");
  const expiry = await page.evaluate(
    (value) => new Date(value!).toLocaleString("zh-CN"),
    suppliers[0].subscription_expires_at,
  );
  const row = page
    .getByRole("row")
    .filter({ has: page.getByRole("link", { name: "supplier0@example.test", exact: true }) });
  await expect(
    row.locator('[data-label="套餐到期"]').getByText(expiry, { exact: true }),
  ).toBeVisible();
  await expect(row.locator('[data-label="绑定数"]')).toHaveText("3");
  await expect(row.getByRole("link", { name: "详情", exact: true })).toHaveCount(0);
  const tagCell = row.locator('[data-label="标签"]');
  await expect(tagCell.locator('[data-slot="badge"]')).toHaveCount(2);
  await expect(tagCell).toContainText("标准池");
  await expect(tagCell).toContainText("高级池");
  await expect(tagCell).not.toContainText("备用池");
  await expect(row.locator('[data-label="操作"]').locator('[data-slot="badge"]')).toHaveCount(0);
  await expect(row.locator('[data-label="操作"]').getByRole("button")).toHaveCount(1);
  const secondRow = page.getByRole("row").nth(2);
  await expect(secondRow.locator('[data-label="标签"]').locator('[data-slot="badge"]')).toHaveCount(
    2,
  );
  await expect(secondRow.getByRole("button", { name: "查看全部标签" })).toHaveCount(0);
  await tagCell.getByRole("button", { name: "查看全部标签" }).click();
  const tagPopover = page.getByRole("dialog", { name: "全部标签", exact: true });
  for (const name of ["标准池", "高级池", "备用池"])
    await expect(tagPopover.getByText(name, { exact: true })).toBeVisible();
  await page.screenshot({ path: info.outputPath("supplier-tags-popover.png") });
  await page.keyboard.press("Escape");
  await expect(tagPopover).not.toBeVisible();
  await page.screenshot({ path: info.outputPath("supplier-tags-column.png") });
  await page.getByRole("button", { name: "切换为卡片视图", exact: true }).click();
  const card = page
    .locator('[data-slot="card"]')
    .filter({ has: page.getByRole("link", { name: "supplier0@example.test", exact: true }) });
  await expect(card.getByText(expiry, { exact: true })).toBeVisible();
  await card.getByRole("button", { name: "查看全部标签" }).click();
  await expect(tagPopover.getByText("备用池", { exact: true })).toBeVisible();
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "切换为表格视图", exact: true }).click();
  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByRole("button", { name: "查看详情：supplier0@example.test", exact: true }).click();
  await expect(page.getByRole("dialog").getByText(expiry, { exact: true })).toBeVisible();
  await expect(page.getByRole("dialog").getByText("备用池", { exact: true })).toBeVisible();
  await page.keyboard.press("Escape");
  await page.getByRole("row").nth(1).getByRole("button", { name: "更多操作" }).click();
  await page.getByRole("menuitem", { name: "详情", exact: true }).click();
  await expect(page).toHaveURL(/\/admin\/suppliers\/detail\/\?id=s0$/);
  await expect(page.getByText(expiry, { exact: true })).toBeVisible();
});

test("list load, query, view changes and clock ticks never request stale or missing quotas", async ({
  page,
}) => {
  const { suppliers } = await fixture(page);
  suppliers[0].quota.stale = true;
  suppliers[1].quota.stale = true;
  let reads = 0;
  page.on("request", (request) => {
    if (new URL(request.url()).pathname.endsWith("/quota")) reads++;
  });
  await page.route("**/admin/api/suppliers", async (route) => {
    await route.fulfill({
      json: {
        items: suppliers.map((item, index) => (index === 2 ? { ...item, quota: null } : item)),
      },
    });
  });
  await page.clock.install();
  await page.goto("/admin/suppliers/");
  const row = page
    .getByRole("row")
    .filter({ has: page.getByRole("link", { name: "supplier0@example.test", exact: true }) });
  await expect(row.getByText("配额耗尽", { exact: true })).toBeVisible();
  await expect(page.getByText("暂无额度缓存", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "刷新供应账户", exact: true })).toHaveCount(0);
  await page.getByRole("button", { name: "切换为卡片视图", exact: true }).click();
  await page.getByRole("button", { name: "切换为表格视图", exact: true }).click();
  await page.clock.fastForward(31_000);
  await page.getByRole("button", { name: "查询", exact: true }).click();
  await expect(row.getByText("100%", { exact: true })).toBeVisible();
  await page.reload();
  await expect(row.getByText("配额耗尽", { exact: true })).toBeVisible();
  expect(reads).toBe(0);
});

test("left-side cross-page selection updates complete tag sets and tag management has its own page", async ({
  page,
}, info) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  const { writes } = await fixture(page);
  await page.addInitScript(() => {
    const key = "codex2api-ui-v1:suppliers.filters";
    if (!localStorage.getItem(key))
      localStorage.setItem(key, JSON.stringify({ search: "", status: "rate_limited", tag: "" }));
  });
  await page.goto("/admin/suppliers/");
  const table = page.getByRole("table");
  await expect(table.getByRole("row")).toHaveCount(21);
  const first = table.getByRole("row").nth(1);
  await expect(first.locator("td").first().getByRole("checkbox")).toBeVisible();
  await expect(first.locator("td").last().getByRole("checkbox")).toHaveCount(0);
  await expect(page.getByText(/后可重试/)).toHaveCount(0);
  await page.getByRole("combobox", { name: "状态", exact: true }).click();
  await expect(page.getByRole("option", { name: "请求限流中", exact: true })).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(page.getByRole("progressbar").first()).toBeVisible();
  await expect(page.getByRole("button", { name: "标签管理", exact: true })).toHaveCount(0);
  await page.screenshot({ path: info.outputPath("suppliers.png"), fullPage: true });
  await page.getByRole("checkbox", { name: "选择当前页供应账户", exact: true }).check();
  await expect(page.getByText("已选 20 个账户", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "下一页", exact: true }).click();
  await page
    .getByRole("checkbox", { name: "选择供应账户 supplier20@example.test", exact: true })
    .check();
  await expect(page.getByText("已选 21 个账户", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "全选筛选结果（23）", exact: true }).click();
  await page.getByRole("button", { name: "更新标签", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "批量更新标签", exact: true });
  await expect(dialog.getByRole("button", { name: "更新标签", exact: true })).toBeDisabled();
  await expect(dialog.getByLabel("其他平台标签", { exact: true })).toHaveCount(0);
  await dialog.getByRole("button", { name: "清空勾选", exact: true }).click();
  await dialog.getByRole("checkbox", { name: "高级池", exact: true }).check();
  await dialog.getByRole("button", { name: "更新标签", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  expect(writes[0].account_ids).toHaveLength(23);
  expect(writes[0].tag_ids).toEqual(["b"]);

  await page.getByRole("link", { name: "标签管理", exact: true }).click();
  await expect(page).toHaveURL(/\/admin\/supplier-tags\/$/);
  await expect(page.getByRole("table").getByRole("textbox")).toHaveCount(0);
  await page.getByRole("button", { name: "添加标签", exact: true }).click();
  const createTag = page.getByRole("dialog", { name: "添加标签", exact: true });
  await createTag.getByLabel("标签名称", { exact: true }).fill("新标签");
  await createTag.getByRole("button", { name: "保存", exact: true }).click();
  await expect(createTag).toHaveCount(0);
  await page
    .getByRole("row")
    .filter({ has: page.getByRole("cell", { name: "新标签", exact: true }) })
    .getByRole("button", { name: "编辑", exact: true })
    .click();
  const editTag = page.getByRole("dialog", { name: "编辑标签", exact: true });
  await expect(editTag.getByRole("button", { name: "取消", exact: true })).toHaveCount(1);
  await expect(editTag.getByLabel("标签名称", { exact: true })).toHaveValue("新标签");
  await editTag.getByLabel("标签名称", { exact: true }).fill("已改名标签");
  await editTag.getByRole("button", { name: "保存", exact: true }).click();
  await expect(editTag).toHaveCount(0);
  await expect(page.getByRole("cell", { name: "已改名标签", exact: true })).toBeVisible();
  await page.screenshot({ path: info.outputPath("tag-management.png"), fullPage: true });

  await page.goto("/admin/suppliers/detail/?id=s1");
  await page.getByRole("tab", { name: "标签", exact: true }).click();
  await expect(page.getByRole("checkbox", { name: "高级池", exact: true })).toBeChecked();
  await expect(page.getByRole("checkbox", { name: "其他平台标签", exact: true })).toHaveCount(0);
  await page.getByRole("checkbox", { name: "高级池", exact: true }).uncheck();
  await page.getByRole("checkbox", { name: "标准池", exact: true }).check();
  await page.getByRole("button", { name: "更新标签", exact: true }).click();
  await expect.poll(() => writes.length).toBe(2);
  expect(writes[1]).toEqual({ account_ids: ["s1"], tag_ids: ["a"] });
  await page.screenshot({ path: info.outputPath("supplier-detail-tags.png"), fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/admin/suppliers/");
  await expect(
    page.getByRole("table").getByRole("row").nth(1).locator("td").first().getByRole("checkbox"),
  ).toBeVisible();
  await page.screenshot({ path: info.outputPath("suppliers-mobile.png"), fullPage: true });
  expect(errors).toEqual([]);
});

test("allocation shows the assigned account or unassigned state without operational instructions", async ({
  page,
}, info) => {
  const { state, allocations } = await fixture(page);
  await page.goto("/admin/consumers/detail/?id=v1");
  await expect(page.getByRole("combobox", { name: "分配账户", exact: true })).toContainText(
    "账户01",
  );
  await expect(page.getByText("当前临时绑定 / 手动选择", { exact: true })).toHaveCount(0);
  await expect(page.getByText(/自动选择绑定数最少|保留健康绑定|内部换号不重复计数/)).toHaveCount(0);
  await page.screenshot({ path: info.outputPath("assigned-account.png"), fullPage: true });
  await page.getByRole("combobox", { name: "标签号池", exact: true }).click();
  await page.getByRole("option", { name: /^标准池/ }).click();
  await page.getByRole("combobox", { name: "分配账户", exact: true }).click();
  await page.getByRole("option", { name: "账户02", exact: true }).click();
  await page.getByRole("button", { name: "保存绑定", exact: true }).click();
  await expect.poll(() => allocations.length).toBe(1);
  expect(allocations[0]).toEqual({ tag_id: "a", supplier_id: "s2", revision: 1 });
  await expect(page.getByRole("combobox", { name: "分配账户", exact: true })).toContainText(
    "账户02",
  );

  state.assigned = null;
  await page.reload();
  await expect(page.getByRole("combobox", { name: "分配账户", exact: true })).toContainText(
    "暂未分配",
  );
  await page.screenshot({ path: info.outputPath("unassigned-account.png"), fullPage: true });
});

test("supplier filters, one view toggle and contextual bulk actions stay consistent", async ({
  page,
}, info) => {
  const { suppliers, writes } = await fixture(page, true);
  suppliers[1].tag_ids = [];
  suppliers[20].tag_ids = [];
  suppliers[3].status = "payment_required";
  suppliers[4].status = "error";
  suppliers[4].authentication_invalid = true;
  await page.goto("/admin/suppliers/");
  const rejected = page
    .getByRole("row")
    .filter({ has: page.getByRole("link", { name: "supplier4@example.test", exact: true }) });
  await rejected.getByRole("button", { name: "更多操作", exact: true }).click();
  await expect(page.getByRole("menuitem", { name: "检查授权", exact: true })).toHaveCount(0);
  await expect(page.getByRole("menuitem", { name: "停用账户", exact: true })).toBeEnabled();
  await page.keyboard.press("Escape");
  await page.getByRole("combobox", { name: "状态", exact: true }).click();
  await page.getByRole("option", { name: "账单受限", exact: true }).click();
  await page.getByRole("button", { name: "查询", exact: true }).click();
  await expect(page.getByRole("table").getByRole("row")).toHaveCount(2);
  await expect(page.getByRole("table").getByText("账单受限", { exact: true })).toBeVisible();
  await page.screenshot({ path: info.outputPath("supplier-billing-state.png") });
  await page.getByRole("button", { name: "重置", exact: true }).click();
  await expect(page.getByLabel("版本信息")).toContainText("Grok Build 1.0.45");
  await expect(page.getByRole("toolbar", { name: "已选账户操作" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "更新标签", exact: true })).toHaveCount(0);
  await expect(page.getByRole("combobox", { name: "标签", exact: true })).toBeVisible();
  await expect(page.getByRole("combobox", { name: "标签号池", exact: true })).toHaveCount(0);
  const form = page
    .locator("form")
    .filter({ has: page.getByRole("button", { name: "查询", exact: true }) });
  await expect(form.getByRole("combobox", { name: "平台", exact: true })).toBeVisible();
  await expect(form.getByRole("combobox", { name: "标签", exact: true })).toBeVisible();
  const tableButton = page.getByRole("button", { name: "切换为卡片视图", exact: true });
  await expect(tableButton).toHaveCount(1);
  await tableButton.click();
  await expect(page.getByRole("button", { name: "切换为表格视图", exact: true })).toHaveCount(1);
  await page.getByRole("button", { name: "切换为表格视图", exact: true }).click();
  await page.getByRole("combobox", { name: "平台", exact: true }).click();
  await page.getByRole("option", { name: "Grok", exact: true }).click();
  await page.getByRole("combobox", { name: "标签", exact: true }).click();
  await expect(page.getByRole("option", { name: "标准池", exact: true })).toHaveCount(0);
  await page.getByRole("option", { name: "其他平台标签", exact: true }).click();
  await expect(page.getByRole("table").getByRole("row")).toHaveCount(21);
  await page.getByRole("button", { name: "查询", exact: true }).click();
  await expect(page.getByRole("table").getByRole("row")).toHaveCount(2);
  await expect(page.getByRole("table")).toContainText("grok@example.test");
  await page.reload();
  await expect(page.getByRole("combobox", { name: "平台", exact: true })).toContainText("Grok");
  await expect(page.getByRole("table").getByRole("row")).toHaveCount(2);
  await page.getByRole("checkbox", { name: "选择当前页供应账户", exact: true }).check();
  await expect(page.getByRole("toolbar", { name: "已选账户操作" })).toBeVisible();
  await page.getByRole("button", { name: "清除选择", exact: true }).click();
  await expect(page.getByRole("toolbar", { name: "已选账户操作" })).toHaveCount(0);
  await page.getByRole("combobox", { name: "平台", exact: true }).click();
  await page.getByRole("option", { name: "ChatGPT", exact: true }).click();
  await expect(page.getByRole("combobox", { name: "标签", exact: true })).toContainText("全部标签");
  await page.getByRole("button", { name: "查询", exact: true }).click();
  await expect(page.getByRole("table").getByRole("row")).toHaveCount(21);
  await page.getByRole("button", { name: "重置", exact: true }).click();
  await page.getByRole("checkbox", { name: "选择当前页供应账户", exact: true }).check();
  await page.getByRole("button", { name: "全选筛选结果（24）", exact: true }).click();
  await expect(page.getByRole("button", { name: "更新标签", exact: true })).toBeDisabled();
  await page.screenshot({ path: info.outputPath("supplier-filters-selected.png"), fullPage: true });
  await page.getByRole("button", { name: "清除选择", exact: true }).click();
  await page.screenshot({ path: info.outputPath("supplier-filters.png"), fullPage: true });

  suppliers.find((supplier) => supplier.id === "g1")!.tag_ids = [];
  await page.getByRole("button", { name: "下一页", exact: true }).click();
  await expect(page.getByRole("table").getByRole("row")).toHaveCount(5);
  await page.getByRole("combobox", { name: "标签", exact: true }).click();
  await page.getByRole("option", { name: "无标签", exact: true }).click();
  await expect(page.getByRole("table").getByRole("row")).toHaveCount(5);
  await page.getByRole("button", { name: "查询", exact: true }).click();
  await expect(page.getByRole("table").getByRole("row")).toHaveCount(4);
  for (const email of ["supplier1@example.test", "supplier20@example.test", "grok@example.test"])
    await expect(page.getByRole("link", { name: email, exact: true })).toBeVisible();
  await page.reload();
  await expect(page.getByRole("combobox", { name: "标签", exact: true })).toContainText("无标签");
  await expect(page.getByRole("table").getByRole("row")).toHaveCount(4);
  await page.getByRole("combobox", { name: "平台", exact: true }).click();
  await page.getByRole("option", { name: "ChatGPT", exact: true }).click();
  await page.getByRole("combobox", { name: "标签", exact: true }).click();
  await page.getByRole("option", { name: "无标签", exact: true }).click();
  await page.getByRole("button", { name: "查询", exact: true }).click();
  await expect(page.getByRole("table").getByRole("row")).toHaveCount(3);
  await page.getByRole("checkbox", { name: "选择当前页供应账户", exact: true }).check();
  await page.getByRole("button", { name: "更新标签", exact: true }).click();
  const tagDialog = page.getByRole("dialog", { name: "批量更新标签", exact: true });
  await tagDialog.getByRole("checkbox", { name: "标准池", exact: true }).check();
  await tagDialog.getByRole("button", { name: "更新标签", exact: true }).click();
  await expect(tagDialog).not.toBeVisible();
  expect(writes).toEqual([{ account_ids: ["s1", "s20"], tag_ids: ["a"] }]);
  await expect(page.getByText("暂无符合条件的供应账户", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "重置", exact: true }).click();
  await expect(page.getByRole("combobox", { name: "标签", exact: true })).toContainText("全部标签");
  await expect(page.getByRole("table").getByRole("row")).toHaveCount(21);
});

test("plan edit has exactly one cancel and one close control", async ({ page }, info) => {
  await fixture(page);
  await page.goto("/admin/plans/");
  await page.getByRole("button", { name: "编辑", exact: true }).first().click();
  const dialog = page.getByRole("dialog", { name: "编辑套餐", exact: true });
  await expect(dialog.getByRole("button", { name: "取消", exact: true })).toHaveCount(1);
  await expect(dialog.getByRole("button", { name: /^(关闭|Close)$/ })).toHaveCount(1);
  await page.screenshot({ path: info.outputPath("plan-edit.png"), fullPage: true });
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  await expect(dialog).toHaveCount(0);
});
