# SecretBox

本地加密密码管理器（Rust + Tauri 桌面应用）。数据使用 **scrypt 密钥派生 + AES-256-GCM** 加密后存入 SQLite，主密码不落盘；双击即用，无需浏览器、无网络依赖。

## 功能介绍

- **主密码保护** — 首次设置主密码后，所有条目加密存储；每次启动需解锁，支持锁定与自动锁定（默认 120 秒）
- **条目管理** — 新增/编辑/删除密码、账号等条目，支持分类与备注
- **版本历史** — 每次修改自动保存历史版本，可随时查看、恢复或删除旧版本
- **数据迁移** — 加密导出/导入完整数据快照（含历史版本），导出后可一键清除本地痕迹
- **安全设置** — 修改主密码（自动重加密全部条目）；可要求删除条目/版本时输入主密码确认

## 安装

从发布产物获取单个 exe（`SecretBox.exe`），双击即可运行。桌面渲染依赖 WebView2，Windows 11 已内置，Windows 10 如缺失系统会自动提示安装。

自行构建：

```bash
npm install
npm run tauri build
```

产物位置：

| 产物 | 路径 |
| --- | --- |
| 便携单文件 exe | `target\release\secretbox.exe` |
| NSIS 安装包 | `target\release\bundle\nsis\SecretBox_0.1.0_x64-setup.exe` |
| MSI 安装包 | `target\release\bundle\msi\SecretBox_0.1.0_x64_en-US.msi` |

## 便携模式

默认情况下，数据库存放在用户数据目录（Windows：`%LOCALAPPDATA%\SecretBox\secretbox.db`）。

用 `--db` 参数指定任意位置即可进入便携模式，数据文件随指定目录走（例如放在 U 盘里随身携带）：

```bash
secretbox.exe --db D:\sb-portable\secretbox.db
# 等价写法
secretbox.exe --db=D:\sb-portable\secretbox.db
```

## 数据安全

- 主密码从不存储；忘记主密码即无法恢复数据，请务必牢记
- 数据文件、导出的快照均为 AES-256-GCM 加密，离开主密码不可读
- 删除条目/版本时可要求输入主密码二次确认（设置中开启）
