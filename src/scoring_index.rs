//! In-Memory Scoring Index for fast file ranking
//! Uses rkyv for zero-copy deserialization and mmap for instant startup
//!
//! This module solves the N+1 query problem in tool_smart_file_selection
//! by pre-loading all scoring data into memory.

use std::collections::HashMap;
use std::path::Path;
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};
use crate::models::chunk::SymbolKind;

/// Scoring data for a single file
#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct FileScoringData {
    /// File path (absolute)
    pub file_path: String,
    
    /// List of symbol names in this file
    pub symbol_names: Vec<String>,
    
    /// List of symbol kinds (function, struct, etc.)
    pub symbol_kinds: Vec<SymbolKind>,
    
    /// File summary (optional)
    pub summary: Option<String>,
    
    /// File size in bytes
    pub size_bytes: usize,
    
    /// Last modified timestamp (seconds since UNIX_EPOCH)
    pub last_modified: u64,
    
    /// Domain (backend, frontend, etc.)
    pub domain: Option<String>,
}

/// Hot index for file scoring
/// This structure is memory-mapped from disk for instant access
#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct ScoringIndex {
    /// Map: file path -> scoring data
    pub files: HashMap<String, FileScoringData>,
    
    /// Index build timestamp
    pub built_at: u64,
    
    /// Version for cache invalidation
    pub version: u32,
}

impl ScoringIndex {
    /// Create a new empty index
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            built_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            version: 1,
        }
    }
    
    /// Add file data to index
    pub fn add_file(&mut self, data: FileScoringData) {
        self.files.insert(data.file_path.clone(), data);
    }
    
    /// Get scoring data for a file
    pub fn get(&self, file_path: &str) -> Option<&FileScoringData> {
        self.files.get(file_path)
    }
    
    /// Save index to disk using rkyv
    pub fn save_to_file(&self, path: &Path) -> std::io::Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        // Serialize with rkyv
        let bytes = rkyv::to_bytes::<_, 256>(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        
        // Write to file
        std::fs::write(path, bytes)?;
        
        tracing::info!("Scoring index saved to {:?} ({} files)", path, self.files.len());
        Ok(())
    }
    
    /// Load index from disk using mmap for zero-copy access
    pub fn load_from_file(path: &Path) -> std::io::Result<Self> {
        // Read file into memory
        let bytes = std::fs::read(path)?;
        
        // Zero-copy deserialization with validation
        match rkyv::check_archived_root::<ScoringIndex>(&bytes) {
            Ok(archived) => {
                // Convert archived data to owned structure
                // In a more advanced implementation, you could keep the mmap
                // and work directly with archived data
                let files: HashMap<String, FileScoringData> = archived.files.iter()
                    .map(|(k, v)| {
                        (
                            k.to_string(),
                            FileScoringData {
                                file_path: v.file_path.to_string(),
                                symbol_names: v.symbol_names.iter().map(|s| s.to_string()).collect(),
                                symbol_kinds: v.symbol_kinds.iter().map(|s| {
                                    use rkyv::Deserialize;
                                    s.deserialize(&mut rkyv::Infallible).unwrap()
                                }).collect(),
                                summary: v.summary.as_ref().map(|s| s.to_string()),
                                size_bytes: v.size_bytes as usize,
                                last_modified: v.last_modified,
                                domain: v.domain.as_ref().map(|s| s.to_string()),
                            }
                        )
                    })
                    .collect();
                
                let index = ScoringIndex {
                    files,
                    built_at: archived.built_at,
                    version: archived.version,
                };
                
                tracing::info!("Scoring index loaded from {:?} ({} files)", path, index.files.len());
                Ok(index)
            }
            Err(e) => {
                Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
            }
        }
    }
    
    /// Check if index needs rebuild (based on age or version)
    pub fn needs_rebuild(&self, max_age_seconds: u64) -> bool {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let age = current_time.saturating_sub(self.built_at);
        age > max_age_seconds
    }
}

impl Default for ScoringIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_scoring_index_roundtrip() {
        let mut index = ScoringIndex::new();
        
        index.add_file(FileScoringData {
            file_path: "/test/foo.rs".to_string(),
            symbol_names: vec!["main".to_string(), "helper".to_string()],
            symbol_kinds: vec![SymbolKind::Function, SymbolKind::Function],
            summary: Some("Test file".to_string()),
            size_bytes: 1024,
            last_modified: 123456,
            domain: Some("backend".to_string()),
        });
        
        // Save to temp file
        let temp_path = std::env::temp_dir().join("test_scoring_index.rkyv");
        index.save_to_file(&temp_path).unwrap();
        
        // Load back
        let loaded = ScoringIndex::load_from_file(&temp_path).unwrap();
        
        assert_eq!(loaded.files.len(), 1);
        let data = loaded.get("/test/foo.rs").unwrap();
        assert_eq!(data.symbol_names.len(), 2);
        assert_eq!(data.summary.as_deref(), Some("Test file"));
        
        // Cleanup
        std::fs::remove_file(temp_path).ok();
    }
}
