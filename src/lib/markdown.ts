/**
 * Markdown 渲染工具。
 *
 * 使用 markdown-it 解析 Markdown 文本，集成 highlight.js 对代码块做语法高亮。
 * 禁用原始 HTML 输入（html: false），防止 XSS 注入。
 */
import MarkdownIt from 'markdown-it'
import hljs from 'highlight.js/lib/common'

// 创建 markdown-it 实例
const md = new MarkdownIt({
  html: false, // 禁用原始 HTML，防止 XSS
  linkify: true, // 自动将 URL 文本转为链接
  breaks: true, // 换行符转为 <br>（聊天场景需要）
  highlight(str: string, lang: string): string {
    // 有语言标注且 highlight.js 支持该语言：高亮渲染
    if (lang && hljs.getLanguage(lang)) {
      try {
        return (
          '<pre class="hljs"><code>' +
          hljs.highlight(str, { language: lang, ignoreIllegals: true }).value +
          '</code></pre>'
        )
      } catch {
        // 高亮失败则回退到转义输出
      }
    }
    // 无语言或不支持：转义后原样输出
    return '<pre class="hljs"><code>' + md.utils.escapeHtml(str) + '</code></pre>'
  },
})

// 链接渲染：新标签页打开 + 安全属性
const defaultLinkOpen = md.renderer.rules.link_open || function (tokens, idx, options, env, self) {
  return self.renderToken(tokens, idx, options)
}
md.renderer.rules.link_open = function (tokens, idx, options, env, self) {
  const token = tokens[idx]
  const targetIndex = token.attrIndex('target')
  const relIndex = token.attrIndex('rel')
  if (targetIndex < 0) token.attrPush(['target', '_blank'])
  else token.attrs![targetIndex][1] = '_blank'
  if (relIndex < 0) token.attrPush(['rel', 'noopener noreferrer'])
  else token.attrs![relIndex][1] = 'noopener noreferrer'
  return defaultLinkOpen(tokens, idx, options, env, self)
}

/**
 * 将 Markdown 文本渲染为 HTML。
 * @param text - Markdown 原文
 * @returns 渲染后的 HTML 字符串（已转义，安全用于 v-html）
 */
export function renderMarkdown(text: string): string {
  return md.render(text)
}
