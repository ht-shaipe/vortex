import { defineConfig, presetUno, presetAttributify } from 'unocss'

export default defineConfig({
  presets: [presetUno(), presetAttributify()],
  theme: {
    colors: {
      rust: 'oklch(0.63 0.14 42)',
      'rust-ink': 'oklch(0.42 0.12 42)',
      'rust-bg': 'oklch(0.96 0.03 42)',
      ok: 'oklch(0.62 0.13 150)',
      warn: 'oklch(0.70 0.15 75)',
      err: 'oklch(0.58 0.18 28)',
    },
  },
})