//! Extended MCP tools for rust-analyzer (architecture navigation and refactoring).

use anyhow::Result;
use serde_json::{json, Value};

use crate::daemon::handlers::common::ToolContext;

/// Tool: rust_document_symbols
/// Get document outline (structures, functions, enums, traits) for a Rust file.
#[allow(dead_code)]
pub async fn tool_rust_document_symbols(args: Value, ctx: &ToolContext) -> Result<Value> {
    let file_path = args["file_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing file_path"))?;

    let rust_analyzer = ctx.get_rust_analyzer().await?;

    let abs_path = if std::path::Path::new(file_path).is_absolute() {
        file_path.to_string()
    } else {
        ctx.root_path.join(file_path).to_string_lossy().to_string()
    };

    let content = tokio::fs::read_to_string(&abs_path).await?;
    rust_analyzer
        .did_open(std::path::Path::new(&abs_path), content)
        .await?;

    let symbols = rust_analyzer
        .document_symbols(std::path::Path::new(&abs_path))
        .await?;

    let results: Vec<_> = symbols
        .into_iter()
        .map(|sym| {
            json!({
                "name": sym.name,
                "kind": format!("{:?}", sym.kind),
                "detail": sym.detail,
                "range": {
                    "start": { "line": sym.range.start.line, "character": sym.range.start.character },
                    "end": { "line": sym.range.end.line, "character": sym.range.end.character }
                },
                "selection_range": {
                    "start": { "line": sym.selection_range.start.line, "character": sym.selection_range.start.character },
                    "end": { "line": sym.selection_range.end.line, "character": sym.selection_range.end.character }
                },
                "children": sym.children.map(|children| {
                    children.into_iter().map(|child| json!({
                        "name": child.name,
                        "kind": format!("{:?}", child.kind),
                        "range": {
                            "start": { "line": child.range.start.line, "character": child.range.start.character },
                            "end": { "line": child.range.end.line, "character": child.range.end.character }
                        }
                    })).collect::<Vec<_>>()
                })
            })
        })
        .collect();

    Ok(json!({ "symbols": results }))
}

/// Tool: rust_workspace_symbols
/// Search for symbols across the entire workspace by name.
#[allow(dead_code)]
pub async fn tool_rust_workspace_symbols(args: Value, ctx: &ToolContext) -> Result<Value> {
    let query = args["query"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing query"))?;

    let rust_analyzer = ctx.get_rust_analyzer().await?;

    let symbols = rust_analyzer.workspace_symbols(query).await?;

    let results: Vec<_> = symbols
        .into_iter()
        .map(|sym| {
            json!({
                "name": sym.name,
                "kind": format!("{:?}", sym.kind),
                "container_name": sym.container_name,
                "location": {
                    "file": sym.location.uri.path().as_str(),
                    "range": {
                        "start": { "line": sym.location.range.start.line, "character": sym.location.range.start.character },
                        "end": { "line": sym.location.range.end.line, "character": sym.location.range.end.character }
                    }
                }
            })
        })
        .collect();

    Ok(json!({ "symbols": results }))
}

/// Tool: rust_goto_implementation
/// Go to concrete implementation(s) of a trait method or type.
#[allow(dead_code)]
pub async fn tool_rust_goto_implementation(args: Value, ctx: &ToolContext) -> Result<Value> {
    let file_path = args["file_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing file_path"))?;
    let line = args["line"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Missing line"))? as u32;
    let character = args["character"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Missing character"))? as u32;

    let rust_analyzer = ctx.get_rust_analyzer().await?;

    let abs_path = if std::path::Path::new(file_path).is_absolute() {
        file_path.to_string()
    } else {
        ctx.root_path.join(file_path).to_string_lossy().to_string()
    };

    let content = tokio::fs::read_to_string(&abs_path).await?;
    rust_analyzer
        .did_open(std::path::Path::new(&abs_path), content)
        .await?;

    let locations = rust_analyzer
        .goto_implementation(std::path::Path::new(&abs_path), line, character)
        .await?;

    let results: Vec<_> = locations
        .into_iter()
        .map(|loc| {
            json!({
                "file": loc.uri.path().as_str(),
                "line": loc.range.start.line,
                "character": loc.range.start.character,
                "end_line": loc.range.end.line,
                "end_character": loc.range.end.character,
            })
        })
        .collect();

    Ok(json!({ "implementations": results }))
}

/// Tool: rust_rename
/// Rename a symbol across the entire workspace (safe semantic rename).
#[allow(dead_code)]
pub async fn tool_rust_rename(args: Value, ctx: &ToolContext) -> Result<Value> {
    let file_path = args["file_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing file_path"))?;
    let line = args["line"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Missing line"))? as u32;
    let character = args["character"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Missing character"))? as u32;
    let new_name = args["new_name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing new_name"))?;

    let rust_analyzer = ctx.get_rust_analyzer().await?;

    let abs_path = if std::path::Path::new(file_path).is_absolute() {
        file_path.to_string()
    } else {
        ctx.root_path.join(file_path).to_string_lossy().to_string()
    };

    let content = tokio::fs::read_to_string(&abs_path).await?;
    rust_analyzer
        .did_open(std::path::Path::new(&abs_path), content)
        .await?;

    let workspace_edit = rust_analyzer
        .rename(std::path::Path::new(&abs_path), line, character, new_name)
        .await?;

    match workspace_edit {
        Some(edit) => {
            let mut changes = Vec::new();

            if let Some(document_changes) = edit.document_changes {
                match document_changes {
                    lsp_types::DocumentChanges::Edits(edits) => {
                        for edit in edits {
                            changes.push(json!({
                                "file": edit.text_document.uri.path().as_str(),
                                "edits": edit.edits.into_iter().map(|e| {
                                    let text = match e {
                                        lsp_types::OneOf::Left(te) => te.new_text,
                                        lsp_types::OneOf::Right(ae) => ae.text_edit.new_text,
                                    };
                                    json!({ "new_text": text })
                                }).collect::<Vec<_>>()
                            }));
                        }
                    }
                    lsp_types::DocumentChanges::Operations(ops) => {
                        for op in ops {
                            if let lsp_types::DocumentChangeOperation::Edit(edit) = op {
                                changes.push(json!({
                                    "file": edit.text_document.uri.path().as_str(),
                                    "edits": edit.edits.len()
                                }));
                            }
                        }
                    }
                }
            } else if let Some(change_map) = edit.changes {
                for (uri, edits) in change_map {
                    changes.push(json!({
                        "file": uri.path().as_str(),
                        "edits": edits.len()
                    }));
                }
            }

            Ok(json!({
                "success": true,
                "changes": changes,
                "total_files": changes.len()
            }))
        }
        None => Ok(json!({
            "success": false,
            "message": "No rename available at this position"
        })),
    }
}

/// Tool: rust_expand_macro
/// Expand Rust macro at position to see generated code.
#[allow(dead_code)]
pub async fn tool_rust_expand_macro(args: Value, ctx: &ToolContext) -> Result<Value> {
    let file_path = args["file_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing file_path"))?;
    let line = args["line"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Missing line"))? as u32;
    let character = args["character"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Missing character"))? as u32;

    let rust_analyzer = ctx.get_rust_analyzer().await?;

    let abs_path = if std::path::Path::new(file_path).is_absolute() {
        file_path.to_string()
    } else {
        ctx.root_path.join(file_path).to_string_lossy().to_string()
    };

    let content = tokio::fs::read_to_string(&abs_path).await?;
    rust_analyzer
        .did_open(std::path::Path::new(&abs_path), content)
        .await?;

    let expansion = rust_analyzer
        .expand_macro(std::path::Path::new(&abs_path), line, character)
        .await?;

    match expansion {
        Some(exp) => Ok(json!({
            "success": true,
            "expansion": exp
        })),
        None => Ok(json!({
            "success": false,
            "message": "No macro at this position"
        })),
    }
}

/// Tool: rust_incoming_calls
/// Get incoming calls (callers) for a function/method at position.
#[allow(dead_code)]
pub async fn tool_rust_incoming_calls(args: Value, ctx: &ToolContext) -> Result<Value> {
    let file_path = args["file_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing file_path"))?;
    let line = args["line"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Missing line"))? as u32;
    let character = args["character"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Missing character"))? as u32;

    let rust_analyzer = ctx.get_rust_analyzer().await?;

    let abs_path = if std::path::Path::new(file_path).is_absolute() {
        file_path.to_string()
    } else {
        ctx.root_path.join(file_path).to_string_lossy().to_string()
    };

    let content = tokio::fs::read_to_string(&abs_path).await?;
    rust_analyzer
        .did_open(std::path::Path::new(&abs_path), content)
        .await?;

    let items = rust_analyzer
        .prepare_call_hierarchy(std::path::Path::new(&abs_path), line, character)
        .await?;

    if items.is_empty() {
        return Ok(json!({ "calls": [] }));
    }

    let incoming = rust_analyzer.incoming_calls(items[0].clone()).await?;

    let results: Vec<_> = incoming
        .into_iter()
        .map(|call| {
            json!({
                "from": {
                    "name": call.from.name,
                    "kind": format!("{:?}", call.from.kind),
                    "file": call.from.uri.path().as_str(),
                    "range": {
                        "start": { "line": call.from.range.start.line, "character": call.from.range.start.character },
                        "end": { "line": call.from.range.end.line, "character": call.from.range.end.character }
                    }
                },
                "from_ranges": call.from_ranges.into_iter().map(|r| json!({
                    "start": { "line": r.start.line, "character": r.start.character },
                    "end": { "line": r.end.line, "character": r.end.character }
                })).collect::<Vec<_>>()
            })
        })
        .collect();

    Ok(json!({ "calls": results }))
}

/// Tool: rust_outgoing_calls
/// Get outgoing calls (callees) for a function/method at position.
#[allow(dead_code)]
pub async fn tool_rust_outgoing_calls(args: Value, ctx: &ToolContext) -> Result<Value> {
    let file_path = args["file_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing file_path"))?;
    let line = args["line"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Missing line"))? as u32;
    let character = args["character"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Missing character"))? as u32;

    let rust_analyzer = ctx.get_rust_analyzer().await?;

    let abs_path = if std::path::Path::new(file_path).is_absolute() {
        file_path.to_string()
    } else {
        ctx.root_path.join(file_path).to_string_lossy().to_string()
    };

    let content = tokio::fs::read_to_string(&abs_path).await?;
    rust_analyzer
        .did_open(std::path::Path::new(&abs_path), content)
        .await?;

    let items = rust_analyzer
        .prepare_call_hierarchy(std::path::Path::new(&abs_path), line, character)
        .await?;

    if items.is_empty() {
        return Ok(json!({ "calls": [] }));
    }

    let outgoing = rust_analyzer.outgoing_calls(items[0].clone()).await?;

    let results: Vec<_> = outgoing
        .into_iter()
        .map(|call| {
            json!({
                "to": {
                    "name": call.to.name,
                    "kind": format!("{:?}", call.to.kind),
                    "file": call.to.uri.path().as_str(),
                    "range": {
                        "start": { "line": call.to.range.start.line, "character": call.to.range.start.character },
                        "end": { "line": call.to.range.end.line, "character": call.to.range.end.character }
                    }
                },
                "from_ranges": call.from_ranges.into_iter().map(|r| json!({
                    "start": { "line": r.start.line, "character": r.start.character },
                    "end": { "line": r.end.line, "character": r.end.character }
                })).collect::<Vec<_>>()
            })
        })
        .collect();

    Ok(json!({ "calls": results }))
}
