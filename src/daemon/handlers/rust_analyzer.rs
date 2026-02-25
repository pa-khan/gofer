//! MCP tools for rust-analyzer integration.

use anyhow::Result;
use serde_json::{json, Value};

use crate::daemon::handlers::common::ToolContext;

/// Tool: rust_goto_definition
/// Go to definition for Rust symbol at position.
pub async fn tool_rust_goto_definition(args: Value, ctx: &ToolContext) -> Result<Value> {
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

    // Ensure file is opened in rust-analyzer
    let abs_path = if std::path::Path::new(file_path).is_absolute() {
        file_path.to_string()
    } else {
        ctx.root_path.join(file_path).to_string_lossy().to_string()
    };
    
    let content = tokio::fs::read_to_string(&abs_path).await?;
    rust_analyzer.did_open(std::path::Path::new(&abs_path), content).await?;

    let locations = rust_analyzer
        .goto_definition(std::path::Path::new(&abs_path), line, character)
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

    Ok(json!({ "definitions": results }))
}

/// Tool: rust_find_references
/// Find all references to Rust symbol at position.
pub async fn tool_rust_find_references(args: Value, ctx: &ToolContext) -> Result<Value> {
    let file_path = args["file_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing file_path"))?;
    let line = args["line"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Missing line"))? as u32;
    let character = args["character"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Missing character"))? as u32;
    let include_declaration = args["include_declaration"].as_bool().unwrap_or(true);

    let rust_analyzer = ctx.get_rust_analyzer().await?;

    let abs_path = if std::path::Path::new(file_path).is_absolute() {
        file_path.to_string()
    } else {
        ctx.root_path.join(file_path).to_string_lossy().to_string()
    };
    
    let content = tokio::fs::read_to_string(&abs_path).await?;
    rust_analyzer.did_open(std::path::Path::new(&abs_path), content).await?;

    let locations = rust_analyzer
        .find_references(
            std::path::Path::new(&abs_path),
            line,
            character,
            include_declaration,
        )
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

    Ok(json!({ "references": results }))
}

/// Tool: rust_hover
/// Get hover information (type, docs) for Rust symbol at position.
pub async fn tool_rust_hover(args: Value, ctx: &ToolContext) -> Result<Value> {
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
    rust_analyzer.did_open(std::path::Path::new(&abs_path), content).await?;

    let hover = rust_analyzer
        .hover(std::path::Path::new(&abs_path), line, character)
        .await?;

    match hover {
        Some(h) => {
            let content = match h.contents {
                lsp_types::HoverContents::Scalar(marked) => match marked {
                    lsp_types::MarkedString::String(s) => s,
                    lsp_types::MarkedString::LanguageString(ls) => {
                        format!("```{}\n{}\n```", ls.language, ls.value)
                    }
                },
                lsp_types::HoverContents::Array(arr) => arr
                    .into_iter()
                    .map(|marked| match marked {
                        lsp_types::MarkedString::String(s) => s,
                        lsp_types::MarkedString::LanguageString(ls) => {
                            format!("```{}\n{}\n```", ls.language, ls.value)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n"),
                lsp_types::HoverContents::Markup(markup) => markup.value,
            };

            Ok(json!({
                "content": content,
                "range": h.range.map(|r| json!({
                    "start_line": r.start.line,
                    "start_character": r.start.character,
                    "end_line": r.end.line,
                    "end_character": r.end.character,
                }))
            }))
        }
        None => Ok(json!({ "content": null })),
    }
}

/// Tool: rust_diagnostics
/// Get compiler diagnostics (errors, warnings) for a Rust file.
pub async fn tool_rust_diagnostics(args: Value, ctx: &ToolContext) -> Result<Value> {
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
    rust_analyzer.did_open(std::path::Path::new(&abs_path), content).await?;

    let diagnostics = rust_analyzer
        .diagnostics(std::path::Path::new(&abs_path))
        .await?;

    let results: Vec<_> = diagnostics
        .into_iter()
        .map(|diag| {
            json!({
                "message": diag.message,
                "severity": match diag.severity {
                    Some(lsp_types::DiagnosticSeverity::ERROR) => "error",
                    Some(lsp_types::DiagnosticSeverity::WARNING) => "warning",
                    Some(lsp_types::DiagnosticSeverity::INFORMATION) => "info",
                    Some(lsp_types::DiagnosticSeverity::HINT) => "hint",
                    _ => "unknown",
                },
                "line": diag.range.start.line,
                "character": diag.range.start.character,
                "end_line": diag.range.end.line,
                "end_character": diag.range.end.character,
                "code": diag.code.map(|c| match c {
                    lsp_types::NumberOrString::Number(n) => n.to_string(),
                    lsp_types::NumberOrString::String(s) => s,
                }),
                "source": diag.source,
            })
        })
        .collect();

    Ok(json!({ "diagnostics": results }))
}

/// Tool: rust_completions
/// Get code completions for Rust at position.
pub async fn tool_rust_completions(args: Value, ctx: &ToolContext) -> Result<Value> {
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
    rust_analyzer.did_open(std::path::Path::new(&abs_path), content).await?;

    let items = rust_analyzer
        .completions(std::path::Path::new(&abs_path), line, character)
        .await?;

    let results: Vec<_> = items
        .into_iter()
        .map(|item| {
            json!({
                "label": item.label,
                "kind": format!("{:?}", item.kind),
                "detail": item.detail,
                "documentation": item.documentation.map(|doc| match doc {
                    lsp_types::Documentation::String(s) => s,
                    lsp_types::Documentation::MarkupContent(markup) => markup.value,
                }),
                "insert_text": item.insert_text,
            })
        })
        .collect();

    Ok(json!({ "completions": results }))
}

/// Tool: rust_inlay_hints
/// Get inlay hints (type annotations, parameter names) for a Rust file range.
pub async fn tool_rust_inlay_hints(args: Value, ctx: &ToolContext) -> Result<Value> {
    let file_path = args["file_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing file_path"))?;
    let start_line = args["start_line"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Missing start_line"))? as u32;
    let end_line = args["end_line"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Missing end_line"))? as u32;

    let rust_analyzer = ctx.get_rust_analyzer().await?;

    let abs_path = if std::path::Path::new(file_path).is_absolute() {
        file_path.to_string()
    } else {
        ctx.root_path.join(file_path).to_string_lossy().to_string()
    };
    
    let content = tokio::fs::read_to_string(&abs_path).await?;
    rust_analyzer.did_open(std::path::Path::new(&abs_path), content).await?;

    let hints = rust_analyzer
        .inlay_hints(std::path::Path::new(&abs_path), start_line, end_line)
        .await?;

    let results: Vec<_> = hints
        .into_iter()
        .map(|hint| {
            let label = match hint.label {
                lsp_types::InlayHintLabel::String(s) => s,
                lsp_types::InlayHintLabel::LabelParts(parts) => {
                    parts.into_iter().map(|p| p.value).collect::<Vec<_>>().join("")
                }
            };

            json!({
                "label": label,
                "kind": format!("{:?}", hint.kind),
                "line": hint.position.line,
                "character": hint.position.character,
            })
        })
        .collect();

    Ok(json!({ "hints": results }))
}

/// Tool: rust_code_actions
/// Get code actions (quick fixes, refactorings) for a Rust file range.
pub async fn tool_rust_code_actions(args: Value, ctx: &ToolContext) -> Result<Value> {
    let file_path = args["file_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing file_path"))?;
    let start_line = args["start_line"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Missing start_line"))? as u32;
    let end_line = args["end_line"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Missing end_line"))? as u32;

    let rust_analyzer = ctx.get_rust_analyzer().await?;

    let abs_path = if std::path::Path::new(file_path).is_absolute() {
        file_path.to_string()
    } else {
        ctx.root_path.join(file_path).to_string_lossy().to_string()
    };
    
    let content = tokio::fs::read_to_string(&abs_path).await?;
    rust_analyzer.did_open(std::path::Path::new(&abs_path), content).await?;

    // Get diagnostics for this range to pass as context
    let all_diagnostics = rust_analyzer
        .diagnostics(std::path::Path::new(&abs_path))
        .await?;

    let actions = rust_analyzer
        .code_actions(
            std::path::Path::new(&abs_path),
            start_line,
            end_line,
            all_diagnostics,
        )
        .await?;

    let results: Vec<_> = actions
        .into_iter()
        .map(|action| match action {
            lsp_types::CodeActionOrCommand::CodeAction(ca) => {
                json!({
                    "title": ca.title,
                    "kind": ca.kind.map(|k| k.as_str().to_string()),
                    "is_preferred": ca.is_preferred,
                })
            }
            lsp_types::CodeActionOrCommand::Command(cmd) => {
                json!({
                    "title": cmd.title,
                    "command": cmd.command,
                })
            }
        })
        .collect();

    Ok(json!({ "actions": results }))
}
