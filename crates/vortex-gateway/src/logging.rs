//! 运行日志落盘：stderr 与数据目录 `logs/vortex.log` 双写，超限自动截断。
//!
//! 设计：
//! - env_logger 单一 Target，通过自定义 `Write` 实现 tee：
//!   终端（开发可见）+ 文件（桌面端持久化，供排查与统计）；
//! - 文件大小上限 [`MAX_LOG_FILE_BYTES`]（默认 5MB），超限从头重写，
//!   保证磁盘占用有界——日志是排查用的滚动信息，不是账本；
//! - 统计与清理入口在 `/api/maintenance`（`logFileBytes` 与
//!   `POST /api/maintenance/log/clear`），设置页「存储维护」卡片展示。

use std::fs::{File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// 单个日志文件大小上限（字节）：超过后截断重写，控制磁盘占用
pub const MAX_LOG_FILE_BYTES: u64 = 5 * 1024 * 1024;

/// 日志文件名（位于数据目录 `logs/` 下）
const LOG_FILE_NAME: &str = "vortex.log";

/// 数据目录下的日志文件路径
fn log_path(data_dir: &Path) -> PathBuf {
    data_dir.join("logs").join(LOG_FILE_NAME)
}

/// tee 写入器：stderr + 文件，文件超限自动截断
struct LogWriter {
    path: PathBuf,
    file: Option<Mutex<File>>,
}

impl Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // stderr 始终输出（开发/终端模式可见，失败不影响文件写入）
        let _ = io::stderr().write_all(buf);
        if let Some(file) = &self.file {
            let mut f = file.lock().unwrap_or_else(|e| e.into_inner());
            // 超限截断：重开（truncate）控制磁盘占用
            let need_rotate = match f.metadata() {
                Ok(meta) => meta.len() + buf.len() as u64 > MAX_LOG_FILE_BYTES,
                Err(_) => false,
            };
            if need_rotate {
                match OpenOptions::new()
                    .create(true)
                    // append 必须带上：保证所有写入始终落在当前 EOF，
                    // 外部（设置页清空）truncate 后继续写不会产生 NUL 稀疏文件
                    .append(true)
                    .truncate(true)
                    .open(&self.path)
                {
                    Ok(nf) => *f = nf,
                    // 重开失败：丢弃本条文件写入，保留 stderr 输出
                    Err(_) => return Ok(buf.len()),
                }
            }
            let _ = f.write_all(buf);
            let _ = f.flush();
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// 初始化全局日志：默认过滤器 + 输出 tee 到 stderr 与 `{data_dir}/logs/vortex.log`。
///
/// 供桌面端（src-tauri）与独立服务器（vortex-server）共用，
/// 替代各自直接调用 `env_logger`。文件不可写时静默降级为仅 stderr。
pub fn init_logging(data_dir: &Path) {
    let path = log_path(data_dir);
    let file = std::fs::create_dir_all(path.parent().unwrap_or(data_dir))
        .ok()
        .and_then(|_| OpenOptions::new().create(true).append(true).open(&path).ok())
        .map(Mutex::new);
    if file.is_none() {
        // 目录创建失败等：文件日志不可用，提前告知（本条只进 stderr）
        eprintln!("日志文件不可用，仅输出到终端: {}", path.display());
    }
    let writer = LogWriter { path, file };
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(crate::DEFAULT_LOG_FILTER),
    )
    .write_style(env_logger::WriteStyle::Never)
    .target(env_logger::Target::Pipe(Box::new(writer)))
    .try_init();
}

/// 日志文件当前大小（字节）；文件不存在返回 0
pub fn log_file_size(data_dir: &Path) -> u64 {
    std::fs::metadata(log_path(data_dir)).map(|m| m.len()).unwrap_or(0)
}

/// 清空日志文件，返回清理前大小（字节）；文件不存在返回 0
pub fn clear_log_file(data_dir: &Path) -> io::Result<u64> {
    let path = log_path(data_dir);
    if !path.exists() {
        return Ok(0);
    }
    let before = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    // truncate 保留文件句柄（写入器仍持有旧句柄，继续可写）
    let mut f = OpenOptions::new().write(true).open(&path)?;
    f.set_len(0)?;
    f.seek(SeekFrom::Start(0))?;
    Ok(before)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_size_and_clear() {
        let dir = std::env::temp_dir().join(format!("vortex-log-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("logs")).unwrap();
        let path = log_path(&dir);
        std::fs::write(&path, b"hello log").unwrap();

        assert_eq!(log_file_size(&dir), 9);
        let before = clear_log_file(&dir).unwrap();
        assert_eq!(before, 9);
        assert_eq!(log_file_size(&dir), 0);
        // 不存在时安全返回
        std::fs::remove_file(&path).unwrap();
        assert_eq!(log_file_size(&dir), 0);
        assert_eq!(clear_log_file(&dir).unwrap(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
