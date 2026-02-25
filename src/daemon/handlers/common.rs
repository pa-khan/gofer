use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

use crate::cache::CacheManager;
use crate::error_recovery::CircuitBreaker;
use crate::indexer::{EmbedderPool, Reranker};
use crate::languages::{rust_analyzer::RustAnalyzer, LanguageService};
use crate::storage::{LanceStorage, SqliteStorage};

/// Context for executing tools — Arc-wrapped resources for cloning across async tasks.
#[derive(Clone)]
pub struct ToolContext {
    pub sqlite: Arc<SqliteStorage>,
    pub lance: Arc<Mutex<LanceStorage>>,
    pub embedder: Arc<EmbedderPool>,
    pub reranker: Arc<Option<Reranker>>,
    pub root_path: Arc<PathBuf>,
    pub cache: Arc<CacheManager>,
    pub embedding_circuit: Arc<CircuitBreaker>,
    pub vector_circuit: Arc<CircuitBreaker>,
    #[allow(dead_code)]
    pub rust_analyzer: Arc<RwLock<Option<Arc<RustAnalyzer>>>>,
    /// Language-specific services (Vue, TypeScript, Python, etc.)
    pub language_services: Arc<Vec<Box<dyn LanguageService>>>,
}

impl ToolContext {
    /// Get or initialize rust-analyzer for this project.
    #[allow(dead_code)]
    pub async fn get_rust_analyzer(&self) -> anyhow::Result<Arc<RustAnalyzer>> {
        // Fast path: already initialized
        {
            let ra_guard = self.rust_analyzer.read().await;
            if let Some(ra) = ra_guard.as_ref() {
                if ra.is_ready().await {
                    return Ok(ra.clone());
                }
            }
        }

        // Slow path: initialize rust-analyzer
        let mut ra_guard = self.rust_analyzer.write().await;

        // Double-check in case another task initialized it
        if let Some(ra) = ra_guard.as_ref() {
            if ra.is_ready().await {
                return Ok(ra.clone());
            }
        }

        // Start new instance
        let ra = Arc::new(RustAnalyzer::new(self.root_path.as_ref().clone()));
        ra.start().await?;
        *ra_guard = Some(ra.clone());

        Ok(ra)
    }
}

/// Резолвинг пути: если путь относительный, превращает в абсолютный через root_path.
pub fn resolve_path(root: &Path, file: &str) -> String {
    let p = Path::new(file);
    if p.is_absolute() {
        file.to_string()
    } else {
        root.join(file).to_string_lossy().to_string()
    }
}

/// Резолвинг пути в PathBuf для file operations
pub fn resolve_path_buf(root: &Path, file: &str) -> PathBuf {
    let p = Path::new(file);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        root.join(file)
    }
}

/// Strip root_path prefix from an absolute file path, returning a relative path.
pub fn make_relative(root: &Path, abs_path: &str) -> String {
    Path::new(abs_path)
        .strip_prefix(root)
        .ok()
        .and_then(|p| p.to_str())
        .unwrap_or(abs_path)
        .to_string()
}

/// make_relative для PathBuf
pub fn make_relative_pathbuf(root: &Path, abs_path: &Path) -> String {
    abs_path
        .strip_prefix(root)
        .ok()
        .and_then(|p| p.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| abs_path.to_string_lossy().to_string())
}
