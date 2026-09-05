import { defineConfig } from "@playwright/test";

// Svelte 前端在普通浏览器中由 Playwright 驱动（IPC 走测试注入的 mock）。
// 浏览器环境固定为浅色主题 + 禁用动画，保证与 Go 版基准截图可比。
export default defineConfig({
  testDir: "./tests/e2e",
  timeout: 30_000,
  retries: 0,
  use: {
    baseURL: "http://localhost:5173",
    viewport: { width: 1280, height: 800 },
    deviceScaleFactor: 1,
    colorScheme: "light",
    reducedMotion: "reduce",
  },
  webServer: {
    command: "npm run dev",
    port: 5173,
    reuseExistingServer: true,
    timeout: 60_000,
  },
});
