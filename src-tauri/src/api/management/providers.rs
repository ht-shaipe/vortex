//! 提供商连接管理端点
//!
//! 提供提供商连接的完整 CRUD 操作，以及连接测试和模型预览功能。
//! 所有返回给前端的连接对象均经 [`mask_connection`] 脱敏处理，隐藏 API 密钥明文。
//! 支持自定义 OpenAI 兼容端点，会根据提供商类型选择对应的鉴权方式和 URL 构造规则。

use actix_web::{web, HttpResponse};
use crate::db::{core as db_core, providers as db_providers};
use crate::db::models::CreateProviderRequest;
use crate::AppState;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

/// 对连接对象进行脱敏处理，生成前端可安全展示的 JSON。
///
/// 将 API 密钥脱敏为仅显示末尾 4 位的掩码字符串，并从 `provider_specific_data` 中
/// 提取 `baseUrl`、`apiProtocol`、`customId`、`models` 等扩展字段平铺到顶层，
/// 便于前端直接读取而不必理解嵌套 JSON 结构。
///
/// - `c`：数据库中的提供商连接对象
/// - 返回值：脱敏后的 JSON 值
fn mask_connection(c: &crate::db::models::ProviderConnection) -> serde_json::Value {
    // 密钥脱敏：长度 > 8 时保留末尾 4 位，其余用 * 填充；否则全部掩码
    let masked_key = c.api_key.as_deref().map(|k| {
        if k.len() > 8 { format!("{}{}", "*".repeat(k.len()-4), &k[k.len()-4..]) } else { "*".repeat(k.len()) }
    });
    // 是否配置了密钥。前端不能再靠掩码字符串是否为空来猜——掩码恒为非空，
    // 而真正未配置时 api_key 为 NULL，二者必须区分开。
    let has_api_key = c
        .api_key
        .as_deref()
        .map(|k| !k.trim().is_empty())
        .unwrap_or(false);
    // 是否配置了 OAuth access token（OAuth 连接没有 api_key，但有令牌）
    let has_access_token = c
        .access_token
        .as_deref()
        .map(|t| !t.trim().is_empty())
        .unwrap_or(false);
    // 自定义提供方扩展字段：从 provider_specific_data 提取 baseUrl / apiProtocol / customId，
    // 这样前端不必理解 JSON 结构直接读平铺字段。
    let empty_obj = serde_json::json!({});
    let obj = c.provider_specific_data.as_object().unwrap_or(empty_obj.as_object().unwrap());
    let base_url = obj.get("baseUrl").and_then(|v| v.as_str()).map(|s| s.to_string());
    let api_protocol = obj.get("apiProtocol").and_then(|v| v.as_str()).map(|s| s.to_string());
    let chat_path = obj.get("chatPath").and_then(|v| v.as_str()).map(|s| s.to_string());
    let custom_id = obj.get("customId").and_then(|v| v.as_str()).map(|s| s.to_string());
    let models_val = obj.get("models").cloned().unwrap_or(serde_json::Value::Null);

    json!({
        "id": c.id,
        "provider": c.provider,
        "authType": c.auth_type,
        "name": c.name,
        "email": c.email,
        "priority": c.priority,
        "isActive": c.is_active,
        "apiKey": masked_key,
        "hasApiKey": has_api_key,
        "hasAccessToken": has_access_token,
        "projectId": c.project_id,
        "testStatus": c.test_status,
        "lastTestedAt": c.last_tested,
        "lastLatencyMs": c.last_latency_ms,
        "errorCode": c.error_code,
        "lastError": c.last_error,
        "lastErrorAt": c.last_error_at,
        "backoffLevel": c.backoff_level,
        "rateLimitedUntil": c.rate_limited_until,
        "healthCheckInterval": c.health_check_interval,
        "consecutiveUseCount": c.consecutive_use_count,
        "rateLimitProtection": c.rate_limit_protection,
        "groupName": c.group_name,
        "maxConcurrent": c.max_concurrent,
        "proxyEnabled": c.proxy_enabled,
        "displayName": c.display_name,
        "defaultModel": c.default_model,
        "models": models_val,
        "baseUrl": base_url,
        "apiProtocol": api_protocol,
        "chatPath": chat_path,
        "customProviderId": custom_id,
        "providerSpecificData": c.provider_specific_data,
        "createdAt": c.created_at,
        "updatedAt": c.updated_at,
    })
}

/// 处理 GET 请求，列出所有提供商定义及其连接。
///
/// 遍历提供商注册表，为每个提供商汇总连接数和活跃连接数，
/// 同时返回所有连接的脱敏列表。
///
/// - `state`：应用全局状态
/// - 返回值：`{ "providers": [...], "connections": [...] }` 格式 JSON
pub async fn list_providers(
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let encryption_key = state.encryption_key.clone();
    let registry: Vec<_> = state.provider_registry.list().into_iter().cloned().collect();

    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let conn = db_core::get_conn(&pool).map_err(|e| e.to_string())?;
        let connections = db_providers::list(&conn, &encryption_key).unwrap_or_default();

        let mut result = vec![];
        for def in &registry {
            let provider_connections: Vec<_> = connections.iter()
                .filter(|c| c.provider == def.id)
                .collect();
            result.push(json!({
                "id": def.id,
                "name": def.name,
                "icon": def.icon,
                "color": def.color,
                "hasFree": def.has_free,
                "noAuth": def.no_auth,
                "authHint": def.auth_hint,
                "serviceKinds": def.service_kinds,
                "connections": provider_connections.len(),
                "activeConnections": provider_connections.iter().filter(|c| c.is_active).count(),
            }));
        }

        let masked_connections: Vec<_> = connections.iter().map(|c| mask_connection(c)).collect();
        Ok(json!({"providers": result, "connections": masked_connections}))
    })
    .await;

    match result {
        Ok(Ok(v)) => HttpResponse::Ok().json(v),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

/// 处理 POST 请求，创建新的提供商连接。
pub async fn create_provider(
    state: web::Data<Arc<AppState>>,
    body: web::Json<CreateProviderRequest>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let encryption_key = state.encryption_key.clone();
    let req = body.into_inner();

    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let conn = db_core::get_conn(&pool).map_err(|e| e.to_string())?;
        db_providers::create(&conn, &req, &encryption_key)
            .map(|p| mask_connection(&p))
            .map_err(|e| e.to_string())
    })
    .await;

    match result {
        Ok(Ok(v)) => HttpResponse::Created().json(v),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

/// 处理 GET 请求，按 ID 获取单个提供商连接。
pub async fn get_provider(
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let encryption_key = state.encryption_key.clone();
    let id = path.into_inner();

    let result = tokio::task::spawn_blocking(move || -> Result<Option<serde_json::Value>, String> {
        let conn = db_core::get_conn(&pool).map_err(|e| e.to_string())?;
        db_providers::get_by_id(&conn, &id, &encryption_key)
            .map(|opt| opt.map(|p| mask_connection(&p)))
            .map_err(|e| e.to_string())
    })
    .await;

    match result {
        Ok(Ok(Some(v))) => HttpResponse::Ok().json(v),
        Ok(Ok(None)) => HttpResponse::NotFound().json(json!({ "error": "Provider not found" })),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

/// 处理 PUT/PATCH 请求，按 ID 更新提供商连接。
pub async fn update_provider(
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
    body: web::Json<serde_json::Value>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let encryption_key = state.encryption_key.clone();
    let id = path.into_inner();
    let body = body.into_inner();

    let result = tokio::task::spawn_blocking(move || -> Result<Option<serde_json::Value>, String> {
        let conn = db_core::get_conn(&pool).map_err(|e| e.to_string())?;
        db_providers::update(&conn, &id, &body, &encryption_key)
            .map(|opt| opt.map(|p| mask_connection(&p)))
            .map_err(|e| e.to_string())
    })
    .await;

    match result {
        Ok(Ok(Some(v))) => HttpResponse::Ok().json(v),
        Ok(Ok(None)) => HttpResponse::NotFound().json(json!({ "error": "Provider not found" })),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

/// 处理 DELETE 请求，按 ID 删除提供商连接。
pub async fn delete_provider(
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let id = path.into_inner();

    let result = tokio::task::spawn_blocking(move || -> Result<bool, String> {
        let conn = db_core::get_conn(&pool).map_err(|e| e.to_string())?;
        db_providers::delete(&conn, &id).map_err(|e| e.to_string())
    })
    .await;

    match result {
        Ok(Ok(true)) => HttpResponse::Ok().json(json!({ "success": true })),
        Ok(Ok(false)) => HttpResponse::NotFound().json(json!({ "error": "Provider not found" })),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

/// 处理 GET 请求，按 ID 获取提供商连接的解密后真实 API 密钥。
pub async fn get_api_key(
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let encryption_key = state.encryption_key.clone();
    let id = path.into_inner();

    let result = tokio::task::spawn_blocking(move || -> Result<Result<String, String>, String> {
        let conn = db_core::get_conn(&pool).map_err(|e| e.to_string())?;
        match db_providers::get_by_id(&conn, &id, &encryption_key) {
            Ok(Some(p)) => {
                let api_key = p.api_key.unwrap_or_default();
                if api_key.is_empty() {
                    Ok(Err("该连接未配置 API 密钥".to_string()))
                } else {
                    Ok(Ok(api_key))
                }
            }
            Ok(None) => Ok(Err("Provider not found".to_string())),
            Err(e) => Err(e.to_string()),
        }
    })
    .await;

    match result {
        Ok(Ok(Ok(key))) => HttpResponse::Ok().json(json!({ "apiKey": key })),
        Ok(Ok(Err(msg))) => HttpResponse::NotFound().json(json!({ "error": msg })),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

/// 处理 POST 请求，测试指定提供商连接的可达性。
pub async fn test_provider(
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let encryption_key = state.encryption_key.clone();
    let id = path.into_inner();
    let id_for_db = id.clone();

    // 1. DB 读取
    let provider_result = tokio::task::spawn_blocking(move || -> Result<Option<crate::db::models::ProviderConnection>, String> {
        let conn = db_core::get_conn(&pool).map_err(|e| e.to_string())?;
        db_providers::get_by_id(&conn, &id_for_db, &encryption_key).map_err(|e| e.to_string())
    })
    .await;

    let provider = match provider_result {
        Ok(Ok(Some(p))) => p,
        Ok(Ok(None)) => return HttpResponse::NotFound().json(json!({ "error": "Provider not found" })),
        Ok(Err(e)) => return HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => return HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    };

    // 2. HTTP 连接测试（async）
    let def = state.provider_registry.get(&provider.provider);
    let upstream_client = crate::create_upstream_client(&state.upstream_ssl_connector);
    let test_result = test_connection(&provider, def, &upstream_client).await;

    // 3. DB 写入测试结果
    let pool2 = state.db_pool.clone();
    let status = test_result.status.clone();
    let error = test_result.error.clone();
    let latency = test_result.latency_ms.map(|v| v as i64);
    let id2 = id.clone();
    let _ = tokio::task::spawn_blocking(move || -> Result<(), String> {
        let conn = db_core::get_conn(&pool2).map_err(|e| e.to_string())?;
        db_providers::update_test_status(&conn, &id2, &status, error.as_deref(), latency)
            .map_err(|e| e.to_string())
    })
    .await;

    HttpResponse::Ok().json(json!({
        "status": test_result.status,
        "error": test_result.error,
        "latencyMs": test_result.latency_ms,
    }))
}

/// 连接测试结果
struct TestResult {
    /// 测试状态：`"ok"` 或 `"error"`
    status: String,
    /// 错误描述（测试失败时存在）
    error: Option<String>,
    /// 响应延迟（毫秒）
    latency_ms: Option<u64>,
}

/// 执行实际的连接测试，向提供商模型列表端点发起请求。
///
/// 根据提供商类型构造模型列表 URL 并选择鉴权方式，发起 GET 请求探测可达性。
/// 对自定义 OpenAI 兼容端点做特殊 URL 拼接处理，并防御性地拦截示例占位域名。
///
/// - `connection`：要测试的连接对象
/// - `provider_def`：对应的提供商定义（若为未知提供商则为 None）
/// - 返回值：测试结果 [`TestResult`]
async fn test_connection(
    connection: &crate::db::models::ProviderConnection,
    provider_def: Option<&crate::providers::types::ProviderDef>,
    client: &awc::Client,
) -> TestResult {
    // 获取提供商定义，未知则返回错误
    let def = match provider_def {
        Some(d) => d,
        None => return TestResult {
            status: "error".into(),
            error: Some(format!("未知提供方 `{}`，无法测试连接", connection.provider)),
            latency_ms: None,
        },
    };

    // 需要鉴权却没有任何凭据时直接短路。
    // 否则会发出一个无鉴权请求，上游回 401/404，前端显示成「地址不通」，
    // 把真正的原因（没填密钥）掩盖掉。
    let has_credential = connection
        .api_key
        .as_deref()
        .map(|k| !k.trim().is_empty())
        .unwrap_or(false)
        || connection
            .access_token
            .as_deref()
            .map(|t| !t.trim().is_empty())
            .unwrap_or(false);
    if !def.no_auth && !has_credential {
        return TestResult {
            status: "error".into(),
            error: Some("该连接未配置 API 密钥，无法测试。请在「认证凭据」中填写密钥后重试".into()),
            latency_ms: None,
        };
    }

    // 确定基础 URL：优先使用连接中的自定义 baseUrl
    let base_url = connection
        .provider_specific_data
        .get("baseUrl")
        .and_then(|v| v.as_str())
        .unwrap_or(&def.base_url);

    // 与 preview_models 保持同一构造规则——尤其是 custom-openai（用户填的 baseUrl
    // 通常已含 /v1 后缀），拼接 `base + def.models_path` 会得到 `/v1/v1/models`，
    // 必然 404。这里按 provider 区分：
    //   - custom-openai：直接拼 `{base}/models`（已去掉尾斜杠）
    //   - 其余内置：沿用 `{base}{def.models_path}`（保留向后兼容）
    let url = if def.id == "custom-openai" {
        let base_trimmed = base_url.trim_end_matches('/');
        // 检测占位符
        if base_trimmed.contains("{account_id}") {
            return TestResult {
                status: "error".into(),
                error: Some("该提供方需在「自定义设置」中填写完整 API 地址（替换 {account_id} 占位符）".into()),
                latency_ms: None,
            };
        }
        format!("{}/models", base_trimmed)
    } else {
        format!("{}{}", base_url, def.models_path)
    };

    // 防御性：地址里仍残留占位/示例域名（仅防御旧数据）时短路。
    if let Ok(parsed) = reqwest::Url::parse(&url) {
        let host = parsed.host_str().unwrap_or("");
        let placeholder = host.eq_ignore_ascii_case("your-api-endpoint.com")
            || host.eq_ignore_ascii_case("example.com")
            || host.ends_with(".example.com");
        if placeholder {
            return TestResult {
                status: "error".into(),
                error: Some(format!("API 地址为保留示例域名（{}），请填写真实可访问的端点", host)),
                latency_ms: None,
            };
        }
    }

    // 构造 GET 请求
    let mut req = client.get(&url);

    // 根据提供商类型设置鉴权
    if let Some(ref api_key) = connection.api_key {
        if !api_key.trim().is_empty() {
            if def.id == "anthropic" {
                req = req.insert_header(("x-api-key", api_key.as_str()));
                req = req.insert_header(("anthropic-version", "2023-06-01"));
            } else if def.id == "gemini" {
                req = client.get(&format!("{}?key={}", url, api_key));
            } else {
                req = req.insert_header(("Authorization", format!("Bearer {}", api_key)));
            }
        }
    }

    let start = std::time::Instant::now();

    // 发送请求并处理响应（15 秒超时）
    match tokio::time::timeout(std::time::Duration::from_secs(15), req.send()).await {
        Ok(Ok(mut response)) => {
            let latency = start.elapsed().as_millis() as u64;
            let status_code = response.status();
            if status_code.is_success() {
                // 成功
                TestResult { status: "ok".into(), error: None, latency_ms: Some(latency) }
            } else {
                // HTTP 错误：截取响应体前 280 字符作为线索
                let bytes = response.body().await.unwrap_or_default();
                let body = String::from_utf8_lossy(&bytes)
                    .chars()
                    .take(280)
                    .collect::<String>();
                let snippet = if body.is_empty() {
                    String::new()
                } else {
                    format!(" · {}", body)
                };
                TestResult {
                    status: "error".into(),
                    error: Some(format!("远程返回 HTTP {}{}", status_code.as_u16(), snippet)),
                    latency_ms: Some(latency),
                }
            }
        }
        Ok(Err(e)) => {
            let msg = format!("无法连接到该地址：{}", e);
            TestResult { status: "error".into(), error: Some(msg), latency_ms: None }
        }
        Err(_) => {
            TestResult {
                status: "error".into(),
                error: Some("请求超过 15 秒未响应，请检查网络与地址可达性".into()),
                latency_ms: None,
            }
        }
    }
}

/// 未保存连接即可预览提供方可用的模型列表。
///
/// 入参直接来自「添加提供方」表单里的临时值（provider / apiKey / baseUrl），
/// 不经过数据库，便于用户在保存前确认密钥与地址是否正确、并直接挑选默认模型。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewModelsRequest {
    /// 提供商标识（如 openai / anthropic / custom-openai）
    pub provider: String,
    /// API 密钥（可选，免鉴权提供商可不填）
    #[serde(default)]
    pub api_key: Option<String>,
    /// 自定义基础 URL（可选，覆盖提供商默认值）
    #[serde(default)]
    pub base_url: Option<String>,
    /// 自定义提供方用，指定实际协议以决定鉴权方式（openai/anthropic/gemini 等）
    #[serde(default)]
    pub api_protocol: Option<String>,
}

/// 处理 POST 请求，预览指定提供商配置下可用的模型列表。
///
/// 不需要先保存连接，直接使用表单临时值向提供商模型列表端点发起请求。
/// 会校验 URL 合法性与占位域名，根据 `api_protocol` 或提供商 ID 选择鉴权方式。
/// 成功后返回模型 ID 列表；若解析不到模型则返回警告提示。
///
/// - `state`：应用全局状态
/// - `body`：预览请求体（含 provider / apiKey / baseUrl / apiProtocol）
/// - 返回值：`{ "models": [...] }` 或含 `error` / `warning` 的 JSON
pub async fn preview_models(
    state: web::Data<Arc<AppState>>,
    body: web::Json<PreviewModelsRequest>,
) -> HttpResponse {
    // 获取提供商定义
    let def = match state.provider_registry.get(&body.provider) {
        Some(d) => d,
        None => return HttpResponse::BadRequest().json(json!({"error": "未知的提供方"})),
    };

    // 确定基础 URL：优先使用传入的自定义值
    let provided_base = body.base_url.as_deref().filter(|s| !s.trim().is_empty());
    let base = provided_base.unwrap_or(&def.base_url);

    // 构造 models 接口地址
    let models_url = if body.provider == "custom-openai" && provided_base.is_some() {
        format!("{}/models", base.trim_end_matches('/'))
    } else if base.contains("{account_id}") {
        return HttpResponse::BadRequest().json(json!({
            "error": "该提供方需在「自定义设置」中填写完整 API 地址（替换 {account_id} 占位符）"
        }));
    } else {
        format!("{}{}", base, def.models_path)
    };

    // 校验目标地址
    let parsed = match reqwest::Url::parse(&models_url) {
        Ok(u) => u,
        Err(_) => return HttpResponse::BadRequest().json(json!({
            "error": "API 地址格式不正确，需以 http:// 或 https:// 开头"
        })),
    };
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return HttpResponse::BadRequest().json(json!({
            "error": "仅支持 http/https 协议的 API 地址"
        }));
    }
    match parsed.host_str() {
        Some(host) => {
            let h = host.to_lowercase();
            let is_placeholder = h == "your-api-endpoint.com"
                || h.ends_with(".your-api-endpoint.com")
                || h == "example.com"
                || h.ends_with(".example.com");
            if is_placeholder {
                return HttpResponse::BadRequest().json(json!({
                    "error": format!("「{}」是占位域名，不可解析。请在「自定义设置」中填写你实际可用的 API 端点地址", h)
                }));
            }
        }
        None => return HttpResponse::BadRequest().json(json!({ "error": "API 地址缺少主机名" })),
    }

    // 鉴权方式
    let auth_kind: &str = match body.api_protocol.as_deref() {
        Some("anthropic") => "x-api-key",
        Some("gemini") => "query",
        Some("openai") | Some("cohere") | Some("cloudflare") | Some("openai-compatible") => "bearer",
        _ => match def.id.as_str() {
            "anthropic" => "x-api-key",
            "gemini" => "query",
            _ => "bearer",
        },
    };

    // 使用上游链路客户端（awc + openssl）
    let client = crate::create_upstream_client(&state.upstream_ssl_connector);
    let mut req = client.get(&models_url);
    if let Some(ref key) = body.api_key {
        if !key.trim().is_empty() {
            match auth_kind {
                "x-api-key" => {
                    req = req.insert_header(("x-api-key", key.as_str()));
                    req = req.insert_header(("anthropic-version", "2023-06-01"));
                }
                "query" => {
                    req = client.get(&format!("{}?key={}", models_url, key));
                }
                _ => {
                    req = req.insert_header(("Authorization", format!("Bearer {}", key)));
                }
            }
        }
    }

    // 发送请求并处理响应（15 秒超时）
    match tokio::time::timeout(std::time::Duration::from_secs(15), req.send()).await {
        Ok(Ok(mut resp)) => {
            let status = resp.status();
            if !status.is_success() {
                let bytes = resp.body().await.unwrap_or_default();
                let detail = String::from_utf8_lossy(&bytes).into_owned();
                let upstream = extract_upstream_error(&detail);
                let hint = match status.as_u16() {
                    401 => "API 密钥无效或已过期，请检查密钥是否正确".to_string(),
                    403 => "密钥权限不足，可能未开通模型访问权限".to_string(),
                    429 => match upstream.as_deref() {
                        Some(u) => format!("请求被上游限流（{}），请稍后重试", u),
                        None => "请求过于频繁被限流，请稍后重试（部分平台需等待 1 分钟）".to_string(),
                    },
                    500..=599 => format!("上游服务异常（HTTP {}），请稍后重试", status.as_u16()),
                    _ => format!("远程返回 HTTP {}", status.as_u16()),
                };
                return HttpResponse::BadGateway().json(json!({
                    "error": hint,
                    "detail": detail.chars().take(600).collect::<String>(),
                }));
            }
            let bytes = match resp.body().await {
                Ok(b) => b,
                Err(e) => return HttpResponse::BadGateway().json(json!({
                    "error": format!("响应体读取失败：{}", e),
                })),
            };
            match serde_json::from_slice::<serde_json::Value>(&bytes) {
                Ok(v) => {
                    let models = extract_model_ids(&v);
                    if models.is_empty() {
                        return HttpResponse::Ok().json(json!({
                            "models": [],
                            "warning": "已成功连接，但未能从响应中解析出模型列表（可手动填写模型 ID）",
                        }));
                    }
                    HttpResponse::Ok().json(json!({ "models": models }))
                }
                Err(e) => HttpResponse::BadGateway().json(json!({
                    "error": format!("响应 JSON 解析失败：{}", e),
                })),
            }
        }
        Ok(Err(e)) => HttpResponse::BadGateway().json(json!({
            "error": format!("无法连接到该地址，请检查 API 地址是否正确、网络是否可访问该服务（底层错误：{}）", e)
        })),
        Err(_) => HttpResponse::BadGateway().json(json!({
            "error": "请求超过 15 秒未响应，请检查网络与地址可达性"
        })),
    }
}

/// 从上游错误响应体中提取真实错误信息，用于补充友好提示。
///
/// 支持两种常见格式：
/// - OpenAI 风格：`{ "error": { "message": "...", "code": "..." } }`
/// - AMD / FastAPI 风格：`{ "detail": { "error": { "message": "...", "code": "..." } } }`
///   或 `{ "detail": "纯文本错误" }`
///
/// - `body`：上游返回的响应体文本
/// - 返回值：提取出的 `message [code]` 形式描述，无法解析时返回 None
fn extract_upstream_error(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    // 优先取 error 对象（OpenAI 风格），其次 detail.error（AMD/FastAPI 风格）
    let err = v
        .get("error")
        .cloned()
        .or_else(|| v.get("detail").and_then(|d| d.get("error")).cloned());
    let mut parts: Vec<String> = Vec::new();
    if let Some(err) = err {
        if let Some(m) = err.get("message").and_then(|x| x.as_str()) {
            if !m.is_empty() {
                parts.push(m.to_string());
            }
        }
        if let Some(c) = err.get("code").and_then(|x| x.as_str()) {
            if !c.is_empty() {
                parts.push(format!("[{}]", c));
            }
        }
    } else if let Some(d) = v.get("detail").and_then(|x| x.as_str()) {
        // detail 为纯文本的情况（如 FastAPI 默认错误）
        if !d.is_empty() {
            parts.push(d.to_string());
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

/// 从多种厂商响应形态中提取模型 ID：
/// - OpenAI / Anthropic: `{ data: [ { id } ] }`
/// - Gemini / Perplexity: `{ models: [ { name | id } ] }`（Gemini 的 `models/` 前缀会被去除）
/// - Cloudflare: `{ result: [ { id | name } ] }
///
/// - `v`：提供商返回的 JSON 响应
/// - 返回值：去重后的模型 ID 列表
fn extract_model_ids(v: &serde_json::Value) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    // 去重添加闭包
    let mut add = |raw: String| {
        let t = raw.trim().to_string();
        if !t.is_empty() && seen.insert(t.clone()) {
            ids.push(t);
        }
    };

    // OpenAI / Anthropic 风格：data 数组中的 id 字段
    if let Some(arr) = v.get("data").and_then(|x| x.as_array()) {
        for m in arr {
            if let Some(id) = m.get("id").and_then(|x| x.as_str()) {
                add(id.to_string());
            }
        }
    }
    // Gemini / Perplexity 风格：models 数组中的 name 或 id 字段
    if let Some(arr) = v.get("models").and_then(|x| x.as_array()) {
        for m in arr {
            let raw = m.get("name").and_then(|x| x.as_str())
                .or_else(|| m.get("id").and_then(|x| x.as_str()));
            if let Some(name) = raw {
                // 去掉 Gemini 的 models/ 前缀
                add(name.strip_prefix("models/").unwrap_or(name).to_string());
            }
        }
    }
    // Cloudflare 风格：result 数组中的 id 或 name 字段
    if let Some(arr) = v.get("result").and_then(|x| x.as_array()) {
        for m in arr {
            let raw = m.get("id").and_then(|x| x.as_str())
                .or_else(|| m.get("name").and_then(|x| x.as_str()));
            if let Some(name) = raw {
                add(name.to_string());
            }
        }
    }
    ids
}
