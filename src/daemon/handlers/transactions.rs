//! Atomic Transactions - Phase 2 implementation
//!
//! Implements:
//! - begin_transaction - начать транзакцию
//! - add_operation - добавить операцию в транзакцию
//! - commit_transaction - атомарно применить все операции
//! - rollback_transaction - откатить транзакцию
//! - list_transactions - показать активные транзакции

use super::common::{resolve_path_buf, ToolContext};
use super::file_ops;
use super::trash;
use crate::error::GoferError;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionStatus {
    Active,
    Committed,
    RolledBack,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    PatchFile {
        path: String,
        search_string: String,
        replace_string: String,
        occurrence: usize,
    },
    WriteFile {
        path: String,
        content: String,
        create_dirs: bool,
    },
    AppendToFile {
        path: String,
        content: String,
        newline_before: bool,
    },
    DeleteSafe {
        path: String,
        reason: Option<String>,
        tags: Vec<String>,
    },
    MoveFile {
        source: String,
        destination: String,
        overwrite: bool,
    },
    CreateDirectory {
        path: String,
        recursive: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationRecord {
    pub operation_id: String,
    pub operation: Operation,
    pub status: String, // "staged", "validated", "applied", "failed"
    pub validation_result: Option<ValidationResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub syntax_check: String, // "passed", "failed", "skipped"
    pub conflicts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSnapshot {
    pub path: String,
    pub content: Vec<u8>,
    pub existed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub transaction_id: String,
    pub operations: Vec<OperationRecord>,
    pub status: TransactionStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub snapshots: Vec<FileSnapshot>,
}

// Global transaction storage
lazy_static::lazy_static! {
    static ref TRANSACTIONS: Arc<RwLock<HashMap<String, Transaction>>> =
        Arc::new(RwLock::new(HashMap::new()));
}

/// Begin a new transaction
pub async fn tool_begin_transaction(args: Value, _ctx: &ToolContext) -> Result<Value> {
    let transaction_id = args
        .get("transaction_id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| format!("tx_{}", Uuid::new_v4()));

    let mut transactions = TRANSACTIONS.write().await;

    // Check if transaction already exists
    if transactions.contains_key(&transaction_id) {
        return Err(GoferError::InvalidParams(format!(
            "Transaction {} already exists",
            transaction_id
        ))
        .into());
    }

    let transaction = Transaction {
        transaction_id: transaction_id.clone(),
        operations: Vec::new(),
        status: TransactionStatus::Active,
        started_at: Utc::now(),
        completed_at: None,
        snapshots: Vec::new(),
    };

    transactions.insert(transaction_id.clone(), transaction);

    Ok(json!({
        "transaction_id": transaction_id,
        "status": "active",
        "started_at": Utc::now().to_rfc3339(),
    }))
}

/// Add operation to transaction
pub async fn tool_add_operation(args: Value, ctx: &ToolContext) -> Result<Value> {
    let transaction_id = args
        .get("transaction_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| GoferError::InvalidParams("transaction_id is required".into()))?;

    let operation_data = args
        .get("operation")
        .ok_or_else(|| GoferError::InvalidParams("operation is required".into()))?;

    let operation = parse_operation(operation_data)?;

    let mut transactions = TRANSACTIONS.write().await;

    let transaction = transactions.get_mut(transaction_id).ok_or_else(|| {
        GoferError::InvalidParams(format!("Transaction {} not found", transaction_id))
    })?;

    // Check if transaction is still active
    if !matches!(transaction.status, TransactionStatus::Active) {
        return Err(GoferError::InvalidParams(format!(
            "Transaction {} is not active (status: {:?})",
            transaction_id, transaction.status
        ))
        .into());
    }

    // Validate operation
    let validation = validate_operation(&operation, ctx).await?;

    let operation_id = format!("op_{:03}", transaction.operations.len() + 1);

    let record = OperationRecord {
        operation_id: operation_id.clone(),
        operation,
        status: "staged".to_string(),
        validation_result: Some(validation.clone()),
    };

    transaction.operations.push(record);

    Ok(json!({
        "operation_id": operation_id,
        "status": "staged",
        "validation": {
            "syntax_check": validation.syntax_check,
            "conflicts": validation.conflicts,
        }
    }))
}

/// Commit transaction - atomically apply all operations
pub async fn tool_commit_transaction(args: Value, ctx: &ToolContext) -> Result<Value> {
    let transaction_id = args
        .get("transaction_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| GoferError::InvalidParams("transaction_id is required".into()))?;

    let mut transactions = TRANSACTIONS.write().await;

    let transaction = transactions.get_mut(transaction_id).ok_or_else(|| {
        GoferError::InvalidParams(format!("Transaction {} not found", transaction_id))
    })?;

    // Check status
    if !matches!(transaction.status, TransactionStatus::Active) {
        return Err(GoferError::InvalidParams(format!(
            "Transaction {} is not active",
            transaction_id
        ))
        .into());
    }

    if transaction.operations.is_empty() {
        return Err(GoferError::InvalidParams("Transaction has no operations".into()).into());
    }

    // Step 1: Create snapshots of all affected files
    let mut snapshots = Vec::new();
    for op_record in &transaction.operations {
        if let Some(snapshot) = create_snapshot(&op_record.operation, ctx).await? {
            snapshots.push(snapshot);
        }
    }

    transaction.snapshots = snapshots;

    // Step 2: Apply all operations
    let mut files_changed = Vec::new();
    let mut operations_applied = 0;

    for op_record in &mut transaction.operations {
        match apply_operation(&op_record.operation, ctx).await {
            Ok(result) => {
                op_record.status = "applied".to_string();
                operations_applied += 1;

                // Collect changed files
                if let Some(path) = extract_path_from_operation(&op_record.operation) {
                    files_changed.push(path);
                }

                tracing::info!("Applied operation {}: {:?}", op_record.operation_id, result);
            }
            Err(e) => {
                // Rollback on error
                tracing::error!("Operation {} failed: {}", op_record.operation_id, e);

                // Restore from snapshots
                restore_snapshots(&transaction.snapshots, ctx).await?;

                transaction.status = TransactionStatus::Failed;
                transaction.completed_at = Some(Utc::now());

                return Ok(json!({
                    "transaction_id": transaction_id,
                    "status": "failed",
                    "error": format!("Operation {} failed: {}", op_record.operation_id, e),
                    "action": "All changes rolled back automatically",
                    "operations_applied_before_failure": operations_applied,
                }));
            }
        }
    }

    // Success - clear snapshots
    transaction.snapshots.clear();
    transaction.status = TransactionStatus::Committed;
    transaction.completed_at = Some(Utc::now());

    Ok(json!({
        "transaction_id": transaction_id,
        "status": "committed",
        "operations_applied": operations_applied,
        "files_changed": files_changed,
        "committed_at": Utc::now().to_rfc3339(),
    }))
}

/// Rollback transaction
pub async fn tool_rollback_transaction(args: Value, _ctx: &ToolContext) -> Result<Value> {
    let transaction_id = args
        .get("transaction_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| GoferError::InvalidParams("transaction_id is required".into()))?;

    let mut transactions = TRANSACTIONS.write().await;

    let transaction = transactions.get_mut(transaction_id).ok_or_else(|| {
        GoferError::InvalidParams(format!("Transaction {} not found", transaction_id))
    })?;

    if !matches!(transaction.status, TransactionStatus::Active) {
        return Err(GoferError::InvalidParams(format!(
            "Transaction {} is not active",
            transaction_id
        ))
        .into());
    }

    let operations_count = transaction.operations.len();

    transaction.status = TransactionStatus::RolledBack;
    transaction.completed_at = Some(Utc::now());
    transaction.operations.clear();

    Ok(json!({
        "transaction_id": transaction_id,
        "status": "rolled_back",
        "operations_discarded": operations_count,
    }))
}

/// List active transactions
pub async fn tool_list_transactions(_args: Value, _ctx: &ToolContext) -> Result<Value> {
    let transactions = TRANSACTIONS.read().await;

    let transaction_list: Vec<Value> = transactions
        .values()
        .map(|tx| {
            let elapsed = Utc::now().signed_duration_since(tx.started_at);
            let elapsed_str = format_duration(elapsed);

            json!({
                "transaction_id": tx.transaction_id,
                "status": format!("{:?}", tx.status),
                "operations_count": tx.operations.len(),
                "started_at": elapsed_str,
            })
        })
        .collect();

    Ok(json!({
        "transactions": transaction_list,
        "total": transaction_list.len(),
    }))
}

// Helper functions

fn parse_operation(data: &Value) -> Result<Operation> {
    let op_type = data
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| GoferError::InvalidParams("operation.type is required".into()))?;

    let params = data
        .get("params")
        .ok_or_else(|| GoferError::InvalidParams("operation.params is required".into()))?;

    match op_type {
        "patch_file" => Ok(Operation::PatchFile {
            path: params
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| GoferError::InvalidParams("path is required".into()))?
                .to_string(),
            search_string: params
                .get("search_string")
                .and_then(|v| v.as_str())
                .ok_or_else(|| GoferError::InvalidParams("search_string is required".into()))?
                .to_string(),
            replace_string: params
                .get("replace_string")
                .and_then(|v| v.as_str())
                .ok_or_else(|| GoferError::InvalidParams("replace_string is required".into()))?
                .to_string(),
            occurrence: params
                .get("occurrence")
                .and_then(|v| v.as_u64())
                .unwrap_or(1) as usize,
        }),
        "write_file" => Ok(Operation::WriteFile {
            path: params
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| GoferError::InvalidParams("path is required".into()))?
                .to_string(),
            content: params
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or_else(|| GoferError::InvalidParams("content is required".into()))?
                .to_string(),
            create_dirs: params
                .get("create_dirs")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        }),
        "append_to_file" => Ok(Operation::AppendToFile {
            path: params
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| GoferError::InvalidParams("path is required".into()))?
                .to_string(),
            content: params
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or_else(|| GoferError::InvalidParams("content is required".into()))?
                .to_string(),
            newline_before: params
                .get("newline_before")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
        }),
        "delete_safe" => Ok(Operation::DeleteSafe {
            path: params
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| GoferError::InvalidParams("path is required".into()))?
                .to_string(),
            reason: params
                .get("reason")
                .and_then(|v| v.as_str())
                .map(String::from),
            tags: params
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
        }),
        "move_file" => Ok(Operation::MoveFile {
            source: params
                .get("source")
                .and_then(|v| v.as_str())
                .ok_or_else(|| GoferError::InvalidParams("source is required".into()))?
                .to_string(),
            destination: params
                .get("destination")
                .and_then(|v| v.as_str())
                .ok_or_else(|| GoferError::InvalidParams("destination is required".into()))?
                .to_string(),
            overwrite: params
                .get("overwrite")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        }),
        "create_directory" => Ok(Operation::CreateDirectory {
            path: params
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| GoferError::InvalidParams("path is required".into()))?
                .to_string(),
            recursive: params
                .get("recursive")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
        }),
        _ => Err(GoferError::InvalidParams(format!("Unknown operation type: {}", op_type)).into()),
    }
}

async fn validate_operation(operation: &Operation, ctx: &ToolContext) -> Result<ValidationResult> {
    // Basic validation - check if file exists for operations that require it
    let path = match operation {
        Operation::PatchFile { path, .. }
        | Operation::AppendToFile { path, .. }
        | Operation::DeleteSafe { path, .. } => Some(path.as_str()),
        Operation::MoveFile { source, .. } => Some(source.as_str()),
        _ => None,
    };

    let mut conflicts = Vec::new();

    if let Some(p) = path {
        let abs_path = resolve_path_buf(&ctx.root_path, p);
        if !abs_path.exists() {
            conflicts.push(format!("File does not exist: {}", p));
        }
    }

    Ok(ValidationResult {
        syntax_check: "skipped".to_string(), // TODO: integrate with compiler
        conflicts,
    })
}

async fn create_snapshot(operation: &Operation, ctx: &ToolContext) -> Result<Option<FileSnapshot>> {
    let path = match operation {
        Operation::PatchFile { path, .. }
        | Operation::WriteFile { path, .. }
        | Operation::AppendToFile { path, .. }
        | Operation::DeleteSafe { path, .. } => path,
        Operation::MoveFile { source, .. } => source,
        Operation::CreateDirectory { .. } => return Ok(None),
    };

    let abs_path = resolve_path_buf(&ctx.root_path, path);

    if abs_path.exists() {
        let content = tokio::fs::read(&abs_path).await?;
        Ok(Some(FileSnapshot {
            path: path.clone(),
            content,
            existed: true,
        }))
    } else {
        // File doesn't exist yet (e.g., write_file for new file)
        Ok(Some(FileSnapshot {
            path: path.clone(),
            content: Vec::new(),
            existed: false,
        }))
    }
}

async fn restore_snapshots(snapshots: &[FileSnapshot], ctx: &ToolContext) -> Result<()> {
    for snapshot in snapshots {
        let abs_path = resolve_path_buf(&ctx.root_path, &snapshot.path);

        if snapshot.existed {
            // Restore original content
            tokio::fs::write(&abs_path, &snapshot.content).await?;
            tracing::info!("Restored snapshot for {}", snapshot.path);
        } else {
            // File didn't exist, remove it
            if abs_path.exists() {
                tokio::fs::remove_file(&abs_path).await?;
                tracing::info!("Removed newly created file {}", snapshot.path);
            }
        }
    }

    Ok(())
}

async fn apply_operation(operation: &Operation, ctx: &ToolContext) -> Result<Value> {
    match operation {
        Operation::PatchFile {
            path,
            search_string,
            replace_string,
            occurrence,
        } => {
            let args = json!({
                "path": path,
                "search_string": search_string,
                "replace_string": replace_string,
                "occurrence": occurrence,
            });
            file_ops::tool_patch_file(args, ctx).await
        }
        Operation::WriteFile {
            path,
            content,
            create_dirs,
        } => {
            let args = json!({
                "path": path,
                "content": content,
                "create_dirs": create_dirs,
            });
            file_ops::tool_write_file(args, ctx).await
        }
        Operation::AppendToFile {
            path,
            content,
            newline_before,
        } => {
            let args = json!({
                "path": path,
                "content": content,
                "newline_before": newline_before,
            });
            file_ops::tool_append_to_file(args, ctx).await
        }
        Operation::DeleteSafe { path, reason, tags } => {
            let args = json!({
                "path": path,
                "reason": reason,
                "tags": tags,
            });
            trash::tool_delete_safe(args, ctx).await
        }
        Operation::MoveFile {
            source,
            destination,
            overwrite,
        } => {
            let args = json!({
                "source": source,
                "destination": destination,
                "overwrite": overwrite,
            });
            file_ops::tool_move_file(args, ctx).await
        }
        Operation::CreateDirectory { path, recursive } => {
            let args = json!({
                "path": path,
                "recursive": recursive,
            });
            file_ops::tool_create_directory(args, ctx).await
        }
    }
}

fn extract_path_from_operation(operation: &Operation) -> Option<String> {
    match operation {
        Operation::PatchFile { path, .. }
        | Operation::WriteFile { path, .. }
        | Operation::AppendToFile { path, .. }
        | Operation::DeleteSafe { path, .. }
        | Operation::CreateDirectory { path, .. } => Some(path.clone()),
        Operation::MoveFile { destination, .. } => Some(destination.clone()),
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
