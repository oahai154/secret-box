import { expect, test } from "@playwright/test";
import { injectMockIpc } from "./helpers";

test.describe("解锁/锁定会话", () => {
  test.beforeEach(async ({ page }) => {
    injectMockIpc(page);
  });

  test("错误主密码显示错误提示且不进入应用", async ({ page }) => {
    await page.goto("/");
    await expect(page.locator(".auth-card")).toBeVisible();

    await page.fill("#authPassword", "错误的主密码");
    await page.click("#authBtn");

    await expect(page.locator("#authError")).toHaveText("主密码错误");
    // 不进入主界面
    await expect(page.locator(".main-view")).toHaveCount(0);
    await expect(page.locator(".auth-card")).toBeVisible();
  });

  test("正确主密码进入条目列表", async ({ page }) => {
    await page.goto("/");
    await page.fill("#authPassword", "golden-test-password");
    await page.click("#authBtn");

    await expect(page.locator(".main-view")).toBeVisible();
    await expect(page.locator("#itemList .item")).toHaveCount(3);
    // 默认选中第一个条目，右侧显示详情
    await expect(page.locator("#itemList .item.active")).toHaveCount(1);
    await expect(page.locator("#itemTitle")).not.toHaveValue("");
  });

  test("手动锁定后回到解锁页，需要重新解锁", async ({ page }) => {
    await page.goto("/");
    await page.fill("#authPassword", "golden-test-password");
    await page.click("#authBtn");
    await expect(page.locator(".main-view")).toBeVisible();

    await page.click("#lockBtn");
    await expect(page.locator(".auth-card")).toBeVisible();
    await expect(page.locator(".main-view")).toHaveCount(0);

    // 重新解锁成功
    await page.fill("#authPassword", "golden-test-password");
    await page.click("#authBtn");
    await expect(page.locator(".main-view")).toBeVisible();
  });

  test("无操作 120 秒后自动锁定（fake timer）", async ({ page }) => {
    await page.clock.install();
    await page.goto("/");
    await page.fill("#authPassword", "golden-test-password");
    await page.click("#authBtn");
    await expect(page.locator(".main-view")).toBeVisible();

    // 快进 119 秒：仍处于解锁状态
    await page.clock.runFor(119_000);
    await expect(page.locator(".main-view")).toBeVisible();

    // 快进 2 秒（跨过 120 秒阈值）：自动锁定
    await page.clock.runFor(2_000);
    await expect(page.locator(".auth-card")).toBeVisible();
    await expect(page.locator(".main-view")).toHaveCount(0);
  });
});
