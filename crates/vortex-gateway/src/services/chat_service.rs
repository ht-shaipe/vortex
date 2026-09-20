//! 对话流取消管理服务。
//!
//! 提供跨命令共享的取消标记集合，供 IPC 流式对话和 HTTP 端点复用。

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

/// 已取消的请求 ID 集合（跨命令共享）。
fn cancelled_set() -> &'static Mutex<HashSet<String>> {
    static SET: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    SET.get_or_init(|| Mutex::new(HashSet::new()))
}

/// 标记请求已取消。
pub fn mark_cancelled(request_id: &str) {
    if !request_id.is_empty() {
        cancelled_set().lock().unwrap().insert(request_id.to_string());
    }
}

/// 消费取消标记（读取后清除，保证同 ID 只生效一次）。
pub fn take_cancelled(request_id: &str) -> bool {
    if request_id.is_empty() {
        return false;
    }
    cancelled_set().lock().unwrap().remove(request_id)
}
