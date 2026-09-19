-- ToS 合规审查表：每个提供商的服务条款状态
CREATE TABLE IF NOT EXISTS tos_review (
    id TEXT PRIMARY KEY,
    provider TEXT NOT NULL UNIQUE,
    -- 审查结论：ok / caution / avoid
    verdict TEXT NOT NULL,
    -- 审查说明
    notes TEXT,
    -- 最后审查日期
    review_date TEXT,
    -- 审查来源 URL
    source_url TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 初始数据
INSERT OR IGNORE INTO tos_review (id, provider, verdict, notes, review_date) VALUES
('tos-001', 'openai', 'ok', 'API access permitted for personal/internal use', '2026-05-01'),
('tos-002', 'anthropic', 'ok', 'Permitted; explicitly forbids key transfer', '2026-05-01'),
('tos-003', 'gemini', 'caution', 'March 2026 ToS narrows scope; self-hosted proxy still defensible', '2026-05-01'),
('tos-004', 'groq', 'ok', 'GroqCloud Services Agreement permits Customer Application integration', '2026-05-01'),
('tos-005', 'cerebras', 'ok', 'Permitted; explicitly forbids selling/transferring API keys', '2026-05-01'),
('tos-006', 'deepseek', 'ok', 'APIs allowed for personal/internal business use', '2026-05-01'),
('tos-007', 'mistral', 'ok', 'APIs allowed for personal/internal business use', '2026-05-01'),
('tos-008', 'openrouter', 'ok', 'Private single-user proxy fine; no-resale/no-competing clause', '2026-05-01'),
('tos-009', 'cloudflare', 'caution', 'No anti-proxy clause; covered by general Self-Serve Agreement', '2026-05-01'),
('tos-010', 'nvidia', 'caution', 'Trial ToS: evaluation only, not production', '2026-05-01'),
('tos-011', 'huggingface', 'ok', 'Free inference API permitted for personal use', '2026-05-01'),
('tos-012', 'cohere', 'avoid', 'Terms forbid personal, family or household purposes', '2026-05-01'),
('tos-013', 'zai', 'caution', 'Anti-traffic-redirect clause may apply; no personal-use carve-out', '2026-05-01'),
('tos-014', 'qwen', 'ok', 'Personal/non-commercial research carve-out in platform docs', '2026-05-01'),
('tos-015', 'siliconflow', 'ok', 'API access permitted for personal use', '2026-05-01'),
('tos-016', 'volcengine-code', 'ok', 'API access permitted for personal use', '2026-05-01'),
('tos-017', 'sensenova', 'ok', 'API access permitted for personal use', '2026-05-01'),
('tos-018', 'minimax', 'ok', 'API access permitted for personal use', '2026-05-01'),
('tos-019', 'ollama', 'ok', 'Open source; no restrictions', '2026-05-01'),
('tos-020', 'xai', 'ok', 'API access permitted for personal use', '2026-05-01');
