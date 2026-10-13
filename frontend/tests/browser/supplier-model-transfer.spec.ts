import { expect, test } from "@playwright/test";
import { mkdir, readFile } from "node:fs/promises";
import { resolve } from "node:path";

test("supplier model selection, batch replacement and JSON proxy roundtrip use the real service", async ({
  page,
  context,
}) => {
  test.setTimeout(180000);
  page.setDefaultTimeout(15000);
  const suffix = Date.now().toString(36);
  const errors: string[] = [];
  const upstreamReads: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (["error", "warning"].includes(message.type()) && !/status of 401/.test(message.text()))
      errors.push(message.text());
  });
  page.on("request", (request) => {
    if (/\/suppliers\/.*\/(quota|models\/sync)(\?|$)/.test(request.url()))
      upstreamReads.push(request.url());
  });
  await page.goto("/admin/suppliers/");
  await page.getByLabel("用户名", { exact: true }).fill("admin");
  await page.getByLabel("密码", { exact: true }).fill("admin");
  await page.getByRole("button", { name: "登录", exact: true }).click();
  await expect(page.getByRole("button", { name: "添加供应账户", exact: true })).toBeEnabled();
  const session = await (await context.request.get("/admin/api/session")).json();
  const headers = { "X-CSRF-Token": session.csrf_token };
  const model = `transfer-model-${suffix}`;
  expect(
    (
      await context.request.post("/admin/api/models", {
        headers,
        data: {
          provider_id: "chatgpt",
          model,
          kind: "text",
          enabled: false,
          revision: null,
          token_prices: [
            {
              tier: "standard",
              min_input_tokens: 0,
              input_rate: "1",
              cached_rate: "0.5",
              cache_write_rate: "1",
              output_rate: "2",
            },
          ],
          image_prices: [],
        },
      })
    ).ok(),
  ).toBe(true);
  const names = [`transfer-a-${suffix}`, `transfer-b-${suffix}`, `transfer-g-${suffix}`];
  const data = {
    accounts: [
      ...names.map((name, index) => ({
        platform: index === 2 ? "grok" : "openai",
        type: "oauth",
        name,
        concurrency: 3,
        priority: 1,
        proxy_key: "p",
        credentials:
          index === 2
            ? {
                access_token: "local-test-grok-access",
                refresh_token: "local-test-grok-refresh",
                sub: name,
                expires_at: 2000000000,
                email: `${name}@example.test`,
              }
            : {
                access_token: "local-test-openai-access",
                refresh_token: "local-test-openai-refresh",
                chatgpt_user_id: name,
                chatgpt_account_id: "local-test-workspace",
                email: `${name}@example.test`,
                model_mapping: { old: "old-model" },
              },
      })),
      {
        platform: "unsupported",
        type: "oauth",
        name: "skip",
        concurrency: 0,
        priority: 0,
        proxy_key: "unused",
        credentials: {},
      },
    ],
    proxies: [
      {
        proxy_key: "p",
        name: `proxy-${suffix}`,
        protocol: "http",
        host: "127.0.0.1",
        port: 1099,
        username: `proxy-user-${suffix}`,
        password: "local-test-proxy-password",
      },
    ],
  };
  await page.getByRole("button", { name: "导入", exact: true }).click();
  let dialog = page.getByRole("dialog", { name: "导入供应账户" });
  await dialog.getByLabel("导入格式").click();
  await page.getByRole("option", { name: "Sub2api 格式", exact: true }).click();
  await dialog.getByLabel("JSON 文件").setInputFiles({
    name: "sub2api.json",
    mimeType: "application/json",
    buffer: Buffer.from(JSON.stringify(data)),
  });
  await dialog.getByLabel("导入代理", { exact: true }).check();
  await dialog.getByRole("button", { name: "导入", exact: true }).click();
  await expect(dialog.getByText("导入 3 个，跳过 1 个，失败 0 个", { exact: true })).toBeVisible();
  await expect(dialog.getByRole("columnheader", { name: "名称", exact: true })).toBeVisible();
  await expect(dialog.getByRole("columnheader", { name: "序号", exact: true })).toHaveCount(0);
  for (const name of names)
    await expect(
      dialog.getByRole("cell", { name: `${name}@example.test`, exact: true }),
    ).toBeVisible();
  await dialog.getByRole("button", { name: "关闭", exact: true }).first().click();
  await page.getByLabel("搜索账户", { exact: true }).fill(suffix);
  await page.getByRole("button", { name: "查询", exact: true }).click();
  const suppliers = (
    await (await context.request.get(`/admin/api/suppliers?search=transfer-&page_size=50`)).json()
  ).items.filter((row: { display_name: string }) => names.includes(row.display_name));
  const first = suppliers.find((row: { display_name: string }) => row.display_name === names[0]);
  const ids = suppliers.map((row: { id: string }) => row.id);
  const select = (index: number) =>
    page.getByRole("checkbox", { name: `选择供应账户 ${names[index]}@example.test`, exact: true });
  await select(0).check();
  await expect(page.getByRole("button", { name: "更新模型", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "更新模型", exact: true }).click();
  dialog = page.getByRole("dialog", { name: "更新模型", exact: true });
  await dialog.getByLabel("可用模型", { exact: true }).fill(`single-${suffix}`);
  await page.getByRole("option", { name: `single-${suffix}`, exact: true }).click();
  await dialog.getByRole("button", { name: "覆盖更新", exact: true }).click();
  await expect(dialog).not.toBeVisible();
  const singleModels = await (
    await context.request.get(`/admin/api/suppliers/${first.id}/models`)
  ).json();
  expect(singleModels.items.map((item: { model: string }) => item.model)).toEqual([
    `single-${suffix}`,
  ]);
  await select(1).check();
  await expect(page.getByRole("button", { name: "更新模型", exact: true })).toBeVisible();
  await select(2).check();
  await expect(page.getByRole("button", { name: "更新模型", exact: true })).toHaveCount(0);
  await select(2).uncheck();
  await page.getByRole("button", { name: "更新模型", exact: true }).click();
  dialog = page.getByRole("dialog", { name: "更新模型", exact: true });
  await dialog.getByLabel("可用模型", { exact: true }).fill(model);
  await page.getByRole("option", { name: model, exact: true }).click();
  await dialog.getByLabel("可用模型", { exact: true }).fill(`typed-${suffix}`);
  await page.getByRole("option", { name: `typed-${suffix}`, exact: true }).click();
  await dialog.getByRole("button", { name: "覆盖更新", exact: true }).click();
  await expect(dialog).not.toBeVisible();
  for (const row of suppliers.filter(
    (row: { provider_id: string }) => row.provider_id === "chatgpt",
  )) {
    const models = await (
      await context.request.get(`/admin/api/suppliers/${row.id}/models`)
    ).json();
    expect(models.items.map((item: { model: string }) => item.model).sort()).toEqual(
      [model, `typed-${suffix}`].sort(),
    );
  }
  await page.goto(`/admin/suppliers/detail/?id=${first.id}`);
  await page.getByRole("tab", { name: "可用模型", exact: true }).click();
  await expect(page.getByRole("button", { name: "添加", exact: true })).toHaveCount(0);
  const picker = page.getByLabel("添加自定义模型", { exact: true });
  await picker.fill(`detail-${suffix}`);
  const custom = page.getByRole("option", { name: `detail-${suffix}`, exact: true });
  await expect(custom).toBeEnabled();
  await custom.click();
  await expect(page.getByRole("cell", { name: `detail-${suffix}`, exact: true })).toBeVisible();
  await picker.fill(`detail-${suffix}`);
  await expect(custom).toHaveAttribute("aria-selected", "true");
  await expect(custom).toBeEnabled();
  await picker.fill(model);
  const existing = page.getByRole("option", { name: model, exact: true });
  await expect(existing).toHaveAttribute("aria-selected", "true");
  await expect(existing).toBeEnabled();
  await existing.click();
  await expect(page.getByRole("cell", { name: model, exact: true })).toHaveCount(0);
  await picker.press("Escape");
  await page.goto("/admin/suppliers/");
  for (let index = 0; index < 3; index++) await select(index).check();
  await page.getByRole("button", { name: "导出", exact: true }).click();
  dialog = page.getByRole("dialog", { name: "导出供应账户" });
  await dialog.getByLabel("导出代理", { exact: true }).check();
  const exportHeight = (await dialog.boundingBox())!.height;
  const downloaded = page.waitForEvent("download");
  await dialog.getByRole("button", { name: "导出 JSON", exact: true }).click();
  const download = await downloaded;
  await expect(dialog.getByText("已导出 3 个账户", { exact: true })).toBeVisible();
  await expect(dialog.getByRole("columnheader", { name: "名称", exact: true })).toBeVisible();
  await expect(dialog.getByRole("columnheader", { name: "结果", exact: true })).toBeVisible();
  await expect(dialog.getByRole("columnheader", { name: "结果", exact: true })).toHaveCSS(
    "text-align",
    "right",
  );
  await expect(dialog.getByRole("cell", { name: "已导出", exact: true }).first()).toHaveCSS(
    "text-align",
    "right",
  );
  for (const name of names)
    await expect(dialog.getByRole("row").filter({ hasText: `${name}@example.test` })).toContainText(
      "已导出",
    );
  expect(Math.abs((await dialog.boundingBox())!.height - exportHeight)).toBeLessThan(1);
  await page.setViewportSize({ width: 390, height: 480 });
  const exportBox = (await dialog.boundingBox())!;
  expect(exportBox.y).toBeGreaterThanOrEqual(0);
  expect(exportBox.y + exportBox.height).toBeLessThanOrEqual(480);
  await expect(dialog.getByRole("heading", { name: "导出供应账户" })).toBeInViewport();
  await expect(dialog.getByRole("button", { name: "导出 JSON", exact: true })).toBeInViewport();
  const exportViewport = dialog.locator("[data-slot=scroll-area-viewport]");
  expect(
    await exportViewport.evaluate((element) => element.scrollHeight > element.clientHeight),
  ).toBe(true);
  await exportViewport.hover();
  await page.mouse.wheel(0, 1000);
  await expect
    .poll(() => exportViewport.evaluate((element) => element.scrollTop))
    .toBeGreaterThan(0);
  await expect(dialog.getByRole("button", { name: "关闭", exact: true })).toBeInViewport();
  await dialog.getByRole("button", { name: "关闭", exact: true }).click();
  await page.setViewportSize({ width: 1440, height: 1100 });
  const exported = JSON.parse(await readFile((await download.path())!, "utf8"));
  expect(exported.accounts).toHaveLength(3);
  for (const row of exported.accounts) {
    expect(row.proxy.url).toContain("local-test-proxy-password");
    expect(row.proxy.name).toBe(`proxy-${suffix}`);
  }
  for (const id of ids)
    expect((await context.request.delete(`/admin/api/suppliers/${id}`, { headers })).ok()).toBe(
      true,
    );
  await page.reload();
  await page.getByRole("button", { name: "导入", exact: true }).click();
  dialog = page.getByRole("dialog", { name: "导入供应账户" });
  await dialog.getByLabel("JSON 文件").setInputFiles({
    name: "native.json",
    mimeType: "application/json",
    buffer: Buffer.from(JSON.stringify(exported)),
  });
  await dialog.getByLabel("导入代理", { exact: true }).check();
  await dialog.getByRole("button", { name: "导入", exact: true }).click();
  await expect(dialog.getByText("导入 3 个，跳过 0 个，失败 0 个", { exact: true })).toBeVisible();
  const restored = await (
    await context.request.post("/admin/api/suppliers/export", {
      headers,
      data: { account_ids: ids, include_proxies: true },
    })
  ).json();
  expect(restored.accounts).toEqual(
    exported.accounts.map((row: { tokens: { updated_at: string } }) => ({
      ...row,
      tokens: { ...row.tokens, updated_at: expect.any(String) },
    })),
  );
  await dialog.getByRole("button", { name: "导入", exact: true }).click();
  await expect(dialog.getByText("导入 0 个，跳过 3 个，失败 0 个", { exact: true })).toBeVisible();
  await dialog.getByRole("button", { name: "关闭", exact: true }).first().click();
  for (let index = 0; index < 3; index++) await select(index).check();
  await page.getByRole("button", { name: "导出", exact: true }).click();
  dialog = page.getByRole("dialog", { name: "导出供应账户" });
  await dialog.getByLabel("导出格式").click();
  await page.getByRole("option", { name: "Sub2api 格式", exact: true }).click();
  await dialog.getByLabel("导出代理", { exact: true }).check();
  const subDownload = page.waitForEvent("download");
  await dialog.getByRole("button", { name: "导出 JSON", exact: true }).click();
  const subFile = await subDownload;
  await expect(dialog.getByText("已导出 3 个账户", { exact: true })).toBeVisible();
  for (const name of names)
    await expect(dialog.getByRole("row").filter({ hasText: `${name}@example.test` })).toContainText(
      "已导出",
    );
  await dialog.getByRole("button", { name: "关闭", exact: true }).click();
  const subData = JSON.parse(await readFile((await subFile.path())!, "utf8"));
  expect(subData.type).toBe("sub2api-data");
  expect(subData.proxies).toHaveLength(1);
  expect(subData.proxies[0].password).toBe("local-test-proxy-password");
  for (const id of ids)
    expect((await context.request.delete(`/admin/api/suppliers/${id}`, { headers })).ok()).toBe(
      true,
    );
  await page.reload();
  await page.getByRole("button", { name: "导入", exact: true }).click();
  dialog = page.getByRole("dialog", { name: "导入供应账户" });
  await dialog.getByLabel("导入格式").click();
  await page.getByRole("option", { name: "Sub2api 格式", exact: true }).click();
  await dialog.getByLabel("JSON 文件").setInputFiles({
    name: "exported-sub2api.json",
    mimeType: "application/json",
    buffer: Buffer.from(JSON.stringify(subData)),
  });
  await dialog.getByLabel("导入代理", { exact: true }).check();
  await dialog.getByRole("button", { name: "导入", exact: true }).click();
  await expect(dialog.getByText("导入 3 个，跳过 0 个，失败 0 个", { exact: true })).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(dialog.getByRole("button", { name: "导入", exact: true })).toBeInViewport();
  const output = resolve(process.env.CODEX2API_TEST_OUTPUT_DIR!, "screenshots");
  await mkdir(output, { recursive: true });
  await page.screenshot({ path: resolve(output, "supplier-import-mobile.png"), fullPage: true });
  expect(upstreamReads).toEqual([]);
  expect(errors).toEqual([]);
});
