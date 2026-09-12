<p align="center">
  <img src="src-tauri/icons/logo.png" width="120" alt="Vortex Logo" />
</p>

# Vortex AI Gateway

> 统一的 AI 网关桌面应用 — 将多家 AI 提供商聚合为 OpenAI / Anthropic 兼容 API

Vortex 是一个基于 **Tauri 2 (Rust + Vue 3)** 构建的桌面 AI 网关应用。它在本地启动一个 OpenAI 兼容的 API 服务器，将请求路由到多个 AI 提供商（OpenAI、Anthropic、Google Gemini、DeepSeek 等），支持弹性重试、API 密钥管理和用量统计。

## 特性

- **多家内置 AI 提供商** — OpenAI、Anthropic、Google Gemini、DeepSeek、Groq、xAI、Mistral、OpenRouter、Cohere、Together AI、Fireworks AI、Cerebras、NVIDIA NIM、Cloudflare AI、Ollama、SiliconFlow、HuggingFace、Pollinations、Perplexity、Qwen、MiniMax、Z.AI (GLM)，以及自定义 OpenAI 兼容端点
- **OpenAI / Anthropic 双协议入口** — `/v1/chat/completions` 与 `/v1/messages` 两套端点，无需修改客户端代码，直接替换 `base_url` 即可
- **跨格式转换** — 自动将 Anthropic/Gemini 请求和响应转换为 OpenAI 格式，包括 SSE 流式响应
- **弹性机制** — 熔断器 + 指数退避重试，保障上游故障时的可用性
- **模型映射 + 故障转移** — 自定义虚拟模型名映射到多个真实模型目标，按优先级自动故障转移，免费额度组成"永动机"
- **安全存储** — API 密钥使用 AES-256-GCM 加密存储
- **用量统计** — 按提供商、模型、时间维度记录请求数、Token 用量和成本
- **免费 Token 目录** — 内置 43 个可申请免费额度的 AI 平台（国内 / 海外 / 本地部署），标注是否支持 API、是否需绑卡与实名，支持自行提交推荐并保存到本地
- **内置对话** — 应用内直接对话测试，支持会话分支树、模型选择、流式输出，数据持久化到 localStorage
- **模型选择对话框** — 获取远程模型后弹出对话框，checkbox 多选批量管理，支持搜索与全选
- **自动更新** — 基于 tauri-plugin-updater，启动时静默检查 GitHub Releases，发现新版本提示下载安装
- **通知系统** — 从远程通知中心拉取站内通知，未读通知弹出桌面提示，已读状态持久化到本地
- **桌面应用** — 系统托盘常驻（显示窗口 / 启动代理 / 停止代理 / 退出），Vue 3 管理界面，多窗口支持（主窗口 + 状态面板小窗口）

## 快速开始

### 直接下载

预编译安装包发布在 [GitHub Releases](https://github.com/ht-shaipe/vortex/releases/latest)：

| 平台 | 产物 |
|---|---|
| Windows 10 / 11 · x64 | `Vortex_<版本>_x64-setup.exe`（NSIS）、`Vortex_<版本>_x64_en-US.msi` |
| macOS · Apple 芯片 | `Vortex_<版本>_aarch64.dmg` |
| macOS · Intel | `Vortex_<版本>_x64.dmg` |
| Linux · x64 | `Vortex_<版本>_amd64.AppImage`、`Vortex_<版本>_amd64.deb` |

产物由 `.github/workflows/release.yml` 在推送 `v*` tag（或手动触发 workflow_dispatch）时构建并发布。
官网的下载区会在运行时拉取最新 Release，把按钮直接指向上述文件，因此**文件名带版本号不需要手工维护**。

> macOS 产物未做 Apple 公证，首次打开需右键 → 打开；Windows 若弹 SmartScreen 提示，选「更多信息 → 仍要运行」。

#### macOS 提示「已损坏，无法打开」

从浏览器下载的 `.dmg` 安装后，macOS Gatekeeper 会给应用打上隔离属性（`com.apple.quarantine`），导致打开时提示 **「"Vortex"已损坏，无法打开。你应该将它移到废纸篓。」**。应用本身没有损坏，清除隔离属性即可：

```bash
xattr -cr /Applications/Vortex.app
```

在终端中执行上述命令后，再双击打开 Vortex 即可正常运行。如果仍无法打开，前往 **系统设置 → 隐私与安全性**，在底部找到关于 Vortex 的提示，点击「仍要打开」。

### 环境要求

- [Rust](https://rustup.rs/) 1.70+ (stable)
- [Bun](https://bun.sh/)（仓库带 `bun.lock`，推荐）或 [Node.js](https://nodejs.org/) 20.19+ / 22.12+
- [Tauri 2 CLI](https://v2.tauri.app/) 前置依赖（参见 [Tauri 官方文档](https://v2.tauri.app/start/prerequisites/)）

### 从源码安装与运行

```bash
# 安装前端依赖
bun install        # 或 npm install / pnpm install

# 开发模式（同时启动前端和 Rust 后端）
bun run tauri dev

# 构建生产版本
bun run tauri build
```

开发模式下：
- 前端开发服务器：`http://localhost:1420`
- API 网关服务器：`http://localhost:10168`

### 使用网关

启动后，将你的 AI 客户端的 `base_url` 指向 Vortex：

```
http://localhost:10168/v1
```

**示例 — 使用 OpenAI Python SDK：**

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://localhost:10168/v1",
    api_key="vx-4f2a9c1e8b7d4a6f9c3e2b1a8d5f7c40"  # 通过 POST /api/keys 创建的网关密钥
)

response = client.chat.completions.create(
    model="openai/gpt-4o",        # 使用 provider/model 格式
    messages=[{"role": "user", "content": "Hello!"}]
)
```

**示例 — 使用 cURL：**

```bash
curl http://localhost:10168/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer vx-4f2a9c1e8b7d4a6f9c3e2b1a8d5f7c40" \
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
| Z.AI (GLM) | `zi` | openai | llm | ✓ |
| Custom | `cx` | openai | llm, embedding | - |

## 配置

### 环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `VORTEX_PORT` | `10168` | API 服务器端口 |
| `VORTEX_DATA_DIR` | 系统数据目录/vortex | 数据存储目录 |
| `VORTEX_ENCRYPTION_KEY` | 自动生成 | API 密钥加密密钥（32 字节十六进制） |
| `VORTEX_REQUIRE_API_KEY` | `false` | 是否要求客户端提供 API Key |
| `VORTEX_FREE_TOKENS_REMOTE` | `https://hub.htui.cc/api/edge/free_tokens` | 免费 Token 远程服务地址 |
| `VORTEX_LOG_LEVEL` | `info` | 日志级别 |

### 管理界面

打开桌面应用可访问以下页面（左侧导航，从上到下）：

| 页面 | 路由 | 说明 |
|------|------|------|
| 接入指南 | `/guide` | 客户端接入示例与模型指定方式 |
| 实时路由 | `/live-routing` | 网关拓扑、Base URL、API 端点一览（默认首页） |
| 订阅 | `/subscriptions` | 提供商连接管理 —— 添加 API Key、测试连接；含新建 / 自定义 / 编辑子页 |
| 模型映射 | `/model-aliases` | 虚拟模型名与多目标故障转移配置 |
| 免费 Token | `/free-tokens` | 免费额度站点目录 —— 卡片 / 表格双视图、按区域与「是否支持 API」筛选、提交与删除本地推荐 |
| 请求日志 | `/request-log` | 请求流水与用量明细 |
| 统计 | `/statistics` | 端点统计（KPI / 热力图 / 趋势 / 端点表）、用量统计（应用来源 / 日模型明细） |
| 同步 | `/sync` | cc-switch 迁移、WebDAV 云备份、本地配置导出与导入 |
| 对话 | `/chat` | 内置对话客户端 —— 会话分支树、模型选择、流式输出 |
| 设置 | `/settings` | 通用设置 / 高级设置 |
| 关于 | `/about` | 版本与项目信息、检查更新 |

> **同步页现状**：cc-switch 迁移与本地配置导出 / 导入已可用；WebDAV 的测试、备份、恢复与云端备份列表需要后端命令支持，当前 UI 已就绪，调用会提示「后端未接入」。适配层位于 `src/api/sync.ts`，接入后只需替换其中的桩函数。

## API 端点

### OpenAI 兼容端点 (`/v1`)

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/v1/chat/completions` | 聊天补全（支持流式） |
| POST | `/v1/messages` | Anthropic Messages 兼容（协议转换，支持流式） |
| GET | `/v1/models` | 模型列表 |
| POST | `/v1/embeddings` | 文本嵌入 |
| POST | `/v1/images/generations` | 图像生成 |

### 管理端点 (`/api`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET/POST | `/api/providers` | 提供商连接列表/创建 |
| GET/PATCH/DELETE | `/api/providers/{id}` | 单个提供商连接操作 |
| POST | `/api/providers/{id}/test` | 测试提供商连接 |
| GET | `/api/providers/{id}/apikey` | 获取解密后的真实 API 密钥（用于前端复制） |
| POST | `/api/providers/preview-models` | 预览远程可用模型列表（无需先保存连接） |
| GET/POST | `/api/keys` | API 密钥列表/创建 |
| GET/DELETE | `/api/keys/{id}` | 单个 API 密钥操作 |
| GET | `/api/usage` | 用量记录 |
| GET | `/api/usage/stats` | 用量统计 |
| GET/PATCH | `/api/settings` | 设置 |
| GET/POST | `/api/free-tokens` | 免费 Token 站点列表 / 提交推荐 |
| DELETE | `/api/free-tokens/{id}` | 删除用户提交的推荐（内置条目不可删） |
| GET/POST | `/api/model-aliases` | 模型别名列表/创建 |
| GET/PATCH/DELETE | `/api/model-aliases/{id}` | 单个模型别名操作 |
| GET | `/api/health` | 健康检查 |

详细架构与内部实现参见 [ARCHITECTURE.md](./ARCHITECTURE.md)。

## 技术栈

### 后端 (Rust)
- Tauri 2 — 桌面应用框架
- Actix-Web 4 — HTTP 服务器
- rusqlite + r2d2 — SQLite 数据库（WAL 模式，连接池）
- reqwest — HTTP 客户端（native-tls + HTTP/2）
- aes-gcm — API 密钥加密
- tokio — 异步运行时
- tauri-plugin-updater — 自动更新
- tauri-plugin-process — 进程管理（重启应用）
- tauri-plugin-autostart — 开机自启

### 前端 (Vue 3)
- Vue 3.5 + TypeScript (strict) — UI 框架
- Element Plus — UI 组件库
- Ant Design Vue — 辅助组件库（a-card、a-timeline 等）
- UnoCSS — 原子化 CSS
- Pinia — 状态管理
- Vue Router — 路由（Web 端 hash / Tauri 端 history 模式）
- ECharts 6 + vue-echarts — 图表可视化
- highlight.js — 语法高亮
- Vite — 构建工具

## 项目结构

```
vortex/
├── src/                    # 前端源代码 (Vue 3 + TypeScript)
│   ├── api/                # 请求层与适配层 (client / providers / keys / usage / settings / stats / chat / sync / freeTokens / notifications)
│   ├── components/         # Vue 组件 (layout / chat / stats / sync / ui)
│   ├── composables/        # 组合式函数 (useTheme / useThemeColors / useUpdater / useNotifications)
│   ├── lib/                # 工具函数 (range / dateRange / format / usageChart / runtime)
│   ├── router/             # 路由配置
│   ├── stores/             # Pinia 状态管理
│   ├── styles/             # 设计系统 (cc-theme.css / cc-components.css)
│   ├── types/              # 类型定义
│   └── views/              # 页面视图 (16 个 .vue 文件)
├── src-tauri/              # 后端源代码 (Rust + Tauri)
│   └── src/
│       ├── api/            # HTTP API 端点 (v1 OpenAI 兼容 + management 管理接口)
│       ├── db/             # 数据库操作与迁移
│       ├── providers/      # 提供商注册表
│       ├── proxy/          # 代理引擎 (engine / executor / retry / sse)
│       ├── routing/        # 熔断器等弹性组件
│       ├── tauri_cmds/     # Tauri IPC 命令
│       └── translator/     # 响应格式转换器
├── docs/                   # 项目官网与文档
│   ├── index.html          # 官网落地页（原生 HTML/CSS/JS 零依赖）
│   ├── wechat/             # 公众号系列文章
│   └── promo/              # 视频脚本等宣传材料
├── package.json            # 前端配置
└── src-tauri/Cargo.toml    # 后端配置
```

详细架构参见 [ARCHITECTURE.md](./ARCHITECTURE.md)。

## 开发

```bash
# 前端开发
bun run dev          # Vite 开发服务器 (http://localhost:1420)
bun run build        # 类型检查 (vue-tsc) + 构建
bun run preview      # 预览构建产物

# 后端开发
cd src-tauri
cargo build          # 编译
cargo test           # 运行测试
cargo clippy         # 代码检查

# 完整开发模式
bun run tauri dev
```

> **类型检查的已知问题**：`bun run build` 会先跑 `vue-tsc --noEmit`。当前 `typescript@7` 与 `vue-tsc@3` 组合会抛 `ERR_PACKAGE_PATH_NOT_EXPORTED: './lib/tsc' is not defined by "exports"`，属于依赖版本兼容问题而非代码错误。需要验证构建产物时直接执行 `npx vite build`（跳过类型检查）。

## 官网

`docs/index.html` 是项目的独立静态落地页，原生 HTML/CSS/JS 实现、零构建依赖，配色沿用应用主题（`src/styles/cc-theme.css`）的深紫强调色。

```bash
# 直接打开
open docs/index.html

# 或起一个本地静态服务
python3 -m http.server 8080 --directory docs
```

页面结构：Hero → 数据概览 → 接入范围 → 特性 → 界面预览 → 快速开始 → 请求流程 → 下载安装 → FAQ。
截图位于 `docs/assets/screens/`，取自本机开发实例，更新 UI 后可重新截取替换。

> 下载区的按钮由 `docs/assets/app.js` 在运行时请求 `api.github.com` 解析最新 Release 的产物地址，
> 静态 HTML 里只放「指向 Releases 页面」的兜底链接 —— 因为安装包文件名带版本号，写死必然过期。

> 官网刻意不逐个列出上游平台的厂商名：站点只讲接入能力（云端 API / 聚合中转 / 本地推理 / 自定义端点）与协议兼容性，具体清单以应用内「订阅」页和本 README 的「内置提供商」表为准，避免站点与注册表脱节。

## 许可证

本项目基于 [MIT 许可证](./LICENSE) 开源。你可以自由使用、修改、分发和商用本软件，但须保留版权声明与许可证文本。

> Copyright (c) 2026 Vortex
