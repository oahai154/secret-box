import { expect, test } from "@playwright/test";
import * as fs from "fs";
import * as path from "path";
import pixelmatch from "pixelmatch";
import { PNG } from "pngjs";
import { injectMockIpc } from "./helpers";

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
    await page.fill("#authPassword", "golden-test-password");
    await page.click("#authBtn");
    await expect(page.locator("#itemList .item")).toHaveCount(3);
    await expect(page.locator("#itemTitle")).not.toHaveValue("");
    // 等 toast（已解锁）消失后再截图，两侧一致
    await page.waitForTimeout(2500);
    const check = expectSameAsBaseline(page, "main.png");
    await check();
  });
});
