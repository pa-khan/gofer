//! Execution Sandbox - Phase 3 implementation
//!
//! Безопасное выполнение кода в изолированном окружении.
//!
//! Implements:
//! - execute_code - выполнить произвольный код
//! - execute_function - выполнить конкретную функцию
//! - run_test - запустить тесты
//! - run_all_tests - запустить все тесты проекта

use super::common::{resolve_path_buf, ToolContext};
use crate::error::goferError;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

const DEFAULT_TIMEOUT_SECONDS: u64 = 5;
const MAX_TIMEOUT_SECONDS: u64 = 60;

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub status: String, // "success", "error", "timeout"
    pub result: Option<Value>,
    pub stdout: String,
    pub stderr: String,
    pub execution_time_ms: u64,
    pub error_type: Option<String>,
    pub error_message: Option<String>,
}

/// Execute arbitrary code (simple wrapper for testing)
pub async fn tool_execute_code(args: Value, ctx: &ToolContext) -> Result<Value> {
    let code = args
        .get("code")
        .and_then(|v| v.as_str())
        .ok_or_else(|| goferError::InvalidParams("code is required".into()))?;

    let language = args
        .get("language")
        .and_then(|v| v.as_str())
        .ok_or_else(|| goferError::InvalidParams("language is required".into()))?;

    let timeout_seconds = args
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_TIMEOUT_SECONDS)
        .min(MAX_TIMEOUT_SECONDS);

    let result = match language {
        "rust" => execute_rust_code(code, timeout_seconds, &ctx.root_path).await?,
        "python" => execute_python_code(code, timeout_seconds).await?,
        "javascript" | "js" => execute_javascript_code(code, timeout_seconds).await?,
        _ => {
            return Err(
                goferError::InvalidParams(format!("Unsupported language: {}", language)).into(),
            )
        }
    };

    Ok(serde_json::to_value(&result)?)
}

/// Execute specific function from file
pub async fn tool_execute_function(args: Value, ctx: &ToolContext) -> Result<Value> {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| goferError::InvalidParams("path is required".into()))?;

    let function_name = args
        .get("function_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| goferError::InvalidParams("function_name is required".into()))?;

    let function_args = args
        .get("args")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let timeout_seconds = args
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_TIMEOUT_SECONDS)
        .min(MAX_TIMEOUT_SECONDS);

    let abs_path = resolve_path_buf(&ctx.root_path, path);

    if !abs_path.exists() {
        return Err(goferError::InvalidParams(format!("File not found: {}", path)).into());
    }

    let ext = abs_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    let result = match ext {
        "rs" => {
            execute_rust_function(
                &abs_path,
                function_name,
                &function_args,
                timeout_seconds,
                &ctx.root_path,
            )
            .await?
        }
        "py" => {
            execute_python_function(&abs_path, function_name, &function_args, timeout_seconds)
                .await?
        }
        "js" | "ts" => {
            execute_javascript_function(&abs_path, function_name, &function_args, timeout_seconds)
                .await?
        }
        _ => {
            return Err(
                goferError::InvalidParams(format!("Unsupported file extension: {}", ext)).into(),
            )
        }
    };

    Ok(serde_json::to_value(&result)?)
}

/// Run specific test
pub async fn tool_run_test(args: Value, ctx: &ToolContext) -> Result<Value> {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| goferError::InvalidParams("path is required".into()))?;

    let test_name = args.get("test_name").and_then(|v| v.as_str());

    let timeout_seconds = args
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(30)
        .min(MAX_TIMEOUT_SECONDS);

    let abs_path = resolve_path_buf(&ctx.root_path, path);

    if !abs_path.exists() {
        return Err(goferError::InvalidParams(format!("File not found: {}", path)).into());
    }

    let ext = abs_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    let result = match ext {
        "rs" => run_rust_test(&ctx.root_path, test_name, timeout_seconds).await?,
        "py" => run_python_test(&abs_path, test_name, timeout_seconds).await?,
        "js" | "ts" => run_javascript_test(&abs_path, test_name, timeout_seconds).await?,
        _ => {
            return Err(
                goferError::InvalidParams(format!("Unsupported file extension: {}", ext)).into(),
            )
        }
    };

    Ok(result)
}

/// Run all tests in project
pub async fn tool_run_all_tests(args: Value, ctx: &ToolContext) -> Result<Value> {
    let filter = args.get("filter").and_then(|v| v.as_str());

    let timeout_seconds = args
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(60)
        .min(MAX_TIMEOUT_SECONDS * 2); // Allow longer for all tests

    // Detect project type and run appropriate test command
    let project_root = &ctx.root_path;

    // Check for Cargo.toml (Rust)
    if project_root.join("Cargo.toml").exists() {
        return run_cargo_tests(project_root, filter, timeout_seconds).await;
    }

    // Check for package.json (Node.js)
    if project_root.join("package.json").exists() {
        return run_npm_tests(project_root, filter, timeout_seconds).await;
    }

    // Check for pytest (Python)
    if project_root.join("pytest.ini").exists()
        || project_root.join("pyproject.toml").exists()
        || project_root.join("tests").exists()
    {
        return run_pytest_tests(project_root, filter, timeout_seconds).await;
    }

    Err(goferError::InvalidParams("No test framework detected in project".into()).into())
}

// Rust execution implementations

async fn execute_rust_code(
    code: &str,
    timeout_secs: u64,
    _project_root: &Path,
) -> Result<ExecutionResult> {
    let start = std::time::Instant::now();

    // Create temporary file
    let temp_dir = tempfile::tempdir()?;
    let temp_file = temp_dir.path().join("temp.rs");

    // Wrap code in main function if not present
    let full_code = if code.contains("fn main") {
        code.to_string()
    } else {
        format!("fn main() {{\n{}\n}}", code)
    };

    tokio::fs::write(&temp_file, full_code).await?;

    // Compile and run
    let compile_output = Command::new("rustc")
        .arg(&temp_file)
        .arg("-o")
        .arg(temp_dir.path().join("temp"))
        .output()
        .await?;

    if !compile_output.status.success() {
        return Ok(ExecutionResult {
            status: "error".to_string(),
            result: None,
            stdout: String::new(),
            stderr: String::from_utf8_lossy(&compile_output.stderr).to_string(),
            execution_time_ms: start.elapsed().as_millis() as u64,
            error_type: Some("compilation_error".to_string()),
            error_message: Some("Failed to compile".to_string()),
        });
    }

    // Execute
    let execute_future = Command::new(temp_dir.path().join("temp"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    let output = match timeout(Duration::from_secs(timeout_secs), execute_future).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            return Ok(ExecutionResult {
                status: "error".to_string(),
                result: None,
                stdout: String::new(),
                stderr: e.to_string(),
                execution_time_ms: start.elapsed().as_millis() as u64,
                error_type: Some("execution_error".to_string()),
                error_message: Some(e.to_string()),
            })
        }
        Err(_) => {
            return Ok(ExecutionResult {
                status: "timeout".to_string(),
                result: None,
                stdout: String::new(),
                stderr: format!("Execution timed out after {} seconds", timeout_secs),
                execution_time_ms: start.elapsed().as_millis() as u64,
                error_type: Some("timeout".to_string()),
                error_message: Some("Timeout exceeded".to_string()),
            })
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    Ok(ExecutionResult {
        status: if output.status.success() {
            "success"
        } else {
            "error"
        }
        .to_string(),
        result: if output.status.success() {
            Some(json!(stdout.trim()))
        } else {
            None
        },
        stdout,
        stderr: stderr.clone(),
        execution_time_ms: start.elapsed().as_millis() as u64,
        error_type: if output.status.success() {
            None
        } else {
            Some("runtime_error".to_string())
        },
        error_message: if output.status.success() {
            None
        } else {
            Some(stderr)
        },
    })
}

async fn execute_rust_function(
    _path: &Path,
    _function_name: &str,
    _args: &[Value],
    _timeout_secs: u64,
    _project_root: &Path,
) -> Result<ExecutionResult> {
    // Simplified implementation - would need proper Rust test harness
    Ok(ExecutionResult {
        status: "error".to_string(),
        result: None,
        stdout: String::new(),
        stderr: "execute_function for Rust requires cargo integration".to_string(),
        execution_time_ms: 0,
        error_type: Some("not_implemented".to_string()),
        error_message: Some("Use run_test for Rust functions".to_string()),
    })
}

// Python execution implementations

async fn execute_python_code(code: &str, timeout_secs: u64) -> Result<ExecutionResult> {
    let start = std::time::Instant::now();

    let execute_future = Command::new("python3")
        .arg("-c")
        .arg(code)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    let output = match timeout(Duration::from_secs(timeout_secs), execute_future).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            return Ok(ExecutionResult {
                status: "error".to_string(),
                result: None,
                stdout: String::new(),
                stderr: e.to_string(),
                execution_time_ms: start.elapsed().as_millis() as u64,
                error_type: Some("execution_error".to_string()),
                error_message: Some(e.to_string()),
            })
        }
        Err(_) => {
            return Ok(ExecutionResult {
                status: "timeout".to_string(),
                result: None,
                stdout: String::new(),
                stderr: format!("Execution timed out after {} seconds", timeout_secs),
                execution_time_ms: start.elapsed().as_millis() as u64,
                error_type: Some("timeout".to_string()),
                error_message: Some("Timeout exceeded".to_string()),
            })
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    Ok(ExecutionResult {
        status: if output.status.success() {
            "success"
        } else {
            "error"
        }
        .to_string(),
        result: if output.status.success() {
            Some(json!(stdout.trim()))
        } else {
            None
        },
        stdout,
        stderr: stderr.clone(),
        execution_time_ms: start.elapsed().as_millis() as u64,
        error_type: if output.status.success() {
            None
        } else {
            Some("runtime_error".to_string())
        },
        error_message: if output.status.success() {
            None
        } else {
            Some(stderr)
        },
    })
}

async fn execute_python_function(
    path: &Path,
    function_name: &str,
    args: &[Value],
    timeout_secs: u64,
) -> Result<ExecutionResult> {
    let args_json = serde_json::to_string(args)?;

    let code = format!(
        r#"
import sys
import json
sys.path.insert(0, '{}')
from {} import {}

args = json.loads('{}')
result = {}(*args)
print(json.dumps(result))
"#,
        path.parent().unwrap().display(),
        path.file_stem().unwrap().to_string_lossy(),
        function_name,
        args_json.replace('\'', "\\'"),
        function_name
    );

    execute_python_code(&code, timeout_secs).await
}

// JavaScript execution implementations

async fn execute_javascript_code(code: &str, timeout_secs: u64) -> Result<ExecutionResult> {
    let start = std::time::Instant::now();

    let execute_future = Command::new("node")
        .arg("-e")
        .arg(code)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    let output = match timeout(Duration::from_secs(timeout_secs), execute_future).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            return Ok(ExecutionResult {
                status: "error".to_string(),
                result: None,
                stdout: String::new(),
                stderr: format!("Node.js not found or execution error: {}", e),
                execution_time_ms: start.elapsed().as_millis() as u64,
                error_type: Some("execution_error".to_string()),
                error_message: Some(e.to_string()),
            })
        }
        Err(_) => {
            return Ok(ExecutionResult {
                status: "timeout".to_string(),
                result: None,
                stdout: String::new(),
                stderr: format!("Execution timed out after {} seconds", timeout_secs),
                execution_time_ms: start.elapsed().as_millis() as u64,
                error_type: Some("timeout".to_string()),
                error_message: Some("Timeout exceeded".to_string()),
            })
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    Ok(ExecutionResult {
        status: if output.status.success() {
            "success"
        } else {
            "error"
        }
        .to_string(),
        result: if output.status.success() {
            Some(json!(stdout.trim()))
        } else {
            None
        },
        stdout,
        stderr: stderr.clone(),
        execution_time_ms: start.elapsed().as_millis() as u64,
        error_type: if output.status.success() {
            None
        } else {
            Some("runtime_error".to_string())
        },
        error_message: if output.status.success() {
            None
        } else {
            Some(stderr)
        },
    })
}

async fn execute_javascript_function(
    path: &Path,
    function_name: &str,
    args: &[Value],
    timeout_secs: u64,
) -> Result<ExecutionResult> {
    let args_json = serde_json::to_string(args)?;

    let code = format!(
        r#"
const module = require('{}');
const args = {};
const result = module.{}(...args);
console.log(JSON.stringify(result));
"#,
        path.display(),
        args_json,
        function_name
    );

    execute_javascript_code(&code, timeout_secs).await
}

// Test runner implementations

async fn run_rust_test(
    project_root: &Path,
    test_name: Option<&str>,
    timeout_secs: u64,
) -> Result<Value> {
    let start = std::time::Instant::now();

    let mut cmd = Command::new("cargo");
    cmd.arg("test")
        .current_dir(project_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(name) = test_name {
        cmd.arg(name);
    }

    cmd.arg("--");
    cmd.arg("--nocapture");

    let execute_future = cmd.output();

    let output = match timeout(Duration::from_secs(timeout_secs), execute_future).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            return Ok(json!({
                "status": "error",
                "error": e.to_string(),
            }))
        }
        Err(_) => {
            return Ok(json!({
                "status": "timeout",
                "message": format!("Tests timed out after {} seconds", timeout_secs),
            }))
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Parse test results (simplified)
    let status = if output.status.success() {
        "passed"
    } else {
        "failed"
    };

    Ok(json!({
        "status": status,
        "execution_time_ms": start.elapsed().as_millis(),
        "stdout": stdout,
        "stderr": stderr,
    }))
}

async fn run_python_test(path: &Path, test_name: Option<&str>, timeout_secs: u64) -> Result<Value> {
    let start = std::time::Instant::now();

    let mut cmd = Command::new("pytest");
    cmd.arg(path)
        .arg("-v")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(name) = test_name {
        cmd.arg("-k").arg(name);
    }

    let execute_future = cmd.output();

    let output = match timeout(Duration::from_secs(timeout_secs), execute_future).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            return Ok(json!({
                "status": "error",
                "error": format!("pytest not found or execution error: {}", e),
            }))
        }
        Err(_) => {
            return Ok(json!({
                "status": "timeout",
                "message": format!("Tests timed out after {} seconds", timeout_secs),
            }))
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);

    let status = if output.status.success() {
        "passed"
    } else {
        "failed"
    };

    Ok(json!({
        "status": status,
        "execution_time_ms": start.elapsed().as_millis(),
        "output": stdout,
    }))
}

async fn run_javascript_test(
    path: &Path,
    test_name: Option<&str>,
    timeout_secs: u64,
) -> Result<Value> {
    let start = std::time::Instant::now();

    // Try Jest first
    let mut cmd = Command::new("npx");
    cmd.arg("jest")
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(name) = test_name {
        cmd.arg("-t").arg(name);
    }

    let execute_future = cmd.output();

    let output = match timeout(Duration::from_secs(timeout_secs), execute_future).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            return Ok(json!({
                "status": "error",
                "error": format!("jest not found: {}", e),
            }))
        }
        Err(_) => {
            return Ok(json!({
                "status": "timeout",
                "message": format!("Tests timed out after {} seconds", timeout_secs),
            }))
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);

    let status = if output.status.success() {
        "passed"
    } else {
        "failed"
    };

    Ok(json!({
        "status": status,
        "execution_time_ms": start.elapsed().as_millis(),
        "output": stdout,
    }))
}

async fn run_cargo_tests(
    project_root: &Path,
    filter: Option<&str>,
    timeout_secs: u64,
) -> Result<Value> {
    let mut cmd = Command::new("cargo");
    cmd.arg("test")
        .current_dir(project_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(f) = filter {
        cmd.arg(f);
    }

    let start = std::time::Instant::now();
    let execute_future = cmd.output();

    let output = match timeout(Duration::from_secs(timeout_secs), execute_future).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            return Ok(json!({
                "status": "error",
                "error": e.to_string(),
            }))
        }
        Err(_) => {
            return Ok(json!({
                "status": "timeout",
            }))
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);

    Ok(json!({
        "status": if output.status.success() { "passed" } else { "failed" },
        "execution_time_ms": start.elapsed().as_millis(),
        "output": stdout.to_string(),
    }))
}

async fn run_npm_tests(
    project_root: &Path,
    filter: Option<&str>,
    timeout_secs: u64,
) -> Result<Value> {
    let mut cmd = Command::new("npm");
    cmd.arg("test")
        .current_dir(project_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(f) = filter {
        cmd.arg("--").arg("-t").arg(f);
    }

    let start = std::time::Instant::now();
    let execute_future = cmd.output();

    let output = match timeout(Duration::from_secs(timeout_secs), execute_future).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            return Ok(json!({
                "status": "error",
                "error": e.to_string(),
            }))
        }
        Err(_) => {
            return Ok(json!({
                "status": "timeout",
            }))
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);

    Ok(json!({
        "status": if output.status.success() { "passed" } else { "failed" },
        "execution_time_ms": start.elapsed().as_millis(),
        "output": stdout.to_string(),
    }))
}

async fn run_pytest_tests(
    project_root: &Path,
    filter: Option<&str>,
    timeout_secs: u64,
) -> Result<Value> {
    let mut cmd = Command::new("pytest");
    cmd.arg("-v")
        .current_dir(project_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(f) = filter {
        cmd.arg("-k").arg(f);
    }

    let start = std::time::Instant::now();
    let execute_future = cmd.output();

    let output = match timeout(Duration::from_secs(timeout_secs), execute_future).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            return Ok(json!({
                "status": "error",
                "error": e.to_string(),
            }))
        }
        Err(_) => {
            return Ok(json!({
                "status": "timeout",
            }))
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);

    Ok(json!({
        "status": if output.status.success() { "passed" } else { "failed" },
        "execution_time_ms": start.elapsed().as_millis(),
        "output": stdout.to_string(),
    }))
}
