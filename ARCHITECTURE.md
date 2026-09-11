# Vortex 架构文档

## 概述

Vortex 是一个 AI 网关桌面应用，采用 **Tauri 2** 框架，后端使用 **Rust**（Actix-Web + SQLite），前端使用 **Vue 3 + TypeScript**。其核心职责是将 OpenAI 兼容的 API 请求智能路由到多个上游 AI 提供商。

## 系统架构

```
                          ┌─────────────────────────────────────────────────┐
                          │            Vortex 桌面应用 (Tauri 2)            │
                          │                                                 │
   AI 客户端 ─────────────│  ┌───────────────────────────────────────────┐  │
              (OpenAI SDK,                           │  │       Actix-Web HTTP Server :10168        │  │
               cURL, etc.)│  │                                           │  │
                          │  │/v1/chat/completions    ──┐                │  │
                          │  │/v1/models                │                │  │
                          │  │/v1/embeddings            │                │  │
                          │  │/v1/images/generations    │                │  │
                          │  │                          ▼                │  │
                          │  │                     ProxyEngine           │  │
                          │  │                          │                │  │
                          │  │          ┌───────────────┼───────────────┐│  │
                          │  │          │               │               ││  │
                          │  │    API Key 验证   Route Resolver 用量记录 │  │
                          │  │          │               │               ││  │
                          │  │                          ▼                │  │
                          │  │                  ProviderRegistry         │  │
                          │  │                    (21 个提供商)          │  │
                          │  │                          │                │  │
                          │  │                      Executor             │  │
                          │  │            (OpenAI / Anthropic / Gemini)  │  │
                          │  │                          │                │  │
                          │  │               Retry + CircuitBreaker      │  │
                          │  │                          │                │  │
                          │  │             Translator (→ OpenAI 格式)    │  │
                          │  └───────────────────────────────────────────┘  │
                          │                                                 │
                          │  ┌───────────────────────────────────────────┐  │
                          │  │             SQLite (WAL 模式)             │  │
                          │  │provider_connections | api_keys            │  │
                          │  │usage_history | key_value (settings)       │  │
                          │  │free_token_sites (免费站点目录)            │  │
                          │  └───────────────────────────────────────────┘  │
                          │                                                 │
                          │  ┌───────────────────────────────────────────┐  │
                          │  │        Vue 3 管理界面 (10 个页面)         │  │
                          │  │接入指南 | 实时路由 | 订阅 | 免费 Token    │  │
                          │  │请求日志 | 统计 | 同步 | 对话              │  │
                          │  │设置 | 关于                                │  │
                          │  └───────────────────────────────────────────┘  │
                          └─────────────────────────────────────────────────┘
                                          │
                    ┌─────────────────────┼─────────────────────┐
                    │                     │                     │
               OpenAI API           Anthropic API          Gemini API
               (上游提供商)          (上游提供商)          (上游提供商)
```

## 核心模块

### 1. 应用状态 (`lib.rs`)

`AppState` 是全局共享状态，通过 `Arc<AppState>` 在所有请求间共享：

```rust
pub struct AppState {
    pub db_pool: db::core::DbPool,              // SQLite 连接池 (r2d2, 最大8连接)
    pub config: config::AppConfig,              // 配置 (端口、数据目录、加密密钥等)
    pub provider_registry: ProviderRegistry,     // 21个内置提供商定义
    pub proxy_engine: RwLock<ProxyEngine>,       // 代理引擎
    pub resilience_manager: ResilienceManager,   // 熔断器管理
    pub http_client: reqwest::Client,            // HTTP 客户端 (120s超时)
    pub encryption_key: Vec<u8>,                 // AES-256-GCM 密钥
    pub proxy_handle: Mutex<Option<ServerHandle>>, // 网关服务器句柄，支持运行时启停
    pub proxy_port: u16,                         // 网关监听端口
}
```

**启动流程** (`run()`):
1. 加载配置 (`AppConfig::load()`)
2. 创建应用状态 (`create_app_state()`) — 初始化 DB 连接池、运行迁移、创建各引擎
3. 启动 API 服务器 (`start_api_server()`) — 独立线程中运行 Actix-Web
4. 启动 Tauri 应用 — 注册插件（updater / process / shell）、命令处理器与系统托盘菜单（显示窗口 / 启动代理 / 停止代理 / 退出）

### 2. 代理引擎 (`proxy/engine.rs`)

`ProxyEngine` 是请求处理的核心，负责完整的请求生命周期：

```
请求进入 → API Key 验证 → 路由解析 → 执行(带重试+熔断) → 用量记录 → 返回
```

**关键方法：**

- `handle_request()` — 入口，协调整个流程
- `resolve_route()` — 路由解析：解析 `provider/model` 格式，定位提供商定义与连接
- `handle_single_with_retry()` — 单一提供商请求，带重试和熔断器
- `extract_usage_from_response()` — 从响应中提取 token 用量（支持 openai/anthropic/gemini 格式）

### 3. 执行器 (`proxy/executor.rs`)

`ProviderExecutor` trait 有 3 个实现，负责不同 API 格式的请求构建和发送：

| 执行器 | 认证方式 | 请求格式 | 响应处理 |
|--------|----------|----------|----------|
| `OpenAIExecutor` | `Authorization: Bearer {key}` | OpenAI 原生格式 | 直接透传 |
| `AnthropicExecutor` | `x-api-key: {key}` + `anthropic-version` | Anthropic Messages 格式 | 转换为 OpenAI 格式 |
| `GeminiExecutor` | `?key={key}` query 参数 | Gemini generateContent 格式 | 转换为 OpenAI 格式 |

`ExecutorFactory` 根据 `ProviderDef.api_format` 字段选择对应的执行器。

### 4. SSE 流式处理 (`proxy/sse.rs`)

支持将上游的 SSE 流式响应实时转换为 OpenAI chunk 格式：

- `stream_sse_response()` — 根据源格式分发
- `stream_anthropic_as_openai()` — 转换 Anthropic 事件 (message_start, content_block_delta, message_delta)
- `stream_gemini_as_openai()` — 转换 Gemini 流式 JSON

### 5. 弹性机制 (`routing/resilience.rs`)

#### 熔断器 (`CircuitBreaker`)

三态有限状态机：

```
    Closed ──(失败≥5次)──→ Open ──(60s后)──→ HalfOpen ──(成功≥3次)──→ Closed
      ↑                                                      │
      └──────────────────────────────────────────────────────┘
                                                           │
                                                    (失败) → Open
```

- **Closed**: 正常放行，记录失败次数
- **Open**: 拒绝所有请求，等待 60 秒后转 HalfOpen
- **HalfOpen**: 允许有限请求，需 3 次连续成功才恢复 Closed

#### 重试策略 (`proxy/retry.rs`)

- 最大重试次数: 2
- 基础延迟: 500ms
- 退避策略: 指数退避 (`500ms * 2^attempt`，上限 30s)
- 重试条件: HTTP 5xx 或 429 状态码

### 6. 提供商注册表 (`providers/registry.rs`)

`ProviderRegistry` 内置 21 个提供商定义，每个定义包含：

```rust
pub struct ProviderDef {
    pub id: String,           // "openai", "anthropic", ...
    pub alias: String,        // "oa", "an", ...
    pub name: String,         // 显示名称
    pub icon: String,         // 图标标识
    pub color: String,        // 品牌色
    pub service_kinds: Vec<String>, // ["llm", "embedding", "image"]
    pub no_auth: bool,        // 是否无需认证 (Ollama, Pollinations)
    pub has_free: bool,       // 是否有免费层
    pub base_url: String,     // API 基础 URL
    pub chat_path: String,    // 聊天端点路径
    pub models_path: String,  // 模型列表端点
    pub api_format: String,   // "openai" | "anthropic" | "gemini" | "cohere" | "cloudflare"
    pub auth_type: String,    // "apikey" | "noauth"
}
```

**模型解析** (`resolve_model_provider()`):
1. 先尝试 `provider/model` 格式分割（如 `openai/gpt-4o`）
2. 再尝试别名前缀匹配（如 `ds-deepseek-chat` → DeepSeek/deepseek-chat）

### 7. 响应转换器 (`translator/mod.rs`)

将不同格式的响应统一转换为 OpenAI chat.completion 格式：

- `anthropic_to_openai_response()` — 转换 Anthropic 响应（content 数组、input_tokens/output_tokens、stop_reason）
- `gemini_to_openai_response()` — 转换 Gemini 响应（candidates/parts、usageMetadata、finishReason）

### 8. 数据库层 (`db/`)

#### 连接池配置

- 引擎: SQLite + r2d2 连接池
- 最大连接: 8，最小空闲: 2
- 模式: WAL (Write-Ahead Logging)
- 外键约束: 开启
- 忙超时: 5 秒

#### 数据表

| 表名 | 用途 | 关键字段 |
|------|------|----------|
| `provider_connections` | 提供商连接 | id, provider, name, api_key/access_token(加密), priority, is_active, test_status, backoff_level, rate_limited_until, consecutive_use_count |
| `api_keys` | 网关 API 密钥 | id, name, key(唯一, `vx-{32位hex}`), allowed_models, allowed_connections, allowed_endpoints, no_log, auto_resolve, is_active, is_banned, rate_limits, usage_limits |
| `usage_history` | 用量记录 | provider, model, connection_id, api_key_id, tokens_input/output/cache_read/cache_creation/reasoning, service_tier, status, success, latency_ms, ttft_ms, cost |
| `key_value` | 键值存储(设置) | namespace, key, value(JSON) — 目前仅 `settings/general` |
| `free_token_sites` | 免费额度站点目录 | id, name, home_url, apply_url, api_supported, api_base, api_format, free_quota, region(`cn`/`global`/`local`), requires_card, requires_verify, tags(JSON), note, provider_id, source(`builtin`/`user`), submitter, sort_order |

#### 迁移 (`db/migrations/`)

| 版本 | 文件 | 内容 |
|------|------|------|
| 001 | `001_initial.sql` | provider_connections / api_keys / usage_history / key_value + 初始设置 |
| 002 | `002_free_token_sites.sql` | free_token_sites 建表 + 30 条内置站点种子（均提供 API） |
| 003 | `003_free_token_web_only.sql` | 补充 13 条「仅网页版、无 API」站点，使 `api_supported` 具备区分度 |

> 迁移按版本号一次性应用并记录在 `_vortex_migrations` 中；已应用的版本不会重跑，新增内容一律追加新版本文件。

#### 加密 (`db/encryption.rs`)

- 算法: AES-256-GCM
- 密钥派生: 从 passphrase 经 1000 轮混合哈希派生 32 字节密钥
- 存储: nonce (12字节) + ciphertext，Base64 编码
- 所有敏感字段（api_key, access_token, refresh_token）在存储时加密、读取时解密

### 9. API 端点 (`api/`)

#### OpenAI 兼容端点 (`/v1`)

| 端点 | 处理函数 | 说明 |
|------|----------|------|
| `POST /v1/chat/completions` | `chat::chat_completions` | 聊天补全，支持流式和非流式 |
| `GET /v1/models` | `models::list_models` | 聚合所有提供商的模型列表 |
| `POST /v1/embeddings` | `embeddings::create_embeddings` | 文本嵌入 |
| `POST /v1/images/generations` | `images::create_images` | 图像生成 |

#### 管理端点 (`/api`)

| 端点 | 说明 |
|------|------|
| `GET/POST /api/providers` | 提供商连接列表/创建 |
| `GET/PATCH/DELETE /api/providers/{id}` | 单个连接 CRUD |
| `POST /api/providers/{id}/test` | 测试连接可用性 |
| `POST /api/providers/preview-models` | 预览远程可用模型列表（无需先保存连接） |
| `GET/POST /api/keys` | API 密钥列表/创建 |
| `GET/DELETE /api/keys/{id}` | 单个密钥操作 |
| `GET /api/usage` | 用量记录列表 |
| `GET /api/usage/stats` | 用量聚合统计 |
| `GET/PATCH /api/settings` | 设置读写 |
| `GET/POST /api/free-tokens` | 免费 Token 站点列表 / 提交推荐（`name` 必填，其余字段可留空） |
| `DELETE /api/free-tokens/{id}` | 删除用户提交的推荐；内置条目返回 400「内置站点不可删除」 |
| `GET /api/health` | 健康检查 (DB 连通性 + 版本) |

### 10. Tauri 命令 (`tauri_cmds/`)

前端通过 Tauri IPC 调用的命令，功能与管理 API 类似但走 IPC 通道：

- `list_providers`, `add_provider`, `test_provider`
- `get_settings`, `update_settings`
- `start_proxy`, `stop_proxy` — 运行时启停网关服务器
- 托盘事件通过 `tray-action` 事件向前端广播（`start` / `stop`）

### 11. Tauri 插件

| 插件 | 用途 |
|------|------|
| `tauri-plugin-updater` | 自动更新：检查 GitHub Releases 新版本、下载并安装 |
| `tauri-plugin-process` | 进程管理：更新安装后重启应用 |
| `tauri-plugin-shell` | Shell 命令执行 |

前端通过 `composables/useUpdater.ts` 封装更新逻辑：启动时静默检查 → 发现新版本弹出通知 → 用户确认后下载 → 下载完成提示重启。

## 前端架构

### 技术栈

- **Vue 3.5** — Composition API + `<script setup>`
- **TypeScript** — 严格模式 (`strict: true`)
- **Element Plus** — UI 组件库（图标全局注册）
- **Ant Design Vue** — 辅助组件库（a-card、a-timeline 等）
- **UnoCSS** — 原子化 CSS (`presetUno` + `presetAttributify`)
- **Pinia** — 状态管理 (Composition API 风格)
- **Vue Router** — SPA 路由（Web 端 hash 模式 / Tauri 端 history 模式）
- **ECharts 6 + vue-echarts** — 图表可视化
- **highlight.js** — 语法高亮
- **Axios** — HTTP 客户端 (baseURL: `http://localhost:10168/api`, 超时 30s)

### 页面结构

```
AppLayout
├── WindowChrome (32px, data-tauri-drag-region 拖拽条)
├── Sidebar (204px, 可折叠)
│   ├── 品牌标识
│   ├── 导航菜单 (主区 8 项 + 底部 2 项 + 折叠按钮)
│   └── 订阅项带连接数徽章
└── <main class="main"> (部分路由 flush 满高布局)
    ├── Header (56px)
    │   ├── 页面标题
    │   └── 健康状态标签 (每30s 轮询 /api/health)
    └── <router-view />
        ├── Guide        — 接入指南
        ├── LiveRouting  — 实时路由 + 网关状态（默认首页）
        ├── Subscriptions — 提供商连接管理（含新建 / 自定义 / 编辑子路由）
        ├── FreeTokens   — 免费 Token 站点目录（卡片 / 表格双视图）
        ├── RequestLogs  — 请求日志
        ├── Statistics   — 端点统计 / 用量统计
        ├── Sync         — 备份与配置迁移
        ├── Chat         — 对话（会话分支树 + 流式输出）
        ├── Settings     — 通用设置 / 高级设置
        └── About        — 关于（含检查更新）
```

> `flush` 路由（`/live-routing`、`/subscriptions/*`、`/chat`）走满高布局，不套 Header 的内边距滚动容器；例外是 `/subscriptions/new`。

### 独立窗口

| 窗口 label | 用途 |
|------------|------|
| `main` | 主窗口（1000×680，可调整大小） |
| `status-panel` | 状态面板小窗口（340×460，无边框透明置顶，托盘可切换显隐） |

### API 适配层 (`src/api/`)

`client.ts` 提供 axios 实例，其余模块按「直连后端」与「前端适配」两类组织：

| 模块 | 数据来源 | 说明 |
|------|----------|------|
| `providers.ts` / `keys.ts` / `usage.ts` / `settings.ts` | 直连 `/api/*` | 与后端接口一一对应 |
| `freeTokens.ts` | 直连 `/api/free-tokens` | 站点列表 / 提交推荐 / 删除推荐；字段统一 camelCase，与后端 `#[serde(rename_all = "camelCase")]` 对应 |
| `stats.ts` | 前端聚合 | 后端无 `daily_stats` 表，`statsApi` 从 `/api/usage` 原始记录聚合出端点统计（KPI / 热力图 / 趋势 / 日模型明细）；`usageApi` 的同步与清理类方法为 TODO 桩 |
| `chat.ts` | 前端 + 网关 | 分支树（`parentId` / `activeNodeId` / `siblingIds`）持久化在 localStorage；发送走网关 `POST /v1/chat/completions` SSE，鉴权自动取第一个可用 API Key |
| `sync.ts` | 混合 | cc-switch 迁移预览、本地配置导出/导入为真实实现；WebDAV 云端备份/恢复与云端备份列表为 TODO 桩，UI 降级提示「后端未接入」 |
| `notifications.ts` | 远程通知中心 | 从远程服务端拉取站内通知列表，与本地后端无关 |

后端补齐接口后，只需替换 `stats.ts` / `sync.ts` 中标注 TODO 的函数，组件层无需改动。

### 状态管理 (Pinia Stores)

| Store | 职责 |
|-------|------|
| `provider.ts` | 提供商列表和连接管理 |
| `settings.ts` | 应用设置 |
| `usage.ts` | 用量统计 |
| `chatLayout.ts` | 对话页布局(侧栏折叠/输入栏高度)，持久化到 localStorage |

### Composables

| Composable | 职责 |
|------------|------|
| `useTheme.ts` | 主题管理（light / dark / system），持久化到 localStorage |
| `useThemeColors.ts` | CSS 变量解析为具体色值供 ECharts / Canvas 使用 |
| `useUpdater.ts` | 自动更新：检查 / 下载 / 安装 / 重启，启动时静默检查 |
| `useNotifications.ts` | 通知管理：每 5 分钟轮询远程通知，未读弹出桌面通知 |

### 主题

亮 / 暗双主题，由 `composables/useTheme.ts` 在 `<html>` 上切换 `.dark` 类（模式：`light` / `dark` / `system`，持久化到 `localStorage['vortex-theme']`）：

| | 亮色 | 暗色 |
|---|---|---|
| `--bg` | `#f6f5f2`（暖中性灰） | `#0a0a0e` |
| `--surface` | `#ffffff` | `#151519` |
| `--ink` | `#1a1a22` | `#e8e8ec` |
| `--accent` | `oklch(0.55 0.20 280)` 深紫（亮色）/ `oklch(0.65 0.20 280)` 深紫（暗色） | 同左 |

变量定义在 `src/styles/cc-theme.css`（`:root` 亮色 / `.dark` 暗色，并映射 Element Plus 暗色变量），基础类（`.card` / `.btn` / `.table` / `.tabs` / `.pill` 等）在 `src/styles/cc-components.css`。ECharts / Canvas 无法读取 `var()`，由 `composables/useThemeColors.ts` 解析为具体色值。

## 请求处理流程

以 `POST /v1/chat/completions` 为例：

```
1. 请求到达 chat_completions()
2. 解析 OpenAI 格式请求体
3. 调用 ProxyEngine::handle_request()
4.   ├── API Key 验证 (若 require_api_key=true)
5.   ├── resolve_route()
6.   │   └── ProviderRegistry::resolve_model_provider()
7.   │       └── 查 provider_connections 表找活跃连接
8.   └── handle_single_with_retry()
9.       ├── 检查熔断器状态
10.      ├── Executor::execute() → 上游 API 调用
11.      ├── 成功: 提取用量 → 记录 → 返回
12.      ├── 失败(5xx/429): 指数退避重试
13.      └── 失败(其他): 记录熔断器失败 → 返回错误
14. 记录 UsageEntry 到 usage_history
15. 返回 OpenAI 格式响应
```

## 配置管理

### 配置加载 (`config.rs`)

`AppConfig` 从环境变量加载，支持：

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `VORTEX_PORT` | 10168 | API 服务器端口 |
| `VORTEX_DATA_DIR` | 系统数据目录/vortex | 数据存储目录 |
| `VORTEX_ENCRYPTION_KEY` | 自动生成并持久化 | 加密密钥 (32字节十六进制) |
| `VORTEX_REQUIRE_API_KEY` | false | 是否要求客户端 API Key |
| `VORTEX_LOG_LEVEL` | info | 日志级别 |

### 设置存储

设置存储在 `key_value` 表中：
- `settings/general` — 通用设置 (port, requireApiKey, theme)

## 错误处理 (`error.rs`)

`AppError` 枚举实现 `actix_web::ResponseError`，自动映射 HTTP 状态码：

| 错误类型 | HTTP 状态码 | 说明 |
|----------|-------------|------|
| `NotFound` | 404 | 资源不存在 |
| `BadRequest` | 400 | 请求参数错误 |
| `Unauthorized` | 401 | 认证失败 |
| `Db` / `Pool` | 500 | 数据库错误 |
| `Provider` | 502 | 上游提供商错误 |
| `Routing` | 503 | 路由解析失败 |
| `Internal` | 500 | 内部错误 |
