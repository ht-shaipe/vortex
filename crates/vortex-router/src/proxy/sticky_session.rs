//! 粘性会话管理：让对话粘在同一个模型上 30 分钟。
//!
//! 会话 ID → (provider, model, connection_id, last_used) 映射，
//! 过期后自动清除。中途切换时生成紧凑的 handoff note。

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 粘性会话条目
struct StickyEntry {
    provider: String,
    model: String,
    connection_id: Option<String>,
    last_used: Instant,
}

/// 粘性会话管理器
#[derive(Clone)]
pub struct StickySessionManager {
    /// 按 session_id 索引
    sessions: Arc<RwLock<HashMap<String, StickyEntry>>>,
    /// 粘性时长（默认 30 分钟）
    sticky_duration: Duration,
}

impl StickySessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            sticky_duration: Duration::from_secs(1800),
        }
    }

    /// 获取会话当前绑定的模型（若未过期）
    pub fn get(&self, session_id: &str) -> Option<(String, String, Option<String>)> {
        let sessions = self.sessions.read();
        let entry = sessions.get(session_id)?;
        if entry.last_used.elapsed() >= self.sticky_duration {
            return None;
        }
        Some((entry.provider.clone(), entry.model.clone(), entry.connection_id.clone()))
    }

    /// 绑定会话到指定模型
    pub fn set(&self, session_id: &str, provider: &str, model: &str, connection_id: Option<&str>) {
        let mut sessions = self.sessions.write();
        sessions.insert(
            session_id.to_string(),
            StickyEntry {
                provider: provider.to_string(),
                model: model.to_string(),
                connection_id: connection_id.map(|s| s.to_string()),
                last_used: Instant::now(),
            },
        );
    }

    /// 清除会话绑定
    pub fn clear(&self, session_id: &str) {
        let mut sessions = self.sessions.write();
        sessions.remove(session_id);
    }

    /// 清理所有过期会话
    pub fn gc(&self) {
        let mut sessions = self.sessions.write();
        sessions.retain(|_, entry| entry.last_used.elapsed() < self.sticky_duration);
    }

    /// 生成 handoff note：当模型切换时，生成简短的上下文交接说明
    pub fn generate_handoff_note(&self, old_model: &str, new_model: &str) -> String {
        format!(
            "[Context handoff: previously using {}, now switching to {}. Please maintain continuity.]",
            old_model, new_model
        )
    }
}
