//! TEMP DEBUG：前端调试日志透传（排查 data-tauri-drag-region 二次失效，之后移除）。

/// 把前端日志追加写入独立文件（不受 stdout 归属影响），并同步打应用日志。
#[tauri::command]
pub fn debug_log(message: String) {
    use std::io::Write;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/vortex-dragdbg.log")
    {
        let _ = writeln!(f, "{ts} {message}");
    }
    log::info!("[dragdbg] {message}");
}
