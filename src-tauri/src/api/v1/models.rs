//! GET /v1/models 模型列表端点
//!
//! 该端点兼容 OpenAI `/v1/models` 接口，聚合**本地已配置连接**的模型列表。
//!
//! 设计要点：
//! - 模型来源为数据库中各活跃连接的 `models` 数组（为空时回退 `default_model`），
//!   不再实时请求上游：上游拉取串行耗时（单家 10s 超时）会让首次调用挂起 10~30s，
//!   且失败时只能产出无法路由的 `{provider}/models` 占位条目；
//! - 本地配置即路由真相：模型 ID 统一为 `{provider_id}/{raw_model_id}`，
//!   恰好命中注册表 `resolve_model_provider` 的前缀解析规则，选择后可直接路由；
//! - 响应恒为瞬时返回，前端模型选择框不再出现长时间空白/不可用。

use actix_web::{web, HttpResponse};
use crate::db::{core as db_core, providers as db_providers};
use crate::AppState;
use serde_json::json;
use std::collections::HashSet;
use std::sync::Arc;

/// 处理 GET /v1/models 请求，返回所有本地已配置模型的聚合列表。
///
/// 遍历提供商注册表中的每个提供商定义，读取其下所有**活跃**连接，
/// 将连接上配置的模型（`models` 数组，为空时回退 `default_model`）
/// 以 `{provider_id}/{model_id}` 格式合并为 OpenAI 格式响应，并按完整 ID 去重。
///
/// - `state`：应用全局状态
/// - 返回值：`{ "object": "list", "data": [...] }` 格式的 JSON 响应
pub async fn list_models(
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    // 获取数据库连接
    let conn = match db_core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    };

    let created = chrono::Utc::now().timestamp();
    let mut models = vec![]; // 聚合所有已配置模型
    let mut seen = HashSet::new(); // 按完整模型 ID 去重（多连接同模型只出现一次）

    // 遍历注册表中的每个提供商定义
    for def in state.provider_registry.list() {
        // 查询该提供商的连接列表（解密后的凭证在此场景无需使用）
        let connections = db_providers::list_by_provider(&conn, &def.id, &state.encryption_key).unwrap_or_default();

        // 仅聚合活跃连接上配置的模型
        for c in connections.iter().filter(|c| c.is_active) {
            // 连接配置的模型列表；为空时回退 default_model
            let ids: Vec<String> = match &c.models {
                Some(list) if !list.is_empty() => list.iter().map(|m| m.id.clone()).collect(),
                _ => c.default_model.clone().map(|m| vec![m]).unwrap_or_default(),
            };

            for id in ids {
                if id.is_empty() {
                    continue;
                }
                let full = format!("{}/{}", def.id, id);
                if !seen.insert(full.clone()) {
                    continue;
                }
                models.push(json!({
                    "id": full,
                    "object": "model",
                    "created": created,
                    "owned_by": def.id,
                    "permission": [],
                }));
            }
        }
    }

    // 返回 OpenAI 格式的模型列表
    HttpResponse::Ok().json(json!({
        "object": "list",
        "data": models,
    }))
}
