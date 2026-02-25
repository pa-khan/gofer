//! File summarization: docstring extraction + optional LLM (Ollama) generation.

use regex::Regex;

/// Extract docstring/module-level comment from a file (no LLM).
pub fn extract_docstring(content: &str, ext: &str) -> Option<String> {
    match ext {
        "rs" => {
            let module_doc_re = Regex::new(r"(?m)^//!\s*(.+)$").ok()?;
            let mut doc_lines: Vec<String> = Vec::new();
            for cap in module_doc_re.captures_iter(content) {
                let line = cap.get(1)?.as_str().trim();
                if !line.is_empty() && !line.starts_with('#') {
                    doc_lines.push(line.to_string());
                }
                if doc_lines.len() >= 3 {
                    break;
                }
            }
            if !doc_lines.is_empty() {
                return Some(doc_lines.join(" "));
            }
        }
        "ts" | "tsx" | "js" | "jsx" => {
            let jsdoc_re = Regex::new(
                r"(?s)/\*\*.*?@(?:fileoverview|description|module)\s+(.+?)(?:\n\s*\*\s*@|\*/)",
            )
            .ok()?;
            if let Some(cap) = jsdoc_re.captures(content) {
                let summary = cap
                    .get(1)?
                    .as_str()
                    .trim()
                    .lines()
                    .map(|l| l.trim().trim_start_matches('*').trim())
                    .filter(|l| !l.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
                if !summary.is_empty() {
                    return Some(summary);
                }
            }
        }
        "py" => {
            let docstring_re =
                Regex::new(r#"^(?:\s*#[^\n]*\n)*\s*(?:"""(.*?)"""|'''(.*?)''')"#).ok()?;
            if let Some(cap) = docstring_re.captures(content) {
                let doc = cap.get(1).or(cap.get(2))?.as_str();
                let summary = doc
                    .lines()
                    .take(3)
                    .map(|l| l.trim())
                    .filter(|l| !l.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
                if !summary.is_empty() {
                    return Some(summary);
                }
            }
        }
        _ => {}
    }
    None
}

// === Ollama LLM summarization ===

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct SummarizerConfig {
    pub enable_llm: bool,
    pub model_id: String,
    pub max_tokens: usize,
    pub temperature: f64,
    pub ollama_url: String,
}

impl Default for SummarizerConfig {
    fn default() -> Self {
        Self {
            enable_llm: true,
            model_id: "qwen2.5-coder:1.5b".to_string(),
            max_tokens: 150,
            temperature: 0.3,
            ollama_url: "http://localhost:11434".to_string(),
        }
    }
}

impl SummarizerConfig {
    pub fn from_toml(toml: &crate::indexer::watcher::SummarizerTomlConfig) -> Self {
        Self {
            enable_llm: toml.enable_llm,
            model_id: toml.model_id.clone(),
            max_tokens: toml.max_tokens,
            temperature: toml.temperature,
            ollama_url: "http://localhost:11434".to_string(),
        }
    }
}

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
    options: OllamaOptions,
}

#[derive(Serialize)]
struct OllamaOptions {
    temperature: f64,
    num_predict: usize,
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
}

/// Call Ollama to generate a one-line summary for a code file.
pub async fn summarize_with_ollama(
    config: &SummarizerConfig,
    file_path: &str,
    content: &str,
) -> anyhow::Result<String> {
    // Обрезаем содержимое до ~2000 символов, безопасно по границе UTF-8
    let truncated = if content.len() > 2000 {
        &content[..content.floor_char_boundary(2000)]
    } else {
        content
    };

    let prompt = format!(
        "Summarize the purpose of this source file in one sentence (max 30 words). \
         Reply ONLY with the summary, no explanation.\n\nFile: {}\n```\n{}\n```",
        file_path, truncated
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let body = OllamaRequest {
        model: config.model_id.clone(),
        prompt,
        stream: false,
        options: OllamaOptions {
            temperature: config.temperature,
            num_predict: config.max_tokens,
        },
    };

    let resp = client
        .post(format!("{}/api/generate", config.ollama_url))
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("Ollama returned status {}", resp.status());
    }

    let result: OllamaResponse = resp.json().await?;
    let summary = result.response.trim().to_string();

    // Sanity: cap at 300 chars
    Ok(if summary.len() > 300 {
        format!("{}...", &summary[..297])
    } else {
        summary
    })
}

/// Background worker that processes the summary queue using Ollama.
/// Runs until the cancellation token is triggered.
pub async fn summary_worker(
    config: SummarizerConfig,
    sqlite: crate::storage::SqliteStorage,
    cancel: tokio_util::sync::CancellationToken,
) {
    if !config.enable_llm {
        tracing::info!("Summarizer: LLM disabled, worker not started");
        return;
    }

    tracing::info!("Summarizer: worker started (model={})", config.model_id);

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                tracing::info!("Summarizer: shutting down");
                break;
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {}
        }

        let item = match sqlite.pop_summary_queue().await {
            Ok(Some(item)) => item,
            Ok(None) => continue,
            Err(e) => {
                tracing::warn!("Summarizer: queue error: {}", e);
                continue;
            }
        };

        // Get file info
        let file_record = match sqlite.get_file_by_id(item.file_id).await {
            Ok(Some(f)) => f,
            Ok(None) => {
                let _ = sqlite.complete_summary_queue(item.id).await;
                continue;
            }
            Err(e) => {
                tracing::warn!("Summarizer: get file error: {}", e);
                let _ = sqlite.fail_summary_queue(item.id, &e.to_string()).await;
                continue;
            }
        };

        // Read file content
        let content = match tokio::fs::read_to_string(&file_record.path).await {
            Ok(c) => c,
            Err(e) => {
                let _ = sqlite.fail_summary_queue(item.id, &e.to_string()).await;
                continue;
            }
        };

        // Try docstring first, then LLM
        let ext = std::path::Path::new(&file_record.path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let (summary, source) = if let Some(doc) = extract_docstring(&content, ext) {
            (doc, "docstring")
        } else {
            match summarize_with_ollama(&config, &file_record.path, &content).await {
                Ok(s) => (s, "llm"),
                Err(e) => {
                    tracing::warn!("Summarizer: LLM failed for {}: {}", file_record.path, e);
                    let _ = sqlite.fail_summary_queue(item.id, &e.to_string()).await;
                    continue;
                }
            }
        };

        // Store summary
        match sqlite
            .upsert_summary(
                item.file_id,
                &summary,
                source,
                Some(&config.model_id),
                Some(1.0),
            )
            .await
        {
            Ok(_) => {
                let _ = sqlite.complete_summary_queue(item.id).await;
                tracing::debug!("Summarizer: {} => {}", file_record.path, summary);
            }
            Err(e) => {
                let _ = sqlite.fail_summary_queue(item.id, &e.to_string()).await;
                tracing::warn!("Summarizer: store failed: {}", e);
            }
        }
    }
}
