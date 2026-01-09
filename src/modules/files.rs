#![allow(dead_code)]
use crate::app::FileState;

pub fn update_files(state: &mut FileState, session: Option<&ssh2::Session>) {
    if let Some(sess) = session {
        update_remote_files(state, sess);
    } else {
        update_local_files(state);
    }
}

fn sort_files(state: &mut FileState) {
    let sort_col = state.sort_column;
    let sort_asc = state.sort_ascending;
    state.files.sort_by(|a, b| {
        let a_meta = state.metadata.get(a);
        let b_meta = state.metadata.get(b);
        let a_is_dir = a_meta.map(|m| m.is_dir).unwrap_or(false);
        let b_is_dir = b_meta.map(|m| m.is_dir).unwrap_or(false);

        // Directories always come first
        if a_is_dir && !b_is_dir {
            return std::cmp::Ordering::Less;
        } else if !a_is_dir && b_is_dir {
            return std::cmp::Ordering::Greater;
        }

        // Within same type, sort by selected column
        let ord = match sort_col {
            crate::app::FileColumn::Name => {
                let a_name = a.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
                let b_name = b.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
                a_name.cmp(&b_name)
            },
            crate::app::FileColumn::Size => {
                let a_size = a_meta.map(|m| m.size).unwrap_or(0);
                let b_size = b_meta.map(|m| m.size).unwrap_or(0);
                a_size.cmp(&b_size)
            }
            crate::app::FileColumn::Modified => {
                let a_mod = a_meta
                    .map(|m| m.modified)
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                let b_mod = b_meta
                    .map(|m| m.modified)
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                a_mod.cmp(&b_mod)
            }
            crate::app::FileColumn::Created => {
                let a_cr = a_meta
                    .map(|m| m.created)
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                let b_cr = b_meta
                    .map(|m| m.created)
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                a_cr.cmp(&b_cr)
            }
            crate::app::FileColumn::Permissions => {
                let a_perm = a_meta.map(|m| m.permissions).unwrap_or(0);
                let b_perm = b_meta.map(|m| m.permissions).unwrap_or(0);
                a_perm.cmp(&b_perm)
            }
        };

        if sort_asc {
            ord
        } else {
            ord.reverse()
        }
    });
}

fn update_local_files(state: &mut FileState) {
    state.files.clear();
    state.metadata.clear();

    if state.search_filter.len() >= 3 {
        // Global Search (Recursive)
        let mut count = 0;
        let mut scored_files = Vec::new();
        let walker = walkdir::WalkDir::new(&state.current_path)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                if !state.show_hidden && name.starts_with('.') { return false; }
                true
            });

        let filter_lower = state.search_filter.to_lowercase();

        for entry in walker.filter_map(|e| e.ok()) {
            if count >= 1000 { break; } // Increase limit for scoring
            let path = entry.path().to_path_buf();
            if path == state.current_path { continue; }

            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let name_lower = name.to_lowercase();
            
            if name_lower.contains(&filter_lower) {
                if let Ok(m) = entry.metadata() {
                    // Simple score: shorter paths and closer depth are better
                    // Subtract from a large number so we can sort descending or just use it as is
                    let depth = entry.depth();
                    let path_len = path.to_string_lossy().len();
                    let is_exact = if name_lower == filter_lower { 0 } else { 1 };
                    let score = depth * 10 + path_len + is_exact * 100;
                    
                    let meta = crate::app::FileMetadata {
                        size: m.len(),
                        modified: m.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                        created: m.created().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                        #[cfg(unix)]
                        permissions: {
                            use std::os::unix::fs::PermissionsExt;
                            m.permissions().mode()
                        },
                        #[cfg(not(unix))]
                        permissions: 0,
                        extension: path.extension().and_then(|e| e.to_str()).unwrap_or("").to_string(),
                        is_dir: m.is_dir(),
                    };
                    scored_files.push((score, path, meta));
                    count += 1;
                }
            }
        }
        
        // Sort by score (ascending: smaller score = better)
        scored_files.sort_by_key(|(s, _, _)| *s);
        
        for (_, path, meta) in scored_files.into_iter().take(100) {
            state.metadata.insert(path.clone(), meta);
            state.files.push(path);
        }
    } else {
        // Local Search (Current Dir Only)
        if let Ok(entries) = std::fs::read_dir(&state.current_path) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();

                // Filtering
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !state.show_hidden && name.starts_with('.') {
                    continue;
                }
                if !state.search_filter.is_empty()
                    && !name
                        .to_lowercase()
                        .contains(&state.search_filter.to_lowercase())
                {
                    continue;
                }

                // Metadata Cache (The Fix)
                // Use std::fs::metadata to ensure we follow symlinks to get the REAL file size.
                // Fallback to entry.metadata() (symlink itself) if target is broken.
                let metadata_result = std::fs::metadata(&path).or_else(|_| entry.metadata());

                if let Ok(m) = metadata_result {
                    let meta = crate::app::FileMetadata {
                        size: m.len(),
                        modified: m.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                        created: m.created().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                        #[cfg(unix)]
                        permissions: {
                            use std::os::unix::fs::PermissionsExt;
                            m.permissions().mode()
                        },
                        #[cfg(not(unix))]
                        permissions: 0,
                        extension: path
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("")
                            .to_string(),
                        is_dir: m.is_dir(),
                    };
                    state.metadata.insert(path.clone(), meta);
                }
                state.files.push(path);
            }
        }
    }

    sort_files(state);

    // Git Integration
    state.git_status.clear();
    state.git_branch = get_git_branch(&state.current_path);
}

fn get_git_branch(path: &std::path::Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(path)
        .output()
        .ok()?;

    if output.status.success() {
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !branch.is_empty() {
            return Some(branch);
        }
    }
    None
}

fn update_remote_files(state: &mut FileState, session: &ssh2::Session) {
    if let Ok(sftp) = session.sftp() {
        if let Ok(entries) = sftp.readdir(&state.current_path) {
            state.files.clear();
            state.metadata.clear();

            for (path, stat) in entries {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !state.show_hidden && name.starts_with('.') {
                    continue;
                }
                if !state.search_filter.is_empty()
                    && !name
                        .to_lowercase()
                        .contains(&state.search_filter.to_lowercase())
                {
                    continue;
                }

                let meta = crate::app::FileMetadata {
                    size: stat.size.unwrap_or(0),
                    modified: stat
                        .mtime
                        .map(|t| {
                            std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(t)
                        })
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                    created: std::time::SystemTime::UNIX_EPOCH, // SFTP usually doesn't provide birth time easily
                    permissions: stat.perm.unwrap_or(0),
                    extension: path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_string(),
                    is_dir: stat.is_dir(),
                };
                state.metadata.insert(path.clone(), meta);
                state.files.push(path);
            }

            sort_files(state);
        }
    }
}

pub fn copy_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            if ty.is_dir() {
                copy_recursive(&entry.path(), &dst.join(entry.file_name()))?;
            } else {
                std::fs::copy(entry.path(), dst.join(entry.file_name()))?;
            }
        }
    } else {
        std::fs::copy(src, dst)?;
    }
    Ok(())
}

pub fn move_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    if src == dst {
        return Ok(());
    }

    // Attempt atomic rename first
    if let Err(e) = std::fs::rename(src, dst) {
        // Fallback for cross-device moves (EXDEV = 18)
        let err_code = e.raw_os_error();
        if err_code == Some(18) || e.kind() == std::io::ErrorKind::Other {
            // Safety: Ensure source exists
            if !src.exists() {
                return Err(e);
            }
            // Safety: Don't move into self
            if dst.starts_with(src) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Cannot move into self",
                ));
            }

            copy_recursive(src, dst)?;
            if src.is_dir() {
                std::fs::remove_dir_all(src)?;
            } else {
                std::fs::remove_file(src)?;
            }
        } else {
            return Err(e);
        }
    }
    Ok(())
}
