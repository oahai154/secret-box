import { expect, test } from "@playwright/test";
import * as fs from "fs";
import * as path from "path";
import pixelmatch from "pixelmatch";
import { PNG } from "pngjs";
import { injectMockIpc, login } from "./helpers";

// 视觉对照：与 Go 版 web/ 页面截取的基准图做像素对比。
// 基准图由 tests/e2e/capture-baselines.mjs 生成（Go 版跑在同一台机器、同一浏览器、
// 同一视口、浅色主题、禁用动画）。不同浏览器引擎渲染不可能逐像素相同，
// 因此允许 1% 的差异像素（差异图写入 test-results 供人工审查）。

const BASELINES = path.join(import.meta.dirname, "baselines");
const RESULTS = path.join(import.meta.dirname, "../../test-results");
const ALLOWED_DIFF_PERCENT = 1.0;

function expectSameAsBaseline(page: Page, baselineName: string) {
  return async () => {
    const actual: Buffer = await page.screenshot();
    const baselinePath = path.join(BASELINES, baselineName);
    expect(fs.existsSync(baselinePath), `缺少基准图 ${baselineName}，先运行 npm run baseline`).toBe(true);

    const baseline = PNG.sync.read(fs.readFileSync(baselinePath));
    const actualPng = PNG.sync.read(actual);
    expect(actualPng.width).toBe(baseline.width);
    expect(actualPng.height).toBe(baseline.height);

    const diff = new PNG({ width: baseline.width, height: baseline.height });
    const diffPixels = pixelmatch(
      baseline.data,
      actualPng.data,
      diff.data,
      baseline.width,
      baseline.height,
      { threshold: 0.2 },
    );
    const percent = (diffPixels / (baseline.width * baseline.height)) * 100;
    const diffPath = path.join(RESULTS, `diff-${baselineName}`);
    fs.mkdirSync(RESULTS, { recursive: true });
    fs.writeFileSync(diffPath, PNG.sync.write(diff));
    fs.writeFileSync(path.join(RESULTS, `actual-${baselineName}`), actual);
    expect(
      percent,
      `与基准 ${baselineName} 的像素差异 ${percent.toFixed(3)}% 超过 ${ALLOWED_DIFF_PERCENT}%，差异图: ${diffPath}`,
    ).toBeLessThan(ALLOWED_DIFF_PERCENT);
  };
}

test.describe("视觉对照（与 Go 版基准）", () => {
  test("解锁页", async ({ page }) => {
    injectMockIpc(page);
    await page.goto("/");
    await expect(page.locator(".auth-card")).toBeVisible();
    await expect(page.locator("#authPassword")).toBeFocused();
    const check = expectSameAsBaseline(page, "auth.png");
    await check();
  });

  test("条目列表页", async ({ page }) => {
    injectMockIpc(page);
    await page.goto("/");
    await login(page, "golden-test-password");
    await expect(page.locator("#itemList .item")).toHaveCount(3);
    await expect(page.locator("#itemTitle")).not.toHaveValue("");
    // 等 toast（已解锁）消失后再截图，两侧一致
    await page.waitForTimeout(2500);
    const check = expectSameAsBaseline(page, "main.png");
    await check();
  });

  test("新增条目页", async ({ page }) => {
    injectMockIpc(page);
    await page.goto("/");
    await login(page, "golden-test-password");
    await expect(page.locator("#itemList .item")).toHaveCount(3);
    await page.waitForTimeout(2500); // 等 toast 消失
    await page.click("#newBtn");
    await expect(page.locator("#itemMeta")).toHaveText("新条目");
    await page.waitForTimeout(400); // 等进场动画结束
    const check = expectSameAsBaseline(page, "new.png");
    await check();
  });

  test("保密内容显示态", async ({ page }) => {
    injectMockIpc(page);
    await page.goto("/");
    await login(page, "golden-test-password");
    await expect(page.locator("#itemList .item")).toHaveCount(3);
    await page.waitForTimeout(2500);
    await page.click("#toggleValueBtn");
    await page.waitForTimeout(300);
    const check = expectSameAsBaseline(page, "value-visible.png");
    await check();
  });

  test("历史版本面板", async ({ page }) => {
    injectMockIpc(page);
    await page.goto("/");
    await login(page, "golden-test-password");
    await expect(page.locator("#itemList .item")).toHaveCount(3);
    await page.waitForTimeout(2500);
    // 选中第二个条目（公司邮箱，含 3 个版本）并滚动到历史面板
    await page.click("#itemList .item:nth-child(2)");
    await expect(page.locator("#historyPanel")).toBeVisible();
    await page.locator("#historyPanel").scrollIntoViewIfNeeded();
    await page.waitForTimeout(300);
    const check = expectSameAsBaseline(page, "history.png");
    await check();
  });

  test("设置弹窗", async ({ page }) => {
    injectMockIpc(page);
    await page.goto("/");
    await login(page, "golden-test-password");
    await expect(page.locator("#itemList .item")).toHaveCount(3);
    await page.waitForTimeout(2500);
    // 与 Go 基准一致：条目 1 + 页面顶部
    await page.click("#itemList .item:nth-child(1)");
    await page.evaluate(() => {
      window.scrollTo(0, 0);
      document.querySelectorAll("*").forEach((el) => {
        if (el.scrollTop) el.scrollTop = 0;
        if (el.scrollLeft) el.scrollLeft = 0;
      });
    });
    await page.waitForTimeout(300);
    await page.click("#settingsBtn");
    await expect(page.locator(".card-settings")).toBeVisible();
    await page.waitForTimeout(300);
    const check = expectSameAsBaseline(page, "settings.png");
    await check();
  });

  test("修改密码弹窗", async ({ page }) => {
    injectMockIpc(page);
    await page.goto("/");
    await login(page, "golden-test-password");
    await expect(page.locator("#itemList .item")).toHaveCount(3);
    await page.waitForTimeout(2500);
    await page.click("#itemList .item:nth-child(1)");
    await page.evaluate(() => {
      window.scrollTo(0, 0);
      document.querySelectorAll("*").forEach((el) => {
        if (el.scrollTop) el.scrollTop = 0;
        if (el.scrollLeft) el.scrollLeft = 0;
      });
    });
    await page.waitForTimeout(300);
    await page.click("#settingsBtn");
    await expect(page.locator(".card-settings")).toBeVisible();
    await page.click("#changePasswordBtn");
    await expect(page.locator(".card-password")).toBeVisible();
    await page.waitForTimeout(300);
    const check = expectSameAsBaseline(page, "change-password.png");
    await check();
  });

  test("数据备份与迁移弹窗", async ({ page }) => {
    injectMockIpc(page);
    await page.goto("/");
    await login(page, "golden-test-password");
    await expect(page.locator("#itemList .item")).toHaveCount(3);
    await page.waitForTimeout(2500);
    await page.click("#itemList .item:nth-child(1)");
    await page.evaluate(() => {
      window.scrollTo(0, 0);
      document.querySelectorAll("*").forEach((el) => {
        if (el.scrollTop) el.scrollTop = 0;
        if (el.scrollLeft) el.scrollLeft = 0;
      });
    });
    await page.waitForTimeout(300);
    await page.click("#dbBtn");
    await expect(page.locator(".card-db")).toBeVisible();
    await page.waitForTimeout(300);
    const check = expectSameAsBaseline(page, "db.png");
    await check();
  });

  test("空列表态", async ({ page }) => {
    injectMockIpc(page);
    await page.goto("/");
    await login(page, "golden-test-password");
    await expect(page.locator("#itemList .item")).toHaveCount(3);
    await page.waitForTimeout(2500);
    // 逐个删除全部条目（与 Go 基准流程一致；确认弹窗为自定义组件）
    while (await page.locator("#itemList .item").count()) {
      await page.click("#itemList .item:nth-child(1)");
      await page.waitForTimeout(300);
      await page.click("#deleteBtn");
      // 自定义确认弹窗（Go 侧为原生 confirm，不出现在两侧截图状态中）
      await page.locator(".modal").filter({ hasText: "删除确认" }).getByRole("button", { name: "确定删除" }).click();
      await page.waitForSelector("#verifyPasswordInput");
      await page.fill("#verifyPasswordInput", "golden-test-password");
      await page.click("#confirmVerifyBtn");
      await page.waitForTimeout(600);
    }
    await expect(page.locator("#itemList .item-empty")).toBeVisible();
    await page.waitForTimeout(400);
    const check = expectSameAsBaseline(page, "empty.png");
    await check();
  });
});
