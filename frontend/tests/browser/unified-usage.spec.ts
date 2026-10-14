import { expect, test } from "@playwright/test";
import { createServer, type Server } from "node:http";
import { readFile, stat } from "node:fs/promises";
import { extname, resolve, sep } from "node:path";

let server: Server;
let origin: string;
test.beforeAll(async () => {
  const root = resolve(process.cwd(), "../frontend-user/out");
  server = createServer(async (req, res) => {
    const pathname = new URL(req.url ?? "/", "http://localhost").pathname;
    let path = resolve(root, "." + decodeURIComponent(pathname.replace(/^\/user/, "")));
    if (!path.startsWith(root + sep) && path !== root) {
      res.writeHead(404).end();
      return;
    }
    try {
      if ((await stat(path)).isDirectory()) path = resolve(path, "index.html");
      const types: Record<string, string> = {
        ".html": "text/html",
        ".js": "application/javascript",
        ".css": "text/css",
        ".json": "application/json",
        ".svg": "image/svg+xml",
        ".woff2": "font/woff2",
      };
      res.setHeader("Content-Type", types[extname(path)] ?? "text/plain");
      res.end(await readFile(path));
    } catch {
      res.writeHead(404).end();
    }
  });
  await new Promise<void>((done) => server.listen(0, "127.0.0.1", done));
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("Missing local address");
  origin = `http://127.0.0.1:${address.port}`;
});
test.afterAll(async () => {
  await new Promise<void>((done) => server.close(() => done()));
});

test("unified user usage matches the admin table on desktop and phone", async ({ page }, info) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  const record = {
    id: "unified-request",
    consumer_name: "Primary key",
    subject_kind: "api_key",
    key_id: "key",
    source_name: "Pro 200",
    provider_id: "chatgpt",
    endpoint: "/v1/responses",
    transport: "http",
    stream: true,
    model: "gpt-6-astra",
    actual_model: null,
    requested_at_ms: Date.now(),
    status: "failed",
    http_status: 200,
    failure_kind: "transport",
    failure_status: 502,
    input_tokens: 1000,
    output_tokens: 200,
    cached_tokens: 300,
    cache_write_tokens: 0,
    reasoning_tokens: 50,
    reasoning_effort: "high",
    service_tier: "default",
    total_ms: 12813,
    first_byte_ms: 35,
    cost_nano_usd: 3_000_000,
    billing_calculation: {
      terms: [
        { kind: "cache_read", quantity: 300, unit_price_usd: "0" },
        { kind: "output", quantity: 200, unit_price_usd: "15" },
        { kind: "input", quantity: 700, unit_price_usd: "10" },
      ],
      multiplier_micros: 300_000,
      total_nano_usd: 3_000_000,
    },
    billing_status: "priced",
    billing_tier: "standard",
    multiplier_micros: 300_000,
    wallet_debit_cents: 0,
    image_count: null,
    image_size: null,
    image_usage_json: null,
    image_input_usage_json: null,
  };
  await page.route("**/user/api/**", async (route) => {
    const path = new URL(route.request().url()).pathname;
    const body = path.endsWith("/session")
      ? {
          csrf_token: "fixture",
          user: {
            id: "user",
            username: "user",
            name: "User",
            email: "",
            enabled: true,
            wallet_balance_usd: "10",
            revision: 1,
            created_at: "now",
          },
        }
      : { items: [record], total: 1, page: 1, limit: 20, wallet_pending_nano_usd: 3_000_000 };
    await route.fulfill({ json: body });
  });
  await page.goto(`${origin}/user/usage/`);
  await expect(page.getByRole("columnheader", { name: "模型 / 接口", exact: true })).toBeVisible();
  await expect(page.getByRole("columnheader", { name: "供应账户", exact: true })).toHaveCount(0);
  await expect(page.getByRole("link", { name: "API 用量", exact: true })).toHaveCount(0);
  await expect(page.getByRole("table")).toContainText("流式");
  await expect(page.getByRole("table")).toContainText("1K");
  await expect(page.getByRole("table")).toContainText("$0.003");
  await page.getByRole("button", { name: "查看请求详情：502", exact: true }).click();
  const detail = page.getByRole("dialog");
  for (const label of ["HTTP 状态（连接）", "生成错误状态", "计费状态", "命中计费挡位", "成本"]) {
    await expect(detail.getByText(label, { exact: true })).toHaveCount(0);
  }
  await expect(detail.getByText("502", { exact: true })).toHaveClass(/text-destructive/);
  await expect(detail.getByText("失败", { exact: true })).toHaveCount(0);
  await expect(detail).toContainText("传输中断");
  await expect(detail).toContainText("Api:Pro 200");
  await expect(detail.getByText("API 组", { exact: true })).toHaveCount(0);
  await expect(detail.getByText("钱包扣款", { exact: true })).toHaveCount(0);
  await expect(detail.getByText("消费", { exact: true })).toBeVisible();
  await page.screenshot({ path: info.outputPath("usage-desktop-detail.png"), fullPage: true });
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "查看费用公式：$0.003", exact: true }).click();
  await expect(page.getByRole("dialog", { name: "费用计算公式", exact: true })).toContainText(
    "(缓读:300*0 + 输出:200*15 + 输入:700*10)*0.3x = $0.003",
  );
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "显示列", exact: true }).click();
  await expect(page.getByRole("menuitemcheckbox", { name: "消费账户", exact: true })).toBeVisible();
  await page.keyboard.press("Escape");
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(page.getByRole("columnheader")).toHaveCount(5);
  await expect(page.getByRole("table")).toContainText("$0.003");
  await page.screenshot({ path: info.outputPath("usage-phone.png"), fullPage: true });
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth > window.innerWidth,
  );
  expect(overflow).toBe(false);
  await page.getByRole("button", { name: "查看详情：gpt-6-astra", exact: true }).click();
  await expect(detail).toContainText("传输中断");
  expect(errors).toEqual([]);
});
