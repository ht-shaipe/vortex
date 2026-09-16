-- ============================================================
-- 008 · 用量估算标记
-- 为 usage_history 新增 usage_estimated 列，标记该条记录的 token 数
-- 是上游真实返回（0）还是在上游未返回用量时按内容估算的（1）。
-- ============================================================

ALTER TABLE usage_history ADD COLUMN usage_estimated INTEGER NOT NULL DEFAULT 0;
