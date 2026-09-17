//! 智能体集成测试
//!
//! 测试智能体检测、配置合并、备份恢复等功能。

use tempfile::TempDir;

use crate::agent_integrations::{
    AdapterRegistry, AgentKind, VortexGatewayConfig, ModelInfo,
};

/// 创建测试用的网关配置
fn create_test_config() -> VortexGatewayConfig {
    VortexGatewayConfig::new(
        10168,
        "test-token-12345".to_string(),
        vec![
            ModelInfo {
                id: "gpt-4".to_string(),
                name: "GPT-4".to_string(),
                provider: "openai".to_string(),
                context_window: Some(128000),
            },
            ModelInfo {
                id: "claude-3-opus".to_string(),
                name: "Claude 3 Opus".to_string(),
                provider: "anthropic".to_string(),
                context_window: Some(200000),
            },
        ],
    )
}

#[test]
fn test_adapter_registry_creation() {
    let registry = AdapterRegistry::new();
    // 应该能够获取所有已注册的适配器
    for kind in AgentKind::ALL {
        assert!(registry.get(*kind).is_some(), "适配器 {:?} 应该已注册", kind);
    }
}

#[test]
fn test_agent_kind_display_names() {
    assert_eq!(AgentKind::Dsh.display_name(), "DeepSeek Harness");
    assert_eq!(AgentKind::ClaudeCode.display_name(), "Claude Code");
    assert_eq!(AgentKind::OpenCode.display_name(), "OpenCode");
    assert_eq!(AgentKind::Codex.display_name(), "Codex");
    assert_eq!(AgentKind::QwenCode.display_name(), "Qwen Code");
}

#[test]
fn test_agent_kind_supports_auto_config() {
    // 支持自动配置的智能体
    assert!(AgentKind::Dsh.supports_auto_config());
    assert!(AgentKind::ClaudeCode.supports_auto_config());
    assert!(AgentKind::OpenCode.supports_auto_config());
    assert!(AgentKind::Codex.supports_auto_config());
    assert!(AgentKind::QwenCode.supports_auto_config());

    // 不支持自动配置的智能体
    assert!(!AgentKind::GeminiCli.supports_auto_config());
    assert!(!AgentKind::CursorAgent.supports_auto_config());
}

#[test]
fn test_vortex_gateway_config() {
    let config = create_test_config();
    assert_eq!(config.port, 10168);
    assert_eq!(config.token, "test-token-12345");
    assert_eq!(config.models.len(), 2);
    assert_eq!(config.openai_base_url, "http://127.0.0.1:10168/v1");
    assert_eq!(config.anthropic_base_url, "http://127.0.0.1:10168");
}

#[test]
fn test_detect_with_empty_home() {
    let temp_dir = TempDir::new().unwrap();
    let home = temp_dir.path().to_path_buf();
    
    let registry = AdapterRegistry::new();
    let agents = registry.detect_all(&home);
    
    // 应该返回所有已知的智能体类型
    assert_eq!(agents.len(), AgentKind::ALL.len());
    
    // 验证每个智能体都有正确的类型
    for (i, agent) in agents.iter().enumerate() {
        assert_eq!(agent.kind, AgentKind::ALL[i]);
    }
}

#[test]
fn test_backup_manager() {
    use crate::agent_integrations::backup::BackupManager;
    
    let temp_dir = TempDir::new().unwrap();
    let backup_dir = temp_dir.path().join("backups");
    
    let manager = BackupManager::new(backup_dir.clone());
    
    // 初始时应该没有备份
    let backups = manager.list_backups().unwrap();
    assert!(backups.is_empty());
    
    // 创建备份目录
    manager.ensure_backup_dir().unwrap();
    assert!(backup_dir.exists());
}

#[test]
fn test_safe_write_atomic() {
    use crate::agent_integrations::safe_write;
    
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    
    // 写入文件
    safe_write::atomic_write(&file_path, b"test content").unwrap();
    
    // 验证内容
    let content = std::fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "test content");
    
    // 再次写入
    safe_write::atomic_write(&file_path, b"new content").unwrap();
    let content = std::fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "new content");
}

#[test]
fn test_safe_write_with_backup() {
    use crate::agent_integrations::safe_write;
    
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    let backup_dir = temp_dir.path().join("backups");
    
    // 创建初始文件
    std::fs::write(&file_path, "original content").unwrap();
    
    // 带备份写入
    let backup_path = safe_write::atomic_write_with_backup(
        &file_path,
        b"new content",
        &backup_dir,
    ).unwrap();
    
    // 验证新内容
    let content = std::fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "new content");
    
    // 验证备份存在
    assert!(backup_path.exists());
    let backup_content = std::fs::read_to_string(&backup_path).unwrap();
    assert_eq!(backup_content, "original content");
}

#[test]
fn test_hash_content() {
    use crate::agent_integrations::safe_write;
    
    let hash1 = safe_write::hash_content(b"test content");
    let hash2 = safe_write::hash_content(b"test content");
    let hash3 = safe_write::hash_content(b"different content");
    
    // 相同内容应该有相同的哈希
    assert_eq!(hash1, hash2);
    
    // 不同内容应该有不同的哈希
    assert_ne!(hash1, hash3);
    
    // 哈希应该是 64 个字符（SHA256）
    assert_eq!(hash1.len(), 64);
}
