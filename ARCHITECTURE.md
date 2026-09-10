# Vortex 架构文档

## 概述

Vortex 是一个 AI 网关桌面应用，采用 **Tauri 2** 框架，后端使用 **Rust**（Actix-Web + SQLite），前端使用 **Vue 3 + TypeScript**。其核心职责是将 OpenAI 兼容的 API 请求智能路由到多个上游 AI 提供商。

## 系统架构

```
                          ┌─────────────────────────────────────────────────┐
                          │              Vortex 桌面应用 (Tauri 2)            │
                          │                                                 │
   AI 客户端 ──────────── │  ┌───────────────────────────────────────────┐  │
   (OpenAI SDK,           │  │         Actix-Web HTTP Server :20128       │  │
    cURL, etc.)           │  │                                           │  │
                          │  │  /v1/chat/completions  ──┐                │  │
                          │  │  /v1/models             │                 │  │
                          │  │  /v1/embeddings         │                 │  │
                          │  │  /v1/images/generations │                 │  │
                          │  │                          ▼                 │  │
                          │  │                    ProxyEngine            │  │
                          │  │                          │                 │  │
                          │  │         ┌──────────────┼──────────────┐  │  │
                          │  │         │              │              │  │  │
                          │  │    API Key       Route Resolver    Usage   │  │
                          │  │    验证          (组合/模型)        记录   │  │
                          │  │                         │                 │  │
                          │  │              ┌──────────┴──────────┐     │  │
                          │  │              │                     │     │  │
                          │  │        RoutingEngine        ProviderRegistry│  │
                          │  │        (17 种策略)          (21 个提供商)  │  │
                          │  │              │                     │     │  │
                          │  │              ▼                     ▼     │  │
                          │  │         Executor (OpenAI / Anthropic / Gemini) │
                          │  │              │                     │     │  │
                          │  │         Retry + CircuitBreaker      │     │  │
                          │  │              │                     │     │  │
                          │  │         Translator (→ OpenAI 格式)  │     │  │
                          │  └───────────────────────────────────────────┘  │
                          │                                                 │
                          │  ┌───────────────────────────────────────────┐  │
                          │  │              SQLite (WAL 模式)              │  │
                          │  │  provider_connections | combos | api_keys  │  │
                          │  │  usage_history | key_value (settings)     │  │
                          │  └───────────────────────────────────────────┘  │
                          │                                                 │
                          │  ┌───────────────────────────────────────────┐  │
                          │  │           Vue 3 管理界面 (Tauri Webview)    │  │
                          │  │  Dashboard | Providers | Combos | Keys     │  │
                          │  │  Usage | Settings                          │  │
                          │  └───────────────────────────────────────────┘  │
                          └─────────────────────────────────────────────────┘
                                          │
                    ┌─────────────────────┼─────────────────────┐
                    │                     │                     │
               OpenAI API           Anthropic API          Gemini API
               (上游提供商)         (上游提供商)           (上游提供商)
```

## 核心模块

### 1. 应用状态 (`lib.rs`)

`AppState` 是全局共享状态，通过 `Arc<AppState>` 在所有请求间共享：

```rust
pub struct AppState {
    pub db_pool: DbPool,                    // SQLite 连接池 (r2d2, 最大8连接)
    pub config: AppConfig,                  // 配置 (端口、数据目录、加密密钥等)
    pub provider_registry: ProviderRegistry, // 21个内置提供商定义
    pub routing_engine: RoutingEngine,      // 17种路由策略注册表
    pub proxy_engine: RwLock<ProxyEngine>,  // 代理引擎
    pub resilience_manager: ResilienceManager, // 熔断器管理
    pub http_client: reqwest::Client,       // HTTP 客户端 (120s超时)
    pub encryption_key: Vec<u8>,            // AES-256-GCM 密钥
}
```

**启动流程** (`run()`):
1. 加载配置 (`AppConfig::load()`)
2. 创建应用状态 (`create_app_state()`) — 初始化 DB 连接池、运行迁移、创建各引擎
3. 启动 API 服务器 (`start_api_server()`) — 独立线程中运行 Actix-Web
4. 启动 Tauri 应用 — 带系统托盘 (Open Dashboard / Quit)

### 2. 代理引擎 (`proxy/engine.rs`)

`ProxyEngine` 是请求处理的核心，负责完整的请求生命周期：

```
请求进入 → API Key 验证 → 路由解析 → 执行(带重试+熔断) → 用量记录 → 返回
```

**关键方法：**

- `handle_request()` — 入口，协调整个流程
- `resolve_route()` — 路由解析：先查组合名匹配，再查 `provider/model` 格式解析
- `handle_single_with_retry()` — 单一提供商请求，带重试和熔断器
- `handle_combo_request()` — 组合请求，按策略排序的目标依次尝试
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

### 5. 路由策略引擎 (`routing/`)

#### 策略接口

```rust
pub trait RoutingStrategy: Send + Sync {
    fn select<'a>(
        &self,
        targets: &'a [ResolvedComboTarget],
        context: &RoutingContext,
    ) -> Result<Vec<&'a ResolvedComboTarget>>;
}
```

所有策略返回一个排序后的目标引用列表，代理引擎按顺序依次尝试。

#### 策略实现

| 策略 | 状态管理 | 核心逻辑 |
|------|----------|----------|
| `Priority` | 无状态 | 原序返回 |
| `FillFirst` | `AtomicUsize` + `Mutex<Vec<u64>>` | 填满配额(weight)后切换 |
| `Weighted` | 无状态 | 按权重随机选择主目标 |
| `RoundRobin` | `AtomicUsize` | 原子计数器取模 |
| `P2C` | 无状态 | 随机选2个，取权重大的 |
| `LeastUsed` | `Mutex<Vec<u64>>` | 维护使用计数，选最少的 |
| `Random` | 无状态 | 均匀随机选1个 |
| `StrictRandom` | 无状态 | 随机打乱全部 |
| `CostOptimized` | 无状态 | 按 `context.cost_catalog` 排序 |
| `Headroom` | 无状态 | 按剩余配额降序 |
| `ResetWindow` | 无状态 | 按配额重置时间升序 |
| `ResetAware` | 无状态 | 综合剩余配额和重置窗口评分 |
| `ContextRelay` | 无状态 | 按上下文窗口大小和成本评分 |
| `ContextOptimized` | 无状态 | 根据请求 token 数选择最佳上下文窗口 |
| `LKGP` | `Mutex<Option<String>>` | 粘性路由，记住上次成功的目标 |
| `Auto` | 无状态 | 9 因子综合评分 |
| `Fusion` | 无状态 | 扇出模式（当前实现为原序返回） |

#### `Auto` 策略评分因子

`Auto` 策略综合以下 9 个因子进行评分：

1. **权重** (`weight * 20.0`) — 用户配置的权重
2. **LKGP 偏好** (`+30.0`) — 上次成功的目标加分
3. **剩余配额** (`remaining * 0.3`) — 配额百分比
4. **重置窗口** (`+15.0`) — 5分钟内重置加分
5. **成本** (`-cost * 10.0`) — 成本惩罚
6. **延迟** (`-latency * 0.01`) — 延迟惩罚
7. **成功率** (`success_rate * 20.0`) — 历史成功率
8. **上下文窗口** — 根据请求大小适配
9. **熔断器状态** — 不可用的目标排除

### 6. 弹性机制 (`routing/resilience.rs`)

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
- 基硎延迟: 500ms
- 退避策略: 指数退避 (`500ms * 2^attempt`，上限 30s)
- 重试条件: HTTP 5xx 或 429 状态码

### 7. 提供商注册表 (`providers/registry.rs`)

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

### 8. 响应转换器 (`translator/mod.rs`)

将不同格式的响应统一转换为 OpenAI chat.completion 格式：

- `anthropic_to_openai_response()` — 转换 Anthropic 响应（content 数组、input_tokens/output_tokens、stop_reason）
- `gemini_to_openai_response()` — 转换 Gemini 响应（candidates/parts、usageMetadata、finishReason）

### 9. 数据库层 (`db/`)

#### 连接池配置

- 引擎: SQLite + r2d2 连接池
- 最大连接: 8，最小空闲: 2
- 模式: WAL (Write-Ahead Logging)
- 外键约束: 开启
- 忙超时: 5 秒

#### 数据表

| 表名 | 用途 | 关键字段 |
|------|------|----------|
| `provider_connections` | 提供商连接 | id, provider, api_key(加密), priority, is_active, backoff_level, rate_limited_until |
| `combos` | 组合定义 | id, name(唯一), data(JSON), sort_order |
| `api_keys` | 网关 API 密钥 | id, name, key(唯一, `vx-{uuid}`), allowed_models, rate_limits |
| `usage_history` | 用量记录 | provider, model, tokens_input/output, cost, latency_ms, status |
| `key_value` | 键值存储(设置) | namespace, key, value(JSON) |

#### 加密 (`db/encryption.rs`)

- 算法: AES-256-GCM
- 密钥派生: 从 passphrase 经 1000 轮混合哈希派生 32 字节密钥
- 存储: nonce (12字节) + ciphertext，Base64 编码
- 所有敏感字段（api_key, access_token, refresh_token）在存储时加密、读取时解密

### 10. API 端点 (`api/`)

#### OpenAI 兼容端点 (`/v1`)

| 端点 | 处理函数 | 说明 |
|------|----------|------|
| `POST /v1/chat/completions` | `chat::chat_completions` | 聊天补全，支持流式和非流式 |
| `GET /v1/models` | `models::list_models` | 聚合所有提供商和组合的模型列表 |
| `POST /v1/embeddings` | `embeddings::create_embeddings` | 文本嵌入 |
| `POST /v1/images/generations` | `images::create_images` | 图像生成 |

#### 管理端点 (`/api`)

| 端点 | 说明 |
|------|------|
| `GET/POST /api/providers` | 提供商连接列表/创建 |
| `GET/PATCH/DELETE /api/providers/{id}` | 单个连接 CRUD |
| `POST /api/providers/{id}/test` | 测试连接可用性 |
| `GET/POST /api/combos` | 组合列表/创建 |
| `GET/PATCH/DELETE /api/combos/{id}` | 单个组合 CRUD |
| `GET/POST /api/keys` | API 密钥列表/创建 |
| `GET/DELETE /api/keys/{id}` | 单个密钥操作 |
| `GET /api/usage` | 用量记录列表 |
| `GET /api/usage/stats` | 用量聚合统计 |
| `GET/PATCH /api/settings` | 设置读写 |
| `GET /api/health` | 健康检查 (DB 连通性 + 版本) |

### 11. Tauri 命令 (`tauri_cmds/`)

前端通过 Tauri IPC 调用的命令，功能与管理 API 类似但走 IPC 通道：

- `list_providers`, `add_provider`, `test_provider`
- `list_combos`, `save_combo`, `delete_combo`
- `get_settings`, `update_settings`

## 前端架构

### 技术栈

- **Vue 3.5** — Composition API + `<script setup>`
- **TypeScript** — 严格模式
- **Element Plus** — UI 组件库
- **Pinia** — 状态管理 (Composition API 风格)
- **Vue Router** — SPA 路由
- **ECharts** — 图表可视化
- **Axios** — HTTP 客户端 (baseURL: `http://localhost:20128/api`)

### 页面结构

```
AppLayout
├── Sidebar (220px, 可折叠)
│   ├── 品牌标识
│   ├── 导航菜单 (6项)
│   └── API 端点信息
├── Header (56px)
│   ├── 页面标题
│   └── 健康状态标签 (每30s检查)
└── <router-view />
    ├── Dashboard  — 统计卡片 + 状态列表
    ├── Providers  — 提供商卡片 + 连接管理
    ├── Combos     — 组合卡片 + 创建/编辑对话框
    ├── ApiKeys    — 密钥表格 + 创建对话框
    ├── Usage      — 统计卡片 + 用量进度条
    └── Settings   — 通用设置 + 路由配置
```

### 状态管理 (Pinia Stores)

| Store | 职责 |
|-------|------|
| `provider.ts` | 提供商列表和连接管理 |
| `combo.ts` | 组合列表管理 |
| `settings.ts` | 应用设置 |
| `usage.ts` | 用量统计 |

### 主题

暗色主题，主色 `#e54d5e`（红粉色），背景 `#0b0e14`（深黑蓝），通过 CSS 变量实现，覆盖 Element Plus 暗色主题。

## 请求处理流程

以 `POST /v1/chat/completions` 为例：

```
1. 请求到达 chat_completions()
2. 解析 OpenAI 格式请求体
3. 调用 ProxyEngine::handle_request()
4.   ├── API Key 验证 (若 require_api_key=true)
5.   ├── resolve_route()
6.   │   ├── 查 combos 表匹配组合名
7.   │   │   └── resolve_combo() → 按策略排序目标
8.   │   └── ProviderRegistry::resolve_model_provider()
9.   │       └── 查 provider_connections 表找活跃连接
10.  ├── 若组合: handle_combo_request()
11.  │   └── 遍历排序后的目标，依次 handle_single_with_retry()
12.  └── 若单一: handle_single_with_retry()
13.      ├── 检查熔断器状态
14.      ├── Executor::execute() → 上游 API 调用
15.      ├── 成功: 提取用量 → 记录 → 返回
16.      ├── 失败(5xx/429): 指数退避重试
17.      └── 失败(其他): 记录熔断器失败 → 返回错误
18. 记录 UsageEntry 到 usage_history
19. 返回 OpenAI 格式响应
```

## 配置管理

### 配置加载 (`config.rs`)

`AppConfig` 从环境变量加载，支持：

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `VORTEX_PORT` | 20128 | API 服务器端口 |
| `VORTEX_DATA_DIR` | 系统数据目录/vortex | 数据存储目录 |
| `VORTEX_ENCRYPTION_KEY` | 自动生成并持久化 | 加密密钥 (32字节十六进制) |
| `VORTEX_REQUIRE_API_KEY` | false | 是否要求客户端 API Key |
| `VORTEX_LOG_LEVEL` | info | 日志级别 |

### 设置存储

设置存储在 `key_value` 表中，分两个命名空间：
- `settings/general` — 通用设置 (port, requireApiKey, theme)
- `settings/routing` — 路由设置 (defaultStrategy, autoComboEnabled)

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
