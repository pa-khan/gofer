//! MCP stdio-to-daemon bridge.
//!
//! Proxies JSON-RPC between the MCP client (stdin/stdout) and the gofer daemon
//! (Unix socket). Determines the actual project path via MCP `roots/list` and
//! injects it into every request forwarded to the daemon.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{json, Value};
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// Sentinel IDs for internal bridge requests — filtered from client output.
const ROOTS_LIST_ID: &str = "__gofer_roots__";
const REGISTER_ID: &str = "__gofer_register__";
const ACTIVATE_ID: &str = "__gofer_activate__";

/// Run the MCP bridge: stdin <-> daemon socket.
///
/// `fallback_path` is used as project_path until MCP `roots/list` provides the
/// real workspace root.
pub async fn run_bridge(fallback_path: PathBuf, socket_path: &Path) -> Result<()> {
    let stream = UnixStream::connect(socket_path).await?;
    let (sock_reader, mut sock_writer) = stream.into_split();
    let mut sock_reader = BufReader::new(sock_reader);

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut stdin_lines = BufReader::new(stdin).lines();

    let mut project_path = fallback_path.to_string_lossy().to_string();
    let mut roots_requested = false;

    let mut sock_line = String::new();

    loop {
        sock_line.clear();

        tokio::select! {
            // ── stdin (MCP client) ──────────────────────────────
            result = stdin_lines.next_line() => {
                match result {
                    Ok(Some(line)) => {
                        if line.trim().is_empty() {
                            continue;
                        }

                        match serde_json::from_str::<Value>(&line) {
                            Ok(mut msg) => {
                                let method = msg.get("method")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                let msg_id = msg.get("id").cloned();

                                // ── Response to our roots/list request ──
                                if is_bridge_id(&msg_id, ROOTS_LIST_ID) {
                                    if let Some(root) = extract_root_path(&msg) {
                                        project_path = root;
                                        // Auto-register & activate project in daemon
                                        let _ = send_json(&mut sock_writer, &json!({
                                            "jsonrpc": "2.0",
                                            "id": REGISTER_ID,
                                            "method": "daemon/register_project",
                                            "params": { "project_path": &project_path }
                                        })).await;
                                        let _ = send_json(&mut sock_writer, &json!({
                                            "jsonrpc": "2.0",
                                            "id": ACTIVATE_ID,
                                            "method": "daemon/activate_project",
                                            "params": {
                                                "project_path": &project_path,
                                                "watch": true
                                            }
                                        })).await;
                                    }
                                    continue; // don't forward to daemon
                                }

                                // ── After "initialized", ask client for roots ──
                                if method == "initialized" && !roots_requested {
                                    roots_requested = true;
                                    let _ = send_json(&mut stdout, &json!({
                                        "jsonrpc": "2.0",
                                        "id": ROOTS_LIST_ID,
                                        "method": "roots/list"
                                    })).await;
                                    // Don't forward notification to daemon
                                    // (daemon responds to notifications with id:null
                                    //  which confuses clients)
                                    continue;
                                }

                                // ── Default: inject project_path, forward ──
                                inject_project_path(&mut msg, &project_path);
                                send_json(&mut sock_writer, &msg).await?;
                            }
                            Err(_) => {
                                // Not valid JSON — forward as-is
                                sock_writer.write_all(line.as_bytes()).await?;
                                sock_writer.write_all(b"\n").await?;
                                sock_writer.flush().await?;
                            }
                        }
                    }
                    Ok(None) => break, // EOF
                    Err(_) => break,
                }
            }

            // ── daemon socket → stdout (MCP client) ────────────
            n = sock_reader.read_line(&mut sock_line) => {
                match n {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        // Filter out responses to bridge-internal requests
                        if let Ok(resp) = serde_json::from_str::<Value>(&sock_line) {
                            if is_bridge_id(&resp.get("id").cloned(), REGISTER_ID)
                                || is_bridge_id(&resp.get("id").cloned(), ACTIVATE_ID)
                            {
                                continue; // swallow
                            }
                        }
                        stdout.write_all(sock_line.as_bytes()).await?;
                        stdout.flush().await?;
                    }
                    Err(_) => break,
                }
            }
        }
    }

    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Check whether a JSON-RPC `id` field matches a bridge sentinel string.
fn is_bridge_id(id: &Option<Value>, sentinel: &str) -> bool {
    id.as_ref()
        .and_then(|v| v.as_str())
        .map(|s| s == sentinel)
        .unwrap_or(false)
}

/// Extract the first root path from a `roots/list` response.
///
/// Expects: `{"result":{"roots":[{"uri":"file:///path/to/project"}]}}`
fn extract_root_path(response: &Value) -> Option<String> {
    response
        .get("result")
        .and_then(|r| r.get("roots"))
        .and_then(|r| r.as_array())
        .and_then(|roots| roots.first())
        .and_then(|root| root.get("uri"))
        .and_then(|uri| uri.as_str())
        .map(|uri| uri.strip_prefix("file://").unwrap_or(uri).to_string())
}

/// Inject `project_path` into the `params` object of a JSON-RPC request.
fn inject_project_path(req: &mut Value, project_path: &str) {
    if let Some(params) = req.get_mut("params") {
        if let Some(obj) = params.as_object_mut() {
            obj.insert(
                "project_path".to_string(),
                Value::String(project_path.to_string()),
            );
        }
    } else {
        req["params"] = json!({ "project_path": project_path });
    }
}

/// Serialize a JSON value and write it as a single line + flush.
async fn send_json<W: tokio::io::AsyncWrite + Unpin>(w: &mut W, msg: &Value) -> Result<()> {
    let s = serde_json::to_string(msg)?;
    w.write_all(s.as_bytes()).await?;
    w.write_all(b"\n").await?;
    w.flush().await?;
    Ok(())
}
