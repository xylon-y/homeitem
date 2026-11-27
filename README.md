# homeitem

> 中文优先的家庭物品管理系统（Home Inventory）。单容器部署、纯 Rust 实现。

基于开源项目 [HomeBox](https://github.com/sysadminsmedia/homebox) 的功能设计，使用 Rust 技术栈重写实现。上游使用 Go + Vue + SQLite，本项目使用：

- **后端**：[Salvo](https://salvo.rs) 1.2（HTTP 框架）+ rusqlite（SQLite, bundled）+ r2d2 连接池 + Argon2id 密码哈希 + JWT（双令牌：API 令牌 / 媒体令牌）
- **前端**：[Leptos](https://leptos.dev) 0.8 CSR（WASM）+ Trunk 构建，界面默认中文
- **架构**：Cargo workspace（crates/common 共享 DTO / crates/backend / crates/frontend），前后端分离，后端同时托管前端静态产物，单容器部署
- **阻塞 IO**：所有 SQLite 操作经 tokio::spawn_blocking 封装，不阻塞异步运行时

## 上游来源与协议

- 功能设计与数据模型参考上游 [sysadminsmedia/homebox](https://github.com/sysadminsmedia/homebox)（物品/位置共用实体表、位置就近解析逻辑等均沿用上游行为）
- 上游协议为 **AGPL-3.0**，本项目遵循同一协议发布（见 LICENSE）
- 本项目代码为原创实现，未复制上游源码

## 功能

### 物品管理
- 物品与位置管理（共享实体表，树形位置视图，面包屑，位置就近解析）
- 物品新建 / 编辑 / 详情 / 删除 / 复制 / 归档，完整前端操作界面
- 自定义物品类型（entity types，含 CRUD 界面）
- 标签（含嵌套父子标签，含 CRUD 界面）
- 自定义字段
- 附件上传（multipart），照片自动设为主图，画廊展示与删除，媒体链接支持 7 天媒体令牌
- 保养/维护记录（scheduled / completed，含独立管理页面）
- 搜索与筛选：全文（名称/序列号/备注等）、标签、类型、位置、仅带照片、低库存、排序分页

### 提醒与统计
- 到期提醒：保修到期 / 自定义提醒 / 保养计划 / 库存不足（低库存阈值），聚合页展示
- 通知器管理（CRUD + 测试发送）：SMTP 邮件推送，后台定时扫描，按 (通知器, 物品, 日期) 去重
- 统计：物品总数 / 资产总值 / 按位置汇总 / 按标签汇总 / 采购时间线图表

### 账号与协作
- 账号密码登录（Argon2id，支持用户名或邮箱），保持登录 30 天，邀请码注册页
- 密码修改后自动撤销所有旧令牌（token version 机制）
- 多租户组：组内成员角色（user / owner），成员管理、邀请令牌、组设置界面

## 开发

依赖：Rust 1.85+（含 wasm32-unknown-unknown target）、[Trunk](https://trunkrs.dev)

```bash
# 克隆后直接运行后端（dev 模式前端产物用占位页）
cargo run -p backend
# 或先构建前端再托管真实界面
cd crates/frontend
trunk serve              # 前端热重载开发（:3000）
trunk build --release    # 产物输出 crates/frontend/dist
cd ../..
cargo run -p backend     # :7745，自动托管 dist

# 测试
cargo test --workspace
```

### 环境变量

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| HOMEITEM_LISTEN_ADDR | 0.0.0.0:7745 | 监听地址 |
| HOMEITEM_DB_PATH | ./data/homeitem.db | SQLite 路径 |
| HOMEITEM_UPLOAD_DIR | ./data/uploads | 附件目录 |
| HOMEITEM_JWT_SECRET | 自动生成 | JWT 密钥（缺省持久化到 data/.jwt_secret） |
| HOMEITEM_ALLOW_REGISTRATION | false | 是否允许无邀请注册 |
| HOMEITEM_DEV_CORS | false | 开发用放行所有 CORS |
| HOMEITEM_MAX_UPLOAD_MB | 25 | 单附件上限（MB） |
| HOMEITEM_SMTP_URL | 空 | 如 smtp://user:pass@mail.example.com:587 |
| HOMEITEM_MAIL_FROM | homeitem@localhost | 发件人 |
| HOMEITEM_PUBLIC_BASE_URL | http://localhost:7745 | 邮件里的站点链接 |
| HOMEITEM_REMINDER_INTERVAL_MIN | 60 | 提醒扫描间隔（分钟） |

首次访问 http://localhost:7745/setup 初始化管理员账号。

## Docker 部署

```bash
docker compose up -d --build
# 访问 http://localhost:7745
```

数据卷挂载 ./data（SQLite 数据库 + 上传文件）。在 docker-compose.yml 中配置 SMTP 环境变量即可启用邮件推送。

## 项目结构

```
crates/
  common/      # 前后端共享 DTO、查询参数、错误码（serde camelCase 线协议）
  backend/     # Salvo API 服务器：db(SQLite+迁移) / auth / api / 静态托管 / 后台提醒
  frontend/    # Leptos CSR：登录/初始化/仪表盘/物品/位置/提醒/设置
Dockerfile     # 3 阶段：trunk 构建前端 → cargo 构建后端（嵌入前端）→ debian-slim 非 root
docker-compose.yml
```

## API 概览

前缀 /api/v1，JSON 线协议 camelCase。完整清单见 docs/DESIGN.md。

- GET /health  GET /status
- POST /setup  POST /login  POST /register
- GET|PUT /user  PUT /user/password
- GET|PUT /group  GET /group/members  DELETE /group/members/{id}  PATCH /group/members/{id}/role
- GET|POST /group/invitations  DELETE /group/invitations/{id}
- GET|POST /entities  GET|PUT|PATCH|DELETE /entities/{id}  POST /entities/{id}/duplicate  GET /entities/{id}/path
- GET|POST /entities/{id}/attachments  GET|DELETE /entities/{id}/attachments/{aid}
- GET|POST /entities/{id}/maintenance  PUT|DELETE /entities/{id}/maintenance/{mid}
- GET /locations/tree
- GET|POST /entity-types  PUT|DELETE /entity-types/{id}
- GET /tags  GET /tags/tree  POST /tags  PUT|DELETE /tags/{id}
- GET|POST /notifiers  PUT|DELETE /notifiers/{id}  POST /notifiers/{id}/test
- GET /reminders  GET /statistics  GET /statistics/by-location  GET /statistics/by-tag  GET /statistics/value-over-time
