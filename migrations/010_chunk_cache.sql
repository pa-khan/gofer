-- Chunk embedding cache: stores content_hash → embedding to skip re-embedding
-- unchanged chunks across incremental syncs.
CREATE TABLE IF NOT EXISTS chunk_cache (
    content_hash TEXT PRIMARY KEY,
    embedding    BLOB NOT NULL,
    created_at   INTEGER NOT NULL DEFAULT (unixepoch())
);
