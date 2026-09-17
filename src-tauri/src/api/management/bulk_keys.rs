//! 批量密钥导入/导出端点。

use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;
use crate::AppState;

/// 批量导入请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkImportRequest {
    /// .env 格式文本或 JSON 数组
    content: String,
    /// 导入格式：env / json / csv
    format: String,
}

/// 批量导入结果
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkImportResult {
    success_count: usize,
    fail_count: usize,
    errors: Vec<String>,
}

/// 批量导出请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkExportRequest {
    format: String,
}

/// 解析 .env 格式密钥
fn parse_env(content: &str) -> Vec<(String, String, Option<String>)> {
    let mut result = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim().trim_matches('"').trim_matches('\'');
            // 尝试从 key 名推断 provider
            let provider = infer_provider_from_key(key);
            result.push((provider, value.to_string(), Some(key.to_string())));
        }
    }
    result
}

/// 从环境变量名推断 provider
fn infer_provider_from_key(key: &str) -> String {
    let lower = key.to_lowercase();
    if lower.contains("openai") { "openai".into() }
    else if lower.contains("anthropic") { "anthropic".into() }
    else if lower.contains("gemini") || lower.contains("google") { "gemini".into() }
    else if lower.contains("deepseek") { "deepseek".into() }
    else if lower.contains("groq") { "groq".into() }
    else if lower.contains("mistral") { "mistral".into() }
    else if lower.contains("openrouter") { "openrouter".into() }
    else if lower.contains("xai") || lower.contains("grok") { "xai".into() }
    else if lower.contains("nvidia") { "nvidia".into() }
    else if lower.contains("cloudflare") { "cloudflare".into() }
    else if lower.contains("huggingface") || lower.contains("hf") { "huggingface".into() }
    else if lower.contains("siliconflow") { "siliconflow".into() }
    else if lower.contains("qwen") || lower.contains("dashscope") { "qwen".into() }
    else if lower.contains("zai") || lower.contains("zhipu") { "zai".into() }
    else if lower.contains("volcengine") || lower.contains("ark") { "volcengine-code".into() }
    else if lower.contains("sensenova") { "sensenova".into() }
    else if lower.contains("minimax") { "minimax".into() }
    else if lower.contains("cohere") { "cohere".into() }
    else { "custom-openai".into() }
}

pub async fn bulk_import(
    state: web::Data<std::sync::Arc<AppState>>,
    body: web::Json<BulkImportRequest>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let enc_key = state.encryption_key.clone();
    let req = body.into_inner();

    let entries = match req.format.as_str() {
        "env" => parse_env(&req.content),
        "json" => {
            match serde_json::from_str::<Vec<serde_json::Value>>(&req.content) {
                Ok(arr) => arr.into_iter().filter_map(|v| {
                    let provider = v.get("provider")?.as_str()?.to_string();
                    let key = v.get("apiKey")?.as_str()?.to_string();
                    let name = v.get("name").and_then(|n| n.as_str()).map(|s| s.to_string());
                    Some((provider, key, name))
                }).collect(),
                Err(_) => return HttpResponse::BadRequest().json(json!({ "error": "Invalid JSON format" })),
            }
        }
        _ => return HttpResponse::BadRequest().json(json!({ "error": "Unsupported format" })),
    };

    let result = tokio::task::spawn_blocking(move || -> Result<BulkImportResult, String> {
        let conn = crate::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        let mut success_count = 0;
        let mut fail_count = 0;
        let mut errors = Vec::new();

        for (provider, api_key, name) in &entries {
            let conn_name = name.clone().unwrap_or_else(|| format!("{}-bulk", provider));
            let req = crate::db::models::CreateProviderRequest {
                provider: provider.clone(),
                auth_type: Some("apikey".into()),
                name: conn_name,
                email: None,
                api_key: Some(api_key.clone()),
                access_token: None,
                refresh_token: None,
                project_id: None,
                base_url: None,
                priority: None,
                group_name: None,
                max_concurrent: None,
                default_model: None,
                models: None,
                display_name: None,
                api_protocol: None,
                chat_path: None,
                custom_provider_id: None,
                provider_specific_data: None,
            };
            match crate::db::providers::create(&conn, &req, &enc_key) {
                Ok(_) => success_count += 1,
                Err(e) => {
                    fail_count += 1;
                    errors.push(format!("{}: {}", provider, e));
                }
            }
        }
        Ok(BulkImportResult { success_count, fail_count, errors })
    })
    .await;

    match result {
        Ok(Ok(r)) => HttpResponse::Ok().json(r),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

pub async fn bulk_export(
    state: web::Data<std::sync::Arc<AppState>>,
    body: web::Json<BulkExportRequest>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let enc_key = state.encryption_key.clone();
    let format = body.into_inner().format;

    let result = tokio::task::spawn_blocking(move || -> Result<String, String> {
        let conn = crate::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        let providers = crate::db::providers::list_all_active(&conn, &enc_key).map_err(|e| e.to_string())?;

        match format.as_str() {
            "env" => {
                let mut lines = Vec::new();
                for p in &providers {
                    if let Some(ref key) = p.api_key {
                        let env_name = format!("{}_API_KEY", p.provider.to_uppercase());
                        lines.push(format!("{}={}", env_name, key));
                    }
                }
                Ok(lines.join("\n"))
            }
            "json" => {
                let arr: Vec<serde_json::Value> = providers.iter().map(|p| {
                    json!({
                        "provider": p.provider,
                        "name": p.name,
                        "apiKey": p.api_key,
                        "priority": p.priority,
                        "defaultModel": p.default_model,
                    })
                }).collect();
                Ok(serde_json::to_string_pretty(&arr).unwrap_or_default())
            }
            "csv" => {
                let mut lines = vec!["provider,name,api_key,priority,default_model".to_string()];
                for p in &providers {
                    if let Some(ref key) = p.api_key {
                        lines.push(format!("{},{},{},{},{}", p.provider, p.name, key, p.priority, p.default_model.as_deref().unwrap_or("")));
                    }
                }
                Ok(lines.join("\n"))
            }
            _ => Err("Unsupported format".into()),
        }
    })
    .await;

    match result {
        Ok(Ok(content)) => HttpResponse::Ok().json(json!({ "content": content })),
        Ok(Err(e)) => HttpResponse::BadRequest().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}
