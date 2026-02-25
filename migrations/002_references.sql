-- Symbol references (dependency graph)
-- Tracks which symbols reference other symbols

CREATE TABLE IF NOT EXISTS symbol_references (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_symbol_id INTEGER NOT NULL,
    target_name TEXT NOT NULL,
    target_symbol_id INTEGER,
    kind TEXT NOT NULL,
    line INTEGER NOT NULL,
    FOREIGN KEY(source_symbol_id) REFERENCES symbols(id) ON DELETE CASCADE,
    FOREIGN KEY(target_symbol_id) REFERENCES symbols(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_refs_source ON symbol_references(source_symbol_id);
CREATE INDEX IF NOT EXISTS idx_refs_target_name ON symbol_references(target_name);
CREATE INDEX IF NOT EXISTS idx_refs_target_symbol ON symbol_references(target_symbol_id);
