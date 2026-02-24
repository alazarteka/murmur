use anyhow::Result;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::{fs, path::Path};

#[derive(Debug, Clone, Serialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub text: String,
    pub created_at: String,
    pub duration_ms: Option<i64>,
    pub model: String,
}

pub fn init(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let conn = Connection::open(path)?;
    conn.execute_batch(
        r#"
        PRAGMA journal_mode=WAL;
        PRAGMA synchronous=NORMAL;
        PRAGMA foreign_keys=ON;
        PRAGMA busy_timeout=2500;

        CREATE TABLE IF NOT EXISTS transcriptions (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            text        TEXT NOT NULL,
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            duration_ms INTEGER,
            model       TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_transcriptions_created_at
          ON transcriptions(created_at DESC);

        CREATE VIRTUAL TABLE IF NOT EXISTS transcriptions_fts
          USING fts5(text, content=transcriptions, content_rowid=id);

        CREATE TRIGGER IF NOT EXISTS transcriptions_ai AFTER INSERT ON transcriptions BEGIN
          INSERT INTO transcriptions_fts(rowid, text) VALUES (new.id, new.text);
        END;

        CREATE TRIGGER IF NOT EXISTS transcriptions_ad AFTER DELETE ON transcriptions BEGIN
          INSERT INTO transcriptions_fts(transcriptions_fts, rowid, text)
          VALUES ('delete', old.id, old.text);
        END;

        CREATE TRIGGER IF NOT EXISTS transcriptions_au AFTER UPDATE ON transcriptions BEGIN
          INSERT INTO transcriptions_fts(transcriptions_fts, rowid, text)
          VALUES ('delete', old.id, old.text);
          INSERT INTO transcriptions_fts(rowid, text) VALUES (new.id, new.text);
        END;
        "#,
    )?;

    Ok(())
}

pub fn insert(path: &Path, text: &str, duration_ms: i64, model: &str) -> Result<i64> {
    let conn = Connection::open(path)?;
    conn.execute(
        "INSERT INTO transcriptions (text, duration_ms, model) VALUES (?1, ?2, ?3)",
        params![text, duration_ms, model],
    )?;

    Ok(conn.last_insert_rowid())
}

pub fn list(path: &Path, limit: i64) -> Result<Vec<HistoryEntry>> {
    let conn = Connection::open(path)?;
    let mut stmt = conn.prepare(
        "SELECT id, text, created_at, duration_ms, model
         FROM transcriptions
         ORDER BY created_at DESC
         LIMIT ?1",
    )?;

    let rows = stmt.query_map([limit], |row| {
        Ok(HistoryEntry {
            id: row.get(0)?,
            text: row.get(1)?,
            created_at: row.get(2)?,
            duration_ms: row.get(3)?,
            model: row.get(4)?,
        })
    })?;

    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

pub fn delete(path: &Path, id: i64) -> Result<()> {
    let conn = Connection::open(path)?;
    conn.execute("DELETE FROM transcriptions WHERE id = ?1", [id])?;
    Ok(())
}
