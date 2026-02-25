-- Добавляем UNIQUE индекс на golden_samples.file_id
-- Исправление: ON CONFLICT(file_id) в mark_golden_sample требует уникальность
CREATE UNIQUE INDEX IF NOT EXISTS idx_golden_samples_file_id ON golden_samples(file_id);
