use rusqlite::{params, Connection};
use serde::Serialize;
use std::sync::Mutex;

pub struct HistoryDb(pub Mutex<Connection>);

#[derive(Serialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub timestamp: String,
    pub text: String,
    pub language: String,
    pub app_name: Option<String>,
}

pub fn open() -> anyhow::Result<Connection> {
    let dir = dirs_next::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("DictaFlow");
    std::fs::create_dir_all(&dir)?;
    let conn = Connection::open(dir.join("history.sqlite3"))?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            text TEXT NOT NULL,
            language TEXT NOT NULL,
            app_name TEXT
        )",
        [],
    )?;
    Ok(conn)
}

pub fn insert(conn: &Connection, text: &str, language: &str, app_name: Option<&str>) -> anyhow::Result<()> {
    conn.execute(
        "INSERT INTO history (timestamp, text, language, app_name) VALUES (?1, ?2, ?3, ?4)",
        params![chrono::Utc::now().to_rfc3339(), text, language, app_name],
    )?;
    Ok(())
}

pub fn search(conn: &Connection, query: &str) -> anyhow::Result<Vec<HistoryEntry>> {
    let like = format!("%{}%", query);
    let mut stmt = conn.prepare(
        "SELECT id, timestamp, text, language, app_name FROM history
         WHERE text LIKE ?1 ORDER BY id DESC LIMIT 200",
    )?;
    let rows = stmt.query_map(params![like], |row| {
        Ok(HistoryEntry {
            id: row.get(0)?,
            timestamp: row.get(1)?,
            text: row.get(2)?,
            language: row.get(3)?,
            app_name: row.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn clear(conn: &Connection) -> anyhow::Result<()> {
    conn.execute("DELETE FROM history", [])?;
    Ok(())
}
