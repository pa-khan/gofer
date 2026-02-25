-- Diagnostics and Configuration

-- Active compiler errors (from cargo check / tsc)
CREATE TABLE IF NOT EXISTS active_errors (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_path TEXT NOT NULL,
    line INTEGER NOT NULL,
    column INTEGER,
    severity TEXT NOT NULL,     -- 'error', 'warning'
    code TEXT,                  -- e.g., 'E0502', 'TS2322'
    message TEXT NOT NULL,
    suggestion TEXT,            -- compiler suggestion if any
    updated_at INTEGER NOT NULL,
    UNIQUE(file_path, line, message)
);

CREATE INDEX IF NOT EXISTS idx_errors_file ON active_errors(file_path);
CREATE INDEX IF NOT EXISTS idx_errors_severity ON active_errors(severity);

-- Config keys (from .env.example and config structs)
CREATE TABLE IF NOT EXISTS config_keys (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    key_name TEXT NOT NULL UNIQUE,
    data_type TEXT,             -- 'String', 'u32', 'bool', etc.
    source TEXT NOT NULL,       -- '.env.example', 'config.rs', etc.
    description TEXT,
    default_value TEXT,
    required INTEGER DEFAULT 1
);

-- Vue component tree (simplified DOM structure)
CREATE TABLE IF NOT EXISTS vue_trees (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id INTEGER NOT NULL,
    tree_text TEXT NOT NULL,    -- ASCII tree representation
    components TEXT,            -- JSON array of component names used
    updated_at INTEGER NOT NULL,
    FOREIGN KEY(file_id) REFERENCES files(id) ON DELETE CASCADE,
    UNIQUE(file_id)
);
