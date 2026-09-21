/**
 * 统计图表共享色板与模型配色映射。
 *
 * 趋势图与环形图共用同一套色板，并通过模型名称的确定性哈希映射颜色，
 * 保证同一模型在不同图表中颜色一致。
 */

/** 图表系列色板 */
export const CHART_PALETTE = ['#4C8DFF', '#4ADE80', '#A78BFA', '#F87171', '#FBBF24', '#38BDF8', '#FB923C', '#E879F9']

/**
 * 根据模型名称确定性映射到色板索引，确保同一模型在不同图表中颜色一致。
 * @param model - 模型名称
 * @returns 色板索引
 */
export function modelColorIndex(model: string): number {
  let hash = 0
  for (let i = 0; i < model.length; i++) {
    hash = ((hash << 5) - hash + model.charCodeAt(i)) | 0
  }
  return Math.abs(hash) % CHART_PALETTE.length
}
