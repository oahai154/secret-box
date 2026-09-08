// 基线重截工具：v2 起应用 UI 有意偏离 Go 版（ADR-0003），基线改为记录当前应用
// 自身的状态。复用 injectMockIpc 保证与 visual.spec 完全相同的页面环境。
// 用法：RECAPTURE=1 npx playwright test recapture.spec.ts
//      （dev server 需运行在 5173，或让 playwright webServer 自行启动）
import { expect, test } from "@playwright/test";
import * as fs from "fs";
import * as path from "path";
import { injectMockIpc, login } from "./helpers";

const BASELINES = path.join(import.meta.dirname, "baselines");
const STEPS: Record<string, (page: import("@playwright/test").Page) => Promise<void>> = {
  "auth.png": async (page) => {
    await expect(page.locator(".auth-card")).toBeVisible();
    await expect(page.locator("#authPassword")).toBeFocused();
  },
  "settings.png": async (page) => {
    await login(page, "golden-test-password");
    await page.locator("#itemList .item").first().waitFor();
    await page.waitForTimeout(2500);
    await page.click("#itemList .item:nth-child(1)");
    await page.evaluate(() => {
      window.scrollTo(0, 0);
      document.querySelectorAll("*").forEach((el) => {
        if (el.scrollTop) el.scrollTop = 0;
        if (el.scrollLeft) el.scrollLeft = 0;
      });
    });
    await page.waitForTimeout(300);
    await page.click("#settingsBtn");
    await page.locator(".card-settings").waitFor();
    await page.waitForTimeout(300);
  },
};

for (const [name, prepare] of Object.entries(STEPS)) {
  test(`重截基线 ${name}`, async ({ page }) => {
    test.skip(!process.env.RECAPTURE, "仅 RECAPTURE=1 时运行");
    injectMockIpc(page);
    await page.goto("/");
    await prepare(page);
    const shot = await page.screenshot();
    fs.writeFileSync(path.join(BASELINES, name), shot);
    console.log(`基准已更新: ${path.join(BASELINES, name)} (${shot.length} bytes)`);
  });
}
