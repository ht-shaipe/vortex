//! �%模型目录同步模块。
//!
//! 定期从远程 feed 拉取模型目录，更新本地 model_catalog 表。
//! 同步频率：每 12 小时一次。支持手动触发。

use crate::db::model_catalog::{self, CatalogEntry};
use crate::error::Result;
use serde::Deserialize;

/// 远程 feed 响应格式
#[derive(Debug, Deserialize)]
struct FeedResponse {
    models: Vec<FeedModel>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FeedModel {
    provider: String,
    model: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    context_window: Option<i64>,
    #[serde(default)]
    rpm: Option<i64>,
    #[serde(default)]
    rpd: Option<i64>,
    #[serde(default)]
    tpm: Option<i64>,
    #[serde(default)]
    tpd: Option<i64>,
    #[serde(default)]
    free_quota: Option<String>,
    #[serde(default)]
    supports_tools: bool,
    #[serde(default)]
    supports_streaming: bool,
    #[serde(default)]
    supports_vision: bool,
    #[serde(default)]
    capabilities: serde_json::Value,
}

/// 内置模型目录种子数据（编译期内嵌，随应用版本更新）
const BUILTIN_SEED: &str = include_str!("catalog_seed.json");

/// 从远程 feed 拉取并同步模型目录
pub async fn sync_catalog(
    pool: &crate::db::core::DbPool,
    feed_url: &str,
) -> Result<usize> {
    log::info!("开始同步模型目录: {}", feed_url);

    let client = awc::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .finish();

    let mut response = client
        .get(feed_url)
        .insert_header(("User-Agent", "Vortex/0.1"))
        .send()
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("Feed fetch failed: {}", e)))?;

    if !response.status().is_success() {
        return Err(crate::error::AppError::Internal(format!(
            "Feed returned HTTP {}",
            response.status()
        )));
    }

    let body = response
        .body()
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("Feed body read failed: {}", e)))?;

    let feed: FeedResponse = serde_json::from_slice(&body)
        .map_err(|e| crate::error::AppError::Internal(format!("Feed parse failed: {}", e)))?;

    save_entries(pool, feed.models, feed_url).await
}

/// 把一批 feed 模型条目写入数据库（共用落库逻辑）
async fn save_entries(
    pool: &crate::db::core::DbPool,
    models: Vec<FeedModel>,
    source_feed: &str,
) -> Result<usize> {
    let now = chrono::Utc::now().to_rfc3339();
    let entries: Vec<CatalogEntry> = models
        .into_iter()
        .map(|m| CatalogEntry {
            id: uuid::Uuid::new_v4().to_string(),
            provider: m.provider,
            model: m.model,
            name: m.name,
            context_window: m.context_window,
            rpm: m.rpm,
            rpd: m.rpd,
            tpm: m.tpm,
            tpd: m.tpd,
            free_quota: m.free_quota,
            supports_tools: m.supports_tools,
            supports_streaming: m.supports_streaming,
            supports_vision: m.supports_vision,
            capabilities: m.capabilities,
            source_feed: Some(source_feed.to_string()),
            is_active: true,
            last_synced: Some(now.clone()),
            created_at: now.clone(),
            updated_at: now.clone(),
        })
        .collect();

    let count = entries.len();
    let pool_clone = pool.clone();
    let entries_clone = entries.clone();
    tokio::task::spawn_blocking(move || -> std::result::Result<(), String> {
        let conn = crate::db::core::get_conn(&pool_clone).map_err(|e| e.to_string())?;
        model_catalog::batch_upsert(&conn, &entries_clone).map_err(|e| e.to_string())?;
        log::info!("模型目录同步完成: {} 条", entries_clone.len());
        Ok(())
    })
    .await
    .map_err(|e| crate::error::AppError::Internal(format!("Sync task failed: {}", e)))?
    .map_err(|e| crate::error::AppError::Internal(e))?;

    Ok(count)
}

/// 导入内置种子目录（无条件 UPSERT，用于手动「同步」刷新快照）。
/// 返回导入的条目数。
pub async fn seed_builtin_catalog(pool: &crate::db::core::DbPool) -> Result<usize> {
    let feed: FeedResponse = serde_json::from_str(BUILTIN_SEED)
        .map_err(|e| crate::error::AppError::Internal(format!("内置目录数据解析失败: {}", e)))?;
    save_entries(pool, feed.models, "builtin").await
}

/// 表为空时导入内置种子目录（应用首次启动时调用，幂等）。
/// 返回实际导入的条目数；表非空时返回 0。
pub async fn seed_builtin_catalog_if_empty(pool: &crate::db::core::DbPool) -> Result<usize> {
    let pool_check = pool.clone();
    let empty = tokio::task::spawn_blocking(move || -> std::result::Result<bool, String> {
        let conn = crate::db::core::get_conn(&pool_check).map_err(|e| e.to_string())?;
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM model_catalog", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        Ok(n == 0)
    })
    .await
    .map_err(|e| crate::error::AppError::Internal(format!("Seed check failed: {}", e)))?
    .map_err(crate::error::AppError::Internal)?;

    if empty {
        let n = seed_builtin_catalog(pool).await?;
        log::info!("首次启动：已导入内置模型目录 {} 条", n);
        Ok(n)
    } else {
        Ok(0)
    }
}

/// 从已配置的活跃连接自动拉取 `/v1/models`（或 `/models`）端点，
/// 将返回的模型列表合并到 model_catalog 表。
///
/// 仅处理 OpenAI 兼容的连接（provider 非 google 且 base_url 存在）。
/// Anthropic 使用 `x-api-key` 头，其余使用 `Authorization: Bearer`。
/// 返回 `(成功连接数, 新增/更新模型总数, 各连接结果)`。
pub async fn refresh_from_connections(
    pool: &crate::db::core::DbPool,
    enc_key: &[u8],
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
            Some(v) => v.as_str().unwrap_or("").trim_end_matches('/').to_string(),
            None => continue,
        };
        if base_url.is_empty() {
            continue;
        }
        let api_key = match &conn.api_key {
            Some(k) if !k.is_empty() => k.clone(),
            _ => continue,
        };

        let models_url = if base_url.ends_with("/v1") {
            format!("{}/models", base_url)
        } else if base_url.ends_with("/v1/") {
            format!("{}/models", base_url.trim_end_matches('/'))
        } else {
            format!("{}/v1/models", base_url)
        };

        let provider = conn.provider.clone();
        let is_anthropic = provider == "anthropic";

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

        let resp = match req.send().await {
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
            #[serde(default)]
            owned_by: Option<String>,
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
            let model_provider = item.owned_by.unwrap_or_else(|| provider.clone());
            all_entries.push(CatalogEntry {
                id: uuid::Uuid::new_v4().to_string(),
                provider: model_provider,
                model: item.id,
                name: None,
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

/// 启动定时同步任务（每 12 小时）。必须在 Actix runtime 上下文中调用。
pub fn start_periodic_sync(
    pool: crate::db::core::DbPool,
    feed_url: String,
) {
    if feed_url.is_empty() {
        log::info!("模型目录 feed URL 未配置，仅使用内置目录数据");
        return;
    }
    actix_rt::spawn(async move {
        loop {
            // 启动后等待 5 分钟再首次同步
            tokio::time::sleep(std::time::Duration::from_secs(300)).await;
            match sync_catalog(&pool, &feed_url).await {
                Ok(count) => log::info!("定时模型目录同步完成: {} 条", count),
                Err(e) => log::warn!("定时模型目录同步失败: {}", e),
            }
            // 每 12 小时同步一次
            tokio::time::sleep(std::time::Duration::from_secs(43200)).await;
        }
    });
}
