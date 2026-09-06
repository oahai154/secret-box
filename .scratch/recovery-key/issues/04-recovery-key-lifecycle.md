# 04 — 设置页生命周期：查看与重生成恢复密钥

**What to build:**
解锁后在设置中可查看、复制当前恢复密钥；可重生成：重输主密码验证后生成新码并重新包装 DEK，旧码立即作废（旧包装行删除）。锁定状态下此信息不可见、不可达。依据 ADR-0003。

**Blocked by:** 02 — 恢复密钥生成与强制确认

**Status:** done

> 实施说明：
> - 关键设计：恢复密钥明文不可逆推（库里只存它包装 DEK 的密文），要"查看且与
>   生成时一致"必须把明文码**用 DEK 加密**存一份（meta `recovery_key_enc`）。
>   锁定态拿不到 DEK → 自然满足"锁定不暴露"，机制保证而非 UI 约束。
> - 核心层：`recovery.rs` 新增 `format_grouped`（归一化码的规范 8×4 分组形态，
>   生成/存盘/展示三处统一）；`set_recovery_key` 重置时同步更新明文加密行；
>   新增 `get_recovery_key(dek)`。
> - Tauri：`get_recovery_key`（require_unlocked，锁定返回"未解锁"）、
>   `regenerate_recovery_key(master_password)`（先验证主密码，生成新码重包装，
>   旧码立即作废，返回新码）。
> - 前端：SettingsModal 新增"恢复密钥"区——查看/复制/重生成（内联主密码输入 +
>   确认），重生成成功 Toast 提示"旧恢复密钥已作废"。
> - 测试：Tauri `恢复密钥生命周期_查看与重生成`（锁定不可看 → 查看一致 → 错误
>   主密码拒绝 → 新码≠旧码 → 旧码解锁 DEK 失败 → 新码走通救援）；
>   Playwright `recovery-settings.spec.ts` 三用例。视觉基线 settings.png 因新增
>   恢复密钥区有意偏离，用 recapture.spec.ts 重截（替代 .mjs 方案——直接复用
>   injectMockIpc，保证与 visual.spec 环境逐字节一致）。
> - 注：救援流程（03）在重生成后使用新码，已由 e2e 与 Tauri 测试衔接覆盖。

- [x] 解锁后设置页可查看/复制恢复密钥，内容与生成时一致
- [x] 重生成需验证主密码；旧恢复密钥随之失效，无法再用旧码解开 DEK 包装
- [x] 重生成后新码可正常走通救援流程（与 03 衔接）
- [x] 锁定状态下设置入口不暴露恢复密钥的任何信息
- [x] 前端测试：查看、复制、重生成（含主密码验证失败被拒）
