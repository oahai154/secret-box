import { expect, test } from "@playwright/test";
import { injectMockIpc } from "./helpers";

// 回归：textarea 按内容自动长开，无内层滚动塌陷。
// 覆盖路径：切换条目、显示/隐藏保密内容。
const LONG = Array.from({ length: 15 }, (_, i) => `第${i + 1}行保密内容`).join("\n");

async function heightOf(page: import("@playwright/test").Page, id: string) {
  return page.locator(id).evaluate((el: HTMLTextAreaElement) => ({
    client: el.clientHeight,
    scroll: el.scrollHeight,
  }));
}

test("保密内容显示时按内容长开，隐藏时固定最小高度", async ({ page }) => {
  injectMockIpc(page);
  await page.goto("/");
  await page.fill("#authPassword", "golden-test-password");
  await page.click("#authBtn");
  await expect(page.locator("#itemList .item")).toHaveCount(3);

  // 新建条目写入多行保密内容
  await page.click("#newBtn");
  await page.fill("#itemTitle", "长内容条目");
  await page.fill("#itemValue", LONG);
  await page.fill("#itemNote", LONG);
  await page.click("#saveBtn");
  await expect(page.locator("#toast")).toContainText("已保存");

  // 隐藏态：固定最小高度（min-height 72px），不随内容长开
  const hidden = await heightOf(page, "#itemValue");
  expect(hidden.client).toBeLessThan(100);

  // 点「点击显示」→ 完整长开，无内层滚动
  await page.click("#toggleValueBtn");
  const shown = await heightOf(page, "#itemValue");
  expect(shown.client, "显示后应 ≥ scrollHeight").toBeGreaterThanOrEqual(shown.scroll - 2);
  expect(shown.client, "长内容应明显超过 min-height").toBeGreaterThan(300);

  // 点「点击隐藏」→ 回到最小高度
  await page.click("#toggleValueBtn");
  const hiddenAgain = await heightOf(page, "#itemValue");
  expect(hiddenAgain.client).toBeLessThan(100);

  // 切走再切回：隐藏态仍是最小高度；备注框按内容长开
  await page.click("#itemList .item >> nth=0");
  await page.click("#itemList .item:has-text('长内容条目')");
  await page.waitForTimeout(200);
  const hiddenAfterSwitch = await heightOf(page, "#itemValue");
  expect(hiddenAfterSwitch.client).toBeLessThan(100);
  const note = await heightOf(page, "#itemNote");
  expect(note.client, "切回后备注应 ≥ scrollHeight").toBeGreaterThanOrEqual(note.scroll - 2);
  expect(note.client, "长备注应明显超过 min-height").toBeGreaterThan(300);

  // 切回后点「显示」也要长开（修复过的塌陷路径）
  await page.click("#toggleValueBtn");
  const shownAfterSwitch = await heightOf(page, "#itemValue");
  expect(shownAfterSwitch.client).toBeGreaterThanOrEqual(shownAfterSwitch.scroll - 2);
  expect(shownAfterSwitch.client).toBeGreaterThan(300);
});
