//! 登录二维码落盘：data URL 解码 → 保存图片文件 + 可直接浏览器打开的 HTML
//!
//! 支持 `data:image/svg+xml` 与常见位图类型（base64 或百分号编码）。
//! 外链二维码（http/https）不落盘，由调用方直接使用响应中的 URL。

use super::util::percent_decode;

/// 用内容字符串本地生成二维码 SVG data URL（data:image/svg+xml;base64,...）。
/// 用于无法从页面直接拿到二维码图片、但知道二维码内容的平台（如 Kimi 微信扫码）。
pub(crate) fn qr_svg_data_url(content: &str) -> anyhow::Result<String> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use qrcodegen::{QrCode, QrCodeEcc};

    let qr = QrCode::encode_text(content, QrCodeEcc::Medium)
        .map_err(|e| anyhow::anyhow!("生成二维码失败: {e}"))?;
    let border = 4;
    let dim = qr.size() + border * 2;
    let mut path = String::new();
    for y in 0..qr.size() {
        for x in 0..qr.size() {
            if qr.get_module(x, y) {
                if !path.is_empty() {
                    path.push(' ');
                }
                path.push_str(&format!("M{},{}h1v1h-1z", x + border, y + border));
            }
        }
    }
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" version="1.1" viewBox="0 0 {dim} {dim}" stroke="none"><rect width="100%" height="100%" fill="#FFFFFF"/><path d="{path}" fill="#000000"/></svg>"##
    );
    Ok(format!(
        "data:image/svg+xml;base64,{}",
        STANDARD.encode(svg.as_bytes())
    ))
}

/// 把二维码 data URL 解码后保存为当前目录下的图片文件（{file_prefix}.svg / .png 等），
/// 并生成一个可在浏览器中直接打开的 HTML 文件（{file_prefix}.html）。
/// 返回 HTML 文件路径。http(s) 外链无法落盘，返回 None（此时只用响应里的 qr_data_url）。
pub(crate) fn save_qr_image(
    display: &str,
    file_prefix: &str,
    qr: &str,
    tip: &str,
) -> Option<String> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    let rest = qr.strip_prefix("data:")?;
    let (meta, payload) = rest.split_once(',')?;
    let mime = meta.split(';').next()?.strip_prefix("image/")?.to_string();
    let is_base64 = meta.contains(";base64");

    let decode_b64 = |p: &str| -> Option<Vec<u8>> { STANDARD.decode(p).ok() };

    let (bytes, ext, fixed_svg) = match mime.as_str() {
        "svg+xml" => {
            let mut svg = if is_base64 {
                String::from_utf8(decode_b64(payload)?).ok()?
            } else {
                percent_decode(payload)
            };
            // 修复缺失的 xmlns 命名空间声明（部分浏览器/查看器没有它无法渲染 SVG）
            if !svg.contains("xmlns=\"http://www.w3.org/2000/svg\"") {
                svg = svg.replacen(
                    "<svg",
                    "<svg xmlns=\"http://www.w3.org/2000/svg\"",
                    1,
                );
            }
            let svg_bytes = svg.clone().into_bytes();
            (svg_bytes, "svg", Some(svg))
        }
        "png" | "jpeg" | "webp" | "gif" | "bmp" => {
            let data = if is_base64 {
                decode_b64(payload)?
            } else {
                percent_decode(payload).into_bytes()
            };
            let ext = match mime.as_str() {
                "jpeg" => "jpg",
                other => other,
            };
            (data, ext, None)
        }
        // 未知图片类型：按 base64 尝试解码，扩展名退化为 img
        _ => (decode_b64(payload)?, "img", None),
    };

    let dir = std::env::current_dir().ok()?;

    // 保存原始图片文件
    let img_path = dir.join(format!("{file_prefix}.{ext}"));
    std::fs::write(&img_path, &bytes).ok()?;
    let img_p = img_path.to_string_lossy().to_string();

    // 生成 HTML 文件（浏览器中直接打开扫码）
    let inner = if let Some(svg_text) = &fixed_svg {
        format!(
            r#"<div class="qr" id="qr-svg">{svg_text}</div>
  <canvas class="qr" id="qr-canvas" style="display:none"></canvas>
  <script>
  (function() {{
    var svg = document.querySelector('#qr-svg svg');
    var canvas = document.getElementById('qr-canvas');
    if (!svg || !canvas) return;
    var size = 280;
    canvas.width = size; canvas.height = size;
    var ctx = canvas.getContext('2d');
    var svgStr = new XMLSerializer().serializeToString(svg);
    var img = new Image();
    img.onload = function() {{
      ctx.fillStyle = '#fff'; ctx.fillRect(0,0,size,size);
      ctx.drawImage(img, 0, 0, size, size);
      svg.style.display = 'none';
      canvas.style.display = 'inline';
    }};
    img.onerror = function() {{ console.log('canvas fallback failed, showing raw SVG'); }};
    img.src = 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(svgStr);
  }})();
  </script>"#
        )
    } else {
        format!("<img class=\"qr\" src=\"{qr}\" alt=\"登录二维码\">")
    };
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="zh">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{display}登录二维码</title>
<style>
  body {{ margin:0; display:flex; flex-direction:column; align-items:center;
         justify-content:center; min-height:100vh; background:#f5f5f5;
         font-family:system-ui,-apple-system,sans-serif; }}
  h2 {{ color:#333; margin-bottom:8px; }}
  p  {{ color:#888; margin-top:8px; }}
  .box {{ background:#fff; padding:24px 32px; border-radius:12px;
          box-shadow:0 2px 12px rgba(0,0,0,.08); text-align:center; }}
  .qr {{ width:300px; height:300px; image-rendering:pixelated; }}
  .qr svg {{ width:300px; height:300px; }}
  .ts {{ color:#aaa; font-size:12px; margin-top:12px; }}
</style>
</head>
<body>
<div class="box">
  <h2>{display}登录二维码</h2>
  {inner}
  <p>{tip}</p>
  <div class="ts">生成时间：{ts}</div>
</div>
</body>
</html>"#,
        ts = {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let secs = now % 60;
            let mins = (now / 60) % 60;
            let hours = (now / 3600 + 8) % 24;
            format!("{hours:02}:{mins:02}:{secs:02} (UTC+8)")
        }
    );
    let html_path = dir.join(format!("{file_prefix}.html"));
    match std::fs::write(&html_path, html.as_bytes()) {
        Ok(_) => Some(html_path.to_string_lossy().to_string()),
        Err(e) => {
            tracing::warn!("保存 HTML 失败: {e}，退回原始图片路径");
            Some(img_p)
        }
    }
}
