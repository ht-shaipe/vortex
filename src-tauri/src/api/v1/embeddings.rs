//! POST /v1/embeddings 文本嵌入端点
//!
//! 该端点兼容 OpenAI `/v1/embeddings` 接口，将嵌入请求路由到对应提供商并转发响应。
//! 请求中的模型名经 [`resolve_model_provider`](crate::providers::registry) 解析后，
//! 替换为提供商侧的真实模型 ID 再转发。响应成功后记录用量条目。

use actix_web::{web, HttpResponse};
use crate::db::{core as db_core, providers as db_providers};
use crate::db::models::UsageEntry;
use crate::AppState;
use serde_json::json;
use std::sync::Arc;
use std::time::Instant;

use crate::db::usage as db_usage;

/// 处理 POST /v1/embeddings 请求，创建文本嵌入。
///
/// 解析请求中的 `model` 字段，通过注册表解析出对应提供商和真实模型名，
/// 选取该提供商的一个活跃连接，将请求转发到提供商的嵌入端点。
/// 成功后记录用量条目（嵌入不统计 token，仅记录延迟），原样返回提供商响应。
///
/// - `state`：应用全局状态
/// - `body`：OpenAI 格式嵌入请求体
/// - 返回值：提供商响应（原样透传）或错误 JSON
pub async fn create_embeddings(
    state: web::Data<Arc<AppState>>,
    body: web::Json<serde_json::Value>,
) -> HttpResponse {
    // 提取模型名，默认 "unknown"
    let model = body.get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let start = Instant::now(); // 记录请求开始时间，用于计算延迟
    // 获取数据库连接
    let conn = match db_core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    };

    // 若配置要求 API 密钥，保留连接引用供后续鉴权逻辑使用
    if state.config.require_api_key {
        let _ = &conn;
    }

    // 解析模型对应的提供商和真实模型名
    let (provider_id, resolved_model) = if let Some((pid, _def, m)) = state.provider_registry.resolve_model_provider(model) {
        (pid, m)
    } else {
        // 模型未找到
        return HttpResponse::NotFound().json(json!({
            "error": {"message": format!("Model '{}' not found", model), "type": "invalid_request_error"}
        }));
    };

    // 获取提供商定义
    let def = match state.provider_registry.get(&provider_id) {
        Some(d) => d,
        None => return HttpResponse::NotFound().json(json!({"error": "Provider not found"})),
    };

    // 查询该提供商的连接列表
    let connections = match db_providers::list_by_provider(&conn, &provider_id, &state.encryption_key) {
        Ok(c) => c,
        Err(_) => vec![],
    };

    // 选取第一个连接
    let connection = match connections.into_iter().next() {
        Some(c) => c,
        None => {
            // 无可用连接
            if !def.no_auth {
                return HttpResponse::NotFound().json(json!({"error": "No active connection for this provider"}));
            }
            return HttpResponse::ServiceUnavailable().json(json!({"error": "No connection available"}));
        }
    };

    let client = crate::create_upstream_client(&state.upstream_ssl_connector);
    // 确定基础 URL：优先使用连接中的自定义 baseUrl
    let base_url = connection.provider_specific_data.get("baseUrl")
        .and_then(|v| v.as_str())
        .unwrap_or(&def.base_url);

    // 拼接嵌入端点 URL
    let url = format!("{}/v1/embeddings", base_url);

    // 替换请求体中的模型名为提供商侧真实模型名
    let mut req_body = body.into_inner();
    if let Some(obj) = req_body.as_object_mut() {
        obj.insert("model".to_string(), json!(resolved_model));
    }

    // 构造 POST 请求并添加鉴权
    let mut req = client.post(&url);
    if let Some(ref api_key) = connection.api_key {
        req = req.insert_header(("Authorization", format!("Bearer {}", api_key)));
    }

    // 发送请求并处理响应
    match req.send_json(&req_body).await {
        Ok(mut response) => {
            let status = response.status();
            let body_bytes = response.body().await.unwrap_or_default();

            // 非 2xx：原样返回错误响应
            if !status.is_success() {
                return HttpResponse::build(actix_web::http::StatusCode::from_u16(status.as_u16()).unwrap_or(actix_web::http::StatusCode::BAD_GATEWAY))
                    .body(body_bytes.to_vec());
            }

            // 计算延迟并记录用量
            let latency = start.elapsed().as_millis() as i64;
            let entry = UsageEntry {
                id: 0, provider: Some(connection.name.clone()), model: Some(resolved_model),
                connection_id: Some(connection.id.clone()), api_key_id: None, api_key_name: None,
                tokens_input: 0, tokens_output: 0, tokens_cache_read: 0, // 嵌入不统计 token
                tokens_cache_creation: 0, tokens_reasoning: 0,
                service_tier: "standard".to_string(), status: "success".to_string(),
                success: true, error_code: None, latency_ms: Some(latency), ttft_ms: None,
                cost: 0.0, timestamp: chrono::Utc::now().to_rfc3339(),
            };
            // best-effort 写入用量记录，失败不影响响应
            let _ = db_core::get_conn(&state.db_pool).ok().and_then(|c| db_usage::record(&c, &entry).ok());

            // 原样返回提供商响应
            HttpResponse::Ok()
                .content_type("application/json")
                .body(body_bytes.to_vec())
        }
        Err(e) => {
            // 请求发送失败
            HttpResponse::BadGateway().json(json!({"error": format!("Provider request failed: {}", e)}))
        }
    }
}
