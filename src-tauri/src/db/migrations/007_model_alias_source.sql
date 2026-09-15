-- ============================================================
-- 007 · 模型别名来源标记
-- 为 model_aliases 新增 source 列，区分自动归纳（auto）与手动创建（manual）。
-- 重新自动归纳时仅删除 source='auto' 的记录，保留手动创建的别名。
-- ============================================================

ALTER TABLE model_aliases ADD COLUMN source TEXT NOT NULL DEFAULT 'manual';
