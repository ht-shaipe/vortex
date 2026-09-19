-- 模型别名（虚拟模型映射）表
-- 用户自定义虚拟模型名，映射到一个或多个已接入的真实模型，按数组顺序做故障转移。

CREATE TABLE IF NOT EXISTS model_aliases (
    id          TEXT PRIMARY KEY,                         -- UUID
    alias       TEXT NOT NULL UNIQUE,                     -- 虚拟模型名（对外输出的名称）
    targets     TEXT NOT NULL DEFAULT '[]',               -- JSON 数组：[{ "provider": "...", "model": "...", "connection_id": "..." }, ...]
    is_active   INTEGER NOT NULL DEFAULT 1,               -- 是否启用
    created_at  TEXT NOT NULL,                            -- 创建时间（RFC3339）
    updated_at  TEXT NOT NULL                             -- 更新时间（RFC3339）
);

CREATE INDEX IF NOT EXISTS idx_model_aliases_alias ON model_aliases (alias);
CREATE INDEX IF NOT EXISTS idx_model_aliases_active ON model_aliases (is_active);
