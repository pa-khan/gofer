use super::common::{make_relative, resolve_path, ToolContext};
use crate::error::GoferError;
use crate::models::Rule;
use anyhow::Result;
use serde_json::{json, Value};
use walkdir::{DirEntry, WalkDir};

pub async fn tool_project_tree(args: Value, ctx: &ToolContext) -> Result<Value> {
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let depth = args.get("depth").and_then(|v| v.as_u64()).unwrap_or(3) as usize;
    let pattern = args.get("pattern").and_then(|v| v.as_str());

    let root_path = if path.is_empty() {
        ctx.root_path.as_ref().clone()
    } else {
        ctx.root_path.join(path)
    };

    if !root_path.exists() {
        return Err(GoferError::InvalidParams(format!("Path not found: {}", path)).into());
    }

    let mut tree = Vec::new();
    let walker = WalkDir::new(&root_path)
        .max_depth(depth)
        .sort_by_file_name()
        .into_iter();

    // Optional glob filter
    let glob_pat = pattern.and_then(|p| glob::Pattern::new(p).ok());

    // We filter entries but need to iterate to get them
    for e in walker
        .filter_entry(|e: &DirEntry| {
            let name = e.file_name().to_string_lossy();
            // Skip hidden and common ignored dirs
            !name.starts_with('.')
                && name != "node_modules"
                && name != "target"
                && name != "dist"
                && name != "build"
        })
        .flatten()
    {
        let relative = make_relative(&ctx.root_path, e.path().to_str().unwrap_or(""));
        if relative.is_empty() {
            continue;
        } // skip root itself if empty

        // Apply pattern filter only to files, or inclusion logic
        if let Some(ref gp) = glob_pat {
            if e.file_type().is_file() && !gp.matches_path(e.path()) {
                continue;
            }
        }

        tree.push(json!({
            "path": relative,
            "type": if e.file_type().is_dir() { "directory" } else { "file" }
        }));
    }

    Ok(json!({
        "root": path,
        "files": tree
    }))
}

pub async fn tool_get_dependencies(args: Value, ctx: &ToolContext) -> Result<Value> {
    let ecosystem = args.get("ecosystem").and_then(|v| v.as_str());

    let deps = &ctx.sqlite.get_dependencies_filtered(ecosystem).await?;

    Ok(json!({
        "total": deps.len(),
        "dependencies": deps.iter().map(|dep| json!({
            "name": dep.name,
            "version": dep.version,
            "ecosystem": dep.ecosystem,
            "dev_only": dep.dev_only == 1
        })).collect::<Vec<_>>()
    }))
}

pub async fn tool_dependency_impact(args: Value, ctx: &ToolContext) -> Result<Value> {
    let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("");

    if name.is_empty() {
        return Err(GoferError::InvalidParams("Dependency name is required".into()).into());
    }

    let usages = &ctx.sqlite.get_dependency_usage(name).await?;

    Ok(json!({
        "dependency": name,
        "total": usages.len(),
        "usages": usages.iter().map(|u| json!({
            "file_path": make_relative(&ctx.root_path, &u.file_path),
            "line": u.line,
            "usage_type": u.usage_type,
            "import_path": u.import_path
        })).collect::<Vec<_>>()
    }))
}

pub async fn tool_domain_stats(ctx: &ToolContext) -> Result<Value> {
    let stats = &ctx.sqlite.get_domain_stats().await?;

    Ok(json!({
        "domains": stats.iter().map(|(domain, count)| json!({
            "domain": domain,
            "file_count": count
        })).collect::<Vec<_>>()
    }))
}

pub async fn tool_get_api_routes(args: Value, ctx: &ToolContext) -> Result<Value> {
    let side = args.get("side").and_then(|v| v.as_str());

    let mut backend_routes = Vec::new();
    let mut frontend_calls = Vec::new();

    if side.is_none() || side == Some("backend") {
        let endpoints = &ctx.sqlite.get_api_endpoints().await?;
        backend_routes = endpoints
            .iter()
            .map(|ep| {
                json!({
                    "method": ep.method,
                    "path": ep.path
                })
            })
            .collect();
    }

    if side.is_none() || side == Some("frontend") {
        let calls = &ctx.sqlite.get_frontend_api_calls().await?;
        frontend_calls = calls
            .iter()
            .map(|call| {
                json!({
                    "method": call.method,
                    "path": call.path
                })
            })
            .collect();
    }

    Ok(json!({
        "backend": backend_routes,
        "frontend": frontend_calls
    }))
}

pub async fn tool_get_summary(args: Value, ctx: &ToolContext) -> Result<Value> {
    let file = args.get("file").and_then(|v| v.as_str()).unwrap_or("");

    if file.is_empty() {
        return Err(GoferError::InvalidParams("File path is required".into()).into());
    }

    match &ctx
        .sqlite
        .get_summary_by_path(&resolve_path(&ctx.root_path, file))
        .await?
    {
        Some(summary) => Ok(json!({
            "file": file,
            "summary": summary.summary,
            "source": summary.summary_source
        })),
        None => {
            let file_path = &ctx.root_path.join(file);
            if !file_path.exists() {
                return Ok(json!({
                    "file": file,
                    "summary": null,
                    "message": format!("File not found: {}", file)
                }));
            }

            let content = tokio::fs::read_to_string(&file_path).await?;
            let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

            if let Some(docstring) = crate::indexer::summarizer::extract_docstring(&content, ext) {
                Ok(json!({
                    "file": file,
                    "summary": docstring,
                    "source": "docstring"
                }))
            } else {
                Ok(json!({
                    "file": file,
                    "summary": null,
                    "message": "No summary available. The file has not been summarized yet."
                }))
            }
        }
    }
}

pub async fn tool_get_vue_tree(args: Value, ctx: &ToolContext) -> Result<Value> {
    let file = args.get("file").and_then(|v| v.as_str()).unwrap_or("");

    if file.is_empty() {
        return Err(GoferError::InvalidParams("File path is required".into()).into());
    }

    match &ctx
        .sqlite
        .get_vue_tree(&resolve_path(&ctx.root_path, file))
        .await?
    {
        Some(tree) => Ok(json!({
            "file": file,
            "tree": tree.tree_text
        })),
        None => Ok(json!({
            "file": file,
            "tree": null,
            "message": format!("No Vue tree found for '{}'. File may not be indexed.", file)
        })),
    }
}

pub async fn tool_add_rule(args: Value, ctx: &ToolContext) -> Result<Value> {
    let category = args
        .get("category")
        .and_then(|v| v.as_str())
        .unwrap_or("general");
    let rule = args.get("rule").and_then(|v| v.as_str()).unwrap_or("");
    let priority = args.get("priority").and_then(|v| v.as_i64()).unwrap_or(0) as i32;

    if rule.is_empty() {
        return Err(GoferError::InvalidParams("Rule text is required".into()).into());
    }

    let r = Rule {
        category: category.to_string(),
        rule: rule.to_string(),
        priority,
        source: Some("mcp_tool".to_string()),
        id: 0,
    };

    ctx.sqlite.upsert_rules(&[r], "mcp_tool").await?;

    Ok(json!({
        "status": "success",
        "message": "Rule added"
    }))
}

pub async fn tool_mark_golden_sample(args: Value, ctx: &ToolContext) -> Result<Value> {
    let file = args.get("file").and_then(|v| v.as_str()).unwrap_or("");
    let category = args.get("category").and_then(|v| v.as_str());
    let description = args.get("description").and_then(|v| v.as_str());

    if file.is_empty() {
        return Err(GoferError::InvalidParams("File path is required".into()).into());
    }

    let file_path = resolve_path(&ctx.root_path, file);

    // Find file_id
    if let Some(file) = ctx.sqlite.get_file(&file_path).await? {
        ctx.sqlite
            .mark_golden_sample(file.id, category, description)
            .await?;
        Ok(json!({
            "status": "success",
            "file": file.path,
            "marked": true
        }))
    } else {
        Err(GoferError::InvalidParams(format!("File not indexed: {}", file)).into())
    }
}
