//! 用量记录和统计端点
//!
//! 提供用量记录查询和统计聚合接口，供前端 UI 展示用量仪表盘。
//! 所有查询委托 [`db_usage`](crate::db::usage) 数据库层完成。
//! 同步 DB 操作通过 `spawn_blocking` 移至阻塞线程池，避免阻塞 actix async runtime。

use actix_web::{web, HttpResponse};
use crate::db::{core as db_core, usage as db_usage};
use crate::AppState;
use serde_json::json;
use std::sync::Arc;

/// 处理 GET 请求，获取用量记录列表（支持分页）。
pub async fn get_usage(
    state: web::Data<Arc<AppState>>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let query_map = query.into_inner();

    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let conn = db_core::get_conn(&pool).map_err(|e| e.to_string())?;
        if let (Some(p), Some(ps)) = (query_map.get("page"), query_map.get("pageSize")) {
            let page: i64 = p.parse().unwrap_or(1).max(1);
            let page_size: i64 = ps.parse().unwrap_or(20).clamp(1, 200);
            let total = db_usage::count(&conn).unwrap_or(0);
            db_usage::list_paginated(&conn, page, page_size)
                .map(|entries| json!({ "usage": entries, "total": total }))
                .map_err(|e| e.to_string())
        } else {
            let limit: i64 = query_map.get("limit").and_then(|v| v.parse().ok()).unwrap_or(100);
            db_usage::list_recent(&conn, limit)
                .map(|entries| json!({ "usage": entries }))
                .map_err(|e| e.to_string())
        }
    })
    .await;

    match result {
        Ok(Ok(value)) => HttpResponse::Ok().json(value),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

/// 处理 GET 请求，获取用量统计聚合数据。
pub async fn get_stats(
    state: web::Data<Arc<AppState>>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let query_map = query.into_inner();

    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let conn = db_core::get_conn(&pool).map_err(|e| e.to_string())?;
        let since = query_map.get("since").map(|s| s.as_str());
        db_usage::get_stats(&conn, since)
            .map(|stats| serde_json::to_value(stats).unwrap_or(json!({})))
            .map_err(|e| e.to_string())
    })
    .await;

    match result {
        Ok(Ok(value)) => HttpResponse::Ok().json(value),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}
