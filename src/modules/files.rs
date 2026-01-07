#![allow(dead_code)]
use crate::app::FileState;

pub fn update_files(state: &mut FileState, session: Option<&ssh2::Session>) {
    if let Some(sess) = session {
        update_remote_files(state, sess);
    } else {
        update_local_files(state);
    }
}

fn update_local_files(state: &mut FileState) {
    if let Ok(entries) = std::fs::read_dir(&state.current_path) {
        state.files.clear();
        state.metadata.clear();

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

        // Sort files: directories first, then by selected column
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
                crate::app::FileColumn::Name => a.file_name().cmp(&b.file_name()),
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
                crate::app::FileColumn::Extension => {
                    let a_ext = a.extension().and_then(|e| e.to_str()).unwrap_or("");
                    let b_ext = b.extension().and_then(|e| e.to_str()).unwrap_or("");
                    a_ext.cmp(b_ext)
                }
            };

            if sort_asc {
                ord
            } else {
                ord.reverse()
            }
        });

        // Git Integration (Disabled/Backlog)
        state.git_status.clear();
    }
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
            state
                .files
                .sort_by(|a, b| a.file_name().cmp(&b.file_name()));
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
