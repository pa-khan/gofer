use regex::Regex;
use std::collections::HashSet;
use std::path::Path;

/// Domain classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Domain {
    Rust,
    Python,
    Frontend,
    Shared,
    Ops,
    Unknown,
}

impl Domain {
    pub fn as_str(&self) -> &'static str {
        match self {
            Domain::Rust => "backend",
            Domain::Python => "backend",
            Domain::Frontend => "frontend",
            Domain::Shared => "shared",
            Domain::Ops => "ops",
            Domain::Unknown => "unknown",
        }
    }
}

/// Domain detection configuration
#[derive(Debug, Clone, Default)]
pub struct DomainConfig {
    pub rs_paths: Vec<String>,
    pub py_paths: Vec<String>,
    pub frontend_paths: Vec<String>,
    pub ops_paths: Vec<String>,
    pub shared_paths: Vec<String>,
}

impl DomainConfig {
    pub fn default_config() -> Self {
        Self {
            rs_paths: vec![
                "backend/".into(),
                "server/".into(),
                "api/".into(),
                "src-rust/".into(),
                "src/".into(),
            ],
            py_paths: vec![
                "python/".into(),
                "py/".into(),
                "src-py/".into(),
                "scripts/".into(),
            ],
            frontend_paths: vec![
                "frontend/".into(),
                "ui/".into(),
                "client/".into(),
                "web/".into(),
                "src-ui/".into(),
                "app/".into(),
            ],
            ops_paths: vec![
                "docker/".into(),
                "k8s/".into(),
                ".github/".into(),
                "deploy/".into(),
            ],
            shared_paths: vec!["shared/".into(), "common/".into(), "types/".into()],
        }
    }
}

/// Detect domain by folder path (Level 1)
pub fn detect_domain_by_path(file_path: &str, config: &DomainConfig) -> Domain {
    for path in &config.shared_paths {
        if file_path.starts_with(path) {
            return Domain::Shared;
        }
    }
    for path in &config.ops_paths {
        if file_path.starts_with(path) {
            return Domain::Ops;
        }
    }
    for path in &config.rs_paths {
        if file_path.starts_with(path) {
            return Domain::Rust;
        }
    }
    for path in &config.py_paths {
        if file_path.starts_with(path) {
            return Domain::Python;
        }
    }
    for path in &config.frontend_paths {
        if file_path.starts_with(path) {
            return Domain::Frontend;
        }
    }
    Domain::Unknown
}

/// Detect domain by file extension (Level 2)
pub fn detect_domain_by_extension(file_path: &str) -> Domain {
    let ext = Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    match ext {
        "rs" => Domain::Rust,
        "py" => Domain::Python,
        "vue" => Domain::Frontend,
        "tsx" | "jsx" => Domain::Frontend,
        "sql" => Domain::Rust, // Usually backend
        "dockerfile" | "yaml" | "yml" => Domain::Ops,
        _ => Domain::Unknown,
    }
}

/// Detect domain by imports (Level 3) - most accurate
pub fn detect_domain_by_imports(content: &str, extension: &str) -> (Domain, Vec<String>) {
    let mut tech_stack = Vec::new();
    let mut rust_score = 0;
    let mut python_score = 0;
    let mut frontend_score = 0;

    match extension {
        "rs" => {
            // Rust is always rust domain
            rust_score += 10;

            if content.contains("use axum") {
                tech_stack.push("axum".into());
                rust_score += 5;
            }
            if content.contains("use actix") {
                tech_stack.push("actix".into());
                rust_score += 5;
            }
            if content.contains("use sqlx") {
                tech_stack.push("sqlx".into());
                rust_score += 3;
            }
            if content.contains("use diesel") {
                tech_stack.push("diesel".into());
                rust_score += 3;
            }
            if content.contains("use tokio") {
                tech_stack.push("tokio".into());
            }
            if content.contains("use serde") {
                tech_stack.push("serde".into());
            }
        }
        "py" => {
            // Python domain
            python_score += 10;

            if content.contains("import fastapi") || content.contains("from fastapi") {
                tech_stack.push("fastapi".into());
                python_score += 5;
            }
            if content.contains("import django") || content.contains("from django") {
                tech_stack.push("django".into());
                python_score += 5;
            }
            if content.contains("import flask") || content.contains("from flask") {
                tech_stack.push("flask".into());
                python_score += 5;
            }
            if content.contains("import sqlalchemy") || content.contains("from sqlalchemy") {
                tech_stack.push("sqlalchemy".into());
                python_score += 3;
            }
            if content.contains("import asyncio") || content.contains("from asyncio") {
                tech_stack.push("asyncio".into());
            }
            if content.contains("import pydantic") || content.contains("from pydantic") {
                tech_stack.push("pydantic".into());
            }
            if content.contains("import pytest") || content.contains("from pytest") {
                tech_stack.push("pytest".into());
            }
        }
        "ts" | "tsx" | "js" | "jsx" | "vue" => {
            // Check for frontend markers
            if content.contains("from 'vue'") || content.contains("from \"vue\"") {
                tech_stack.push("vue".into());
                frontend_score += 10;
            }
            if content.contains("from 'react'") || content.contains("from \"react\"") {
                tech_stack.push("react".into());
                frontend_score += 10;
            }
            if content.contains("from 'svelte'") {
                tech_stack.push("svelte".into());
                frontend_score += 10;
            }
            if content.contains("@tailwind") || content.contains("tailwindcss") {
                tech_stack.push("tailwindcss".into());
                frontend_score += 3;
            }

            // Check for backend markers (Node.js)
            if content.contains("from 'express'") || content.contains("require('express')") {
                tech_stack.push("express".into());
                rust_score += 10; // Node backend counts as backend
            }
            if content.contains("from '@nestjs") {
                tech_stack.push("nestjs".into());
                rust_score += 10;
            }
            if content.contains("from 'pg'") || content.contains("from 'mysql'") {
                rust_score += 5;
            }

            // Check for shared/utility
            if content.contains("from 'axios'") || content.contains("from 'ky'") {
                frontend_score += 2; // Usually frontend, but could be either
            }
        }
        _ => {}
    }

    let domain = if rust_score > frontend_score && rust_score > python_score && rust_score > 0 {
        Domain::Rust
    } else if python_score > frontend_score && python_score > rust_score && python_score > 0 {
        Domain::Python
    } else if frontend_score > rust_score && frontend_score > python_score && frontend_score > 0 {
        Domain::Frontend
    } else {
        Domain::Unknown
    };

    (domain, tech_stack)
}

/// Combined domain detection
pub fn detect_domain(
    file_path: &str,
    content: &str,
    config: &DomainConfig,
) -> (Domain, Vec<String>) {
    let ext = Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    // Level 1: By path
    let path_domain = detect_domain_by_path(file_path, config);
    if path_domain != Domain::Unknown {
        let (_, tech_stack) = detect_domain_by_imports(content, ext);
        return (path_domain, tech_stack);
    }

    // Level 2: By extension
    let ext_domain = detect_domain_by_extension(file_path);
    if ext_domain != Domain::Unknown && ext_domain != Domain::Ops {
        let (_, tech_stack) = detect_domain_by_imports(content, ext);
        return (ext_domain, tech_stack);
    }

    // Level 3: By imports
    detect_domain_by_imports(content, ext)
}

// === Field Extraction (regex-based, deprecated — use AST-based below) ===

/// Parsed API endpoint
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ParsedEndpoint {
    pub method: String,
    pub path: String,
    pub handler: Option<String>,
    pub request_type: Option<String>,
    pub response_type: Option<String>,
    pub line: u32,
}

/// Parse Axum routes from Rust code
pub fn parse_axum_routes(content: &str) -> Vec<ParsedEndpoint> {
    let mut endpoints = Vec::new();

    // Match .route("/path", method(handler))
    let route_re = Regex::new(
        r#"\.route\s*\(\s*"([^"]+)"\s*,\s*(get|post|put|delete|patch)\s*\(\s*(\w+)\s*\)"#,
    )
    .unwrap();

    for (line_num, line) in content.lines().enumerate() {
        for caps in route_re.captures_iter(line) {
            let path = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let method = caps
                .get(2)
                .map(|m| m.as_str().to_uppercase())
                .unwrap_or_default();
            let handler = caps.get(3).map(|m| m.as_str().to_string());

            endpoints.push(ParsedEndpoint {
                method,
                path: path.to_string(),
                handler,
                request_type: None,
                response_type: None,
                line: line_num as u32,
            });
        }
    }

    // Try to find handler signatures and extract types
    for endpoint in &mut endpoints {
        if let Some(handler) = &endpoint.handler {
            // Find handler function: async fn handler(Json<Type>)
            let handler_re = Regex::new(&format!(
                r"(?s)async\s+fn\s+{}\s*\([^)]*Json<(\w+)>",
                regex::escape(handler)
            ))
            .ok();

            if let Some(re) = handler_re {
                if let Some(caps) = re.captures(content) {
                    endpoint.request_type = caps.get(1).map(|m| m.as_str().to_string());
                }
            }

            // Find return type: -> Json<Type> or -> impl IntoResponse
            let return_re = Regex::new(&format!(
                r"(?s)async\s+fn\s+{}[^{{]*->\s*(?:impl\s+IntoResponse|Json<(\w+)>)",
                regex::escape(handler)
            ))
            .ok();

            if let Some(re) = return_re {
                if let Some(caps) = re.captures(content) {
                    endpoint.response_type = caps.get(1).map(|m| m.as_str().to_string());
                }
            }
        }
    }

    endpoints
}

/// Parse Express.js routes from JavaScript/TypeScript code.
/// Matches: app.get('/path', handler), router.post('/path', handler)
pub fn parse_express_routes(content: &str) -> Vec<ParsedEndpoint> {
    let mut endpoints = Vec::new();

    let route_re = Regex::new(
        r#"(?:app|router|server)\s*\.\s*(get|post|put|delete|patch|all)\s*\(\s*[`'"]([^`'"]+)[`'"]"#
    ).unwrap();

    for (line_num, line) in content.lines().enumerate() {
        for caps in route_re.captures_iter(line) {
            let method = caps
                .get(1)
                .map(|m| m.as_str().to_uppercase())
                .unwrap_or_default();
            let path = caps.get(2).map(|m| m.as_str()).unwrap_or("");

            endpoints.push(ParsedEndpoint {
                method,
                path: path.to_string(),
                handler: None,
                request_type: None,
                response_type: None,
                line: line_num as u32,
            });
        }
    }

    endpoints
}

/// Parse FastAPI routes from Python code.
/// Matches: @app.get("/path"), @router.post("/path")
pub fn parse_fastapi_routes(content: &str) -> Vec<ParsedEndpoint> {
    let mut endpoints = Vec::new();

    let route_re = Regex::new(
        r#"@\s*(?:app|router)\s*\.\s*(get|post|put|delete|patch)\s*\(\s*["']([^"']+)["']"#,
    )
    .unwrap();

    for (line_num, line) in content.lines().enumerate() {
        for caps in route_re.captures_iter(line) {
            let method = caps
                .get(1)
                .map(|m| m.as_str().to_uppercase())
                .unwrap_or_default();
            let path = caps.get(2).map(|m| m.as_str()).unwrap_or("");

            endpoints.push(ParsedEndpoint {
                method,
                path: path.to_string(),
                handler: None,
                request_type: None,
                response_type: None,
                line: line_num as u32,
            });
        }
    }

    endpoints
}

/// Parse Flask routes from Python code.
/// Matches: @app.route("/path", methods=["GET"]), @app.get("/path"), @bp.route(...)
pub fn parse_flask_routes(content: &str) -> Vec<ParsedEndpoint> {
    let mut endpoints = Vec::new();

    // @app.route("/path", methods=["GET", "POST"])
    let route_re = Regex::new(
        r#"@\s*(?:app|bp|blueprint)\s*\.\s*route\s*\(\s*["']([^"']+)["'](?:\s*,\s*methods\s*=\s*\[([^\]]+)\])?"#
    ).unwrap();

    // @app.get("/path"), @app.post("/path") (Flask 2.0+)
    let shorthand_re = Regex::new(
        r#"@\s*(?:app|bp|blueprint)\s*\.\s*(get|post|put|delete|patch)\s*\(\s*["']([^"']+)["']"#,
    )
    .unwrap();

    let method_re = Regex::new(r#"["'](\w+)["']"#).unwrap();
    for (line_num, line) in content.lines().enumerate() {
        for caps in route_re.captures_iter(line) {
            let path = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let methods_str = caps.get(2).map(|m| m.as_str()).unwrap_or("\"GET\"");

            // Parse method list: ["GET", "POST"] or ['GET']
            for method_cap in method_re.captures_iter(methods_str) {
                let method = method_cap
                    .get(1)
                    .map(|m| m.as_str().to_uppercase())
                    .unwrap_or_default();
                endpoints.push(ParsedEndpoint {
                    method,
                    path: path.to_string(),
                    handler: None,
                    request_type: None,
                    response_type: None,
                    line: line_num as u32,
                });
            }
        }

        for caps in shorthand_re.captures_iter(line) {
            let method = caps
                .get(1)
                .map(|m| m.as_str().to_uppercase())
                .unwrap_or_default();
            let path = caps.get(2).map(|m| m.as_str()).unwrap_or("");

            endpoints.push(ParsedEndpoint {
                method,
                path: path.to_string(),
                handler: None,
                request_type: None,
                response_type: None,
                line: line_num as u32,
            });
        }
    }

    endpoints
}

/// Parse NestJS routes from TypeScript code.
/// Matches: @Get('/path'), @Post('/path') with @Controller('/prefix')
pub fn parse_nestjs_routes(content: &str) -> Vec<ParsedEndpoint> {
    let mut endpoints = Vec::new();

    // Detect controller prefix: @Controller('/api/users')
    let controller_re = Regex::new(r#"@Controller\s*\(\s*["']([^"']*)["']\s*\)"#).unwrap();
    let prefix = controller_re
        .captures(content)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();

    // @Get('/path'), @Post(), @Delete(':id')
    let method_re =
        Regex::new(r#"@(Get|Post|Put|Delete|Patch)\s*\(\s*(?:["']([^"']*)["'])?\s*\)"#).unwrap();

    for (line_num, line) in content.lines().enumerate() {
        for caps in method_re.captures_iter(line) {
            let method = caps
                .get(1)
                .map(|m| m.as_str().to_uppercase())
                .unwrap_or_default();
            let sub_path = caps.get(2).map(|m| m.as_str()).unwrap_or("");

            let full_path = if sub_path.is_empty() {
                prefix.clone()
            } else if prefix.is_empty() {
                format!("/{}", sub_path.trim_start_matches('/'))
            } else {
                format!(
                    "{}/{}",
                    prefix.trim_end_matches('/'),
                    sub_path.trim_start_matches('/')
                )
            };

            endpoints.push(ParsedEndpoint {
                method,
                path: full_path,
                handler: None,
                request_type: None,
                response_type: None,
                line: line_num as u32,
            });
        }
    }

    endpoints
}

/// Detect framework and parse backend routes from any supported framework.
pub fn parse_backend_routes(content: &str, extension: &str) -> Vec<ParsedEndpoint> {
    match extension {
        "rs" => parse_axum_routes(content),
        "py" => {
            if content.contains("FastAPI") || content.contains("fastapi") {
                parse_fastapi_routes(content)
            } else if content.contains("Flask") || content.contains("flask") {
                parse_flask_routes(content)
            } else {
                // Try both, return whichever finds results
                let fast = parse_fastapi_routes(content);
                if !fast.is_empty() {
                    fast
                } else {
                    parse_flask_routes(content)
                }
            }
        }
        "ts" | "js" => {
            if content.contains("@Controller")
                || content.contains("@nestjs")
                || content.contains("@Get(")
                || content.contains("@Post(")
            {
                parse_nestjs_routes(content)
            } else if content.contains("express")
                || content.contains("Router()")
                || content.contains("app.get(")
                || content.contains("app.post(")
            {
                parse_express_routes(content)
            } else {
                let nest = parse_nestjs_routes(content);
                if !nest.is_empty() {
                    nest
                } else {
                    parse_express_routes(content)
                }
            }
        }
        _ => Vec::new(),
    }
}

/// Parsed frontend API call
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ParsedApiCall {
    pub method: Option<String>,
    pub path: String,
    pub path_pattern: String, // Normalized with :params
    pub type_used: Option<String>,
    pub line: u32,
}

/// Parse fetch/axios calls from TypeScript/JavaScript
pub fn parse_frontend_api_calls(content: &str) -> Vec<ParsedApiCall> {
    let mut calls = Vec::new();

    // Match axios.method('/path') or fetch('/path')
    let axios_re = Regex::new(
        r#"(?:axios|api|http)\s*\.\s*(get|post|put|delete|patch)\s*(?:<[^>]+>)?\s*\(\s*[`'"]([^`'"]+)[`'"]"#
    ).unwrap();

    let fetch_re = Regex::new(
        r#"fetch\s*\(\s*[`'"]([^`'"]+)[`'"](?:\s*,\s*\{[^}]*method\s*:\s*[`'"](\w+)[`'"]\s*)?"#,
    )
    .unwrap();

    for (line_num, line) in content.lines().enumerate() {
        // Check axios-style
        for caps in axios_re.captures_iter(line) {
            let method = caps.get(1).map(|m| m.as_str().to_uppercase());
            let path = caps.get(2).map(|m| m.as_str()).unwrap_or("");

            calls.push(ParsedApiCall {
                method,
                path: path.to_string(),
                path_pattern: normalize_api_path(path),
                type_used: None,
                line: line_num as u32,
            });
        }

        // Check fetch-style
        for caps in fetch_re.captures_iter(line) {
            let path = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let method = caps.get(2).map(|m| m.as_str().to_uppercase());

            calls.push(ParsedApiCall {
                method,
                path: path.to_string(),
                path_pattern: normalize_api_path(path),
                type_used: None,
                line: line_num as u32,
            });
        }
    }

    calls
}

/// Normalize API path: /api/users/${id} -> /api/users/:id
pub fn normalize_api_path(path: &str) -> String {
    let re = Regex::new(r"\$\{[^}]+\}").unwrap();
    re.replace_all(path, ":param").to_string()
}

/// Check if two API paths match (accounting for parameters)
pub fn paths_match(backend_path: &str, frontend_path: &str) -> bool {
    let backend_normalized = normalize_api_path(backend_path);
    let frontend_normalized = normalize_api_path(frontend_path);

    // Split into segments
    let backend_parts: Vec<&str> = backend_normalized.split('/').collect();
    let frontend_parts: Vec<&str> = frontend_normalized.split('/').collect();

    if backend_parts.len() != frontend_parts.len() {
        return false;
    }

    for (b, f) in backend_parts.iter().zip(frontend_parts.iter()) {
        // Skip parameter segments
        if b.starts_with(':') || f.starts_with(':') {
            continue;
        }
        if b != f {
            return false;
        }
    }

    true
}

// === AST-based Structural Fingerprinting ===

use super::parser::{parse_all_type_fields, SupportedLanguage};
use crate::models::TypeField;
use crate::storage::SqliteStorage;

/// Результат Jaccard-сравнения двух наборов полей
#[derive(Debug, Clone)]
pub struct FieldMatch {
    pub similarity: f64,
    pub matched_fields: Vec<String>,
}

/// Jaccard-сравнение двух наборов TypeField по normalized-именам
pub fn jaccard_type_fields(a: &[TypeField], b: &[TypeField]) -> FieldMatch {
    let set_a: HashSet<String> = a.iter().map(|f| f.normalized.clone()).collect();
    let set_b: HashSet<String> = b.iter().map(|f| f.normalized.clone()).collect();

    let intersection: HashSet<_> = set_a.intersection(&set_b).cloned().collect();
    let union: HashSet<_> = set_a.union(&set_b).cloned().collect();

    if union.is_empty() {
        return FieldMatch {
            similarity: 0.0,
            matched_fields: Vec::new(),
        };
    }

    let similarity = intersection.len() as f64 / union.len() as f64;
    let matched_fields: Vec<String> = intersection.into_iter().collect();

    FieldMatch {
        similarity,
        matched_fields,
    }
}

/// Полная фаза structural fingerprinting:
/// 1. Извлекает поля всех struct/interface из распарсенных файлов
/// 2. Сохраняет fingerprints в SQLite
/// 3. Сравнивает Rust structs <-> TS/JS interfaces по Jaccard
/// 4. Сохраняет найденные cross_stack_links
pub async fn run_structural_fingerprinting(
    parsed_files: &[(String, String, SupportedLanguage)], // (path, content, language)
    sqlite: &SqliteStorage,
) -> anyhow::Result<usize> {
    // Фаза 1: Извлекаем fingerprints
    let mut rust_types: Vec<(String, String, Vec<TypeField>)> = Vec::new(); // (file_path, type_name, fields)
    let mut ts_types: Vec<(String, String, Vec<TypeField>)> = Vec::new();

    for (path, content, language) in parsed_files {
        let all_types = match parse_all_type_fields(content, *language) {
            Ok(t) => t,
            Err(_) => continue,
        };

        for (type_name, fields) in all_types {
            if fields.len() < 3 {
                continue; // Пропускаем тривиальные типы
            }

            // Сохраняем fingerprint в SQLite
            if let Ok(Some(file)) = sqlite.get_file(path).await {
                if let Ok(symbols) = sqlite.get_symbol_by_name(&type_name).await {
                    if let Some(symbol) = symbols.iter().find(|s| s.file_id == file.id) {
                        let fields_json = serde_json::to_string(
                            &fields.iter().map(|f| &f.name).collect::<Vec<_>>(),
                        )
                        .unwrap_or_default();
                        let fields_normalized = serde_json::to_string(
                            &fields.iter().map(|f| &f.normalized).collect::<Vec<_>>(),
                        )
                        .unwrap_or_default();

                        let _ = sqlite
                            .upsert_type_fingerprint(
                                file.id,
                                symbol.id,
                                &type_name,
                                match language {
                                    SupportedLanguage::Rust => "rust",
                                    SupportedLanguage::TypeScript
                                    | SupportedLanguage::JavaScript
                                    | SupportedLanguage::Vue => "typescript",
                                    SupportedLanguage::Python => "python",
                                    SupportedLanguage::Go => "go",
                                },
                                &fields_json,
                                &fields_normalized,
                                fields.len() as i32,
                            )
                            .await;
                    }
                }
            }

            match language {
                SupportedLanguage::Rust => {
                    rust_types.push((path.clone(), type_name, fields));
                }
                SupportedLanguage::TypeScript
                | SupportedLanguage::JavaScript
                | SupportedLanguage::Vue => {
                    ts_types.push((path.clone(), type_name, fields));
                }
                SupportedLanguage::Python => {
                    // Python тоже может быть бэкендом — пока привязываем к rust_types
                    rust_types.push((path.clone(), type_name, fields));
                }
                SupportedLanguage::Go => {
                    // Go is backend — group with rust_types for cross-stack matching
                    rust_types.push((path.clone(), type_name, fields));
                }
            }
        }
    }

    // Фаза 2: Jaccard-сравнение rust <-> ts
    sqlite.clear_structural_links().await?;

    let mut links_created = 0;
    let threshold = 0.75;

    for (rust_path, rust_name, rust_fields) in &rust_types {
        for (ts_path, ts_name, ts_fields) in &ts_types {
            let m = jaccard_type_fields(rust_fields, ts_fields);

            if m.similarity >= threshold && !m.matched_fields.is_empty() {
                let metadata = serde_json::json!({
                    "matched_fields": m.matched_fields,
                    "jaccard": m.similarity,
                    "rust_field_count": rust_fields.len(),
                    "ts_field_count": ts_fields.len(),
                });

                let _ = sqlite
                    .upsert_cross_stack_link(
                        rust_path,
                        ts_path,
                        rust_name,
                        ts_name,
                        "structural",
                        m.similarity,
                        &metadata.to_string(),
                    )
                    .await;

                links_created += 1;
                tracing::debug!(
                    "Structural link: {} ({}) <-> {} ({}) | J={:.2}, fields: {:?}",
                    rust_name,
                    rust_path,
                    ts_name,
                    ts_path,
                    m.similarity,
                    m.matched_fields
                );
            }
        }
    }

    Ok(links_created)
}
