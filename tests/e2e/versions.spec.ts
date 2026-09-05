import { expect, test } from "@playwright/test";
import { injectMockIpc } from "./helpers";

// fixture 中"公司邮箱"（id=2）有 3 个历史版本，v3 内容为 Q3 密码
const ITEM_TITLE = "公司邮箱";
const V3_SNAPSHOT = "zhang.san@corp.example.cn: Mail-Pass-2026-Q3-@888";
const V1_SNAPSHOT = "zhang.san@corp.example.cn: Mail-Pass-2026-#315";

async function unlockAndSelect(page: import("@playwright/test").Page) {
  await page.goto("/");
  await page.fill("#authPassword", "golden-test-password");
  await page.click("#authBtn");
  await expect(page.locator("#itemList .item")).toHaveCount(3);
  // 选中公司邮箱（v3 条目）
  await page.locator("#itemList .item", { hasText: ITEM_TITLE }).click();
  await expect(page.locator("#historyList .history-item")).toHaveCount(3);
}

test.describe("历史版本：查看/恢复/删除", () => {
  test.beforeEach(async ({ page }) => {
    injectMockIpc(page);
  });

  test("点击版本行可查看该版本内容", async ({ page }) => {
    await unlockAndSelect(page);

    // 点第一行（v3）→ 保密内容区显示 v3 快照明文
    await page.locator("#historyList .history-item").first().locator(".hver").click();
    await expect(page.locator("#toast")).toHaveText("正在查看 v3 的内容");
    await expect(page.locator("#itemValue")).toHaveValue(V3_SNAPSHOT);
    await expect(page.locator("#itemValue")).toHaveClass(/value-visible/);

    // 再看 v1 → 内容切换
    await page.locator("#historyList .history-item").last().locator(".hver").click();
    await expect(page.locator("#itemValue")).toHaveValue(V1_SNAPSHOT);
  });

  test("还原历史版本会生成新的修改记录", async ({ page }) => {
    await unlockAndSelect(page);

    await page.locator("#historyList .history-item").last().locator(".restore-btn").click();
    await expect(page.locator(".modal")).toContainText("确定将当前内容还原为该版本吗?");
    await page.locator(".modal .btn-primary", { hasText: "还原" }).click();

    await expect(page.locator("#toast")).toHaveText("已还原到 v1");
    // 当前内容回到 v1 快照，版本数 3 → 4
    await expect(page.locator("#itemValue")).toHaveValue(V1_SNAPSHOT);
    await expect(page.locator("#historyList .history-item")).toHaveCount(4);
  });

  test("删除历史版本需要主密码确认：输错不删除，正确后删除", async ({ page }) => {
    await unlockAndSelect(page);

    await page.locator("#historyList .history-item").first().locator(".delete-ver-btn").click();
    await expect(page.locator(".verify-hint")).toHaveText("删除历史版本需要验证主密码");

    // 错误密码：提示且版本仍在
    await page.fill("#verifyPasswordInput", "错误的密码");
    await page.click("#confirmVerifyBtn");
    await expect(page.locator(".password-error")).toHaveText("密码不正确");
    await expect(page.locator("#historyList .history-item")).toHaveCount(3);

    // 正确密码：版本删除
    await page.fill("#verifyPasswordInput", "golden-test-password");
    await page.click("#confirmVerifyBtn");
    await expect(page.locator("#toast")).toHaveText("已删除 v3");
    await expect(page.locator("#historyList .history-item")).toHaveCount(2);
  });
});
