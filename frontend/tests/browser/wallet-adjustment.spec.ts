import { test, expect } from "@playwright/test";

test("wallet deltas use the current balance while the adjustment dialog stays open", async ({
  page,
  context,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (["error", "warning"].includes(message.type())) errors.push(message.text());
  });
  const login = await context.request.post("/admin/api/login", {
    data: { username: "admin", password: "admin" },
  });
  expect(login.status()).toBe(200);
  const { csrf_token } = await login.json();
  const username = `wallet-delta-${Date.now()}`;
  const created = await context.request.post("/admin/api/users", {
    headers: { "x-csrf-token": csrf_token },
    data: {
      username,
      name: username,
      email: `${username}@example.test`,
      password: "isolated-wallet-test",
      enabled: true,
      revision: null,
    },
  });
  expect(created.status()).toBe(200);
  const user = await created.json();
  const path = `/admin/api/users/${user.id}/wallet-adjustments`;
  await page.goto("/admin/users/");
  await page.getByLabel("搜索用户", { exact: true }).fill(username);
  await page.getByRole("button", { name: "查询", exact: true }).click();
  const row = page.getByRole("row").filter({
    has: page.getByRole("cell", { name: username, exact: true }).first(),
  });
  const dialog = page.getByRole("dialog");
  for (const [direction, amount, before, after] of [
    ["增加", "2", 1000, 1200],
    ["减少", "15", 2200, 700],
  ] as const) {
    await row.getByRole("button", { name: "调整余额", exact: true }).click();
    const intervening = await context.request.post(path, {
      headers: { "x-csrf-token": csrf_token },
      data: {
        request_id: `${username}-${direction}`,
        amount_cents: 1000,
        expected_revision: user.revision,
      },
    });
    expect(intervening.status()).toBe(200);
    if (direction === "减少") {
      await dialog.getByRole("combobox", { name: "操作", exact: true }).click();
      await page.getByRole("option", { name: "减少余额", exact: true }).click();
    }
    await dialog.getByLabel("金额（USD）", { exact: true }).fill(amount);
    const saved = page.waitForResponse(
      (response) =>
        new URL(response.url()).pathname === path && response.request().method() === "POST",
    );
    await dialog.getByRole("button", { name: `确认${direction}`, exact: true }).click();
    const response = await saved;
    expect(response.status()).toBe(200);
    const { entry } = await response.json();
    expect(entry.balance_before_cents).toBe(before);
    expect(entry.balance_cents).toBe(after);
    await expect(dialog).toBeHidden();
    await expect(row.getByRole("button", { name: "调整余额", exact: true })).toBeEnabled();
    await expect(row).toContainText(direction === "增加" ? "$12" : "$7");
  }
  const detail = await (await context.request.get(`/admin/api/users/${user.id}`)).json();
  expect(detail.user.wallet_balance_usd).toBe("7");
  expect(detail.user.revision).toBe(user.revision);
  const entries = await (
    await context.request.get(`/admin/api/users/${user.id}/wallet-entries`)
  ).json();
  expect(entries.total).toBe(4);
  expect(
    entries.items.reduce(
      (sum: number, entry: { amount_cents: number }) => sum + entry.amount_cents,
      0,
    ),
  ).toBe(700);
  expect(errors).toEqual([]);
});
