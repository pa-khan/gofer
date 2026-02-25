//! Content-Addressable Buffer (CAS) - Phase 3 implementation
//!
//! Революционная система: AI оперирует хешами вместо копирования кода.
//!
//! Implements:
//! - extract_to_hash - создать хеш из блока кода (copy/cut)
//! - insert_hash - вставить код по хешу
//! - replace_with_hash - заменить блок кода на содержимое хеша
//! - content_to_hash - создать хеш из произвольного контента
//! - list_buffers - показать активные хеши
//! - clear_buffer - удалить хеш из памяти
//! - apply_template - шаблонизация с подстановкой хешей

use super::common::{resolve_path_buf, ToolContext};
use crate::error::goferError;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Content buffer metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBuffer {
    pub hash_id: String,
    pub content: String,
    pub size_bytes: usize,
    pub lines: usize,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub source_file: Option<String>,
    pub access_count: u32,
}

// Global buffer storage (in-memory for Phase 3)
lazy_static::lazy_static! {
    static ref BUFFERS: Arc<RwLock<HashMap<String, ContentBuffer>>> = 
        Arc::new(RwLock::new(HashMap::new()));
}

const BUFFER_TTL_SECONDS: i64 = 86400; // 24 hours
const MAX_BUFFER_SIZE_BYTES: usize = 1024 * 1024; // 1 MB
const MAX_BUFFERS: usize = 1000;

/// Extract code block to hash
pub async fn tool_extract_to_hash(args: Value, ctx: &ToolContext) -> Result<Value> {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| goferError::InvalidParams("path is required".into()))?;

    let start_line = args
        .get("start_line")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| goferError::InvalidParams("start_line is required".into()))? as usize;

    let end_line = args
        .get("end_line")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| goferError::InvalidParams("end_line is required".into()))? as usize;

    let cut = args.get("cut").and_then(|v| v.as_bool()).unwrap_or(false);

    let abs_path = resolve_path_buf(&ctx.root_path, path);

    if !abs_path.exists() {
        return Err(goferError::InvalidParams(format!("File not found: {}", path)).into());
    }

    // Read file
    let content = tokio::fs::read_to_string(&abs_path).await?;
    let lines: Vec<&str> = content.lines().collect();

    if start_line < 1 || end_line > lines.len() || start_line > end_line {
        return Err(goferError::InvalidParams(format!(
            "Invalid line range: {}-{} (file has {} lines)",
            start_line,
            end_line,
            lines.len()
        ))
        .into());
    }

    // Extract block (1-indexed to 0-indexed)
    let block_lines = &lines[(start_line - 1)..end_line];
    let block_content = block_lines.join("\n");

    // Check size limit
    if block_content.len() > MAX_BUFFER_SIZE_BYTES {
        return Err(goferError::InvalidParams(format!(
            "Block size {} exceeds max {} bytes",
            block_content.len(),
            MAX_BUFFER_SIZE_BYTES
        ))
        .into());
    }

    // Calculate hash (first 8 chars of SHA256)
    let hash_id = calculate_hash(&block_content);

    // Store in buffer
    let buffer = ContentBuffer {
        hash_id: hash_id.clone(),
        content: block_content.clone(),
        size_bytes: block_content.len(),
        lines: block_lines.len(),
        created_at: Utc::now(),
        expires_at: Utc::now() + chrono::Duration::seconds(BUFFER_TTL_SECONDS),
        source_file: Some(path.to_string()),
        access_count: 0,
    };

    // Check buffer limit
    {
        let buffers = BUFFERS.read().await;
        if buffers.len() >= MAX_BUFFERS {
            // Clean up expired buffers
            drop(buffers);
            cleanup_expired_buffers().await;

            let buffers = BUFFERS.read().await;
            if buffers.len() >= MAX_BUFFERS {
                return Err(goferError::InvalidParams(format!(
                    "Buffer limit reached ({} buffers)",
                    MAX_BUFFERS
                ))
                .into());
            }
        }
    }

    // Store buffer
    {
        let mut buffers = BUFFERS.write().await;
        buffers.insert(hash_id.clone(), buffer);
    }

    // If cut mode, remove block from file
    let action = if cut {
        let remaining_lines: Vec<&str> = lines[..(start_line - 1)]
            .iter()
            .chain(lines[end_line..].iter())
            .copied()
            .collect();

        let new_content = remaining_lines.join("\n");
        tokio::fs::write(&abs_path, new_content).await?;

        // Invalidate cache
        ctx.cache.invalidate_file(path).await;

        "cut"
    } else {
        "copied"
    };

    // Generate preview (first 3 lines)
    let preview_lines: Vec<&str> = block_lines.iter().take(3).copied().collect();
    let mut preview = preview_lines.join("\n");
    if block_lines.len() > 3 {
        preview.push_str("\n...");
    }

    Ok(json!({
        "hash_id": hash_id,
        "size": format_bytes(block_content.len()),
        "size_bytes": block_content.len(),
        "lines": block_lines.len(),
        "preview": preview,
        "action": action,
        "expires_in": format_duration_from_now(BUFFER_TTL_SECONDS),
    }))
}

/// Insert hash content at specified line
pub async fn tool_insert_hash(args: Value, ctx: &ToolContext) -> Result<Value> {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| goferError::InvalidParams("path is required".into()))?;

    let line_number = args
        .get("line_number")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| goferError::InvalidParams("line_number is required".into()))? as usize;

    let hash_id = args
        .get("hash_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| goferError::InvalidParams("hash_id is required".into()))?;

    // Get buffer content
    let buffer_content = {
        let mut buffers = BUFFERS.write().await;
        let buffer = buffers
            .get_mut(hash_id)
            .ok_or_else(|| goferError::InvalidParams(format!("Hash not found: {}", hash_id)))?;

        // Check expiration
        if Utc::now() > buffer.expires_at {
            return Err(
                goferError::InvalidParams(format!("Hash expired: {}", hash_id)).into()
            );
        }

        // Increment access count
        buffer.access_count += 1;

        buffer.content.clone()
    };

    let abs_path = resolve_path_buf(&ctx.root_path, path);

    // Read file (or create if doesn't exist)
    let mut lines = if abs_path.exists() {
        let content = tokio::fs::read_to_string(&abs_path).await?;
        content.lines().map(String::from).collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    // Insert at line (1-indexed)
    let insert_idx = if line_number == 0 {
        0
    } else {
        (line_number - 1).min(lines.len())
    };

    // Split buffer content into lines
    let buffer_lines: Vec<String> = buffer_content.lines().map(String::from).collect();

    // Insert
    for (i, line) in buffer_lines.iter().enumerate() {
        lines.insert(insert_idx + i, line.clone());
    }

    // Write back
    let new_content = lines.join("\n");
    
    // Create parent directories if needed
    if let Some(parent) = abs_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    
    tokio::fs::write(&abs_path, new_content).await?;

    // Invalidate cache
    ctx.cache.invalidate_file(path).await;

    Ok(json!({
        "path": path,
        "hash_id": hash_id,
        "inserted_at_line": line_number,
        "lines_inserted": buffer_lines.len(),
        "status": "inserted",
    }))
}

/// Replace code block with hash content
pub async fn tool_replace_with_hash(args: Value, ctx: &ToolContext) -> Result<Value> {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| goferError::InvalidParams("path is required".into()))?;

    let start_line = args
        .get("start_line")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| goferError::InvalidParams("start_line is required".into()))? as usize;

    let end_line = args
        .get("end_line")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| goferError::InvalidParams("end_line is required".into()))? as usize;

    let hash_id = args
        .get("hash_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| goferError::InvalidParams("hash_id is required".into()))?;

    // Get buffer content
    let buffer_content = {
        let mut buffers = BUFFERS.write().await;
        let buffer = buffers
            .get_mut(hash_id)
            .ok_or_else(|| goferError::InvalidParams(format!("Hash not found: {}", hash_id)))?;

        if Utc::now() > buffer.expires_at {
            return Err(
                goferError::InvalidParams(format!("Hash expired: {}", hash_id)).into()
            );
        }

        buffer.access_count += 1;
        buffer.content.clone()
    };

    let abs_path = resolve_path_buf(&ctx.root_path, path);

    if !abs_path.exists() {
        return Err(goferError::InvalidParams(format!("File not found: {}", path)).into());
    }

    // Read file
    let content = tokio::fs::read_to_string(&abs_path).await?;
    let mut lines: Vec<String> = content.lines().map(String::from).collect();

    if start_line < 1 || end_line > lines.len() || start_line > end_line {
        return Err(goferError::InvalidParams(format!(
            "Invalid line range: {}-{} (file has {} lines)",
            start_line,
            end_line,
            lines.len()
        ))
        .into());
    }

    // Remove old lines
    lines.drain((start_line - 1)..end_line);

    // Insert new content
    let buffer_lines: Vec<String> = buffer_content.lines().map(String::from).collect();
    for (i, line) in buffer_lines.iter().enumerate() {
        lines.insert(start_line - 1 + i, line.clone());
    }

    // Write back
    let new_content = lines.join("\n");
    tokio::fs::write(&abs_path, new_content).await?;

    // Invalidate cache
    ctx.cache.invalidate_file(path).await;

    Ok(json!({
        "path": path,
        "hash_id": hash_id,
        "replaced_lines": format!("{}-{}", start_line, end_line),
        "new_lines": buffer_lines.len(),
        "status": "replaced",
    }))
}

/// Create hash from arbitrary content
pub async fn tool_content_to_hash(args: Value, _ctx: &ToolContext) -> Result<Value> {
    let content = args
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| goferError::InvalidParams("content is required".into()))?;

    if content.len() > MAX_BUFFER_SIZE_BYTES {
        return Err(goferError::InvalidParams(format!(
            "Content size {} exceeds max {} bytes",
            content.len(),
            MAX_BUFFER_SIZE_BYTES
        ))
        .into());
    }

    let hash_id = calculate_hash(content);
    let lines = content.lines().count();

    let buffer = ContentBuffer {
        hash_id: hash_id.clone(),
        content: content.to_string(),
        size_bytes: content.len(),
        lines,
        created_at: Utc::now(),
        expires_at: Utc::now() + chrono::Duration::seconds(BUFFER_TTL_SECONDS),
        source_file: None,
        access_count: 0,
    };

    {
        let buffers = BUFFERS.read().await;
        if buffers.len() >= MAX_BUFFERS {
            drop(buffers);
            cleanup_expired_buffers().await;
        }
    }

    {
        let mut buffers = BUFFERS.write().await;
        buffers.insert(hash_id.clone(), buffer);
    }

    // Generate preview
    let preview_lines: Vec<&str> = content.lines().take(3).collect();
    let mut preview = preview_lines.join("\n");
    if lines > 3 {
        preview.push_str("\n...");
    }

    Ok(json!({
        "hash_id": hash_id,
        "size": format_bytes(content.len()),
        "lines": lines,
        "preview": preview,
        "expires_in": format_duration_from_now(BUFFER_TTL_SECONDS),
    }))
}

/// List all active buffers
pub async fn tool_list_buffers(_args: Value, _ctx: &ToolContext) -> Result<Value> {
    // Cleanup expired first
    cleanup_expired_buffers().await;

    let buffers = BUFFERS.read().await;

    let buffer_list: Vec<Value> = buffers
        .values()
        .map(|buf| {
            let age = Utc::now().signed_duration_since(buf.created_at);
            let ttl = buf.expires_at.signed_duration_since(Utc::now());

            json!({
                "hash_id": buf.hash_id,
                "size": format_bytes(buf.size_bytes),
                "lines": buf.lines,
                "source_file": buf.source_file,
                "age": format_duration(age),
                "expires_in": format_duration(ttl),
                "access_count": buf.access_count,
            })
        })
        .collect();

    let total_size: usize = buffers.values().map(|b| b.size_bytes).sum();

    Ok(json!({
        "buffers": buffer_list,
        "total_buffers": buffer_list.len(),
        "total_size": format_bytes(total_size),
    }))
}

/// Clear specific buffer or all buffers
pub async fn tool_clear_buffer(args: Value, _ctx: &ToolContext) -> Result<Value> {
    let hash_id = args.get("hash_id").and_then(|v| v.as_str());

    let mut buffers = BUFFERS.write().await;

    if let Some(id) = hash_id {
        // Clear specific buffer
        if buffers.remove(id).is_some() {
            Ok(json!({
                "hash_id": id,
                "status": "cleared",
            }))
        } else {
            Err(goferError::InvalidParams(format!("Hash not found: {}", id)).into())
        }
    } else {
        // Clear all buffers
        let count = buffers.len();
        buffers.clear();

        Ok(json!({
            "status": "all_cleared",
            "buffers_cleared": count,
        }))
    }
}

// Helper functions

fn calculate_hash(content: &str) -> String {
    use blake3::Hasher;
    let mut hasher = Hasher::new();
    hasher.update(content.as_bytes());
    let hash = hasher.finalize();
    // Use first 8 chars of hex for short hash
    format!("{:.8}", hash.to_hex())
}

async fn cleanup_expired_buffers() {
    let mut buffers = BUFFERS.write().await;
    let now = Utc::now();

    buffers.retain(|_, buf| buf.expires_at > now);
}

fn format_bytes(bytes: usize) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    if unit_idx == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.1} {}", size, UNITS[unit_idx])
    }
}

fn format_duration(duration: chrono::Duration) -> String {
    let seconds = duration.num_seconds().abs();
    if seconds < 60 {
        format!("{}s", seconds)
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else if seconds < 86400 {
        format!("{}h", seconds / 3600)
    } else {
        format!("{}d", seconds / 86400)
    }
}

fn format_duration_from_now(seconds: i64) -> String {
    format_duration(chrono::Duration::seconds(seconds))
}
