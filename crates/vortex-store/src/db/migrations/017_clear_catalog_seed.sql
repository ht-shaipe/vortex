-- 清空旧的内置种子数据，模型目录改为仅从已配置连接拉取
DELETE FROM model_catalog WHERE source_feed = 'builtin' OR source_feed IS NULL;
