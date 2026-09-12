//! 用量记录和统计端点
//!
//! 提供用量记录查询和统计聚合接口，供前端 UI 展示用量仪表盘。
//! 所有查询委托 [`db_usage`](crate::db::usage) 数据库层完成。

use actix_web::{web, HttpResponse};
use crate::db::{core as db_core, usage as db_usage};
use crate::AppState;
use serde_json::json;
use std::sync::Arc;

/// 处理 GET 请求，获取用量记录列表（支持分页）。
///
/// 支持通过 `page` 和 `pageSize` 查询参数分页，也兼容旧的 `limit` 参数。
///
/// - `state`：应用全局状态
/// - `query`：URL 查询参数（支持 `page`、`pageSize` 或 `limit`）
/// - 返回值：`{ "usage": [...], "total": N }` 格式 JSON 响应
pub async fn get_usage(
    state: web::Data<Arc<AppState>>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> HttpResponse {
    let conn = match db_core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    };

    // 优先使用分页参数，兼容旧的 limit 参数
    if let (Some(p), Some(ps)) = (query.get("page"), query.get("pageSize")) {
        let page: i64 = p.parse().unwrap_or(1).max(1);
        let page_size: i64 = ps.parse().unwrap_or(20).clamp(1, 200);
        let total = db_usage::count(&conn).unwrap_or(0);
        match db_usage::list_paginated(&conn, page, page_size) {
            Ok(entries) => HttpResponse::Ok().json(json!({ "usage": entries, "total": total })),
            Err(e) => HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
        }
    } else {
        let limit: i64 = query.get("limit").and_then(|v| v.parse().ok()).unwrap_or(100);
        match db_usage::list_recent(&conn, limit) {
            Ok(entries) => HttpResponse::Ok().json(json!({ "usage": entries })),
            Err(e) => HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
        }
    }
}

/// 处理 GET 请求，获取用量统计聚合数据。
///
/// 支持通过 `since` 查询参数指定统计起始时间（RFC3339 格式），
/// 返回该时间点之后的汇总统计信息。
///
/// - `state`：应用全局状态
/// - `query`：URL 查询参数（支持 `since`）
/// - 返回值：统计聚合 JSON 响应
pub async fn get_stats(
    state: web::Data<Arc<AppState>>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> HttpResponse {
    // 获取数据库连接
    let conn = match db_core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    };

    // 解析 since 参数（可选的时间过滤起点）
    let since = query.get("since").map(|s| s.as_str());
    // 查询统计数据
    match db_usage::get_stats(&conn, since) {
        Ok(stats) => HttpResponse::Ok().json(stats),
        Err(e) => HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    }
}
