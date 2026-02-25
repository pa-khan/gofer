//! IPC client for connecting to the daemon over Unix socket.

use std::path::Path;

use anyhow::Result;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::UnixStream;

/// Client for communicating with the gofer daemon.
pub struct DaemonClient {
    reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: BufWriter<tokio::net::unix::OwnedWriteHalf>,
    next_id: u64,
}

impl DaemonClient {
    /// Connect to the daemon socket.
    pub async fn connect(socket_path: &Path) -> Result<Self> {
        let stream = UnixStream::connect(socket_path).await?;
        let (read_half, write_half) = stream.into_split();

        Ok(Self {
            reader: BufReader::new(read_half),
            writer: BufWriter::new(write_half),
            next_id: 1,
        })
    }

    /// Send a JSON-RPC request and wait for the response.
    pub async fn call(
        &mut self,
        method: &str,
        params: Value,
    ) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;

        let req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let data = serde_json::to_string(&req)?;
        self.writer.write_all(data.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;
        self.writer.flush().await?;

        let mut line = String::new();
        self.reader.read_line(&mut line).await?;

        let resp: Value = serde_json::from_str(line.trim())?;

        if let Some(err) = resp.get("error") {
            let msg = err
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            anyhow::bail!("{}", msg);
        }

        Ok(resp.get("result").cloned().unwrap_or(Value::Null))
    }

    /// Check if a daemon is alive at the given socket path.
    pub async fn is_alive(socket_path: &Path) -> bool {
        if !socket_path.exists() {
            return false;
        }
        match Self::connect(socket_path).await {
            Ok(mut client) => client
                .call("ping", json!({}))
                .await
                .is_ok(),
            Err(_) => false,
        }
    }
}
