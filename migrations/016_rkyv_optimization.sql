-- Migration 016: Convert JSON TEXT fields to BLOB for rkyv zero-copy optimization
-- This migration converts fields that store serialized data from JSON (TEXT) to rkyv (BLOB)
-- for better performance (zero-copy deserialization)

-- Step 1: Add new BLOB columns alongside existing TEXT columns
ALTER TABLE files ADD COLUMN tech_stack_blob BLOB;

ALTER TABLE entity_links ADD COLUMN matched_fields_blob BLOB;

ALTER TABLE cross_stack_links ADD COLUMN metadata_blob BLOB;

ALTER TABLE type_fingerprints ADD COLUMN fields_json_blob BLOB;
ALTER TABLE type_fingerprints ADD COLUMN fields_normalized_blob BLOB;

-- Step 2: Create indexes for better query performance
CREATE INDEX IF NOT EXISTS idx_files_tech_stack_blob ON files(tech_stack_blob) WHERE tech_stack_blob IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_cross_stack_metadata_blob ON cross_stack_links(metadata_blob) WHERE metadata_blob IS NOT NULL;

-- Note: Existing data in TEXT columns remains for backward compatibility during transition
-- The application will gradually migrate to BLOB columns and eventually TEXT columns can be dropped
-- in a future migration once all data is migrated
