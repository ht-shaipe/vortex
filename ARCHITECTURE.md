# Vortex 架构文档

## 概述

Vortex 是一个 AI 网关应用，后端使用 **Rust**（Actix-Web + SQLite），前端使用 **Vue 3 + TypeScript**。支持两种运行模式：

- **桌面模式**（`src-tauri`）— 基于 **Tauri 2** 的桌面应用，内嵌 HTTP 服务器 + 系统托盘 + IPC 命令
- **Web 模式**（`vortex-server`）— 纯 actix-web 服务器，不依赖 Tauri，适合服务器部署

后端 Rust 代码按功能拆分为 4 个 crate（依赖链：`vortex-store ← vortex-router ← vortex-gateway ← src-tauri`），核心层可独立复用于 Web 项目。

## Crate 结构

```
vortex/
├── crates/
│   ├── vortex-store/       # 数据层：SQLite、配置、加密、错误
│   ├── vortex-router/      # 路由层：代理引擎、提供商注册表、弹性路由、格式转换
│   └── vortex-gateway/     # 网关层：HTTP API 服务器、services、智能体集成
│       └── src/bin/vortex-server.rs  # 独立 Web 服务器入口
└── src-tauri/              # 桌面应用：Tauri IPC 命令（薄包装）、托盘、窗口
```

| Crate | 职责 | 可独立复用 |
|-------|------|------------|
| `vortex-store` | 数据库（r2d2 连接池 + 18 个迁移）、配置加载、AES-256-GCM 加密、错误类型 | ✓ |
| `vortex-router` | 代理引擎（ProxyEngine）、提供商注册表、熔断器/重试/速率限制、SSE 流式、格式转换器 | ✓ |
| `vortex-gateway` | AppState + EngineContext 实现、actix-web 路由注册、services 业务逻辑层、智能体集成 | ✓（vortex-server） |
| `src-tauri` | Tauri 命令薄包装（委托 services）、系统托盘、窗口管理、自动更新 | ✗（依赖 Tauri） |

## 系统架构

```
                          ┌─────────────────────────────────────────────────┐
                          │              运行模式                           │
                          │                                                 │
                          │  ┌──────────────┐    ┌──────────────────────┐  │
                          │  │ 桌面模式      │    │ Web 模式             │  │
                          │  │ src-tauri    │    │ vortex-server        │  │
                          │  │ (Tauri 2)    │    │ (纯 actix-web)       │  │
                          │  └──────┬───────┘    └──────────┬───────────┘  │
                          │         │                       │              │
                          │         └───────────┬───────────┘              │
                          │                     │                          │
                          │         ┌───────────▼───────────┐              │
                          │         │  vortex-gateway        │              │
                          │         │  AppState + Services   │              │
                          │         │  Actix-Web :10168      │              │
                          │         └───────────┬───────────┘              │
                          │                     │                          │
                          │         ┌───────────▼───────────┐              │
                          │         │  vortex-router         │              │
                          │         │  ProxyEngine           │              │
                          │         │  EngineContext trait   │              │
                          │         └───────────┬───────────┘              │
                          │                     │                          │
                          │         ┌───────────▼───────────┐              │
                          │         │  vortex-store          │              │
                          │         │  SQLite + Config +     │              │
                          │         │  Encryption            │              │
                          │         └───────────────────────┘              │
                          └─────────────────────────────────────────────────┘

  前端 (Vue 3)
  ┌─────────────────────────────────────────────────────────┐
  │  runtime.kind 检测                                      │
  │  ├── 'desktop' → Tauri IPC (invoke)                     │
  │  └── 'web'     → HTTP API (axios → localhost:10168/api) │
  └─────────────────────────────────────────────────────────┘
```

## 核心模块

### 1. 应用状态 (`vortex-gateway/src/lib.rs`)

`AppState` 是全局共享状态，通过 `Arc<AppState>` 在所有请求间共享：

```rust
pub struct AppState {
    pub db_pool: db::core::DbPool,                    // SQLite 连接池 (r2d2, 最大8连接)
    pub config: config::AppConfig,                    // 配置 (端口、数据目录、加密密钥等)
    pub provider_registry: ProviderRegistry,           // 多家内置提供商定义
    pub proxy_engine: ProxyEngine,                     // 代理引擎
    pub resilience_manager: ResilienceManager,         // 熔断器管理
    pub rate_limiter: RateLimiter,                     // 内存速率限制器 (滑动窗口)
    pub bandit_router: BanditRouter,                   // Thompson 采样 bandit 路由
    pub occupancy: OccupancyTracker,                   // 模型占用追踪 (auto 路由软避让)
    pub sticky_session: StickySessionManager,          // 粘性会话 (auto 路由模型记忆)
    pub encryption_key: Vec<u8>,                       // AES-256-GCM 密钥
    pub proxy_handle: Mutex<Option<ServerHandle>>,     // 网关服务器句柄
    pub proxy_port: u16,                               // 网关监听端口
}
```

#### EngineContext trait (`vortex-router/src/context.rs`)

代理引擎通过 `EngineContext` trait 与 `AppState` 解耦，不直接依赖 `AppState` 的具体字段：

```rust
pub trait EngineContext: Send + Sync {
    fn db_pool(&self) -> &DbPool;
    fn config(&self) -> &AppConfig;
    fn provider_registry(&self) -> &ProviderRegistry;
    fn sticky_session(&self) -> &StickySessionManager;
    fn encryption_key(&self) -> &[u8];
    fn resilience_manager(&self) -> &ResilienceManager;
    fn rate_limiter(&self) -> &RateLimiter;
    fn bandit_router(&self) -> &BanditRouter;
    fn occupancy(&self) -> &OccupancyTracker;
}
```

`AppState` 实现 `EngineContext`，`ProxyEngine::handle_request()` 接收 `&dyn EngineContext`，使引擎可脱离 Tauri/actix 独立测试和复用。

#### 启动流程

**桌面模式** (`src-tauri/src/lib.rs::run()`):
1. 加载配置 (`AppConfig::load()`)
2. 创建应用状态 (`create_app_state()`) — 初始化 DB 连接池、运行迁移、创建各引擎
3. 启动 API 服务器 (`start_api_server()`) — 独立线程中运行 Actix-Web
4. 启动 Tauri 应用 — 注册插件（updater / process / shell）、命令处理器与系统托盘

**Web 模式** (`vortex-gateway/src/bin/vortex-server.rs::main()`):
1. 加载配置 (`AppConfig::load()`)
2. 创建应用状态 (`create_app_state()`)
3. 启动 API 服务器 (`start_api_server()`)
4. 等待 SIGINT / SIGTERM 信号 → 优雅关闭 (`handle.stop(true).await`)

两种模式共享步骤 1-3，区别仅在进程保持方式（Tauri 事件循环 vs 信号等待）。

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

`ExecutorFactory` 根据 `ProviderDef.api_format` 字段选择对应的执行器。Cloudflare（`cloudflare`）格式当前回退到 `OpenAIExecutor`。

### 4. SSE 流式处理 (`proxy/sse.rs`)

支持将上游的 SSE 流式响应实时转换为 OpenAI chunk 格式：

- `stream_sse_response()` — 根据源格式分发
- `stream_anthropic_as_openai()` — 转换 Anthropic 事件 (message_start, content_block_delta, message_delta)
- `stream_gemini_as_openai()` — 转换 Gemini 流式 JSON

流式用量通过 `StreamUsage` 结构捕获：流结束时由各解析器的 `usage()` 返回输入/输出 Token，并从 Anthropic `message_start` 提取 `cache_creation_input_tokens` / `cache_read_input_tokens`、从 OpenAI `completion_tokens_details` 提取 `reasoning_tokens`，由引擎回调写入 `usage_history`。上游未在流中返回 usage 时按累计输出文本估算并置 `estimated` 标志。非流式路径由 `extract_usage_from_response()` 返回等价的 `UsageExtract` 结构。

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

熔断器按名称独立计数，网关内有两级：

| 级别 | 熔断器名称 | 失败阈值 | 说明 |
|------|-----------|---------|------|
| 连接级 | `{provider}:{connection}` | 5 | 整个连接故障（密钥失效、网络不通） |
| 模型级 | `{provider}:{connection}:{model}` | 3 | 单模型确定性故障（下线、无权限）更快隔离 |

两者均冷却 60 秒后转 HalfOpen 探测。对下游的错误映射：熔断打开时返回 503 与友好提示（熔断器名称仅保留在应用日志与请求日志 error_code 中）；上游 4xx/5xx 不可重试错误以 `AppError::Upstream` 原样透传状态码与响应体。

#### 重试策略 (`proxy/retry.rs`)

- 最大重试次数: 2
- 基础延迟: 500ms
- 退避策略: 指数退避 (`500ms * 2^attempt`，上限 30s)
- 重试条件: HTTP 5xx 或 429 状态码

### 6. 提供商注册表 (`providers/registry.rs`)

`ProviderRegistry` 内置多家提供商定义，每个定义包含：

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
    pub api_format: String,   // "openai" | "anthropic" | "gemini" | "cloudflare"
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
| `usage_history` | 用量记录 | provider, model, connection_id, api_key_id, tokens_input/output/cache_read/cache_creation/reasoning, service_tier, status, success, latency_ms, ttft_ms, cost, usage_estimated, saved_tokens, agent |
| `key_value` | 键值存储(设置) | namespace, key, value(JSON) — `settings/general`（端口/UA/隐藏已映射模型等）与 `settings/security`（Token 鉴权、访问令牌、CORS）；首次启动自动写入安全默认值（Token 鉴权开启 + 自动生成访问令牌） |
| `free_token_sites` | 免费额度站点目录 | id, name, home_url, apply_url, api_supported, api_base, api_format, free_quota, region(`cn`/`global`/`local`), requires_card, requires_verify, tags(JSON), note, provider_id, source(`builtin`/`user`), submitter, sort_order |
| `model_aliases` | 模型别名（虚拟模型名映射） | id, alias, targets(JSON数组: provider, model, connection_id), created_at |

#### 迁移 (`db/migrations/`)

| 版本 | 文件 | 内容 |
|------|------|------|
| 001 | `001_initial.sql` | provider_connections / api_keys / usage_history / key_value + 初始设置 |
| 002 | `002_free_token_sites.sql` | free_token_sites 建表 + 30 条内置站点种子（均提供 API） |
| 003 | `003_free_token_web_only.sql` | 补充 13 条「仅网页版、无 API」站点，使 `api_supported` 具备区分度 |
| 004 | `004_provider_latency.sql` | provider_connections 添加 last_latency_ms 列 |
| 005 | `005_usage_index.sql` | usage_history 添加 timestamp 降序索引 |
| 006 | `006_model_aliases.sql` | model_aliases 建表（虚拟模型名 + 多目标故障转移） |
| 007 | `007_model_alias_source.sql` | model_aliases 添加 source 列，区分自动归纳（auto）与手动创建（manual），重新归纳时仅替换 auto 记录 |
| 008 | `008_usage_estimated.sql` | usage_history 添加 usage_estimated 列，标记 Token 用量为估算值（上游未返回 usage 时按文本长度估算） |
| 009 | `009_rate_limits.sql` | 连接级速率限制表 |
| 010 | `010_model_catalog.sql` | 模型目录表（自动拉取的模型列表与元信息） |
| 011 | `011_routing_profiles.sql` | 命名路由配置（auto:profile 回退链） |
| 012 | `012_tos_review.sql` | 服务条款审阅记录 |
| 013 | `013_saved_tokens.sql` | usage_history 添加 saved_tokens 列，记录 Prompt 压缩节省的 Token 数 |
| 014 | `014_compressed_content.sql` | 压缩内容寻址回忆表 |
| 015 | `015_key_permissions.sql` | API Key 权限规则表 |
| 016 | `016_usage_agent.sql` | usage_history 添加 agent 列，记录请求来源 Agent/终端标识 |
| 017 | `017_clear_catalog_seed.sql` | 清空 model_catalog 旧内置种子数据 |
| 018 | `018_model_alias_sort_order.sql` | model_aliases 添加 sort_order 列，支持自定义排序权重 |

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
| `POST /v1/messages` | `messages::anthropic_messages` | Anthropic Messages 兼容（协议转换，支持流式） |
| `GET /v1/models` | `models::list_models` | 聚合已配置模型（`provider/model`）与已启用的虚拟别名；开启「隐藏已映射的真实模型」（默认开）且存在已启用虚拟映射时，真实模型全部不输出，仅暴露虚拟模型名 |
| `POST /v1/embeddings` | `embeddings::create_embeddings` | 文本嵌入 |
| `POST /v1/images/generations` | `images::create_images` | 图像生成 |

#### 管理端点 (`/api`)

| 端点 | 说明 |
|------|------|
| `GET/POST /api/providers` | 提供商连接列表/创建 |
| `GET/PATCH/DELETE /api/providers/{id}` | 单个连接 CRUD |
| `POST /api/providers/{id}/test` | 测试连接可用性 |
| `GET /api/providers/{id}/apikey` | 获取解密后的真实 API 密钥（前端复制用，绕过脱敏） |
| `POST /api/providers/preview-models` | 预览远程可用模型列表（无需先保存连接） |
| `GET/POST /api/keys` | API 密钥列表/创建 |
| `GET/DELETE /api/keys/{id}` | 单个密钥操作 |
| `GET /api/usage` | 用量记录列表 |
| `GET /api/usage/stats` | 用量聚合统计 |
| `GET/PATCH /api/settings` | 设置读写 |
| `GET/POST /api/free-tokens` | 免费 Token 站点列表 / 提交推荐（`name` 必填，其余字段可留空） |
| `DELETE /api/free-tokens/{id}` | 删除用户提交的推荐；内置条目返回 400「内置站点不可删除」 |
| `GET/POST /api/model-aliases` | 模型别名列表/创建（虚拟模型名 + 多目标故障转移） |
| `POST /api/model-aliases/auto-generate` | 自动归纳：按模型家族（剥离日期/`-latest`/`-free`/`:free`/上下文长度等后缀）分组生成虚拟别名，单模型家族也生成同名别名；先删除旧 `auto` 别名再批量创建（跳过与 manual 同名的） |
| `GET/PATCH/DELETE /api/model-aliases/{id}` | 单个模型别名操作 |
| `GET /api/health` | 健康检查 (DB 连通性 + 版本) |
| `GET /api/system/status` | 系统运行状态快照（代理状态、端口、版本、DB、连接统计、今日用量） |
| `POST /api/system/proxy/start` | 启动本地代理服务器 |
| `POST /api/system/proxy/stop` | 停止本地代理服务器 |
| `GET /api/agents/detect` | 检测本机已安装的 AI 编程智能体 |
| `POST /api/agents/preview` | 预览智能体配置变更 |
| `POST /api/agents/apply` | 应用智能体配置（写入前自动备份） |
| `POST /api/agents/restore` | 从备份恢复智能体配置 |
| `GET /api/agents/backups` | 列出所有配置备份 |
| `POST /api/chat/cancel` | 取消进行中的流式对话 |

### 10. Services 层 (`vortex-gateway/src/services/`)

业务逻辑从 Tauri 命令和 HTTP 端点中提取为独立的服务函数，供两种调用方式复用：

| 模块 | 函数 | 说明 |
|------|------|------|
| `provider_service` | `list_providers` / `add_provider` / `test_provider` | 提供商列表（脱敏）、新增连接、测试连通性 |
| `settings_service` | `get_settings` / `update_settings` | 设置读写 |
| `system_service` | `get_system_status` | 系统运行状态快照 |
| `proxy_service` | `start_proxy` / `stop_proxy` | 代理服务器启停 |
| `chat_service` | `mark_cancelled` / `take_cancelled` | 流式对话取消标记管理（跨命令共享） |
| `agent_service` | `detect_agents` / `preview_config` / `apply_config` / `restore_config` / `list_backups` | 智能体检测、配置预览/应用/恢复、备份列表 |

> `test_provider` 内部使用 `awc::Client`（非 `Send`），Tauri 命令通过 `spawn_blocking` + `actix_rt::Runtime` 调用，HTTP handler 在 actix runtime 中直接调用。同步 DB/文件操作在 HTTP handler 中通过 `spawn_blocking` 移至阻塞线程池。

### 11. Tauri 命令 (`src-tauri/src/tauri_cmds/`)

前端通过 Tauri IPC 调用的命令，全部为**薄包装**——仅提取 `State` 并委托给 services 层：

- `list_providers`, `add_provider`, `test_provider` → `provider_service`
- `get_settings`, `update_settings` → `settings_service`
- `start_proxy`, `stop_proxy` → `proxy_service`
- `get_system_status` → `system_service`
- `agent_detect`, `agent_preview_config`, `agent_apply_config`, `agent_restore_config`, `agent_list_backups` → `agent_service`
- `chat_completions_stream` — Tauri Channel 流式传输（本模块特有），取消逻辑委托 `chat_service`
- `cancel_chat_stream` → `chat_service::mark_cancelled`
- 托盘事件通过 `tray-action` 事件向前端广播（`start` / `stop`）

### 12. 智能体集成 (`vortex-gateway/src/agent_integrations/`)

自动检测本机已安装的 AI 编程智能体（Claude Code、Codex、OpenCode、Qwen Code、Gemini CLI、Cursor Agent 等 12 种），并一键将 Vortex 网关配置写入其配置文件。设计原则：只修改受管的 Vortex 节点、写入前备份、不读取用户凭据、幂等操作。

| 子模块 | 职责 |
|--------|------|
| `mod.rs` | `AgentKind` 枚举（serde 序列化为 snake_case，OpenCode 显式 rename 为 `opencode`）与适配器注册 |
| `adapters/` | 每种智能体一个适配器（detect / preview / apply / restore），负责定位安装路径与配置文件、生成配置预览 |
| `backup.rs` | `BackupManager`：写入前备份目标文件，支持按备份 ID 恢复 |
| `detect.rs` | 在 `PATH` 中查找可执行文件、解析配置目录（支持 `*_HOME` 环境变量覆盖） |
| `safe_write.rs` | 受管节点的安全 JSON 写入 |

前端入口在「接入指南」页（`src/components/sync/AgentIntegration.vue`）：检测结果缓存 1 小时，已安装的排在最前并带标识；未安装的不显示配置按钮。配置流程为 检测 → 预览变更（含警告与重启提示）→ 确认写入 → 可恢复。

运行时通过 `api/v1/mod.rs::detect_agent()` 从请求头（User-Agent / originator）识别调用方 Agent，写入 `ProxyRequest.agent` 并随 `UsageEntry.agent` 落库到 `usage_history`，请求日志页面展示"来源"列。

### 13. Tauri 插件

| 插件 | 用途 |
|------|------|
| `tauri-plugin-updater` | 自动更新：检查 GitHub Releases 新版本、下载并安装 |
| `tauri-plugin-process` | 进程管理：更新安装后重启应用 |
| `tauri-plugin-shell` | Shell 命令执行 |
| `tauri-plugin-autostart` | 开机自启 |

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
│   ├── 导航菜单 (主区 9 项 + 底部 2 项 + 折叠按钮)
│   └── 订阅项带连接数徽章
└── <main class="main"> (部分路由 flush 满高布局)
    ├── Header (56px)
    │   ├── 页面标题
    │   └── 健康状态标签 (每30s 轮询 /api/health)
    └── <router-view />
        ├── Guide        — 接入指南
        ├── LiveRouting  — 实时路由 + 网关状态（默认首页）
        ├── Subscriptions — 提供商连接管理（含新建 / 自定义 / 编辑子路由）
        ├── ModelAliases  — 模型映射（虚拟模型名 + 多目标故障转移）
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

`client.ts` 提供 axios 实例（baseURL: `http://localhost:10168/api`），`runtime.ts` 检测运行环境（`desktop` / `web`）。其余模块按调用方式分三类：

**双通道适配**（桌面端走 Tauri IPC `invoke`，Web 端走 HTTP `axios`）：

| 模块 | 函数 | 桌面端 | Web 端 |
|------|------|--------|--------|
| `system.ts` | `getSystemStatus` / `startProxy` / `stopProxy` | `invoke('get_system_status')` 等 | `GET /api/system/status` 等 |
| `agents.ts` | `detectAgents` / `previewConfig` / `applyConfig` / `restoreConfig` / `listBackups` | `invoke('agent_detect')` 等 | `GET /api/agents/detect` 等 |
| `chat.ts` | `streamCompletion` | Tauri Channel IPC | `fetch` SSE + `AbortController` |
| `chat.ts` | `cancelChatStream` | `invoke('cancel_chat_stream')` | `AbortController.abort()` |

**直连 HTTP**（两种模式都走 HTTP API）：

| 模块 | 数据来源 | 说明 |
|------|----------|------|
| `providers.ts` / `keys.ts` / `usage.ts` / `settings.ts` | 直连 `/api/*` | 与后端接口一一对应 |
| `freeTokens.ts` | 直连 `/api/free-tokens` | 站点列表 / 提交推荐 / 删除推荐 |
| `stats.ts` | 前端聚合 | 从 `/api/usage` 原始记录聚合出端点统计 |
| `sync.ts` | 混合 | cc-switch 迁移、本地配置导出/导入为真实实现；WebDAV 为 TODO 桩 |
| `notifications.ts` | 远程通知中心 | 从远程服务端拉取站内通知列表 |

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
| `useUpdater.ts` | 自动更新：检查 / 下载 / 安装 / 重启，启动时静默检查 + 运行期间每 6 小时周期复查（同一版本仅通知一次） |
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
3. detect_agent() 从 User-Agent/originator 头识别调用方 Agent
4. 调用 ProxyEngine::handle_request()
5.   ├── API Key 验证 (若 require_api_key=true)
6.   ├── resolve_route()
7.   │   └── ProviderRegistry::resolve_model_provider()
8.   │       └── 查 provider_connections 表找活跃连接
9.   └── handle_single_with_retry()
10.      ├── 检查熔断器状态
11.      ├── Executor::execute() → 上游 API 调用
12.      ├── 成功: 提取用量 → 记录 → 返回
13.      ├── 失败(5xx/429): 指数退避重试
14.      └── 失败(其他): 记录熔断器失败 → 返回错误
15. 记录 UsageEntry（含 agent 来源标识）到 usage_history
16. 返回 OpenAI 格式响应
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
| `VORTEX_FREE_TOKENS_REMOTE` | `https://hub.htui.cc/api/edge/free_tokens` | 免费 Token 远程服务地址 |
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
