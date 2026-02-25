//! Language tools folding: lang_tools_list and lang_tools_call
//! 
//! These meta-tools provide access to all language-specific tools (Vue, Rust, TypeScript, etc.)
//! without polluting the MCP tools/list with 50+ tool schemas.
//! 
//! Benefits:
//! - Token efficiency: 100+ tools → 2 meta-tools in MCP context
//! - Semantic search: AI can search for tools by description
//! - Lazy loading: Tool schemas loaded only when needed
//! - Scalability: Adding new languages doesn't increase MCP context size

use anyhow::Result;
use serde_json::{json, Value};

use crate::daemon::handlers::common::ToolContext;

/// Tool: lang_tools_list
/// 
/// Returns a list of all available language-specific tools.
/// Supports filtering by language and semantic search.
/// 
/// Args:
/// - lang (optional): Filter by language name (e.g., "vue", "rust", "typescript")
/// - search (optional): Semantic search query to find relevant tools
/// - include_schema (optional): Include full inputSchema (default: false for token efficiency)
pub async fn tool_lang_tools_list(args: Value, ctx: &ToolContext) -> Result<Value> {
    let lang_filter = args.get("lang").and_then(|v| v.as_str());
    let search_query = args.get("search").and_then(|v| v.as_str());
    let include_schema = args.get("include_schema").and_then(|v| v.as_bool()).unwrap_or(false);
    
    let mut tools = Vec::new();
    
    // Collect all tools from language services
    for service in ctx.language_services.iter() {
        // Apply language filter
        if let Some(lang) = lang_filter {
            if service.name() != lang {
                continue;
            }
        }
        
        // Collect tools from this service
        for tool_def in service.tools() {
            let mut tool_info = json!({
                "name": tool_def.name,
                "description": tool_def.description,
                "lang": service.name(),
            });
            
            if include_schema {
                tool_info["inputSchema"] = tool_def.input_schema.clone();
            }
            
            tools.push(tool_info);
        }
    }
    
    // Apply semantic search if provided
    if let Some(query) = search_query {
        tools = semantic_rank_tools(query, tools, ctx).await?;
    }
    
    Ok(json!({
        "tools": tools,
        "total": tools.len(),
        "filtered_by": {
            "lang": lang_filter,
            "search": search_query,
        }
    }))
}

/// Tool: lang_tools_call
/// 
/// Execute a specific language tool by name.
/// 
/// Args:
/// - tool (required): Tool name (e.g., "vue_get_meta", "rust_goto_definition")
/// - args (required): Arguments to pass to the tool (JSON object)
pub async fn tool_lang_tools_call(args: Value, ctx: &ToolContext) -> Result<Value> {
    let tool_name = args.get("tool")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: 'tool'"))?;
    
    let tool_args = args.get("args")
        .cloned()
        .unwrap_or_else(|| json!({}));
    
    // Find the service that provides this tool
    for service in ctx.language_services.iter() {
        for tool_def in service.tools() {
            if tool_def.name == tool_name {
                // Found the tool - execute it
                let result = service.call_tool(tool_name, tool_args, &ctx.root_path).await?;
                
                return Ok(json!({
                    "tool": tool_name,
                    "lang": service.name(),
                    "result": result
                }));
            }
        }
    }
    
    // Tool not found
    Err(anyhow::anyhow!(
        "Tool '{}' not found. Use lang_tools_list to see available tools.",
        tool_name
    ))
}

/// Semantic ranking of tools based on search query.
/// 
/// Uses embedding similarity to rank tools by relevance to the search query.
async fn semantic_rank_tools(
    query: &str,
    tools: Vec<Value>,
    ctx: &ToolContext,
) -> Result<Vec<Value>> {
    // Generate embedding for the search query
    let query_embedding = ctx.embedder
        .embed_query(query)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to embed search query: {}", e))?;
    
    // Calculate similarity scores for each tool
    let mut scored_tools: Vec<(Value, f32)> = Vec::new();
    
    for tool in tools {
        // Create a searchable text from tool name + description
        let searchable = format!(
            "{} {}",
            tool["name"].as_str().unwrap_or(""),
            tool["description"].as_str().unwrap_or("")
        );
        
        // Generate embedding for tool description
        let tool_embedding = ctx.embedder
            .embed_query(&searchable)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to embed tool description: {}", e))?;
        
        // Calculate cosine similarity
        let score = cosine_similarity(&query_embedding, &tool_embedding);
        
        scored_tools.push((tool, score));
    }
    
    // Sort by score (highest first)
    scored_tools.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    
    // Add relevance scores to results
    let ranked_tools: Vec<Value> = scored_tools
        .into_iter()
        .map(|(mut tool, score)| {
            tool["relevance_score"] = json!(format!("{:.3}", score));
            tool
        })
        .collect();
    
    Ok(ranked_tools)
}

/// Calculate cosine similarity between two embedding vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    
    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    
    dot_product / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);
        
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 0.0).abs() < 0.001);
        
        let a = vec![1.0, 1.0, 0.0];
        let b = vec![1.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);
    }
}
