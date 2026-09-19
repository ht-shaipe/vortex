//! 备份与恢复模块
//!
//! 管理智能体配置的备份和恢复操作。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::agent_integrations::AgentKind;

/// 备份清单
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    /// 备份 ID
    pub id: String,
    /// 智能体类型
    pub agent: AgentKind,
    /// 创建时间
    pub created_at: String,
    /// 备份的文件列表
    pub files: Vec<BackupFile>,
    /// 备注
    pub notes: Option<String>,
}

/// 备份文件记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupFile {
    /// 原始路径
    pub original_path: PathBuf,
    /// 备份路径
    pub backup_path: PathBuf,
    /// 文件是否存在
    pub existed: bool,
    /// 文件内容哈希（用于验证）
    pub hash: Option<String>,
}

/// 备份管理器
pub struct BackupManager {
    backup_dir: PathBuf,
}

impl BackupManager {
    /// 创建新的备份管理器
    ///
    /// 接受任意可转为路径的类型（`PathBuf`/`&Path` 等），调用方无需关心所有权。
    pub fn new<P: AsRef<Path>>(backup_dir: P) -> Self {
        Self {
            backup_dir: backup_dir.as_ref().to_path_buf(),
        }
    }

    /// 获取备份目录
    ///
    /// 预留：备份管理 UI 展示备份位置用。
    #[allow(dead_code)]
    pub fn backup_dir(&self) -> &Path {
        &self.backup_dir
    }

    /// 确保备份目录存在
    pub fn ensure_backup_dir(&self) -> Result<(), String> {
        fs::create_dir_all(&self.backup_dir)
            .map_err(|e| format!("创建备份目录失败: {}", e))
    }

    /// 生成备份 ID
    pub fn generate_backup_id(agent: AgentKind) -> String {
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let agent_str = match agent {
            AgentKind::Dsh => "dsh",
            AgentKind::ClaudeCode => "claude",
            AgentKind::OpenCode => "opencode",
            AgentKind::Codex => "codex",
            AgentKind::QwenCode => "qwen",
            AgentKind::GeminiCli => "gemini",
            AgentKind::CursorAgent => "cursor",
            AgentKind::Windsurf => "windsurf",
            AgentKind::Copilot => "copilot",
            AgentKind::Aider => "aider",
            AgentKind::Zed => "zed",
            AgentKind::Amp => "amp",
        };
        format!("{}_{}", agent_str, timestamp)
    }

    /// 创建备份
    pub fn create_backup(
        &self,
        agent: AgentKind,
        files: &[(PathBuf, Option<PathBuf>)], // (原始路径, 备份路径)
    ) -> Result<BackupManifest, String> {
        self.ensure_backup_dir()?;

        let id = Self::generate_backup_id(agent);
        let created_at = chrono::Utc::now().to_rfc3339();
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");

        let mut backup_files = Vec::new();

        for (original_path, backup_path) in files {
            let existed = original_path.exists();
            let hash = if existed {
                super::safe_write::hash_file(original_path).ok()
            } else {
                None
            };

            // 如果调用方未指定备份路径，自动生成与 atomic_write_with_backup 一致的路径
            let resolved_backup = match backup_path {
                Some(p) => p.clone(),
                None => {
                    let file_name = original_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("file");
                    self.backup_dir.join(format!("{}_{}.bak", file_name, timestamp))
                }
            };

            backup_files.push(BackupFile {
                original_path: original_path.clone(),
                backup_path: resolved_backup,
                existed,
                hash,
            });
        }

        let manifest = BackupManifest {
            id,
            agent,
            created_at,
            files: backup_files,
            notes: None,
        };

        // 保存清单
        let manifest_path = self.get_manifest_path(&manifest.id);
        let manifest_json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| format!("序列化清单失败: {}", e))?;

        fs::write(&manifest_path, manifest_json)
            .map_err(|e| format!("保存清单失败: {}", e))?;

        // 设置权限
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&manifest_path, fs::Permissions::from_mode(0o600));
        }

        Ok(manifest)
    }

    /// 获取清单路径
    fn get_manifest_path(&self, backup_id: &str) -> PathBuf {
        self.backup_dir.join(format!("{}.json", backup_id))
    }

    /// 加载备份清单
    pub fn load_manifest(&self, backup_id: &str) -> Result<BackupManifest, String> {
        let manifest_path = self.get_manifest_path(backup_id);

        let content = fs::read_to_string(&manifest_path)
            .map_err(|e| format!("读取清单失败: {}", e))?;

        serde_json::from_str(&content)
            .map_err(|e| format!("解析清单失败: {}", e))
    }

    /// 列出所有备份
    pub fn list_backups(&self) -> Result<Vec<BackupManifest>, String> {
        self.ensure_backup_dir()?;

        let mut manifests = Vec::new();

        let entries = fs::read_dir(&self.backup_dir)
            .map_err(|e| format!("读取备份目录失败: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("读取目录项失败: {}", e))?;
            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) == Some("json")
                && let Ok(manifest) = self.load_manifest(
                    path.file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or(""),
                ) {
                    manifests.push(manifest);
                }
        }

        // 按创建时间倒序排列
        manifests.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(manifests)
    }

    /// 删除备份
    ///
    /// 预留：备份管理 UI 删除操作用；cleanup_old_backups 内部也依赖。
    #[allow(dead_code)]
    pub fn delete_backup(&self, backup_id: &str) -> Result<(), String> {
        let manifest = self.load_manifest(backup_id)?;

        // 删除备份文件
        for file in &manifest.files {
            if file.existed && file.backup_path.exists() {
                let _ = fs::remove_file(&file.backup_path);
            }
        }

        // 删除清单
        let manifest_path = self.get_manifest_path(backup_id);
        fs::remove_file(&manifest_path)
            .map_err(|e| format!("删除清单失败: {}", e))?;

        Ok(())
    }

    /// 验证备份完整性
    pub fn verify_backup(&self, backup_id: &str) -> Result<bool, String> {
        let manifest = self.load_manifest(backup_id)?;

        for file in &manifest.files {
            if file.existed {
                // 检查备份文件是否存在
                if !file.backup_path.exists() {
                    return Ok(false);
                }

                // 验证哈希
                if let Some(expected_hash) = &file.hash {
                    let actual_hash = super::safe_write::hash_file(&file.backup_path)?;
                    if &actual_hash != expected_hash {
                        return Ok(false);
                    }
                }
            }
        }

        Ok(true)
    }

    /// 获取智能体的最新备份
    ///
    /// 预留：配置状态页展示最近备份用。
    #[allow(dead_code)]
    pub fn get_latest_backup(&self, agent: AgentKind) -> Result<Option<BackupManifest>, String> {
        let backups = self.list_backups()?;

        Ok(backups
            .into_iter()
            .find(|b| b.agent == agent))
    }

    /// 清理旧备份（保留每个智能体最近的 N 个）
    ///
    /// 预留：备份保留策略（如启动时/定时清理）接入用。
    #[allow(dead_code)]
    pub fn cleanup_old_backups(&self, keep_count: usize) -> Result<usize, String> {
        let backups = self.list_backups()?;

        // 按智能体分组
        let mut by_agent: HashMap<AgentKind, Vec<BackupManifest>> = HashMap::new();
        for backup in backups {
            by_agent
                .entry(backup.agent)
                .or_default()
                .push(backup);
        }

        let mut deleted_count = 0;

        for (_agent, agent_backups) in by_agent {
            // 已经按时间倒序排列，跳过要保留的
            for backup in agent_backups.into_iter().skip(keep_count) {
                if self.delete_backup(&backup.id).is_ok() {
                    deleted_count += 1;
                }
            }
        }

        Ok(deleted_count)
    }
}

/// 加密备份内容（用于敏感数据）
///
/// 预留：备份文件静态加密（当前备份存 ~/.vortex，权限 0600）。
#[allow(dead_code)]
pub fn encrypt_backup_content(content: &[u8], key: &[u8]) -> Result<Vec<u8>, String> {
    use vortex_store::db::encryption;
    let plaintext = String::from_utf8_lossy(content).to_string();
    let encrypted = encryption::encrypt(key, &plaintext)
        .map_err(|e| format!("加密失败: {}", e))?;
    Ok(encrypted.into_bytes())
}

/// 解密备份内容
///
/// 预留：与 encrypt_backup_content 配套使用。
#[allow(dead_code)]
pub fn decrypt_backup_content(content: &[u8], key: &[u8]) -> Result<Vec<u8>, String> {
    use vortex_store::db::encryption;
    let encrypted = String::from_utf8_lossy(content).to_string();
    let decrypted = encryption::decrypt(key, &encrypted)
        .map_err(|e| format!("解密失败: {}", e))?;
    Ok(decrypted.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_backup_manager() {
        let temp_dir = tempdir().unwrap();
        let manager = BackupManager::new(temp_dir.path());

        // 应该可以列出空备份
        let backups = manager.list_backups().unwrap();
        assert!(backups.is_empty());
    }

    #[test]
    fn test_generate_backup_id() {
        let id = BackupManager::generate_backup_id(AgentKind::Dsh);
        assert!(id.starts_with("dsh_"));
    }
}
