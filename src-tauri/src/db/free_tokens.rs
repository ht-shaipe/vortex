//! 免费 Token 站点管理模块。
//!
//! 负责对 `free_token_sites` 表的增删改查。支持用户提交站点推荐，
//! 用户提交的站点排序权重固定为 9000（排在内置条目之后），且仅允许删除用户自己提交的条目。

use rusqlite::{params, Row};
use crate::db::models::{CreateFreeTokenSiteRequest, FreeTokenSite};
use crate::error::{AppError, Result};

/// 查询列名常量，供各查询复用
const SELECT_COLS: &str = "id, name, home_url, apply_url, api_supported, api_base, api_format, \
     free_quota, region, requires_card, requires_verify, tags, note, provider_id, source, \
     submitter, sort_order, created_at, updated_at";

/// 用户提交的推荐排在内置条目之后
const USER_SORT_ORDER: i32 = 9000;

/// 查询所有免费 Token 站点列表。
///
/// 按排序权重升序、名称升序排列。
///
/// # 参数
/// - `conn`：数据库连接
///
/// # 返回
/// 所有站点的列表
pub fn list(conn: &rusqlite::Connection) -> Result<Vec<FreeTokenSite>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {} FROM free_token_sites ORDER BY sort_order ASC, name ASC",
        SELECT_COLS
    ))?;
    let rows = stmt.query_map([], row_to_site)?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// 按 ID 查询单个免费 Token 站点。
///
/// # 参数
/// - `conn`：数据库连接
/// - `id`：站点唯一标识
///
/// # 返回
/// 找到返回 `Some(站点)`，未找到返回 `None`
pub fn get_by_id(conn: &rusqlite::Connection, id: &str) -> Result<Option<FreeTokenSite>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {} FROM free_token_sites WHERE id = ?1",
        SELECT_COLS
    ))?;
    let mut rows = stmt.query_map(params![id], row_to_site)?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

/// 创建新的免费 Token 站点（用户提交）。
///
/// 生成 UUID 作为主键，对可选字段填充默认值（如 `region` 默认 `global`、
/// `api_supported` 默认 `true`），`source` 固定为 `"user"`，
/// 排序权重固定为 `USER_SORT_ORDER`（9000）。
///
/// # 参数
/// - `conn`：数据库连接
/// - `req`：创建请求参数
///
/// # 返回
/// 创建成功的站点实体
pub fn create(conn: &rusqlite::Connection, req: &CreateFreeTokenSiteRequest) -> Result<FreeTokenSite> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let tags = serde_json::to_string(&req.tags.clone().unwrap_or(serde_json::json!([])))?;

    // 组装站点实体，填充默认值
    let site = FreeTokenSite {
        id: id.clone(),
        name: req.name.trim().to_string(),
        home_url: req.home_url.clone().unwrap_or_default(),
        apply_url: req.apply_url.clone().unwrap_or_default(),
        api_supported: req.api_supported.unwrap_or(true),
        api_base: req.api_base.clone().filter(|s| !s.trim().is_empty()),
        api_format: req.api_format.clone().filter(|s| !s.trim().is_empty()),
        free_quota: req.free_quota.clone().unwrap_or_default(),
        region: req.region.clone().unwrap_or_else(|| "global".to_string()),
        requires_card: req.requires_card.unwrap_or(false),
        requires_verify: req.requires_verify.unwrap_or(false),
        tags: req.tags.clone().unwrap_or(serde_json::json!([])),
        note: req.note.clone().filter(|s| !s.trim().is_empty()),
        provider_id: req.provider_id.clone().filter(|s| !s.trim().is_empty()),
        source: "user".to_string(),
        submitter: req.submitter.clone().filter(|s| !s.trim().is_empty()),
        sort_order: USER_SORT_ORDER,
        created_at: now.clone(),
        updated_at: now,
    };

    // 插入数据库
    conn.execute(
        "INSERT INTO free_token_sites (id, name, home_url, apply_url, api_supported, api_base, \
         api_format, free_quota, region, requires_card, requires_verify, tags, note, provider_id, \
         source, submitter, sort_order, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
        params![
            site.id,
            site.name,
            site.home_url,
            site.apply_url,
            site.api_supported as i32,
            site.api_base,
            site.api_format,
            site.free_quota,
            site.region,
            site.requires_card as i32,
            site.requires_verify as i32,
            tags,
            site.note,
            site.provider_id,
            site.source,
            site.submitter,
            site.sort_order,
            site.created_at,
            site.updated_at,
        ],
    )?;

    Ok(site)
}

/// 仅允许删除用户自己提交的推荐，内置条目不可删
///
/// # 参数
/// - `conn`：数据库连接
/// - `id`：站点唯一标识
///
/// # 返回
/// 删除成功返回 `true`；站点不存在返回 `false`；尝试删除内置站点返回 `AppError::BadRequest`
pub fn delete_user_site(conn: &rusqlite::Connection, id: &str) -> Result<bool> {
    let site = get_by_id(conn, id)?;
    match site {
        Some(s) if s.source == "user" => {
            // 用户提交的站点：允许删除
            let rows = conn.execute("DELETE FROM free_token_sites WHERE id = ?1", params![id])?;
            Ok(rows > 0)
        }
        Some(_) => Err(AppError::BadRequest("内置站点不可删除".to_string())),
        None => Ok(false),
    }
}

/// 将数据库行映射为 `FreeTokenSite` 实体。
///
/// 解析 tags JSON 字段，将布尔列从整数还原。
///
/// # 参数
/// - `row`：数据库行引用
///
/// # 返回
/// 映射后的站点实体（rusqlite 错误类型）
fn row_to_site(row: &Row) -> rusqlite::Result<FreeTokenSite> {
    // 读取 tags JSON 字符串
    let tags_str: String = row.get(11)?;

    Ok(FreeTokenSite {
        id: row.get(0)?,
        name: row.get(1)?,
        home_url: row.get(2)?,
        apply_url: row.get(3)?,
        api_supported: row.get::<_, i32>(4)? != 0,
        api_base: row.get(5)?,
        api_format: row.get(6)?,
        free_quota: row.get(7)?,
        region: row.get(8)?,
        requires_card: row.get::<_, i32>(9)? != 0,
        requires_verify: row.get::<_, i32>(10)? != 0,
        // 解析 tags JSON，失败时回退为空数组
        tags: serde_json::from_str(&tags_str).unwrap_or(serde_json::json!([])),
        note: row.get(12)?,
        provider_id: row.get(13)?,
        source: row.get(14)?,
        submitter: row.get(15)?,
        sort_order: row.get(16)?,
        created_at: row.get(17)?,
        updated_at: row.get(18)?,
    })
}
