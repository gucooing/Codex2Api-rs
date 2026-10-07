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
  return { writes, suppliers, state, allocations };
}

test("list refresh checks fresh official quotas and offers an offline state reset", async ({
  page,
}) => {
  const { suppliers } = await fixture(page);
  suppliers[1].status = "disabled";
  suppliers[2].status = "error";
  suppliers[2].authentication_invalid = true;
  suppliers[3].authorized = false;
  let fail = false;
  const reads: string[] = [];
  await page.route("**/admin/api/suppliers/*/quota?refresh=true", async (route) => {
    const id = new URL(route.request().url()).pathname.split("/").at(-2)!;
    reads.push(id);
    if (fail && id === "s0") {
      await route.fulfill({ status: 502, json: { error: { message: "official unavailable" } } });
      return;
    }
    const supplier = suppliers.find((item) => item.id === id)!;
    supplier.status = "active";
    supplier.cooldown_until = null;
    supplier.quota.windows[0].used_percent = 0;
    await route.fulfill({ json: supplier });
  });
  await page.goto("/admin/suppliers/");
  const row = page
    .getByRole("row")
    .filter({ has: page.getByRole("link", { name: "supplier0@example.test", exact: true }) });
  await expect(row.getByText("配额耗尽", { exact: true })).toBeVisible();
  expect(reads).toEqual([]);
  const refresh = page.getByRole("button", { name: "刷新供应账户", exact: true });
  await refresh.click();
  await expect(row.getByText("启用", { exact: true })).toBeVisible();
  await expect(refresh).toBeEnabled();
  expect(reads).toHaveLength(20);
  expect(reads).not.toContain("s1");
  expect(reads).not.toContain("s2");
  expect(reads).not.toContain("s3");
  fail = true;
  suppliers[0].status = "quota_exhausted";
  suppliers[0].cooldown_until = Math.floor(Date.now() / 1000) + 86400;
  await refresh.click();
  await expect(row.getByText("配额耗尽", { exact: true })).toBeVisible();
  await expect(refresh).toBeEnabled();
  const checked = reads.length;
  await row.getByRole("button", { name: "更多操作", exact: true }).click();
  await page.getByRole("menuitem", { name: "重置状态", exact: true }).click();
  await expect(row.getByText("启用", { exact: true })).toBeVisible();
  expect(reads).toHaveLength(checked);
});

test("supplier subscription expiry is visible in table, cards, mobile and account details", async ({
  page,
}) => {
  const { suppliers } = await fixture(page);
  await page.goto("/admin/suppliers/");
  const expiry = await page.evaluate(
    (value) => new Date(value!).toLocaleString("zh-CN"),
    suppliers[0].subscription_expires_at,
  );
  const row = page
    .getByRole("row")
    .filter({ has: page.getByRole("link", { name: "supplier0@example.test", exact: true }) });
  await expect(
    row.locator('[data-label="提供商 / 订阅"]').getByText(`套餐到期：${expiry}`, { exact: true }),
  ).toBeVisible();
  await page.getByRole("radio", { name: "卡片视图", exact: true }).click();
  const card = page
    .locator('[data-slot="card"]')
    .filter({ has: page.getByRole("link", { name: "supplier0@example.test", exact: true }) });
  await expect(card.getByText(expiry, { exact: true })).toBeVisible();
  await page.getByRole("radio", { name: "表格视图", exact: true }).click();
  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByRole("button", { name: "查看详情：supplier0@example.test", exact: true }).click();
  await expect(
    page.getByRole("dialog").getByText(`套餐到期：${expiry}`, { exact: true }),
  ).toBeVisible();
  await page.goto("/admin/suppliers/detail/?id=s0");
  await expect(page.getByText(expiry, { exact: true })).toBeVisible();
});

test("exhausted accounts with stale quota are queried on list load", async ({ page }) => {
  const { suppliers } = await fixture(page);
  suppliers[0].quota.stale = true;
  let reads = 0;
  await page.route("**/admin/api/suppliers/s0/quota", async (route) => {
    reads++;
    suppliers[0].status = "active";
    suppliers[0].cooldown_until = null;
    suppliers[0].quota.stale = false;
    suppliers[0].quota.windows[0].used_percent = 0;
    await route.fulfill({ json: suppliers[0] });
  });
  await page.goto("/admin/suppliers/");
  const row = page
    .getByRole("row")
    .filter({ has: page.getByRole("link", { name: "supplier0@example.test", exact: true }) });
  await expect(row.getByText("启用", { exact: true })).toBeVisible();
  expect(reads).toBe(1);
  await page.getByRole("radio", { name: "卡片视图", exact: true }).click();
  await page.getByRole("radio", { name: "表格视图", exact: true }).click();
  expect(reads).toBe(1);
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
  await fixture(page, true);
  await page.goto("/admin/suppliers/");
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
