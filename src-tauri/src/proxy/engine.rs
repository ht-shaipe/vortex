//! 代理引擎核心
//!
//! 负责入站请求的完整处理流程：API Key 鉴权 → 路由解析 → 上游执行（含重试与熔断）
//! → 用量记录。流式响应以归一化 SSE 流返回，由入站协议层决定包装方式。

use crate::db::models::{ProxyRequest, UsageEntry};
use crate::db::{core as db_core, model_aliases as db_aliases, providers as db_providers, rate_limits as db_rate_limits, routing_profiles as db_routing_profiles, settings as db_settings, usage as db_usage};
use crate::error::{AppError, Result};
use crate::providers::ProviderRegistry;
use crate::proxy::executor::{ExecutorFactory, ExecutorOutput};
use crate::proxy::retry::RetryPolicy;
use crate::proxy::sse::{self, SseStream, StreamUsage, UsageCallback, UsageExtract};
use crate::proxy::tool_call_rescue;
use crate::AppState;
use std::sync::Arc;
use std::time::Instant;

/// 引擎处理结果。
/// 流式响应以归一化 SSE 流返回，由入站协议层（OpenAI/Anthropic）决定如何包装。
pub enum ProxyOutput {
    /// 非流式响应（OpenAI 格式 JSON）
    Response(serde_json::Value),
    /// 流式响应（归一化为 OpenAI chunk 格式的 SSE 流）
    Stream(SseStream),
}

/// 代理引擎
///
/// 核心代理逻辑入口，持有执行器工厂与重试策略，
/// 负责将请求路由到上游提供商并处理重试、熔断与用量记录。
/// 实现 Clone 以便 Tauri IPC 命令把引擎克隆出读锁后异步执行
///（parking_lot 读锁守卫非 Send，不能跨 await 持有）。
#[derive(Clone)]
pub struct ProxyEngine {
    /// 执行器工厂，按 API 格式创建对应执行器
    executor_factory: ExecutorFactory,
    /// 重试策略（最大 2 次重试，基础延迟 500ms）
    retry_policy: RetryPolicy,
}

impl ProxyEngine {
    /// 创建代理引擎
    ///
    /// 默认配置：最大重试 2 次，基础延迟 500 毫秒。
    pub fn new() -> Self {
        Self {
            executor_factory: ExecutorFactory::new(),
            retry_policy: RetryPolicy::new(2, 500),
        }
    }

    /// 处理入站代理请求
    ///
    /// 完整流程：API Key 鉴权 → 路由解析 → 带重试的上游执行 → 用量记录。
    /// 失败时记录错误用量条目。
    ///
    /// # 参数
    /// - `state`：应用全局状态
    /// - `request`：代理请求对象
    ///
    /// # 返回
    /// 成功时返回 `ProxyOutput`（流式或非流式），失败时返回 `AppError`
    pub async fn handle_request(
        &self,
        state: &Arc<AppState>,
        client: &awc::Client,
        request: ProxyRequest,
    ) -> Result<ProxyOutput> {
        let start = Instant::now();
        let mut request = request;
        let conn = db_core::get_conn(&state.db_pool)?;

        // API Key 鉴权
        if state.config.require_api_key {
            if let Some(ref api_key) = request.api_key {
                let key_record = db_api_keys::get_by_key(&conn, api_key)?;
                if key_record.is_none() {
                    return Err(AppError::Unauthorized("Invalid API key".into()));
                }
                let key_obj = key_record.unwrap();
                // 校验 key_permissions 表的 allow/deny 规则
                if !crate::db::key_permissions::check_access(&conn, &key_obj.id, &request.model)? {
                    return Err(AppError::Unauthorized(
                        format!("Model '{}' is not allowed for this API key (permission denied)", request.model),
                    ));
                }
                // 校验 API Key 的模型白名单
                if let Some(allowed) = key_obj.allowed_models.as_array()
                    && !allowed.is_empty() {
                        let model_allowed = allowed.iter().any(|m| {
                            m.as_str().map(|s| request.model.starts_with(s)).unwrap_or(false)
                        });
                        if !model_allowed {
                            return Err(AppError::Unauthorized("Model not allowed for this API key".into()));
                        }
                    }
            } else {
                return Err(AppError::Unauthorized("API key required".into()));
            }
        }

        // Prompt 压缩：从 settings 读取压缩级别（fail-open + never-worse 保护）
        {
            let comp_config = {
                let pool = state.db_pool.clone();
                let level_str = tokio::task::spawn_blocking(move || -> String {
                    let Ok(conn) = crate::db::core::get_conn(&pool) else { return "minimal".to_string() };
                    let val = crate::db::settings::get(&conn, "settings", "general").unwrap_or(None);
                    val.as_ref()
                        .and_then(|v| v.get("compressionLevel"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("minimal")
                        .to_string()
                }).await.unwrap_or_else(|_| "minimal".to_string());

                let level = match level_str.as_str() {
                    "none" => crate::proxy::prompt_compression::CompressionLevel::None,
                    "aggressive" => crate::proxy::prompt_compression::CompressionLevel::Aggressive,
                    _ => crate::proxy::prompt_compression::CompressionLevel::Minimal,
                };
                crate::proxy::prompt_compression::CompressionConfig {
                    enabled: level != crate::proxy::prompt_compression::CompressionLevel::None,
                    level,
                    ..Default::default()
                }
            };
            let original_messages = request.messages.clone();
            let (compressed, saved) = crate::proxy::prompt_compression::compress_messages(&request.messages, &comp_config);
            if saved > 0 {
                log::info!("Prompt 压缩：节省 ~{} tokens", saved);
                // 存储原始内容到回忆系统（best-effort，失败不影响请求）
                let pool = state.db_pool.clone();
                let model = request.model.clone();
                tokio::task::spawn_blocking(move || {
                    let Ok(conn) = crate::db::core::get_conn(&pool) else { return };
                    let content = serde_json::to_string(&original_messages).unwrap_or_default();
                    let hash = crate::db::compressed_content::content_hash(&content);
                    let _ = crate::db::compressed_content::store(
                        &conn, &hash, &content, saved, None, Some(&model),
                    );
                });
                request.messages = compressed;
                request.saved_tokens = saved;
            }
        }

        // 粘性会话：若请求携带 session_id 且有有效绑定，直接使用绑定的模型
        let session_id = request.extra.get("session_id").and_then(|v| v.as_str()).map(|s| s.to_string());
        if let Some(ref sid) = session_id
            && let Some((provider, model, connection_id)) = state.sticky_session.get(sid) {
                log::info!("粘性会话 '{}' 命中：{}/{}/{:?}", sid, provider, model, connection_id);
                if let Some(def) = state.provider_registry.get(&provider) {
                    let connection = if let Some(ref cid) = connection_id {
                        db_providers::list_by_provider(&conn, &provider, &state.encryption_key)?
                            .into_iter()
                            .find(|c| c.id == *cid)
                    } else {
                        db_providers::list_by_provider(&conn, &provider, &state.encryption_key)?
                            .into_iter()
                            .next()
                    };
                    if let Some(conn_obj) = connection {
                        let result = self
                            .handle_single_with_retry(state, client, &conn, &request, def, &conn_obj, &model, start, None)
                            .await;
                        match &result {
                            Ok(_) => {
                                state.sticky_session.set(sid, &provider, &model, Some(&conn_obj.id));
                            }
                            Err(e) => {
                                log::warn!("粘性会话 '{}' 绑定模型失败: {}，回退到正常路由", sid, e);
                                state.sticky_session.clear(sid);
                            }
                        }
                        if result.is_ok() {
                            return result;
                        }
                    }
                }
            }

        // 虚拟模型别名：检查请求模型是否匹配某个别名，若匹配则按 targets 顺序故障转移
        if let Some(alias) = db_aliases::get_by_alias(&conn, &request.model)? {
            log::info!("模型别名 '{}' 命中，共 {} 个目标", alias.alias, alias.targets.len());
            // 真实错误（上游/网络），优先返回给下游便于排查
            let mut last_err: Option<AppError> = None;
            // 熔断器名称：所有目标均因熔断不可用时，以熔断错误兜底（下游转友好提示）
            let mut last_circuit: Option<String> = None;
            for target in &alias.targets {
                let Some(def) = state.provider_registry.get(&target.provider) else {
                    log::warn!("别名 '{}' 目标 provider '{}' 未注册，跳过", alias.alias, target.provider);
                    continue;
                };
                // 查找连接：优先用 connection_id，否则取该 provider 的第一个活跃连接
                let connection = if let Some(ref cid) = target.connection_id {
                    db_providers::list_by_provider(&conn, &target.provider, &state.encryption_key)?
                        .into_iter()
                        .find(|c| c.id == *cid)
                } else {
                    db_providers::list_by_provider(&conn, &target.provider, &state.encryption_key)?
                        .into_iter()
                        .next()
                };
                let Some(conn_obj) = connection else {
                    log::warn!("别名 '{}' 目标 provider '{}' 无活跃连接，跳过", alias.alias, target.provider);
                    continue;
                };
                // 熔断快速失败：目标连接处于 Open 态时直接跳过，不再重置重试。
                // 熔断器在冷却期（默认 60s）后会自动转入 HalfOpen 放行探测请求，
                // 因此这里若执行 reset，会让熔断对别名目标彻底失效——此前正是因此
                // 导致每个请求都要完整重试一遍已确认故障的连接，长尾延迟显著。
                let breaker_name = format!("{}:{}", def.id, conn_obj.id);
                if !state.resilience_manager.is_available(&breaker_name) {
                    log::warn!(
                        "别名 '{}' 目标 {}/{} 熔断器打开，跳过（冷却后自动探测）",
                        alias.alias,
                        target.provider,
                        target.model
                    );
                    last_circuit = Some(breaker_name);
                    continue;
                }

                log::info!("别名 '{}' 尝试目标 {}/{} (连接: {})", alias.alias, target.provider, target.model, conn_obj.name);
                let result = self
                    .handle_single_with_retry(state, client, &conn, &request, def, &conn_obj, &target.model, start, None)
                    .await;
                match result {
                    Ok(output) => {
                        log::info!("别名 '{}' 目标 {}/{} 成功", alias.alias, target.provider, target.model);
                        return Ok(output);
                    }
                    Err(e) => {
                        let err_str = format!("{}", e);
                        log::warn!("别名 '{}' 目标 {}/{} 失败: {}，尝试下一个", alias.alias, target.provider, target.model, e);
                        let latency = start.elapsed().as_millis() as i64;
                        let entry = UsageEntry {
                            id: 0,
                            provider: Some(conn_obj.name.clone()),
                            model: Some(target.model.clone()),
                            connection_id: Some(conn_obj.id.clone()),
                            api_key_id: request.api_key.clone(),
                            api_key_name: None,
                            tokens_input: 0,
                            tokens_output: 0,
                            tokens_cache_read: 0,
                            tokens_cache_creation: 0,
                            tokens_reasoning: 0,
                            service_tier: "standard".to_string(),
                            status: "error".to_string(),
                            success: false,
                            error_code: Some(err_str),
                            latency_ms: Some(latency),
                            ttft_ms: None,
                            cost: 0.0,
                            usage_estimated: false,
                            saved_tokens: request.saved_tokens,
                            agent: request.agent.clone(),
                            timestamp: chrono::Utc::now().to_rfc3339(),
                        };
                        let _ = db_usage::record(&conn, &entry);
                        // 熔断快速失败仅记录名称；真实错误优先返回给下游（含上游错误透传）
                        if let AppError::CircuitOpen(ref name) = e {
                            last_circuit = Some(name.clone());
                        } else {
                            last_err = Some(e);
                        }
                    }
                }
            }
            // 错误优先级：真实错误 > 熔断（下游转友好提示）> 路由错误
            return Err(if let Some(e) = last_err {
                e
            } else if let Some(name) = last_circuit {
                AppError::CircuitOpen(name)
            } else {
                AppError::Routing(format!("All targets failed for alias '{}'", alias.alias))
            });
        }

        // auto 模型：按优先级遍历所有活跃连接与模型
        if request.model == "auto" {
            return self.handle_auto_route(state, client, &conn, &request, start).await;
        }

        // 路由配置 profile：auto:<profile_name> 使用命名回退链
        if let Some(profile_name) = request.model.strip_prefix("auto:")
            && let Some(profile) = db_routing_profiles::get_by_name(&conn, profile_name)? {
                log::info!("路由配置 '{}' 命中，共 {} 个目标", profile.name, profile.targets.len());
                let mut last_err: Option<AppError> = None;
                let mut last_circuit: Option<String> = None;
                for target in &profile.targets {
                    let Some(def) = state.provider_registry.get(&target.provider) else {
                        log::warn!("配置 '{}' 目标 provider '{}' 未注册，跳过", profile.name, target.provider);
                        continue;
                    };
                    let connection = if let Some(ref cid) = target.connection_id {
                        db_providers::list_by_provider(&conn, &target.provider, &state.encryption_key)?
                            .into_iter()
                            .find(|c| c.id == *cid)
                    } else {
                        db_providers::list_by_provider(&conn, &target.provider, &state.encryption_key)?
                            .into_iter()
                            .next()
                    };
                    let Some(conn_obj) = connection else {
                        log::warn!("配置 '{}' 目标 provider '{}' 无活跃连接，跳过", profile.name, target.provider);
                        continue;
                    };
                    let breaker_name = format!("{}:{}", def.id, conn_obj.id);
                    if !state.resilience_manager.is_available(&breaker_name) {
                        log::warn!("配置 '{}' 目标 {}/{} 熔断器打开，跳过", profile.name, target.provider, target.model);
                        last_circuit = Some(breaker_name);
                        continue;
                    }
                    log::info!("配置 '{}' 尝试目标 {}/{}", profile.name, target.provider, target.model);
                    let result = self
                        .handle_single_with_retry(state, client, &conn, &request, def, &conn_obj, &target.model, start, None)
                        .await;
                    match result {
                        Ok(output) => {
                            log::info!("配置 '{}' 目标 {}/{} 成功", profile.name, target.provider, target.model);
                            if let Some(ref sid) = session_id {
                                state.sticky_session.set(sid, &target.provider, &target.model, Some(&conn_obj.id));
                            }
                            return Ok(output);
                        }
                        Err(e) => {
                            log::warn!("配置 '{}' 目标 {}/{} 失败: {}，尝试下一个", profile.name, target.provider, target.model, e);
                            if let AppError::CircuitOpen(ref name) = e {
                                last_circuit = Some(name.clone());
                            } else {
                                last_err = Some(e);
                            }
                        }
                    }
                }
                return Err(if let Some(e) = last_err {
                    e
                } else if let Some(name) = last_circuit {
                    AppError::CircuitOpen(name)
                } else {
                    AppError::Routing(format!("All targets failed for profile '{}'", profile.name))
                });
            }

        // 路由解析：找到提供商定义、活跃连接与实际模型名
        let (provider_def, connection, model) =
            self.resolve_route(&conn, &state.provider_registry, &request, &state.encryption_key)?;

        let conn_obj = connection.ok_or_else(|| AppError::Provider("No active connection".into()))?;
        // 带重试与熔断的上游执行
        let result = self
            .handle_single_with_retry(state, client, &conn, &request, &provider_def, &conn_obj, &model, start, None)
            .await;

        // 失败时记录错误用量
        match &result {
            Ok(_) => {
                if let Some(ref sid) = session_id {
                    state.sticky_session.set(sid, &provider_def.id, &model, Some(&conn_obj.id));
                }
            }
            Err(e) => {
                let latency = start.elapsed().as_millis() as i64;
                let entry = UsageEntry {
                    id: 0,
                    provider: Some(conn_obj.name.clone()),
                    model: Some(model.clone()),
                    connection_id: Some(conn_obj.id.clone()),
                    api_key_id: request.api_key.clone(),
                    api_key_name: None,
                    tokens_input: 0,
                    tokens_output: 0,
                    tokens_cache_read: 0,
                    tokens_cache_creation: 0,
                    tokens_reasoning: 0,
                    service_tier: "standard".to_string(),
                    status: "error".to_string(),
                    success: false,
                    error_code: Some(format!("{}", e)),
                    latency_ms: Some(latency),
                    ttft_ms: None,
                    cost: 0.0,
                    usage_estimated: false,
                    saved_tokens: request.saved_tokens,
                    agent: request.agent.clone(),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                };
                let _ = db_usage::record(&conn, &entry);
            }
        }

        // 可转移错误：在 10 秒预算内后台寻找可用模型（紧急故障转移），
        // 找到则直接返回结果；转移失败仍返回原始错误（比“所有候选失败”更有排查价值）
        if let Err(ref e) = result
            && Self::is_failoverable(e)
        {
            log::warn!(
                "模型 {}/{} 上游失败（{}），启动紧急故障转移",
                provider_def.id, model, e
            );
            if let Ok(output) = self
                .handle_emergency_failover(
                    state, client, &conn, &request, &provider_def, &conn_obj, &model,
                    session_id.as_deref(), start,
                )
                .await
            {
                return Ok(output);
            }
        }

        result
    }

    /// 路由解析：根据请求模型名找到对应的提供商定义与活跃连接
    ///
    /// 解析策略：
    /// 1. 优先通过注册表精确匹配模型名
    /// 2. 匹配失败时回退到配置了默认模型的活跃连接
    ///
    /// # 参数
    /// - `conn`：数据库连接
    /// - `registry`：提供商注册表
    /// - `request`：代理请求
    /// - `enc_key`：加密密钥（用于解密连接凭证）
    ///
    /// # 返回
    /// `(提供商定义, 活跃连接, 实际模型名)`
    fn resolve_route(
        &self,
        conn: &rusqlite::Connection,
        registry: &ProviderRegistry,
        request: &ProxyRequest,
        enc_key: &[u8],
    ) -> Result<(crate::providers::types::ProviderDef, Option<crate::db::models::ProviderConnection>, String)> {
        // 优先通过注册表精确匹配
        if let Some((provider_id, def, model)) = registry.resolve_model_provider(&request.model) {
            let connections = db_providers::list_by_provider(conn, &provider_id, enc_key)?;
            // 优先选择 models 列表包含该模型的连接，回退到第一个活跃连接
            let connection = connections
                .iter()
                .find(|c| {
                    c.models
                        .as_ref()
                        .is_some_and(|models| models.iter().any(|m| m.id == model))
                })
                .or_else(|| connections.first())
                .cloned();
            return Ok((def.clone(), connection, model));
        }

        // 模型名无法解析（如 Claude Code 发送的 claude-sonnet-4 等原生模型名）：
        // 回退到配置了默认模型的活跃连接（借鉴 relay-rs 的默认模型回退）
        for def in registry.list() {
            let connections = db_providers::list_by_provider(conn, &def.id, enc_key)?;
            if let Some(c) = connections.iter().find(|c| {
                c.default_model.as_deref().is_some_and(|m| !m.is_empty())
            }) {
                let model = c.default_model.clone().unwrap_or_default();
                log::warn!(
                    "Model '{}' cannot be resolved, falling back to default model '{}/{}' (connection: {})",
                    request.model,
                    def.id,
                    model,
                    c.name
                );
                return Ok((def.clone(), Some(c.clone()), model));
            }
        }

        Err(AppError::Routing(format!("Cannot resolve model: {}", request.model)))
    }

    /// auto 模型路由：按优先级遍历所有活跃连接与模型
    ///
    /// 当请求模型为 `auto` 时，查询所有活跃连接（按优先级降序），
    /// 遍历每个连接的模型列表，检查熔断器状态后依次尝试。
    /// 失败的模型使用更长的冷静期（300秒）以避免反复命中故障目标。
    ///
    /// # 参数
    /// - `state`：应用全局状态
    /// - `client`：HTTP 客户端
    /// - `conn`：数据库连接
    /// - `request`：代理请求
    /// - `start`：请求起始时间
    async fn handle_auto_route(
        &self,
        state: &Arc<AppState>,
        client: &awc::Client,
        conn: &rusqlite::Connection,
        request: &ProxyRequest,
        start: Instant,
    ) -> Result<ProxyOutput> {
        // 查询所有活跃连接，已按 priority DESC 排序
        let all_connections = db_providers::list_all_active(conn, &state.encryption_key)?;
        if all_connections.is_empty() {
            return Err(AppError::Routing("No active connections for auto routing".into()));
        }

        // 读取 auto 路由策略：model-first（模型优先）或 provider-first（提供方优先，默认）
        let auto_strategy = db_settings::get(conn, "settings", "general")
            .ok()
            .flatten()
            .and_then(|s| s.get("autoRouteStrategy").and_then(|v| v.as_str()).map(|s| s.to_string()))
            .unwrap_or_else(|| "provider-first".to_string());

        log::info!("auto 路由：共 {} 个活跃连接，策略: {}", all_connections.len(), auto_strategy);

        // 收集所有候选 (connection_index, model)
        let mut candidates: Vec<(usize, String)> = Vec::new();

        if auto_strategy == "model-first" {
            // 模型优先：按模型映射（alias targets）顺序收集候选，同模型内按连接优先级排序
            let aliases = db_aliases::list(conn).unwrap_or_default();
            let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
            for alias in &aliases {
                if !alias.is_active { continue; }
                for target in &alias.targets {
                    // 找到匹配的连接（按 connection_id 精确匹配，或按 provider+model 模糊匹配）
                    let mut matching: Vec<(usize, String)> = Vec::new();
                    for (ci, c) in all_connections.iter().enumerate() {
                        if c.provider != target.provider { continue; }
                        let models: Vec<String> = match &c.models {
                            Some(list) if !list.is_empty() => list.iter().map(|m| m.id.clone()).collect(),
                            _ => c.default_model.clone().map(|m| vec![m]).unwrap_or_default(),
                        };
                        for m in &models {
                            if m != &target.model { continue; }
                            if let Some(ref cid) = target.connection_id {
                                if &c.id != cid { continue; }
                            }
                            let key = (c.id.clone(), m.clone());
                            if seen.insert(key) {
                                matching.push((ci, m.clone()));
                            }
                        }
                    }
                    // 同模型内按连接优先级排序（priority DESC）
                    matching.sort_by(|a, b| {
                        all_connections[b.0].priority.cmp(&all_connections[a.0].priority)
                    });
                    candidates.extend(matching);
                }
            }
            // 若模型映射未覆盖任何候选，回退到提供方优先
            if candidates.is_empty() {
                for (ci, connection) in all_connections.iter().enumerate() {
                    let models: Vec<String> = match &connection.models {
                        Some(list) if !list.is_empty() => list.iter().map(|m| m.id.clone()).collect(),
                        _ => connection.default_model.clone().map(|m| vec![m]).unwrap_or_default(),
                    };
                    for model in models {
                        candidates.push((ci, model));
                    }
                }
            }
        } else {
            // 提供方优先（默认）：按连接 priority DESC 遍历，每连接内遍历模型
            for (ci, connection) in all_connections.iter().enumerate() {
                let models: Vec<String> = match &connection.models {
                    Some(list) if !list.is_empty() => list.iter().map(|m| m.id.clone()).collect(),
                    _ => connection.default_model.clone().map(|m| vec![m]).unwrap_or_default(),
                };
                for model in models {
                    candidates.push((ci, model));
                }
            }
        }

        // auto 粘性：优先尝试最近成功的模型，保持 agent 连续性
        if let Some((last_provider, last_model, last_conn_id)) = state.bandit_router.get_last_success() {
            for (ci, model) in &candidates {
                let pconn = &all_connections[*ci];
                if pconn.provider != last_provider || model != &last_model {
                    continue;
                }
                if let Some(ref cid) = last_conn_id
                    && &pconn.id != cid {
                        continue;
                    }
                let Some(provider_def) = state.provider_registry.get(&pconn.provider) else {
                    break;
                };
                // 粘性候选正被其他智能体占用时避让，回退 bandit 排序（软避让）
                let occ_key = format!("{}:{}:{}", pconn.provider, pconn.id, model);
                if state.occupancy.busy_by_others(&occ_key, request.agent.as_deref()) {
                    log::info!(
                        "auto 粘性：{}/{} 正被其他智能体使用，避让并回退 bandit 排序",
                        last_provider, last_model
                    );
                    break;
                }
                let breaker_name = format!("{}:{}", provider_def.id, pconn.id);
                let auto_model_breaker = format!("auto:{}:{}:{}", provider_def.id, pconn.id, model);
                if !state.resilience_manager.is_available(&breaker_name)
                    || !state.resilience_manager.is_available(&auto_model_breaker)
                {
                    log::info!("auto 粘性：上次模型 {}/{}/{} 熔断中，回退到 bandit 排序", last_provider, pconn.name, last_model);
                    state.bandit_router.clear_last_success();
                    break;
                }
                log::info!("auto 粘性：优先尝试上次成功的 {}/{}/{}", last_provider, pconn.name, last_model);
                let result = self
                    .handle_single_with_retry(state, client, conn, request, provider_def, pconn, model.as_str(), start, None)
                    .await;
                match result {
                    Ok(output) => {
                        let latency_ms = start.elapsed().as_millis() as f64;
                        state.bandit_router.record_success(&pconn.provider, model.as_str(), Some(&pconn.id), latency_ms);
                        log::info!("auto 粘性：成功 {}/{}/{}", last_provider, pconn.name, last_model);
                        return Ok(output);
                    }
                    Err(e) => {
                        log::warn!("auto 粘性：{}/{}/{} 失败: {}，回退到 bandit 排序", last_provider, pconn.name, last_model, e);
                        state.bandit_router.clear_last_success();
                        state.bandit_router.record_failure(&pconn.provider, model.as_str(), Some(&pconn.id));
                        state.resilience_manager.record_failure_cfg(&auto_model_breaker, 2, 300);
                        break;
                    }
                }
            }
        }

        // 用 bandit 评分排序候选
        let bandit_keys: Vec<(String, String, Option<String>)> = candidates
            .iter()
            .map(|(ci, model)| {
                let conn = &all_connections[*ci];
                (conn.provider.clone(), model.clone(), Some(conn.id.clone()))
            })
            .collect();
        let ranked_indices = state.bandit_router.rank_candidates(&bandit_keys);

        // 跨智能体软避让：被其他智能体占用的候选稳定后移（保持 bandit 相对顺序），
        // 全部被占用时仍按序尝试，不拒绝服务
        let ranked_indices = if request.agent.is_some() {
            let me = request.agent.as_deref();
            let mut ordered = ranked_indices;
            ordered.sort_by_key(|&idx| {
                let (ci, model) = &candidates[idx];
                let c = &all_connections[*ci];
                let key = format!("{}:{}:{}", c.provider, c.id, model);
                usize::from(state.occupancy.busy_by_others(&key, me))
            });
            ordered
        } else {
            ranked_indices
        };

        let mut last_err: Option<AppError> = None;
        let mut last_circuit: Option<String> = None;

        for &idx in &ranked_indices {
            let (ci, model) = &candidates[idx];
            let connection = &all_connections[*ci];
            // 获取提供商定义
            let Some(provider_def) = state.provider_registry.get(&connection.provider) else {
                log::warn!("auto: provider '{}' 未注册，跳过连接 {}", connection.provider, connection.name);
                continue;
            };

            // 连接级熔断器检查
            let breaker_name = format!("{}:{}", provider_def.id, connection.id);
            if !state.resilience_manager.is_available(&breaker_name) {
                log::warn!("auto: 连接级熔断器打开，跳过 {}/{}", provider_def.id, connection.name);
                last_circuit = Some(breaker_name);
                continue;
            }

            // 模型级熔断器检查（auto 专用，冷静期 300 秒）
            let auto_model_breaker = format!("auto:{}:{}:{}", provider_def.id, connection.id, model);
            if !state.resilience_manager.is_available(&auto_model_breaker) {
                log::warn!("auto: 模型熔断器打开，跳过 {}/{}/{}", provider_def.id, connection.name, model);
                last_circuit = Some(auto_model_breaker);
                continue;
            }

            log::info!("auto: 尝试 {}/{}/{} (连接: {}, 优先级: {})", 
                provider_def.id, model, connection.name, connection.id, connection.priority);

            let result = self
                .handle_single_with_retry(state, client, conn, request, provider_def, connection, model.as_str(), start, None)
                .await;

            match result {
                Ok(output) => {
                    let latency_ms = start.elapsed().as_millis() as f64;
                    state.bandit_router.record_success(
                        &connection.provider, model.as_str(), Some(&connection.id), latency_ms,
                    );
                    log::info!("auto: 成功 {}/{}/{}", provider_def.id, connection.name, model);
                    return Ok(output);
                }
                Err(e) => {
                    let err_str = format!("{}", e);
                    log::warn!("auto: {}/{}/{} 失败: {}，尝试下一个", provider_def.id, connection.name, model, e);
                    state.bandit_router.record_failure(
                        &connection.provider, model.as_str(), Some(&connection.id),
                    );

                    // 记录用量错误条目
                    let latency = start.elapsed().as_millis() as i64;
                    let entry = UsageEntry {
                        id: 0,
                        provider: Some(connection.name.clone()),
                        model: Some(model.to_string()),
                        connection_id: Some(connection.id.clone()),
                        api_key_id: request.api_key.clone(),
                        api_key_name: None,
                        tokens_input: 0,
                        tokens_output: 0,
                        tokens_cache_read: 0,
                        tokens_cache_creation: 0,
                        tokens_reasoning: 0,
                        service_tier: "standard".to_string(),
                        status: "error".to_string(),
                        success: false,
                        error_code: Some(err_str),
                        latency_ms: Some(latency),
                        ttft_ms: None,
                        cost: 0.0,
                        usage_estimated: false,
                        saved_tokens: request.saved_tokens,
                        agent: request.agent.clone(),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    };
                    let _ = db_usage::record(conn, &entry);

                    // 模型级熔断：auto 专用，冷静期 300 秒（比普通模型级 60 秒更长）
                    if let AppError::CircuitOpen(ref name) = e {
                        last_circuit = Some(name.clone());
                    } else {
                        // 非熔断错误也记录到 auto 专用熔断器，冷静期 300 秒
                        state.resilience_manager.record_failure_cfg(&auto_model_breaker, 2, 300);
                        last_err = Some(e);
                    }
                }
            }
        }

        // 所有候选均失败
        Err(if let Some(e) = last_err {
            e
        } else if let Some(name) = last_circuit {
            AppError::CircuitOpen(name)
        } else {
            AppError::Routing("All candidates failed for auto routing".into())
        })
    }

    /// 判断错误是否值得做紧急故障转移
    ///
    /// 上游错误（非 400 的 4xx 与 5xx）、网络错误、熔断、路由失败可转移；
    /// 请求参数错误（换模型大概率同样失败）与网关鉴权失败（与上游无关）不转移。
    fn is_failoverable(e: &AppError) -> bool {
        match e {
            AppError::Upstream { status, .. } => *status != 400,
            AppError::Provider(_)
            | AppError::Http(_)
            | AppError::CircuitOpen(_)
            | AppError::Routing(_) => true,
            _ => false,
        }
    }

    /// 显式模型失败后的紧急故障转移：在时间预算内自动寻找其他可用模型
    ///
    /// 与 auto 路由的区别：这是对"调用方显式指定模型"失败后的兜底——不再把
    /// 上游错误直接抛回，而是后台遍历其余候选（bandit 排序 + 占用避让），
    /// 成功后直接返回结果。10 秒预算耗尽或候选耗尽时放弃，由调用方返回
    /// 原始错误（比"所有候选失败"更有排查价值）。
    // 各参数承担独立职责（状态/客户端/连接/请求/失败目标/会话），拆分结构体收益有限。
    #[allow(clippy::too_many_arguments)]
    async fn handle_emergency_failover(
        &self,
        state: &Arc<AppState>,
        client: &awc::Client,
        conn: &rusqlite::Connection,
        request: &ProxyRequest,
        failed_def: &crate::providers::types::ProviderDef,
        failed_conn: &crate::db::models::ProviderConnection,
        failed_model: &str,
        session_id: Option<&str>,
        start: Instant,
    ) -> Result<ProxyOutput> {
        const FAILOVER_BUDGET_SECS: u64 = 10;
        // 预算剩余不足一次尝试的保底时间时不再启动新尝试
        const MIN_ATTEMPT_SECS: u64 = 2;
        let deadline = Instant::now() + std::time::Duration::from_secs(FAILOVER_BUDGET_SECS);

        // 收集所有活跃连接的全部模型候选，排除刚失败过的组合
        let all_connections = db_providers::list_all_active(conn, &state.encryption_key)?;
        let mut candidates: Vec<(usize, String)> = Vec::new();
        for (ci, connection) in all_connections.iter().enumerate() {
            let models: Vec<String> = match &connection.models {
                Some(list) if !list.is_empty() => list.iter().map(|m| m.id.clone()).collect(),
                _ => connection.default_model.clone().map(|m| vec![m]).unwrap_or_default(),
            };
            for model in models {
                if connection.id == failed_conn.id && model == failed_model {
                    continue;
                }
                candidates.push((ci, model));
            }
        }
        if candidates.is_empty() {
            return Err(AppError::Routing("no failover candidates".into()));
        }

        // bandit 排序 + 占用避让（与 auto 路由相同的排序策略）
        let bandit_keys: Vec<(String, String, Option<String>)> = candidates
            .iter()
            .map(|(ci, model)| {
                let c = &all_connections[*ci];
                (c.provider.clone(), model.clone(), Some(c.id.clone()))
            })
            .collect();
        let mut ranked = state.bandit_router.rank_candidates(&bandit_keys);
        if request.agent.is_some() {
            let me = request.agent.as_deref();
            ranked.sort_by_key(|&idx| {
                let (ci, model) = &candidates[idx];
                let c = &all_connections[*ci];
                let key = format!("{}:{}:{}", c.provider, c.id, model);
                usize::from(state.occupancy.busy_by_others(&key, me))
            });
        }

        log::warn!(
            "紧急故障转移：{}/{} 失败，预算 {}s，共 {} 个候选",
            failed_def.id, failed_model, FAILOVER_BUDGET_SECS, ranked.len()
        );

        let mut tried = 0usize;
        for &idx in &ranked {
            let (ci, model) = &candidates[idx];
            let connection = &all_connections[*ci];
            // 预算检查：剩余时间不足一次尝试时放弃
            if Instant::now() + std::time::Duration::from_secs(MIN_ATTEMPT_SECS) >= deadline {
                log::warn!("紧急故障转移：预算即将耗尽（已尝试 {} 个候选），放弃", tried);
                break;
            }
            let Some(provider_def) = state.provider_registry.get(&connection.provider) else {
                continue;
            };
            let breaker_name = format!("{}:{}", provider_def.id, connection.id);
            if !state.resilience_manager.is_available(&breaker_name) {
                continue;
            }
            let model_breaker = format!("{}:{}:{}", provider_def.id, connection.id, model);
            if !state.resilience_manager.is_available(&model_breaker) {
                continue;
            }

            tried += 1;
            log::info!(
                "紧急故障转移：尝试 {}/{}/{}",
                provider_def.id, model, connection.name
            );
            let result = self
                .handle_single_with_retry(
                    state, client, conn, request, provider_def, connection, model.as_str(), start,
                    Some(deadline),
                )
                .await;
            match result {
                Ok(output) => {
                    let latency_ms = start.elapsed().as_millis() as f64;
                    state.bandit_router.record_success(
                        &connection.provider, model.as_str(), Some(&connection.id), latency_ms,
                    );
                    if let Some(sid) = session_id {
                        state.sticky_session.set(sid, &provider_def.id, model, Some(&connection.id));
                    }
                    log::info!(
                        "紧急故障转移：成功切换到 {}/{}/{}",
                        provider_def.id, model, connection.name
                    );
                    return Ok(output);
                }
                Err(e) => {
                    state.bandit_router.record_failure(
                        &connection.provider, model.as_str(), Some(&connection.id),
                    );
                    log::warn!("紧急故障转移：{}/{} 失败: {}", provider_def.id, model, e);
                }
            }
        }

        Err(AppError::Routing(format!(
            "紧急故障转移失败（已尝试 {} 个候选）",
            tried
        )))
    }

    /// 带重试与熔断的单连接请求处理
    ///
    /// 每次尝试前检查熔断器状态，执行后根据结果记录成功/失败。
    /// 对 5xx 和 429 错误按重试策略进行指数退避重试。
    ///
    /// # 参数
    /// - `state`：应用全局状态
    /// - `conn`：数据库连接
    /// - `request`：代理请求
    /// - `def`：提供商定义
    /// - `connection`：提供商连接
    /// - `model`：实际模型名
    /// - `start`：请求起始时间
    ///
    /// # 返回
    /// 成功时返回 `ProxyOutput`，失败时返回 `AppError`
    // 各参数承担独立职责（状态/客户端/连接/请求/路由目标），拆分结构体收益有限。
    #[allow(clippy::too_many_arguments)]
    async fn handle_single_with_retry(
        &self,
        state: &Arc<AppState>,
        client: &awc::Client,
        conn: &rusqlite::Connection,
        request: &ProxyRequest,
        def: &crate::providers::types::ProviderDef,
        connection: &crate::db::models::ProviderConnection,
        model: &str,
        start: Instant,
        deadline: Option<Instant>,
    ) -> Result<ProxyOutput> {
        // 记录智能体对该模型的占用（auto 路由据此做跨智能体软避让）
        if let Some(ref agent) = request.agent {
            let model_key = format!("{}:{}:{}", def.id, connection.id, model);
            state.occupancy.touch(&model_key, agent);
        }
        // 按提供商格式创建执行器
        let executor = self.executor_factory.create(def);
        let mut last_err = None;

        // 连接级熔断器：provider_id:connection_id（阈值 5，冷却 60s）
        let breaker_name = format!("{}:{}", def.id, connection.id);
        // 模型级熔断器：provider_id:connection_id:model（阈值 3，冷却 60s）。
        // 模型下线 / 无权限 / 不存在等"该模型不可用"的故障比连接级故障
        // 更早触发快速失败，别名故障转移可更快跳到下一个目标。
        let model_breaker = format!("{}:{}:{}", def.id, connection.id, model);

        // 重试循环
        for attempt in 0..=self.retry_policy.max_retries() {
            // 故障转移时间预算耗尽：停止重试（紧急故障转移场景传入 deadline）
            if let Some(d) = deadline
                && Instant::now() >= d
            {
                if last_err.is_none() {
                    last_err = Some(AppError::Routing("failover deadline exceeded".into()));
                }
                break;
            }
            // 连接级熔断器打开时跳过执行（快速失败）
            if !state.resilience_manager.is_available(&breaker_name) {
                last_err = Some(AppError::CircuitOpen(breaker_name.clone()));
                break;
            }
            // 模型级熔断器打开时同样快速失败
            if !state.resilience_manager.is_available(&model_breaker) {
                log::warn!(
                    "模型级熔断器打开，快速失败：{} (冷却后自动探测)",
                    model_breaker
                );
                last_err = Some(AppError::CircuitOpen(model_breaker.clone()));
                break;
            }

            // 速率限制检查：RPM/RPD/TPM/TPD
            let estimated_tokens = estimate_request_tokens(&request.messages);
            if !state.rate_limiter.check(&def.id, model, Some(&connection.id), estimated_tokens) {
                log::warn!(
                    "速率限制触发，跳过：{}/{} (连接: {})",
                    def.id, model, connection.id
                );
                last_err = Some(AppError::CircuitOpen(format!("rate-limited:{}:{}", def.id, model)));
                break;
            }

            // 构造流式用量落库回调（流结束时由 sse.rs 调用）
            let cb_pool = state.db_pool.clone();
            let cb_provider = connection.name.clone();
            let cb_model = model.to_string();
            let cb_conn = connection.id.clone();
            let cb_api_key = request.api_key.clone();
            let cb_start = start;
            let cb_provider_id = def.id.clone();
            let cb_rate_limiter = state.rate_limiter.clone();
            // 请求体输入 token 估算值：上游未返回 usage 时回填输入侧
            let cb_req_input = estimate_request_tokens(&request.messages);
            let cb_saved_tokens = request.saved_tokens;
            let cb_agent = request.agent.clone();
            let usage_cb: UsageCallback = Arc::new(move |usage: StreamUsage, ok: bool| {
                let Ok(c) = db_core::get_conn(&cb_pool) else { return };
                let latency = cb_start.elapsed().as_millis() as i64;
                // 估算兜底：上游未返回输入 token 时用请求体估算值补齐
                let (input_tokens, usage_estimated) = if usage.estimated && usage.input_tokens == 0 {
                    (cb_req_input, true)
                } else {
                    (usage.input_tokens, usage.estimated)
                };
                let entry = UsageEntry {
                    id: 0,
                    provider: Some(cb_provider.clone()),
                    model: Some(cb_model.clone()),
                    connection_id: Some(cb_conn.clone()),
                    api_key_id: cb_api_key.clone(),
                    api_key_name: None,
                    tokens_input: input_tokens,
                    tokens_output: usage.output_tokens,
                    tokens_cache_read: usage.cache_read,
                    tokens_cache_creation: usage.cache_creation,
                    tokens_reasoning: usage.reasoning,
                    service_tier: "standard".to_string(),
                    status: if ok { "success".to_string() } else { "error".to_string() },
                    success: ok,
                    error_code: if ok { None } else { Some("stream_error".to_string()) },
                    latency_ms: Some(latency),
                    ttft_ms: None,
                    cost: 0.0,
                    usage_estimated,
                    saved_tokens: cb_saved_tokens,
                    agent: cb_agent.clone(),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                };
                let _ = db_usage::record(&c, &entry);
                // 记数速率限制用量
                let total_tokens = input_tokens + usage.output_tokens;
                cb_rate_limiter.record(&cb_provider_id, &cb_model, Some(&cb_conn), total_tokens);
                let _ = db_rate_limits::record_usage(&c, &cb_provider_id, &cb_model, Some(&cb_conn), total_tokens);
            });

            match executor.execute(client, request, def, connection, model, Some(usage_cb)).await {
                Ok(ExecutorOutput::Json(body)) => {
                    // 非流式成功：提取用量、记录成功、写入用量条目
                    let extracted = extract_usage_from_response(&body, def.api_format.as_str());
                    // 估算兜底：上游未返回输入 token 时用请求体估算值补齐
                    let tokens_in = if extracted.estimated && extracted.input_tokens == 0 {
                        estimate_request_tokens(&request.messages)
                    } else {
                        extracted.input_tokens
                    };
                    let latency = start.elapsed().as_millis() as i64;
                    let _ = db_providers::reset_backoff(conn, &connection.id);
                    let _ = db_providers::increment_usage(conn, &connection.id);
                    state.resilience_manager.record_success(&breaker_name);
                    state.resilience_manager.record_success(&model_breaker);
                    let entry = UsageEntry {
                        id: 0,
                        provider: Some(def.id.clone()),
                        model: Some(model.to_string()),
                        connection_id: Some(connection.id.clone()),
                        api_key_id: request.api_key.clone(),
                        api_key_name: None,
                        tokens_input: tokens_in,
                        tokens_output: extracted.output_tokens,
                        tokens_cache_read: extracted.cache_read,
                        tokens_cache_creation: extracted.cache_creation,
                        tokens_reasoning: extracted.reasoning,
                        service_tier: "standard".to_string(),
                        status: "success".to_string(),
                        success: true,
                        error_code: None,
                        latency_ms: Some(latency),
                        ttft_ms: None,
                        cost: extracted.cost,
                        usage_estimated: extracted.estimated,
                        saved_tokens: request.saved_tokens,
                        agent: request.agent.clone(),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    };
                    let _ = db_usage::record(conn, &entry);

                    // 记数速率限制用量
                    let total_tokens = tokens_in + extracted.output_tokens;
                    state.rate_limiter.record(&def.id, model, Some(&connection.id), total_tokens);
                    let _ = db_rate_limits::record_usage(conn, &def.id, model, Some(&connection.id), total_tokens);

                    // 记数 bandit 评分
                    state.bandit_router.record_success(&def.id, model, Some(&connection.id), latency as f64);

                    // Tool-call rescue：检测并救援纯文本 tool call
                    let mut body = body;
                    let rescued = tool_call_rescue::rescue_from_response(&mut body);
                    if rescued {
                        log::info!("Tool-call rescue: 纯文本 tool call 已转换为结构化格式");
                    }

                    return Ok(ProxyOutput::Response(body));
                }
                Ok(ExecutorOutput::Stream(stream)) => {
                    // 流式响应直接透传，token 用量由回调在流结束时写库
                    let _ = db_providers::reset_backoff(conn, &connection.id);
                    let _ = db_providers::increment_usage(conn, &connection.id);
                    state.resilience_manager.record_success(&breaker_name);
                    state.resilience_manager.record_success(&model_breaker);

                    return Ok(ProxyOutput::Stream(stream));
                }
                Ok(ExecutorOutput::UpstreamError { status, body }) => {
                    // 上游返回非 2xx
                    let status_code = status;
                    log::warn!(
                        "上游返回 HTTP {} (attempt {}/{}): {}",
                        status_code,
                        attempt + 1,
                        self.retry_policy.max_retries() + 1,
                        body.chars().take(500).collect::<String>()
                    );
                    if !self.retry_policy.should_retry(attempt, Some(status_code)) {
                        // 不可重试：429 时增加退避时间，记录熔断失败
                        if status_code == 429 {
                            let retry_after = chrono::Utc::now() + chrono::Duration::seconds(60);
                            let _ = db_providers::increment_backoff(conn, &connection.id, Some(retry_after.to_rfc3339().as_str()));
                        }
                        state.resilience_manager.record_failure(&breaker_name);
                        // 模型级熔断：4xx（除 429）通常意味着"该模型不可用"
                        //（下线 / 无权限 / 不存在），比连接级更快触发快速失败
                        state.resilience_manager.record_failure_cfg(&model_breaker, 3, 60);
                        // bandit 评分记录失败
                        state.bandit_router.record_failure(&def.id, model, Some(&connection.id));
                        // 原样透传上游状态码与响应体，方便下游排查
                        return Err(AppError::Upstream { status: status_code, body });
                    }

                    // 可重试：指数退避等待后继续
                    let delay = self.retry_policy.delay_for_attempt(attempt);
                    tokio::time::sleep(delay).await;
                    last_err = Some(AppError::Upstream { status: status_code, body });
                }
                Err(e) => {
                    // 网络错误或其他异常
                    let err_chain = format!("{}", e);
                    log::warn!(
                        "上游请求失败 (attempt {}/{}): {} — provider: {}, connection: {}, model: {}",
                        attempt + 1,
                        self.retry_policy.max_retries() + 1,
                        err_chain,
                        def.id,
                        connection.id,
                        model
                    );
                    state.resilience_manager.record_failure(&breaker_name);
                    state.resilience_manager.record_failure_cfg(&model_breaker, 3, 60);
                    // bandit 评分记录失败
                    state.bandit_router.record_failure(&def.id, model, Some(&connection.id));
                    if !self.retry_policy.should_retry(attempt, None) {
                        return Err(e);
                    }
                    // 指数退避等待后继续
                    let delay = self.retry_policy.delay_for_attempt(attempt);
                    tokio::time::sleep(delay).await;
                    last_err = Some(e);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| AppError::Provider("All retries exhausted".into())))
    }
}

/// 从响应体中提取用量信息（输入/输出 token 数与费用）
///
/// 根据不同的 API 格式从响应 JSON 中提取对应的用量字段：
/// - `anthropic`：`usage.input_tokens` / `usage.output_tokens`
/// - `gemini`：`usageMetadata.promptTokenCount` / `usageMetadata.candidatesTokenCount`
/// - 其他（OpenAI）：`usage.prompt_tokens` / `usage.completion_tokens`
///
/// 上游未返回用量时，按响应中的文本内容估算输出 token 并标记 `estimated=true`，
/// 输入 token 由调用方按请求体估算回填。
///
/// # 参数
/// - `body`：响应 JSON 体
/// - `api_format`：API 格式标识
///
/// # 返回
/// `UsageExtract` 结构体，包含输入/输出/缓存/推理 token 数、费用与是否估算标志
fn extract_usage_from_response(body: &serde_json::Value, api_format: &str) -> UsageExtract {
    let usage = body.get("usage");
    let (tokens_in, tokens_out, cache_creation, cache_read, reasoning) = match api_format {
        "anthropic" => {
            let input = usage
                .and_then(|u| u.get("input_tokens"))
                .or_else(|| usage.and_then(|u| u.get("prompt_tokens")))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let output = usage
                .and_then(|u| u.get("output_tokens"))
                .or_else(|| usage.and_then(|u| u.get("completion_tokens")))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let cc = usage
                .and_then(|u| u.get("cache_creation_input_tokens"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let cr = usage
                .and_then(|u| u.get("cache_read_input_tokens"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            (input, output, cc, cr, 0)
        }
        "gemini" => {
            let meta = body.get("usageMetadata");
            let input = meta
                .and_then(|u| u.get("promptTokenCount"))
                .or_else(|| usage.and_then(|u| u.get("prompt_tokens")))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let output = meta
                .and_then(|u| u.get("candidatesTokenCount"))
                .or_else(|| usage.and_then(|u| u.get("completion_tokens")))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            (input, output, 0, 0, 0)
        }
        _ => {
            let input = usage
                .and_then(|u| u.get("prompt_tokens"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let output = usage
                .and_then(|u| u.get("completion_tokens"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let reasoning = usage
                .and_then(|u| u.get("completion_tokens_details"))
                .and_then(|d| d.get("reasoning_tokens"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            (input, output, 0, 0, reasoning)
        }
    };

    // 上游返回了真实用量
    if tokens_in > 0 || tokens_out > 0 {
        return UsageExtract {
            input_tokens: tokens_in,
            output_tokens: tokens_out,
            cache_creation,
            cache_read,
            reasoning,
            cost: 0.0,
            estimated: false,
        };
    }

    // 上游未返回用量：按响应文本内容估算输出 token（输入由调用方按请求体回填）
    let text = body
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("");
    if !text.is_empty() {
        return UsageExtract {
            input_tokens: 0,
            output_tokens: sse::estimate_tokens_from_text(text),
            estimated: true,
            ..Default::default()
        };
    }

    // 费用暂未实现，固定返回 0.0
    UsageExtract {
        input_tokens: tokens_in,
        output_tokens: tokens_out,
        cache_creation,
        cache_read,
        reasoning,
        cost: 0.0,
        estimated: false,
    }
}

/// 按请求体估算输入 token 数。
///
/// 遍历 `messages` 数组中所有消息的文本内容（string content 或
/// content 数组中的 text 部分）与顶层 `system` 字段，按字符数粗略估算。
/// 仅在上游未返回真实用量时作为兜底使用。
///
/// # 参数
/// - `body`：OpenAI 格式请求体
///
/// # 返回
/// 估算的输入 token 数
fn estimate_request_tokens(body: &serde_json::Value) -> i64 {
    let mut text = String::new();
    // system 提示词（string 或 content 数组）
    if let Some(sys) = body.get("system") {
        collect_text(sys, &mut text);
    }
    if let Some(messages) = body.get("messages").and_then(|m| m.as_array()) {
        for msg in messages {
            if let Some(content) = msg.get("content") {
                collect_text(content, &mut text);
            }
        }
    }
    if text.is_empty() {
        return 0;
    }
    sse::estimate_tokens_from_text(&text)
}

/// 递归收集 JSON 值中的文本内容到缓冲区。
///
/// 支持 string（直接追加）与数组（遍历其中的 text / thinking 字段）两种形态，
/// 对应 OpenAI 请求体中 content 的 string 与分段两种写法。
fn collect_text(v: &serde_json::Value, out: &mut String) {
    match v {
        serde_json::Value::String(s) => out.push_str(s),
        serde_json::Value::Array(arr) => {
            for item in arr {
                for key in ["text", "thinking"] {
                    if let Some(t) = item.get(key).and_then(|t| t.as_str()) {
                        out.push_str(t);
                    }
                }
            }
        }
        _ => {}
    }
}

use crate::db::api_keys as db_api_keys;
