// 工单 #06 验收：v1 旧库强制升级向导（ADR-0003）。
// legacy 状态下向导拦截、无跳过；错误主密码可重试；成功后强制恢复密钥确认，
// 数据完整，主密码不变。
import { expect, test } from "@playwright/test";
import { injectMockIpc } from "./helpers";

const MOCK_RECOVERY_KEY = "K7MQ-4XTA-9PLW-2RDN-6VHC-3XBT-8YQE-5ZJS";

async function injectLegacyMock(page: import("@playwright/test").Page) {
  injectMockIpc(page);
  // 覆盖为 v1 旧库待升级状态
  await page.addInitScript(() => {
    const ipc = window.__SECRETBOX_IPC__;
    if (!ipc) throw new Error("mock IPC 未注入");
    ipc.getStatus = async () => ({ has_password: true, unlocked: false, legacy: true });
  });
}

test.describe("v1 旧库升级向导", () => {
  test("legacy 状态下向导拦截且无跳过，错误主密码可重试", async ({ page }) => {
    await injectLegacyMock(page);
    await page.goto("/");

    // 直接进入升级向导，看不到解锁页与主界面
    await expect(page.locator("#upgradePassword")).toBeVisible();
    await expect(page.locator(".auth-card")).toHaveCount(1);
    await expect(page.locator(".main-view")).toHaveCount(0);
    await expect(page.getByText("跳过")).toHaveCount(0);

    // 错误主密码：停留在向导
    await page.fill("#upgradePassword", "错误的密码");
    await page.click("#upgradeBtn");
    await expect(page.locator("#upgradeError")).toHaveText("主密码不正确");
    await expect(page.locator("#upgradePassword")).toBeVisible();
  });

  test("升级成功后强制恢复密钥确认，完成后数据完整进入主界面", async ({ page }) => {
    await injectLegacyMock(page);
    await page.goto("/");
    await expect(page.locator("#upgradePassword")).toBeVisible();

    await page.fill("#upgradePassword", "golden-test-password");
    await page.click("#upgradeBtn");

    // 强制恢复密钥确认页
    await expect(page.locator("#recoveryCode")).toHaveText(MOCK_RECOVERY_KEY);
    await expect(page.locator(".main-view")).toHaveCount(0);

    await page.fill("#recoveryInput", MOCK_RECOVERY_KEY);
    await page.check("#recoveryConfirm");
    await page.click("#recoveryBtn");

    // 进入主界面，数据完整（黄金样本 3 条），主密码不变
    await expect(page.locator(".main-view")).toBeVisible();
    await expect(page.locator("#itemList .item")).toHaveCount(3);
  });
});
