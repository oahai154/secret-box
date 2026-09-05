// 生成 Go 版基准截图（美术基准）：运行 Go 版本并对解锁页/列表页截图。
// 运行方式: npm run baseline
// 结果写入 tests/e2e/baselines/{auth,main}.png，提交进仓库供像素对比。
import { chromium } from "@playwright/test";
import { spawn, execSync } from "child_process";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import { fileURLToPath } from "url";

const ROOT = fileURLToPath(new URL("../..", import.meta.url));
const PORT = 8273;
const BASE_URL = `http://127.0.0.1:${PORT}`;
const PASSWORD = "golden-test-password";

const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "secretbox-baseline-"));
const dbPath = path.join(tmpDir, "golden.db");
fs.copyFileSync(
  path.join(ROOT, "crates", "secretbox-core", "tests", "fixtures", "golden.db"),
  dbPath,
);

console.log("编译 Go 版本…");
execSync("go build -o .scratch/secretbox-go.exe .", { cwd: ROOT, stdio: "inherit" });

console.log("启动 Go 版本…");
const server = spawn(path.join(ROOT, ".scratch", "secretbox-go.exe"), [
  "--db",
  dbPath,
  "--port",
  String(PORT),
  "--no-open",
], { cwd: ROOT, stdio: "ignore" });

async function waitForServer() {
  for (let i = 0; i < 60; i++) {
    try {
      const res = await fetch(`${BASE_URL}/api/status`);
      if (res.ok) return;
    } catch {
      /* 尚未就绪 */
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error("Go 版本服务启动超时");
}

try {
  await waitForServer();

  const browser = await chromium.launch();
  const context = await browser.newContext({
    viewport: { width: 1280, height: 800 },
    deviceScaleFactor: 1,
    colorScheme: "light",
    reducedMotion: "reduce",
  });
  const page = await context.newPage();

  await page.goto(BASE_URL);
  await page.waitForSelector(".auth-card");
  // 与 Go 版 init() 完成后的文案一致
  await page.waitForFunction(
    () => document.querySelector("#authBtn")?.textContent === "解锁",
  );
  await page.screenshot({ path: path.join(ROOT, "tests", "e2e", "baselines", "auth.png") });
  console.log("已截取解锁页基准 auth.png");

  await page.fill("#authPassword", PASSWORD);
  await page.click("#authBtn");
  await page.waitForSelector("#itemList .item");
  await page.waitForSelector("#itemTitle");
  // 等 toast（已解锁）消失后再截图
  await page.waitForTimeout(2500);
  await page.screenshot({ path: path.join(ROOT, "tests", "e2e", "baselines", "main.png") });
  console.log("已截取列表页基准 main.png");

  // 新增模式（清空编辑区）
  await page.click("#newBtn");
  await page.waitForFunction(() => document.querySelector("#itemMeta")?.textContent === "新条目");
  await page.waitForTimeout(400);
  await page.screenshot({ path: path.join(ROOT, "tests", "e2e", "baselines", "new.png") });
  console.log("已截取新增页基准 new.png");

  // 保密内容显示态（点击显示）
  await page.click("#itemList .item:nth-child(1)");
  await page.waitForSelector("#toggleValueBtn");
  await page.click("#toggleValueBtn");
  await page.waitForTimeout(300);
  await page.screenshot({ path: path.join(ROOT, "tests", "e2e", "baselines", "value-visible.png") });
  console.log("已截取保密内容显示态基准 value-visible.png");

  // 历史版本面板（公司邮箱有 3 个版本，滚动到面板）
  await page.click("#itemList .item:nth-child(2)");
  await page.waitForSelector("#historyPanel:not(.hidden)");
  await page.locator("#historyPanel").scrollIntoViewIfNeeded();
  await page.waitForTimeout(300);
  await page.screenshot({ path: path.join(ROOT, "tests", "e2e", "baselines", "history.png") });
  console.log("已截取历史版本页基准 history.png");

  // 设置弹窗（先恢复到条目 1 + 滚动复位，保证弹窗背后的页面状态两侧一致）
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
  await page.waitForSelector("#settingsModal:not(.hidden)");
  await page.waitForTimeout(300);
  await page.screenshot({ path: path.join(ROOT, "tests", "e2e", "baselines", "settings.png") });
  console.log("已截取设置弹窗基准 settings.png");

  // 修改密码弹窗
  await page.click("#changePasswordBtn");
  await page.waitForSelector("#changePasswordModal:not(.hidden)");
  await page.waitForTimeout(300);
  await page.screenshot({ path: path.join(ROOT, "tests", "e2e", "baselines", "change-password.png") });
  console.log("已截取修改密码弹窗基准 change-password.png");

  // 数据备份与迁移弹窗
  await page.click("#cancelPasswordBtn");
  await page.click("#settingsCloseBtn");
  await page.waitForTimeout(200);
  await page.click("#dbBtn");
  await page.waitForSelector("#dbModal:not(.hidden)");
  await page.waitForTimeout(300);
  await page.screenshot({ path: path.join(ROOT, "tests", "e2e", "baselines", "db.png") });
  console.log("已截取数据备份弹窗基准 db.png");

  // 空列表态：逐个删除全部条目（删除确认走原生 confirm，删除需验证主密码）
  await page.on("dialog", (dialog) => dialog.accept());
  await page.click("#dbCloseBtn");
  const deleteAll = async () => {
    while (await page.locator("#itemList .item").count()) {
      await page.click("#itemList .item:nth-child(1)");
      await page.waitForTimeout(300);
      await page.click("#deleteBtn");
      await page.waitForSelector("#verifyPasswordModal:not(.hidden)");
      await page.fill("#verifyPasswordInput", PASSWORD);
      await page.click("#confirmVerifyBtn");
      await page.waitForTimeout(600);
    }
  };
  await deleteAll();
  await page.waitForSelector("#itemList .item-empty");
  await page.waitForTimeout(400);
  await page.screenshot({ path: path.join(ROOT, "tests", "e2e", "baselines", "empty.png") });
  console.log("已截取空列表态基准 empty.png");

  await browser.close();
} finally {
  try {
    execSync(`taskkill /PID ${server.pid} /T /F`, { stdio: "ignore" });
  } catch {
    /* 进程可能已退出 */
  }
}
console.log("基准截图完成");
