-- Migration: Query Optimization
-- Adds critical indexes for improved query performance

-- Index on symbol kind for filtering by type
CREATE INDEX IF NOT EXISTS idx_symbols_kind ON symbols(kind);

-- Composite index for symbol kind + name queries
CREATE INDEX IF NOT EXISTS idx_symbols_kind_name ON symbols(kind, name);

-- Composite index for file + kind queries (e.g., all functions in a file)
CREATE INDEX IF NOT EXISTS idx_symbols_file_kind ON symbols(file_id, kind);

-- Composite index for symbol_references lookup (correct table name)
CREATE INDEX IF NOT EXISTS idx_symbol_refs_target ON symbol_references(target_symbol_id);

-- Index on active_errors for fast error/warning queries (correct table name)
CREATE INDEX IF NOT EXISTS idx_active_errors_file_severity ON active_errors(file_path, severity);

-- Index on domains for cross-stack queries
CREATE INDEX IF NOT EXISTS idx_files_domain ON files(domain);

-- Composite index for cross-stack links navigation
CREATE INDEX IF NOT EXISTS idx_links_source ON cross_stack_links(source_file, link_type);
CREATE INDEX IF NOT EXISTS idx_links_target ON cross_stack_links(target_file, link_type);
