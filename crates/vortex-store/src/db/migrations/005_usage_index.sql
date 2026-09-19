-- 为 usage_history.timestamp 添加降序索引，加速按时间排序的查询
CREATE INDEX IF NOT EXISTS idx_usage_history_timestamp ON usage_history (timestamp DESC);