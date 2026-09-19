-- 远程模型目录表：存储从远程 feed 拉取的模型信息
CREATE TABLE IF NOT EXISTS model_catalog (
    id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    -- 显示名称
    name TEXT,
    -- 上下文窗口大小
    context_window INTEGER,
    -- 每分钟最大请求数
    rpm INTEGER,
    -- 每天最大请求数
    rpd INTEGER,
    -- 每分钟最大 token 数
    tpm INTEGER,
    -- 每天最大 token 数
    tpd INTEGER,
    -- 免费 token 额度描述
    free_quota TEXT,
    -- 是否支持工具调用
    supports_tools INTEGER DEFAULT 0,
    -- 是否支持流式
    supports_streaming INTEGER DEFAULT 1,
    -- 是否支持视觉
    supports_vision INTEGER DEFAULT 0,
    -- 模型能力标签（JSON 数组）
    capabilities TEXT DEFAULT '[]',
    -- 来源 feed URL
    source_feed TEXT,
    -- 是否活跃
    is_active INTEGER DEFAULT 1,
    -- 最后同步时间
    last_synced TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(provider, model)
);
