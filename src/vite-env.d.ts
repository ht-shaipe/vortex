/**
 * Vite 环境类型声明文件。
 * 引用 Vite 客户端类型，并声明 .vue 单文件组件模块类型。
 */
/// <reference types="vite/client" />

/** 声明 .vue 文件模块，使 TypeScript 能正确识别单文件组件导入。 */
declare module '*.vue' {
  import type { DefineComponent } from 'vue'
  const component: DefineComponent<{}, {}, any>
  export default component
}
