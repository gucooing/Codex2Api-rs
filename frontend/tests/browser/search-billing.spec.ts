import { test, expect } from "@playwright/test";
import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";

test("search per-request pricing saves and preserves its form during read failures", async ({
  page,
}) => {
  const errors: string[] = [];
  let signedIn = false;
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (!signedIn && message.text().includes("status of 401")) return;
    if (["error", "warning"].includes(message.type()) && !message.text().includes("status of 500"))
      errors.push(message.text());
  });
  await page.goto("/admin/");
  await page.getByLabel("用户名", { exact: true }).fill("admin");
  await page.getByLabel("密码", { exact: true }).fill("admin");
  const login = page.waitForResponse(
    (response) =>
      response.url().endsWith("/admin/api/login") && response.request().method() === "POST",
    { timeout: 60000 },
  );
  await page.getByRole("button", { name: "登录", exact: true }).click();
  expect((await login).status()).toBe(200);
  await expect(page.getByRole("link", { name: "计费配置", exact: true })).toBeVisible();
  signedIn = true;
  await page.getByRole("link", { name: "计费配置", exact: true }).click();
  await page.getByRole("tab", { name: "其他计费", exact: true }).click();
  const price = page.getByLabel("每次价格（USD）", { exact: true });
  const save = page.getByRole("button", { name: "保存", exact: true });
  await expect(price).toBeEnabled();
  await expect(save).toBeDisabled();
  await expect(page.getByRole("tabpanel")).not.toContainText("alpha/search");
  await price.fill("0.005");
  await save.click();
  await expect(page.getByText("搜索价格已保存", { exact: true })).toBeVisible();
  await expect(save).toBeDisabled();
  await page.reload();
  await page.getByRole("tab", { name: "其他计费", exact: true }).click();
  await expect(price).toHaveValue("0.005");
  await page.route("**/admin/api/billing/chatgpt/search", async (route) => {
    if (route.request().method() === "GET")
      await route.fulfill({
        status: 500,
        contentType: "application/json",
        body: JSON.stringify({ error: { code: "test_failure", message: "暂时无法读取" } }),
      });
    else await route.continue();
  });
  await page.getByRole("button", { name: "刷新", exact: true }).click();
  await expect(price).toBeDisabled();
  await expect(price).toHaveValue("0.005");
  await expect(save).toBeDisabled();
  await page.unroute("**/admin/api/billing/chatgpt/search");
  await page.getByRole("button", { name: "刷新", exact: true }).click();
  await expect(price).toBeEnabled();
  const output = resolve(
    process.env.CODEX2API_TEST_OUTPUT_DIR ?? "../target/search-billing-review",
    "screenshots",
  );
  await mkdir(output, { recursive: true });
  await page.screenshot({ path: resolve(output, "search-billing.png"), fullPage: true });
  expect(errors).toEqual([]);
});
