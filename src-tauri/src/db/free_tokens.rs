use rusqlite::{params, Row};
use crate::db::models::{CreateFreeTokenSiteRequest, FreeTokenSite};
use crate::error::{AppError, Result};

const SELECT_COLS: &str = "id, name, home_url, apply_url, api_supported, api_base, api_format, \
     free_quota, region, requires_card, requires_verify, tags, note, provider_id, source, \
     submitter, sort_order, created_at, updated_at";

/// 用户提交的推荐排在内置条目之后
const USER_SORT_ORDER: i32 = 9000;

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

pub fn create(conn: &rusqlite::Connection, req: &CreateFreeTokenSiteRequest) -> Result<FreeTokenSite> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let tags = serde_json::to_string(&req.tags.clone().unwrap_or(serde_json::json!([])))?;

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
pub fn delete_user_site(conn: &rusqlite::Connection, id: &str) -> Result<bool> {
    let site = get_by_id(conn, id)?;
    match site {
        Some(s) if s.source == "user" => {
            let rows = conn.execute("DELETE FROM free_token_sites WHERE id = ?1", params![id])?;
            Ok(rows > 0)
        }
        Some(_) => Err(AppError::BadRequest("内置站点不可删除".to_string())),
        None => Ok(false),
    }
}

fn row_to_site(row: &Row) -> rusqlite::Result<FreeTokenSite> {
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
