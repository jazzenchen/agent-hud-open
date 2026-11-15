import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./src/apps/desktop/test",
  testMatch: "**/*.spec.ts",
  fullyParallel: true,
  workers: 3,
  timeout: 15_000,
  use: {
    baseURL: "http://127.0.0.1:4176",
    channel: process.env.PLAYWRIGHT_CHANNEL ?? "chrome",
    locale: "en-US",
    viewport: { width: 1000, height: 700 },
    trace: "retain-on-failure",
  },
  webServer: {
    // Preview serves the shipped assets without starting the development
    // bridge, which would otherwise read the user's local agent activity.
    command:
      "pnpm --filter @agent-hud/desktop run build && pnpm --filter @agent-hud/desktop run preview --port 4176 --strictPort",
    url: "http://127.0.0.1:4176",
    reuseExistingServer: false,
  },
});
