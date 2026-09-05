# 10 — 删除 Go 版代码 + README 重写

**What to build:**
界面验收通过后（ADR-0002 的删除前提），仓库根目录只保留 Rust 版。README 重写为桌面版文档（安装、便携模式、功能说明）。Go 版仅存于 git 历史。

**Blocked by:** 07 — 界面对照验收（#09 不阻塞本票）

**Status:** done

**AI自主验收：**

- [x] 自动断言：仓库无 `*.go` 残留、go.mod/go.sum 及 Go 版二进制产物已移除
- [x] `cargo test` 与前端测试全绿，`tauri build` 正常
- [x] README 为桌面版文档，`CONTEXT.md` 词汇表与 ADR 无需改动即仍准确

> 实施说明：
> - 删除：9 个 Go 源文件（含 `golden_fixture_test.go` 跨语言回环工具）、go.mod/go.sum、
>   `web/`（Go 版前端；美术基准已逐字拷贝进 `src/style.css`，截图基准已固化在
>   `tests/e2e/baselines/`）、`tests/e2e/capture-baselines.mjs` 与 `npm run baseline`
>   脚本（Go 基准重生成流程随之消失，Go 版仅存于 git 历史）。
> - 删除根目录 Go 二进制：`secretbox.exe`、`secretbox-windows-amd64.exe`、
>   `secretbox-linux-amd64`、`.scratch/secretbox-go.exe`，及冒烟遗留
>   `.scratch/{smoke,ticket05,ticket06,e2e}/`。
> - README 重写为桌面版文档：功能说明、安装（构建命令与产物路径）、便携模式（`--db`）、
>   数据安全。`CONTEXT.md` 与 ADR 无需改动（ADR 中对 Go 的提及是历史决策记录，仍准确）。
> - 验收重跑：`cargo test` 全绿、`npm test` 28/28 全绿（视觉对照仍用既有 Go 基准截图）、
>   `npm run tauri build` 正常产出。
> - 根目录 `secretbox.db`（真实用户数据）未触碰，继续留在本地不提交。
