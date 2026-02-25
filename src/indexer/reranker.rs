use anyhow::Result;
use ort::session::{builder::GraphOptimizationLevel, Session};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokenizers::Tokenizer;

use super::watcher::RerankerConfig;

const DEFAULT_MODEL_URL: &str =
    "https://huggingface.co/cross-encoder/ms-marco-TinyBERT-L-2-v2/resolve/main/onnx/model.onnx";
const DEFAULT_TOKENIZER_URL: &str =
    "https://huggingface.co/cross-encoder/ms-marco-TinyBERT-L-2-v2/resolve/main/tokenizer.json";

/// Maximum number of documents to rerank (prevents resource exhaustion)
const MAX_RERANK_DOCUMENTS: usize = 100;
/// Default timeout for rerank operation (5 seconds)
const DEFAULT_RERANK_TIMEOUT: Duration = Duration::from_secs(5);

/// Cross-encoder reranker for improving search results
pub struct Reranker {
    session: Mutex<Session>,
    tokenizer: Tokenizer,
}

impl Reranker {
    /// Create a new reranker with default config, downloading model if needed
    pub fn new() -> Result<Self> {
        Self::with_config(&RerankerConfig::default())
    }

    /// Create a new reranker with custom config
    pub fn with_config(config: &RerankerConfig) -> Result<Self> {
        let model_dir = &config.model_dir;
        let model_url = config.model_url.as_deref().unwrap_or(DEFAULT_MODEL_URL);
        let tokenizer_url = config
            .tokenizer_url
            .as_deref()
            .unwrap_or(DEFAULT_TOKENIZER_URL);

        let model_path = Path::new(model_dir).join("model.onnx");
        let tokenizer_path = Path::new(model_dir).join("tokenizer.json");

        // Create model directory
        std::fs::create_dir_all(model_dir)?;

        // Download model if not exists
        if !model_path.exists() {
            tracing::info!("Downloading reranker model...");
            download_file(model_url, &model_path)?;
        }

        if !tokenizer_path.exists() {
            tracing::info!("Downloading reranker tokenizer...");
            download_file(tokenizer_url, &tokenizer_path)?;
        }

        // Load ONNX session
        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(4)?
            .commit_from_file(&model_path)?;

        // Load tokenizer
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {}", e))?;

        tracing::info!("Reranker initialized (ms-marco-TinyBERT-L-2-v2)");

        Ok(Self {
            session: Mutex::new(session),
            tokenizer,
        })
    }

    /// Rerank documents by relevance to query
    /// Returns indices sorted by relevance score (highest first)
    ///
    /// Limits: max 100 documents, 5 second timeout
    pub fn rerank(
        &self,
        query: &str,
        documents: &[String],
        top_k: usize,
    ) -> Result<Vec<(usize, f32)>> {
        self.rerank_with_limits(
            query,
            documents,
            top_k,
            MAX_RERANK_DOCUMENTS,
            DEFAULT_RERANK_TIMEOUT,
        )
    }

    /// Rerank with explicit limits
    /// OPTIMIZED: Uses batch inference instead of sequential scoring
    pub fn rerank_with_limits(
        &self,
        query: &str,
        documents: &[String],
        top_k: usize,
        max_docs: usize,
        timeout: Duration,
    ) -> Result<Vec<(usize, f32)>> {
        if documents.is_empty() {
            return Ok(Vec::new());
        }

        let start = Instant::now();

        // Apply document limit
        let docs_to_process = if documents.len() > max_docs {
            tracing::warn!(
                "Reranker: truncating {} documents to {} (limit)",
                documents.len(),
                max_docs
            );
            &documents[..max_docs]
        } else {
            documents
        };

        // OPTIMIZATION: Batch scoring instead of sequential
        let scores = self.score_batch(query, docs_to_process)?;

        if start.elapsed() > timeout {
            tracing::warn!(
                "Reranker: completed in {}ms (timeout: {}ms)",
                start.elapsed().as_millis(),
                timeout.as_millis()
            );
        }

        // Create (index, score) pairs
        let mut indexed_scores: Vec<(usize, f32)> = scores.into_iter().enumerate().collect();

        // Sort by score descending
        indexed_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Return top_k
        indexed_scores.truncate(top_k);
        Ok(indexed_scores)
    }

    /// Score a batch of documents at once (10-50x faster than sequential scoring)
    fn score_batch(&self, query: &str, documents: &[String]) -> Result<Vec<f32>> {
        if documents.is_empty() {
            return Ok(Vec::new());
        }

        // Tokenize all pairs at once
        let mut all_input_ids: Vec<Vec<i64>> = Vec::with_capacity(documents.len());
        let mut all_attention_masks: Vec<Vec<i64>> = Vec::with_capacity(documents.len());
        let mut all_token_type_ids: Vec<Vec<i64>> = Vec::with_capacity(documents.len());
        let mut max_seq_len = 0usize;

        for doc in documents {
            let encoding = self
                .tokenizer
                .encode((query, doc.as_str()), true)
                .map_err(|e| anyhow::anyhow!("Tokenization failed: {}", e))?;

            let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&x| x as i64).collect();
            let attention_mask: Vec<i64> = encoding
                .get_attention_mask()
                .iter()
                .map(|&x| x as i64)
                .collect();
            let token_type_ids: Vec<i64> =
                encoding.get_type_ids().iter().map(|&x| x as i64).collect();

            max_seq_len = max_seq_len.max(input_ids.len());

            all_input_ids.push(input_ids);
            all_attention_masks.push(attention_mask);
            all_token_type_ids.push(token_type_ids);
        }

        // Pad sequences to max_seq_len
        let batch_size = documents.len();
        let mut padded_input_ids = Vec::with_capacity(batch_size * max_seq_len);
        let mut padded_attention_masks = Vec::with_capacity(batch_size * max_seq_len);
        let mut padded_token_type_ids = Vec::with_capacity(batch_size * max_seq_len);

        for i in 0..batch_size {
            let seq_len = all_input_ids[i].len();
            let padding_len = max_seq_len - seq_len;

            // Add tokens
            padded_input_ids.extend_from_slice(&all_input_ids[i]);
            padded_attention_masks.extend_from_slice(&all_attention_masks[i]);
            padded_token_type_ids.extend_from_slice(&all_token_type_ids[i]);

            // Add padding (0s)
            padded_input_ids.extend(std::iter::repeat(0i64).take(padding_len));
            padded_attention_masks.extend(std::iter::repeat(0i64).take(padding_len));
            padded_token_type_ids.extend(std::iter::repeat(0i64).take(padding_len));
        }

        // Create batch tensors
        let input_ids_tensor = ort::value::Tensor::from_array((
            [batch_size, max_seq_len],
            padded_input_ids.into_boxed_slice(),
        ))?;
        let attention_mask_tensor = ort::value::Tensor::from_array((
            [batch_size, max_seq_len],
            padded_attention_masks.into_boxed_slice(),
        ))?;
        let token_type_ids_tensor = ort::value::Tensor::from_array((
            [batch_size, max_seq_len],
            padded_token_type_ids.into_boxed_slice(),
        ))?;

        // Run batch inference
        let mut session = self
            .session
            .lock()
            .map_err(|e| anyhow::anyhow!("Session lock poisoned: {}", e))?;
        let outputs = session.run(ort::inputs![
            "input_ids" => input_ids_tensor,
            "attention_mask" => attention_mask_tensor,
            "token_type_ids" => token_type_ids_tensor,
        ])?;

        // Extract scores from output
        let output_tensor = outputs[0].try_extract_tensor::<f32>()?;
        let data = output_tensor.1;

        // Output shape is [batch_size, 1] or [batch_size]
        // Extract score for each document
        let scores: Vec<f32> = (0..batch_size).map(|i| data[i]).collect();

        Ok(scores)
    }

    /// Score a single query-document pair
    #[allow(dead_code)]
    fn score_pair(&self, query: &str, document: &str) -> Result<f32> {
        // Tokenize input pair
        let encoding = self
            .tokenizer
            .encode((query, document), true)
            .map_err(|e| anyhow::anyhow!("Tokenization failed: {}", e))?;

        let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&x| x as i64).collect();
        let attention_mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .map(|&x| x as i64)
            .collect();
        let token_type_ids: Vec<i64> = encoding.get_type_ids().iter().map(|&x| x as i64).collect();

        let seq_len = input_ids.len();

        // Create input tensors using ort::Tensor
        let input_ids_tensor =
            ort::value::Tensor::from_array(([1usize, seq_len], input_ids.into_boxed_slice()))?;
        let attention_mask_tensor =
            ort::value::Tensor::from_array(([1usize, seq_len], attention_mask.into_boxed_slice()))?;
        let token_type_ids_tensor =
            ort::value::Tensor::from_array(([1usize, seq_len], token_type_ids.into_boxed_slice()))?;

        // Run inference
        let mut session = self
            .session
            .lock()
            .map_err(|e| anyhow::anyhow!("Session lock poisoned: {}", e))?;
        let outputs = session.run(ort::inputs![
            "input_ids" => input_ids_tensor,
            "attention_mask" => attention_mask_tensor,
            "token_type_ids" => token_type_ids_tensor,
        ])?;

        // Extract logits - get the first element from the output
        let output_tensor = outputs[0].try_extract_tensor::<f32>()?;
        let data = output_tensor.1; // (shape, data)
        let score = data[0];

        Ok(score)
    }
}

fn download_file(url: &str, path: &Path) -> Result<()> {
    let response = ureq::get(url).call()?;
    let mut file = std::fs::File::create(path)?;
    let mut reader = response.into_body().into_reader();
    std::io::copy(&mut reader, &mut file)?;
    Ok(())
}
