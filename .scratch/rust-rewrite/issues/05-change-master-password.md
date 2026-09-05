# 05 — 修改主密码（全量重加密）

**What to build:**
用户在设置中修改主密码，全部数据（条目 + 历史版本）用新主密码重新加密。改密后的数据库仍与 Go 版格式兼容。

**Blocked by:** 02 — 解锁/锁定会话 + 条目列表

**Status:** done

> 实施说明：
> - 核心新增 `Db::change_password(old_key, new_password)`：与 Go 版 ChangePassword 一致——
>   新随机盐（Rust 侧 32 字节，Go 侧读 meta 表的 base64 盐值，长度无关兼容）、
>   事务内重加密全部条目 + 历史版本、更新 meta 盐值、返回新密钥。
> - 命令层 `change_password(old_password, new_password)` 先用旧密码 `db.unlock` 重新派生校验
>   （Go 版 handler 未校验旧密码，Rust 侧补上；空旧密码/新密码 <4 位直接拒绝），
>   成功后把会话密钥换成新密码派生的密钥。
> - 前端新增 `ChangePasswordModal.svelte`（逐字复刻 Go 版 changePasswordModal 结构，
>   card-password 类已在美术基准 style.css 中），设置弹窗的"修改"按钮接线后关闭设置弹窗。
> - Playwright mock 同步支持 changePassword（错误旧密码抛"解密失败(主密码可能不正确)"）。
> - 验收：SECRETBOX_CRUD_OUT 跑 `--test password` 写盘 + Go 以新密码
>   `SECRETBOX_VERIFY_PASSWORD="new-pass-#05"` 读回，3 条目逐字段一致。

- [x] Rust 集成测试：改密后用旧密码解锁失败、新密码解锁成功且数据完整
- [x] 跨语言验证：Go 测试工具能用新主密码打开改密后的数据库并读出一致数据（兼容性硬证据）
- [x] Playwright：改密 UI 流程（含旧密码验证、两次新密码确认）；截图与 Go 版基准对比通过
