-- 记录最近一次连接测试的响应延迟（毫秒）。
-- 订阅列表第一列直接展示该值，用于一眼判断提供方快慢。
ALTER TABLE provider_connections ADD COLUMN last_latency_ms INTEGER;
