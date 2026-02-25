-- Knowledge Base: Dependencies and Rules

-- Track project dependencies (from Cargo.toml, package.json)
CREATE TABLE IF NOT EXISTS dependencies (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    version TEXT NOT NULL,
    ecosystem TEXT NOT NULL,  -- 'cargo' or 'npm'
    features TEXT,            -- JSON array of features
    dev_only INTEGER DEFAULT 0,
    updated_at INTEGER NOT NULL,
    UNIQUE(name, ecosystem)
);

CREATE INDEX IF NOT EXISTS idx_deps_ecosystem ON dependencies(ecosystem);

-- Project rules and best practices
CREATE TABLE IF NOT EXISTS rules (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    category TEXT NOT NULL,   -- 'error_handling', 'async', 'style', etc.
    rule TEXT NOT NULL,
    priority INTEGER DEFAULT 0,
    source TEXT               -- file path where rule was defined
);

-- Golden sample files (exemplary code)
CREATE TABLE IF NOT EXISTS golden_samples (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id INTEGER NOT NULL,
    category TEXT,            -- 'handler', 'service', 'component', etc.
    description TEXT,
    FOREIGN KEY(file_id) REFERENCES files(id) ON DELETE CASCADE
);

-- Dependency usage: tracks WHERE each dependency is used in the codebase
CREATE TABLE IF NOT EXISTS dependency_usage (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    dependency_id INTEGER NOT NULL,
    file_id INTEGER NOT NULL,
    line INTEGER NOT NULL,
    usage_type TEXT NOT NULL,  -- 'import', 'require', 'use'
    import_path TEXT NOT NULL, -- full import path (e.g., 'sqlx::Pool', 'vue')
    items TEXT,                -- JSON array of imported items
    FOREIGN KEY(dependency_id) REFERENCES dependencies(id) ON DELETE CASCADE,
    FOREIGN KEY(file_id) REFERENCES files(id) ON DELETE CASCADE,
    UNIQUE(file_id, line, import_path)
);

CREATE INDEX IF NOT EXISTS idx_dep_usage_dep ON dependency_usage(dependency_id);
CREATE INDEX IF NOT EXISTS idx_dep_usage_file ON dependency_usage(file_id);
