-- gofer: Project Memory Service
-- Initial schema

-- Track indexed files
CREATE TABLE IF NOT EXISTS files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT UNIQUE NOT NULL,
    last_modified INTEGER NOT NULL,
    content_hash TEXT NOT NULL,
    domain TEXT CHECK(domain IN ('backend', 'frontend', 'shared', 'ops', 'unknown')),
    tech_stack TEXT  -- JSON array: ["axum", "sqlx"] or ["vue", "tailwindcss"]
);

CREATE INDEX IF NOT EXISTS idx_files_path ON files(path);

-- Code symbols (functions, structs, etc.)
CREATE TABLE IF NOT EXISTS symbols (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    line_start INTEGER NOT NULL,
    line_end INTEGER NOT NULL,
    signature TEXT,
    FOREIGN KEY(file_id) REFERENCES files(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_symbols_file_id ON symbols(file_id);
CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name);

-- FTS5 virtual table for fast text search
CREATE VIRTUAL TABLE IF NOT EXISTS symbols_fts USING fts5(
    name,
    signature,
    content='symbols',
    content_rowid='id'
);

-- Triggers to keep FTS5 in sync
CREATE TRIGGER IF NOT EXISTS symbols_ai AFTER INSERT ON symbols BEGIN
    INSERT INTO symbols_fts(rowid, name, signature)
    VALUES (new.id, new.name, new.signature);
END;

CREATE TRIGGER IF NOT EXISTS symbols_ad AFTER DELETE ON symbols BEGIN
    INSERT INTO symbols_fts(symbols_fts, rowid, name, signature)
    VALUES ('delete', old.id, old.name, old.signature);
END;

CREATE TRIGGER IF NOT EXISTS symbols_au AFTER UPDATE ON symbols BEGIN
    INSERT INTO symbols_fts(symbols_fts, rowid, name, signature)
    VALUES ('delete', old.id, old.name, old.signature);
    INSERT INTO symbols_fts(rowid, name, signature)
    VALUES (new.id, new.name, new.signature);
END;
