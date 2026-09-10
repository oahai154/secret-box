import { expect, test } from "@playwright/test";
import { injectMockIpc, login } from "./helpers";

// 回归两个编辑器缺陷：
// 1) 保存不幂等：内容没有改动也写库建版本——连点保存会刷出一串内容相同的版本，
//    还把条目按修改时间顶到列表最前；
// 2) 保密内容隐藏态是"看着像输入框、实际点不动"的死区（pointer-events:none +
//    tabindex=-1），且两种状态的占位文案完全一样，切换后看不出任何变化。

async function unlock(page: import("@playwright/test").Page) {
  await page.goto("/");
  await login(page, "golden-test-password");
  await expect(page.locator("#itemList .item")).toHaveCount(3);
}

test.describe("编辑器保存与保密内容交互", () => {
  test.beforeEach(async ({ page }) => {
    injectMockIpc(page);
  });

  test("内容未改动时保存置灰，Ctrl+S 如实提示且不产生新版本", async ({ page }) => {
    await unlock(page);
    const versions = page.locator("#historyList .history-item");
    const before = await versions.count();

    await expect(page.locator("#saveBtn")).toBeDisabled();

    // 按钮点不动，快捷键必须给出明确回执，而不是"点了没反应"或悄悄写库
    await page.keyboard.press("Control+s");
    await expect(page.locator("#toast")).toHaveText("内容未变化,无需保存");
    await expect(versions).toHaveCount(before);
  });

  test("改动后再保存产生新版本，保存完按钮重新置灰", async ({ page }) => {
    await unlock(page);
    const versions = page.locator("#historyList .history-item");
    const before = await versions.count();

    await page.locator("#itemTitle").fill("改过的标题");
    await expect(page.locator("#saveBtn")).toBeEnabled();

    await page.click("#saveBtn");
    await expect(page.locator("#toast")).toHaveText("已保存,已记录新版本");
    await expect(versions).toHaveCount(before + 1);

    // 落库内容成为新基线：再点保存仍是空操作，不会继续堆版本
    await expect(page.locator("#saveBtn")).toBeDisabled();
    await page.keyboard.press("Control+s");
    await expect(page.locator("#toast")).toHaveText("内容未变化,无需保存");
    await expect(versions).toHaveCount(before + 1);
  });

  test("新建条目默认展开，无需先点显示即可直接输入", async ({ page }) => {
    await unlock(page);
    await page.click("#newBtn");

    const value = page.locator("#itemValue");
    await expect(value).toHaveClass(/value-visible/);
    await expect(page.locator("#toggleValueBtn .toggle-text")).toHaveText("点击隐藏");
    await expect(value).toHaveAttribute("placeholder", "输入保密内容…");

    await value.click();
    await page.keyboard.type("sk-live-123");
    await expect(value).toHaveValue("sk-live-123");

    await page.fill("#itemTitle", "新建即展开");
    await page.click("#saveBtn");
    await expect(page.locator("#toast")).toHaveText("已保存");
    await expect(value).toHaveValue("sk-live-123");
  });

  test("隐藏态点击内容区即展开并聚焦，掩码只在确有内容时出现", async ({ page }) => {
    await unlock(page);

    const value = page.locator("#itemValue");
    // 已有条目默认隐藏；有内容时占位是掩码，明确区分于展开态
    await expect(value).toHaveClass(/value-hidden/);
    await expect(value).toHaveAttribute("placeholder", /●●●●●●●●/);

    // 点内容区本体（不是按钮）也要展开并聚焦，不能是点不动的死区
    await value.click();
    await expect(value).toHaveClass(/value-visible/);
    await page.keyboard.type("XYZ");
    expect(await value.inputValue()).toContain("XYZ");
    await expect(page.locator("#saveBtn")).toBeEnabled();

    // 新建一个没有保密内容的条目：隐藏后不应显示掩码（那是"有内容"的暗示）
    await page.click("#newBtn");
    await page.fill("#itemTitle", "只有标题");
    await page.click("#saveBtn");
    await expect(page.locator("#toast")).toHaveText("已保存");
    await page.click("#itemList .item:nth-child(1)");
    await page.locator("#itemList .item", { hasText: "只有标题" }).click();
    await expect(value).toHaveClass(/value-hidden/);
    await expect(value).toHaveAttribute("placeholder", "点击这里或「点击显示」后可输入…");
  });
});
