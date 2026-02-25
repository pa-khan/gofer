use super::common::ToolContext;
use crate::error::goferError;
use crate::indexer::git::GitRepo;
use anyhow::Result;
use serde_json::{json, Value};

pub async fn tool_git_blame(args: Value, ctx: &ToolContext) -> Result<Value> {
    let file = args.get("file").and_then(|v| v.as_str()).unwrap_or("");
    let line = args.get("line").and_then(|v| v.as_u64()).unwrap_or(1) as u32;

    if file.is_empty() {
        return Err(goferError::InvalidParams("File path is required".into()).into());
    }

    let repo = match GitRepo::open(&ctx.root_path) {
        Some(r) => r,
        None => return Err(goferError::InvalidParams("Not a git repository".into()).into()),
    };
    let file_path = &ctx.root_path.join(file);

    match repo.line_history(file_path, line) {
        Some(blame) => Ok(json!({
            "file": file,
            "line": line,
            "author": blame.author,
            "date": blame.timestamp,
            "commit": &blame.commit_id[..8.min(blame.commit_id.len())],
            "message": blame.message
        })),
        None => Ok(json!({
            "file": file,
            "line": line,
            "message": format!("No blame info for {}:{}", file, line)
        })),
    }
}

pub async fn tool_git_history(args: Value, ctx: &ToolContext) -> Result<Value> {
    let file = args.get("file").and_then(|v| v.as_str()).unwrap_or("");
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

    if file.is_empty() {
        return Err(goferError::InvalidParams("File path is required".into()).into());
    }

    let repo = match GitRepo::open(&ctx.root_path) {
        Some(r) => r,
        None => return Err(goferError::InvalidParams("Not a git repository".into()).into()),
    };
    let file_path = &ctx.root_path.join(file);
    let history = repo.file_history(file_path, limit);

    Ok(json!({
        "file": file,
        "total": history.len(),
        "commits": history.iter().map(|c| json!({
            "id": &c.id[..8.min(c.id.len())],
            "author": c.author,
            "email": c.email,
            "date": c.timestamp,
            "message": c.message
        })).collect::<Vec<_>>()
    }))
}

pub async fn tool_git_diff(args: Value, ctx: &ToolContext) -> Result<Value> {
    let file = args.get("file").and_then(|v| v.as_str());
    let staged = args
        .get("staged")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let repo = match GitRepo::open(&ctx.root_path) {
        Some(r) => r,
        None => return Err(goferError::InvalidParams("Not a git repository".into()).into()),
    };

    let file_path = file.map(|f| ctx.root_path.join(f));

    match repo.git_diff(file_path.as_deref(), staged) {
        Some(diff_text) => Ok(json!({
            "file": file.unwrap_or("(all)"),
            "staged": staged,
            "diff": diff_text
        })),
        None => Ok(json!({
            "file": file.unwrap_or("(all)"),
            "staged": staged,
            "diff": "",
            "message": "No changes found"
        })),
    }
}

pub async fn tool_suggest_commit(args: Value, ctx: &ToolContext) -> Result<Value> {
    let style = args
        .get("style")
        .and_then(|v| v.as_str())
        .unwrap_or("conventional");

    let include_emoji = args
        .get("include_emoji")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    // Use root_path as repository path
    let repo_path = &ctx.root_path;

    // Call commit analyzer
    let suggestion = crate::commit::suggest_commit_message(repo_path, include_emoji, style).await?;

    Ok(serde_json::to_value(suggestion)?)
}

pub async fn tool_verify_patch(args: Value, ctx: &ToolContext) -> Result<Value> {
    let file = args.get("file").and_then(|v| v.as_str()).unwrap_or("");
    let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");

    if file.is_empty() || content.is_empty() {
        return Err(
            goferError::InvalidParams("Both 'file' and 'content' are required".into()).into(),
        );
    }

    let result = crate::indexer::diagnostics::verify_patch(&ctx.root_path, file, content).await?;

    Ok(json!({
        "file": file,
        "status": result.status,
        "summary": result.summary,
        "diagnostics": result.diagnostics.iter().map(|d| json!({
            "severity": d.severity,
            "line": d.line,
            "column": d.column,
            "code": d.code,
            "message": d.message,
            "suggestion": d.suggestion
        })).collect::<Vec<_>>()
    }))
}
