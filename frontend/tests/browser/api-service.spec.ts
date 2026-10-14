import { expect, test } from "@playwright/test";

test("API policy, precise model prices and user-owned keys work across independent listeners", async ({
  page,
  context,
  browser,
}, info) => {
  test.setTimeout(150000);
  const userOrigin = process.env.CODEX2API_TEST_USER_BASE_URL;
  if (!userOrigin || !["127.0.0.1", "localhost", "[::1]"].includes(new URL(userOrigin).hostname))
    throw new Error("CODEX2API_TEST_USER_BASE_URL must identify the isolated local user listener");
  const errors: string[] = [];
  const recordErrors = (target: typeof page) => {
    target.on("pageerror", (error) => errors.push(error.message));
    target.on("console", (message) => {
      if (
        ["error", "warning"].includes(message.type()) &&
        !/status of (401|503)/.test(message.text())
      )
        errors.push(message.text());
    });
  };
  recordErrors(page);
  const suffix = Date.now().toString(36);
  const groupName = `API 验收 ${suffix}`;
  const tagName = `API 标签 ${suffix}`;
  const textModel = `api-review-text-${suffix}`;
  const imageModel = `api-review-image-${suffix}`;
  const username = `api-review-${suffix}`;
  const password = "local-api-review-password";
  await page.goto("/admin/api-groups/");
  await page.getByLabel("用户名", { exact: true }).fill("admin");
  await page.getByLabel("密码", { exact: true }).fill("admin");
  await page.getByRole("button", { name: "登录", exact: true }).click();
  await expect(page.getByRole("button", { name: "添加 API 组", exact: true })).toBeEnabled();
  const session = await (await context.request.get("/admin/api/session")).json();
  const headers = { "X-CSRF-Token": session.csrf_token };
  const tagResponse = await context.request.post("/admin/api/supplier-tags", {
    headers,
    data: { name: tagName, provider_id: "chatgpt" },
  });
  expect(tagResponse.ok()).toBe(true);
  const tags = await (await context.request.get("/admin/api/supplier-tags/options")).json();
  const tag = tags.items.find((item: { name: string }) => item.name === tagName);
  for (const model of [
    {
      model: textModel,
      kind: "text",
      token_prices: [
        {
          tier: "standard",
          min_input_tokens: 0,
          max_input_tokens: null,
          input_rate: "2",
          cached_rate: "0.25",
          cache_write_rate: "2",
          output_rate: "3",
        },
      ],
      image_prices: [],
    },
    {
      model: imageModel,
      kind: "image",
      token_prices: [],
      image_prices: [{ resolution: "1K", price: "0.123456789" }],
    },
  ]) {
    const result = await context.request.post("/admin/api/models", {
      headers,
      data: { ...model, provider_id: "chatgpt", enabled: true, revision: null },
    });
    expect(result.ok(), await result.text()).toBe(true);
  }
  // The explicitly seeded review supplier is a local fixture; this never calls upstream.
  const supplier = await context.request.get("/admin/api/suppliers/review-supplier-active");
  expect(supplier.ok(), "Seed the disposable database with review_fixture first").toBe(true);
  expect(
    (
      await context.request.post("/admin/api/suppliers/tags", {
        headers,
        data: { account_ids: ["review-supplier-active"], tag_ids: [tag.id] },
      })
    ).ok(),
  ).toBe(true);
  for (const model of [textModel, imageModel])
    expect(
      (
        await context.request.post("/admin/api/suppliers/review-supplier-active/models", {
          headers,
          data: { model },
        })
      ).ok(),
    ).toBe(true);
  expect(
    (
      await context.request.post("/admin/api/users", {
        headers,
        data: {
          username,
          name: "API 验收用户",
          email: `${suffix}@example.test`,
          password,
          enabled: true,
          revision: null,
        },
      })
    ).ok(),
  ).toBe(true);

  await page.getByRole("button", { name: "添加 API 组", exact: true }).click();
  const editor = page.getByRole("dialog");
  await editor.getByLabel("名称", { exact: true }).fill(groupName);
  await editor.getByLabel("介绍", { exact: true }).fill("本地 API 模型与价格验收");
  await editor.getByLabel(tagName, { exact: true }).check();
  await editor.getByLabel("组计费倍率", { exact: true }).fill("2");
  await editor.getByLabel("每用户组并发", { exact: true }).fill("5");
  await editor.getByLabel("每用户组 RPM", { exact: true }).fill("20");
  await editor.getByRole("tab", { name: "模型与规则", exact: true }).click();
  await editor.getByLabel("搜索模型", { exact: true }).fill(suffix);
  await editor.getByLabel(textModel + " 倍率", { exact: true }).fill("1.25");
  await editor.getByLabel(textModel + " 并发", { exact: true }).fill("3");
  await editor.getByLabel(textModel + " RPM", { exact: true }).fill("7");
  await editor.getByRole("button", { name: "保存", exact: true }).click();
  await expect(editor).toHaveCount(0);
  const groups = await (
    await context.request.get(`/admin/api/api-groups?search=${encodeURIComponent(groupName)}`)
  ).json();
  const group = groups.items[0];
  expect(group.allowed_models).toEqual([]);
  expect(group.multiplier).toBe("2");
  expect(group.model_rules).toEqual([
    { model: textModel, multiplier: "1.25", concurrency_limit: 3, rpm_limit: 7 },
  ]);
  await page.getByRole("button", { name: groupName, exact: true }).click();
  await page.screenshot({
    path: info.outputPath("api-group-settings.png"),
    fullPage: true,
    animations: "disabled",
  });
  await editor.getByRole("tab", { name: "模型与规则", exact: true }).click();
  await editor.getByLabel("搜索模型", { exact: true }).fill(suffix);
  await expect(editor.getByLabel(textModel + " 倍率", { exact: true })).toHaveValue("1.25");
  await page.screenshot({
    path: info.outputPath("api-group-model-rules.png"),
    fullPage: true,
    animations: "disabled",
  });
  await editor.getByRole("button", { name: "取消", exact: true }).click();
  await page.screenshot({
    path: info.outputPath("api-groups.png"),
    fullPage: true,
    animations: "disabled",
  });

  const userContext = await browser.newContext({
    baseURL: userOrigin,
    viewport: { width: 1440, height: 1000 },
  });
  try {
    const userPage = await userContext.newPage();
    recordErrors(userPage);
    await userPage.goto("/user/model-market/");
    await userPage.getByLabel("用户名", { exact: true }).fill(username);
    await userPage.getByLabel("密码", { exact: true }).fill(password);
    await userPage.getByRole("button", { name: "登录", exact: true }).click();
    const userNavigation = userPage.getByRole("navigation", { name: "用户导航", exact: true });
    await expect(userNavigation.getByRole("link", { name: "模型广场", exact: true })).toBeVisible();
    await expect(userNavigation.getByRole("link", { name: "API 密钥", exact: true })).toBeVisible();
    await expect(userNavigation.getByRole("link", { name: "使用信息", exact: true })).toBeVisible();
    await expect(userPage.getByRole("tabpanel")).toBeVisible();
    await userPage.getByLabel("模型", { exact: true }).fill(suffix);
    await userPage.getByRole("button", { name: "查询", exact: true }).click();
    await expect(userPage.getByRole("row").filter({ hasText: textModel })).toContainText("$2 → $2");
    await userPage.getByRole("tab", { name: groupName, exact: true }).click();
    await expect(userPage.getByRole("row").filter({ hasText: textModel })).toContainText(
      "$2 → $2.5",
    );
    await expect(userPage.getByRole("row").filter({ hasText: imageModel })).toContainText(
      "$0.123456789 → $0.246913578",
    );
    await userPage
      .getByRole("row")
      .filter({ hasText: textModel })
      .getByRole("button", { name: "查看", exact: true })
      .click();
    await expect(
      userPage.getByRole("dialog").getByRole("row").filter({ hasText: "缓存读取" }),
    ).toContainText("$0.3125");
    await expect(
      userPage.getByRole("dialog").getByRole("row").filter({ hasText: "API 组内所有模型" }),
    ).toContainText("5");
    await expect(
      userPage.getByRole("dialog").getByRole("row").filter({ hasText: "当前模型" }),
    ).toContainText("7");
    await userPage.screenshot({
      path: info.outputPath("text-prices.png"),
      fullPage: true,
      animations: "disabled",
    });
    await userPage.getByRole("button", { name: "关闭", exact: true }).click();
    await expect(userPage.getByRole("dialog")).toHaveCount(0);
    await userPage.screenshot({
      path: info.outputPath("model-market.png"),
      fullPage: true,
      animations: "disabled",
    });
    await userPage.setViewportSize({ width: 390, height: 844 });
    await userPage.getByRole("tab", { name: "基础计费目录", exact: true }).click();
    await expect(userPage.getByRole("row").filter({ hasText: textModel })).toContainText("$2 → $2");
    await userPage.getByRole("tab", { name: groupName, exact: true }).click();
    await expect(userPage.getByText("每用户组并发 5", { exact: true })).toBeVisible();
    await expect(userPage.getByText("组 RPM 20", { exact: true })).toBeVisible();
    await userPage
      .getByRole("row")
      .filter({ hasText: imageModel })
      .getByRole("button", { name: "查看", exact: true })
      .click();
    await expect(userPage.getByRole("dialog")).toContainText("$0.246913578");
    await userPage.screenshot({
      path: info.outputPath("image-prices-mobile.png"),
      fullPage: true,
      animations: "disabled",
    });
    await userPage.getByRole("button", { name: "关闭", exact: true }).click();
    await userPage.setViewportSize({ width: 1440, height: 1000 });

    await userNavigation.getByRole("link", { name: "API 密钥", exact: true }).click();
    await userPage.getByRole("button", { name: "创建密钥", exact: true }).click();
    await userPage.getByRole("dialog").getByLabel("密钥名称", { exact: true }).fill("验收密钥");
    await userPage.getByRole("dialog").getByLabel("API 组", { exact: true }).click();
    await userPage.getByRole("option", { name: groupName + " · ChatGPT", exact: true }).click();
    const createdResponse = userPage.waitForResponse(
      (response) =>
        new URL(response.url()).pathname === "/user/api/api-keys" &&
        response.request().method() === "POST",
    );
    await userPage.getByRole("dialog").getByRole("button", { name: "创建", exact: true }).click();
    const created = await (await createdResponse).json();
    expect(typeof created.secret).toBe("string");
    await expect(
      userPage.getByRole("dialog").getByRole("heading", { name: "密钥已创建", exact: true }),
    ).toBeVisible();
    await userPage.getByRole("button", { name: "已保存，关闭", exact: true }).click();
    expect(await userPage.locator("body").textContent()).not.toContain(created.secret);
    const keyList = await (await userContext.request.get("/user/api/api-keys")).json();
    expect(JSON.stringify(keyList)).not.toContain(created.secret);
    expect(JSON.stringify(keyList)).not.toContain("tag_ids");
    expect(keyList.items[0].active_requests).toBe(0);
    const preferences = await userPage.evaluate(() => JSON.stringify(localStorage));
    expect(preferences).not.toContain(created.secret);

    await userPage.getByRole("button", { name: "操作 验收密钥", exact: true }).click();
    await userPage.getByRole("menuitem", { name: "编辑", exact: true }).click();
    await userPage.getByRole("dialog").getByLabel("密钥名称", { exact: true }).fill("更新后的密钥");
    await userPage.getByRole("dialog").getByRole("button", { name: "保存", exact: true }).click();
    await expect(userPage.getByRole("dialog")).toHaveCount(0);
    await expect(userPage.getByRole("table")).toContainText("更新后的密钥");
    await userPage.route("**/user/api/api-keys?**", async (route) =>
      route.fulfill({
        status: 503,
        json: { error: { code: "fixture_read_failure", message: "暂时无法读取密钥" } },
      }),
    );
    await userPage.getByRole("button", { name: "刷新密钥", exact: true }).click();
    await expect(userPage.getByText("暂时无法读取密钥", { exact: true })).toBeVisible();
    await expect(userPage.getByRole("table")).toContainText("更新后的密钥");
    await expect(userPage.getByRole("button", { name: "创建密钥", exact: true })).toBeDisabled();
    await userPage.unroute("**/user/api/api-keys?**");
    await userPage.getByRole("button", { name: "刷新密钥", exact: true }).click();
    await expect(userPage.getByRole("button", { name: "创建密钥", exact: true })).toBeEnabled();
    await userPage.getByRole("button", { name: "操作 更新后的密钥", exact: true }).click();
    await userPage.getByRole("menuitem", { name: "撤销", exact: true }).click();
    await userPage.getByRole("button", { name: "确认撤销", exact: true }).click();
    await expect(userPage.getByRole("table")).toContainText("已撤销");
    await expect(userPage.getByRole("dialog")).toHaveCount(0);
    await userPage.screenshot({
      path: info.outputPath("user-api-keys.png"),
      fullPage: true,
      animations: "disabled",
    });
    await userNavigation.getByRole("link", { name: "使用信息", exact: true }).click();
    await expect(userPage.getByRole("table")).toBeVisible();
    await expect(
      userPage.getByRole("columnheader", { name: "模型 / 接口", exact: true }),
    ).toBeVisible();
  } finally {
    await userContext.close();
  }

  await page.goto("/admin/api-keys/");
  await page.getByLabel("用户 / 密钥", { exact: true }).fill(username);
  await page.getByRole("button", { name: "查询", exact: true }).click();
  await expect(page.getByRole("table")).toContainText("更新后的密钥");
  await expect(page.getByRole("columnheader", { name: "当前并发", exact: true })).toBeVisible();
  await page.goto("/admin/usage/");
  await expect(page.getByRole("table")).toBeVisible();
  expect(errors).toEqual([]);
});
