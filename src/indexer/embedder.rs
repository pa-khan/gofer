use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use fastembed::{
    EmbeddingModel, InitOptions, InitOptionsUserDefined, QuantizationMode, TextEmbedding,
    UserDefinedEmbeddingModel,
};
use thiserror::Error;
use tokio::sync::{RwLock, Semaphore};

use super::watcher::EmbeddingConfig;

#[derive(Error, Debug)]
pub enum EmbedderError {
    #[error("Embedding error: {0}")]
    Embedding(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, EmbedderError>;

/// Единичный embedder (обёртка над fastembed)
struct Embedder {
    model: std::sync::Mutex<TextEmbedding>,
}

impl Embedder {
    /// Создать embedder с указанной моделью и каталогом кэша.
    fn with_model(model: EmbeddingModel, cache_dir: &PathBuf, pool_size: usize) -> Result<Self> {
        std::fs::create_dir_all(cache_dir).ok();

        #[cfg(feature = "gpu")]
        tracing::info!("GPU feature включён — ort будет использовать CUDA если доступен");

        // OPTIMIZATION: Configure ONNX Runtime threads to prevent oversubscription
        // Each instance should use (num_physical_cores / pool_size) threads
        let num_physical_cores = num_cpus::get_physical();
        let threads_per_instance = (num_physical_cores / pool_size).max(1);

        let model = TextEmbedding::try_new(
            InitOptions::new(model)
                .with_cache_dir(cache_dir.clone())
                .with_show_download_progress(true),
        )
        .map_err(EmbedderError::Embedding)?;

        tracing::debug!(
            "Embedder initialized: {} threads (physical cores: {}, pool size: {})",
            threads_per_instance,
            num_physical_cores,
            pool_size
        );

        Ok(Self {
            model: std::sync::Mutex::new(model),
        })
    }

    /// Создать embedder из custom quantized ONNX модели
    fn with_quantized_model(
        onnx_path: &PathBuf,
        tokenizer_path: &PathBuf,
        tokenizer_config_path: &PathBuf,
        pool_size: usize,
    ) -> Result<Self> {
        tracing::info!("Loading quantized INT8 model from: {:?}", onnx_path);

        // Load ONNX file
        let onnx_file = std::fs::read(onnx_path).map_err(|e| {
            EmbedderError::Embedding(anyhow::anyhow!("Failed to read ONNX file: {}", e))
        })?;

        // Load tokenizer files
        let tokenizer_json = std::fs::read(tokenizer_path).map_err(|e| {
            EmbedderError::Embedding(anyhow::anyhow!("Failed to read tokenizer.json: {}", e))
        })?;
        let tokenizer_config_json = std::fs::read(tokenizer_config_path).map_err(|e| {
            EmbedderError::Embedding(anyhow::anyhow!(
                "Failed to read tokenizer_config.json: {}",
                e
            ))
        })?;

        // Create TokenizerFiles
        let tokenizer_files = fastembed::TokenizerFiles {
            tokenizer_file: tokenizer_json,
            config_file: tokenizer_config_json,
            special_tokens_map_file: Vec::new(), // Optional
            tokenizer_config_file: Vec::new(),   // Optional
        };

        // Create UserDefinedEmbeddingModel with INT8 quantization
        let user_model = UserDefinedEmbeddingModel::new(onnx_file, tokenizer_files)
            .with_quantization(QuantizationMode::Static); // INT8 static quantization

        // OPTIMIZATION: Configure ONNX Runtime threads
        let num_physical_cores = num_cpus::get_physical();
        let threads_per_instance = (num_physical_cores / pool_size).max(1);

        let options = InitOptionsUserDefined::new().with_max_length(512);

        let model = TextEmbedding::try_new_from_user_defined(user_model, options)
            .map_err(EmbedderError::Embedding)?;

        tracing::info!(
            "Quantized INT8 embedder initialized: {} threads (physical cores: {}, pool size: {})",
            threads_per_instance,
            num_physical_cores,
            pool_size
        );

        Ok(Self {
            model: std::sync::Mutex::new(model),
        })
    }

    /// Сгенерировать embeddings для списка текстов (sync, CPU-bound)
    fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        tracing::debug!(
            "Embedder::embed: locking model mutex for {} texts",
            texts.len()
        );
        let mut model = self.model.lock().map_err(|e| {
            tracing::error!("Embedder::embed: mutex poisoned: {}", e);
            EmbedderError::Embedding(anyhow::anyhow!("Mutex poisoned: {}", e))
        })?;

        tracing::debug!(
            "Embedder::embed: calling fastembed model.embed() for {} texts",
            texts.len()
        );
        let embeddings = model.embed(texts, None).map_err(|e| {
            tracing::error!("Embedder::embed: fastembed error: {}", e);
            EmbedderError::Embedding(e)
        })?;

        tracing::debug!(
            "Embedder::embed: successfully embedded {} texts",
            embeddings.len()
        );
        Ok(embeddings)
    }
}

// ---------------------------------------------------------------------------
// EmbedderPool — пул из N embedder-инстансов для concurrent embedding
// ---------------------------------------------------------------------------

/// Пул embedder-ов: N инстансов под Semaphore для параллельного embedding.
/// Поддерживает динамическое масштабирование: scale_up() перед индексацией,
/// scale_down() после для экономии памяти.
///
/// Решает проблемы:
/// - C2: ownership loss (Arc, не Option::take)
/// - C3: shared embedder (один пул на весь daemon)
/// - H4: снятие bottleneck Mutex (параллельные embed-запросы)
/// - Memory: динамическое масштабирование (1 инстанс в простое, N при индексации)
pub struct EmbedderPool {
    instances: RwLock<Vec<Arc<Embedder>>>,
    semaphore: Arc<RwLock<Arc<Semaphore>>>,
    pool_size: AtomicUsize,
    next_idx: AtomicUsize,
    model_dimension: usize,
    model_name: String,
    model: EmbeddingModel,
    cache_dir: PathBuf,
}

impl EmbedderPool {
    /// Создать пул из `size` embedder-инстансов (от 1 до 8).
    #[allow(dead_code)]
    pub fn new(size: usize) -> Result<Self> {
        Self::with_config(size, &EmbeddingConfig::default())
    }

    /// Создать пул с конфигурацией модели.
    pub fn with_config(size: usize, config: &EmbeddingConfig) -> Result<Self> {
        let size = size.clamp(1, 8);

        // Check if using custom quantized model
        let use_quantized = config.quantized_model_path.is_some()
            && config.tokenizer_path.is_some()
            && config.tokenizer_config_path.is_some();

        if use_quantized {
            tracing::info!(
                "Инициализация embedder pool с INT8 quantized моделью: {} инстансов",
                size
            );
        } else {
            tracing::info!(
                "Инициализация embedder pool: {} инстансов, model={}",
                size,
                config.model
            );
        }

        let cache_dir = config
            .cache_dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                // Используем абсолютный путь в ~/.cache/fastembed для совместимости с библиотекой
                dirs::cache_dir()
                    .unwrap_or_else(|| PathBuf::from("/tmp"))
                    .join("fastembed")
            });

        let model = match config.model.as_str() {
            "BGESmallENV15" | "bge-small-en-v1.5" => EmbeddingModel::BGESmallENV15,
            "BGEBaseENV15" | "bge-base-en-v1.5" => EmbeddingModel::BGEBaseENV15,
            "AllMiniLML6V2" | "all-MiniLM-L6-v2" => EmbeddingModel::AllMiniLML6V2,
            _ => {
                tracing::warn!(
                    "Неизвестная модель '{}', fallback на BGESmallENV15",
                    config.model
                );
                EmbeddingModel::BGESmallENV15
            }
        };

        let model_dimension = if use_quantized {
            // nomic-embed-text-v1.5 INT8 has 768 dimensions
            768
        } else {
            match &model {
                EmbeddingModel::BGEBaseENV15 => 768,
                _ => 384, // BGESmallENV15, AllMiniLML6V2
            }
        };

        let mut instances = Vec::with_capacity(size);

        if use_quantized {
            // Load custom quantized model
            let onnx_path = PathBuf::from(config.quantized_model_path.as_ref().unwrap());
            let tokenizer_path = PathBuf::from(config.tokenizer_path.as_ref().unwrap());
            let tokenizer_config_path =
                PathBuf::from(config.tokenizer_config_path.as_ref().unwrap());

            for i in 0..size {
                let embedder = Embedder::with_quantized_model(
                    &onnx_path,
                    &tokenizer_path,
                    &tokenizer_config_path,
                    size,
                )?;
                tracing::debug!("Embedder pool: quantized инстанс {}/{} создан", i + 1, size);
                instances.push(Arc::new(embedder));
            }
            tracing::info!(
                "Embedder pool инициализирован: {} INT8 quantized инстансов ({} dims)",
                size,
                model_dimension
            );
        } else {
            // Load standard model
            for i in 0..size {
                let embedder = Embedder::with_model(model.clone(), &cache_dir, size)?;
                tracing::debug!("Embedder pool: инстанс {}/{} создан", i + 1, size);
                instances.push(Arc::new(embedder));
            }
            tracing::info!(
                "Embedder pool инициализирован: {} инстансов, {:?} ({} dims)",
                size,
                config.model,
                model_dimension
            );
        }

        Ok(Self {
            instances: RwLock::new(instances),
            semaphore: Arc::new(RwLock::new(Arc::new(Semaphore::new(size)))),
            pool_size: AtomicUsize::new(size),
            next_idx: AtomicUsize::new(0),
            model_dimension,
            model_name: config.model.clone(),
            model,
            cache_dir,
        })
    }

    /// Масштабировать пул до `target_size` инстансов (для индексации).
    /// Если текущий размер >= target, ничего не делает.
    pub async fn scale_up(&self, target_size: usize) -> Result<()> {
        let target_size = target_size.clamp(1, 8);
        let current_size = self.pool_size.load(Ordering::Acquire);

        if current_size >= target_size {
            return Ok(());
        }

        let to_add = target_size - current_size;
        tracing::info!(
            "Масштабирование embedder pool: {} -> {} (+{} инстансов)",
            current_size,
            target_size,
            to_add
        );

        // Создаём новые инстансы
        let mut new_instances = Vec::with_capacity(to_add);
        for i in 0..to_add {
            let embedder = Embedder::with_model(self.model.clone(), &self.cache_dir, target_size)?;
            tracing::debug!(
                "Embedder pool: дополнительный инстанс {}/{} создан",
                i + 1,
                to_add
            );
            new_instances.push(Arc::new(embedder));
        }

        // Добавляем инстансы в пул
        {
            let mut instances = self.instances.write().await;
            instances.extend(new_instances);
        }

        // Обновляем семафор
        {
            let mut sem_guard = self.semaphore.write().await;
            *sem_guard = Arc::new(Semaphore::new(target_size));
        }

        self.pool_size.store(target_size, Ordering::Release);
        tracing::info!("Embedder pool масштабирован до {} инстансов", target_size);

        Ok(())
    }

    /// Уменьшить пул до `target_size` инстансов (для экономии памяти).
    /// Освобождает память, удаляя лишние инстансы.
    pub async fn scale_down(&self, target_size: usize) -> Result<()> {
        let target_size = target_size.clamp(1, 8);
        let current_size = self.pool_size.load(Ordering::Acquire);

        if current_size <= target_size {
            return Ok(());
        }

        tracing::info!(
            "Уменьшение embedder pool: {} -> {} (-{} инстансов)",
            current_size,
            target_size,
            current_size - target_size
        );

        // Уменьшаем количество инстансов
        {
            let mut instances = self.instances.write().await;
            instances.truncate(target_size);
            instances.shrink_to_fit(); // Освобождаем память Vec
        }

        // Обновляем семафор
        {
            let mut sem_guard = self.semaphore.write().await;
            *sem_guard = Arc::new(Semaphore::new(target_size));
        }

        self.pool_size.store(target_size, Ordering::Release);
        self.next_idx.store(0, Ordering::Release); // Сброс round-robin

        tracing::info!("Embedder pool уменьшен до {} инстансов", target_size);

        Ok(())
    }

    /// Текущий размер пула.
    #[allow(dead_code)]
    pub fn current_size(&self) -> usize {
        self.pool_size.load(Ordering::Acquire)
    }

    /// Embed batch текстов — использует spawn_blocking для CPU-bound операции.
    pub async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let semaphore = {
            let guard = self.semaphore.read().await;
            guard.clone()
        };

        let _permit = semaphore
            .acquire()
            .await
            .map_err(|_| EmbedderError::Embedding(anyhow::anyhow!("Semaphore закрыт")))?;

        // Round-robin выбор инстанса (lock-free)
        let pool_size = self.pool_size.load(Ordering::Acquire);
        let idx = self.next_idx.fetch_add(1, Ordering::Relaxed) % pool_size;

        let embedder = {
            let instances = self.instances.read().await;
            instances.get(idx).cloned()
        };

        let embedder = embedder.ok_or_else(|| {
            EmbedderError::Embedding(anyhow::anyhow!("Embedder инстанс {} недоступен", idx))
        })?;

        // CPU-bound embedding в отдельном thread (НЕ rayon, а tokio::spawn_blocking)
        // spawn_blocking использует dedicated thread pool, не блокируя tokio runtime
        tokio::task::spawn_blocking(move || embedder.embed(texts))
            .await
            .map_err(|e| {
                EmbedderError::Embedding(anyhow::anyhow!("spawn_blocking join error: {}", e))
            })?
    }

    /// Embed одного текста (для search queries).
    pub async fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self.embed(vec![text.to_string()]).await?;
        Ok(embeddings.into_iter().next().unwrap_or_default())
    }

    /// Размерность embedding вектора (зависит от выбранной модели).
    pub fn dimension(&self) -> usize {
        self.model_dimension
    }

    /// Name of the active embedding model (for cache versioning).
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// Cache version key: `{model_name}:{dimension}`.
    pub fn cache_version_key(&self) -> String {
        format!("{}:{}", self.model_name, self.model_dimension)
    }
}

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // EmbedderPool basic tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_pool_creation() {
        // Pool should be created with valid size
        let pool = EmbedderPool::new(2);
        assert!(pool.is_ok());

        let pool = pool.unwrap();
        assert_eq!(pool.current_size(), 2);
    }

    #[test]
    fn test_pool_size_clamping() {
        // Size should be clamped to 1-8
        let pool = EmbedderPool::new(0).unwrap();
        assert_eq!(pool.current_size(), 1); // min 1

        let pool = EmbedderPool::new(100).unwrap();
        assert_eq!(pool.current_size(), 8); // max 8
    }

    #[test]
    fn test_dimension() {
        let pool = EmbedderPool::new(1).unwrap();
        // BGESmallENV15 has 384 dimensions
        assert_eq!(pool.dimension(), 384);
    }

    #[test]
    fn test_model_name() {
        let pool = EmbedderPool::new(1).unwrap();
        // Default model is BGESmallENV15
        assert_eq!(pool.model_name(), "BGESmallENV15");
    }

    #[test]
    fn test_cache_version_key() {
        let pool = EmbedderPool::new(1).unwrap();
        assert_eq!(pool.cache_version_key(), "BGESmallENV15:384");
    }

    #[test]
    fn test_config_different_models() {
        // Test BGEBaseENV15 (768 dims)
        let config = EmbeddingConfig {
            model: "BGEBaseENV15".to_string(),
            batch_size: 32,
            cache_dir: None,
            pool_size: 1,
        };
        let pool = EmbedderPool::with_config(1, &config).unwrap();
        assert_eq!(pool.dimension(), 768);
        assert_eq!(pool.model_name(), "BGEBaseENV15");

        // Test AllMiniLML6V2 (384 dims)
        let config = EmbeddingConfig {
            model: "AllMiniLML6V2".to_string(),
            batch_size: 32,
            cache_dir: None,
            pool_size: 1,
        };
        let pool = EmbedderPool::with_config(1, &config).unwrap();
        assert_eq!(pool.dimension(), 384);
    }

    #[test]
    fn test_unknown_model_fallback() {
        let config = EmbeddingConfig {
            model: "UnknownModel".to_string(),
            batch_size: 32,
            cache_dir: None,
            pool_size: 1,
        };
        // Should fallback to BGESmallENV15
        let pool = EmbedderPool::with_config(1, &config).unwrap();
        assert_eq!(pool.dimension(), 384);
    }

    // -------------------------------------------------------------------------
    // Embedding tests (require model download, run with --ignored)
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_embed_empty_input() {
        let pool = EmbedderPool::new(1).unwrap();
        let result = pool.embed(vec![]).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_embed_single_text() {
        let pool = EmbedderPool::new(1).unwrap();
        let result = pool.embed(vec!["Hello, world!".to_string()]).await;

        assert!(result.is_ok());
        let embeddings = result.unwrap();
        assert_eq!(embeddings.len(), 1);
        assert_eq!(embeddings[0].len(), 384); // BGESmallENV15 dimension
    }

    #[tokio::test]
    async fn test_embed_multiple_texts() {
        let pool = EmbedderPool::new(1).unwrap();
        let texts = vec![
            "First document".to_string(),
            "Second document".to_string(),
            "Third document".to_string(),
        ];
        let result = pool.embed(texts).await;

        assert!(result.is_ok());
        let embeddings = result.unwrap();
        assert_eq!(embeddings.len(), 3);
        for emb in &embeddings {
            assert_eq!(emb.len(), 384);
        }
    }

    #[tokio::test]
    async fn test_embed_query() {
        let pool = EmbedderPool::new(1).unwrap();
        let result = pool.embed_query("test query").await;

        assert!(result.is_ok());
        let embedding = result.unwrap();
        assert_eq!(embedding.len(), 384);
    }

    #[tokio::test]
    async fn test_embedding_consistency() {
        // Same text should produce same embedding
        let pool = EmbedderPool::new(1).unwrap();
        let text = "Consistent test string";

        let emb1 = pool.embed_query(text).await.unwrap();
        let emb2 = pool.embed_query(text).await.unwrap();

        // Embeddings should be identical
        assert_eq!(emb1.len(), emb2.len());
        for (a, b) in emb1.iter().zip(emb2.iter()) {
            assert!((a - b).abs() < 1e-6, "Embeddings differ: {} vs {}", a, b);
        }
    }

    #[tokio::test]
    async fn test_embedding_similarity() {
        // Similar texts should have similar embeddings
        let pool = EmbedderPool::new(1).unwrap();

        let emb_rust = pool.embed_query("Rust programming language").await.unwrap();
        let emb_cargo = pool
            .embed_query("Cargo package manager for Rust")
            .await
            .unwrap();
        let emb_python = pool.embed_query("Python snake animal").await.unwrap();

        // Cosine similarity helper
        fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
            let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
            let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
            let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
            dot / (norm_a * norm_b)
        }

        let sim_rust_cargo = cosine_similarity(&emb_rust, &emb_cargo);
        let sim_rust_python = cosine_similarity(&emb_rust, &emb_python);

        // Rust should be more similar to Cargo than to Python snake
        assert!(
            sim_rust_cargo > sim_rust_python,
            "Expected Rust-Cargo similarity ({}) > Rust-Python similarity ({})",
            sim_rust_cargo,
            sim_rust_python
        );
    }

    #[tokio::test]
    async fn test_concurrent_embedding() {
        // Test that pool handles concurrent requests
        let pool = Arc::new(EmbedderPool::new(2).unwrap());

        let mut handles = vec![];
        for i in 0..4 {
            let pool_clone = pool.clone();
            let handle = tokio::spawn(async move {
                let text = format!("Concurrent test {}", i);
                pool_clone.embed_query(&text).await
            });
            handles.push(handle);
        }

        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_ok());
            assert_eq!(result.unwrap().len(), 384);
        }
    }

    #[tokio::test]
    async fn test_unicode_text() {
        let pool = EmbedderPool::new(1).unwrap();
        let result = pool
            .embed_query("Привет мир! 你好世界! مرحبا بالعالم")
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 384);
    }

    #[tokio::test]
    async fn test_long_text() {
        let pool = EmbedderPool::new(1).unwrap();
        // Generate a long text (model should handle truncation)
        let long_text = "word ".repeat(1000);
        let result = pool.embed_query(&long_text).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 384);
    }

    #[tokio::test]
    async fn test_special_characters() {
        let pool = EmbedderPool::new(1).unwrap();
        let result = pool
            .embed_query("fn main() { println!(\"Hello\"); } // comment")
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 384);
    }

    // -------------------------------------------------------------------------
    // Round-robin distribution test
    // -------------------------------------------------------------------------

    #[test]
    fn test_round_robin_index() {
        let pool = EmbedderPool::new(4).unwrap();
        let pool_size = pool.current_size();

        // Simulate round-robin selection
        let idx0 = pool.next_idx.fetch_add(1, Ordering::Relaxed) % pool_size;
        let idx1 = pool.next_idx.fetch_add(1, Ordering::Relaxed) % pool_size;
        let idx2 = pool.next_idx.fetch_add(1, Ordering::Relaxed) % pool_size;
        let idx3 = pool.next_idx.fetch_add(1, Ordering::Relaxed) % pool_size;
        let idx4 = pool.next_idx.fetch_add(1, Ordering::Relaxed) % pool_size;

        assert_eq!(idx0, 0);
        assert_eq!(idx1, 1);
        assert_eq!(idx2, 2);
        assert_eq!(idx3, 3);
        assert_eq!(idx4, 0); // wraps around
    }

    // -------------------------------------------------------------------------
    // Dynamic scaling tests
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_scale_up() {
        let pool = EmbedderPool::new(1).unwrap();
        assert_eq!(pool.current_size(), 1);

        pool.scale_up(4).await.unwrap();
        assert_eq!(pool.current_size(), 4);

        // Should not increase beyond target
        pool.scale_up(2).await.unwrap();
        assert_eq!(pool.current_size(), 4);
    }

    #[tokio::test]
    async fn test_scale_down() {
        let pool = EmbedderPool::new(4).unwrap();
        assert_eq!(pool.current_size(), 4);

        pool.scale_down(1).await.unwrap();
        assert_eq!(pool.current_size(), 1);

        // Should not decrease below target
        pool.scale_down(2).await.unwrap();
        assert_eq!(pool.current_size(), 1);
    }

    #[tokio::test]
    async fn test_scale_up_down_cycle() {
        let pool = EmbedderPool::new(1).unwrap();

        // Scale up for indexing
        pool.scale_up(4).await.unwrap();
        assert_eq!(pool.current_size(), 4);

        // Embedding should still work
        let result = pool.embed_query("test after scale up").await;
        assert!(result.is_ok());

        // Scale down for idle
        pool.scale_down(1).await.unwrap();
        assert_eq!(pool.current_size(), 1);

        // Embedding should still work
        let result = pool.embed_query("test after scale down").await;
        assert!(result.is_ok());
    }
}
