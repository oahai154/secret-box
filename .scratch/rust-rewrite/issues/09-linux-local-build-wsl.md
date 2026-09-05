# 09 — Linux 本地构建产物（WSL）【暂时跳过】

**本次跳过，不安排执行；完成后直接进入 10。**

**What to build（暂缓）:**
不依赖 GitHub Actions：在本机 WSL2 Ubuntu 24.04 中构建 Tauri Linux 产物（二进制 + `.deb` 安装包），补齐平台覆盖。

**Blocked by:** 08 — `--db` 便携模式 + Windows 打包

**Status:** skipped（暂时跳过）

如后续需要 Linux 产物，以下工作全部下发给人工操作：

- [ ] 人工在 WSL Ubuntu 24.04 中安装 Tauri Linux 系统依赖（WebKITGTK 等）
- [ ] 人工执行 `cargo/前端` 构建，产出 Linux 二进制与 `.deb`，并拷回 Windows 侧
- [ ] 人工启动产物，完成一次解锁流程冒烟验证
- [ ] 人工在 WSL 内跑 `cargo test` 与前端测试，确认 Linux 侧回归全绿
