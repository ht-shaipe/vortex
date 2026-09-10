//! 程序入口模块。
//!
//! 本文件是 Vortex AI Gateway 可执行程序的入口点，仅负责调用核心库
//! [`vortex_lib::run`] 启动整个应用（加载配置 → 初始化状态 → 启动 API 服务器 → 启动 Tauri 桌面应用）。

fn main() {
    // 委托给核心库的 run() 函数，启动应用主流程
    vortex_lib::run();
}
