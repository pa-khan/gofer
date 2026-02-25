//! Minimal HTTP server for Prometheus `/metrics` scraping.
//! Uses raw `TcpListener` — no external HTTP server dependency.

use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use super::state::DaemonMetrics;

/// Start a tiny HTTP server on `addr` (e.g. "127.0.0.1:9091") that serves
/// Prometheus text metrics at any path. Stops when the cancel token fires.
pub async fn serve_metrics(addr: &str, metrics: Arc<DaemonMetrics>, cancel: CancellationToken) {
    let listener = match TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!("Metrics HTTP: failed to bind {}: {}", addr, e);
            return;
        }
    };

    tracing::info!("Metrics HTTP: listening on http://{}/metrics", addr);

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                tracing::info!("Metrics HTTP: shutting down");
                break;
            }
            accept = listener.accept() => {
                let (mut stream, _) = match accept {
                    Ok(s) => s,
                    Err(_) => continue,
                };

                let body = metrics.to_prometheus();
                let response = format!(
                    "HTTP/1.1 200 OK\r\n\
                     Content-Type: text/plain; version=0.0.4; charset=utf-8\r\n\
                     Content-Length: {}\r\n\
                     Connection: close\r\n\
                     \r\n\
                     {}",
                    body.len(),
                    body
                );

                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            }
        }
    }
}
