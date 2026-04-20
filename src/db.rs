use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::PathBuf;

pub struct Db {
    conn: Connection,
}

pub struct DbMessage {
    pub role: String,
    pub content: String,
    pub model_label: Option<String>,
}

impl Db {
    pub fn open() -> Result<Self> {
        let path = db_path();
        std::fs::create_dir_all(path.parent().unwrap())?;
        let conn = Connection::open(&path)?;
        let db = Self { conn };
        db.init_schema()?;
        db.migrate_legacy_files();
        Ok(db)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS settings (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS input_history (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                entry      TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS conversations (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                name       TEXT NOT NULL UNIQUE,
                model      TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS messages (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                conversation_id INTEGER NOT NULL REFERENCES conversations(id),
                role            TEXT NOT NULL,
                content         TEXT NOT NULL,
                model_label     TEXT,
                created_at      INTEGER NOT NULL
            );",
        )?;
        Ok(())
    }

    fn migrate_legacy_files(&self) {
        let dir = crate::config::pantheon_dir();

        // ~/.pantheon/history → input_history table
        let history_path = dir.join("history");
        if history_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&history_path) {
                for entry in content
                    .lines()
                    .filter(|l| l.starts_with('+'))
                    .map(|l| &l[1..])
                    .filter(|l| !l.is_empty())
                {
                    let _ = self.append_input_history(entry);
                }
            }
            let _ = std::fs::remove_file(&history_path);
        }

        // ~/.pantheon/settings.toml → settings table
        let settings_path = dir.join("settings.toml");
        if settings_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&settings_path) {
                if let Ok(table) = toml::from_str::<toml::Table>(&content) {
                    if let Some(v) = table.get("last_model").and_then(|v| v.as_str()) {
                        let _ = self.set_setting("last_model", v);
                    }
                }
            }
            let _ = std::fs::remove_file(&settings_path);
        }
    }

    pub fn get_setting(&self, key: &str) -> Option<String> {
        self.conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .ok()
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn append_input_history(&self, entry: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO input_history (entry, created_at) VALUES (?1, ?2)",
            params![entry, now()],
        )?;
        Ok(())
    }

    pub fn load_input_history(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT entry FROM input_history ORDER BY id ASC")?;
        let entries = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(entries)
    }

    pub fn save_conversation(&self, name: &str, model: &str, messages: &[DbMessage]) -> Result<()> {
        let n = now();
        self.conn.execute(
            "INSERT INTO conversations (name, model, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?3)
             ON CONFLICT(name) DO UPDATE SET model = excluded.model, updated_at = excluded.updated_at",
            params![name, model, n],
        )?;
        let conv_id: i64 = self.conn.query_row(
            "SELECT id FROM conversations WHERE name = ?1",
            params![name],
            |row| row.get(0),
        )?;
        self.conn.execute(
            "DELETE FROM messages WHERE conversation_id = ?1",
            params![conv_id],
        )?;
        for msg in messages {
            self.conn.execute(
                "INSERT INTO messages (conversation_id, role, content, model_label, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![conv_id, msg.role, msg.content, msg.model_label, n],
            )?;
        }
        Ok(())
    }

    pub fn load_conversation(&self, name: &str) -> Result<(String, Vec<DbMessage>)> {
        let (conv_id, model): (i64, String) = self.conn.query_row(
            "SELECT id, model FROM conversations WHERE name = ?1",
            params![name],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let mut stmt = self.conn.prepare(
            "SELECT role, content, model_label FROM messages
             WHERE conversation_id = ?1 ORDER BY id ASC",
        )?;
        let messages = stmt
            .query_map(params![conv_id], |row| {
                Ok(DbMessage {
                    role: row.get(0)?,
                    content: row.get(1)?,
                    model_label: row.get(2)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok((model, messages))
    }

    pub fn list_conversations(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name FROM conversations ORDER BY updated_at DESC")?;
        let names = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(names)
    }
}

fn db_path() -> PathBuf {
    crate::config::pantheon_dir().join("pantheon.db")
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
