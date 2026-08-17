# SecretBox 隐私保险箱 Spec

## Why

用户需要一个本地运行的电脑小工具，用来存储隐私内容（应用密钥、账号密码等）。要求：
- 直接在文本上编辑和修改，使用体验为"记事本 + 保险箱"
- 记录每次修改时间
- 可还原某个历史版本，防止改错、避免失去以前的密码
- 运行时资源占用低（选定 Go 方案，内存 ~10-15MB，单文件部署）

## What Changes

- 使用 **Go** 构建单个可执行文件（.exe），内置 HTTP 服务 + SQLite + AES-256-GCM 加密
- 前端使用 **纯 HTML/CSS/JS（无构建工具）**，通过 `embed` 打包进二进制，编译后双击即用，浏览器打开即用
- 首次运行设置主密码，之后登录解锁（主密码派生加密密钥，数据落盘加密）
- 条目支持标题、分类、内容，直接编辑保存
- 每次保存自动创建版本快照，记录修改时间；支持查看历史版本并对任意版本**一键还原**
- 所有能力通过本地 REST API 暴露，方便 AI 自主开发与调试

## Impact

- Affected specs: 无（全新项目）
- Affected code: 本项目所有文件（后端 Go、前端 HTML/CSS/JS、数据层、加密层）

## 技术选型

| 项 | 选择 | 理由 |
|----|------|------|
| 语言/运行时 | Go | 内存占用量低,单二进制,AI 训练数据充分,方便调试 |
| HTTP | 标准库 net/http | 零依赖,go 自带,易调试 |
| 数据库 | SQLite (modernc.org/sqlite) | 纯 Go 无 CGO,嵌入式单文件,零配置 |
| 加密 | AES-256-GCM + scrypt (golang.org/x/crypto) | 密钥由主密码派生,数据落盘加密 |
| 前端 | 原生 HTML/CSS/JS | 无构建工具,改完刷浏览器即看,AI 易操作 |
| 静态资源 | go:embed | 前端打进二进制,单文件分发 |
| 包管理 | go mod + 标准工具 | AI 好操作 |

## 数据模型

### secret_items（主表）
| 字段 | 类型 | 说明 |
|------|------|------|
| id | INTEGER PK | 主键 |
| title | TEXT | 标题,如 "GitHub 密钥" |
| category | TEXT | 分类,如 "账号密码/API密钥" |
| encrypted_value | BLOB/TEXT | AES-256-GCM 加密后的内容(base64) |
| created_at | TEXT | 创建时间(ISO8601) |
| updated_at | TEXT | 最后修改时间(ISO8601) |

### secret_versions（版本历史表）
| 字段 | 类型 | 说明 |
|------|------|------|
| id | INTEGER PK | 主键 |
| secret_id | INTEGER FK | 关联 secret_items.id |
| version | INTEGER | 版本号(从 1 递增) |
| encrypted_snapshot | BLOB/TEXT | 该版本内容的加密快照 |
| created_at | TEXT | 该版本创建时间 |

## 目录结构

```
secretbox/
├── go.mod
├── main.go            # 入口,密钥派生与加载,HTTP 路由
├── db.go              # SQLite 初始化 + CRUD + 版本管理
├── crypto.go          # AES-256-GCM 加解密 + scrypt 派生
├── handlers.go        # REST API 处理器
└── web/
    ├── index.html     # 主界面
    ├── style.css      # 样式
    └── app.js         # 前端逻辑
```

## API 契约

| 方法 | 路径 | 说明 |
|------|------|------|
| GET  | /api/status            | 是否已设置主密码 |
| POST | /api/setup-password    | 首次设置主密码 `{password}` |
| POST | /api/unlock           | 登录解锁 `{password}`,返回会话 token |
| POST | /api/lock             | 锁定(清除内存密钥) |
| GET  | /api/items             | 列出全部条目(不含明文值) |
| POST | /api/items             | 新建条目 `{title,category,value}` |
| GET  | /api/items/{id}        | 读取单条(含解密明文值) |
| PUT  | /api/items/{id}        | 更新条目(自动创建新版本) `{title,category,value}` |
| DELETE | /api/items/{id}      | 删除条目(连同历史) |
| GET  | /api/items/{id}/versions | 该条目版本历史列表 |
| POST | /api/items/{id}/restore/{version} | 还原到指定版本 |

> 说明:会话基于内存保存的加密密钥(锁定时清空)。项目为本地单用户工具,采用简单 token 校验即可,不引入复杂认证框架。

## ADDED Requirements

### Requirement: 首次设置主密码并解锁
系统 SHALL 在首次运行时要求设置主密码,之后每次启动需输入主密码解锁才能读写数据。

#### Scenario: 首次使用
- **WHEN** 用户首次打开工具
- **THEN** 界面提示设置主密码；设置后使用 scrypt 派生加密密钥,并进入主界面

#### Scenario: 已设置过主密码
- **WHEN** 用户再次打开工具
- **THEN** 仅显示解锁界面,输入正确主密码后进入;错误则提示

### Requirement: 条目的增删改查与直接编辑
系统 SHALL 允许用户创建、查看、直接编辑、删除含隐私内容的条目。

#### Scenario: 新增/编辑条目
- **WHEN** 用户在列表点"新增"或在右侧编辑区修改后点击保存
- **THEN** 数据以加密形式写入 SQLite,并刷新列表与修改时间

### Requirement: 版本历史与还原
系统 SHALL 在每次保存更新条目时创建新的版本快照,记录修改时间；系统 SHALL 允许用户查看历史并还原到任意历史版本。

#### Scenario: 记录修改历史
- **WHEN** 用户更新某条目的内容
- **THEN** 系统为该条目新增一个版本快照,并记录该版本的修改时间

#### Scenario: 还原历史版本
- **WHEN** 用户在某条目的版本历史中选择一个旧版本并点击还原
- **THEN** 当前内容被替换为该旧版本内容,同时作为最新一次修改记录一条新版本(保留可操作性)

### Requirement: 数据加密存储
系统 SHALL 保证所有隐私内容以 AES-256-GCM 加密形式落盘,主密码不落盘,仅保存派生所需盐值。

#### Scenario: 查看数据库文件
- **WHEN** 用户或第三方直接查看 SQLite 文件
- **THEN** 无法看到任何明文隐私内容

### Requirement: 锁定
系统 SHALL 提供锁定功能,锁定后清除内存中的加密密钥,任何数据读取需重新解锁。

#### Scenario: 锁定工具
- **WHEN** 用户点击"锁定"
- **THEN** 界面回到解锁页,内存密钥被清除,无法读取/修改任何条目