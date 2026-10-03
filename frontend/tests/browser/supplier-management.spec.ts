import { test, expect, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

async function fixture(page: Page) {
  const contracts = JSON.parse(
    await readFile(resolve("../crates/codex2api-admin/tests/contracts.json"), "utf8"),
  );
  const tags = [
    { id: "a", provider_id: "chatgpt", name: "标准池", supplier_count: 22, binding_count: 0 },
    { id: "b", provider_id: "chatgpt", name: "高级池", supplier_count: 1, binding_count: 0 },
    {
      id: "foreign",
      provider_id: "other",
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
  await page.getByRole("combobox", { name: "账户状态", exact: true }).click();
  await expect(page.getByRole("option", { name: "请求限流中", exact: true })).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(page.getByRole("progressbar").first()).toBeVisible();
  await expect(page.getByRole("button", { name: "标签管理", exact: true })).toHaveCount(0);
  await page.screenshot({ path: info.outputPath("suppliers.png"), fullPage: true });
  await page.getByRole("checkbox", { name: "选择当前页供应账户", exact: true }).check();
  await expect(page.getByText("已选择 20 个", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "下一页", exact: true }).click();
  await page.getByRole("checkbox", { name: "选择供应账户 账户20", exact: true }).check();
  await expect(page.getByText("已选择 21 个", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "选择全部筛选结果（23）", exact: true }).click();
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
  await page.getByLabel("标签名称", { exact: true }).fill("新标签");
  await page.getByRole("button", { name: "创建标签", exact: true }).click();
  await expect(page.getByLabel("标签名称 新标签", { exact: true })).toBeVisible();
  await page.getByLabel("标签名称 新标签", { exact: true }).fill("已改名标签");
  await page
    .getByRole("row")
    .filter({ has: page.getByLabel("标签名称 新标签", { exact: true }) })
    .getByRole("button", { name: "保存", exact: true })
    .click();
  await expect(page.getByLabel("标签名称 已改名标签", { exact: true })).toBeVisible();
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
