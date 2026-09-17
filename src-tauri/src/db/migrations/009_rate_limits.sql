-- 速率限制配置表：存储每个 provider+model+connection 的 RPM/RPD/TPM/TPD 限额
CREATE TABLE IF NOT EXISTS rate_limit_caps (
    id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    connection_id TEXT,
    -- 每分钟最大请求数（NULL 表示不限）
    rpm INTEGER,
    -- 每天最大请求数（NULL 表示不限）
    rpd INTEGER,
    -- 每分钟最大 Token 数（NULL 表示不限）
    tpm INTEGER,
    -- 每天最大 Token 数（NULL 表示不限）
    tpd INTEGER,
    -- 是否启用
    is_active INTEGER DEFAULT 1,
    -- 来源：auto（自动学习）或 manual（手动配置）
    source TEXT DEFAULT 'manual',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(provider, model, connection_id)
);

-- 速率使用计数表：滑动窗口记录每个 provider+model+connection 的实时用量
CREATE TABLE IF NOT EXISTS rate_limit_usage (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    connection_id TEXT,
    -- 请求 token 数
    tokens INTEGER DEFAULT 0,
    -- 时间戳（RFC3339）
    timestamp TEXT NOT NULL
);

-- 为速率查询创建索引
CREATE INDEX IF NOT EXISTS idx_rate_usage_provider_model ON rate_limit_usage(provider, model);
CREATE INDEX IF NOT EXISTS idx_rate_usage_timestamp ON rate_limit_usage(timestamp);
