use std::path::{Path, PathBuf};
use std::collections::HashMap;
use crate::app::{FileMetadata};

pub fn read_dir_with_metadata(path: &Path) -> (Vec<PathBuf>, HashMap<PathBuf, FileMetadata>) {
    let mut files = Vec::new();
    let mut metadata = HashMap::new();

    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.filter_map(|e| e.ok()) {
            let p = entry.path();
            if let Ok(m) = entry.metadata() {
                let meta = FileMetadata {
                    size: m.len(),
                    modified: m.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                    created: m.created().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                    permissions: 0, // Simplified
                    is_dir: m.is_dir(),
                };
                files.push(p.clone());
                metadata.insert(p, meta);
            }
        }
    }

    (files, metadata)
}

pub fn get_file_category(path: &Path) -> crate::app::FileCategory {
    terma::utils::get_file_category(path)
}

pub fn update_git_status(state: &mut crate::app::FileState) {
    // Mock implementation for now
    state.git_branch = Some("master".to_string());
    state.git_ahead = 0;
    state.git_behind = 0;
    state.git_pending = Vec::new();
}

pub fn copy_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    terma::utils::copy_recursive(src, dst)
}
