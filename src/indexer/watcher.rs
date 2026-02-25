use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::time::Duration;

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode, DebounceEventResult};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::parser::SupportedLanguage;

/// Task for the indexer worker
#[derive(Debug, Clone)]
pub enum IndexTask {
    Reindex(PathBuf),
    Delete(PathBuf),
}

/// gofer configuration from config.toml
#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct goferConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub indexer: IndexerConfig,
    #[serde(default)]
    pub embedding: EmbeddingConfig,
    #[serde(default)]
    pub reranker: RerankerConfig,
    #[serde(default)]
    pub summarizer: SummarizerTomlConfig,
    #[serde(default)]
    pub domains: DomainsConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServerConfig {
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_port() -> u16 {
    10987
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: default_port(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct IndexerConfig {
    #[serde(default)]
    pub ignore: Vec<String>,
    #[serde(default)]
    pub parallel_workers: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EmbeddingConfig {
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// Embedding model name (fastembed model enum variant)
    #[serde(default = "default_embedding_model")]
    pub model: String,
    /// Cache directory for model files
    #[serde(default)]
    pub cache_dir: Option<String>,
    /// Number of embedder pool instances (1-8)
    #[serde(default = "default_pool_size")]
    pub pool_size: usize,
    /// Path to custom quantized ONNX model (INT8)
    /// If specified, will use UserDefinedEmbeddingModel instead of standard model
    #[serde(default)]
    pub quantized_model_path: Option<String>,
    /// Path to tokenizer.json for custom model
    #[serde(default)]
    pub tokenizer_path: Option<String>,
    /// Path to tokenizer_config.json for custom model
    #[serde(default)]
    pub tokenizer_config_path: Option<String>,
}

fn default_batch_size() -> usize {
    32
}
fn default_embedding_model() -> String {
    "BGESmallENV15".to_string()
}
fn default_pool_size() -> usize {
    4
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            batch_size: default_batch_size(),
            model: default_embedding_model(),
            cache_dir: None,
            pool_size: default_pool_size(),
            quantized_model_path: None,
            tokenizer_path: None,
            tokenizer_config_path: None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RerankerConfig {
    /// Enable reranker (default: true, auto-downloads model)
    #[serde(default = "default_reranker_enabled")]
    pub enabled: bool,
    /// Directory for reranker model files
    #[serde(default = "default_reranker_model_dir")]
    pub model_dir: String,
    /// URL for ONNX model file
    #[serde(default)]
    pub model_url: Option<String>,
    /// URL for tokenizer file
    #[serde(default)]
    pub tokenizer_url: Option<String>,
}

fn default_reranker_enabled() -> bool {
    true
}
fn default_reranker_model_dir() -> String {
    ".gofer/data/models/reranker".to_string()
}

impl Default for RerankerConfig {
    fn default() -> Self {
        Self {
            enabled: default_reranker_enabled(),
            model_dir: default_reranker_model_dir(),
            model_url: None,
            tokenizer_url: None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SummarizerTomlConfig {
    #[serde(default)]
    pub enable_llm: bool,
    #[serde(default = "default_model_id")]
    pub model_id: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    #[serde(default = "default_temperature")]
    pub temperature: f64,
}

fn default_model_id() -> String {
    "qwen2.5-coder:1.5b".to_string()
}
fn default_max_tokens() -> usize {
    150
}
fn default_temperature() -> f64 {
    0.3
}

impl Default for SummarizerTomlConfig {
    fn default() -> Self {
        Self {
            enable_llm: true,
            model_id: default_model_id(),
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct DomainsConfig {
    #[serde(default)]
    pub rs_paths: Vec<String>,
    #[serde(default)]
    pub py_paths: Vec<String>,
    #[serde(default)]
    pub frontend_paths: Vec<String>,
    #[serde(default)]
    pub ops_paths: Vec<String>,
    #[serde(default)]
    pub shared_paths: Vec<String>,
}

/// Load gofer configuration from .gofer/config.toml
pub fn load_config(gofer_dir: &Path) -> goferConfig {
    let config_path = gofer_dir.join("config.toml");
    if !config_path.exists() {
        return goferConfig::default();
    }

    match std::fs::read_to_string(&config_path) {
        Ok(content) => toml::from_str(&content).unwrap_or_else(|e| {
            tracing::warn!("Failed to parse config.toml: {}", e);
            goferConfig::default()
        }),
        Err(e) => {
            tracing::warn!("Failed to read config.toml: {}", e);
            goferConfig::default()
        }
    }
}

/// Build gitignore matcher for a directory
fn build_gitignore(root: &Path, extra_ignores: &[String]) -> Gitignore {
    let gitignore_path = root.join(".gitignore");
    let mut builder = GitignoreBuilder::new(root);

    // Add default ignores (always ignored)
    let _ = builder.add_line(None, ".git");
    let _ = builder.add_line(None, ".gofer");
    let _ = builder.add_line(None, ".qoder");
    let _ = builder.add_line(None, "*.lock");

    // Add user-configured ignores from config.toml
    for pattern in extra_ignores {
        let _ = builder.add_line(None, pattern);
    }

    // Add project .gitignore if exists
    if gitignore_path.exists() {
        let _ = builder.add(&gitignore_path);
    }

    builder.build().unwrap_or_else(|_| Gitignore::empty())
}

/// Check if a file should be indexed based on extension
fn should_index(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .and_then(SupportedLanguage::from_extension)
        .is_some()
}

/// Start the file watcher
pub async fn start_watcher(
    root_path: PathBuf,
    task_tx: mpsc::Sender<IndexTask>,
    extra_ignores: Vec<String>,
    cancel: CancellationToken,
) {
    let gitignore = build_gitignore(&root_path, &extra_ignores);

    // Channel for notify events
    let (tx, rx) = channel::<DebounceEventResult>();

    // Create debouncer with 500ms delay
    let mut debouncer = match new_debouncer(Duration::from_millis(500), tx) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("Failed to create file watcher: {}", e);
            return;
        }
    };

    // Watch the root path recursively
    if let Err(e) = debouncer
        .watcher()
        .watch(&root_path, RecursiveMode::Recursive)
    {
        tracing::error!("Failed to watch directory: {}", e);
        return;
    }

    tracing::info!("File watcher started for: {:?}", root_path);

    // Process events in a loop
    loop {
        // Use recv_timeout so we can check cancellation periodically
        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(Ok(events)) => {
                for event in events {
                    let path = event.path;

                    // Skip ignored files
                    if gitignore.matched(&path, path.is_dir()).is_ignore() {
                        continue;
                    }

                    // Skip non-indexable files
                    if !should_index(&path) {
                        continue;
                    }

                    // Determine task type based on file existence
                    let task = if path.exists() {
                        IndexTask::Reindex(path.clone())
                    } else {
                        IndexTask::Delete(path.clone())
                    };

                    tracing::debug!("File event: {:?}", task);

                    if task_tx.send(task).await.is_err() {
                        tracing::warn!("Indexer channel closed");
                        return;
                    }
                }
            }
            Ok(Err(e)) => {
                tracing::error!("Watcher error: {:?}", e);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                tracing::info!("Watcher channel closed");
                break;
            }
        }

        if cancel.is_cancelled() {
            tracing::info!("Watcher stopped for: {:?}", root_path);
            break;
        }
    }
}

/// Scan directory and return all indexable files.
/// Uses `ignore::WalkBuilder` for parallel traversal with built-in .gitignore support.
pub fn scan_directory(root: &Path, extra_ignores: &[String]) -> Vec<PathBuf> {
    use ignore::WalkBuilder;

    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(true) // Пропускать скрытые файлы
        .git_ignore(true) // Учитывать .gitignore
        .git_global(true) // Учитывать глобальный gitignore
        .git_exclude(true) // Учитывать .git/info/exclude
        .threads(
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4),
        );

    // Добавить пользовательские ignore-паттерны
    for pattern in extra_ignores {
        builder.add_custom_ignore_filename(pattern);
    }

    // Также добавить через Override для glob-паттернов
    let mut overrides = ignore::overrides::OverrideBuilder::new(root);
    for pattern in extra_ignores {
        // Negate pattern: !pattern означает "ignore this"
        let _ = overrides.add(&format!("!{}", pattern));
    }
    if let Ok(ov) = overrides.build() {
        builder.overrides(ov);
    }

    let mut files = Vec::new();

    for entry in builder.build().flatten() {
        let path = entry.path().to_path_buf();
        if path.is_file() && should_index(&path) {
            files.push(path);
        }
    }

    files
}

// scan_recursive больше не используется — заменён на ignore::WalkBuilder
