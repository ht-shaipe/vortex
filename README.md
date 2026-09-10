# Vortex AI Gateway

> 统一的 AI 网关桌面应用 — 将 21+ 个 AI 提供商聚合为 OpenAI 兼容 API

Vortex 是一个基于 **Tauri 2 (Rust + Vue 3)** 构建的桌面 AI 网关应用。它在本地启动一个 OpenAI 兼容的 API 服务器，将请求智能路由到多个 AI 提供商（OpenAI、Anthropic、Google Gemini、DeepSeek 等），支持 17 种路由策略、组合模型、API 密钥管理和用量统计。

## 特性

- **21 个内置 AI 提供商** — OpenAI、Anthropic、Google Gemini、DeepSeek、Groq、xAI、Mistral、OpenRouter、Cohere、Together AI、Fireworks AI、Cerebras、NVIDIA NIM、Cloudflare AI、Ollama、SiliconFlow、HuggingFace、Pollinations、Perplexity、Qwen、MiniMax，以及自定义 OpenAI 兼容端点
- **17 种路由策略** — 从简单的优先级/轮询到 9 因子自动评分、P2C 负载均衡、成本优化等
- **组合模型** — 将多个模型/提供商组合为虚拟模型，按策略自动调度
- **OpenAI 兼容 API** — 无需修改客户端代码，直接替换 `base_url` 即可
- **跨格式转换** — 自动将 Anthropic/Gemini 请求和响应转换为 OpenAI 格式，包括 SSE 流式响应
- **弹性机制** — 熔断器 + 指数退避重试，保障上游故障时的可用性
- **安全存储** — API 密钥使用 AES-256-GCM 加密存储
- **用量统计** — 按提供商、模型、时间维度记录请求数、Token 用量和成本
- **桌面应用** — 系统托盘后台运行，Vue 3 管理界面

## 快速开始

### 环境要求

- [Rust](https://rustup.rs/) 1.70+ (stable)
- [Node.js](https://nodejs.org/) 18+ 或 [Bun](https://bun.sh/)
- [Tauri 2 CLI](https://v2.tauri.app/) 前置依赖（参见 [Tauri 官方文档](https://v2.tauri.app/start/prerequisites/)）

### 安装与运行

```bash
# 安装前端依赖
npm install        # 或 bun install

# 开发模式（同时启动前端和 Rust 后端）
npm run tauri dev

# 构建生产版本
npm run tauri build
```

开发模式下：
- 前端开发服务器：`http://localhost:1420`
- API 网关服务器：`http://localhost:20128`

### 使用网关

启动后，将你的 AI 客户端的 `base_url` 指向 Vortex：

```
http://localhost:20128/v1
```

**示例 — 使用 OpenAI Python SDK：**

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://localhost:20128/v1",
    api_key="your-vortex-api-key"  # 在管理界面创建的密钥
)

response = client.chat.completions.create(
    model="openai/gpt-4o",        # 使用 provider/model 格式
    messages=[{"role": "user", "content": "Hello!"}]
)
```

**示例 — 使用 cURL：**

```bash
curl http://localhost:20128/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer your-vortex-api-key" \
  -d '{
    "model": "deepseek/deepseek-chat",
    "messages": [{"role": "user", "content": "Hello!"}]
  }'
```

### 模型指定方式

| 格式 | 示例 | 说明 |
|------|------|------|
| `provider/model` | `openai/gpt-4o` | 显式指定提供商和模型 |
| `alias-model` | `ds-deepseek-chat` | 使用提供商别名前缀 |
| 组合名 | `my-combo` | 使用预定义的组合模型 |

## 路由策略

| 策略 | 说明 |
|------|------|
| `priority` | 按顺序尝试，使用第一个可用的 |
| `fill_first` | 填满每个目标的配额再切换 |
| `weighted` | 按权重随机选择 |
| `round_robin` | 轮询 |
| `p2c` | Power of Two Choices 随机负载均衡 |
| `least_used` | 选择当前负载最低的 |
| `random` | 均匀随机选择（去重） |
| `strict_random` | 随机打乱全部 |
| `cost_optimized` | 按实时定价最小化成本 |
| `headroom` | 选择剩余配额最多的 |
| `reset_window` | 优先配额重置最快的 |
| `reset_aware` | 综合剩余配额和重置窗口评分 |
| `context_relay` | 按上下文窗口大小和成本评分 |
| `context_optimized` | 根据请求 token 数选择最佳上下文窗口 |
| `lkgp` | Last-Known-Good-Path 粘性路由 |
| `auto` | 9 因子综合评分（权重、LKGP、配额、重置窗口、成本、延迟等） |
| `fusion` | 扇出到多个模型 + 评判合成 |

## 内置提供商

| 提供商 | 别名 | API 格式 | 服务类型 | 免费层 |
|--------|------|----------|----------|--------|
| OpenAI | `oa` | openai | llm, embedding, image | - |
| Anthropic | `an` | anthropic | llm | - |
| Google Gemini | `gm` | gemini | llm, embedding | ✓ |
| DeepSeek | `ds` | openai | llm | ✓ |
| Groq | `gq` | openai | llm | ✓ |
| xAI (Grok) | `xa` | openai | llm | - |
| Mistral | `ml` | openai | llm, embedding | - |
| OpenRouter | `or` | openai | llm, image | ✓ |
| Cohere | `ch` | cohere | llm, embedding, rerank | ✓ |
| Together AI | `tg` | openai | llm, image | ✓ |
| Fireworks AI | `fw` | openai | llm, image | - |
| Cerebras | `cb` | openai | llm | ✓ |
| NVIDIA NIM | `nv` | openai | llm, embedding | ✓ |
| Cloudflare AI | `cf` | cloudflare | llm, embedding, image | ✓ |
| Ollama (Local) | `ol` | openai | llm, embedding | ✓ (无认证) |
| SiliconFlow | `sf` | openai | llm, image | ✓ |
| HuggingFace | `hf` | openai | llm, embedding | ✓ |
| Pollinations | `pl` | openai | llm | ✓ (无认证) |
| Perplexity | `pp` | openai | llm, webSearch | - |
| Qwen (通义千问) | `qw` | openai | llm, embedding | ✓ |
| MiniMax | `mm` | openai | llm | - |
| Custom | `cx` | openai | llm, embedding | - |

## 配置

### 环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `VORTEX_PORT` | `20128` | API 服务器端口 |
| `VORTEX_DATA_DIR` | 系统数据目录/vortex | 数据存储目录 |
| `VORTEX_ENCRYPTION_KEY` | 自动生成 | API 密钥加密密钥（32 字节十六进制） |
| `VORTEX_REQUIRE_API_KEY` | `false` | 是否要求客户端提供 API Key |
| `VORTEX_LOG_LEVEL` | `info` | 日志级别 |

### 管理界面

打开桌面应用可访问以下管理页面：

- **Dashboard** — 总览统计和状态
- **Providers** — 提供商连接管理（添加 API Key、测试连接）
- **Combos** — 组合模型管理（创建/编辑路由策略和模型步骤）
- **API Keys** — 网关 API 密钥管理
- **Usage** — 用量统计和成本分析
- **Settings** — 通用设置和路由配置

## API 端点

### OpenAI 兼容端点 (`/v1`)

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/v1/chat/completions` | 聊天补全（支持流式） |
| GET | `/v1/models` | 模型列表 |
| POST | `/v1/embeddings` | 文本嵌入 |
| POST | `/v1/images/generations` | 图像生成 |

### 管理端点 (`/api`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET/POST | `/api/providers` | 提供商连接列表/创建 |
| GET/PATCH/DELETE | `/api/providers/{id}` | 单个提供商连接操作 |
| POST | `/api/providers/{id}/test` | 测试提供商连接 |
| GET/POST | `/api/combos` | 组合列表/创建 |
| GET/PATCH/DELETE | `/api/combos/{id}` | 单个组合操作 |
| GET/POST | `/api/keys` | API 密钥列表/创建 |
| GET/DELETE | `/api/keys/{id}` | 单个 API 密钥操作 |
| GET | `/api/usage` | 用量记录 |
| GET | `/api/usage/stats` | 用量统计 |
| GET/PATCH | `/api/settings` | 设置 |
| GET | `/api/health` | 健康检查 |

详细 API 文档参见 [API.md](./API.md)。

## 技术栈

### 后端 (Rust)
- Tauri 2 — 桌面应用框架
- Actix-Web 4 — HTTP 服务器
- rusqlite + r2d2 — SQLite 数据库（WAL 模式，连接池）
- reqwest — HTTP 客户端（rustls-tls）
- aes-gcm — API 密钥加密
- tokio — 异步运行时

### 前端 (Vue 3)
- Vue 3.5 + TypeScript — UI 框架
- Element Plus — UI 组件库
- Pinia — 状态管理
- Vue Router — 路由
- ECharts — 图表可视化
- Vite — 构建工具

## 项目结构

```
vortex/
├── src/                    # 前端源代码 (Vue 3 + TypeScript)
│   ├── api/                # API 请求层
│   ├── components/         # Vue 组件
│   ├── router/             # 路由配置
│   ├── stores/             # Pinia 状态管理
│   ├── types/              # 类型定义
│   └── views/              # 页面视图
├── src-tauri/              # 后端源代码 (Rust + Tauri)
│   └── src/
│       ├── api/            # HTTP API 端点
│       ├── db/             # 数据库操作
│       ├── providers/      # 提供商注册表
│       ├── proxy/          # 代理引擎
│       ├── routing/        # 路由策略引擎
│       ├── tauri_cmds/     # Tauri IPC 命令
│       └── translator/     # 响应格式转换器
├── package.json            # 前端配置
└── src-tauri/Cargo.toml    # 后端配置
```

详细架构参见 [ARCHITECTURE.md](./ARCHITECTURE.md)。

## 开发

```bash
# 前端开发
npm run dev          # Vite 开发服务器
npm run build        # 类型检查 + 构建

# 后端开发
cd src-tauri
cargo build          # 编译
cargo test           # 运行测试
cargo clippy         # 代码检查

# 完整开发模式
npm run tauri dev
```

## 许可证

私有项目，未开源。
