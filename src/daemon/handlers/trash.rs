//! Trash management - safe deletion with recovery
//!
//! Implements:
//! - delete_safe - move files to trash instead of permanent deletion
//! - list_trash - show trash contents
//! - restore - restore files from trash
//! - purge_trash - permanently delete from trash

use super::common::{make_relative_pathbuf, resolve_path_buf, ToolContext};
use crate::error::goferError;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrashMetadata {
    pub deletion_uuid: String,
    pub original_path: String,
    pub deleted_at: DateTime<Utc>,
    pub deleted_by: String,
    pub reason: Option<String>,
    pub file_type: String, // "file" or "directory"
    pub size_bytes: u64,
    pub tags: Vec<String>,
}

impl TrashMetadata {
    fn metadata_path(trash_root: &Path, uuid: &str) -> PathBuf {
        trash_root.join(uuid).join("metadata.json")
    }

    fn content_path(trash_root: &Path, uuid: &str) -> PathBuf {
        trash_root.join(uuid).join("content")
    }

    async fn load(trash_root: &Path, uuid: &str) -> Result<Self> {
        let path = Self::metadata_path(trash_root, uuid);
        let content = tokio::fs::read_to_string(path).await?;
        let metadata: Self = serde_json::from_str(&content)?;
        Ok(metadata)
    }

    async fn save(&self, trash_root: &Path) -> Result<()> {
        let metadata_path = Self::metadata_path(trash_root, &self.deletion_uuid);
        if let Some(parent) = metadata_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let json = serde_json::to_string_pretty(self)?;
        tokio::fs::write(metadata_path, json).await?;
        Ok(())
    }
}

/// Get trash root directory for the project
fn get_trash_root(ctx: &ToolContext) -> PathBuf {
    // Use project-specific trash: <project-root>/.gofer/trash/
    ctx.root_path.join(".gofer").join("trash")
}

/// Delete file/directory safely by moving to trash
pub async fn tool_delete_safe(args: Value, ctx: &ToolContext) -> Result<Value> {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| goferError::InvalidParams("path is required".into()))?;

    let reason = args.get("reason").and_then(|v| v.as_str());

    let tags = args
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let abs_path = resolve_path_buf(&ctx.root_path, path);

    if !abs_path.exists() {
        return Err(goferError::InvalidParams(format!("Path not found: {}", path)).into());
    }

    let trash_root = get_trash_root(ctx);
    tokio::fs::create_dir_all(&trash_root).await?;

    let deletion_uuid = Uuid::new_v4().to_string();
    let is_dir = abs_path.is_dir();

    // Calculate size
    let size_bytes = if is_dir {
        calculate_dir_size(&abs_path).await?
    } else {
        tokio::fs::metadata(&abs_path).await?.len()
    };

    // Create metadata
    let metadata = TrashMetadata {
        deletion_uuid: deletion_uuid.clone(),
        original_path: path.to_string(),
        deleted_at: Utc::now(),
        deleted_by: "ai_agent".to_string(),
        reason: reason.map(String::from),
        file_type: if is_dir { "directory" } else { "file" }.to_string(),
        size_bytes,
        tags,
    };

    // Save metadata
    metadata.save(&trash_root).await?;

    // Move content to trash
    let content_path = TrashMetadata::content_path(&trash_root, &deletion_uuid);
    if let Some(parent) = content_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // Use rename (move) for atomic operation
    tokio::fs::rename(&abs_path, &content_path).await?;

    // Invalidate cache
    ctx.cache.invalidate_file(path).await;

    Ok(json!({
        "deletion_uuid": deletion_uuid,
        "original_path": path,
        "size_saved": format_bytes(size_bytes),
        "timestamp": metadata.deleted_at.to_rfc3339(),
        "trash_location": make_relative_pathbuf(&ctx.root_path, &content_path),
    }))
}

/// List trash contents
pub async fn tool_list_trash(_args: Value, ctx: &ToolContext) -> Result<Value> {
    let trash_root = get_trash_root(ctx);

    if !trash_root.exists() {
        return Ok(json!({
            "total_items": 0,
            "total_size": 0,
            "total_size_human": "0 B",
            "items": [],
        }));
    }

    let mut items = Vec::new();
    let mut total_size = 0u64;

    // Read all trash entries
    let mut entries = tokio::fs::read_dir(&trash_root).await?;
    while let Some(entry) = entries.next_entry().await? {
        let uuid = entry.file_name().to_string_lossy().to_string();

        // Try to load metadata
        if let Ok(metadata) = TrashMetadata::load(&trash_root, &uuid).await {
            total_size += metadata.size_bytes;

            let deleted_ago = format_duration(Utc::now().signed_duration_since(metadata.deleted_at));

            items.push(json!({
                "deletion_uuid": metadata.deletion_uuid,
                "original_path": metadata.original_path,
                "deleted_at": deleted_ago,
                "size": format_bytes(metadata.size_bytes),
                "reason": metadata.reason,
                "tags": metadata.tags,
                "file_type": metadata.file_type,
            }));
        }
    }

    // Sort by deletion time (newest first)
    items.sort_by(|a, b| {
        let a_uuid = a["deletion_uuid"].as_str().unwrap_or("");
        let b_uuid = b["deletion_uuid"].as_str().unwrap_or("");
        b_uuid.cmp(a_uuid)
    });

    Ok(json!({
        "total_items": items.len(),
        "total_size": total_size,
        "total_size_human": format_bytes(total_size),
        "items": items,
    }))
}

/// Restore file from trash
pub async fn tool_restore(args: Value, ctx: &ToolContext) -> Result<Value> {
    let deletion_uuid = args
        .get("deletion_uuid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| goferError::InvalidParams("deletion_uuid is required".into()))?;

    let target_path = args.get("target_path").and_then(|v| v.as_str());

    let trash_root = get_trash_root(ctx);
    let metadata = TrashMetadata::load(&trash_root, deletion_uuid).await?;

    let restore_path = if let Some(target) = target_path {
        resolve_path_buf(&ctx.root_path, target)
    } else {
        resolve_path_buf(&ctx.root_path, &metadata.original_path)
    };

    // Check for conflict
    if restore_path.exists() {
        return Ok(json!({
            "status": "conflict",
            "message": format!("Path already exists: {}", restore_path.display()),
            "original_path": metadata.original_path,
            "suggestion": "Use target_path parameter to restore to a different location",
        }));
    }

    // Create parent directory if needed
    if let Some(parent) = restore_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // Move content back
    let content_path = TrashMetadata::content_path(&trash_root, deletion_uuid);
    tokio::fs::rename(&content_path, &restore_path).await?;

    // Remove metadata
    let metadata_path = TrashMetadata::metadata_path(&trash_root, deletion_uuid);
    tokio::fs::remove_file(metadata_path).await?;

    // Remove trash entry directory
    let entry_dir = trash_root.join(deletion_uuid);
    if entry_dir.exists() {
        tokio::fs::remove_dir(&entry_dir).await?;
    }

    Ok(json!({
        "status": "restored",
        "path": make_relative_pathbuf(&ctx.root_path, &restore_path),
        "size": format_bytes(metadata.size_bytes),
    }))
}

/// Permanently delete from trash
pub async fn tool_purge_trash(args: Value, ctx: &ToolContext) -> Result<Value> {
    let deletion_uuid = args.get("deletion_uuid").and_then(|v| v.as_str());

    let trash_root = get_trash_root(ctx);

    if !trash_root.exists() {
        return Ok(json!({
            "deleted_count": 0,
            "freed_space": "0 B",
        }));
    }

    let mut deleted_count = 0;
    let mut freed_space = 0u64;

    if let Some(uuid) = deletion_uuid {
        // Delete specific item
        if let Ok(metadata) = TrashMetadata::load(&trash_root, uuid).await {
            freed_space += metadata.size_bytes;

            let content_path = TrashMetadata::content_path(&trash_root, uuid);
            if content_path.exists() {
                if content_path.is_dir() {
                    tokio::fs::remove_dir_all(&content_path).await?;
                } else {
                    tokio::fs::remove_file(&content_path).await?;
                }
            }

            let metadata_path = TrashMetadata::metadata_path(&trash_root, uuid);
            if metadata_path.exists() {
                tokio::fs::remove_file(metadata_path).await?;
            }

            let entry_dir = trash_root.join(uuid);
            if entry_dir.exists() {
                tokio::fs::remove_dir(&entry_dir).await?;
            }

            deleted_count = 1;
        }
    } else {
        // Delete all (purge entire trash)
        let mut entries = tokio::fs::read_dir(&trash_root).await?;
        while let Some(entry) = entries.next_entry().await? {
            let uuid = entry.file_name().to_string_lossy().to_string();

            if let Ok(metadata) = TrashMetadata::load(&trash_root, &uuid).await {
                freed_space += metadata.size_bytes;
                deleted_count += 1;
            }

            // Remove entire entry directory
            let entry_path = entry.path();
            if entry_path.is_dir() {
                tokio::fs::remove_dir_all(&entry_path).await?;
            }
        }
    }

    Ok(json!({
        "deleted_count": deleted_count,
        "freed_space": format_bytes(freed_space),
    }))
}

// Helper functions

async fn calculate_dir_size(path: &Path) -> Result<u64> {
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];

    while let Some(current) = stack.pop() {
        let mut entries = tokio::fs::read_dir(&current).await?;
        while let Some(entry) = entries.next_entry().await? {
            let metadata = entry.metadata().await?;
            if metadata.is_dir() {
                stack.push(entry.path());
            } else {
                total += metadata.len();
            }
        }
    }

    Ok(total)
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
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
    let seconds = duration.num_seconds();
    if seconds < 60 {
        format!("{}s ago", seconds)
    } else if seconds < 3600 {
        format!("{}m ago", seconds / 60)
    } else if seconds < 86400 {
        format!("{}h ago", seconds / 3600)
    } else {
        format!("{}d ago", seconds / 86400)
    }
}
