/**
 * 数值展示格式化。
 */

/**
 * Token 数量的辅助单位文案：就近取量级，约等号 + 两位小数 + 中文单位。
 * - ≥ 1 亿：`≈1.25亿`
 * - ≥ 1 万：`≈900.00万`
 * - 否则：原始数字（千分位），不加单位与约等号
 *
 * 主数值仍应展示精确值，本函数仅产出「辅助小字」文案。
 * 非有限值按 `"0"` 处理；负数取绝对值折算并保留负号。
 *
 * @param n - Token 数量
 * @returns 格式化后的辅助文案
 */
export function formatTokenCompact(n: number): string {
  if (!Number.isFinite(n)) return '0'
  const sign = n < 0 ? '-' : ''
  const abs = Math.abs(n)
  if (abs >= 1e8) return `${sign}≈${(abs / 1e8).toFixed(2)}亿`
  if (abs >= 1e4) return `${sign}≈${(abs / 1e4).toFixed(2)}万`
  return n.toLocaleString()
}

/**
 * Token 数量紧凑展示（千位 k 单位）：
 * - |n| ≥ 1000：取整千 → `1k`、`102k`、`110k`
 * - 否则：原始整数
 * 用于悬停明细等空间紧凑处。负数保留符号。
 *
 * @param n - Token 数量
 * @returns 紧凑展示文案
 */
export function formatTokenK(n: number): string {
  if (!Number.isFinite(n)) return '0'
  const sign = n < 0 ? '-' : ''
  const abs = Math.abs(n)
  if (abs >= 1000) return `${sign}${Math.round(abs / 1000)}k`
  return String(Math.round(n))
}

/**
 * 耗时统一按秒展示（两位小数）：`6458ms → 6.46s`。非有限值按 `0.00s`。
 * @param ms - 毫秒数
 * @returns 秒级展示文案
 */
export function formatDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return '—'
  return ms < 1000 ? `${Math.round(ms)}ms` : `${(ms / 1000).toFixed(1)}s`
}

/**
 * 千分位整数。
 * @param n - 数值，null/undefined 按 0 处理
 * @returns 千分位格式字符串
 */
export function fmtInt(n: number | null | undefined): string {
  return (n ?? 0).toLocaleString()
}

/** 紧凑数字格式化器（图表坐标轴用）：1.2K / 3.4M。 */
const compactFmt = new Intl.NumberFormat('en', { notation: 'compact', maximumFractionDigits: 1 })

/**
 * 紧凑数字格式化（复用 Intl.NumberFormat）。
 * @param n - 数值
 * @returns 紧凑格式字符串（如 1.2K、3.4M）
 */
export function fmtCompact(n: number): string {
  return compactFmt.format(n)
}
