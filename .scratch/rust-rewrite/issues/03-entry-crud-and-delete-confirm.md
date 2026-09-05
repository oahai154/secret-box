# 03 — 条目增删改查 + 删除主密码确认

**What to build:**
条目的新建、编辑、删除全流程（含分类、备注），删除条目时要求输入主密码确认。写路径第一次贯通：Rust 侧写入的数据库必须仍能被 Go 版读取（格式兼容）。

**Blocked by:** 02 — 解锁/锁定会话 + 条目列表

**Status:** done

> 实施说明：删除验证走独立 verify_password 命令（重新派生校验，不改变会话），
> 交互为自定义确认弹窗 + 验证密码弹窗（Go 版用 window.confirm，WebView 中不可靠，视觉基准不含弹窗态）。
> 跨语言回环：SECRETBOX_CRUD_OUT 驱动 Rust 写路径测试，SECRETBOX_VERIFY_EXPECT 驱动 Go 读回比对。

- [x] Rust 集成测试：对 fixture 数据库副本做增/改/删后，Go 测试工具能读回并验证数据一致（跨语言回环）
- [x] Rust 集成测试：删除条目必须验证主密码，错误密码拒绝
- [x] Playwright：新建、编辑条目流程完整可用；截图与 Go 版基准对比通过
- [x] Playwright：删除流程必须出现主密码确认，取消/输错均不删除
