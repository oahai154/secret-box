# 02 — 解锁/锁定会话 + 条目列表（只读）

**What to build:**
完整的主密码解锁界面与条目列表（只读）。视觉对照 Go 版 `web/` 现有样式移植（美术基准，见 ADR-0001）。支持手动锁定与 120 秒无操作自动锁定；锁定后必须重新输入主密码。

**验证基建（本票一并落地）：**
- 前端将 Tauri IPC 调用抽象为可替换接口，测试时注入 mock，使 Svelte 应用可在普通浏览器中由 Playwright 驱动。
- 用 Playwright 对 Go 版 `web/` 页面截取基准图，建立截图对比机制。

**Blocked by:** 01 — 加密核心移植 + 打开现有数据库

**Status:** done

> 实施说明：Playwright 全部走 mock IPC 驱动 Svelte 应用（`window.__SECRETBOX_IPC__` 注入点），
> 后端语义由 Rust 单元测试 + 真实窗口桌面自动化冒烟覆盖（解锁/锁定/自动锁定）。
> 像素对比允许 1% 差异（不同渲染引擎无法逐像素一致），差异图写入 test-results 供审查。

- [x] Playwright：错误主密码显示错误提示且不进入应用；正确密码进入条目列表
- [x] Playwright：条目列表与 Go 版基准截图像素对比通过（解锁页 + 列表页）
- [x] Playwright（fake timer）：无操作 120 秒后自动锁定；手动锁定后需重新解锁
- [x] 锁定状态下内存中的明文数据被清空（Rust 侧测试断言）
