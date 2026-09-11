/**
 * 日期范围选择器的纯日期逻辑（月历网格、日期/时间部件合成）。
 * 与组件分离便于复用与测试。
 */

/**
 * 生成 42 格月历（周日起始、固定 6 行，GitHub 日历风格），返回每格当天 0 点毫秒。
 * @param year - 年份
 * @param monthIndex - 月份索引（0-11）
 * @returns 42 个毫秒时间戳数组
 */
export function calendarDays(year: number, monthIndex: number): number[] {
  // 当月 1 号是星期几（0=周日）
  const firstDow = new Date(year, monthIndex, 1).getDay()
  // 从当月 1 号向前偏移到周日，连续生成 42 天
  return Array.from({ length: 42 }, (_, i) => new Date(year, monthIndex, 1 - firstDow + i).getTime())
}

/**
 * 当天 0 点。
 * @param ms - 毫秒时间戳
 * @returns 当天 0 点的毫秒时间戳
 */
export function startOfDayMs(ms: number): number {
  const d = new Date(ms)
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime()
}

/**
 * 换日保时：取 dayMs 的年月日 + ms 的时分。
 * @param ms - 提供时分的时间戳
 * @param dayMs - 提供年月日的时间戳
 * @returns 合成后的毫秒时间戳
 */
export function withDayFrom(ms: number, dayMs: number): number {
  const t = new Date(ms)
  const d = new Date(dayMs)
  return new Date(d.getFullYear(), d.getMonth(), d.getDate(), t.getHours(), t.getMinutes()).getTime()
}

/**
 * 应用 input[type=date] 的值（YYYY-MM-DD），保留时分。无效输入原样返回。
 * @param ms - 原始时间戳
 * @param date - YYYY-MM-DD 格式日期字符串
 * @returns 更新日期部分后的时间戳
 */
export function withDatePart(ms: number, date: string): number {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(date)
  if (!m) return ms
  const t = new Date(ms)
  // 替换年月日，保留原时分
  return new Date(+m[1], +m[2] - 1, +m[3], t.getHours(), t.getMinutes()).getTime()
}

/**
 * 应用 input[type=time] 的值（HH:mm），保留年月日。无效输入原样返回。
 * @param ms - 原始时间戳
 * @param time - HH:mm 格式时间字符串
 * @returns 更新时间部分后的时间戳
 */
export function withTimePart(ms: number, time: string): number {
  const m = /^(\d{1,2}):(\d{2})$/.exec(time)
  if (!m) return ms
  const d = new Date(ms)
  // 替换时分，保留原年月日
  return new Date(d.getFullYear(), d.getMonth(), d.getDate(), +m[1], +m[2]).getTime()
}

/**
 * input[type=time] 的值 HH:mm。
 * @param ms - 毫秒时间戳
 * @returns HH:mm 格式时间字符串
 */
export function fmtTimeInput(ms: number): string {
  const d = new Date(ms)
  const p = (n: number) => String(n).padStart(2, '0')
  return `${p(d.getHours())}:${p(d.getMinutes())}`
}
