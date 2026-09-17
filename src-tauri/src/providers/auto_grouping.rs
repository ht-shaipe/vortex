//! 模型自动归纳：按模型家族将同族模型分组，生成虚拟模型别名。
//!
//! 家族提取规则：迭代剥离模型名末尾的版本/变体后缀，剩余部分即为家族名。
//! 可剥离的后缀包括：日期（`-2024-08-06`、`-20241022`、`-0613`）、
//! `-latest`、`-preview`、`-free`、上下文长度（`-16k`、`-32k`、`-128k`、`-1m`）等。
//! 每个家族都生成别名（含单模型家族），按"无后缀优先 > 有后缀"排序作为故障转移目标，
//! 保证所有真实模型都有对应的虚拟模型名。

use crate::db::models::ModelAliasTarget;
use std::collections::HashMap;

/// 执行自动归纳全流程：读取所有活跃连接的模型 → 按家族分组 →
/// 删除旧 auto 别名 → 批量创建新别名（跳过与 manual 同名的）。
///
/// 供保存订阅后自动调用，以及 `/api/model-aliases/auto-generate` 端点调用。
pub fn run_auto_grouping(
    conn: &rusqlite::Connection,
    enc_key: &[u8],
) -> crate::error::Result<Vec<(String, Vec<ModelAliasTarget>)>> {
    // 收集所有活跃连接的模型
    let all_conns = crate::db::providers::list(conn, enc_key)?;
    let mut models: Vec<(String, String, String)> = Vec::new();
    for c in &all_conns {
        if !c.is_active {
            continue;
        }
        if let Some(ms) = &c.models {
            for m in ms {
                models.push((c.provider.clone(), m.id.clone(), c.id.clone()));
            }
        } else if let Some(dm) = &c.default_model {
            models.push((c.provider.clone(), dm.clone(), c.id.clone()));
        }
    }

    // 按家族分组
    let groups = group_by_family(&models);

    // 删除旧 auto 别名 + 创建新别名
    crate::db::model_aliases::delete_all_auto(conn)?;
    crate::db::model_aliases::create_auto_batch(conn, &groups)?;

    Ok(groups)
}

/// 从模型名中提取家族名。
///
/// 迭代剥离末尾的版本/变体后缀，直到无法继续剥离。
/// 同时规范化 `claude-3-5-` → `claude-3.5-` 以统一 Anthropic 命名变体。
pub fn extract_family(model: &str) -> String {
    let mut name = model.to_lowercase();

    // 剥离提供商前缀：z-ai/glm-5.3-flash → glm-5.3-flash
    if let Some(pos) = name.rfind('/') {
        name = name[pos + 1..].to_string();
    }

    // 规范化 claude-3-5-xxx → claude-3.5-xxx
    name = name.replace("claude-3-5-", "claude-3.5-");

    // 迭代剥离末尾后缀
    loop {
        let stripped = strip_one_suffix(&name);
        match stripped {
            Some(s) => name = s,
            None => break,
        }
    }

    name
}

/// 尝试剥除一个末尾后缀。返回 None 表示无可剥离后缀。
fn strip_one_suffix(s: &str) -> Option<String> {
    // 日期后缀：-YYYY-MM-DD
    if s.len() > 11 {
        let tail = &s[s.len() - 11..];
        if tail.starts_with('-') && tail[1..5].chars().all(|c| c.is_ascii_digit())
            && &tail[5..6] == "-" && tail[6..8].chars().all(|c| c.is_ascii_digit())
            && &tail[8..9] == "-" && tail[9..11].chars().all(|c| c.is_ascii_digit())
        {
            return Some(s[..s.len() - 11].to_string());
        }
    }
    // 日期后缀：-YYYYMMDD
    if s.len() > 9 {
        let tail = &s[s.len() - 9..];
        if tail.starts_with('-') && tail[1..9].chars().all(|c| c.is_ascii_digit()) {
            return Some(s[..s.len() - 9].to_string());
        }
    }
    // 三位数字版本号：-NNN (如 -002)
    if s.len() > 4 {
        let tail = &s[s.len() - 4..];
        if tail.starts_with('-') && tail[1..4].chars().all(|c| c.is_ascii_digit()) {
            return Some(s[..s.len() - 4].to_string());
        }
    }
    // 四位数字日期/版本：-MMDD (如 -0613)
    if s.len() > 5 {
        let tail = &s[s.len() - 5..];
        if tail.starts_with('-') && tail[1..5].chars().all(|c| c.is_ascii_digit()) {
            return Some(s[..s.len() - 5].to_string());
        }
    }
    // 固定字符串后缀（:free 为 OpenRouter 等平台的免费变体标记）
    for suffix in &["-latest", "-preview", "-free", ":free"] {
        if let Some(stripped) = s.strip_suffix(suffix) {
            return Some(stripped.to_string());
        }
    }
    // 上下文长度后缀：-\d+k 或 -\d+m (如 -16k, -32k, -128k, -1m)
    if let Some(stripped) = strip_context_length(s) {
        return Some(stripped);
    }
    None
}

/// 尝试剥除末尾的上下文长度后缀（`-16k`、`-32k`、`-128k`、`-1m` 等）。
fn strip_context_length(s: &str) -> Option<String> {
    if !s.contains('-') {
        return None;
    }
    let last_dash = s.rfind('-')?;
    let suffix = &s[last_dash + 1..];
    // 必须以 k 或 m 结尾，前面是纯数字
    if (suffix.ends_with('k') || suffix.ends_with('m')) && suffix.len() >= 2 {
        let nums = &suffix[..suffix.len() - 1];
        if !nums.is_empty() && nums.chars().all(|c| c.is_ascii_digit()) {
            return Some(s[..last_dash].to_string());
        }
    }
    None
}

/// 判断模型名是否含日期/版本后缀（用于排序：无后缀优先）。
fn has_version_suffix(model: &str) -> bool {
    extract_family(model) != model.to_lowercase().replace("claude-3-5-", "claude-3.5-")
}

/// 按家族分组模型，生成 (别名, 目标列表) 对。
///
/// 每个家族都生成别名（含单模型家族），目标按"无版本后缀优先"排序。
/// 单模型家族也生成同名别名，确保开启「隐藏已映射的真实模型」后
/// 每个真实模型都有对应的虚拟模型名可供选用。
pub fn group_by_family(
    models: &[(String, String, String)], // (provider_id, model_id, connection_id)
) -> Vec<(String, Vec<ModelAliasTarget>)> {
    // family → Vec<(provider, model, connection_id, original_model)>
    let mut groups: HashMap<String, Vec<(String, String, String, String)>> = HashMap::new();

    for (provider, model, conn_id) in models {
        let family = extract_family(model);
        groups
            .entry(family)
            .or_default()
            .push((provider.clone(), model.clone(), conn_id.clone(), model.clone()));
    }

    let mut result: Vec<(String, Vec<ModelAliasTarget>)> = Vec::new();

    for (family, mut members) in groups {
        // 排序：无版本后缀优先，其次按模型名字母序
        members.sort_by(|a, b| {
            let a_has = has_version_suffix(&a.3);
            let b_has = has_version_suffix(&b.3);
            match (a_has, b_has) {
                (false, true) => std::cmp::Ordering::Less,
                (true, false) => std::cmp::Ordering::Greater,
                _ => a.3.cmp(&b.3),
            }
        });

        let targets: Vec<ModelAliasTarget> = members
            .into_iter()
            .map(|(provider, model, conn_id, _)| ModelAliasTarget {
                provider,
                model,
                connection_id: Some(conn_id),
            })
            .collect();

        result.push((family, targets));
    }

    // 按别名排序，保证输出稳定
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_family() {
        // 基本家族
        assert_eq!(extract_family("gpt-4o"), "gpt-4o");
        assert_eq!(extract_family("gpt-4o-2024-08-06"), "gpt-4o");
        assert_eq!(extract_family("gpt-4o-mini-2024-07-18"), "gpt-4o-mini");
        assert_eq!(extract_family("claude-3.5-sonnet-20241022"), "claude-3.5-sonnet");
        assert_eq!(extract_family("claude-3-5-sonnet-latest"), "claude-3.5-sonnet");
        assert_eq!(extract_family("gemini-1.5-pro-latest"), "gemini-1.5-pro");
        assert_eq!(extract_family("gemini-1.5-flash-002"), "gemini-1.5-flash");
        assert_eq!(extract_family("deepseek-chat"), "deepseek-chat");
        assert_eq!(extract_family("o1-preview"), "o1");
        assert_eq!(extract_family("gpt-4-turbo-preview"), "gpt-4-turbo");

        // -free / :free 后缀（:free 为 OpenRouter 风格）
        assert_eq!(extract_family("glm-4-flash-free"), "glm-4-flash");
        assert_eq!(extract_family("glm-4-flash"), "glm-4-flash");
        assert_eq!(extract_family("qwen-max-free"), "qwen-max");
        assert_eq!(extract_family("glm-5.3-flash:free"), "glm-5.3-flash");

        // 上下文长度后缀
        assert_eq!(extract_family("gpt-3.5-turbo-16k"), "gpt-3.5-turbo");
        assert_eq!(extract_family("gpt-3.5-turbo-32k"), "gpt-3.5-turbo");
        assert_eq!(extract_family("gpt-4-128k"), "gpt-4");
        assert_eq!(extract_family("claude-2.1-200k"), "claude-2.1");

        // 迭代剥离多层后缀
        assert_eq!(extract_family("gpt-3.5-turbo-16k-0613"), "gpt-3.5-turbo");
        assert_eq!(extract_family("glm-4-flash-free-2024"), "glm-4-flash");

        // 四位数字日期 -0613
        assert_eq!(extract_family("gpt-3.5-turbo-0613"), "gpt-3.5-turbo");

        // 提供商前缀剥离
        assert_eq!(extract_family("z-ai/glm-5.3-flash"), "glm-5.3-flash");
        assert_eq!(extract_family("z-ai/glm-4-flash-free"), "glm-4-flash");
        assert_eq!(extract_family("openai/gpt-4o-2024-08-06"), "gpt-4o");
        assert_eq!(extract_family("siliconflow/Qwen/Qwen2.5-7B-Instruct"), "qwen2.5-7b-instruct");
    }

    #[test]
    fn test_group_by_family() {
        let models = vec![
            ("openai".into(), "gpt-4o".into(), "conn1".into()),
            ("openai".into(), "gpt-4o-2024-08-06".into(), "conn1".into()),
            ("openai".into(), "gpt-4o-mini".into(), "conn1".into()),
            ("openai".into(), "gpt-4o-mini-2024-07-18".into(), "conn1".into()),
            ("deepseek".into(), "deepseek-chat".into(), "conn2".into()),
        ];

        let groups = group_by_family(&models);

        // gpt-4o 家族有 2 个模型
        let gpt4o = groups.iter().find(|(a, _)| a == "gpt-4o").unwrap();
        assert_eq!(gpt4o.1.len(), 2);
        assert_eq!(gpt4o.1[0].model, "gpt-4o"); // 无后缀优先
        assert_eq!(gpt4o.1[1].model, "gpt-4o-2024-08-06");

        // gpt-4o-mini 家族有 2 个模型
        let gpt4o_mini = groups.iter().find(|(a, _)| a == "gpt-4o-mini").unwrap();
        assert_eq!(gpt4o_mini.1.len(), 2);
        assert_eq!(gpt4o_mini.1[0].model, "gpt-4o-mini");

        // deepseek-chat 单模型家族也生成别名（1 个目标）
        let ds = groups.iter().find(|(a, _)| a == "deepseek-chat").unwrap();
        assert_eq!(ds.1.len(), 1);
        assert_eq!(ds.1[0].model, "deepseek-chat");
    }

    #[test]
    fn test_group_by_family_with_free_and_context() {
        let models = vec![
            ("zai".into(), "glm-4-flash".into(), "conn1".into()),
            ("zai".into(), "glm-4-flash-free".into(), "conn1".into()),
            ("openai".into(), "gpt-3.5-turbo".into(), "conn2".into()),
            ("openai".into(), "gpt-3.5-turbo-16k".into(), "conn2".into()),
            ("openai".into(), "gpt-3.5-turbo-0613".into(), "conn2".into()),
        ];

        let groups = group_by_family(&models);

        // glm-4-flash 家族：flash + flash-free → 2 个
        let glm = groups.iter().find(|(a, _)| a == "glm-4-flash").unwrap();
        assert_eq!(glm.1.len(), 2);
        assert_eq!(glm.1[0].model, "glm-4-flash"); // 无后缀优先

        // gpt-3.5-turbo 家族：turbo + turbo-16k + turbo-0613 → 3 个
        let turbo = groups.iter().find(|(a, _)| a == "gpt-3.5-turbo").unwrap();
        assert_eq!(turbo.1.len(), 3);
        assert_eq!(turbo.1[0].model, "gpt-3.5-turbo"); // 无后缀优先
    }
}
