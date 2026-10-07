import { test, expect } from "@playwright/test";

test("Grok uses its own login contract, accepts pasted codes and imports independent RT rows", async ({
  page,
}, info) => {
  const starts: Record<string, unknown>[] = [];
  const callbacks: Record<string, unknown>[] = [];
  await page.route("**/admin/api/**", async (route) => {
    const path = new URL(route.request().url()).pathname.slice("/admin/api".length);
    let value: unknown = { items: [] };
    if (path === "/session")
      value = {
        authenticated: true,
        username: "admin",
        csrf_token: "fixture-csrf",
        app_version: "test",
        codex_cli_version: "0.160.0",
      };
    if (path.endsWith("/oauth/setup"))
      value = {
        fingerprint: {
          os_type: "Windows",
          os_version: "10.0.26100",
          arch: "x86_64",
          terminal: "grok-pager",
          timezone: "",
          proxy_id: null,
        },
      };
    if (path === "/suppliers/grok/oauth/start") {
      const body = route.request().postDataJSON();
      starts.push(body);
      value =
        body.method === "refresh_token"
          ? { status: "complete", supplier_id: `supplier-${starts.length}`, reused_existing: false }
          : {
              status: "pending",
              method: "callback",
              state: "grok-fixture",
              authorize_url: "https://auth.x.ai/oauth2/authorize?fixture=1",
            };
    }
    if (path === "/suppliers/grok/oauth/callback") {
      callbacks.push(route.request().postDataJSON());
      value = { status: "complete", supplier_id: "supplier-code" };
    }
    await route.fulfill({ json: value });
  });
  await page.goto("/admin/suppliers/");
  await page.getByRole("button", { name: "添加供应账户", exact: true }).click();
  await page.getByRole("combobox", { name: "提供商", exact: true }).click();
  await page.getByRole("option", { name: "Grok", exact: true }).click();
  await page.getByRole("button", { name: "下一步", exact: true }).click();
  await page.getByRole("radio", { name: "RT 授权", exact: true }).check();
  await page.getByRole("button", { name: "下一步", exact: true }).click();
  await page
    .getByLabel("Refresh Token", { exact: true })
    .fill("fixture-rt-one\nfixture-rt-two\nfixture-rt-one");
  await page.getByRole("button", { name: "开始授权", exact: true }).click();
  await expect(page.getByText("添加成功", { exact: true })).toHaveCount(2);
  await expect(page.getByText("与第 1 行重复，已跳过", { exact: true })).toBeVisible();
  expect(starts).toHaveLength(2);
  expect(starts.every((v) => v.independent_fingerprint === true)).toBeTruthy();
  await page.screenshot({ path: info.outputPath("grok-batch.png"), fullPage: true });
  await page.getByRole("button", { name: "完成并刷新列表", exact: true }).click();
  await page.getByRole("button", { name: "添加供应账户", exact: true }).click();
  await page.getByRole("combobox", { name: "提供商", exact: true }).click();
  await page.getByRole("option", { name: "Grok", exact: true }).click();
  await page.getByRole("button", { name: "下一步", exact: true }).click();
  await page.getByRole("radio", { name: "代码／回调授权", exact: true }).check();
  await page.getByRole("button", { name: "下一步", exact: true }).click();
  await page.getByLabel("授权代码或完整回调链接", { exact: true }).fill("official-pasted-code");
  await page.getByRole("button", { name: "提交回调", exact: true }).click();
  await expect.poll(() => callbacks.length).toBe(1);
  expect(callbacks[0]).toEqual({ state: "grok-fixture", code: "official-pasted-code" });
});

test("supplier details display the email and Grok's observed subscription", async ({
  page,
}, info) => {
  await page.route("**/admin/api/**", async (route) => {
    const path = new URL(route.request().url()).pathname.slice("/admin/api".length);
    let value: unknown = { items: [] };
    if (path === "/session")
      value = {
        authenticated: true,
        username: "admin",
        csrf_token: "fixture-csrf",
        app_version: "test",
        codex_cli_version: "0.160.0",
      };
    if (path === "/suppliers/grok-fixture")
      value = {
        id: "grok-fixture",
        provider_id: "grok",
        email: "supplier@example.test",
        username: "WRONG-USERNAME",
        status: "active",
        enabled: true,
        plan_type: "Free",
        tag_ids: [],
        authorized: true,
        authentication_invalid: false,
        quota: null,
        created_at: "2026-10-04T00:00:00Z",
      };
    await route.fulfill({ json: value });
  });
  await page.goto("/admin/suppliers/detail/?id=grok-fixture");
  await expect(page.getByText("supplier@example.test", { exact: true })).toHaveCount(2);
  await expect(page.getByText("WRONG-USERNAME")).toHaveCount(0);
  await expect(page.getByText("Free", { exact: true })).toBeVisible();
  await page.screenshot({ path: info.outputPath("grok-details.png"), fullPage: true });
});

for (const provider of ["chatgpt", "grok"]) {
  test(`${provider} reauthorization submits RT for the existing supplier`, async ({ page }) => {
    const writes: unknown[] = [];
    await page.route("**/admin/api/**", async (route) => {
      const path = new URL(route.request().url()).pathname.slice("/admin/api".length);
      let value: unknown = { items: [] };
      if (path === "/session")
        value = {
          authenticated: true,
          username: "admin",
          csrf_token: "fixture-csrf",
          app_version: "test",
          codex_cli_version: "0.160.0",
          grok_build_version: "1.0.45",
        };
      if (path === "/suppliers/existing")
        value = {
          id: "existing",
          provider_id: provider,
          email: "supplier@example.test",
          status: "active",
          plan_type: provider === "grok" ? "Free" : "plus",
          tag_ids: [],
          authorized: true,
          quota: null,
        };
      if (path === "/suppliers/existing/relogin") {
        writes.push(route.request().postDataJSON());
        value = { status: "complete", supplier_id: "existing" };
      }
      await route.fulfill({ json: value });
    });
    await page.goto("/admin/suppliers/detail/?id=existing");
    await page.getByRole("button", { name: "重新授权", exact: true }).click();
    const dialog = page.getByRole("dialog", { name: "重新授权供应账户", exact: true });
    await dialog.getByRole("radio", { name: "RT 授权", exact: true }).check();
    await dialog.getByRole("button", { name: "下一步", exact: true }).click();
    await dialog.getByLabel("Refresh Token", { exact: true }).fill("replacement-rt");
    await dialog.getByRole("button", { name: "开始授权", exact: true }).click();
    await expect(dialog).toHaveCount(0);
    expect(writes).toEqual([{ method: "refresh_token", refresh_token: "replacement-rt" }]);
  });
}
