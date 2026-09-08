// 工单 #04 验收：设置页恢复密钥生命周期（ADR-0003）。
// 解锁后可查看/复制（与生成时一致）；重生成需验证主密码，旧码作废。
import { expect, test } from "@playwright/test";
import { injectMockIpc, login } from "./helpers";

const MOCK_RECOVERY_KEY = "K7MQ-4XTA-9PLW-2RDN-6VHC-3XBT-8YQE-5ZJS";

async function openSettings(page: import("@playwright/test").Page) {
  await login(page, "golden-test-password");
  await expect(page.locator(".main-view")).toBeVisible();
  await page.click("#settingsBtn");
  await expect(page.locator(".card-settings")).toBeVisible();
}

test.describe("设置页恢复密钥生命周期", () => {
  test("查看恢复密钥与生成时一致并可复制", async ({ page }) => {
    injectMockIpc(page);
    await page.goto("/");
    await openSettings(page);

    await page.click("#viewRecoveryBtn");
    await expect(page.locator("#recoveryKeyDisplay")).toHaveText(MOCK_RECOVERY_KEY);

    await page.context().grantPermissions(["clipboard-read", "clipboard-write"]);
    await page.click("#copyRecoveryBtn");
    await expect(page.locator("#copyRecoveryBtn")).toHaveText("已复制 ✓");
    const clipboard = await page.evaluate(() => navigator.clipboard.readText());
    expect(clipboard).toBe(MOCK_RECOVERY_KEY);
  });

  test("重生成需验证主密码，错误密码被拒", async ({ page }) => {
    injectMockIpc(page);
    await page.goto("/");
    await openSettings(page);

    await page.click("#viewRecoveryBtn");
    await page.click("#regenerateRecoveryBtn");
    await page.fill("#regenPasswordInput", "错误的密码");
    await page.click("#regenConfirmBtn");

    // 后端拒绝，恢复密钥保持原值
    await expect(page.locator(".card-settings .auth-error")).not.toHaveText("");
    await expect(page.locator("#recoveryKeyDisplay")).toHaveText(MOCK_RECOVERY_KEY);
  });

  test("重生成成功后显示新码，旧码不再能通过救援", async ({ page }) => {
    injectMockIpc(page);
    await page.goto("/");
    await openSettings(page);

    await page.click("#viewRecoveryBtn");
    await page.click("#regenerateRecoveryBtn");
    await page.fill("#regenPasswordInput", "golden-test-password");
    await page.click("#regenConfirmBtn");

    const newCode = (await page.locator("#recoveryKeyDisplay").textContent())?.trim() ?? "";
    expect(newCode).not.toBe(MOCK_RECOVERY_KEY);
    expect(newCode.split("-").length).toBe(8);

    // 锁定后用新码救援成功，旧码被拒
    await page.click("#settingsCloseBtn");
    await page.click("#lockBtn");
    await page.click("#forgotPassword");
    await page.fill("#recoverKeyInput", MOCK_RECOVERY_KEY);
    await page.fill("#recoverPassword", "rescued-pass-1");
    await page.fill("#recoverPassword2", "rescued-pass-1");
    await page.click("#recoverBtn");
    await expect(page.locator("#recoverError")).toHaveText("恢复密钥不正确");

    // 失败提交后表单已清空，重新填入新码与新密码
    await page.fill("#recoverKeyInput", newCode);
    await page.fill("#recoverPassword", "rescued-pass-1");
    await page.fill("#recoverPassword2", "rescued-pass-1");
    await page.click("#recoverBtn");
    await expect(page.locator(".main-view")).toBeVisible();
  });
});
