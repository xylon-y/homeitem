# homeitem 设计文档（待确认）

> 本文档为编码前的设计确认稿。**确认后再进入编码阶段。**
>
> 上游来源：[sysadminsmedia/homebox](https://github.com/sysadminsmedia/homebox)
> 上游协议：**AGPL-3.0**（本项目作为衍生作品，同样以 AGPL-3.0 发布）

---

## 1. 项目定位与范围

homeitem 用**纯 Rust** 重构 HomeBox：家庭物品、位置、标签、附件、购买/保修/保养记录、到期提醒、搜索筛选、账号密码登录。
前后端分离，前端 Leptos CSR(WASM)，后端 Salvo，单容器部署，无 Node.js 工具链（Trunk 为 Rust 编写）。

### 1.1 本期实现（复刻范围）

| HomeBox 能力 | homeitem | 说明 |
|---|---|---|
| 物品（entities） | ✅ 完整 | 含自定义字段、父子层级、重复复制 |
| 位置（is_location 型 entity_type） | ✅ 完整 | 与 HomeBox 一致：物品/位置同一张表 |
| 标签（含子标签） | ✅ 完整 | 多对多、颜色、图标 |
| 附件上传（照片/手册/保修/收据） | ✅ 完整 | multipart、磁盘存储、缩略图不生成 |
| 库存数量 | ✅ | quantity + 最低库存阈值 + 统计 |
| 到期提醒 | ✅ | 保修到期 / 物品自定义到期 / 保养计划到期 / 低库存 |
| 搜索筛选 | ✅ | 关键词 + 标签 + 类型 + 位置 + 有无照片 + 归档 + 排序分页 |
| 账号密码登录 | ✅ | Argon2id + JWT |
| 统计 | ✅ | 总价值、按位置、按标签、保修数量 |
| 多用户/分组（group）/邀请 | ✅ 完整 | group 多租户 + 成员角色 + 邀请令牌，见 3.4 |
| CSV 导入导出、标签打印、Notifier、OIDC、API Key、模板、备份 | ❌ 不在本期 | 表结构预留扩展位 |

### 1.2 技术栈

| 层 | 选型 |
|---|---|
| 前端 | Leptos 0.8（CSR, wasm32-unknown-unknown）+ leptos_router + gloo-net，Trunk 构建 |
| 后端 | Salvo 1.2（最新稳定），tokio 运行时 |
| 数据库 | rusqlite 0.43（bundled SQLite 3）+ r2d2 连接池；所有阻塞调用经 `tokio::spawn_blocking` |
| 序列化 | serde / serde_json（camelCase 契约） |
| 密码 | argon2（Argon2id） |
| 令牌 | jsonwebtoken（HS256，双令牌：user / attachment） |
| 静态资源 | rust-embed 内嵌 dist，`HOMEITEM_STATIC_DIR` 可覆盖为磁盘目录（开发用） |

---

## 2. 仓库结构（Cargo workspace）

```
homeitem/
├─ Cargo.toml                 # [workspace] members = crates/*
├─ Cargo.lock
├─ LICENSE                    # AGPL-3.0
├─ README.md                  # 上游来源/协议/开发与部署命令
├─ .gitignore  .dockerignore
├─ Dockerfile                 # 多阶段：trunk 前端 → cargo 后端 → debian-slim 运行
├─ docker-compose.yml         # 数据卷挂载 db + uploads
├─ docs/DESIGN.md             # 本文档
└─ crates/
   ├─ common/                 # 前后端共享：DTO、枚举、查询参数、错误体、常量
   │  └─ src/lib.rs
   ├─ backend/
   │  ├─ build.rs             # 保证 dist 目录存在（rust-embed 编译期要求）
   │  └─ src/
   │     ├─ main.rs           # 启动、装配
   │     ├─ config.rs         # 环境变量配置
   │     ├─ error.rs          # AppError → HTTP 响应（统一 JSON 错误体）
   │     ├─ db/{mod,pool,migrations}.rs
   │     ├─ db/repo/{users,entities,tags,entity_types,attachments,maintenance,settings,stats}.rs
   │     ├─ auth/{password,jwt,guard}.rs
   │     ├─ api/{mod,routes}.rs
   │     ├─ api/handlers/{system,auth,users,entities,tags,entity_types,attachments,maintenance,reminders,statistics}.rs
   │     └─ assets.rs         # rust-embed + SPA fallback
   └─ frontend/
      ├─ Cargo.toml  Trunk.toml  index.html
      ├─ dist/                # trunk build 产物（git 忽略）
      └─ src/{main,app,api,auth,storage,error}.rs
         src/components/{layout,nav,item_card,tag_chip,pagination,modal,toast,...}.rs
         src/pages/{login,setup,dashboard,items_list,item_detail,item_form,tags,locations,reminders,settings}.rs
```

**分层原则**：`common` 不依赖任何 crate 内部模块，只依赖 serde/chrono/uuid；backend/frontend 互不依赖，仅共享 `common`。

---

## 3. 数据模型

### 3.1 核心建模决策

1. **物品与位置同表**（与 HomeBox 一致）：`entities` 表通过 `entity_type.is_location` 区分。位置也是 entity，因此可以嵌套、可以挂附件、可以改名——避免两套 CRUD。
2. **层级用 parent_id**：位置树 / 物品装箱（物品的 parent 可以是位置或物品）。
3. **位置解析规则**：`location_id` 显式覆盖 > 沿 parent 链向上取第一个 `is_location` 祖先。这是 HomeBox #1688 的行为。
4. **主键统一 TEXT + UUIDv4**：前后端无自增依赖，便于后续导入导出与离线同步。
5. **时间统一 TEXT ISO-8601 UTC**（`2025-01-31T08:00:00Z`），日期型字段允许 `YYYY-MM-DD`。SQLite 用 TEXT 便于 `BETWEEN` 比较与索引。
6. **金额 REAL**：与上游一致；后续可迁移为 INTEGER 分。
7. **JSON 契约 camelCase**；`primary` 是 SQL 关键字，列名用 `primary_photo`，API 字段仍为 `primary`。

### 3.2 表结构（SQLite DDL）

```sql
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;

-- 运行时设置（货币、注册开关、实例 ID 等）
CREATE TABLE app_settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

-- 用户
CREATE TABLE users (
  id            TEXT PRIMARY KEY,
  name          TEXT NOT NULL,
  email         TEXT NOT NULL COLLATE NOCASE,
  password_hash TEXT,                                  -- argon2id；OIDC 用户为 NULL
  is_superuser  INTEGER NOT NULL DEFAULT 0,
  activated_on  TEXT,
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_users_email ON users(email COLLATE NOCASE);

-- 物品类型 / 位置类型
CREATE TABLE entity_types (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  icon        TEXT NOT NULL DEFAULT '',
  is_location INTEGER NOT NULL DEFAULT 0,
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_entity_types_name ON entity_types(name COLLATE NOCASE);

-- 物品 + 位置（同表）
CREATE TABLE entities (
  id                           TEXT PRIMARY KEY,
  name                         TEXT NOT NULL,
  description                  TEXT NOT NULL DEFAULT '',
  quantity                     REAL NOT NULL DEFAULT 1,
  min_quantity                 REAL NOT NULL DEFAULT 0,       -- 库存下限，>0 时参与低库存提醒
  asset_id                     INTEGER NOT NULL DEFAULT 0,     -- 资产编号
  archived                     INTEGER NOT NULL DEFAULT 0,
  insured                      INTEGER NOT NULL DEFAULT 0,
  notes                        TEXT NOT NULL DEFAULT '',
  parent_id                    TEXT REFERENCES entities(id) ON DELETE CASCADE,
  location_id                  TEXT REFERENCES entities(id) ON DELETE SET NULL, -- 位置覆盖
  sync_child_entity_locations  INTEGER NOT NULL DEFAULT 0,
  entity_type_id               TEXT NOT NULL REFERENCES entity_types(id) ON DELETE RESTRICT,
  serial_number                TEXT NOT NULL DEFAULT '',
  model_number                 TEXT NOT NULL DEFAULT '',
  manufacturer                 TEXT NOT NULL DEFAULT '',
  lifetime_warranty            INTEGER NOT NULL DEFAULT 0,
  warranty_expires             TEXT,                            -- 保修到期
  warranty_details             TEXT NOT NULL DEFAULT '',
  purchase_date                TEXT,
  purchase_from                TEXT NOT NULL DEFAULT '',
  purchase_price               REAL NOT NULL DEFAULT 0,
  sold_date                    TEXT,
  sold_to                      TEXT NOT NULL DEFAULT '',
  sold_price                   REAL NOT NULL DEFAULT 0,
  sold_notes                   TEXT NOT NULL DEFAULT '',
  reminder_date                TEXT,                            -- 自定义到期日（提醒）
  reminder_note                TEXT NOT NULL DEFAULT '',
  import_ref                   TEXT NOT NULL DEFAULT '',
  created_at                   TEXT NOT NULL,
  updated_at                   TEXT NOT NULL
);
CREATE INDEX idx_entities_name        ON entities(name);
CREATE INDEX idx_entities_parent      ON entities(parent_id);
CREATE INDEX idx_entities_type_arch   ON entities(entity_type_id, archived);
CREATE INDEX idx_entities_asset       ON entities(asset_id);
CREATE INDEX idx_entities_warranty    ON entities(warranty_expires);
CREATE INDEX idx_entities_reminder    ON entities(reminder_date);
CREATE INDEX idx_entities_purchased   ON entities(purchase_date);

-- 标签（支持父子）
CREATE TABLE tags (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  color       TEXT NOT NULL DEFAULT '',
  icon        TEXT NOT NULL DEFAULT '',
  parent_id   TEXT REFERENCES tags(id) ON DELETE SET NULL,
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_tags_name ON tags(name COLLATE NOCASE);

CREATE TABLE entity_tags (
  entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  tag_id    TEXT NOT NULL REFERENCES tags(id)     ON DELETE CASCADE,
  PRIMARY KEY (entity_id, tag_id)
);
CREATE INDEX idx_entity_tags_tag ON entity_tags(tag_id);

-- 自定义字段
CREATE TABLE entity_fields (
  id            TEXT PRIMARY KEY,
  entity_id     TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  name          TEXT NOT NULL,
  type          TEXT NOT NULL DEFAULT 'text',   -- text | number | boolean
  text_value    TEXT NOT NULL DEFAULT '',
  number_value  REAL NOT NULL DEFAULT 0,
  boolean_value INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_entity_fields_entity ON entity_fields(entity_id);
CREATE INDEX idx_entity_fields_name   ON entity_fields(name);

-- 附件（元数据；文件落盘）
CREATE TABLE attachments (
  id            TEXT PRIMARY KEY,
  entity_id     TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  type          TEXT NOT NULL DEFAULT 'attachment', -- photo|manual|warranty|receipt|attachment
  title         TEXT NOT NULL DEFAULT '',
  file_name     TEXT NOT NULL DEFAULT '',
  path          TEXT NOT NULL,                       -- 相对 UPLOAD_DIR，如 <entity>/<id>.jpg
  mime_type     TEXT NOT NULL DEFAULT 'application/octet-stream',
  size_bytes    INTEGER NOT NULL DEFAULT 0,
  primary_photo INTEGER NOT NULL DEFAULT 0,
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL
);
CREATE INDEX idx_attachments_entity ON attachments(entity_id);

-- 保养 / 维修记录（scheduled_date 参与到期提醒）
CREATE TABLE maintenance_entries (
  id             TEXT PRIMARY KEY,
  entity_id      TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  name           TEXT NOT NULL,
  description    TEXT NOT NULL DEFAULT '',
  cost           REAL NOT NULL DEFAULT 0,
  completed_date TEXT,
  scheduled_date TEXT,
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL
);
CREATE INDEX idx_maint_entity    ON maintenance_entries(entity_id);
CREATE INDEX idx_maint_scheduled ON maintenance_entries(scheduled_date);
```

### 3.3 种子数据（首次迁移）

- entity_types：`Location(位置, is_location=1)`、`Item(物品, is_location=0)` —— 保证首个物品可直接创建。
- 不预置用户；首次启动进入 **/setup** 创建管理员。

### 3.4 用户模型（多租户）

与上游一致，homeitem 采用 **group 多租户**：每个用户归属一个 group，所有实体、标签、类型、附件、通知器、统计均按 `group_id` 隔离。
- 首次 `/setup` 创建 owner 及其默认 group，并播种默认物品/位置类型。
- 组内角色分 `owner` / `user`；owner 可管理成员、角色与邀请令牌。
- 注册需邀请令牌；`HOMEITEM_ALLOW_REGISTRATION=true` 时允许自助注册（自助注册者成为自己 group 的 owner）。
- JWT 携带 `gid` + `role`；鉴权中间件每次请求都会回查 `user_groups`，成员被移除或角色变更后旧令牌立即失效。
- `is_superuser` 为未来跨组管理能力预留，当前不参与常规鉴权。

> 注：早期设计稿曾计划简化为单租户，最终实现已采用完整 group 模型，本节与 README 为准。

---

## 4. API 清单

前缀 `/api/v1`。除标注 `public` 外均需 `Authorization: Bearer <token>`。
错误统一体：`{ "error": "message", "code": "NOT_FOUND", "fields": {"name": "..."} }`。
列表统一分页：`{ "items": [...], "page": 1, "pageSize": 25, "total": 123 }`。

### 4.1 系统 & 认证

| 方法 | 路径 | 说明 | 认证 |
|---|---|---|---|
| GET | `/status` | 版本、build、`needsSetup`、`registrationEnabled` | public |
| GET | `/currencies` | 货币码列表 | public |
| POST | `/users/setup` | 首次创建管理员（仅 needsSetup 时可用） | public |
| POST | `/users/login` | `{username,password,stayLoggedIn}` → `{token,attachmentToken,expiresAt}` | public |
| POST | `/users/register` | 注册（`HOMEITEM_ALLOW_REGISTRATION=true` 时开放） | public |
| POST | `/users/logout` | 无状态，客户端丢弃令牌（保留接口以兼容） | user |
| GET | `/users/self` | 当前用户 | user |
| PUT | `/users/self` | 改名/邮箱 | user |
| POST | `/users/self/change-password` | `{current,new}` | user |
| GET | `/users/self/stats` | 个人统计（预留） | user |

### 4.2 物品 / 位置（entities）

| 方法 | 路径 | 说明 |
|---|---|---|
| GET | `/entities` | 列表+筛选（见 4.3） |
| POST | `/entities` | 新建 |
| GET | `/entities/{id}` | 详情（含标签、字段、附件、父级、解析后的位置、itemCount） |
| PUT | `/entities/{id}` | 全量更新 |
| PATCH | `/entities/{id}` | 局部更新（quantity/tagIds/parentId/locationId/entityTypeId） |
| DELETE | `/entities/{id}` | 删除（级联子物品、字段、附件、保养记录；文件一并清理） |
| POST | `/entities/{id}/duplicate` | 复制（可选复制字段/保养/附件） |
| GET | `/entities/{id}/path` | 面包屑（根 → 自身） |
| GET | `/entities/tree` | 位置树（`withItems`、`withArchived`） |
| GET | `/entities/fields` | 全部自定义字段名 |
| GET | `/entities/fields/values` | 某字段的全部值（去重） |
| GET | `/entities/export` | CSV 导出 |

**位置**不设独立路由：`GET /entities?isLocation=true` 返回位置列表，`POST /entities` 配 `entityTypeId`（位置类型）即可创建位置。与上游一致。

### 4.3 筛选 / 排序 / 分页参数（GET /entities）

| 参数 | 类型 | 说明 |
|---|---|---|
| `q` | string | 关键词：name/description/serial/model/manufacturer/notes/asset_id LIKE |
| `page` / `pageSize` | int | 默认 1 / 25，pageSize ≤ 100 |
| `orderBy` | string | `name`\|`createdAt`\|`updatedAt`\|`purchasePrice`\|`warrantyExpires`\|`assetId` |
| `orderDirection` | string | `asc`\|`desc` |
| `includeArchived` | bool | 默认 false |
| `parentIds` | string[] | 逗号分隔 |
| `entityTypeIds` | string[] | 逗号分隔 |
| `tags` | string[] | 标签 ID |
| `negateTags` | bool | 排除这些标签 |
| `matchAllTags` | bool | 需同时具备所有标签 |
| `isLocation` | bool | true 只返回位置；false 只返回非位置 |
| `onlyWithPhoto` / `onlyWithoutPhoto` | bool | 主图过滤 |
| `lowStock` | bool | `quantity <= min_quantity AND min_quantity > 0` |
| `fields` | string[] | 自定义字段过滤（预留） |

### 4.4 标签

| 方法 | 路径 | 说明 |
|---|---|---|
| GET | `/tags` | 全部标签（含 children，一次返回；支持 `?withCounts=true` 带物品数） |
| POST | `/tags` | 新建 |
| GET | `/tags/{id}` | 详情 |
| PUT | `/tags/{id}` | 更新 |
| DELETE | `/tags/{id}` | 删除（解除与物品关联） |

### 4.5 物品类型

| 方法 | 路径 | 说明 |
|---|---|---|
| GET | `/entity-types` | 列表（`?isLocation=true` 过滤） |
| POST | `/entity-types` | 新建 |
| PUT | `/entity-types/{id}` | 更新 |
| DELETE | `/entity-types/{id}` | 删除（被引用时 409） |

### 4.6 附件

| 方法 | 路径 | 说明 |
|---|---|---|
| POST | `/entities/{id}/attachments` | multipart：`file`、`name`、`type`、`primary` → 返回 EntityOut |
| PUT | `/entities/{id}/attachments/{aid}` | 改标题/类型/设为主图 |
| DELETE | `/entities/{id}/attachments/{aid}` | 删除记录 + 磁盘文件 |
| GET | `/entities/{id}/attachments/{aid}` | 下载/内联（`?download=1` 附件下载） |
| GET | `/media/{aid}` | 同上短路径，供 `<img src>` 使用 |
| POST | `/entities/{id}/attachments/external` | 外部 URL 引用（预留） |

图片渲染：`<img src="/api/v1/media/{id}?token={attachmentToken}">` —— 附件令牌作用域仅限读取媒体，有效期 7 天，避免在 URL 中暴露主令牌。
限制：`HOMEITEM_MAX_UPLOAD_MB`（默认 25），按大小与 MIME 白名单校验。

### 4.7 保养记录

| 方法 | 路径 | 说明 |
|---|---|---|
| GET | `/entities/{id}/maintenance` | 记录列表（`?status=scheduled\|completed\|all`） |
| POST | `/entities/{id}/maintenance` | 新建 |
| PUT | `/entities/{id}/maintenance/{mid}` | 更新 |
| DELETE | `/entities/{id}/maintenance/{mid}` | 删除 |

### 4.8 到期提醒

| 方法 | 路径 | 说明 |
|---|---|---|
| GET | `/reminders` | 聚合提醒：`?within=30&includeOverdue=true&includeDone=false&kinds=warranty,reminder,maintenance,lowStock` |

响应：`{ "items": [{ "kind":"warranty", "entityId", "entityName", "date", "daysRemaining", "overdue", "note", "maintenanceId" }], "counts": {"overdue":0,"dueSoon":3,"total":7} }`
语义：`daysRemaining >= 0` 且 `<= within` 为即将到期；`< 0` 为已逾期。

### 4.9 统计

| 方法 | 路径 | 说明 |
|---|---|---|
| GET | `/statistics` | 总数、总价值、位置数、标签数、用户数、保修中数量、归档数、低库存数 |
| GET | `/statistics/locations` | 按位置聚合价值/数量 |
| GET | `/statistics/tags` | 按标签聚合价值/数量 |
| GET | `/statistics/purchase-price` | 按时间段的采购金额（折线图数据） |

### 4.10 静态资源

| 路径 | 说明 |
|---|---|
| `/`、`/assets/*` | 内嵌 Leptos 产物（rust-embed） |
| `/*`（非 /api） | SPA fallback → `index.html` |
| `/healthz` | 健康检查（DB ping） |

---

## 5. 关键实现约定

### 5.1 配置（环境变量）

| 变量 | 默认值 | 说明 |
|---|---|---|
| `HOMEITEM_LISTEN_ADDR` | `0.0.0.0:7745` | 监听地址 |
| `HOMEITEM_DB_PATH` | `./data/homeitem.db` | SQLite 文件 |
| `HOMEITEM_UPLOAD_DIR` | `./data/uploads` | 附件目录 |
| `HOMEITEM_JWT_SECRET` | 未设置则自动生成并持久化到 `data/.jwt_secret` | 签名密钥 |
| `HOMEITEM_ALLOW_REGISTRATION` | `false` | 是否开放注册 |
| `HOMEITEM_CORS_ORIGINS` | `*`（仅 `HOMEITEM_DEV_CORS=true` 时） | 生产默认同源 |
| `HOMEITEM_MAX_UPLOAD_MB` | `25` | 上传上限 |
| `HOMEITEM_STATIC_DIR` | 空 | 覆盖内嵌静态目录（开发用） |
| `HOMEITEM_LOG` | `info` | 日志级别 |

### 5.2 阻塞 IO 封装

所有 rusqlite 调用走统一 helper：

```rust
// db/pool.rs
pub async fn with_conn<T, F>(pool: &Pool<SqliteConnectionManager>, f: F) -> Result<T, AppError>
where F: FnOnce(&mut Connection) -> Result<T, AppError> + Send + 'static, T: Send + 'static
{
    let pool = pool.clone();
    tokio::task::spawn_blocking(move || {
        let mut conn = pool.get()?;      // r2d2
        let tx = conn.transaction()?;    // 写操作走事务
        let out = f(&mut tx)?;
        tx.commit()?;
        Ok(out)
    }).await.map_err(|e| AppError::Internal(e.to_string()))?
}
```
读写分离：`with_conn`（事务，写）/ `with_read`（无事务，读）。文件 IO 同样经 `spawn_blocking`。
连接池：`max_size = 8`，`PRAGMA busy_timeout = 5000`。

### 5.3 迁移

`db/migrations.rs` 内置 `MIGRATIONS: &[(&str, &str)]`，按顺序在 `PRAGMA user_version` 上递增执行，启动时自动跑；新增迁移只追加不修改。

### 5.4 安全

- 密码：Argon2id（默认参数 m=19456,t=2,p=1）。
- 主令牌 24h / `stayLoggedIn` 30d；附件令牌 7d。
- 上传：大小上限 + 扩展名/MIME 白名单 + 文件名服务端重写为 `{uuid}.{ext}`（杜绝路径穿越）+ 校验落盘路径在 UPLOAD_DIR 内。
- 所有 SQL 使用参数绑定。
- 响应加 `X-Content-Type-Options: nosniff`、`X-Frame-Options: DENY`、CSP（静态页）。
- 媒体端点对非图片类型强制 `Content-Disposition: attachment`。

### 5.5 前端路由（Leptos CSR）

| 路由 | 页面 |
|---|---|
| `/login` | 登录 |
| `/setup` | 首次初始化管理员 |
| `/` | 仪表盘（统计卡、到期提醒、最近物品） |
| `/items` | 物品列表（搜索、标签/类型/位置筛选、排序、分页、视图切换） |
| `/items/new` | 新建物品 |
| `/items/:id` | 物品详情（照片、字段、标签、保养、附件、面包屑） |
| `/items/:id/edit` | 编辑 |
| `/locations` | 位置树（含新建/编辑） |
| `/tags` | 标签管理 |
| `/reminders` | 到期提醒清单 |
| `/settings` | 账号、修改密码、货币、关于 |

鉴权：令牌存 `localStorage`；未登录访问受保护路由 → 重定向 `/login`；`/setup` 仅在 `status.needsSetup` 时可达。

---

## 6. 交付物

1. 可编译源码（`cargo build --workspace` 通过；`trunk build --release` 通过）
2. `Dockerfile`（多阶段）+ `docker-compose.yml`（`./data:/data` 挂载 db 与 uploads）
3. `README.md`：上游来源与 AGPL-3.0 声明、架构说明、本地开发命令、Docker 部署命令、环境变量表、API 摘要
4. `.gitignore` + 分阶段清晰 commit（脚手架 → 数据层 → 认证 → 各业务模块 → 前端 → 容器化 → 文档）

### Dockerfile 阶段

```
[1] ghcr.io/trunk-rs/trunk      → trunk build --release 产出 dist/
[2] rust:1.8x-slim (bookworm)   → COPY dist → rust-embed 编译期嵌入 → cargo build --release
[3] debian:bookworm-slim        → 复制二进制，非 root 用户，EXPOSE 7745，VOLUME /data
```

---

## 7. 已确认决策（2026-08-28）

1. **多租户（跟随上游）**：实现 group。新增 `groups` 表与 `user_groups(user_id, group_id, role ∈ {user, owner})`；`entities`/`tags`/`entity_types`/`notifiers` 增加 `group_id` 外键。注册需邀请令牌。令牌角色沿用上游三档 `admin|user|attachments`。
2. **到期提醒 + 邮件推送**：四类聚合（保修到期 / 自定义到期 / 保养计划到期 / 低库存），并实现 **SMTP 邮件通知器**（notifiers 表 + 后台定时扫描发送 + 手动测试发送），SMTP 经环境变量配置。
3. **角色跟随上游**：group 成员角色 `user`（默认，可读写库存）与 `owner`（可管理组、成员、邀请、通知器）。不做只读角色。
4. **界面默认中文**。
5. **Leptos 0.8 稳定版 + Salvo 1.2 稳定版**（已确认）。
