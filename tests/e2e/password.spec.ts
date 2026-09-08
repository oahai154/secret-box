import { expect, test } from "@playwright/test";
import { injectMockIpc, login } from "./helpers";

// 工单 #05 验收：修改主密码 UI 流程（旧密码验证 + 两次新密码确认）。
// 后端重加密语义由 Rust 集成测试与 Go 跨语言回环保证，这里只验证前端交互。

test.describe("修改主密码", () => {
  test.beforeEach(async ({ page }) => {
    injectMockIpc(page);
    await page.goto("/");
    await login(page, "golden-test-password");
    await expect(page.locator(".main-view")).toBeVisible();
    await page.waitForTimeout(2500); // 等 toast 消失
  });

  async function openChangePasswordModal(page: import("@playwright/test").Page) {
    await page.click("#settingsBtn");
    await expect(page.locator(".card-settings")).toBeVisible();
    await page.click("#changePasswordBtn");
    await expect(page.locator(".card-password")).toBeVisible();
    // 设置弹窗关闭（与 Go 版行为一致）
    await expect(page.locator(".card-settings")).toHaveCount(0);
  }

  test("空当前密码提示且不提交", async ({ page }) => {
    await openChangePasswordModal(page);
    await page.click("#confirmPasswordBtn");
    await expect(page.locator("#passwordError")).toHaveText("请输入当前密码");
    await expect(page.locator(".card-password")).toBeVisible();
  });

  test("新密码过短提示", async ({ page }) => {
    await openChangePasswordModal(page);
    await page.fill("#oldPasswordInput", "golden-test-password");
    await page.fill("#newPasswordInput", "abc");
    await page.fill("#confirmPasswordInput", "abc");
    await page.click("#confirmPasswordBtn");
    await expect(page.locator("#passwordError")).toHaveText("新密码至少 4 位");
  });

  test("两次新密码不一致提示", async ({ page }) => {
    await openChangePasswordModal(page);
    await page.fill("#oldPasswordInput", "golden-test-password");
    await page.fill("#newPasswordInput", "new-pass-#05");
    await page.fill("#confirmPasswordInput", "different-pass");
    await page.click("#confirmPasswordBtn");
    await expect(page.locator("#passwordError")).toHaveText("两次输入的新密码不一致");
  });

  test("旧密码错误显示后端错误信息", async ({ page }) => {
    await openChangePasswordModal(page);
    await page.fill("#oldPasswordInput", "错误旧密码");
    await page.fill("#newPasswordInput", "new-pass-#05");
    await page.fill("#confirmPasswordInput", "new-pass-#05");
    await page.click("#confirmPasswordBtn");
    await expect(page.locator("#passwordError")).toHaveText("解密失败(主密码可能不正确)");
    await expect(page.locator(".card-password")).toBeVisible();
  });

  test("改密成功后弹窗关闭、提示成功，旧密码失效新密码可解锁", async ({ page }) => {
    await openChangePasswordModal(page);
    await page.fill("#oldPasswordInput", "golden-test-password");
    await page.fill("#newPasswordInput", "new-pass-#05");
    await page.fill("#confirmPasswordInput", "new-pass-#05");
    await page.click("#confirmPasswordBtn");

    // 弹窗关闭 + 成功 toast
    await expect(page.locator(".card-password")).toHaveCount(0);
    await expect(page.locator(".toast")).toContainText("密码修改成功");

    // 锁定后旧密码被拒绝、新密码可解锁
    await page.click("#lockBtn");
    await expect(page.locator(".auth-card")).toBeVisible();
    await page.fill("#authPassword", "golden-test-password");
    await page.click("#authBtn");
    await expect(page.locator("#authError")).toHaveText("主密码错误");

    await login(page, "new-pass-#05");
    await expect(page.locator(".main-view")).toBeVisible();
    await expect(page.locator("#itemList .item")).toHaveCount(3);
  });
});
