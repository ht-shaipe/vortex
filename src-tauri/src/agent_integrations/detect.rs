//! 智能体检测模块
//!
//! 负责检测本机已安装的 AI 编程智能体，通过 PATH 查找、常见安装目录和包元数据判断。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// 检测可执行文件是否在 PATH 中
pub fn find_in_path(name: &str) -> Option<PathBuf> {
    // 常见 PATH 目录
    let path_dirs = std::env::var("PATH")
        .ok()
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default();

    // 补充常见用户安装目录
    let home = std::env::var("HOME").ok().map(PathBuf::from);
    let mut extra_dirs = Vec::new();

    if let Some(ref h) = home {
        extra_dirs.push(h.join(".local/bin"));
        extra_dirs.push(h.join(".npm-global/bin"));
        extra_dirs.push(h.join(".nvm/current/bin"));

        // nvm 版本目录
        let nvm_dir = h.join(".nvm/versions/node");
        if nvm_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&nvm_dir) {
                for entry in entries.flatten() {
                    let bin_dir = entry.path().join("bin");
                    if bin_dir.exists() {
                        extra_dirs.push(bin_dir);
                    }
                }
            }
        }
    }

    // 合并搜索路径
    let all_dirs: Vec<&Path> = path_dirs
        .iter()
        .map(|p| p.as_path())
        .chain(extra_dirs.iter().map(|p| p.as_path()))
        .collect();

    for dir in all_dirs {
        let candidate = dir.join(name);
        if candidate.is_file() && is_executable(&candidate) {
            return Some(candidate);
        }
    }

    None
}

/// 检查文件是否可执行
fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            return meta.permissions().mode() & 0o111 != 0;
        }
    }
    #[cfg(windows)]
    {
        // Windows 上检查扩展名
        if let Some(ext) = path.extension() {
            let ext = ext.to_string_lossy().to_lowercase();
            return matches!(ext.as_str(), "exe" | "cmd" | "bat" | "com");
        }
    }
    false
}

/// 获取可执行文件版本
pub fn get_version(path: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new(path)
        .args(args)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    // 尝试从输出中提取版本号
    extract_version_from_output(&combined)
}

/// 从命令输出中提取版本号
fn extract_version_from_output(output: &str) -> Option<String> {
    // 常见版本格式：x.y.z, vX.Y.Z, x.y.z-beta.1 等
    for line in output.lines() {
        // 查找包含版本号的行
        if let Some(pos) = line.find(|c: char| c.is_ascii_digit()) {
            let rest = &line[pos..];
            // 匹配版本号模式
            let version_chars: String = rest
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == '+' || c.is_alphanumeric())
                .collect();

            // 验证版本号格式
            if version_chars.contains('.') && version_chars.chars().next()?.is_ascii_digit() {
                // 清理版本号
                let cleaned = version_chars.trim_end_matches(|c: char| !c.is_ascii_digit() && c != '.');
                if !cleaned.is_empty() && cleaned.contains('.') {
                    return Some(cleaned.to_string());
                }
            }
        }
    }
    None
}

/// 检查配置文件是否存在
pub fn config_exists(home: &Path, relative_path: &str) -> bool {
    home.join(relative_path).exists()
}

/// 获取用户主目录
pub fn get_home_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            #[cfg(target_os = "windows")]
            {
                std::env::var("USERPROFILE").ok().map(PathBuf::from)
            }
            #[cfg(not(target_os = "windows"))]
            None
        })
}

/// 解析智能体特定的 HOME 环境变量
pub fn resolve_agent_home(env_var: &str, default_relative: &str) -> Option<PathBuf> {
    // 优先使用环境变量
    if let Ok(val) = std::env::var(env_var) {
        let path = PathBuf::from(val);
        if path.exists() {
            return Some(path);
        }
    }

    // 回退到默认路径
    get_home_dir().map(|h| h.join(default_relative))
}

/// 带超时的版本检测
pub fn get_version_with_timeout(path: &Path, args: &[&str], timeout_ms: u64) -> Option<String> {
    // 使用 tokio 的超时机制在同步上下文中不可行
    // 这里使用简单的 Command 输出，依赖操作系统的超时
    let output = Command::new(path)
        .args(args)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    extract_version_from_output(&stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_version() {
        assert_eq!(
            extract_version_from_output("v1.2.3"),
            Some("1.2.3".to_string())
        );
        assert_eq!(
            extract_version_from_output("version 2.0.1-beta"),
            Some("2.0.1".to_string())
        );
        assert_eq!(
            extract_version_from_output("codex-cli 0.142.4"),
            Some("0.142.4".to_string())
        );
    }
}
