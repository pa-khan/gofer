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

    fn format_symbol(sym: &lsp_types::DocumentSymbol, depth: usize) -> String {
        let indent = "  ".repeat(depth);
        let mut result = format!(
            "{}{} [{:?}] {} (line {})",
            indent,
            sym.name,
            sym.kind,
            sym.detail.as_deref().unwrap_or(""),
            sym.range.start.line + 1
        );
        if let Some(ref children) = sym.children {
            for child in children {
                result.push('\n');
                result.push_str(&format_symbol(child, depth + 1));
            }
        }
        result
    }

    let results: Vec<_> = symbols
        .into_iter()
        .map(|sym| format_symbol(&sym, 0))
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

    let mut by_file: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for sym in symbols {
        let file = sym.location.uri.path().to_string();
        let container = sym
            .container_name
            .map(|c| format!(" in {}", c))
            .unwrap_or_default();
        let s = format!(
            "{}: [{:?}] {}{}",
            sym.location.range.start.line + 1,
            sym.kind,
            sym.name,
            container
        );
        by_file.entry(file).or_default().push(s);
    }

    Ok(json!({ "symbols": by_file }))
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
            format!(
                "{}:{}:{}-{}:{}",
                loc.uri.path(),
                loc.range.start.line + 1,
                loc.range.start.character + 1,
                loc.range.end.line + 1,
                loc.range.end.character + 1
            )
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
            let ranges: Vec<String> = call
                .from_ranges
                .iter()
                .map(|r| format!("{}:{}", r.start.line + 1, r.start.character + 1))
                .collect();
            format!(
                "{}:{}:{} [{:?}] {} (from ranges: {})",
                call.from.uri.path(),
                call.from.range.start.line + 1,
                call.from.range.start.character + 1,
                call.from.kind,
                call.from.name,
                ranges.join(", ")
            )
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
            let ranges: Vec<String> = call
                .from_ranges
                .iter()
                .map(|r| format!("{}:{}", r.start.line + 1, r.start.character + 1))
                .collect();
            format!(
                "{}:{}:{} [{:?}] {} (from ranges: {})",
                call.to.uri.path(),
                call.to.range.start.line + 1,
                call.to.range.start.character + 1,
                call.to.kind,
                call.to.name,
                ranges.join(", ")
            )
        })
        .collect();

    Ok(json!({ "calls": results }))
}
