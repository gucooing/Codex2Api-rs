import { test, expect } from "@playwright/test";

test("public base addresses retain explicit schemes and ports across saves and refresh failures", async ({
  page,
}, info) => {
  let settings = {
    api_url: "http://127.0.0.1:8080",
    user_url: "http://127.0.0.1:8082",
    admin_url: "http://127.0.0.1:8081",
    revision: 0,
  };
  const writes: (typeof settings)[] = [];
  let failRead = false;
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
    if (path === "/settings/gateway")
      value = { ua_mode: "blacklist", ua_rules: [], default_rpm: 20 };
    if (path === "/settings/public-urls") {
      if (route.request().method() === "PUT") {
        expect(route.request().headers()["x-csrf-token"]).toBe("fixture-csrf");
        const body = route.request().postDataJSON() as typeof settings;
        expect(body.revision).toBe(settings.revision);
        writes.push(body);
        settings = { ...body, revision: body.revision + 1 };
      } else if (failRead) {
        await route.fulfill({ status: 503, json: { error: { message: "暂时无法读取访问地址" } } });
        return;
      }
      value = settings;
    }
    await route.fulfill({ json: value });
  });
  await page.goto("/admin/settings/");
  await page.getByRole("tab", { name: "访问地址", exact: true }).click();
  for (const [api, user, admin] of [
    [
      "https://api.example.test:9443",
      "http://users.example.test:9082",
      "https://admin.example.test:9441",
    ],
    [
      "http://api.example.test:9080",
      "https://users.example.test:9442",
      "http://admin.example.test:9081",
    ],
  ]) {
    await page.getByLabel("API 基础地址", { exact: true }).fill(api);
    await page.getByLabel("用户端基础地址", { exact: true }).fill(user);
    await page.getByLabel("管理端基础地址", { exact: true }).fill(admin);
    const before = writes.length;
    await page.getByRole("button", { name: "保存", exact: true }).click();
    await expect.poll(() => writes.length).toBe(before + 1);
    expect(writes[before]).toEqual({
      api_url: api,
      user_url: user,
      admin_url: admin,
      revision: before,
    });
    await expect(page.getByRole("button", { name: "保存", exact: true })).toBeEnabled();
  }
  await page.screenshot({ path: info.outputPath("public-base-addresses.png"), fullPage: true });
  failRead = true;
  await page.getByRole("button", { name: "刷新", exact: true }).last().click();
  await expect(page.getByText("暂时无法读取访问地址", { exact: true })).toBeVisible();
  await expect(page.getByLabel("API 基础地址", { exact: true })).toHaveValue(settings.api_url);
  await expect(page.getByRole("button", { name: "保存", exact: true })).toBeDisabled();
  failRead = false;
  await page.getByRole("button", { name: "刷新", exact: true }).last().click();
  await expect(page.getByRole("button", { name: "保存", exact: true })).toBeEnabled();
  await page
    .getByLabel("API 基础地址", { exact: true })
    .fill("https://api.example.test/api/oauth/grok");
  await page.getByRole("button", { name: "保存", exact: true }).click();
  await expect(
    page.getByText("API 基础地址仅填写协议、域名和可选端口，不含路径或参数", { exact: true }),
  ).toBeVisible();
  expect(writes).toHaveLength(2);
});
