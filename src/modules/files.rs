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

pub fn update_local_files(state: &mut FileState) {
    let mut local_files = Vec::new();
    let mut global_files = Vec::new();
    state.metadata.clear();

    // 1. LOCAL SEARCH (Always performed)
    if let Ok(entries) = std::fs::read_dir(&state.current_path) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            
            // Hidden filtering
            if !state.show_hidden && name.starts_with('.') {
                continue;
            }
            
            // Search filtering
            if !state.search_filter.is_empty() && !name.to_lowercase().contains(&state.search_filter.to_lowercase()) {
                continue;
            }

            if let Ok(m) = std::fs::metadata(&path).or_else(|_| entry.metadata()) {
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
                state.metadata.insert(path.clone(), meta);
                local_files.push(path);
            }
        }
    }

    // Sort local files (Dirs first, then by sort_column)
    // We'll reuse the existing state.files sorting logic by temporarily assigning local_files to state.files
    state.files = local_files;
    sort_files(state);
    local_files = state.files.clone();

    // 2. GLOBAL SEARCH (Recursive from Root, if filter >= 3)
    if state.search_filter.len() >= 3 {
        let mut scored_global = Vec::new();
        let filter_lower = state.search_filter.to_lowercase();
        
        // Search the whole disk, but skip system/virtual folders for speed and safety
        let walker = walkdir::WalkDir::new("/")
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                let path = e.path();
                if path.is_dir() {
                    let p_str = path.to_string_lossy();
                    // Skip virtual filesystems and massive system directories
                    if p_str == "/proc" || p_str == "/sys" || p_str == "/dev" || 
                       p_str == "/run" || p_str == "/tmp" || p_str == "/nix/store" ||
                       p_str == "/var/lib" || p_str == "/var/cache" 
                    { 
                        return false; 
                    }
                }
                
                let name = e.file_name().to_string_lossy();
                if !state.show_hidden && name.starts_with('.') { return false; }
                true
            });

        for entry in walker.filter_map(|e| e.ok()) {
            if scored_global.len() >= 10000 { break; } // Traverse deeply
            let path = entry.path().to_path_buf();
            
            if path == state.current_path || local_files.contains(&path) { continue; }

            // Match against the full path since we are searching from /
            let search_target = path.to_string_lossy().to_lowercase();
            
            if search_target.contains(&filter_lower) {
                if let Ok(m) = entry.metadata() {
                    // SCORING: lower is better
                    let depth = entry.depth();
                    let path_len = path.to_string_lossy().len();
                    
                    // Priority: if filename matches, it's better than just path matching
                    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
                    let filename_bonus = if filename.contains(&filter_lower) { 0 } else { 500 };
                    let is_exact = if filename == filter_lower { 0 } else { 100 };
                    
                    let hidden_penalty = if path.to_string_lossy().contains("/.") { 100 } else { 0 };
                    
                    // Boost files closer to the current directory
                    let proximity_bonus = if path.starts_with(&state.current_path) { 0 } else { 1000 };
                    
                    let score = depth * 10 + path_len + filename_bonus + is_exact + hidden_penalty + proximity_bonus;
                    
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
                    scored_global.push((score, path, meta));
                }
            }
        }
        
        // Sort by score and take top 500
        scored_global.sort_by_key(|(s, _, _)| *s);
        for (_, path, meta) in scored_global.into_iter().take(500) {
            state.metadata.insert(path.clone(), meta);
            global_files.push(path);
        }
    }

    // Combine: Local results followed by Global results
    state.local_count = local_files.len();
    state.files = local_files;
    
    if !global_files.is_empty() {
        // Insert a sentinel path to represent the divider
        state.files.push(std::path::PathBuf::from("__DIVIDER__"));
        state.files.extend(global_files);
    }

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
