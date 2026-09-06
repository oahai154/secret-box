// 工单 #02 验收：首次设置主密码后强制经过恢复密钥确认页（ADR-0003）。
// 无"跳过"路径；重输不匹配或未勾选确认时无法进入主界面。
import { expect, test } from "@playwright/test";
import { injectMockIpc } from "./helpers";

const MOCK_RECOVERY_KEY = "K7MQ-4XTA-9PLW-2RDN-6VHC-3XBT-8YQE-5ZJS";

async function injectFreshSetupMock(page: import("@playwright/test").Page) {
  injectMockIpc(page);
  // 覆盖为"未设置主密码"的首次使用状态
  await page.addInitScript(() => {
    const ipc = window.__SECRETBOX_IPC__;
    if (!ipc) throw new Error("mock IPC 未注入");
    ipc.getStatus = async () => ({ has_password: false, unlocked: false });
  });
}

test.describe("首次设置与恢复密钥强制确认", () => {
  test("设置主密码后强制进入恢复密钥页并展示分组码", async ({ page }) => {
    await injectFreshSetupMock(page);
    await page.goto("/");
    await expect(page.locator(".auth-card")).toBeVisible();

    await page.fill("#authPassword", "first-pass-123");
    await page.fill("#authPassword2", "first-pass-123");
    await page.click("#authBtn");

    // 强制停留在恢复密钥页，不进入主界面
    await expect(page.locator("#recoveryCode")).toBeVisible();
    await expect(page.locator("#recoveryCode")).toHaveText(MOCK_RECOVERY_KEY);
    await expect(page.locator(".main-view")).toHaveCount(0);

    // 无"跳过"按钮
    await expect(page.getByText("跳过")).toHaveCount(0);
  });

  test("重输不匹配或未勾选确认时无法进入主界面", async ({ page }) => {
    await injectFreshSetupMock(page);
    await page.goto("/");
    await page.fill("#authPassword", "first-pass-123");
    await page.fill("#authPassword2", "first-pass-123");
    await page.click("#authBtn");
    await expect(page.locator("#recoveryCode")).toBeVisible();

    // 重输错误：按钮保持禁用
    await page.fill("#recoveryInput", "AAAA-BBBB-CCCC-DDDD");
    await expect(page.locator("#recoveryBtn")).toBeDisabled();

    // 重输正确但未勾选：仍禁用
    await page.fill("#recoveryInput", MOCK_RECOVERY_KEY);
    await expect(page.locator("#recoveryBtn")).toBeDisabled();

    // 小写、无连字符输入也应通过归一化比对
    await page.fill("#recoveryInput", MOCK_RECOVERY_KEY.toLowerCase().replace(/-/g, ""));

    // 勾选确认后放行
    await page.check("#recoveryConfirm");
    await page.click("#recoveryBtn");
    await expect(page.locator(".main-view")).toBeVisible();
  });

  test("复制按钮把恢复密钥写入剪贴板", async ({ page }) => {
    // 无头 Chromium 需要显式授权剪贴板
    await page.context().grantPermissions(["clipboard-read", "clipboard-write"]);
    await injectFreshSetupMock(page);
    await page.goto("/");
    await page.fill("#authPassword", "first-pass-123");
    await page.fill("#authPassword2", "first-pass-123");
    await page.click("#authBtn");
    await expect(page.locator("#recoveryCode")).toBeVisible();

    await page.click("#recoveryCopy");
    await expect(page.locator("#recoveryCopy")).toHaveText("已复制 ✓");
    const clipboard = await page.evaluate(() => navigator.clipboard.readText());
    expect(clipboard).toBe(MOCK_RECOVERY_KEY);
  });
});
