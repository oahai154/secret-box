# 01 — v2 核心架构：DEK 生成与密钥包装

**What to build:**
首次设置时生成随机数据加密密钥（DEK）用于加密全部条目和历史版本；DEK 由主密码派生的 KEK 包装后落盘。解锁从"派生密钥"变为"解开 DEK 包装"，修改主密码从全库重加密降为秒级重包装（旧改密流程的逐字段重加密行为随之废除）。依据 ADR-0003。

**Blocked by:** None — can start immediately

**Status:** done

> 实施说明：
> - `crypto.rs` 新增 `encrypt_bytes`/`decrypt_bytes`（base64(nonce||ct)，包装 DEK 用）与
>   `random_bytes`；条目加解密 `encrypt`/`decrypt` 签名与密文布局不变，改为其薄封装。
> - `db.rs`：meta 表新增 key=`wrapped_dek`（主密码 KEK 包装的 DEK，base64）。
>   `setup_master_password` 生成随机 DEK + 包装落盘并返回 DEK 作为会话密钥；
>   `unlock` 优先解包装（GCM 标签即密码校验，空库也能识别错误密码——v1 做不到），
>   无包装行时退回 v1 直接派生路径（只读导入源，供 06 升级向导与旧快照导入）；
>   `change_password` 变为"新盐 + 新 KEK 重包装"，返回原 DEK，v1 旧库报
>   `LegacyReadOnly` 拒绝。
> - `migration.rs`：`Snapshot` 增加 `wrapped_dek` 字段（serde default 空，兼容 v1 旧
>   快照），导出/导入透传，保证 v2 快照回环后凭原主密码可解锁。
> - 测试：`password.rs` 重写为 v2 语义（改密后密文逐字节不变为硬断言）；
>   新增 `dek.rs`（空库错误密码、DEK 不随改密变化、v2 快照回环）；
>   Tauri 层改密测试移到全新 v2 库上，另加 `v1旧库改密被拒绝_只读导入源`。
>   golden 系测试（v1 fixture）经只读回退路径原样通过。

- [x] 首次设置生成随机 DEK，条目/历史版本全部由 DEK 加解密，meta 表保存主密码 KEK 包装的 DEK
- [x] 主密码解锁 = 解包装成功；错误主密码解包装失败并提示"主密码可能不正确"
- [x] 修改主密码只重新包装 DEK，不触碰条目密文；旧密码解锁失败、新密码解锁成功且数据完整
- [x] 加解密条目的核心函数签名不变，条目密文布局不变
- [x] 集成测试：建库 → 解锁 → 增删改条目 → 改主密码 → 用新密码解锁读回逐字段一致
