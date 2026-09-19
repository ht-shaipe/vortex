-- 路由配置文件表：命名回退链（如 "coding", "vision" 等）
CREATE TABLE IF NOT EXISTS routing_profiles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT,
    -- 目标列表 JSON: [{provider, model, connectionId}]
    targets TEXT NOT NULL DEFAULT '[]',
    is_active INTEGER DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
