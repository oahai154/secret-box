# AGENTS.md

SecretBox：本地加密密码管理器（Tauri 2 桌面应用，Windows 为主目标平台）。Rust 承接全部安全敏感逻辑，Svelte 5 前端只做界面。存储为 v2 格式：条目由随机 DEK 加密，DEK 由主密码与恢复密钥两把钥匙分别包装（见 ADR-0003）。

## 常用命令

```bash
npm run dev          # Vite dev server（端口 5173，strictPort——Tauri 依赖固定端口）
npm run check        # svelte-check 类型检查
npm test             # Playwright e2e（自动起 dev server，浏览器内跑，IPC 走 mock）
npm run visual       # 仅视觉对照用例
RECAPTURE=1 npx playwright test recapture.spec.ts   # UI 有意变更后重截视觉基准图
cargo test           # Rust 全部测试（workspace：secretbox-core + src-tauri）
npm run tauri build  # 桌面打包，产物在 target/release/
```

## 目录与分层

- `crates/secretbox-core/` — 核心库：crypto（scrypt + AES-256-GCM）、db（SQLite 存储与全部业务规则）、migration（快照导出/导入）、recovery（恢复密钥）。加解密、密钥派生、存储逻辑只许改这里。
- `src-tauri/src/lib.rs` — Tauri 应用层，只做参数搬运。约定：命令实现拆成 `*_impl` 纯函数（可单测），`#[tauri::command]` 只是薄包装；新增命令须同时注册进 `generate_handler!`。
- `src/` — Svelte 前端。`src/ipc.ts` 是唯一 IPC 抽象层：组件不直接 import Tauri API，接口字段须与 Rust 侧 `secretbox_core::Item` 的 snake_case JSON 字段一致。
- `tests/e2e/` — Playwright 用例。`helpers.ts` 的 mock IPC 必须与 Rust 后端语义保持一致（错误文案、行为），改后端语义时同步改 mock。
- `docs/adr/` — 架构决策记录；`CONTEXT.md` — 领域术语表（改代码前先对齐术语，如"恢复密钥"不叫"恢复码"）；`.trae/specs/` — 原始构建规格。

## 安全不变量（改动前必读 ADR-0003）

- 主密码与恢复密钥明文从不落盘；解锁后的 DEK 只存 `AppState` 内存（Mutex 包裹）。
- 锁定态必须拒绝一切明文读取（返回"未解锁"），不得有任何旁路。
- v1 旧库与 v1 快照是**只读导入源**：v1 写路径已删除，不留兼容写后门；升级走强制向导（建新库→导入→归档旧文件，失败回滚）。
- 改主密码 = 只重新包装 DEK，条目密文不动；重新生成恢复密钥后旧码立即作废。
- 恢复密钥与主密码地位平等，两把都丢失则数据不可恢复——UI 文案必须如实告知。

## 约定与坑

- 全仓中文注释、中文测试名、`feat:`/`chore:` 前缀中文提交信息。
- `unlock` 命令必须是 `async fn`：scrypt 校验耗时数百毫秒，同步命令会占住主线程导致 Windows 上键盘消息无法泵入 WebView（打不了字）。
- `apply_window_theme_impl` 里的窗口边框/标题栏颜色是硬编码 COLORREF（字节序 0x00BBGGRR），改 `src/style.css` 主题色时需手动同步。
- 视觉基准图在 `tests/e2e/baselines/`，允许 1% 像素差异；UI 有意变更后用 RECAPTURE 重截并连同 PNG 一起提交。
- 黄金样本 fixture：`crates/secretbox-core/tests/fixtures/golden.db`（主密码 `golden-test-password`）与 `expected.json` 是公开测试数据，测试须复制到临时目录再用，绝不直接改 fixture。
- Windows 开发机上杀毒软件可能瞬时锁文件——涉及文件改名/复制的代码用带重试的循环，测试里同理。
- `*.db`/`*.db-shm`/`*.db-wal` 已 gitignore（fixture 白名单除外）；仓库根目录的 `secretbox.db*` 是本地开发数据，不要提交也不要删除。
- Playwright 环境固定浅色主题 + 禁用动画 + 视口 1280×800 + deviceScaleFactor 1，这是视觉对照可比的前提，不要改。
