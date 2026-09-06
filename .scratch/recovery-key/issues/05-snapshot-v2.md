# 05 — 快照 v2：导出/导入走同一套包装

**What to build:**
导出的快照文件携带 DEK 的两份包装（主密码 KEK 与恢复密钥 KEK 各一份），主密码或恢复密钥都能导入恢复——备份不再随主密码遗忘而作废。v1 快照保持只读导入源身份（导入通道在 06 收尾前仍可用）。依据 ADR-0003。

**Blocked by:** 02 — 恢复密钥生成与强制确认

**Status:** done

> 快照导出/导入命令层（export_snapshot/import_snapshot）无需改动——
> 包装材料经 get_snapshot/restore_from_snapshot 透传。详见 dek.rs 测试。

> 实施说明：
> - `Snapshot` 结构新增三个 v2 字段（serde default 空，v1 旧快照照常反序列化）：
>   `recovery_salt`、`wrapped_dek_recovery`、`recovery_key_enc`——加上原有的
>   `salt`+`wrapped_dek`，快照完整携带主密码侧与恢复密钥侧全部包装材料。
> - `get_snapshot` 改为按 META_KEYS 清单透传；`restore_from_snapshot` 对非空
>   字段逐一写回 meta。快照与主库格式同构，条目仍是 DEK 密文，无明文/弱熵秘密。
> - 导出的快照外层仍由迁移口令加密（build_file/parse_file 不变）；导入后主密码
>   与恢复密钥两种凭据都能解锁——备份不随主密码遗忘而作废。
> - 测试：核心 `dek.rs` 新增 `v2快照双凭据导入回环`（断言三字段非空 + 双凭据
>   分别导入逐字段一致 + 恢复密钥明文加密行随快照迁移）；v1 快照回环
>   （snapshot.rs）与 Tauri 导出导入测试原样通过。

- [x] 导出的 v2 快照包含 DEK 的主密码侧与恢复密钥侧两份包装
- [x] 用主密码导入 v2 快照：全部条目与历史版本逐字段一致
- [x] 用恢复密钥导入 v2 快照（主密码已遗忘场景）：数据完整恢复
- [x] 快照与主库格式同构，不含明文或弱熵秘密
- [x] 集成测试：导出 → 清库 → 两种凭据分别导入 → 逐字段比对一致
