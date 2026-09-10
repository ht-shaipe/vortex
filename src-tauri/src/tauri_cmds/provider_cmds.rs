//! 提供商相关 Tauri 命令。
//!
//! 本模块向前端暴露以下命令：
//! - [`list_providers`]：列出所有已注册的提供商定义及其连接信息
//! - [`add_provider`]：新增一个提供商连接
//! - [`test_provider`]：测试指定提供商连接的可用性

use crate::db::{core as db_core, providers as db_providers};
use crate::db::models::CreateProviderRequest;
use crate::AppState;
use serde_json::json;
use std::sync::Arc;

/// 列出所有提供商定义及其连接信息。
///
/// 该命令会从数据库中读取全部连接记录，并结合内存中的提供商注册表，
/// 返回每个提供商的元数据（名称、图标、是否免费等）以及其下的连接数量。
/// 同时返回经过脱敏处理的连接列表（API Key 仅保留末尾 4 位）。
///
/// # 参数
/// - `state`：Tauri 管理的全局应用状态，包含数据库连接池、加密密钥和提供商注册表。
///
/// # 返回值
/// 返回一个 JSON 对象，包含两个字段：
/// - `providers`：提供商定义数组，每项含 `id`、`name`、`icon`、`color`、
///   `hasFree`、`noAuth` 和 `connections`（该提供商下的连接数量）。
/// - `connections`：脱敏后的连接数组，每项含 `id`、`provider`、`name`、
///   `isActive`、`apiKey`（脱敏后）、`testStatus`。
///
/// # 错误
/// 若获取数据库连接或查询连接列表失败，则返回对应的错误字符串。
#[tauri::command]
pub async fn list_providers(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    // 获取数据库连接
    let conn = db_core::get_conn(&state.db_pool).map_err(|e| e.to_string())?;
    // 查询所有连接记录（API Key 已解密）
    let connections = db_providers::list(&conn, &state.encryption_key).map_err(|e| e.to_string())?;
    // 从注册表中获取所有提供商定义
    let registry = state.provider_registry.list();
    let mut result = vec![];
    for def in registry {
        // 统计当前提供商下的连接数量
        let provider_connections: Vec<_> = connections.iter()
            .filter(|c| c.provider == def.id)
            .collect();
        result.push(json!({
            "id": def.id, "name": def.name, "icon": def.icon, "color": def.color,
            "hasFree": def.has_free, "noAuth": def.no_auth,
            "connections": provider_connections.len(),
        }));
    }

    // 对连接列表中的 API Key 进行脱敏处理：长度大于 8 时仅保留末尾 4 位，其余用 * 替换
    let masked_connections: Vec<_> = connections.iter().map(|c| {
        let masked_key = c.api_key.as_deref().map(|k| {
            if k.len() > 8 { format!("{}{}", "*".repeat(k.len()-4), &k[k.len()-4..]) } else { "*".repeat(k.len()) }
        });
        json!({
            "id": c.id, "provider": c.provider, "name": c.name, "isActive": c.is_active,
            "apiKey": masked_key, "testStatus": c.test_status,
        })
    }).collect();
    Ok(json!({"providers": result, "connections": masked_connections}))
}

/// 新增一个提供商连接。
///
/// 根据传入的提供商标识、连接名称和可选的 API Key，在数据库中创建一条新的连接记录。
/// 创建成功后返回脱敏后的连接信息。
///
/// # 参数
/// - `state`：Tauri 管理的全局应用状态。
/// - `provider`：提供商标识（对应注册表中的定义 ID）。
/// - `name`：连接的显示名称。
/// - `api_key`：可选的 API Key，若提供则会被加密存储。
///
/// # 返回值
/// 返回一个 JSON 对象，包含 `id`、`provider`、`name`、`apiKey`（脱敏后）和 `isActive`。
///
/// # 错误
/// 若获取数据库连接或创建连接记录失败，则返回对应的错误字符串。
#[tauri::command]
pub async fn add_provider(
    state: tauri::State<'_, Arc<AppState>>,
    provider: String,
    name: String,
    api_key: Option<String>,
) -> Result<serde_json::Value, String> {
    // 获取数据库连接
    let conn = db_core::get_conn(&state.db_pool).map_err(|e| e.to_string())?;
    // 构造创建请求，仅填充必要字段，其余字段使用默认值 None
    let req = CreateProviderRequest {
        provider,
        auth_type: None,
        name,
        email: None,
        api_key,
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
        custom_provider_id: None,
        provider_specific_data: None,
    };
    // 在数据库中创建连接记录（API Key 会被加密存储）
    let created = db_providers::create(&conn, &req, &state.encryption_key).map_err(|e| e.to_string())?;
    // 对返回的 API Key 进行脱敏处理
    let masked_key = created.api_key.as_deref().map(|k| {
        if k.len() > 8 { format!("{}{}", "*".repeat(k.len()-4), &k[k.len()-4..]) } else { "*".repeat(k.len()) }
    });
    Ok(json!({
        "id": created.id, "provider": created.provider, "name": created.name,
        "apiKey": masked_key, "isActive": created.is_active,
    }))
}

/// 测试指定提供商连接的可用性。
///
/// 根据连接 ID 从数据库中取出连接信息，结合提供商定义构造一个针对其模型列表
/// 端点的 HTTP GET 请求，根据响应状态码判断连接是否可用，并将测试结果回写到数据库。
///
/// # 参数
/// - `state`：Tauri 管理的全局应用状态。
/// - `id`：待测试的连接 ID。
///
/// # 返回值
/// 返回一个 JSON 对象：
/// - 成功时包含 `status`（"ok" 或 "error"）、`statusCode`（HTTP 状态码）和 `provider`（连接名称）。
/// - 请求失败时包含 `status`（"error"）、`error`（错误信息）和 `provider`。
/// - 若连接或提供商定义不存在，则返回相应的错误信息。
///
/// # 错误
/// 若连接 ID 对应的记录不存在，则返回 `"Provider not found"`。
#[tauri::command]
pub async fn test_provider(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Result<serde_json::Value, String> {
    // 获取数据库连接
    let conn = db_core::get_conn(&state.db_pool).map_err(|e| e.to_string())?;
    // 根据 ID 查询连接记录（API Key 已解密）
    let provider = db_providers::get_by_id(&conn, &id, &state.encryption_key).map_err(|e| e.to_string())?;
    match provider {
        Some(p) => {
            // 从注册表中获取对应的提供商定义
            let def = state.provider_registry.get(&p.provider);
            match def {
                Some(d) => {
                    // 优先使用连接中自定义的 baseUrl，否则回退到提供商定义中的默认值
                    let base_url = p.provider_specific_data.get("baseUrl")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&d.base_url);
                    // 拼接模型列表端点完整 URL
                    let url = format!("{}{}", base_url, d.models_path);

                    let client = &state.http_client;
                    // 构造 GET 请求，设置 10 秒超时
                    let mut req = client.get(&url).timeout(std::time::Duration::from_secs(10));

                    // 根据不同的 API 格式设置不同的认证方式
                    if d.api_format == "anthropic" {
                        // Anthropic 格式：使用 x-api-key 头并附带版本号
                        if let Some(ref api_key) = p.api_key {
                            req = req.header("x-api-key", api_key.as_str());
                        }
                        req = req.header("anthropic-version", "2023-06-01");
                    } else if d.api_format == "gemini" {
                        // Gemini 格式：通过 query 参数传递 API Key
                        if let Some(ref api_key) = p.api_key {
                            req = req.query(&[("key", api_key.as_str())]);
                        }
                    } else if let Some(ref api_key) = p.api_key {
                        // 默认格式：使用 Bearer Token 认证
                        req = req.bearer_auth(api_key.as_str());
                    }

                    // 发送请求并处理响应
                    match req.send().await {
                        Ok(response) => {
                            // 提取 HTTP 状态码，小于 400 视为成功
                            let status = response.status().as_u16();
                            let ok = status < 400;
                            let status_msg = if ok { None } else { Some(format!("HTTP {}", status)) };
                            // 将测试结果回写到数据库
                            let _ = db_providers::update_test_status(&conn, &id,
                                if ok { "ok" } else { "error" },
                                status_msg.as_deref()
                            );
                            Ok(json!({
                                "status": if ok { "ok" } else { "error" },
                                "statusCode": status,
                                "provider": p.name,
                            }))
                        }
                        Err(e) => {
                            // 请求发送失败，记录错误状态到数据库
                            let _ = db_providers::update_test_status(&conn, &id, "error", Some(&e.to_string()));
                            Ok(json!({
                                "status": "error",
                                "error": e.to_string(),
                                "provider": p.name,
                            }))
                        }
                    }
                }
                // 提供商定义不存在于注册表中
                None => Ok(json!({"status": "error", "error": "Provider definition not found"})),
            }
        }
        // 连接 ID 对应的记录不存在
        None => Err("Provider not found".into()),
    }
}
