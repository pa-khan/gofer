-- Audit log for MCP tool calls with latency tracking

CREATE TABLE IF NOT EXISTS audit_log (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    tool_name TEXT    NOT NULL,
    args_json TEXT,
    latency_ms INTEGER NOT NULL,
    success   INTEGER NOT NULL DEFAULT 1,
    error_msg TEXT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX IF NOT EXISTS idx_audit_log_tool ON audit_log(tool_name);
CREATE INDEX IF NOT EXISTS idx_audit_log_created ON audit_log(created_at);
