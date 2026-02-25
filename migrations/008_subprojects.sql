-- Sub-project / monorepo workspace member tracking

CREATE TABLE IF NOT EXISTS subprojects (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL,
    path        TEXT NOT NULL UNIQUE,     -- relative path from project root
    kind        TEXT NOT NULL,            -- 'cargo', 'npm', 'go', 'python'
    parent_path TEXT,                     -- parent workspace path (if nested)
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Tag each file with its owning sub-project
ALTER TABLE files ADD COLUMN subproject_id INTEGER REFERENCES subprojects(id);

CREATE INDEX IF NOT EXISTS idx_subprojects_kind ON subprojects(kind);
CREATE INDEX IF NOT EXISTS idx_files_subproject ON files(subproject_id);
