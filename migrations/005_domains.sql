-- Domain Detection and Cross-Stack Linking
-- Note: domain and tech_stack columns on files are created in 001_init.sql

-- Struct/Interface fields for fingerprinting
CREATE TABLE IF NOT EXISTS type_fields (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    symbol_id INTEGER NOT NULL,
    field_name TEXT NOT NULL,
    field_type TEXT,
    json_name TEXT,           -- from #[serde(rename = "...")] or as-is
    is_optional INTEGER DEFAULT 0,
    FOREIGN KEY(symbol_id) REFERENCES symbols(id) ON DELETE CASCADE,
    UNIQUE(symbol_id, field_name)
);

CREATE INDEX IF NOT EXISTS idx_type_fields_symbol ON type_fields(symbol_id);

-- Cross-stack entity links (Backend struct <-> Frontend interface)
CREATE TABLE IF NOT EXISTS entity_links (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    backend_symbol_id INTEGER NOT NULL,
    frontend_symbol_id INTEGER NOT NULL,
    confidence REAL NOT NULL,  -- 0.0 to 1.0 (Jaccard index)
    link_type TEXT NOT NULL,   -- 'serialization_match', 'route_match', 'name_match'
    matched_fields TEXT,       -- JSON array of matching field names
    FOREIGN KEY(backend_symbol_id) REFERENCES symbols(id) ON DELETE CASCADE,
    FOREIGN KEY(frontend_symbol_id) REFERENCES symbols(id) ON DELETE CASCADE,
    UNIQUE(backend_symbol_id, frontend_symbol_id)
);

-- API endpoints (Backend routes)
CREATE TABLE IF NOT EXISTS api_endpoints (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    method TEXT NOT NULL,      -- 'GET', 'POST', 'PUT', 'DELETE'
    path TEXT NOT NULL,        -- '/api/users', '/api/users/:id'
    handler_symbol_id INTEGER, -- Rust handler function
    request_type_id INTEGER,   -- Rust struct (input)
    response_type_id INTEGER,  -- Rust struct (output)
    file_id INTEGER NOT NULL,
    line INTEGER,
    FOREIGN KEY(handler_symbol_id) REFERENCES symbols(id) ON DELETE SET NULL,
    FOREIGN KEY(request_type_id) REFERENCES symbols(id) ON DELETE SET NULL,
    FOREIGN KEY(response_type_id) REFERENCES symbols(id) ON DELETE SET NULL,
    FOREIGN KEY(file_id) REFERENCES files(id) ON DELETE CASCADE,
    UNIQUE(method, path)
);

-- Frontend API calls (fetch/axios)
CREATE TABLE IF NOT EXISTS frontend_api_calls (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    method TEXT,               -- 'GET', 'POST', etc. (may be null)
    path TEXT NOT NULL,        -- '/api/users' or '/api/users/${id}'
    path_pattern TEXT,         -- normalized: '/api/users/:id'
    ts_type_id INTEGER,        -- TS interface used in request
    file_id INTEGER NOT NULL,
    line INTEGER,
    FOREIGN KEY(ts_type_id) REFERENCES symbols(id) ON DELETE SET NULL,
    FOREIGN KEY(file_id) REFERENCES files(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_api_endpoints_path ON api_endpoints(path);
CREATE INDEX IF NOT EXISTS idx_frontend_calls_path ON frontend_api_calls(path_pattern);
