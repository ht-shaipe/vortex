//! 免费 Token 站点管理端点
//!
//! 提供免费 Token 站点的列表、创建、删除接口。支持远程同步模式：
//! 当配置了 `free_tokens_remote` 时，操作优先转发到远程服务，并在本地保留副本
//! 便于离线回退；远程不可用时自动降级为本地数据库操作。

use actix_web::{web, HttpResponse};
use std::collections::HashMap;
use std::sync::Arc;

use serde_json::json;

use crate::db::{core as db_core, free_tokens as db_free_tokens};
use crate::db::models::CreateFreeTokenSiteRequest;
use crate::remote_proxy;
use crate::AppState;

/// 从远程响应中兼容提取站点数组，支持多种信封：
/// - `{ sites: [...] }`
/// - `{ code, result: [...] }` 或 `{ code, result: { sites: [...] } }`（dsa 风格）
/// - 裸数组 `[...]`
///
/// - `value`：远程服务返回的 JSON 响应
/// - 返回值：提取到的站点 JSON 数组（无法识别时返回空数组）
fn extract_sites(value: &serde_json::Value) -> Vec<serde_json::Value> {
    // 标准信封：{ sites: [...] }
    if let Some(arr) = value.get("sites").and_then(|v| v.as_array()) {
        return arr.clone();
    }
    // dsa 风格信封：{ result: [...] } 或 { result: { sites: [...] } }
    if let Some(result) = value.get("result") {
        if let Some(arr) = result.as_array() {
            return arr.clone();
        }
        if let Some(arr) = result.get("sites").and_then(|v| v.as_array()) {
            return arr.clone();
        }
    }
    // 裸数组
    if let Some(arr) = value.as_array() {
        return arr.clone();
    }
    Vec::new() // 无法识别
}

/// 从远程创建响应中取出站点对象；取不到则用请求体构造一个本地兜底对象。
///
/// 尝试从 `site` / `data` 字段或响应本身提取站点对象，若均不可用则用请求体
/// 字段加上本地生成的 ID 构造一个完整对象，保证前端能立即展示。
///
/// - `value`：远程创建响应 JSON
/// - `req`：原始创建请求
/// - 返回值：站点 JSON 对象
fn site_from_remote_or_request(
    value: &serde_json::Value,
    req: &CreateFreeTokenSiteRequest,
) -> serde_json::Value {
    // 尝试从常见字段提取站点对象
    let candidate = value
        .get("site")
        .or_else(|| value.get("data"))
        .or_else(|| {
            // 整个响应本身就是站点对象（带 id 或 name）
            if value.is_object()
                && (value.get("id").is_some() || value.get("name").is_some())
            {
                Some(value)
            } else {
                None
            }
        });

    if let Some(obj) = candidate {
        if obj.is_object() {
            return obj.clone();
        }
    }

    // 远端未返回可用对象时，用请求体 + 本地生成 id 兜底，保证前端能立即展示
    json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "name": req.name.trim(),
        "homeUrl": req.home_url.clone().unwrap_or_default(),
        "applyUrl": req.apply_url.clone().unwrap_or_default(),
        "apiSupported": req.api_supported.unwrap_or(true),
        "apiBase": req.api_base,
        "apiFormat": req.api_format,
        "freeQuota": req.free_quota.clone().unwrap_or_default(),
        "region": req.region.clone().unwrap_or_else(|| "global".to_string()),
        "requiresCard": req.requires_card.unwrap_or(false),
        "requiresVerify": req.requires_verify.unwrap_or(false),
        "tags": req.tags.clone().unwrap_or(json!([])),
        "note": req.note,
        "providerId": req.provider_id,
        "source": "user",
        "submitter": req.submitter,
        "sortOrder": 9000,
        "createdAt": chrono::Utc::now().to_rfc3339(),
        "updatedAt": chrono::Utc::now().to_rfc3339(),
    })
}

/// 把提交额外落一份本地副本（best-effort），便于离线回退与本地删除。
///
/// - `pool`：数据库连接池
/// - `req`：创建站点的请求体
fn save_local_copy(pool: &crate::db::core::DbPool, req: &CreateFreeTokenSiteRequest) {
    if let Ok(conn) = db_core::get_conn(pool) {
        if let Err(e) = db_free_tokens::create(&conn, req) {
            log::warn!("[FREE-TOKENS] 本地副本写入失败(已忽略): {}", e);
        }
    }
}

/// 处理 GET 请求，列出免费 Token 站点。
///
/// 若配置了远程地址，优先从远程获取站点列表；远程失败时回退到本地数据库。
///
/// - `state`：应用全局状态
/// - 返回值：`{ "sites": [...] }` 格式 JSON 响应
pub async fn list_sites(state: web::Data<Arc<AppState>>) -> HttpResponse {
    let remote = &state.config.free_tokens_remote;
    // 远程模式：优先从远程获取
    if !remote.is_empty() {
        match remote_proxy::send_proxy_request(
            &state.http_client,
            remote,
            "GET",
            &HashMap::new(),
            None,
            30, // 30 秒超时
        )
        .await
        {
            Ok(value) => {
                // 远程成功：提取站点数组返回
                return HttpResponse::Ok().json(json!({ "sites": extract_sites(&value) }));
            }
            Err(e) => {
                // 远程失败：记录警告并回退本地
                log::warn!("[FREE-TOKENS] 远程列表获取失败，回退本地: {}", e);
            }
        }
    }

    // 本地模式或远程回退：从数据库查询
    let pool = state.db_pool.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let conn = db_core::get_conn(&pool).map_err(|e| e.to_string())?;
        db_free_tokens::list(&conn)
            .map(|sites| json!({ "sites": sites }))
            .map_err(|e| e.to_string())
    })
    .await;

    match result {
        Ok(Ok(v)) => HttpResponse::Ok().json(v),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

/// 处理 POST 请求，创建免费 Token 站点。
///
/// 校验站点名称非空后，若配置了远程地址则优先提交到远程服务并保存本地副本；
/// 远程失败时降级为本地保存。
///
/// - `state`：应用全局状态
/// - `body`：创建站点请求体
/// - 返回值：201 Created 含站点对象，或 400/500 错误
pub async fn create_site(
    state: web::Data<Arc<AppState>>,
    body: web::Json<CreateFreeTokenSiteRequest>,
) -> HttpResponse {
    // 校验站点名称非空
    if body.name.trim().is_empty() {
        return HttpResponse::BadRequest().json(json!({"error": "站点名称不能为空"}));
    }

    let remote = &state.config.free_tokens_remote;
    // 远程模式：优先提交到远程
    if !remote.is_empty() {
        // 序列化请求体为 JSON 字符串
        let payload = match serde_json::to_string(&body.0) {
            Ok(p) => p,
            Err(e) => return HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
        };
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());

        match remote_proxy::send_proxy_request(
            &state.http_client,
            remote,
            "POST",
            &headers,
            Some(&payload),
            30, // 30 秒超时
        )
        .await
        {
            Ok(value) => {
                // 远端创建成功：额外落一份本地副本，便于「我的推荐」展示与离线回退
                let pool = state.db_pool.clone();
                let req_clone = body.0.clone();
                let _ = tokio::task::spawn_blocking(move || -> Result<(), String> {
                    save_local_copy(&pool, &req_clone);
                    Ok(())
                })
                .await;
                return HttpResponse::Created().json(site_from_remote_or_request(&value, &body.0));
            }
            Err(e) => {
                // 远程失败：记录警告并降级为本地保存
                log::warn!("[FREE-TOKENS] 远程提交失败，改为本地保存: {}", e);
            }
        }
    }

    // 本地模式或远程降级：直接写入数据库
    let pool = state.db_pool.clone();
    let req = body.into_inner();
    let result = tokio::task::spawn_blocking(move || -> Result<crate::db::models::FreeTokenSite, String> {
        let conn = db_core::get_conn(&pool).map_err(|e| e.to_string())?;
        db_free_tokens::create(&conn, &req).map_err(|e| e.to_string())
    })
    .await;

    match result {
        Ok(Ok(site)) => HttpResponse::Created().json(site),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

/// 处理 DELETE 请求，删除免费 Token 站点。
///
/// 若配置了远程地址，先尝试远程删除（best-effort，失败不影响本地操作），
/// 再删除本地记录。本地无记录但远程已删除时仍视为成功。
///
/// - `state`：应用全局状态
/// - `path`：URL 路径中的站点 ID
/// - 返回值：`{ "success": true }`、404 未找到或 400 错误
pub async fn delete_site(
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    let id = path.into_inner(); // 提取路径参数中的 ID
    let remote = &state.config.free_tokens_remote;

    // 远端删除（best-effort，失败不影响本地结果）
    let mut remote_deleted = false;
    if !remote.is_empty() {
        let url = format!("{}/{}", remote.trim_end_matches('/'), id);
        if remote_proxy::send_proxy_request(&state.http_client, &url, "DELETE", &HashMap::new(), None, 30)
            .await
            .is_ok()
        {
            remote_deleted = true;
        }
    }

    // 删除本地记录
    let pool = state.db_pool.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<Result<bool, String>, String> {
        let conn = db_core::get_conn(&pool).map_err(|e| e.to_string())?;
        match db_free_tokens::delete_user_site(&conn, &id) {
            Ok(deleted) => Ok(Ok(deleted)),
            Err(e) => Ok(Err(e.to_string())),
        }
    })
    .await;

    match result {
        Ok(Ok(Ok(true))) => HttpResponse::Ok().json(json!({ "success": true })),
        Ok(Ok(Ok(false))) if remote_deleted => HttpResponse::Ok().json(json!({ "success": true })),
        Ok(Ok(Ok(false))) => HttpResponse::NotFound().json(json!({ "error": "Site not found" })),
        Ok(Ok(Err(_))) if remote_deleted => HttpResponse::Ok().json(json!({ "success": true })),
        Ok(Ok(Err(e))) => HttpResponse::BadRequest().json(json!({ "error": e })),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}
