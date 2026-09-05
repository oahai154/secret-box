# 01 — 加密核心移植 + 打开现有数据库（曳光弹）

**What to build:**
用户双击启动 Rust 桌面应用（Tauri 2 + Svelte），看到原生窗口（无浏览器、无终端）。输入**现有**主密码后，应用直接解锁现有本地数据库，并在窗口中显示已解密的条目数量。加密层（scrypt 密钥派生 + AES-256-GCM）与 Go 版逐字节兼容（见 ADR-0002），安全敏感逻辑全部在 Rust 侧。

**验证基建（本票一并落地）：**
- 用 Go 版生成跨语言黄金样本：测试密码（非真实密码）+ fixture 数据库 + 期望明文 JSON，提交进仓库。
- Rust 侧以核心 crate 组织加解密与存储，使 `cargo test` 可脱离窗口运行。

**Blocked by:** None — can start immediately

**Status:** done

- [x] `cargo test` 通过：对黄金样本完成 KDF 派生与解密，解密结果与期望明文 JSON 逐字段一致
- [x] 加密导出/导入所需的全部加密原语（加解密、nonce 布局）与 Go 版行为一致（差分断言）
- [x] Tauri 2 + Svelte 项目脚手架就绪，`npm run build` 与 `cargo build` 均绿
- [x] 实际窗口可启动：自动化桌面操作完成一次真实解锁流程，窗口正确显示条目数
