import { test, expect, type Page } from "@playwright/test";
import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";

const evidence = resolve(
  process.env.CODEX2API_TEST_OUTPUT_DIR ?? "../target/review-next-ui-redesign-20260923",
  "screenshots",
);

async function login(page: Page) {
  await page.goto("/admin/");
  await page.getByLabel("用户名", { exact: true }).fill("admin");
  await page.getByLabel("密码", { exact: true }).fill("admin");
  await page.getByRole("button", { name: "登录", exact: true }).click();
  await expect(
    page.locator("[data-slot=breadcrumb-page]").filter({ hasText: "概览" }),
  ).toBeVisible();
  await page.getByRole("link", { name: "虚拟账户", exact: true }).click();
  await expect(page.getByRole("link", { name: "本地验收账户", exact: true })).toBeVisible();
}

async function choose(page: Page, label: string) {
  await page.getByRole("button", { name: "主题设置", exact: true }).click();
  await page.getByRole("menuitemradio", { name: label, exact: true }).click();
  await expect(page.getByRole("menu")).not.toBeVisible();
}

test("appearance persists only in its browser and follows system changes without backend writes", async ({
  browser,
  baseURL,
}) => {
  await mkdir(evidence, { recursive: true });
  const first = await browser.newContext({
    baseURL,
    colorScheme: "light",
    viewport: { width: 1440, height: 1000 },
  });
  const second = await browser.newContext({
    baseURL,
    colorScheme: "light",
    viewport: { width: 1440, height: 1000 },
  });
  const page = await first.newPage();
  const other = await second.newPage();
  const errors: string[] = [];
  const mutations: string[] = [];
  for (const item of [page, other]) {
    item.on("pageerror", (error) => errors.push(error.message));
    item.on("console", (message) => {
      if (
        message.type() === "error" &&
        /hydration|server rendered HTML|didn't match/i.test(message.text())
      )
        errors.push(message.text());
    });
  }
  try {
    await login(page);
    await login(other);
    for (const item of [page, other])
      item.on("request", (request) => {
        if (["POST", "PUT", "PATCH", "DELETE"].includes(request.method()))
          mutations.push(`${request.method()} ${new URL(request.url()).pathname}`);
      });
    await expect(page.locator("html")).toHaveClass(/light/);
    await page.evaluate(() => {
      localStorage.setItem("codex2api-accent", "blue");
      document.documentElement.dataset.accent = "blue";
    });
    await page.reload();
    await expect(page.getByRole("button", { name: "主题设置", exact: true })).toBeEnabled();
    await expect(page.locator("html")).not.toHaveAttribute("data-accent");
    expect(await page.evaluate(() => localStorage.getItem("codex2api-accent"))).toBeNull();
    const expectPrimary = async (target: Page, color: string) => {
      const colors = await target.evaluate((expected) => {
        const context = document.createElement("canvas").getContext("2d")!;
        const pixel = (color: string) => {
          context.fillStyle = color;
          context.fillRect(0, 0, 1, 1);
          return Array.from(context.getImageData(0, 0, 1, 1).data);
        };
        return {
          actual: pixel(
            getComputedStyle(document.documentElement).getPropertyValue("--primary").trim(),
          ),
          expected: pixel(expected),
        };
      }, color);
      expect(colors.actual).toEqual(colors.expected);
    };
    await expectPrimary(page, "oklch(0.205 0 0)");
    await page.getByRole("button", { name: "主题设置", exact: true }).click();
    await expect(page.getByRole("menuitemradio")).toHaveText(["浅色", "深色", "跟随系统"]);
    await expect(page.getByRole("menu")).not.toContainText("强调色");
    await page.keyboard.press("Escape");
    await choose(page, "深色");
    await expect(page.locator("html")).toHaveClass(/dark/);
    await expectPrimary(page, "oklch(0.922 0 0)");
    await page.reload();
    await expect(page.getByRole("button", { name: "主题设置", exact: true })).toBeEnabled();
    await expect(page.locator("html")).toHaveClass(/dark/);
    await expect(page.locator("html")).not.toHaveAttribute("data-accent");
    expect(await page.evaluate(() => localStorage.getItem("codex2api-theme"))).toBe("dark");
    await page.screenshot({
      path: resolve(evidence, "19-theme-dark-neutral.png"),
      fullPage: true,
      animations: "disabled",
    });
    await page.getByRole("button", { name: "主题设置", exact: true }).click();
    await expect(page.getByRole("menuitemradio", { name: "深色", exact: true })).toBeChecked();
    await page.screenshot({
      path: resolve(evidence, "20-theme-menu-dark.png"),
      fullPage: true,
      animations: "disabled",
    });
    await page.keyboard.press("Escape");
    await expect(other.locator("html")).toHaveClass(/light/);
    await expectPrimary(other, "oklch(0.205 0 0)");
    await other.screenshot({
      path: resolve(evidence, "21-theme-light-neutral.png"),
      fullPage: true,
      animations: "disabled",
    });
    await choose(page, "跟随系统");
    await page.emulateMedia({ colorScheme: "dark" });
    await expect(page.locator("html")).toHaveClass(/dark/);
    await page.emulateMedia({ colorScheme: "light" });
    await expect(page.locator("html")).toHaveClass(/light/);
    expect(await page.evaluate(() => localStorage.getItem("codex2api-theme"))).toBe("system");
    await other.evaluate(() => localStorage.clear());
    await other.reload();
    await expect(other.locator("html")).toHaveClass(/light/);
    await expect(other.locator("html")).not.toHaveAttribute("data-accent");
    expect(mutations).toEqual([]);
    expect(errors).toEqual([]);
  } finally {
    await first.close();
    await second.close();
  }
});
