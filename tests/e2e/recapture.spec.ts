// 基线重截工具：v2 起应用 UI 有意偏离 Go 版（ADR-0003），基线改为记录当前应用
// 自身的全部页面状态（含主界面密度调整后的样子）。复用 injectMockIpc 保证与
// visual.spec 完全相同的页面环境。
// 用法：RECAPTURE=1 npx playwright test recapture.spec.ts
//      （dev server 需运行在 5173，或让 playwright webServer 自行启动）
import { expect, test } from "@playwright/test";
import * as fs from "fs";
import * as path from "path";
import { injectMockIpc, login } from "./helpers";

const BASELINES = path.join(import.meta.dirname, "baselines");

// 与 visual.spec 各用例一致：条目 1 选中 + 页面与各容器滚动复位
async function selectFirstItem(page: import("@playwright/test").Page) {
  await page.click("#itemList .item:nth-child(1)");
  await page.evaluate(() => {
    window.scrollTo(0, 0);
    document.querySelectorAll("*").forEach((el) => {
      if (el.scrollTop) el.scrollTop = 0;
      if (el.scrollLeft) el.scrollLeft = 0;
    });
  });
  await page.waitForTimeout(300);
}

const STEPS: Record<string, (page: import("@playwright/test").Page) => Promise<void>> = {
  "auth.png": async (page) => {
    // 首个用例承担 dev server 冷启动，断言给足超时（并行重截时曾因默认 5s 超时抖动）
    await expect(page.locator(".auth-card")).toBeVisible({ timeout: 20_000 });
    await expect(page.locator("#authPassword")).toBeFocused({ timeout: 20_000 });
  },
  "main.png": async (page) => {
    await login(page, "golden-test-password");
    await expect(page.locator("#itemList .item")).toHaveCount(3);
    await expect(page.locator("#itemTitle")).not.toHaveValue("");
    await page.waitForTimeout(2500); // 等 toast（已解锁）消失
  },
  "new.png": async (page) => {
    await login(page, "golden-test-password");
    await expect(page.locator("#itemList .item")).toHaveCount(3);
    await page.waitForTimeout(2500);
    await page.click("#newBtn");
    await expect(page.locator("#itemMeta")).toHaveText("新条目");
    await page.waitForTimeout(400); // 等进场动画结束
  },
  "value-visible.png": async (page) => {
    await login(page, "golden-test-password");
    await expect(page.locator("#itemList .item")).toHaveCount(3);
    await page.waitForTimeout(2500);
    await page.click("#toggleValueBtn");
    await page.waitForTimeout(300);
  },
  "history.png": async (page) => {
    await login(page, "golden-test-password");
    await expect(page.locator("#itemList .item")).toHaveCount(3);
    await page.waitForTimeout(2500);
    // 选中第二个条目（公司邮箱，含 3 个版本）并滚动到历史面板
    await page.click("#itemList .item:nth-child(2)");
    await expect(page.locator("#historyPanel")).toBeVisible();
    await page.locator("#historyPanel").scrollIntoViewIfNeeded();
    await page.waitForTimeout(300);
  },
  "settings.png": async (page) => {
    await login(page, "golden-test-password");
    await page.locator("#itemList .item").first().waitFor();
    await page.waitForTimeout(2500);
    await selectFirstItem(page);
    await page.click("#settingsBtn");
    await page.locator(".card-settings").waitFor();
    await page.waitForTimeout(300);
  },
  "change-password.png": async (page) => {
    await login(page, "golden-test-password");
    await expect(page.locator("#itemList .item")).toHaveCount(3);
    await page.waitForTimeout(2500);
    await selectFirstItem(page);
    await page.click("#settingsBtn");
    await expect(page.locator(".card-settings")).toBeVisible();
    await page.click("#changePasswordBtn");
    await expect(page.locator(".card-password")).toBeVisible();
    await page.waitForTimeout(300);
  },
  "db.png": async (page) => {
    await login(page, "golden-test-password");
    await expect(page.locator("#itemList .item")).toHaveCount(3);
    await page.waitForTimeout(2500);
    await selectFirstItem(page);
    await page.click("#dbBtn");
    await expect(page.locator(".card-db")).toBeVisible();
    await page.waitForTimeout(300);
  },
  "empty.png": async (page) => {
    await login(page, "golden-test-password");
    await expect(page.locator("#itemList .item")).toHaveCount(3);
    await page.waitForTimeout(2500);
    // 逐个删除全部条目（确认弹窗为自定义组件）
    while (await page.locator("#itemList .item").count()) {
      await page.click("#itemList .item:nth-child(1)");
      await page.waitForTimeout(300);
      await page.click("#deleteBtn");
      await page.locator(".modal").filter({ hasText: "删除确认" }).getByRole("button", { name: "确定删除" }).click();
      await page.waitForSelector("#verifyPasswordInput");
      await page.fill("#verifyPasswordInput", "golden-test-password");
      await page.click("#confirmVerifyBtn");
      await page.waitForTimeout(600);
    }
    await expect(page.locator("#itemList .item-empty")).toBeVisible();
    await page.waitForTimeout(400);
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
