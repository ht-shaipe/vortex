import { defineConfig, presetUno, presetAttributify } from 'unocss'

export default defineConfig({
  // dev 模式下全量扫描源码，启动即生成完整样式。
  // 否则按需生成依赖 HMR 推送新路由的样式类，hash 切换页面时会出现样式丢失。
  content: {
    filesystem: ['src/**/*.{vue,ts,tsx}'],
  },
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
    boxShadow: {
      sm: 'var(--shadow-sm)',
      md: 'var(--shadow-md)',
      lg: 'var(--shadow-lg)',
    },
    fontFamily: {
      mono: 'var(--font-mono)',
    },
  },
  shortcuts: {
    // ============ 常用组合 ============
    'flex-center': 'flex items-center justify-center',
    'flex-between': 'flex items-center justify-between',
    'flex-col-gap-sm': 'flex flex-col gap-8px',
    'card-base': 'bg-surface border-solid border border-line rounded-md',
    'text-mono': 'font-mono text-sm',
    'btn-base': 'inline-flex items-center gap-7px rounded-sm border-solid border border-line bg-surface text-ink-2 cursor-pointer transition',
    'btn-primary': 'btn-base bg-accent text-white border-accent',
    'btn-sm': 'btn-base text-xs px-8px py-4px',
    'pill-base': 'inline-flex items-center gap-4px rounded-full px-7px py-2px text-xs',

    // ============ 按钮（components.css 语义类同名迁移，含修饰符变体） ============
    'btn': 'inline-flex items-center justify-center gap-6px px-14px py-8px rounded-sm border-solid border border-line-2 bg-surface text-ink text-13px font-medium leading-[1.4] cursor-pointer whitespace-nowrap no-underline transition-colors [&:hover]:bg-surface-2 [&:disabled]:opacity-45 [&:disabled]:cursor-not-allowed [&.primary]:bg-ink [&.primary]:text-surface [&.primary]:border-ink [&.primary:hover]:bg-ink-2 [&.accent]:bg-accent [&.accent]:text-white [&.accent]:border-transparent [&.accent:hover]:opacity-90 [&.danger]:bg-err [&.danger]:text-white [&.danger]:border-transparent [&.danger:hover]:opacity-90 [&.ghost]:bg-transparent [&.ghost]:border-dashed [&.ghost]:text-ink-3 [&.ghost:hover]:text-ink [&.ghost:hover]:border-ink-4 [&.bare]:bg-transparent [&.bare]:border-none [&.bare]:text-ink-3 [&.bare]:px-8px [&.bare]:py-4px [&.bare:hover]:text-ink [&.bare:hover]:bg-surface-3 [&.sm]:px-10px [&.sm]:py-5px [&.sm]:text-12px [&.sm]:rounded-5px [&.icon]:p-6px [&_.btn-icon]:w-14px [&_.btn-icon]:h-14px',

    // ============ 卡片 ============
    'card': 'bg-surface border-solid border border-line rounded-lg shadow-sm',
    'card-head': 'flex items-start justify-between gap-[var(--gap-md)] px-[var(--pad-card)] pt-[var(--pad-card)]',
    'card-title': 'text-14px font-semibold text-ink',
    'card-sub': 'text-12px text-ink-3 mt-3px leading-[1.5]',
    'card-body': 'p-[var(--pad-card)]',

    // ============ 状态徽章 pill ============
    'pill': 'inline-flex items-center gap-6px px-8px py-2px rounded-full text-11px font-medium leading-[1.6] whitespace-nowrap [&_.dot]:w-6px [&_.dot]:h-6px [&_.dot]:rounded-full [&_.dot]:shrink-0 [&.ok]:bg-ok-bg [&.ok]:text-ok [&.ok_.dot]:bg-ok [&.warn]:bg-warn-bg [&.warn]:text-warn [&.warn_.dot]:bg-warn [&.err]:bg-err-bg [&.err]:text-err [&.err_.dot]:bg-err [&.neutral]:bg-surface-3 [&.neutral]:text-ink-3 [&.neutral_.dot]:bg-ink-4 [&.tag]:bg-surface-3 [&.tag]:text-ink-3 [&.tag]:font-mono [&.tag]:text-[10.5px] [&.mono]:font-mono [&.mono]:text-[10.5px]',
    'dot': 'w-6px h-6px rounded-full shrink-0',

    // ============ 表格 ============
    'table': 'w-full border-collapse [&_th]:text-left [&_th]:font-medium [&_th]:text-11px [&_th]:text-ink-3 [&_th]:uppercase [&_th]:tracking-[0.03em] [&_th]:py-8px [&_th]:px-[var(--pad-row)] [&_th]:border-solid [&_th]:border-0 [&_th]:border-b [&_th]:border-line [&_th]:whitespace-nowrap [&_td]:py-[var(--pad-row)] [&_td]:px-[var(--pad-row)] [&_td]:border-solid [&_td]:border-0 [&_td]:border-b [&_td]:border-line [&_td]:text-13px [&_td]:text-ink-2 [&_td]:align-middle [&_tbody_tr:last-child_td]:border-b-0 [&_tbody_tr:hover_td]:bg-surface-2 [&_tbody_tr]:cursor-pointer',
    'num': 'font-mono tabular-nums',

    // ============ 开关 toggle ============
    'toggle': 'relative w-38px h-22px rounded-full bg-surface-3 border-solid border border-line-2 cursor-pointer shrink-0 p-0 transition-colors after:content-[""] after:absolute after:top-2px after:left-2px after:w-16px after:h-16px after:rounded-full after:bg-surface after:transition-transform [&.on]:bg-accent [&.on]:border-accent [&.on]:after:translate-x-16px',

    // ============ tabs ============
    'tabs': 'flex gap-2px border-solid border-0 border-b border-line mb-[var(--gap-lg)]',
    'tab': 'inline-flex items-center gap-7px px-16px py-8px text-13px text-ink-3 border-solid border-0 border-b-2 border-b-transparent bg-transparent cursor-pointer transition-colors hover:text-ink [&.active]:text-accent-ink [&.active]:font-semibold [&.active]:border-b-accent [&_.tab-icon]:w-14px [&_.tab-icon]:h-14px',

    // ============ 页面骨架 ============
    'app': 'grid grid-cols-[168px_1fr] h-screen w-full max-w-full overflow-hidden bg-bg',
    'sidebar': 'sticky top-0 flex flex-col h-screen overflow-hidden border-solid border-0 border-r border-line bg-surface-2 pt-[calc(38px_+_var(--titlebar-inset))] px-10px pb-12px transition-[width] duration-200 ease-in-out [&.collapsed]:px-8px',
    'brand': 'flex items-center gap-9px px-4px pt-6px pb-16px border-solid border-0 border-b border-line mb-8px shrink-0',
    'brand-mark': 'w-32px h-32px rounded-8px shrink-0 overflow-hidden grid place-items-center bg-ink text-accent font-extrabold text-15px [&_img]:w-full [&_img]:h-full [&_img]:object-cover [&_img]:block',
    'brand-logo': 'w-32px h-32px rounded-8px shrink-0 object-cover block',
    'brand-text': 'leading-[1.1]',
    'brand-name': 'font-bold text-15px tracking-[-0.01em]',
    'brand-tag': 'text-11px text-ink-3 mt-3px leading-[1.4]',
    'nav-main-scroll': 'flex-1 overflow-hidden',
    'nav-main': 'flex flex-col gap-4px py-2px',
    'nav-bottom': 'flex flex-col gap-4px border-solid border-0 border-t border-line pt-8px mt-4px shrink-0',
    'nav-item': 'relative flex items-center gap-9px px-9px py-10px rounded-sm text-ink-2 font-medium text-[13.5px] cursor-pointer border-none bg-transparent text-left w-full no-underline transition-colors hover:bg-surface-3 hover:text-ink [&.active]:bg-accent-bg [&.active]:text-accent-ink [&.active_.nav-icon]:text-accent [&_.nav-icon]:w-16px [&_.nav-icon]:h-16px [&_.nav-icon]:shrink-0 [&_.nav-icon]:inline-flex [&_.nav-icon]:items-center [&_.nav-icon]:justify-center [&_.nav-icon]:text-ink-3 [&_.nav-icon_svg]:w-16px [&_.nav-icon_svg]:h-16px [&_.badge]:ml-auto [&_.badge]:text-[10.5px] [&_.badge]:font-mono [&_.badge]:bg-surface-3 [&_.badge]:text-ink-3 [&_.badge]:px-6px [&_.badge]:py-1px [&_.badge]:rounded-4px [&_.badge]:shrink-0 [&.active_.badge]:bg-accent-line [&.active_.badge]:text-accent-ink dark:[&.active]:text-accent dark:[&.active_.badge]:text-accent',
    'badge': 'ml-auto text-[10.5px] font-mono bg-surface-3 text-ink-3 px-6px py-1px rounded-4px shrink-0',
    'nav-dot': 'w-8px h-8px rounded-full ml-auto bg-err shadow-[0_0_0_2px_var(--err-bg)] shrink-0 [&.ok]:bg-ok [&.ok]:w-6px [&.ok]:h-6px [&.ok]:shadow-[0_0_0_2px_var(--ok-bg)]',
    'collapse-btn': 'flex items-center justify-center py-8px w-full border-none bg-transparent text-ink-3 rounded-sm cursor-pointer transition-colors hover:bg-surface-3 hover:text-ink',
    'main': 'min-w-0 h-screen overflow-hidden',
    'main-inner': 'pt-6px px-28px pb-36px',
    'flush': 'pt-44px h-screen flex flex-col overflow-hidden',
    'page-flow': 'flex-1',
    'page-flow-pad': 'px-24px pb-36px',
    'page-col': 'max-w-880px mx-auto',
    'page-bar': 'h-38px px-24px flex items-center gap-12px shrink-0',
    'page-head': 'flex items-center gap-10px font-semibold text-15px',
    'page-header': 'flex items-start justify-between gap-[var(--gap-md)] mb-[var(--gap-lg)]',
    'page-title': 'text-h1 font-bold tracking-[-0.02em] leading-[1.15]',
    'page-sub': 'text-13px text-ink-3 mt-6px leading-[1.55]',
    'page-actions': 'flex items-center gap-[var(--gap-sm)] [&_.btn]:h-32px',

    // ============ 设置行 ============
    'setting-row': 'flex items-center justify-between gap-[var(--gap-lg)] py-[var(--pad-row)] border-solid border-0 border-b border-line last:border-b-0',
    'setting-label': 'text-13px font-medium text-ink',
    'setting-desc': 'text-12px text-ink-3 mt-2px leading-[1.5]',

    // ============ 空态 / 加载 ============
    'nil-state': 'px-24px py-48px text-center text-ink-3 [&_.empty-icon]:w-32px [&_.empty-icon]:h-32px [&_.empty-icon]:mx-auto [&_.empty-icon]:mb-12px [&_.empty-icon]:text-ink-4 [&_.empty-title]:text-14px [&_.empty-title]:font-semibold [&_.empty-title]:text-ink-2 [&_.empty-title]:mb-6px [&_.empty-desc]:text-[12.5px] [&_.empty-desc]:leading-[1.6] [&_.empty-desc]:mb-16px',
    'spin': 'animate-spin',

    // ============ 代码块 ============
    'codeblock': 'bg-surface-3 border-solid border border-line rounded-sm px-12px py-10px font-mono text-12px leading-[1.7] text-ink-2 overflow-x-auto relative m-0 whitespace-pre',
    'copyline': 'flex items-center gap-8px bg-surface-3 border-solid border border-line rounded-sm pl-12px pr-6px py-6px font-mono text-12px text-ink-2',

    // ============ 统计 KPI ============
    'stat-label': 'text-11px text-ink-3 uppercase tracking-[0.04em]',
    'stat-val': 'font-mono text-24px font-semibold leading-[1.2] text-ink',
    'stat-delta': 'text-11px mt-2px [&.up]:text-ok [&.down]:text-err',

    // ============ 表单辅助 ============
    'field-label': 'text-12px font-medium text-ink-2',
    'field-hint': 'text-11px text-ink-4 mt-3px leading-[1.5]',
    'radio-group': 'inline-flex bg-surface-3 rounded-sm p-2px gap-2px',
    'radio-option': 'px-10px py-4px text-12px font-medium text-ink-3 rounded-4px cursor-pointer border-none bg-transparent transition-colors hover:text-ink-2 [&.active]:bg-surface [&.active]:text-ink',
    'range-tabs': 'inline-flex gap-2px',
    'range-tab': 'px-12px py-5px text-12px font-medium text-ink-3 border-solid border border-line bg-surface cursor-pointer transition-colors first:rounded-l-sm last:rounded-r-sm [&:not(:first-child)]:border-l-0 hover:text-ink [&.active]:bg-accent [&.active]:text-white [&.active]:border-accent',
  },
})
