import { expect, test } from "@playwright/test";
import { injectMockIpc } from "./helpers";

// 工单 #06 验收：快照导出（含选择保存位置 mock）、导入、清除痕迹确认流程。
// 加密文件格式与跨语言互通由 Rust 集成测试 + Go 工具回环保证，这里验证前端交互。
// 口令输入弹窗为 Promise 化单次输入：确认即关闭，流程失败需重新发起。

const MOCK_SNAPSHOT_FILE = {
  name: "secretbox-backup-test.secretbox",
  mimeType: "application/octet-stream",
  buffer: Buffer.from("bW9ja0ZpbGVDb250ZW50", "base64"),
};

test.describe("数据备份与迁移", () => {
  test.beforeEach(async ({ page }) => {
    injectMockIpc(page);
    await page.goto("/");
    await page.fill("#authPassword", "golden-test-password");
    await page.click("#authBtn");
    await expect(page.locator(".main-view")).toBeVisible();
    await page.waitForTimeout(2500); // 等 toast 消失
    await page.click("#dbBtn");
    await expect(page.locator(".card-db")).toBeVisible();
    await expect(page.locator("#dbInfo")).toContainText("✅ 正常");
    await expect(page.locator("#dbInfo")).toContainText("3 条");
    await expect(page.locator("#dbInfo")).toContainText("✅ 已设置");
  });

  test("导出：口令过短提示中止，重新导出成功并提示文件名", async ({ page }) => {
    await page.click("#exportBtn");
    await expect(page.locator(".card-verify")).toBeVisible();

    // 口令过短：确认后弹窗关闭，流程以 toast 提示中止
    await page.fill("#promptInput", "abc");
    await page.click("#promptConfirmBtn");
    await expect(page.locator("#toast")).toContainText("迁移口令至少 4 个字符");
    await expect(page.locator(".card-verify")).toHaveCount(0);

    // 重新发起导出，正确口令 → 导出 + 保存
    await page.click("#exportBtn");
    await page.fill("#promptInput", "mock-pass-123");
    await page.click("#promptConfirmBtn");
    await expect(page.locator("#toast")).toContainText(
      "已导出迁移文件: secretbox-backup-20260905-120000.secretbox",
    );
  });

  test("导入：口令错误提示失败，正确口令导入后回到解锁页", async ({ page }) => {
    await page.click("#importBtn");
    await page.setInputFiles("#importFile", MOCK_SNAPSHOT_FILE);
    await expect(page.locator(".card-verify")).toBeVisible();

    // 错误口令：导入失败 toast
    await page.fill("#promptInput", "wrong-pass");
    await page.click("#promptConfirmBtn");
    await expect(page.locator("#toast")).toContainText("导入失败: 迁移口令错误或文件已损坏");
    await expect(page.locator(".main-view")).toBeVisible();

    // 重新选择文件，正确口令 → 导入成功
    await page.click("#importBtn");
    await page.setInputFiles("#importFile", MOCK_SNAPSHOT_FILE);
    await page.fill("#promptInput", "mock-import-pass");
    await page.click("#promptConfirmBtn");
    await expect(page.locator("#toast")).toContainText("导入成功: 3 条数据");
    // 回到解锁页，需用原主密码重新解锁
    await expect(page.locator(".auth-card")).toBeVisible();
    await expect(page.locator(".main-view")).toHaveCount(0);
  });

  test("清除痕迹：需主密码验证 → 强制导出 → 二次确认", async ({ page }) => {
    await page.click("#wipeBtn");
    // 关闭数据弹窗，弹出主密码验证
    await expect(page.locator(".card-db")).toHaveCount(0);
    await expect(page.locator(".card-verify")).toBeVisible();

    // 取消验证则什么都不发生
    await page.locator(".card-verify").getByRole("button", { name: "取消" }).click();
    await expect(page.locator(".card-verify")).toHaveCount(0);
    await expect(page.locator(".main-view")).toBeVisible();

    // 重新走流程：正确主密码 → 强制导出
    await page.click("#dbBtn");
    await page.click("#wipeBtn");
    await page.fill("#verifyPasswordInput", "golden-test-password");
    await page.click("#confirmVerifyBtn");

    // 先提示导出，弹出迁移口令弹窗
    await expect(page.locator("#toast")).toContainText("请先完成迁移文件导出");
    await page.fill("#promptInput", "mock-pass-123");
    await page.click("#promptConfirmBtn");
    // 保存位置弹窗被 mock，导出成功后弹出最终确认
    await expect(page.locator("#toast")).toContainText("已导出迁移文件");

    // 最终确认弹窗
    const confirmModal = page.locator(".modal").filter({ hasText: "确定要彻底清除本地全部数据吗" });
    await expect(confirmModal).toBeVisible();

    // 确定清除 → 清除成功，回到解锁页
    await confirmModal.getByRole("button", { name: "确定清除" }).click();
    await expect(page.locator("#toast")).toContainText("已清除本地全部数据");
    await expect(page.locator(".auth-card")).toBeVisible();
    await expect(page.locator(".main-view")).toHaveCount(0);
  });
});
