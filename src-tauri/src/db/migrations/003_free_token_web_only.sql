-- ============================================================
-- 003 · 免费 Token 站点目录 · 补充「仅网页版」条目
-- 002 收录的站点均提供 API；本版本补齐只能网页使用、
-- 不提供免费 API 的常见平台，让「是否支持 API」筛选有区分度。
-- ============================================================

INSERT OR IGNORE INTO free_token_sites
    (id, name, home_url, apply_url, api_supported, api_base, api_format, free_quota,
     region, requires_card, requires_verify, tags, note, provider_id, source, sort_order)
VALUES
    -- ===================== 国内 · 仅网页版 =====================
    ('cn-web-doubao', '豆包（网页版）', 'https://www.doubao.com', '',
     0, NULL, NULL,
     '网页端免费使用，无免费 API 额度（API 走火山方舟计费）', 'cn', 0, 1,
     '["网页版","无 API","字节跳动"]',
     '仅网页端可用，无法直接接入 Vortex 网关；需要 API 请查看「火山方舟」条目',
     NULL, 'builtin', 161),

    ('cn-web-yuanbao', '腾讯元宝（网页版）', 'https://yuanbao.tencent.com', '',
     0, NULL, NULL,
     '网页端免费使用，无免费 API 额度', 'cn', 0, 1,
     '["网页版","无 API","腾讯"]',
     '仅网页端可用；需要 API 请查看「腾讯云混元」条目',
     NULL, 'builtin', 162),

    ('cn-web-tongyi', '通义千问（网页版）', 'https://tongyi.aliyun.com', '',
     0, NULL, NULL,
     '网页端免费使用，无免费 API 额度', 'cn', 0, 1,
     '["网页版","无 API","阿里"]',
     '仅网页端可用；需要 API 请查看「阿里云百炼」条目',
     NULL, 'builtin', 163),

    ('cn-web-kimi', 'Kimi（网页版）', 'https://kimi.moonshot.cn', '',
     0, NULL, NULL,
     '网页端免费使用，无免费 API 额度', 'cn', 0, 1,
     '["网页版","无 API","长上下文"]',
     '仅网页端可用；需要 API 请查看「月之暗面 Kimi」条目',
     NULL, 'builtin', 164),

    ('cn-web-yiyan', '文心一言（网页版）', 'https://yiyan.baidu.com', '',
     0, NULL, NULL,
     '网页端免费使用，无免费 API 额度', 'cn', 0, 1,
     '["网页版","无 API","百度"]',
     '仅网页端可用；需要 API 请查看「百度千帆」条目',
     NULL, 'builtin', 165),

    ('cn-web-metaso', '秘塔 AI 搜索', 'https://metaso.cn', '',
     0, NULL, NULL,
     '网页端免费使用（含学术搜索），无公开免费 API', 'cn', 0, 0,
     '["网页版","无 API","搜索增强"]',
     '适合资料检索与溯源，不能作为模型 API 接入',
     NULL, 'builtin', 166),

    ('cn-web-tiangong', '天工 AI', 'https://www.tiangong.cn', '',
     0, NULL, NULL,
     '网页端免费使用，无免费 API 额度', 'cn', 0, 1,
     '["网页版","无 API"]',
     '仅网页端可用',
     NULL, 'builtin', 167),

    -- ===================== 海外 · 仅网页版 =====================
    ('gl-web-chatgpt', 'ChatGPT（网页版）', 'https://chatgpt.com', '',
     0, NULL, NULL,
     '免费账号可使用基础模型，有次数与速率限制', 'global', 0, 0,
     '["网页版","无 API","OpenAI"]',
     'OpenAI 不提供免费 API 额度，API 调用需另行付费',
     NULL, 'builtin', 261),

    ('gl-web-claude', 'Claude（网页版）', 'https://claude.ai', '',
     0, NULL, NULL,
     '免费账号每日有使用次数限制', 'global', 0, 0,
     '["网页版","无 API","Anthropic"]',
     'Anthropic 未提供免费 API 层，API 需付费开通',
     NULL, 'builtin', 262),

    ('gl-web-gemini', 'Google Gemini（网页版）', 'https://gemini.google.com', '',
     0, NULL, NULL,
     '网页端免费使用，限额随账号与地区变化', 'global', 0, 0,
     '["网页版","无 API","Google"]',
     '仅网页端可用；需要 API 请查看「Google AI Studio」条目',
     NULL, 'builtin', 263),

    ('gl-web-copilot', 'Microsoft Copilot', 'https://copilot.microsoft.com', '',
     0, NULL, NULL,
     '网页端免费使用，含联网检索与绘图', 'global', 0, 0,
     '["网页版","无 API","微软"]',
     '仅网页端可用，无面向个人的免费 API',
     NULL, 'builtin', 264),

    ('gl-web-perplexity', 'Perplexity', 'https://www.perplexity.ai', '',
     0, NULL, NULL,
     '网页端每日有限次数的免费检索问答', 'global', 0, 0,
     '["网页版","无 API","联网检索"]',
     'Pro 搜索 API 需付费，免费层仅限网页使用',
     NULL, 'builtin', 265),

    ('gl-web-poe', 'Poe（聚合）', 'https://poe.com', '',
     0, NULL, NULL,
     '每日赠送少量积分，可试用多家模型', 'global', 0, 0,
     '["网页版","聚合","无 API"]',
     '一个账号试用多家模型，但积分不可用于 API 调用',
     NULL, 'builtin', 266);
