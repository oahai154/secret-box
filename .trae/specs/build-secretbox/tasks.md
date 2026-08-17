# Tasks

- [x] Task 1: 初始化 Go 项目与依赖
  - [x] 创建 go.mod,引入 modernc.org/sqlite 与 golang.org/x/crypto
  - [x] 搭建目录结构 main.go / db.go / crypto.go / handlers.go / web/
- [x] Task 2: 加密层(crypto.go)
  - [x] 实现 scrypt 从主密码派生 AES-256 密钥(含随机盐)
  - [x] 实现 AES-256-GCM 加密/解密(base64 编码输出)
  - [x] 单元测试验证加解密往返、错误密码解密失败
- [x] Task 3: 数据层(db.go)
  - [x] 初始化 SQLite,建 secret_items 与 secret_versions 两张表
  - [x] 实现条目 CRUD
  - [x] 实现版本快照记录(每次更新 +1)与按版本还原
- [x] Task 4: HTTP 服务与 API(handlers.go + main.go)
  - [x] 实现 /api/status、setup-password、unlock、lock 接口
  - [x] 实现 /api/items CRUD 与 /api/items/{id}/versions、restore 接口
  - [x] 集成 go:embed 服务静态文件;实现简单会话 token 校验
- [x] Task 5: 前端界面(web/index.html + style.css + app.js)
  - [x] 解锁/设置主密码页
  - [x] 主界面:左侧条目列表(搜索)+ 右侧编辑区 + 底部版本历史面板
  - [x] 新增/编辑/删除、查看/还原历史、锁定、退出生态
- [x] Task 6: 端到端联调与打包
  - [x] 启动服务在浏览器完整走通 "设置密码→新增→编辑(产生历史)→还原→锁定" 全流程
  - [x] go build 产出单个 .exe,验证双击可用不要额外文件
  - [x] 用浏览器插件核对网络请求与界面交互(关键端点均 2xx,UI 布局正常)

# Task Dependencies
- Task 5 依赖 Task 4
- Task 4 依赖 Task 2、Task 3
- Task 2、Task 3 可并行
- Task 6 依赖全部