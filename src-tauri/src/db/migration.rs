use rusqlite::{Connection, params};
use crate::error::Result;

const MIGRATIONS: &[(&str, &str)] = &[
    ("001", include_str!("migrations/001_initial.sql")),
];

pub fn run_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _vortex_migrations (
            version TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );"
    )?;

    for (version, sql) in MIGRATIONS {
        let exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM _vortex_migrations WHERE version = ?1",
            params![version],
            |row| row.get(0),
        )?;

        if !exists {
            conn.execute_batch(sql)?;
            conn.execute(
                "INSERT INTO _vortex_migrations (version, name) VALUES (?1, ?2)",
                params![version, format!("migration_{}", version)],
            )?;
        }
    }

    Ok(())
}
