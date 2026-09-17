-- Token 节省统计：记录每次请求通过 prompt 压缩节省的 token 数
ALTER TABLE usage_history ADD COLUMN saved_tokens INTEGER NOT NULL DEFAULT 0;
