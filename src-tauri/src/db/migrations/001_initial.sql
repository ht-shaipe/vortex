CREATE TABLE IF NOT EXISTS provider_connections (
    id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    auth_type TEXT NOT NULL DEFAULT 'apikey',
    name TEXT NOT NULL,
    email TEXT,
    priority INTEGER NOT NULL DEFAULT 0,
    is_active INTEGER NOT NULL DEFAULT 1,
    access_token TEXT,
    refresh_token TEXT,
    expires_at TEXT,
    api_key TEXT,
    id_token TEXT,
    project_id TEXT,
    test_status TEXT DEFAULT 'unknown',
    error_code TEXT,
    last_error TEXT,
    last_error_at TEXT,
    backoff_level INTEGER NOT NULL DEFAULT 0,
    rate_limited_until TEXT,
    health_check_interval INTEGER DEFAULT 300,
    consecutive_use_count INTEGER NOT NULL DEFAULT 0,
    rate_limit_protection INTEGER NOT NULL DEFAULT 0,
    group_name TEXT,
    max_concurrent INTEGER,
    proxy_enabled INTEGER NOT NULL DEFAULT 0,
    quota_window_thresholds_json TEXT DEFAULT '{}',
    rate_limit_overrides_json TEXT DEFAULT '{}',
    display_name TEXT,
    default_model TEXT,
    token_type TEXT,
    scope TEXT,
    last_used_at TEXT,
    last_health_check_at TEXT,
    last_tested TEXT,
    provider_specific_data TEXT DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS api_keys (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    key TEXT NOT NULL UNIQUE,
    machine_id TEXT,
    allowed_models TEXT DEFAULT '[]',
    allowed_connections TEXT DEFAULT '[]',
    allowed_endpoints TEXT DEFAULT '[]',
    no_log INTEGER NOT NULL DEFAULT 0,
    auto_resolve INTEGER NOT NULL DEFAULT 1,
    is_active INTEGER NOT NULL DEFAULT 1,
    is_banned INTEGER NOT NULL DEFAULT 0,
    rate_limits TEXT DEFAULT '[]',
    usage_limits TEXT DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS usage_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider TEXT,
    model TEXT,
    connection_id TEXT,
    api_key_id TEXT,
    api_key_name TEXT,
    tokens_input INTEGER NOT NULL DEFAULT 0,
    tokens_output INTEGER NOT NULL DEFAULT 0,
    tokens_cache_read INTEGER NOT NULL DEFAULT 0,
    tokens_cache_creation INTEGER NOT NULL DEFAULT 0,
    tokens_reasoning INTEGER NOT NULL DEFAULT 0,
    service_tier TEXT DEFAULT 'standard',
    status TEXT NOT NULL DEFAULT 'success',
    success INTEGER NOT NULL DEFAULT 1,
    error_code TEXT,
    latency_ms INTEGER,
    ttft_ms INTEGER,
    cost REAL DEFAULT 0.0,
    timestamp TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS key_value (
    namespace TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY (namespace, key)
);

INSERT OR IGNORE INTO key_value (namespace, key, value) VALUES
    ('settings', 'general', '{"port":10168,"requireApiKey":false,"theme":"dark"}');
