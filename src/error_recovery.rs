//! Error recovery strategies (Feature 016: error_recovery)
//!
//! Implements Circuit Breaker pattern for external services (Embedder, LLM).

use anyhow::anyhow;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open { opened_at: Instant },
    HalfOpen,
}

/// Circuit breaker implementation
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    failure_threshold: u32,
    recovery_threshold: u32,
    timeout: Duration,
    state: Arc<RwLock<CircuitState>>,
    failure_count: Arc<AtomicU32>,
    success_count: Arc<RwLock<u32>>,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, recovery_threshold: u32, timeout: Duration) -> Self {
        Self {
            failure_threshold,
            recovery_threshold,
            timeout,
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            failure_count: Arc::new(AtomicU32::new(0)),
            success_count: Arc::new(RwLock::new(0)),
        }
    }

    /// Execute an async operation through the circuit breaker
    pub async fn call<F, T, E, Fut>(&self, f: F) -> Result<T, E>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: From<anyhow::Error>, // Ensure E can be created from anyhow::Error
    {
        // Check state
        let state = *self.state.read().await;
        if let CircuitState::Open { opened_at } = state {
            // Check if timeout elapsed
            if opened_at.elapsed() >= self.timeout {
                // Transition to half-open
                *self.state.write().await = CircuitState::HalfOpen;
                *self.success_count.write().await = 0;
                tracing::info!("Circuit breaker transitioning to half-open");
            } else {
                return Err(E::from(anyhow!("Circuit breaker open")));
            }
        }

        // Attempt operation
        match f().await {
            Ok(val) => {
                self.record_success().await;
                Ok(val)
            }
            Err(err) => {
                self.record_failure().await;
                Err(err)
            }
        }
    }

    async fn record_success(&self) {
        let mut state = self.state.write().await;
        match *state {
            CircuitState::HalfOpen => {
                let mut success = self.success_count.write().await;
                *success += 1;
                if *success >= self.recovery_threshold {
                    *state = CircuitState::Closed;
                    self.failure_count.store(0, Ordering::Relaxed);
                    tracing::info!("Circuit breaker recovered (closed)");
                }
            }
            CircuitState::Closed => {
                self.failure_count.store(0, Ordering::Relaxed);
            }
            _ => {}
        }
    }

    async fn record_failure(&self) {
        let failures = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
        if failures >= self.failure_threshold {
            let mut state = self.state.write().await;
            if *state
                != (CircuitState::Open {
                    opened_at: Instant::now(),
                })
            { // dummy check to satisfy type checker, logic handles it
                 // Actually we want to check if it's NOT open, then open it.
                 // But we can just overwrite it.
            }
            // Transition to Open
            match *state {
                CircuitState::Closed | CircuitState::HalfOpen => {
                    *state = CircuitState::Open {
                        opened_at: Instant::now(),
                    };
                    tracing::warn!("Circuit breaker tripped (open)");
                }
                _ => {}
            }
        }
    }
}
