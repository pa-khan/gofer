-- File Summaries for Semantic Search Enhancement

-- Stores LLM-generated or docstring-extracted summaries
CREATE TABLE IF NOT EXISTS file_summaries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id INTEGER NOT NULL UNIQUE,
    summary TEXT NOT NULL,              -- Human-readable summary
    summary_source TEXT NOT NULL,       -- 'llm', 'docstring', 'comment'
    model_name TEXT,                    -- e.g., 'qwen2.5-coder-1.5b'
    confidence REAL,                    -- Model confidence if available
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY(file_id) REFERENCES files(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_summaries_file ON file_summaries(file_id);
CREATE INDEX IF NOT EXISTS idx_summaries_source ON file_summaries(summary_source);

-- Queue for pending summarization tasks
CREATE TABLE IF NOT EXISTS summary_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id INTEGER NOT NULL,
    priority INTEGER DEFAULT 0,         -- Higher = more urgent
    status TEXT DEFAULT 'pending',      -- 'pending', 'processing', 'completed', 'failed'
    error_message TEXT,
    created_at INTEGER NOT NULL,
    FOREIGN KEY(file_id) REFERENCES files(id) ON DELETE CASCADE,
    UNIQUE(file_id, status)
);

CREATE INDEX IF NOT EXISTS idx_queue_status ON summary_queue(status);
