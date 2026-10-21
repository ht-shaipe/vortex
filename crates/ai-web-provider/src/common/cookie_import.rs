//! cookie 导入解析：EditThisCookie / Cookie-Editor JSON 数组 或 Set-Cookie 行
//! → 逐条 Set-Cookie 串（供 obscura `CookieStore::set` 逐条导入）。
//!
//! 从 chatgpt provider 抽出为公共模块，CDP 授权（cdp_auth）收集的真实
//! Chrome cookie 也转换成 EditThisCookie JSON 走同一条导入路径。

/// 解析 cookie 导入输入。两种输入：
/// 1. JSON 数组（EditThisCookie / Cookie-Editor 扩展导出格式）：
///    `[{name,value,domain,path,secure,httpOnly,sameSite,expirationDate,hostOnly}]`
/// 2. 每行一条 `name=value; Domain=...; Path=/` 的 Set-Cookie 串，原样透传
/// 已过期条目（expirationDate 早于当前时间）跳过。
pub(crate) fn parse_cookie_import(raw: &str) -> Vec<String> {
    let trimmed = raw.trim();
    if trimmed.starts_with('[') {
        if let Ok(serde_json::Value::Array(items)) =
            serde_json::from_str::<serde_json::Value>(trimmed)
        {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            return items
                .iter()
                .filter_map(|item| cookie_entry_to_set_cookie(item, now))
                .collect();
        }
        return Vec::new();
    }
    trimmed
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && l.contains('='))
        .map(String::from)
        .collect()
}

/// 单条 JSON cookie 条目 → Set-Cookie 串。None = 无名字或已过期，跳过。
pub(crate) fn cookie_entry_to_set_cookie(item: &serde_json::Value, now: i64) -> Option<String> {
    let name = item.get("name")?.as_str()?;
    if name.is_empty() {
        return None;
    }
    let value = item.get("value").and_then(|v| v.as_str()).unwrap_or("");
    let mut domain = item.get("domain").and_then(|v| v.as_str()).unwrap_or("");
    let mut path = item.get("path").and_then(|v| v.as_str()).unwrap_or("/");
    let mut secure = item
        .get("secure")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let http_only = item
        .get("httpOnly")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let host_only = item
        .get("hostOnly")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    // expirationDate（EditThisCookie）/ expires（部分格式）：epoch 秒
    let expiration = item
        .get("expirationDate")
        .and_then(|v| v.as_f64())
        .or_else(|| item.get("expires").and_then(|v| v.as_f64()));

    // RFC 6265bis 前缀约束：__Secure- 必须 Secure；__Host- 必须
    // Secure + host-only（无 Domain）+ Path=/
    let is_host_prefix = name.starts_with("__Host-");
    if name.starts_with("__Secure-") || is_host_prefix {
        secure = true;
    }
    if is_host_prefix {
        domain = "";
        path = "/";
    }

    let mut parts = vec![format!("{name}={value}")];
    if !domain.is_empty() && !host_only {
        parts.push(format!("Domain={domain}"));
    }
    parts.push(format!("Path={path}"));
    if let Some(exp) = expiration {
        let remaining = exp as i64 - now;
        if remaining <= 0 {
            return None;
        }
        parts.push(format!("Max-Age={remaining}"));
    }
    if secure {
        parts.push("Secure".to_string());
    }
    if http_only {
        parts.push("HttpOnly".to_string());
    }
    // EditThisCookie 命名：no_restriction=None；SameSite=None 必须配 Secure，
    // 否则省略属性走 jar 默认
    let same_site = item.get("sameSite").and_then(|v| v.as_str()).unwrap_or("");
    let same_site = match same_site.to_ascii_lowercase().as_str() {
        "no_restriction" | "none" => Some("None"),
        "lax" => Some("Lax"),
        "strict" => Some("Strict"),
        _ => None,
    };
    if let Some(ss) = same_site {
        if ss != "None" || secure {
            parts.push(format!("SameSite={ss}"));
        }
    }
    Some(parts.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
    }

    #[test]
    fn parse_edit_this_cookie_array() {
        let ts = now();
        let raw = format!(
            r#"[{{
              "domain": ".chatgpt.com",
              "expirationDate": {},
              "hostOnly": false,
              "httpOnly": true,
              "name": "__Secure-next-auth.session-token",
              "path": "/",
              "sameSite": "no_restriction",
              "secure": true,
              "value": "tok123"
            }},{{
              "domain": "chatgpt.com",
              "expirationDate": {},
              "hostOnly": true,
              "name": "oai-did",
              "path": "/",
              "value": "abc"
            }},{{
              "domain": "chatgpt.com",
              "expirationDate": {},
              "name": "expired-one",
              "value": "x"
            }}]"#,
            ts + 3600,
            ts + 7200,
            ts - 100
        );
        let out = parse_cookie_import(&raw);
        assert_eq!(out.len(), 2);
        let expected_prefix =
            "__Secure-next-auth.session-token=tok123; Domain=.chatgpt.com; Path=/; Max-Age=";
        assert!(out[0].starts_with(expected_prefix), "{}", out[0]);
        // 剩余秒数容忍秒级时钟漂移
        let secs: i64 = out[0]
            .split("Max-Age=")
            .nth(1)
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!((3550..=3600).contains(&secs), "Max-Age={secs}");
        assert!(
            out[0].ends_with("Secure; HttpOnly; SameSite=None"),
            "{}",
            out[0]
        );
        // hostOnly=true 不带 Domain
        assert!(out[1].starts_with("oai-did=abc; Path=/"));
        assert!(!out[1].contains("Domain="));
    }

    #[test]
    fn parse_set_cookie_lines() {
        let raw = "\n  __Secure-next-auth.session-token=tok; Domain=chatgpt.com; Path=/; Secure; HttpOnly  \n\noai-did=abc\n";
        let out = parse_cookie_import(raw);
        assert_eq!(
            out,
            vec![
                "__Secure-next-auth.session-token=tok; Domain=chatgpt.com; Path=/; Secure; HttpOnly".to_string(),
                "oai-did=abc".to_string(),
            ]
        );
    }

    #[test]
    fn parse_empty_and_broken() {
        assert!(parse_cookie_import("").is_empty());
        assert!(parse_cookie_import("   \n  ").is_empty());
        assert!(parse_cookie_import("[bad json").is_empty());
        // 无 = 的行不算 cookie
        assert!(parse_cookie_import("just text").is_empty());
    }

    #[test]
    fn secure_prefix_forces_secure() {
        let item = serde_json::json!({
            "name": "__Secure-foo", "value": "v", "domain": ".chatgpt.com",
            "secure": false, "sameSite": "no_restriction"
        });
        let out = cookie_entry_to_set_cookie(&item, now()).unwrap();
        assert!(out.contains("Secure"));
        // SameSite=None 无 Secure 会被 jar 拒绝，此处强制 Secure 后应保留
        assert!(out.contains("SameSite=None"));
    }

    #[test]
    fn host_prefix_constraints() {
        let item = serde_json::json!({
            "name": "__Host-csrf", "value": "v", "domain": ".chatgpt.com",
            "path": "/deep", "secure": false, "hostOnly": false
        });
        let out = cookie_entry_to_set_cookie(&item, now()).unwrap();
        assert!(!out.contains("Domain="), "host 前缀不得带 Domain: {out}");
        assert!(out.contains("Path=/"), "host 前缀 Path 必须 /: {out}");
        assert!(out.contains("Secure"));
    }
}
