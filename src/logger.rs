use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialize tracing with a rolling file appender and stdout.
/// Returns a WorkerGuard that must be kept alive for the duration of the program.
pub fn init(component: &'static str) -> tracing_appender::non_blocking::WorkerGuard {
    let home = dirs::home_dir()
        .expect("Cannot determine home directory")
        .join(".gofer");
    let logs_dir = home.join("logs");
    std::fs::create_dir_all(&logs_dir).expect("Failed to create logs directory");

    // Unified log file: gofer.log.<date>
    let file_appender = tracing_appender::rolling::daily(logs_dir, "gofer.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| "gofer=info".into());

    let text_logging = std::env::var("GOFER_LOG_TEXT")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false);

    if text_logging {
        let fmt_layer = tracing_subscriber::fmt::layer()
            .with_writer(non_blocking)
            .with_ansi(false);

        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();
    } else {
        let fmt_layer = tracing_subscriber::fmt::layer()
            .json()
            .with_writer(non_blocking);

        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();
    }

    // Identify process start
    tracing::info!(
        component = component,
        pid = std::process::id(),
        "Logging initialized for {} (PID {})",
        component,
        std::process::id()
    );

    guard
}
