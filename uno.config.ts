import { defineConfig, presetUno, presetAttributify } from 'unocss'

export default defineConfig({
  presets: [presetUno(), presetAttributify()],
  theme: {
    colors: {
      // 语义色（映射 CSS 变量）
      bg: 'var(--bg)',
      surface: 'var(--surface)',
      'surface-2': 'var(--surface-2)',
      'surface-3': 'var(--surface-3)',
      line: 'var(--line)',
      'line-2': 'var(--line-2)',
      ink: 'var(--ink)',
      'ink-2': 'var(--ink-2)',
      'ink-3': 'var(--ink-3)',
      'ink-4': 'var(--ink-4)',
      'ink-5': 'var(--ink-5)',
      accent: 'var(--accent)',
      'accent-ink': 'var(--accent-ink)',
      'accent-bg': 'var(--accent-bg)',
      'accent-line': 'var(--accent-line)',
      ok: 'var(--ok)',
      'ok-bg': 'var(--ok-bg)',
      warn: 'var(--warn)',
      'warn-bg': 'var(--warn-bg)',
      err: 'var(--err)',
      'err-bg': 'var(--err-bg)',
      // 图表色阶
      seq1: 'var(--seq-1)',
      seq2: 'var(--seq-2)',
      seq3: 'var(--seq-3)',
      seq4: 'var(--seq-4)',
    },
    borderRadius: {
      sm: 'var(--r-sm)',
      md: 'var(--r-md)',
      lg: 'var(--r-lg)',
    },
    fontSize: {
      body: 'var(--fs-body)',
      sm: 'var(--fs-sm)',
      xs: 'var(--fs-xs)',
      h1: 'var(--fs-h1)',
      h2: 'var(--fs-h2)',
      mono: 'var(--fs-mono)',
    },
  },
  shortcuts: {
    // 常用组合
    'flex-center': 'flex items-center justify-center',
    'flex-between': 'flex items-center justify-between',
    'flex-col-gap-sm': 'flex flex-col gap-8px',
    'card-base': 'bg-surface border border-line rounded-md',
    'text-mono': 'font-mono text-sm',
    'btn-base': 'inline-flex items-center gap-7px rounded-sm border border-line bg-surface text-ink-2 cursor-pointer transition',
    'btn-primary': 'btn-base bg-accent text-white border-accent',
    'btn-sm': 'btn-base text-xs px-8px py-4px',
    'pill-base': 'inline-flex items-center gap-4px rounded-full px-7px py-2px text-xs',
  },
})
