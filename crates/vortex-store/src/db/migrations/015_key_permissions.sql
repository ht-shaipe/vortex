-- API Key 级别的模型访问控制权限
CREATE TABLE IF NOT EXISTS key_permissions (
    id TEXT PRIMARY KEY,
    api_key_id TEXT NOT NULL,
    rule_type TEXT NOT NULL DEFAULT 'allow',
    model_pattern TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_key_permissions_key ON key_permissions(api_key_id);
