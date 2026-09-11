/**
 * vendorLogos.ts — 模型厂商 Logo 解析
 *
 * 职责：汇总加载 `src/images/llm/` 下的厂商 SVG 图标，并根据
 * provider id、连接名称、模型名等线索解析出对应的 Logo URL，
 * 供 ProviderLogo、ModelSelector 等组件展示。
 */

// 汇总加载全部厂商 SVG（以 URL 形式引入）
const files = import.meta.glob('../../images/llm/*.svg', {
  eager: true,
  query: '?url',
  import: 'default',
}) as Record<string, string>

// 文件名（不含扩展名）→ URL 映射，如 { gpt: '/assets/gpt-xxx.svg', ... }
const logoByName: Record<string, string> = {}
for (const [path, url] of Object.entries(files)) {
  const base = path.split('/').pop() ?? ''
  const name = base.replace(/\.svg$/, '')
  logoByName[name] = url
}

/**
 * provider id 精确匹配表（内置注册表的 id → 厂商图标名）。
 * 注意 custom-openai 等自定义类型刻意不在表内，避免误挂 OpenAI 图标。
 */
const EXACT_ID: Record<string, string> = {
  openai: 'gpt',
  anthropic: 'claude',
  gemini: 'gemini',
  deepseek: 'deepseek',
  qwen: 'qwen',
  minimax: 'minimax',
  nvidia: 'nvidia',
  zai: 'zp',
}

/**
 * 关键词包含匹配表（按顺序优先级从高到低）。
 * 用于连接名 / baseUrl / 模型名等自由文本的模糊识别。
 */
const KEYWORDS: { logo: string; keys: string[] }[] = [
  { logo: 'claude', keys: ['claude', 'anthropic'] },
  { logo: 'deepseek', keys: ['deepseek'] },
  { logo: 'qwen', keys: ['qwen', '通义', 'tongyi'] },
  { logo: 'kimi', keys: ['kimi', 'moonshot', '月之暗面'] },
  { logo: 'gpt', keys: ['chatgpt', 'gpt'] },
  { logo: 'gemini', keys: ['gemini'] },
  { logo: 'zp', keys: ['zhipu', '智谱', 'chatglm', 'glm', 'bigmodel'] },
  { logo: 'minimax', keys: ['minimax', 'abab', '海螺'] },
  { logo: 'doubao', keys: ['doubao', '豆包'] },
  { logo: 'hunyuan', keys: ['hunyuan', '混元'] },
  { logo: 'wxyy', keys: ['文心', 'wenxin', 'ernie', '百度', 'baidu'] },
  { logo: 'xinhuo', keys: ['星火', 'xinghuo', 'xfyun', '讯飞', 'spark'] },
  { logo: 'nvidia', keys: ['nvidia', '英伟达'] },
  { logo: 'amd', keys: ['amd', '超威'] },
]

/** 泛化名称（如默认连接名 "Custom OpenAI-Compatible"）不参与关键词匹配，防止误识别 */
const GENERIC_NAME = /custom|兼容|compatible/i

/**
 * 解析厂商 Logo。
 *
 * 匹配策略：
 * 1. provider id 精确命中内置注册表（如 openai → gpt.svg）；
 * 2. 否则对辅助线索（连接名、baseUrl、模型名等）做关键词包含匹配；
 *    以 `custom-` 开头的 provider id 与泛化名称不参与该轮匹配。
 *
 * @param providerId - 提供商类型标识（如 'openai'、'custom-openai'）
 * @param hints - 额外匹配线索（连接名 / baseUrl / 模型名等）
 * @returns 命中返回 SVG URL，未命中返回 null
 */
export function resolveVendorLogo(providerId: string, ...hints: (string | undefined | null)[]): string | null {
  // 第一轮：provider id 精确匹配
  const id = (providerId || '').toLowerCase()
  const exact = EXACT_ID[id]
  if (exact && logoByName[exact]) return logoByName[exact]

  // 第二轮：线索关键词匹配
  const parts: string[] = []
  if (id && !id.startsWith('custom-')) parts.push(id)
  for (const h of hints) {
    if (!h) continue
    // 剥掉线索中内嵌的 provider 前缀（如完整模型 id "custom-openai/deepseek-ai/..."），
    // 避免前缀中的 "custom" 触发泛化过滤导致整条线索被丢弃
    let text = h
    if (id) {
      const esc = id.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
      text = text.replace(new RegExp(`^${esc}/`), '').replace(new RegExp(`^${esc}::`), '')
    }
    if (!GENERIC_NAME.test(text)) parts.push(text)
  }
  const joined = parts.join(' ').toLowerCase()
  if (!joined) return null
  for (const { logo, keys } of KEYWORDS) {
    if (keys.some((k) => joined.includes(k)) && logoByName[logo]) return logoByName[logo]
  }
  return null
}
