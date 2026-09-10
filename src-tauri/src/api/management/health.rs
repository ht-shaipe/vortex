//! 健康检查端点
//!
//! 提供 GET 健康检查接口，探测数据库连通性并返回服务版本信息。
//! 供前端 UI 或外部监控系统判断服务可用性。

use actix_web::{web, HttpResponse};
use crate::AppState;
use serde_json::json;
use std::sync::Arc;

/// 处理 GET 请求，执行健康检查。
///
/// 通过执行 `SELECT 1` 探测数据库连通性，返回服务状态、版本号和数据库状态。
///
/// - `state`：应用全局状态
/// - 返回值：`{ "status": "ok"|"degraded", "version": "...", "database": "ok"|"error" }`
pub async fn health_check(
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    // 注意：rusqlite 的 `execute` 只接受不返回结果集的语句，
    // 对 `SELECT 1` 这类会返回行的语句必然报 ExecuteReturnedResults，
    // 必须用 `query_row` 才能真正探测连通性。
    let db_ok = match crate::db::core::get_conn(&state.db_pool) {
        Ok(conn) => conn
            .query_row("SELECT 1", [], |row| row.get::<_, i64>(0)) // 执行探测查询
            .is_ok(),
        Err(_) => false, // 获取连接失败
    };

    // 返回健康状态 JSON
    HttpResponse::Ok().json(json!({
        "status": if db_ok { "ok" } else { "degraded" }, // 整体状态
        "version": env!("CARGO_PKG_VERSION"), // 编译时版本号
        "database": if db_ok { "ok" } else { "error" }, // 数据库状态
    }))
}
