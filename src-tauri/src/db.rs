use anyhow::Result;
use rusqlite::{params, Connection, ErrorCode};
use serde::Serialize;
use std::{fs, path::Path, thread, time::Duration};

#[derive(Debug, Clone, Serialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub text: String,
    pub created_at: String,
    pub duration_ms: Option<i64>,
    pub model: String,
}

const BUSY_TIMEOUT_MS: u64 = 2_500;
const MAX_RETRIES: usize = 5;
const RETRY_BACKOFF_MS: [u64; MAX_RETRIES] = [25, 50, 100, 200, 400];

fn open_connection(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.busy_timeout(Duration::from_millis(BUSY_TIMEOUT_MS))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "FULL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(conn)
}

fn is_retryable(err: &rusqlite::Error) -> bool {
    matches!(
        err,
        rusqlite::Error::SqliteFailure(sql_err, _)
            if matches!(sql_err.code, ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked)
    )
}

fn with_retry<T, F>(mut op: F) -> Result<T>
where
    F: FnMut() -> rusqlite::Result<T>,
{
    for attempt in 0..=MAX_RETRIES {
        match op() {
            Ok(value) => return Ok(value),
            Err(err) if attempt < MAX_RETRIES && is_retryable(&err) => {
                thread::sleep(Duration::from_millis(RETRY_BACKOFF_MS[attempt]));
            }
            Err(err) => return Err(err.into()),
        }
    }

    unreachable!("retry loop should always return");
}

pub fn init(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let conn = open_connection(path)?;
    conn.execute_batch(
        r#"
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
    with_retry(|| {
        let mut conn = open_connection(path)?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO transcriptions (text, duration_ms, model) VALUES (?1, ?2, ?3)",
            params![text, duration_ms, model],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(id)
    })
}

pub fn list(path: &Path, limit: i64) -> Result<Vec<HistoryEntry>> {
    with_retry(|| {
        let conn = open_connection(path)?;
        let mut stmt = conn.prepare(
            "SELECT id, text, created_at, duration_ms, model
             FROM transcriptions
             ORDER BY id DESC
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

        rows.collect::<std::result::Result<Vec<_>, _>>()
    })
}

pub fn delete(path: &Path, id: i64) -> Result<()> {
    with_retry(|| {
        let conn = open_connection(path)?;
        conn.execute("DELETE FROM transcriptions WHERE id = ?1", [id])?;
        Ok(())
    })
}
