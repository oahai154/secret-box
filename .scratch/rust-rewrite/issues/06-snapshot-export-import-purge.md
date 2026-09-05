# 06 — 快照导出 / 导入 + 清除痕迹

**What to build:**
加密导出包含全部条目及历史版本的快照文件；导入快照恢复数据；导出后一键清除本地痕迹（删除本地全部数据含历史）。快照跨版本互通：Go 版导出的快照 Rust 版能导入，反之亦然。

**Blocked by:** 04 — 历史版本：查看 / 恢复 / 删除

**Status:** done

> 实施说明：
> - 迁移文件格式（core 新增 `migration.rs`）：与 Go handlers.go 完全一致——
>   `.secretbox` 内容 = base64( JSON{version:1, salt, cipher} )，cipher = 迁移口令派生密钥
>   加密的 Snapshot JSON；`backup_filename()` 产出 `secretbox-backup-<时间戳>.secretbox`。
> - core `Db` 新增 `get_snapshot`（密文原样导出 + meta 盐）、`restore_from_snapshot`
>   （覆盖式还原 + 重读盐值）、`wipe_and_remove_files`（清表后关闭连接并删除 .db/-wal/-shm，
>   带杀毒重试）。快照结构与 Go 字段名一致（has_password/salt/items/created/updated/...）。
> - 命令层：`export_snapshot` / `import_snapshot`（成功后弃用会话，与 Go 一致）、
>   `wipe`（清表 + 删文件 + 清会话键）、`save_snapshot_file`（rfd 原生"另存为"，取消返回空串）。
>   清除痕迹后 `AppState.db` 置 None，`get_status` 报未设置主密码，`setup_password` 自动重建库。
> - 前端：`DbModal.svelte`（复刻 Go dbModal）、`PromptModal.svelte`（替代不可靠的
>   window.prompt）；清除痕迹流程 = 主密码验证 → 强制导出 → ConfirmModal 二次确认。
>   `VerifyModal` 改为验证成功先关弹窗再执行动作（与 Go confirmVerify 一致），
>   避免"强制导出"弹窗叠在验证弹窗上。
> - 跨语言双向：Go 工具新增 `TestSnapshotExportTool` / `TestSnapshotImportTool`
>   （golden_fixture_test.go，环境变量门控）。双向回环均已跑通：Rust 导出 → Go 导入后
>   3 条目逐字段一致；Go 导出 → Rust 导入后与黄金样本 expected.json 逐字段一致。
> - Playwright mock：exportSnapshot / saveSnapshotFile / importSnapshot（口令 mock-import-pass，
>   错误抛"迁移口令错误或文件已损坏"）/ wipe。口令弹窗为 Promise 化单次输入，
>   流程中止后需重新发起（spec 中体现）。

- [x] 跨语言双向测试：Rust 导出的快照被 Go 测试工具导入且数据一致；Go 生成的快照 fixture 被 Rust 导入且数据一致
- [x] Rust 集成测试：清除痕迹后本地数据库文件（含 WAL/SHM）全部不存在
- [x] Playwright：导出（含选择保存位置 mock）、导入、清除痕迹确认流程；截图与 Go 版基准对比通过
