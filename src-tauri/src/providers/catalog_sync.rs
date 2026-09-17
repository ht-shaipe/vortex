//! 模型目录同步模块。
//!
//! 从已配置的活跃连接拉取 /v1/models 端点，更新本地 model_catalog 表。
//! 启动后延迟 10 秒首次拉取，之后每 12 小时循环。

use crate::db::model_catalog::{self, CatalogEntry};
use crate::error::Result;
use serde::Deserialize;

/// 从已配置的活跃连接自动拉取 `/v1/models`（或 `/models`）端点，
/// 将返回的模型列表合并到 model_catalog 表。
///
/// 仅处理 OpenAI 兼容的连接（provider 非 google 且 base_url 存在）。
/// Anthropic 使用 `x-api-key` 头，其余使用 `Authorization: Bearer`。
/// 返回 `(成功连接数, 新增/更新模型总数, 各连接结果)`。
pub async fn refresh_from_connections(
    pool: &crate::db::core::DbPool,
    enc_key: &[u8],
    provider_defaults: &std::collections::HashMap<String, (String, String)>,
) -> Result<(usize, usize, Vec<ConnectionRefreshResult>)> {
    let pool_clone = pool.clone();
    let enc_key_clone = enc_key.to_vec();
    let connections = tokio::task::spawn_blocking(move || -> std::result::Result<Vec<crate::db::models::ProviderConnection>, String> {
        let conn = crate::db::core::get_conn(&pool_clone).map_err(|e| e.to_string())?;
        crate::db::providers::list_all_active(&conn, &enc_key_clone).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| crate::error::AppError::Internal(format!("DB task failed: {}", e)))?
    .map_err(crate::error::AppError::Internal)?;

    let client = awc::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .finish();

    let mut results = Vec::new();
    let mut all_entries = Vec::new();
    let now = chrono::Utc::now().to_rfc3339();

    for conn in &connections {
        let base_url = match conn.provider_specific_data.get("baseUrl") {
            Some(v) => {
                let s = v.as_str().unwrap_or("").trim_end_matches('/').to_string();
                if s.is_empty() {
                    provider_defaults.get(&conn.provider).map(|(b, _)| b.clone()).unwrap_or_default()
                } else {
                    s
                }
            }
            None => provider_defaults.get(&conn.provider).map(|(b, _)| b.clone()).unwrap_or_default(),
        };
        if base_url.is_empty() {
            log::warn!("连接 {} 无 base_url，跳过", conn.name);
            continue;
        }
        let api_key = match &conn.api_key {
            Some(k) if !k.is_empty() => k.clone(),
            _ => {
                log::warn!("连接 {} 无 api_key，跳过", conn.name);
                continue;
            }
        };

        let models_url = if let Some((_, mp)) = provider_defaults.get(&conn.provider) {
            if !mp.is_empty() {
                format!("{}{}", base_url, mp)
            } else {
                format!("{}/models", base_url)
            }
        } else if base_url.ends_with("/v1") || base_url.ends_with("/v1/") {
            format!("{}/models", base_url.trim_end_matches('/'))
        } else {
            format!("{}/v1/models", base_url)
        };

        let provider = conn
            .provider_specific_data
            .get("customId")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| conn.provider.clone());
        let is_anthropic = conn.provider == "anthropic";

        log::info!("从连接 {} 拉取模型列表: {}", conn.name, models_url);

        let mut req = client
            .get(&models_url)
            .insert_header(("User-Agent", "Vortex/0.1"))
            .insert_header(("Accept", "application/json"));

        if is_anthropic {
            req = req
                .insert_header(("x-api-key", api_key.as_str()))
                .insert_header(("anthropic-version", "2023-06-01"));
        } else {
            req = req.insert_header(("Authorization", format!("Bearer {}", api_key)));
        }

        let mut resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                log::warn!("从 {} 拉取模型列表失败 ({}): {}", conn.name, models_url, e);
                results.push(ConnectionRefreshResult {
                    connection_name: conn.name.clone(),
                    provider: provider.clone(),
                    models_url,
                    model_count: 0,
                    error: Some(format!("请求失败: {}", e)),
                });
                continue;
            }
        };

        if !resp.status().is_success() {
            let status = resp.status();
            results.push(ConnectionRefreshResult {
                connection_name: conn.name.clone(),
                provider: provider.clone(),
                models_url,
                model_count: 0,
                error: Some(format!("HTTP {}", status)),
            });
            continue;
        }

        let body = match resp.body().await {
            Ok(b) => b,
            Err(e) => {
                results.push(ConnectionRefreshResult {
                    connection_name: conn.name.clone(),
                    provider: provider.clone(),
                    models_url,
                    model_count: 0,
                    error: Some(format!("读取响应失败: {}", e)),
                });
                continue;
            }
        };

        #[derive(Deserialize)]
        struct ModelsResponse {
            #[serde(default)]
            data: Vec<ModelItem>,
        }
        #[derive(Deserialize)]
        struct ModelItem {
            id: String,
        }

        let models_resp: ModelsResponse = match serde_json::from_slice(&body) {
            Ok(r) => r,
            Err(e) => {
                results.push(ConnectionRefreshResult {
                    connection_name: conn.name.clone(),
                    provider: provider.clone(),
                    models_url,
                    model_count: 0,
                    error: Some(format!("解析失败: {}", e)),
                });
                continue;
            }
        };

        let count = models_resp.data.len();
        for item in models_resp.data {
            all_entries.push(CatalogEntry {
                id: uuid::Uuid::new_v4().to_string(),
                provider: provider.clone(),
                model: item.id.clone(),
                name: Some(item.id),
                context_window: None,
                rpm: None,
                rpd: None,
                tpm: None,
                tpd: None,
                free_quota: None,
                supports_tools: false,
                supports_streaming: true,
                supports_vision: false,
                capabilities: serde_json::Value::Array(vec![]),
                source_feed: Some(format!("connection:{}", conn.name)),
                is_active: true,
                last_synced: Some(now.clone()),
                created_at: now.clone(),
                updated_at: now.clone(),
            });
        }

        results.push(ConnectionRefreshResult {
            connection_name: conn.name.clone(),
            provider: provider.clone(),
            models_url,
            model_count: count,
            error: None,
        });
    }

    let total_models = all_entries.len();
    let success_connections = results.iter().filter(|r| r.error.is_none()).count();

    if !all_entries.is_empty() {
        let pool_clone2 = pool.clone();
        let entries_clone = all_entries.clone();
        tokio::task::spawn_blocking(move || -> std::result::Result<(), String> {
            let conn = crate::db::core::get_conn(&pool_clone2).map_err(|e| e.to_string())?;
            model_catalog::batch_upsert(&conn, &entries_clone).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("DB write task failed: {}", e)))?
        .map_err(crate::error::AppError::Internal)?;
    }

    log::info!(
        "从连接刷新模型目录完成: {} 个连接成功, {} 条模型",
        success_connections,
        total_models
    );

    Ok((success_connections, total_models, results))
}

/// 单个连接的模型目录刷新结果
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionRefreshResult {
    pub connection_name: String,
    pub provider: String,
    pub models_url: String,
    pub model_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
