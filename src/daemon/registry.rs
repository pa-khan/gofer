//! Project registry — tracks registered projects in a global SQLite database.

use std::path::Path;

use anyhow::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use sqlx::Row;

/// A record in the project registry.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ProjectRecord {
    pub id: String,
    pub path: String,
    pub name: String,
    pub created_at: i64,
    pub last_opened: i64,
}

/// Lightweight wrapper around a SQLite pool for the global registry.
#[derive(Clone)]
pub struct RegistryDb {
    pool: SqlitePool,
}

impl RegistryDb {
    /// Open (or create) the registry database at the given path.
    pub async fn new(db_path: &str) -> Result<Self> {
        if let Some(parent) = Path::new(db_path).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let opts = SqliteConnectOptions::new()
            .filename(db_path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

        let pool = SqlitePoolOptions::new()
            .max_connections(2)
            .connect_with(opts)
            .await?;

        let db = Self { pool };
        db.migrate().await?;
        Ok(db)
    }

    async fn migrate(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS projects (
                id          TEXT PRIMARY KEY,
                path        TEXT NOT NULL UNIQUE,
                name        TEXT NOT NULL,
                created_at  INTEGER NOT NULL,
                last_opened INTEGER NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Register a project. Returns the UUID (new or existing).
    pub async fn register(&self, abs_path: &str) -> Result<String> {
        // Check if already registered
        if let Some(existing) = self.get_by_path(abs_path).await? {
            self.update_last_opened(&existing.id).await?;
            return Ok(existing.id);
        }

        let id = uuid::Uuid::new_v4().to_string();
        let name = Path::new(abs_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed")
            .to_string();
        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            "INSERT INTO projects (id, path, name, created_at, last_opened) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(abs_path)
        .bind(&name)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    /// Look up a project by its absolute path.
    pub async fn get_by_path(&self, abs_path: &str) -> Result<Option<ProjectRecord>> {
        let row = sqlx::query(
            "SELECT id, path, name, created_at, last_opened FROM projects WHERE path = ?",
        )
        .bind(abs_path)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| ProjectRecord {
            id: r.get("id"),
            path: r.get("path"),
            name: r.get("name"),
            created_at: r.get("created_at"),
            last_opened: r.get("last_opened"),
        }))
    }

    /// List all registered projects.
    pub async fn list(&self) -> Result<Vec<ProjectRecord>> {
        let rows = sqlx::query(
            "SELECT id, path, name, created_at, last_opened FROM projects ORDER BY last_opened DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| ProjectRecord {
                id: r.get("id"),
                path: r.get("path"),
                name: r.get("name"),
                created_at: r.get("created_at"),
                last_opened: r.get("last_opened"),
            })
            .collect())
    }

    /// Update the last_opened timestamp.
    pub async fn update_last_opened(&self, project_id: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        sqlx::query("UPDATE projects SET last_opened = ? WHERE id = ?")
            .bind(now)
            .bind(project_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Remove a project from the registry (does NOT delete index files).
    #[allow(dead_code)]
    pub async fn remove(&self, project_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM projects WHERE id = ?")
            .bind(project_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
