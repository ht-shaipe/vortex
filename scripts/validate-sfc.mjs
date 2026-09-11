import { readFileSync } from 'node:fs'
import { parse, compileScript, compileTemplate } from '@vue/compiler-sfc'

const file = process.argv[2]
if (!file) {
  console.error('usage: node scripts/validate-sfc.mjs <path>')
  process.exit(1)
}

const source = readFileSync(file, 'utf-8')
const { descriptor, errors } = parse(source, { filename: file })
if (errors && errors.length) {
  console.error('parse errors:', errors)
  process.exit(1)
}

if (descriptor.script || descriptor.scriptSetup) {
  const result = compileScript(descriptor, { id: 'xxx', genDefaultAs: 'default' })
  if (result.errors && result.errors.length) {
    console.error('script errors:', result.errors)
    process.exit(1)
  }
}

if (descriptor.template) {
  const result = compileTemplate({ source: descriptor.template.content, filename: file, id: 'xxx' })
  if (result.errors && result.errors.length) {
    console.error('template errors:', result.errors)
    process.exit(1)
  }
}

console.log('ok')
