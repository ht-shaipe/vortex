//! 安全文件写入模块
//!
//! 提供带备份、原子替换和锁机制的安全文件写入功能。

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use fs2::FileExt;
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;

/// 文件锁守卫
///
/// 预留：多进程并发写同一配置文件时的互斥保护，当前原子写入已覆盖主流程。
#[allow(dead_code)]
pub struct FileLock {
    file: File,
    path: PathBuf,
}

impl FileLock {
    /// 获取文件锁
    #[allow(dead_code)]
    pub fn acquire(path: &Path) -> Result<Self, String> {
        let lock_path = path.with_extension("lock");

        // 确保父目录存在
        if let Some(parent) = lock_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("创建锁目录失败: {}", e))?;
        }

        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false) // 锁文件只作锁载体，不截断已有内容
            .open(&lock_path)
            .map_err(|e| format!("创建锁文件失败: {}", e))?;

        // 尝试获取排他锁
        file.try_lock_exclusive()
            .map_err(|e| format!("获取文件锁失败: {}", e))?;

        Ok(Self {
            file,
            path: lock_path,
        })
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
        let _ = fs::remove_file(&self.path);
    }
}

/// 计算文件内容的 SHA256 哈希
pub fn hash_content(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    format!("{:x}", hasher.finalize())
}

/// 计算文件的 SHA256 哈希
pub fn hash_file(path: &Path) -> Result<String, String> {
    let content = fs::read(path).map_err(|e| format!("读取文件失败: {}", e))?;
    Ok(hash_content(&content))
}

/// 原子写入文件
///
/// 先写入临时文件，然后原子替换目标文件
pub fn atomic_write(path: &Path, content: &[u8]) -> Result<(), String> {
    // 确保父目录存在
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("创建目录失败: {}", e))?;
    }

    // 创建临时文件
    let temp_dir = path.parent().unwrap_or(Path::new("."));
    let mut temp_file = NamedTempFile::new_in(temp_dir)
        .map_err(|e| format!("创建临时文件失败: {}", e))?;

    // 写入内容
    temp_file
        .write_all(content)
        .map_err(|e| format!("写入临时文件失败: {}", e))?;

    // 刷新
    temp_file
        .as_file()
        .sync_all()
        .map_err(|e| format!("同步文件失败: {}", e))?;

    // 设置权限（Unix）
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o644);
        fs::set_permissions(temp_file.path(), perms)
            .map_err(|e| format!("设置权限失败: {}", e))?;
    }

    // 原子替换
    temp_file
        .persist(path)
        .map_err(|e| format!("替换文件失败: {}", e))?;

    Ok(())
}

/// 带备份的原子写入
///
/// 写入前备份原文件，返回备份路径
pub fn atomic_write_with_backup(path: &Path, content: &[u8], backup_dir: &Path) -> Result<PathBuf, String> {
    // 创建备份目录
    fs::create_dir_all(backup_dir)
        .map_err(|e| format!("创建备份目录失败: {}", e))?;

    // 生成备份文件名
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    let backup_name = format!("{}_{}.bak", file_name, timestamp);
    let backup_path = backup_dir.join(&backup_name);

    // 如果原文件存在，创建备份
    if path.exists() {
        fs::copy(path, &backup_path)
            .map_err(|e| format!("创建备份失败: {}", e))?;

        // 设置备份权限
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o600);
            let _ = fs::set_permissions(&backup_path, perms);
        }
    }

    // 原子写入
    atomic_write(path, content)?;

    Ok(backup_path)
}

/// 文件写入事务
///
/// 支持多文件原子写入：要么全部成功，要么全部回滚
pub struct WriteTransaction {
    operations: Vec<WriteOp>,
    backups: Vec<BackupEntry>,
    backup_dir: PathBuf,
}

struct WriteOp {
    path: PathBuf,
    content: Vec<u8>,
}

struct BackupEntry {
    original_path: PathBuf,
    backup_path: PathBuf,
    existed_before: bool,
}

impl WriteTransaction {
    /// 创建新事务
    ///
    /// 接受任意可转为路径的类型（`PathBuf`/`&Path` 等），调用方无需关心所有权。
    pub fn new<P: AsRef<Path>>(backup_dir: P) -> Self {
        Self {
            operations: Vec::new(),
            backups: Vec::new(),
            backup_dir: backup_dir.as_ref().to_path_buf(),
        }
    }

    /// 添加写入操作
    pub fn add_write(&mut self, path: PathBuf, content: Vec<u8>) {
        self.operations.push(WriteOp { path, content });
    }

    /// 执行事务
    pub fn commit(mut self) -> Result<(), String> {
        // 创建备份目录
        fs::create_dir_all(&self.backup_dir)
            .map_err(|e| format!("创建备份目录失败: {}", e))?;

        // 第一步：备份所有文件
        for op in &self.operations {
            let existed = op.path.exists();
            let backup_path = if existed {
                let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S_%f");
                let file_name = op.path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("file");
                let backup_name = format!("{}_{}.bak", file_name, timestamp);
                let backup_path = self.backup_dir.join(&backup_name);

                fs::copy(&op.path, &backup_path)
                    .map_err(|e| format!("备份失败 {}: {}", op.path.display(), e))?;

                backup_path
            } else {
                PathBuf::new()
            };

            self.backups.push(BackupEntry {
                original_path: op.path.clone(),
                backup_path,
                existed_before: existed,
            });
        }

        // 第二步：执行所有写入
        for op in &self.operations {
            if let Err(e) = atomic_write(&op.path, &op.content) {
                // 写入失败，回滚
                self.rollback();
                return Err(format!("写入失败 {}: {}", op.path.display(), e));
            }
        }

        Ok(())
    }

    /// 回滚事务
    fn rollback(&self) {
        for backup in self.backups.iter().rev() {
            if backup.existed_before {
                // 恢复原文件
                let _ = fs::copy(&backup.backup_path, &backup.original_path);
            } else {
                // 删除新创建的文件
                let _ = fs::remove_file(&backup.original_path);
            }
        }
    }
}

/// 检查路径是否为符号链接
///
/// 预留：写入前的路径安全检查辅助函数。
#[allow(dead_code)]
pub fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

/// 检查路径是否在用户目录内（安全检查）
///
/// 预留：写入前的路径安全检查辅助函数。
#[allow(dead_code)]
pub fn is_within_user_dir(path: &Path, home: &Path) -> bool {
    // 规范化路径
    let canonical_path = match fs::canonicalize(path) {
        Ok(p) => p,
        Err(_) => return false,
    };

    let canonical_home = match fs::canonicalize(home) {
        Ok(p) => p,
        Err(_) => return false,
    };

    canonical_path.starts_with(&canonical_home)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_hash_content() {
        let hash = hash_content(b"hello world");
        assert_eq!(hash.len(), 64); // SHA256 输出 64 个十六进制字符
    }

    #[test]
    fn test_atomic_write() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        atomic_write(&file_path, b"test content").unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "test content");
    }
}
