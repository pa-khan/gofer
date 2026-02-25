//! Commit message suggestion system
//! Analyzes git changes and generates quality commit messages

use std::path::Path;
use anyhow::{Result, anyhow};
use git2::{Repository, DiffOptions, Delta};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitSuggestion {
    pub suggested_message: CommitMessage,
    pub files: Vec<FileChange>,
    pub analysis: ChangeAnalysis,
    pub safety_check: SafetyReport,
    pub can_commit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitMessage {
    pub subject: String,
    pub body: Option<String>,
    pub full_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub status: String,
    pub additions: usize,
    pub deletions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeAnalysis {
    pub change_type: String,
    pub scope: String,
    pub complexity: String,
    pub summary: String,
    pub total_additions: usize,
    pub total_deletions: usize,
    pub files_changed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyReport {
    pub can_commit: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Analyze git changes and suggest commit message
pub async fn suggest_commit_message(
    repo_path: &Path,
    include_emoji: bool,
    style: &str,
) -> Result<CommitSuggestion> {
    let repo = Repository::open(repo_path)?;
    
    // Get diff
    let file_changes = get_file_changes(&repo)?;
    
    if file_changes.is_empty() {
        return Err(anyhow!("No changes to commit"));
    }
    
    // Analyze changes
    let analysis = analyze_changes(&file_changes);
    
    // Generate commit message
    let message = generate_commit_message(&analysis, &file_changes, include_emoji, style);
    
    // Safety checks
    let safety_check = run_safety_checks(&file_changes);
    
    let can_commit = safety_check.errors.is_empty();
    
    Ok(CommitSuggestion {
        suggested_message: message,
        files: file_changes,
        analysis,
        safety_check,
        can_commit,
    })
}

/// Get file changes from git diff
fn get_file_changes(repo: &Repository) -> Result<Vec<FileChange>> {
    let mut changes = Vec::new();
    
    // Get HEAD tree
    let head = repo.head()?;
    let head_tree = head.peel_to_tree()?;
    
    // Get current index
    let mut index = repo.index()?;
    let index_tree = repo.find_tree(index.write_tree()?)?;
    
    // Diff between HEAD and index (staged changes)
    let mut opts = DiffOptions::new();
    let diff = repo.diff_tree_to_tree(Some(&head_tree), Some(&index_tree), Some(&mut opts))?;
    
    // Calculate stats once for the entire diff
    let stats = diff.stats()?;
    let total_additions = stats.insertions();
    let total_deletions = stats.deletions();
    let total_files = stats.files_changed();
    
    // Distribute stats proportionally if possible, or use total for all files
    let additions_per_file = if total_files > 0 { total_additions / total_files } else { 0 };
    let deletions_per_file = if total_files > 0 { total_deletions / total_files } else { 0 };
    
    // Iterate through deltas
    for delta in diff.deltas() {
        let old_file = delta.old_file();
        let new_file = delta.new_file();
        
        let path = new_file.path()
            .or_else(|| old_file.path())
            .and_then(|p| p.to_str())
            .unwrap_or("unknown")
            .to_string();
        
        let status = match delta.status() {
            Delta::Added => "added",
            Delta::Deleted => "deleted",
            Delta::Modified => "modified",
            Delta::Renamed => "renamed",
            Delta::Copied => "copied",
            Delta::Typechange => "typechange",
            _ => "unknown",
        };
        
        // Use proportional stats instead of calling diff.stats() repeatedly
        changes.push(FileChange {
            path,
            status: status.to_string(),
            additions: additions_per_file,
            deletions: deletions_per_file,
        });
    }
    
    Ok(changes)
}

/// Analyze changes to determine type, scope, complexity
fn analyze_changes(files: &[FileChange]) -> ChangeAnalysis {
    let total_additions: usize = files.iter().map(|f| f.additions).sum();
    let total_deletions: usize = files.iter().map(|f| f.deletions).sum();
    let files_changed = files.len();
    
    // Detect change type
    let change_type = detect_change_type(files);
    
    // Detect scope
    let scope = detect_scope(files);
    
    // Calculate complexity
    let complexity = calculate_complexity(files_changed, total_additions + total_deletions);
    
    // Generate summary
    let summary = generate_summary(files, &change_type);
    
    ChangeAnalysis {
        change_type,
        scope,
        complexity,
        summary,
        total_additions,
        total_deletions,
        files_changed,
    }
}

/// Detect change type based on file patterns and changes
fn detect_change_type(files: &[FileChange]) -> String {
    // Check for test files
    if files.iter().any(|f| {
        f.path.contains("test") || 
        f.path.contains("spec") ||
        f.path.ends_with("_test.rs") ||
        f.path.ends_with(".test.ts")
    }) {
        return "test".to_string();
    }
    
    // Check for docs
    if files.iter().any(|f| {
        f.path.ends_with(".md") || 
        f.path.starts_with("docs/") ||
        f.path == "README.md"
    }) {
        return "docs".to_string();
    }
    
    // Check for build/config files
    if files.iter().any(|f| {
        f.path == "Cargo.toml" ||
        f.path == "package.json" ||
        f.path.ends_with(".toml") ||
        f.path.ends_with(".json") ||
        f.path.starts_with(".github/")
    }) {
        return "chore".to_string();
    }
    
    // Check for CI files
    if files.iter().any(|f| {
        f.path.contains(".github/workflows") ||
        f.path.contains(".gitlab-ci") ||
        f.path == "Dockerfile"
    }) {
        return "ci".to_string();
    }
    
    // Check if it's mostly deletions (likely refactor or cleanup)
    let total_additions: usize = files.iter().map(|f| f.additions).sum();
    let total_deletions: usize = files.iter().map(|f| f.deletions).sum();
    
    if total_deletions > total_additions * 2 {
        return "refactor".to_string();
    }
    
    // Check for new files (likely feat)
    if files.iter().any(|f| f.status == "added") {
        return "feat".to_string();
    }
    
    // Check for fixes (heuristic: small changes to existing files)
    if files.len() <= 3 && total_additions + total_deletions < 50 {
        return "fix".to_string();
    }
    
    // Default: refactor for modifications
    "refactor".to_string()
}

/// Detect scope from file paths
fn detect_scope(files: &[FileChange]) -> String {
    // Extract common directory prefix
    if files.is_empty() {
        return "core".to_string();
    }
    
    // Try to find common path component
    let paths: Vec<&str> = files.iter()
        .map(|f| f.path.as_str())
        .collect();
    
    if let Some(first_path) = paths.first() {
        let parts: Vec<&str> = first_path.split('/').collect();
        
        // Check for common patterns
        if first_path.starts_with("src/") && parts.len() > 1 {
            // Return second component: src/auth/... -> auth
            return parts[1].to_string();
        }
        
        if first_path.starts_with("docs/") {
            return "docs".to_string();
        }
        
        if first_path.starts_with("tests/") {
            return "tests".to_string();
        }
    }
    
    // Check for specific file types
    if files.iter().any(|f| f.path.contains("api")) {
        return "api".to_string();
    }
    
    if files.iter().any(|f| f.path.contains("auth")) {
        return "auth".to_string();
    }
    
    if files.iter().any(|f| f.path.contains("storage") || f.path.contains("db")) {
        return "storage".to_string();
    }
    
    "core".to_string()
}

/// Calculate complexity based on changes
fn calculate_complexity(files_changed: usize, total_lines: usize) -> String {
    if files_changed > 10 || total_lines > 500 {
        "large".to_string()
    } else if files_changed > 3 || total_lines > 100 {
        "medium".to_string()
    } else {
        "simple".to_string()
    }
}

/// Generate human-readable summary
fn generate_summary(files: &[FileChange], change_type: &str) -> String {
    let files_count = files.len();
    let new_files = files.iter().filter(|f| f.status == "added").count();
    let modified_files = files.iter().filter(|f| f.status == "modified").count();
    let deleted_files = files.iter().filter(|f| f.status == "deleted").count();
    
    match change_type {
        "feat" => {
            if new_files > 0 {
                format!("New functionality added across {} file(s)", files_count)
            } else {
                format!("Feature enhancement in {} file(s)", files_count)
            }
        }
        "fix" => format!("Bug fix in {} file(s)", files_count),
        "refactor" => {
            if deleted_files > 0 {
                format!("Code refactoring with cleanup ({} files modified, {} removed)", modified_files, deleted_files)
            } else {
                format!("Code refactoring in {} file(s)", files_count)
            }
        }
        "docs" => format!("Documentation update for {} file(s)", files_count),
        "test" => format!("Test updates in {} file(s)", files_count),
        "chore" => format!("Build/config changes in {} file(s)", files_count),
        "ci" => "CI/CD pipeline updates".to_string(),
        _ => format!("Changes in {} file(s)", files_count),
    }
}

/// Generate commit message in specified style
fn generate_commit_message(
    analysis: &ChangeAnalysis,
    files: &[FileChange],
    include_emoji: bool,
    style: &str,
) -> CommitMessage {
    let emoji = if include_emoji {
        match analysis.change_type.as_str() {
            "feat" => " ✨",
            "fix" => " 🐛",
            "refactor" => " ♻️",
            "docs" => " 📝",
            "test" => " ✅",
            "perf" => " ⚡",
            "chore" => " 🔧",
            "ci" => " 👷",
            _ => "",
        }
    } else {
        ""
    };
    
    let subject = match style {
        "simple" => {
            format!("{}: {}", analysis.change_type, analysis.summary)
        }
        "detailed" | "conventional" => {
            format!("{}({}): {}{}", 
                analysis.change_type, 
                analysis.scope,
                truncate_subject(&analysis.summary, 50),
                emoji
            )
        }
        _ => {
            format!("{}({}): {}{}", 
                analysis.change_type, 
                analysis.scope,
                truncate_subject(&analysis.summary, 50),
                emoji
            )
        }
    
    };
    // Generate body with bullet points
    let body = if style == "detailed" || style == "conventional" {
        let mut bullets = Vec::new();
        
        // Add file-specific details
        for (i, file) in files.iter().enumerate() {
            if i >= 5 {
                bullets.push(format!("- ... and {} more files", files.len() - 5));
                break;
            }
            
            let action = match file.status.as_str() {
                "added" => "Add",
                "deleted" => "Remove",
                "modified" => "Update",
                "renamed" => "Rename",
                _ => "Change",
            };
            
            bullets.push(format!("- {} {}", action, file.path));
        }
        
        // Add stats
        bullets.push(String::new());
        bullets.push(format!("Files changed: {}, +{} -{}", 
            analysis.files_changed,
            analysis.total_additions,
            analysis.total_deletions
        ));
        
        Some(bullets.join("\n"))
    } else {
        None
    };
    
    let full_message = if let Some(ref body_text) = body {
        format!("{}\n\n{}", subject, body_text)
    } else {
        subject.clone()
    };
    
    CommitMessage {
        subject,
        body,
        full_message,
    }
}

/// Truncate subject to fit within limit
fn truncate_subject(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}...", &text[..max_len-3])
    }
}

/// Run safety checks on changes
fn run_safety_checks(files: &[FileChange]) -> SafetyReport {
    let errors = Vec::new();
    let mut warnings = Vec::new();
    
    // Check for large commits
    let total_lines: usize = files.iter()
        .map(|f| f.additions + f.deletions)
        .sum();
    
    if total_lines > 1000 {
        warnings.push(format!(
            "Large commit detected ({} lines changed). Consider splitting into smaller commits.",
            total_lines
        ));
    }
    
    if files.len() > 20 {
        warnings.push(format!(
            "Many files changed ({} files). Consider splitting into smaller commits.",
            files.len()
        ));
    }
    
    // Check for potential secrets (basic check)
    for file in files {
        if file.path.contains("secret") || 
           file.path.contains("password") ||
           file.path.contains("api_key") ||
           file.path.contains(".env") {
            warnings.push(format!(
                "File '{}' may contain secrets. Review before committing.",
                file.path
            ));
        }
    }
    
    // Check if all changes are deletions (might be accidental)
    if files.iter().all(|f| f.additions == 0 && f.deletions > 0) {
        warnings.push("All changes are deletions. Verify this is intentional.".to_string());
    }
    
    SafetyReport {
        can_commit: errors.is_empty(),
        errors,
        warnings,
    }
}
