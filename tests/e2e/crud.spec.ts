import { expect, test } from "@playwright/test";
import { injectMockIpc, login } from "./helpers";

async function unlock(page: import("@playwright/test").Page) {
  await page.goto("/");
  await login(page, "golden-test-password");
  await expect(page.locator("#itemList .item")).toHaveCount(3);
}

test.describe("条目增删改查", () => {
  test.beforeEach(async ({ page }) => {
    injectMockIpc(page);
  });

  test("新建条目流程完整可用", async ({ page }) => {
    await unlock(page);

    await page.click("#newBtn");
    await expect(page.locator("#itemMeta")).toHaveText("新条目");
    await expect(page.locator("#itemTitle")).toHaveValue("");

    await page.fill("#itemTitle", "测试站点 🧪");
    // 通过自定义下拉选择分类
    await page.click("#categorySelect .custom-select-trigger");
    await page.locator("#categorySelect .custom-select-dropdown li", { hasText: "应用密钥" }).click();
    await expect(page.locator("#categorySelect .custom-select-value")).toHaveText("应用密钥");
    await page.fill("#itemValue", "user: pass 🦀");
    await page.fill("#itemNote", "临时备注");

    await page.click("#saveBtn");
    await expect(page.locator("#toast")).toHaveText("已保存");

    // 列表出现新条目并处于选中态
    await expect(page.locator("#itemList .item", { hasText: "测试站点 🧪" })).toHaveCount(1);
    await expect(page.locator("#itemList .item.active")).toHaveText(/测试站点/);
    await expect(page.locator("#itemMeta")).not.toHaveText("新条目");
    // 新条目带 1 个历史版本
    await expect(page.locator("#historyList .history-item")).toHaveCount(1);
  });

  test("编辑条目流程完整可用（Ctrl+S 保存并产生新版本）", async ({ page }) => {
    await unlock(page);

    const title = page.locator("#itemTitle");
    const oldTitle = await title.inputValue();
    await title.fill(oldTitle + "（已改）");
    await page.keyboard.press("Control+s");

    await expect(page.locator("#toast")).toHaveText("已保存,已记录新版本");
    await expect(page.locator("#itemList .item.active .item-title")).toHaveText(oldTitle + "（已改）");
    // 编辑产生新版本
    await expect(page.locator("#historyList .history-item")).toHaveCount(2);
  });

  test("删除流程必须主密码确认：取消与输错均不删除", async ({ page }) => {
    await unlock(page);
    const firstTitle = await page.locator("#itemList .item.active .item-title").textContent();

    // 打开删除确认后取消
    await page.click("#deleteBtn");
    await expect(page.locator(".modal")).toContainText("确定删除该条目吗?");
    await page.locator(".modal .btn-ghost", { hasText: "取消" }).click();
    await expect(page.locator("#itemList .item.active")).toHaveCount(1);

    // 确认后输错密码
    await page.click("#deleteBtn");
    await page.locator(".modal .btn-danger", { hasText: "确定删除" }).click();
    await expect(page.locator("#verifyPasswordInput")).toBeVisible();
    await page.fill("#verifyPasswordInput", "错误的密码");
    await page.click("#confirmVerifyBtn");
    await expect(page.locator(".password-error")).toHaveText("密码不正确");
    await expect(page.locator("#itemList .item.active")).toHaveCount(1);

    // 输入正确密码后删除成功
    await page.fill("#verifyPasswordInput", "golden-test-password");
    await page.click("#confirmVerifyBtn");
    await expect(page.locator("#toast")).toHaveText("已删除");
    await expect(page.locator("#itemList .item", { hasText: firstTitle! })).toHaveCount(0);
  });

  test("关闭删除密码开关后删除无需验证", async ({ page }) => {
    await unlock(page);

    await page.click("#settingsBtn");
    // 开关的 checkbox 视觉隐藏，点击其滑块切换
    const slider = page.locator('label.toggle-switch:has(#deleteRequiresPassword) .toggle-slider');
    await slider.click(); // 默认为 true，点击后关闭
    await expect(slider).toBeVisible();
    await page.click("#settingsCloseBtn");

    await page.click("#deleteBtn");
    await page.locator(".modal .btn-danger", { hasText: "确定删除" }).click();
    // 不出现验证弹窗，直接删除
    await expect(page.locator("#verifyPasswordInput")).toHaveCount(0);
    await expect(page.locator("#toast")).toHaveText("已删除");
  });
});
