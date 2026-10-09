use crate::error::IdeResult;
use crate::models::GenerationRecord;
use rusqlite::{params, Connection};
use std::path::Path;

pub struct GenerationStore {
    conn: Connection,
}

impl GenerationStore {
    pub fn open<P: AsRef<Path>>(db_path: P) -> IdeResult<Self> {
        let conn = Connection::open(db_path)?;
        let store = Self { conn };
        store.init_schema()?;
        Ok(store)
    }

    pub fn in_memory() -> IdeResult<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> IdeResult<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS generation_history (
                id TEXT PRIMARY KEY,
                timestamp TEXT NOT NULL,
                spec_version_hash TEXT NOT NULL,
                plan_summary TEXT NOT NULL,
                patches_json TEXT NOT NULL,
                git_base_commit TEXT,
                status TEXT NOT NULL
            )",
            [],
        )?;
        Ok(())
    }

    pub fn record_generation(&self, record: &GenerationRecord) -> IdeResult<()> {
        let patches_json = serde_json::to_string(&record.patches)?;
        self.conn.execute(
            "INSERT INTO generation_history (id, timestamp, spec_version_hash, plan_summary, patches_json, git_base_commit, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                record.id,
                record.timestamp.to_rfc3339(),
                record.spec_version_hash,
                record.plan_summary,
                patches_json,
                record.git_base_commit,
                record.status,
            ],
        )?;
        Ok(())
    }

    pub fn list_history(&self) -> IdeResult<Vec<GenerationRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, spec_version_hash, plan_summary, patches_json, git_base_commit, status 
             FROM generation_history ORDER BY timestamp DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            let patches_str: String = row.get(4)?;
            let patches = serde_json::from_str(&patches_str).unwrap_or_default();
            let timestamp_str: String = row.get(1)?;
            let timestamp = chrono::DateTime::parse_from_rfc3339(&timestamp_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());

            Ok(GenerationRecord {
                id: row.get(0)?,
                timestamp,
                spec_version_hash: row.get(2)?,
                plan_summary: row.get(3)?,
                patches,
                git_base_commit: row.get(5)?,
                status: row.get(6)?,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_and_retrieve_history() {
        let store = GenerationStore::in_memory().unwrap();
        let record = GenerationRecord {
            id: "gen-1".into(),
            timestamp: chrono::Utc::now(),
            spec_version_hash: "sha123".into(),
            plan_summary: "Added phone field to users API".into(),
            patches: vec![],
            git_base_commit: Some("commit1".into()),
            status: "applied".into(),
        };

        store.record_generation(&record).unwrap();
        let history = store.list_history().unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].id, "gen-1");
    }
}
