-- Add missing columns to files table for proper indexing status tracking

-- Add indexing_status column (pending, in_progress, completed, failed)
ALTER TABLE files ADD COLUMN indexing_status TEXT 
    CHECK(indexing_status IN ('pending', 'in_progress', 'completed', 'failed'));

-- Add language column to track file language
ALTER TABLE files ADD COLUMN language TEXT;

-- Add last_indexed_at timestamp
ALTER TABLE files ADD COLUMN last_indexed_at INTEGER;

-- Create indexes for better query performance
CREATE INDEX IF NOT EXISTS idx_files_indexing_status ON files(indexing_status);
CREATE INDEX IF NOT EXISTS idx_files_language ON files(language);
CREATE INDEX IF NOT EXISTS idx_files_last_indexed_at ON files(last_indexed_at);

-- Set default status for existing files
UPDATE files SET indexing_status = 'completed' WHERE indexing_status IS NULL;
