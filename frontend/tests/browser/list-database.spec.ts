import { test, expect } from "@playwright/test";

test("user list filters and pages the real SQLite database through the rendered UI", async ({
  page,
  context,
}, info) => {
  const login = await context.request.post("/admin/api/login", {
    data: { username: "admin", password: "admin" },
  });
  expect(login.status()).toBe(200);
  const { csrf_token } = await login.json();
  const prefix = `list-browser-${Date.now()}`;
  const literal = `${prefix}' OR 1=1 --`;
  for (let i = 0; i < 25; i++) {
    const created = await context.request.post("/admin/api/users", {
      headers: { "x-csrf-token": csrf_token },
      data: {
        username: `${prefix}-${String(i).padStart(2, "0")}`,
        name: i === 0 ? literal : `${prefix} ${i}`,
        email: `${prefix}-${i}@example.test`,
        password: "isolated-list-test",
        enabled: true,
        revision: null,
      },
    });
    expect(created.status()).toBe(200);
  }
  await page.goto("/admin/users/");
  await page.getByLabel("搜索用户", { exact: true }).fill(prefix);
  await page.getByRole("button", { name: "查询", exact: true }).click();
  await expect(page.getByRole("table").getByRole("row")).toHaveCount(21);
  const next = page.waitForResponse((response) => {
    const url = new URL(response.url());
    return url.pathname === "/admin/api/users" && url.searchParams.get("page") === "2";
  });
  await page.getByRole("button", { name: "下一页", exact: true }).click();
  const second = await (await next).json();
  expect(second.total).toBe(25);
  expect(second.page).toBe(2);
  expect(second.items).toHaveLength(5);
  await expect(page.getByRole("table").getByRole("row")).toHaveCount(6);
  await page.getByLabel("搜索用户", { exact: true }).fill(literal);
  const quoted = page.waitForResponse((response) => {
    const url = new URL(response.url());
    return url.pathname === "/admin/api/users" && url.searchParams.get("search") === literal;
  });
  await page.getByRole("button", { name: "查询", exact: true }).click();
  const result = await (await quoted).json();
  expect(result.total).toBe(1);
  expect(result.page).toBe(1);
  expect(result.items[0].name).toBe(literal);
  await expect(page.getByRole("table").getByRole("row")).toHaveCount(2);
  await page.screenshot({ path: info.outputPath("database-filter.png"), fullPage: true });
});
