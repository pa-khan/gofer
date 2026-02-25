-- Structural Fingerprinting: type_fingerprints + cross_stack_links

-- Хранит "отпечатки" типов: нормализованный список полей для Jaccard-сравнения
CREATE TABLE IF NOT EXISTS type_fingerprints (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id INTEGER NOT NULL,
    symbol_id INTEGER NOT NULL,
    type_name TEXT NOT NULL,
    language TEXT NOT NULL,          -- 'rust', 'typescript', 'python'
    fields_json TEXT NOT NULL,       -- JSON: ["id", "username", "email", "created_at"]
    fields_normalized TEXT NOT NULL, -- JSON: ["id", "username", "email", "createdat"] (lower, no separators)
    field_count INTEGER NOT NULL,
    FOREIGN KEY(file_id) REFERENCES files(id) ON DELETE CASCADE,
    FOREIGN KEY(symbol_id) REFERENCES symbols(id) ON DELETE CASCADE,
    UNIQUE(symbol_id)
);

CREATE INDEX IF NOT EXISTS idx_type_fingerprints_language ON type_fingerprints(language);
CREATE INDEX IF NOT EXISTS idx_type_fingerprints_field_count ON type_fingerprints(field_count);

-- Взвешенные cross-stack связи (заменяет бинарные entity_links для structural matches)
CREATE TABLE IF NOT EXISTS cross_stack_links (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_file TEXT NOT NULL,       -- путь к файлу источника (бэкенд)
    target_file TEXT NOT NULL,       -- путь к файлу цели (фронтенд)
    source_symbol TEXT NOT NULL,     -- имя типа/структуры
    target_symbol TEXT NOT NULL,     -- имя интерфейса/типа
    link_type TEXT NOT NULL,         -- 'structural', 'temporal', 'semantic', 'explicit_api'
    weight REAL NOT NULL,            -- 0.0 - 1.0
    metadata TEXT,                   -- JSON: {"matched_fields": ["id", "email"], "jaccard": 0.85}
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    UNIQUE(source_symbol, target_symbol, link_type)
);

CREATE INDEX IF NOT EXISTS idx_cross_stack_links_type ON cross_stack_links(link_type);
CREATE INDEX IF NOT EXISTS idx_cross_stack_links_weight ON cross_stack_links(weight DESC);
CREATE INDEX IF NOT EXISTS idx_cross_stack_links_source ON cross_stack_links(source_file);
CREATE INDEX IF NOT EXISTS idx_cross_stack_links_target ON cross_stack_links(target_file);
