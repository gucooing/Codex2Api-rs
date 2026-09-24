import { defineConfig } from "@playwright/test";

const baseURL = process.env.CODEX2API_TEST_BASE_URL;
if (!baseURL || !["127.0.0.1", "localhost", "[::1]"].includes(new URL(baseURL).hostname)) {
  throw new Error(
    "Set CODEX2API_TEST_BASE_URL to the isolated local test service. Production addresses are forbidden.",
  );
}

export default defineConfig({
  testDir: "./tests/browser",
  workers: 1,
  fullyParallel: false,
  timeout: 90000,
  expect: { timeout: 10000 },
  reporter: "list",
  outputDir: `${process.env.CODEX2API_TEST_OUTPUT_DIR ?? "../target/review-next-ui-redesign-20260923"}/playwright-results`,
  use: {
    baseURL,
    browserName: "chromium",
    headless: true,
    viewport: { width: 1440, height: 1100 },
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
});
