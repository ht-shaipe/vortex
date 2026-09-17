-- 内容寻址回忆：存储被压缩前的原始 prompt 内容
CREATE TABLE IF NOT EXISTS compressed_content (
    hash TEXT PRIMARY KEY,
    original_content TEXT NOT NULL,
    content_size INTEGER NOT NULL DEFAULT 0,
    saved_tokens INTEGER NOT NULL DEFAULT 0,
    provider TEXT,
    model TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_compressed_content_created ON compressed_content(created_at DESC);
