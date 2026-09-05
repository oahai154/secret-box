# 08 — `--db` 便携模式 + Windows 打包

**What to build:**
支持 `--db` 参数指定数据库文件位置（便携模式）。`tauri build` 产出单文件 Windows exe，双击即用，无浏览器、无终端窗口。`--port`、`--no-open` 随旧架构一并消失。

**Blocked by:** 07 — 界面对照验收

**Status:** done（自动化部分全部通过；下方 4 项人工验收待人工执行）

**需要给人工说明操作步骤**

**人工验收任务（由人工执行，不由 agent 自动化操作软件）：**

- [ ] 人工执行打包，核对产出为单个 exe 文件
- [ ] 人工运行该 exe：`--db` 指向新路径时，解锁后到指定位置确认数据文件已生成
- [ ] 人工运行该 exe：不带 `--db` 时，到默认用户数据目录确认数据文件落在那里
- [ ] 人工双击启动体验：确认无终端窗口残留、无浏览器拉起

> 人工验收操作步骤：
> 1. 打包：在仓库根目录执行 `npm run tauri build`。产出核对：
>    - 便携单文件 exe：`target\release\secretbox.exe`（约 9.8 MB，自包含，双击即用；
>       WebView2 由 Windows 11 系统提供）。
>    - 附带安装包（可选使用）：`target\release\bundle\nsis\SecretBox_0.1.0_x64-setup.exe`
>      与 `target\release\bundle\msi\SecretBox_0.1.0_x64_en-US.msi`。
> 2. 便携模式：任选一个**新的空目录**（如 `D:\sb-portable`），在 cmd/资源管理器地址栏运行
>    `target\release\secretbox.exe --db D:\sb-portable\secretbox.db`。设置主密码并解锁后，
>    到 `D:\sb-portable` 确认 `secretbox.db` 已生成且随条目修改增长。
> 3. 默认目录：直接运行 `target\release\secretbox.exe`（不带参数），
>    到 `%LOCALAPPDATA%\SecretBox\` 确认数据文件落在那里。
>    ⚠️ 该目录当前已存在历史数据（secretbox.db），打开即是既有内容，属预期。
> 4. 双击体验：在资源管理器里双击 `secretbox.exe`，确认：无终端黑窗口残留、
>    无浏览器被拉起、窗口标题为 "SecretBox · 隐私保险箱"。

> 实施说明：
> - `--db` 参数解析（`--db <路径>` 与 `--db=<路径>`）早已存在于 `src-tauri/src/main.rs`
>   （`parse_db_arg`）；本票将其重构为接收参数迭代器的纯函数并补 5 个单元测试
>   （空格/等号分隔、忽略无关参数、缺失返回 None、默认路径优先 LOCALAPPDATA）。
> - 默认数据路径与 Go 版 `defaultDBPath` 一致：Windows 优先 `%LOCALAPPDATA%\SecretBox\secretbox.db`。
> - 无终端窗口：`#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`
>   已配置；release exe 经 `file` 验证为 PE32+ **GUI** 子系统。
> - 数据文件创建时机：`Db::open` 在应用启动时即建库（SQLite 建表），无需解锁；
>   因此自动化取证只需短跑进程后检查文件。
> - 自动化取证（本会话已完成，全部通过）：
>   - `--db` 便携模式：release exe 以 `--db .scratch/ticket08/portable/secretbox.db`
>     启动 8 秒后，该目录生成 secretbox.db + wal/shm。
>   - 默认路径解析：为不触碰真实数据目录 `%LOCALAPPDATA%\SecretBox`（已存在历史数据），
>     以 `LOCALAPPDATA=<临时目录>` 环境变量重定向启动 release exe，
>     确认 `<临时目录>\SecretBox\secretbox.db` 自动生成——证明默认路径解析逻辑正确；
>     真实目录时间戳未变化。
> - 打包命令 `npm run tauri build`（release 用 `frontendDist: ../dist`），同时产出
>   MSI 与 NSIS 安装包；便携单文件即 `target/release/secretbox.exe`。
> - 旧架构的 `--port`、`--no-open` 参数在 Rust 版中不存在（无 HTTP 服务器、无浏览器）。
> - 取证遗留：`.scratch/ticket08/portable/`（--db 取证产物，可删）。
