//! 模型别名（虚拟模型映射）CRUD 操作。

use crate::db::models::{CreateModelAlias, ModelAlias, ModelAliasTarget, UpdateModelAlias};
use rusqlite::{params, Connection};
use uuid::Uuid;

/// 行映射：从 DB 行构建 ModelAlias（含 source、sort_order 列）。
fn row_to_alias(row: &rusqlite::Row) -> rusqlite::Result<ModelAlias> {
    let targets_json: String = row.get(2)?;
    let targets: Vec<ModelAliasTarget> = serde_json::from_str(&targets_json).unwrap_or_default();
    Ok(ModelAlias {
        id: row.get(0)?,
        alias: row.get(1)?,
        targets,
        is_active: row.get::<_, i64>(3)? != 0,
        source: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        sort_order: row.get::<_, i64>(7).unwrap_or(0),
    })
}

const SELECT_COLS: &str =
    "id, alias, targets, is_active, source, created_at, updated_at, sort_order";

/// 列出所有模型别名，按 sort_order DESC、alias ASC 排序。
pub fn list(conn: &Connection) -> rusqlite::Result<Vec<ModelAlias>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SELECT_COLS} FROM model_aliases ORDER BY sort_order DESC, alias ASC"
    ))?;
    let rows = stmt.query_map([], row_to_alias)?;
    rows.collect()
}

/// 按 alias 查找模型别名。
pub fn get_by_alias(conn: &Connection, alias: &str) -> rusqlite::Result<Option<ModelAlias>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SELECT_COLS} FROM model_aliases WHERE alias = ?1 AND is_active = 1"
    ))?;
    let mut rows = stmt.query_map(params![alias], row_to_alias)?;
    if let Some(row) = rows.next() {
        Ok(Some(row?))
    } else {
        Ok(None)
    }
}

/// 创建模型别名。
pub fn create(conn: &Connection, req: &CreateModelAlias) -> rusqlite::Result<ModelAlias> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let targets_json = serde_json::to_string(&req.targets).unwrap_or_else(|_| "[]".to_string());
    conn.execute(
        "INSERT INTO model_aliases (id, alias, targets, is_active, source, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'manual', ?5, ?6)",
        params![id, req.alias, targets_json, req.is_active as i64, now, now],
    )?;
    Ok(ModelAlias {
        id,
        alias: req.alias.clone(),
        targets: req.targets.clone(),
        is_active: req.is_active,
        source: "manual".into(),
        created_at: now.clone(),
        updated_at: now,
        sort_order: 0,
    })
}

/// 更新模型别名。
pub fn update(
    conn: &Connection,
    id: &str,
    req: &UpdateModelAlias,
) -> rusqlite::Result<Option<ModelAlias>> {
    let now = chrono::Utc::now().to_rfc3339();
    if let Some(alias) = &req.alias {
        conn.execute(
            "UPDATE model_aliases SET alias = ?1, updated_at = ?2 WHERE id = ?3",
            params![alias, now, id],
        )?;
    }
    if let Some(targets) = &req.targets {
        let targets_json = serde_json::to_string(targets).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "UPDATE model_aliases SET targets = ?1, updated_at = ?2 WHERE id = ?3",
            params![targets_json, now, id],
        )?;
    }
    if let Some(is_active) = req.is_active {
        conn.execute(
            "UPDATE model_aliases SET is_active = ?1, updated_at = ?2 WHERE id = ?3",
            params![is_active as i64, now, id],
        )?;
    }
    if let Some(sort_order) = req.sort_order {
        conn.execute(
            "UPDATE model_aliases SET sort_order = ?1, updated_at = ?2 WHERE id = ?3",
            params![sort_order, now, id],
        )?;
    }
    let mut stmt = conn.prepare(&format!(
        "SELECT {SELECT_COLS} FROM model_aliases WHERE id = ?1"
    ))?;
    let mut rows = stmt.query_map(params![id], row_to_alias)?;
    if let Some(row) = rows.next() {
        Ok(Some(row?))
    } else {
        Ok(None)
    }
}

/// 删除模型别名。
pub fn delete(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM model_aliases WHERE id = ?1", params![id])?;
    Ok(())
}

/// 批量更新排序权重。接受 (id, sort_order) 列表，逐条更新。
pub fn batch_reorder(conn: &Connection, orders: &[(String, i64)]) -> rusqlite::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    for (id, sort_order) in orders {
        conn.execute(
            "UPDATE model_aliases SET sort_order = ?1, updated_at = ?2 WHERE id = ?3",
            params![sort_order, now, id],
        )?;
    }
    Ok(())
}

// ── 自动归纳操作 ──

/// 删除所有 source='auto' 的别名（重新归纳前调用）。
pub fn delete_all_auto(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM model_aliases WHERE source = 'auto'", [])?;
    Ok(())
}

/// 清理悬空别名 target：`connection_id` 不在现存连接集合中的 target 剔除。
///
/// 连接删除后其别名 target 悬空（delete_provider 只删连接行），
/// 悬空 target 会让别名路由到不存在的连接。剔除后 targets 为空的
/// auto 别名整条删除（自动归纳、可重建）；manual 别名保留条目本身
/// （用户手建，不代删）。`dry_run = true` 只统计不改动。
/// 返回 (受影响别名数, 剔除 target 数, 删除的 auto 别名数)。
pub fn cleanup_dangling_targets(
    conn: &Connection,
    live_conn_ids: &std::collections::HashSet<String>,
    dry_run: bool,
) -> rusqlite::Result<(usize, usize, usize)> {
    let aliases = list(conn)?;
    let mut affected = 0usize;
    let mut removed_targets = 0usize;
    let mut deleted_auto = 0usize;
    for alias in aliases {
        let before = alias.targets.len();
        let kept: Vec<ModelAliasTarget> = alias
            .targets
            .iter()
            .filter(|t| match &t.connection_id {
                // 无 connection_id 的 target 由路由时按 provider 自动选择，不算悬空
                None => true,
                Some(id) => live_conn_ids.contains(id),
            })
            .cloned()
            .collect();
        if kept.len() == before {
            continue;
        }
        removed_targets += before - kept.len();
        if kept.is_empty() && alias.source == "auto" {
            deleted_auto += 1;
            if !dry_run {
                delete(conn, &alias.id)?;
            }
            continue;
        }
        // manual 别名（含剔除后 targets 为空的）：保留条目，仅剔除悬空 target；
        // 空 targets 的 manual 条目由用户在 UI 自行处理，不代删
        affected += 1;
        if !dry_run {
            let targets_json = serde_json::to_string(&kept).unwrap_or_else(|_| "[]".to_string());
            conn.execute(
                "UPDATE model_aliases SET targets = ?1, updated_at = ?2 WHERE id = ?3",
                params![targets_json, chrono::Utc::now().to_rfc3339(), alias.id],
            )?;
        }
    }
    Ok((affected, removed_targets, deleted_auto))
}

/// 批量创建自动归纳的别名。跳过与现有 manual 别名同名的条目。
pub fn create_auto_batch(
    conn: &Connection,
    aliases: &[(String, Vec<ModelAliasTarget>)],
) -> rusqlite::Result<Vec<ModelAlias>> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut created = Vec::new();

    // 查询已有 manual 别名集合，避免覆盖
    let mut stmt = conn.prepare("SELECT alias FROM model_aliases WHERE source = 'manual'")?;
    let manual_aliases: std::collections::HashSet<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .collect();
    drop(stmt);

    for (alias, targets) in aliases {
        if manual_aliases.contains(alias) {
            continue;
        }
        let id = Uuid::new_v4().to_string();
        let targets_json = serde_json::to_string(targets).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "INSERT INTO model_aliases (id, alias, targets, is_active, source, created_at, updated_at) VALUES (?1, ?2, ?3, 1, 'auto', ?4, ?5)",
            params![id, alias, targets_json, now, now],
        )?;
        created.push(ModelAlias {
            id,
            alias: alias.clone(),
            targets: targets.clone(),
            is_active: true,
            source: "auto".into(),
            created_at: now.clone(),
            updated_at: now.clone(),
            sort_order: 0,
        });
    }

    Ok(created)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 悬空 target 剔除：auto 剔空整条删、manual 剔空保留、无悬空不动
    #[test]
    fn cleanup_dangling_targets_mixed() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migration::run_migrations(&conn).unwrap();
        let now = chrono::Utc::now().to_rfc3339();

        let insert = |alias: &str, source: &str, ids: &[&str]| {
            let targets: Vec<ModelAliasTarget> = ids
                .iter()
                .map(|id| ModelAliasTarget {
                    provider: "custom-openai".into(),
                    model: format!("m-{id}"),
                    connection_id: if *id == "none" {
                        None
                    } else {
                        Some(id.to_string())
                    },
                })
                .collect();
            conn.execute(
                "INSERT INTO model_aliases (id, alias, targets, is_active, source, created_at, updated_at, sort_order)
                 VALUES (?1, ?2, ?3, 1, ?4, ?5, ?5, 0)",
                params![
                    Uuid::new_v4().to_string(),
                    alias,
                    serde_json::to_string(&targets).unwrap(),
                    source,
                    now
                ],
            )
            .unwrap();
        };

        insert("auto-all-dead", "auto", &["dead1"]); // 剔空 → 整条删
        insert("auto-partial", "auto", &["live", "dead1"]); // 剔 1 留 1
        insert("manual-all-dead", "manual", &["dead2"]); // 剔空但 manual → 保留条目
        insert("no-conn-id", "auto", &["none"]); // 无 connection_id 不算悬空

        let mut live = std::collections::HashSet::new();
        live.insert("live".to_string());

        // 预览：只统计
        let (a, r, d) = cleanup_dangling_targets(&conn, &live, true).unwrap();
        assert_eq!(
            (a, r, d),
            (2, 3, 1),
            "受影响=auto-partial+manual-all-dead，剔3个target，删1条auto"
        );

        // 执行
        let (a, r, d) = cleanup_dangling_targets(&conn, &live, false).unwrap();
        assert_eq!((a, r, d), (2, 3, 1));

        let remaining: Vec<String> = list(&conn).unwrap().into_iter().map(|x| x.alias).collect();
        assert!(
            !remaining.contains(&"auto-all-dead".to_string()),
            "剔空的 auto 别名应被整条删除"
        );
        assert!(remaining.contains(&"auto-partial".to_string()));
        assert!(
            remaining.contains(&"manual-all-dead".to_string()),
            "manual 别名条目不代删"
        );
        assert!(remaining.contains(&"no-conn-id".to_string()));

        // auto-partial 仅剩 live target；manual-all-dead targets 已剔空
        for alias in list(&conn).unwrap() {
            if alias.alias == "auto-partial" {
                assert_eq!(alias.targets.len(), 1);
                assert_eq!(alias.targets[0].connection_id.as_deref(), Some("live"));
            }
            if alias.alias == "manual-all-dead" {
                assert!(alias.targets.is_empty());
            }
        }
    }
}
