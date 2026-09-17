-- 为模型别名增加排序权重列，支持 model-first 路由策略下自定义模型优先级。
-- sort_order 越大越靠前，默认 0。
ALTER TABLE model_aliases ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;