/**
 * 侧边栏折叠状态管理。
 * 通过模块级共享 ref 让 WindowChrome（按钮）与 Sidebar（样式）同步状态。
 */
import { ref } from 'vue'

const COLLAPSE_KEY = 'vortex-sidebar-collapsed'
const collapsed = ref(localStorage.getItem(COLLAPSE_KEY) === '1')

/** 切换侧边栏折叠/展开状态，并持久化到 localStorage。 */
function toggleCollapsed() {
  collapsed.value = !collapsed.value
  localStorage.setItem(COLLAPSE_KEY, collapsed.value ? '1' : '0')
}

export function useSidebarCollapsed() {
  return { collapsed, toggleCollapsed }
}
