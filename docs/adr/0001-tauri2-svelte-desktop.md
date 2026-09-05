# 用 Tauri 2 + Svelte 重构为原生桌面应用

将 SecretBox 从 Go（本地 HTTP 服务 + 浏览器）重构为 Tauri 2 桌面应用：Rust 承接全部安全敏感逻辑（加解密、存储、业务规则），前端用 Svelte 5 + TypeScript + Vite 重写，仅承担界面。

## Considered Options

- **纯 Rust GUI（egui / iced / Slint）**：最"原生"，但界面需从零重画，"美术零降级"无法保证，现有交互逻辑全部重写。拒绝。
- **保持 Go 架构、仅 Rust 重写后端**：工作量最小，但"启动弹浏览器 + 终端窗口"这一核心痛点原样保留。拒绝。
- **Tauri 2（选定）**：窗口原生、渲染走系统 WebView，单 exe 约 3–5MB（Go 版 15MB），是唯一同时满足"轻量、美术可对照移植、代码主体是用户读得懂的 Rust"的选项。

## Consequences

- 界面仍是 WebView 渲染而非原生控件——用户接受此折中，验收基准是现有 `web/` 样式的对照移植。
- 前端从原生 JS 重写为 Svelte，"美术不降级"从结构保证变为移植保证，需截图对照验收。
- Go 版的浏览器拉起逻辑（`browser_windows.go` 等）随架构消失，不再需要。
