//! Fusion 多模型合成：fan-out 到多个模型并行执行，judge 模型合成最终答案。

use serde_json::Value;

/// Fusion 配置
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct FusionConfig {
    /// 参与的模型列表
    pub panel_models: Vec<(String, String)>,
    /// judge 模型
    pub judge_model: (String, String),
    /// 是否启用
    pub enabled: bool,
}

/// 生成 fan-out 请求列表
#[allow(dead_code)]
pub fn create_fanout_requests(request: &Value, panel_models: &[(String, String)]) -> Vec<Value> {
    let mut requests = Vec::new();
    for (_provider, model) in panel_models {
        let mut req = request.clone();
        if let Some(obj) = req.as_object_mut() {
            obj.insert("model".into(), Value::String(model.clone()));
        }
        requests.push(req);
    }
    requests
}

/// 生成 judge 合成请求
/// 生成 judge 请求
#[allow(dead_code)]
pub fn create_judge_request(original_prompt: &str, drafts: &[String], judge_model: &str) -> Value {
    let drafts_text = drafts
        .iter()
        .enumerate()
        .map(|(i, d)| format!("Draft {}:\n{}", i + 1, d))
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");

    let judge_prompt = format!(
        "You are a synthesis judge. Below are multiple draft responses to the same prompt. \
        Synthesize the best aspects of each draft into a single, coherent, high-quality response.\n\n\
        Original prompt:\n{}\n\n\
        Drafts:\n{}\n\n\
        Synthesized response:",
        original_prompt, drafts_text
    );

    serde_json::json!({
        "model": judge_model,
        "messages": [
            { "role": "user", "content": judge_prompt }
        ],
        "stream": false
    })
}

/// 从响应中提取文本
/// 从响应中提取文本
#[allow(dead_code)]
pub fn extract_text(response: &Value) -> String {
    response
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string()
}
