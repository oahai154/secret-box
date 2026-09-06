// 工单 #03 验收：忘记主密码的救援流程（ADR-0003）。
// 锁屏入口 → 输恢复密钥 + 新主密码 → 直接进入主界面，数据完整；
// 错误恢复密钥到不了设新密码一步；救援后恢复密钥继续有效（mock 语义一致）。
import { expect, test } from "@playwright/test";
import { injectMockIpc } from "./helpers";

const MOCK_RECOVERY_KEY = "K7MQ-4XTA-9PLW-2RDN-6VHC-3XBT-8YQE-5ZJS";

test.describe("忘记主密码救援流程", () => {
  test("解锁页有忘记主密码入口，进入救援页", async ({ page }) => {
    injectMockIpc(page);
    await page.goto("/");
    await expect(page.locator(".auth-card")).toBeVisible();

    await page.click("#forgotPassword");
    await expect(page.locator("#recoverKeyInput")).toBeVisible();
    // 主界面不可达
    await expect(page.locator(".main-view")).toHaveCount(0);
  });

  test("错误恢复密钥被拒且停留在救援页", async ({ page }) => {
    injectMockIpc(page);
    await page.goto("/");
    await page.click("#forgotPassword");
    await expect(page.locator("#recoverKeyInput")).toBeVisible();

    await page.fill("#recoverKeyInput", "AAAA-BBBB-CCCC-DDDD");
    await page.fill("#recoverPassword", "new-pass-123");
    await page.fill("#recoverPassword2", "new-pass-123");
    await page.click("#recoverBtn");

    await expect(page.locator("#recoverError")).toHaveText("恢复密钥不正确");
    await expect(page.locator(".main-view")).toHaveCount(0);
  });

  test("两次新密码不一致提示", async ({ page }) => {
    injectMockIpc(page);
    await page.goto("/");
    await page.click("#forgotPassword");

    await page.fill("#recoverKeyInput", MOCK_RECOVERY_KEY);
    await page.fill("#recoverPassword", "new-pass-123");
    await page.fill("#recoverPassword2", "new-pass-456");
    await page.click("#recoverBtn");

    await expect(page.locator("#recoverError")).toHaveText("两次输入不一致");
  });

  test("正确救援后进入主界面_新主密码生效_旧密码失效", async ({ page }) => {
    injectMockIpc(page);
    await page.goto("/");
    await page.click("#forgotPassword");
    await expect(page.locator("#recoverKeyInput")).toBeVisible();

    await page.fill("#recoverKeyInput", MOCK_RECOVERY_KEY.toLowerCase());
    await page.fill("#recoverPassword", "rescued-pass-9");
    await page.fill("#recoverPassword2", "rescued-pass-9");
    await page.click("#recoverBtn");

    // 直接进入主界面，数据完整（黄金样本 3 条）
    await expect(page.locator(".main-view")).toBeVisible();
    await expect(page.locator("#itemList .item")).toHaveCount(3);

    // 锁定后：新主密码可解锁，旧密码（golden-test-password）失效
    await page.click("#lockBtn");
    await expect(page.locator(".auth-card")).toBeVisible();
    await page.fill("#authPassword", "golden-test-password");
    await page.click("#authBtn");
    await expect(page.locator("#authError")).not.toHaveText("");

    await page.fill("#authPassword", "rescued-pass-9");
    await page.click("#authBtn");
    await expect(page.locator(".main-view")).toBeVisible();
  });
});
