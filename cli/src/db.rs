use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use std::path::PathBuf;

pub struct Entry {
    pub id: String,
    pub date: String,
    pub file_path: String,
    pub duration_secs: u64,
    pub transcript: String,
    pub created_at: String,
}

fn db_path() -> Result<PathBuf> {
    let home = dirs_home()?;
    Ok(home.join(".your-voice").join("journal.db"))
}

fn dirs_home() -> Result<PathBuf> {
    std::env::var("HOME")
        .map(PathBuf::from)
        .context("HOME environment variable not set")
}

pub fn init() -> Result<Connection> {
    let path = db_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    // Also ensure media directory exists
    let home = dirs_home()?;
    let media_dir = home.join(".your-voice").join("media");
    std::fs::create_dir_all(&media_dir)
        .with_context(|| format!("Failed to create media directory: {}", media_dir.display()))?;

    let conn = Connection::open(&path)
        .with_context(|| format!("Failed to open database: {}", path.display()))?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS entries (
            id           TEXT PRIMARY KEY,
            date         TEXT NOT NULL,
            file_path    TEXT NOT NULL,
            duration_secs INTEGER NOT NULL,
            transcript   TEXT NOT NULL,
            created_at   TEXT NOT NULL
        );",
    )
    .context("Failed to create entries table")?;

    Ok(conn)
}

pub fn insert_entry(
    conn: &Connection,
    file_path: &str,
    duration_secs: u64,
    transcript: &str,
) -> Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Local::now();
    let date = now.format("%Y-%m-%d").to_string();
    let created_at = now.to_rfc3339();

    conn.execute(
        "INSERT INTO entries (id, date, file_path, duration_secs, transcript, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, date, file_path, duration_secs as i64, transcript, created_at],
    )
    .context("Failed to insert entry")?;

    Ok(id)
}

/// Fetch all entries where date is between start_date and end_date (inclusive, YYYY-MM-DD).
pub fn get_entries_for_range(conn: &Connection, start_date: &str, end_date: &str) -> Result<Vec<Entry>> {
    let mut stmt = conn.prepare(
        "SELECT id, date, file_path, duration_secs, transcript, created_at
         FROM entries
         WHERE date >= ?1 AND date <= ?2
         ORDER BY created_at ASC",
    )?;

    let entries = stmt
        .query_map(params![start_date, end_date], |row| {
            Ok(Entry {
                id: row.get(0)?,
                date: row.get(1)?,
                file_path: row.get(2)?,
                duration_secs: row.get::<_, i64>(3)? as u64,
                transcript: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to query entries")?;

    Ok(entries)
}

pub fn get_entries(conn: &Connection, date: &str) -> Result<Vec<Entry>> {
    let mut stmt = conn.prepare(
        "SELECT id, date, file_path, duration_secs, transcript, created_at
         FROM entries
         WHERE date = ?1
         ORDER BY created_at ASC",
    )?;

    let entries = stmt
        .query_map(params![date], |row| {
            Ok(Entry {
                id: row.get(0)?,
                date: row.get(1)?,
                file_path: row.get(2)?,
                duration_secs: row.get::<_, i64>(3)? as u64,
                transcript: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to query entries")?;

    Ok(entries)
}
