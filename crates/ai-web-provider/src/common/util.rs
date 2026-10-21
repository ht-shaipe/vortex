//! 通用工具：UTF-8 截断、百分号解码、页面 HTML 导出与调试追踪

use std::time::SystemTime;

/// 取渲染后的完整 HTML
pub(crate) const DUMP_HTML_JS: &str = r#"document.documentElement.outerHTML"#;

/// 安全截断 UTF-8 字符串到 n 个字符（不切多字节字符）
pub(crate) fn truncate_utf8(s: &str, n: usize) -> &str {
    match s.char_indices().nth(n) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

/// URL 百分号解码（%XX → 字节），仅按字节处理避免 UTF-8 边界问题
pub(crate) fn percent_decode(s: &str) -> String {
    fn hex_val(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    }
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            match (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                (Some(h), Some(l)) => {
                    out.push(h * 16 + l);
                    i += 3;
                    continue;
                }
                _ => {}
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 把渲染后的页面 HTML 写入临时文件（/tmp/{file_prefix}-{毫秒时间戳}.html），返回文件路径
pub(crate) fn dump_page_html(page: &obscura::Page, file_prefix: &str) -> String {
    let html = page
        .evaluate(DUMP_HTML_JS)
        .as_str()
        .unwrap_or("")
        .to_string();
    let ts = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let path = std::env::temp_dir().join(format!("{file_prefix}-{ts}.html"));
    match std::fs::File::create(&path) {
        Ok(mut f) => {
            use std::io::Write;
            let _ = f.write_all(html.as_bytes());
            path.to_string_lossy().to_string()
        }
        Err(e) => {
            tracing::error!("保存 HTML 失败: {e}");
            String::new()
        }
    }
}

/// 调试 HTML / SSE 落盘目录。
///
/// 桌面端进程的当前目录是 `src-tauri`，直接写相对路径的 `html/` 会把
/// 调试产物堆进仓库。统一放到系统临时目录，保留排查能力但不污染项目。
pub(crate) fn debug_html_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("vortex-debug-html");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// 调试模式：把当前渲染后的页面 HTML 存到系统临时目录。
/// 文件名 = 时间戳毫秒 + 步骤名，自然按时间排序；未开启调试模式时是空操作。
/// 在每步操作（导航/点击/填入/发送）完成、页面渲染稳定后调用。
pub(crate) fn trace_page_html(page: &obscura::Page, debug_html: bool, step: &str) {
    if !debug_html {
        return;
    }
    let html = page
        .evaluate(DUMP_HTML_JS)
        .as_str()
        .unwrap_or("")
        .to_string();
    if html.is_empty() {
        tracing::warn!("[trace] {step}: 页面 HTML 为空，跳过");
        return;
    }
    let dir = debug_html_dir();
    let ts = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let path = dir.join(format!("{ts}_{step}.html"));
    match std::fs::write(&path, html.as_bytes()) {
        Ok(_) => {}
        Err(e) => tracing::warn!("[trace] {step}: 写入失败: {e}"),
    }
}
