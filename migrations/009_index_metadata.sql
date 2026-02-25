-- Index Metadata Tracking (consolidated schema)
-- Provides visibility into index state and completeness
CREATE TABLE IF NOT EXISTS index_metadata (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    key TEXT NOT NULL UNIQUE,
    value TEXT NOT NULL,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Initialize default metadata
INSERT OR IGNORE INTO index_metadata (key, value) VALUES 
    ('last_full_sync', ''),
    ('indexing_started_at', ''),
    ('indexing_completed_at', ''),
    ('total_files_indexed', '0'),
    ('total_symbols_indexed', '0'),
    ('total_chunks_indexed', '0'),
    ('index_version', '1.0');
