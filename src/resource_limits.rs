//! Resource limits and management (Feature 015: connection_pooling)

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Resource limits for controlling system resource usage
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    /// Maximum concurrent requests (default: 100)
    pub max_concurrent_requests: usize,
    
    /// Current active requests counter
    active_requests: Arc<AtomicUsize>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_concurrent_requests: 100,
            active_requests: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl ResourceLimits {
    /// Try to acquire a request slot
    /// Returns Ok(RequestGuard) if successful, Err if limit exceeded
    pub fn try_acquire_request(&self) -> Result<RequestGuard, ResourceLimitError> {
        let current = self.active_requests.fetch_add(1, Ordering::SeqCst);
        
        if current >= self.max_concurrent_requests {
            self.active_requests.fetch_sub(1, Ordering::SeqCst);
            return Err(ResourceLimitError::TooManyRequests {
                current,
                max: self.max_concurrent_requests,
            });
        }
        
        Ok(RequestGuard {
            counter: self.active_requests.clone(),
        })
    }
}

/// RAII guard that decrements active request counter on drop
pub struct RequestGuard {
    counter: Arc<AtomicUsize>,
}

impl Drop for RequestGuard {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Errors related to resource limits
#[derive(Debug, thiserror::Error)]
pub enum ResourceLimitError {
    #[error("Too many concurrent requests: {current}/{max}")]
    TooManyRequests { current: usize, max: usize },
}
