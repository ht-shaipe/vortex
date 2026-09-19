-- ============================================================
-- 002 · 免费 Token 站点目录
-- 收录可申请免费额度的 AI 平台，供用户在「免费 Token」页浏览、
-- 筛选，并自行提交推荐站点。
-- ============================================================

CREATE TABLE IF NOT EXISTS free_token_sites (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    home_url        TEXT NOT NULL DEFAULT '',
    apply_url       TEXT NOT NULL DEFAULT '',
    api_supported   INTEGER NOT NULL DEFAULT 1,
    api_base        TEXT,
    api_format      TEXT,
    free_quota      TEXT NOT NULL DEFAULT '',
    region          TEXT NOT NULL DEFAULT 'global',
    requires_card   INTEGER NOT NULL DEFAULT 0,
    requires_verify INTEGER NOT NULL DEFAULT 0,
    tags            TEXT NOT NULL DEFAULT '[]',
    note            TEXT,
    provider_id     TEXT,
    source          TEXT NOT NULL DEFAULT 'builtin',
    submitter       TEXT,
    sort_order      INTEGER NOT NULL DEFAULT 5000,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_free_token_sites_region ON free_token_sites(region);
CREATE INDEX IF NOT EXISTS idx_free_token_sites_source ON free_token_sites(source);

-- ------------------------------------------------------------
-- 内置站点种子数据
-- region: cn = 国内容易直连 / global = 海外 / local = 本地部署
-- requires_card: 是否需要绑定信用卡
-- requires_verify: 是否需要实名或手机号验证
-- 免费额度随平台政策变动，仅作参考，以各平台官网为准。
-- ------------------------------------------------------------
INSERT OR IGNORE INTO free_token_sites
    (id, name, home_url, apply_url, api_supported, api_base, api_format, free_quota,
     region, requires_card, requires_verify, tags, note, provider_id, source, sort_order)
VALUES
    -- ===================== 国内平台 =====================
    ('cn-siliconflow', '硅基流动 SiliconFlow', 'https://cloud.siliconflow.cn', 'https://cloud.siliconflow.cn/account/ak',
     1, 'https://api.siliconflow.cn/v1', 'openai',
     '新用户注册赠约 2000 万 tokens；9B 及以下开源模型长期免费', 'cn', 0, 1,
     '["聚合平台","OpenAI 兼容","首推"]',
     '一个 Key 可调 DeepSeek / Qwen / GLM / Llama 等几十款模型，中文文档完善，国内低延迟',
     'siliconflow', 'builtin', 101),

    ('cn-zhipu', '智谱 AI BigModel', 'https://bigmodel.cn', 'https://bigmodel.cn/usercenter/apikeys',
     1, 'https://open.bigmodel.cn/api/paas/v4', 'openai',
     'GLM-Flash 系列长期免费；新用户另赠 tokens 额度', 'cn', 0, 1,
     '["国产自研","OpenAI 兼容","长期免费"]',
     'GLM-Flash 免费档为单并发，适合个人调试与轻量应用',
     NULL, 'builtin', 102),

    ('cn-bailian', '阿里云百炼（通义千问）', 'https://bailian.console.aliyun.com', 'https://bailian.console.aliyun.com',
     1, 'https://dashscope.aliyuncs.com/compatible-mode/v1', 'openai',
     'qwen-turbo 等轻量模型长期免费；新用户各模型百万级 tokens 额度', 'cn', 0, 1,
     '["云厂商","OpenAI 兼容","多模态"]',
     '模型种类最全，Qwen 全系覆盖文本 / 视觉 / 代码 / 语音',
     'qwen', 'builtin', 103),

    ('cn-volcengine', '火山方舟（豆包）', 'https://console.volcengine.com/ark', 'https://console.volcengine.com/ark',
     1, 'https://ark.cn-beijing.volces.com/api/v3', 'openai',
     '新用户赠送 tokens；部分模型提供每日刷新的免费额度', 'cn', 0, 1,
     '["云厂商","字节跳动","OpenAI 兼容"]',
     '豆包系列中文对话稳定，额度与限速以控制台实时显示为准',
     NULL, 'builtin', 104),

    ('cn-modelscope', '魔搭 ModelScope（阿里）', 'https://modelscope.cn', 'https://modelscope.cn/my/myaccesstoken',
     1, 'https://api-inference.modelscope.cn/v1', 'openai',
     '每日免费调用额度（公测期约 2000 次/日），热门模型额度动态调整', 'cn', 0, 1,
     '["开源模型","社区","OpenAI 兼容"]',
     '不用自建推理即可调用海量开源 LLM、多模态与文生图模型',
     'huggingface', 'builtin', 105),

    ('cn-qianfan', '百度千帆（文心）', 'https://qianfan.cloud.baidu.com', 'https://console.bce.baidu.com/iam/#/iam/apikey/list',
     1, 'https://qianfan.baidubce.com/v2', 'openai',
     'ERNIE-Speed / Tiny 等轻量模型长期免费；新用户赠送 tokens', 'cn', 0, 1,
     '["云厂商","中文优化","OpenAI 兼容"]',
     '中文理解与文档解析能力强，免费档并发有限',
     NULL, 'builtin', 106),

    ('cn-hunyuan', '腾讯云混元', 'https://cloud.tencent.com/product/hunyuan', 'https://console.cloud.tencent.com/hunyuan/api-key',
     1, 'https://api.hunyuan.cloud.tencent.com/v1', 'openai',
     '新用户大额体验包；hunyuan-lite 提供免费层', 'cn', 0, 1,
     '["云厂商","OpenAI 兼容"]',
     '腾讯生态集成，免费档并发较低',
     NULL, 'builtin', 107),

    ('cn-spark', '讯飞星火', 'https://xinghuo.xfyun.cn', 'https://console.xfyun.cn/services/cbm',
     1, 'https://spark-api-open.xf-yun.com/v1', 'openai',
     'Spark Lite 长期免费；新用户赠送 tokens', 'cn', 0, 1,
     '["国产自研","语音能力","OpenAI 兼容"]',
     '语音识别 / 合成能力突出，Lite 版适合入门试用',
     NULL, 'builtin', 108),

    ('cn-moonshot', '月之暗面 Kimi', 'https://platform.moonshot.cn', 'https://platform.moonshot.cn/console/api-keys',
     1, 'https://api.moonshot.cn/v1', 'openai',
     '新用户注册赠小额额度；无长期免费层', 'cn', 0, 1,
     '["长上下文","国产自研"]',
     '长文本处理口碑好，额度用完需充值',
     NULL, 'builtin', 109),

    ('cn-deepseek', 'DeepSeek 开放平台', 'https://platform.deepseek.com', 'https://platform.deepseek.com/api_keys',
     1, 'https://api.deepseek.com/v1', 'openai',
     '无长期免费层；新用户偶有试用额度活动', 'cn', 0, 1,
     '["国产自研","高性价比","OpenAI 兼容"]',
     '价格极低但需付费，适合作为正式生产环境后端',
     'deepseek', 'builtin', 110),

    ('cn-minimax', 'MiniMax 开放平台', 'https://platform.minimaxi.com', 'https://platform.minimaxi.com/user-center/basic-information/interface-key',
     1, 'https://api.minimaxi.com/v1', 'openai',
     '新用户赠送体验额度', 'cn', 0, 1,
     '["多模态","语音克隆","国产自研"]',
     '语音与视频生成能力有特色，文本模型同时提供',
     'minimax', 'builtin', 111),

    ('cn-huawei', '华为云 ModelArts Studio', 'https://www.huaweicloud.com/product/modelarts.html', 'https://console.huaweicloud.com/modelarts/',
     1, NULL, 'openai',
     '新用户可领取免费推理额度（含 DeepSeek 系列）', 'cn', 0, 1,
     '["云厂商","免费试用"]',
     '需完成华为云实名认证后领取，额度有效期以活动页为准',
     NULL, 'builtin', 112),

    ('cn-infini', '无问芯穹 Infini-AI', 'https://cloud.infini-ai.com', 'https://cloud.infini-ai.com/platform/ai',
     1, 'https://cloud.infini-ai.com/maas/v1', 'openai',
     '注册赠送免费 tokens，部分模型长期免费', 'cn', 0, 1,
     '["国产算力","OpenAI 兼容"]',
     '聚合多家国产算力上的开源模型，适合做备用通道',
     NULL, 'builtin', 113),

    ('cn-sensetime', '商汤日日新 SenseNova', 'https://platform.sensetime.com', 'https://platform.sensetime.com/cloud/api-key',
     1, 'https://api.sensenova.cn/compatible-mode/v1', 'openai',
     '新用户赠送 tokens 试用额度', 'cn', 0, 1,
     '["国产自研","多模态"]',
     '多模态与视觉理解见长',
     NULL, 'builtin', 114),

    -- ===================== 海外平台 =====================
    ('gl-google-ai-studio', 'Google AI Studio (Gemini)', 'https://aistudio.google.com', 'https://aistudio.google.com/apikey',
     1, 'https://generativelanguage.googleapis.com/v1beta/openai', 'openai',
     'Flash / Flash-Lite 级别模型免费，约 10–15 RPM，无需绑卡', 'global', 0, 1,
     '["大厂","免绑卡","多模态","长上下文"]',
     '注意：免费档的请求内容可能被用于改进模型，勿传敏感数据',
     'gemini', 'builtin', 201),

    ('gl-groq', 'Groq', 'https://groq.com', 'https://console.groq.com/keys',
     1, 'https://api.groq.com/openai/v1', 'openai',
     '永久免费层，约 30 RPM，各模型另有每日请求上限，无需绑卡', 'global', 0, 1,
     '["极低延迟","免绑卡","OpenAI 兼容","首推"]',
     '自研 LPU 硬件，推理速度极快，适合短请求与实时对话',
     'groq', 'builtin', 202),

    ('gl-cerebras', 'Cerebras Cloud', 'https://cloud.cerebras.ai', 'https://cloud.cerebras.ai',
     1, 'https://api.cerebras.ai/v1', 'openai',
     '免费层约 100 万 tokens/日，无需绑卡', 'global', 0, 1,
     '["高吞吐","免绑卡","OpenAI 兼容"]',
     '每日额度慷慨，但免费档上下文上限较小（约 8K），不适合长文档',
     'cerebras', 'builtin', 203),

    ('gl-openrouter', 'OpenRouter', 'https://openrouter.ai', 'https://openrouter.ai/keys',
     1, 'https://openrouter.ai/api/v1', 'openai',
     '带 :free 后缀的模型免费，约 20 RPM / 50 次/日', 'global', 0, 0,
     '["聚合平台","一站式","免绑卡"]',
     '一个 Key 试用数十款模型；充值 $10 可将每日上限提升至 1000 次',
     'openrouter', 'builtin', 204),

    ('gl-cloudflare', 'Cloudflare Workers AI', 'https://workers.cloudflare.com', 'https://dash.cloudflare.com/profile/api-tokens',
     1, 'https://api.cloudflare.com/client/v4/accounts/{account_id}/ai/v1', 'openai',
     '每日 1 万 Neurons 免费额度，无需绑卡', 'global', 0, 0,
     '["边缘计算","免绑卡","OpenAI 兼容"]',
     '需在 URL 中填入自己的 account_id，模型偏轻量',
     'cloudflare', 'builtin', 205),

    ('gl-nvidia-nim', 'NVIDIA NIM', 'https://build.nvidia.com', 'https://build.nvidia.com/settings/api-keys',
     1, 'https://integrate.api.nvidia.com/v1', 'openai',
     '托管 100+ 开源模型，约 40 RPM，注册赠免费调用额度', 'global', 0, 1,
     '["大厂","开源模型丰富","OpenAI 兼容"]',
     '需手机号验证；包含 DeepSeek、Llama、Qwen 等主流开源模型',
     'nvidia', 'builtin', 206),

    ('gl-github-models', 'GitHub Models', 'https://github.com/marketplace/models', 'https://github.com/settings/tokens',
     1, 'https://models.github.ai/inference', 'openai',
     '免费调用 GPT / Llama / Phi 等模型，额度与 GitHub 账号等级挂钩', 'global', 0, 0,
     '["开发者友好","免绑卡","OpenAI 兼容"]',
     '用 GitHub 个人访问令牌即可调用，无需单独注册服务商；需注意请求下限速',
     NULL, 'builtin', 207),

    ('gl-mistral', 'Mistral La Plateforme', 'https://console.mistral.ai', 'https://console.mistral.ai/api-keys',
     1, 'https://api.mistral.ai/v1', 'openai',
     'Experiment 免费层可调用全部模型，约 1 RPS / 30 RPM', 'global', 0, 1,
     '["欧洲","免绑卡","OpenAI 兼容"]',
     '明确面向评估与原型，含 Codestral 代码模型，需手机号验证',
     'mistral', 'builtin', 208),

    ('gl-cohere', 'Cohere', 'https://cohere.com', 'https://dashboard.cohere.com/api-keys',
     1, 'https://api.cohere.ai/compatibility/v1', 'openai',
     '免费 Trial Key，约 1000 次调用/月', 'global', 0, 1,
     '["RAG 友好","Embedding","Rerank"]',
     '嵌入与重排序能力突出，适合检索增强场景，免费档限非商业用途',
     'cohere', 'builtin', 209),

    ('gl-huggingface', 'Hugging Face Inference', 'https://huggingface.co', 'https://huggingface.co/settings/tokens',
     1, 'https://router.huggingface.co/v1', 'openai',
     'Serverless 推理免费额度，约 300 请求/小时', 'global', 0, 0,
     '["开源模型","社区","模型最全"]',
     '模型覆盖面最广，含大量小众与微调模型，免费档有排队',
     'huggingface', 'builtin', 210),

    ('gl-sambanova', 'SambaNova Cloud', 'https://cloud.sambanova.ai', 'https://cloud.sambanova.ai/apis',
     1, 'https://api.sambanova.ai/v1', 'openai',
     '免费层按模型计，约 20 万 tokens/日/模型，无需绑卡', 'global', 0, 0,
     '["高速推理","免绑卡","OpenAI 兼容"]',
     '主打高速开源模型推理，适合长上下文场景',
     NULL, 'builtin', 211),

    ('gl-together', 'Together AI', 'https://www.together.ai', 'https://api.together.xyz/settings/api-keys',
     1, 'https://api.together.xyz/v1', 'openai',
     '新用户注册赠送试用额度（可能需绑卡领取）', 'global', 1, 0,
     '["开源模型","OpenAI 兼容"]',
     '模型与微调服务齐全，赠送额度用完后按量付费',
     'together', 'builtin', 212),

    ('gl-zai', 'Z.ai（智谱国际版）', 'https://z.ai', 'https://z.ai/manage-apikey/apikey-list',
     1, 'https://api.z.ai/api/paas/v4', 'openai',
     'GLM 国际版提供免费模型与赠送额度', 'global', 0, 1,
     '["国产出海","OpenAI 兼容"]',
     '智谱的海外站，国内访问需自备网络条件',
     NULL, 'builtin', 213),

    ('gl-vercel-gateway', 'Vercel AI Gateway', 'https://vercel.com/ai-gateway', 'https://vercel.com/ai-gateway',
     1, 'https://ai-gateway.vercel.sh/v1', 'openai',
     '每月滚动赠送调用额度，无需绑卡即可开通', 'global', 0, 0,
     '["聚合平台","免绑卡","OpenAI 兼容"]',
     '与前端框架集成度高，适合部署在 Vercel 上的应用',
     NULL, 'builtin', 214),

    -- ===================== 本地部署（无额度上限） =====================
    ('lc-ollama', 'Ollama（本地运行）', 'https://ollama.com', 'https://ollama.com/download',
     1, 'http://localhost:11434/v1', 'openai',
     '完全免费、无调用上限，取决于本机硬件', 'local', 0, 0,
     '["本地部署","无需 Key","离线可用"]',
     '在本地拉取开源模型即可提供 OpenAI 兼容接口，Vortex 已内置该提供商',
     'ollama', 'builtin', 301),

    ('lc-lmstudio', 'LM Studio（本地运行）', 'https://lmstudio.ai', 'https://lmstudio.ai',
     1, 'http://localhost:1234/v1', 'openai',
     '完全免费、无调用上限，取决于本机硬件', 'local', 0, 0,
     '["本地部署","图形界面","离线可用"]',
     '自带模型下载与管理界面，一键开启本地 API 服务',
     NULL, 'builtin', 302);
