#![allow(dead_code)]
use crate::app::{FileCategory, FileState};
use std::path::Path;

pub fn get_file_category(path: &Path) -> FileCategory {
    terma::utils::get_file_category(path)
}

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
                let a_name = a
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                let b_name = b
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                a_name.cmp(&b_name)
            }
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
                let a_time = a_meta
                    .map(|m| m.created)
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                let b_time = b_meta
                    .map(|m| m.created)
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                a_time.cmp(&b_time)
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

fn calculate_folder_size(path: &std::path::Path) -> u64 {
    const MAX_DEPTH: usize = 2;
    const MAX_FILES: usize = 100;

    fn walk_dir(path: &std::path::Path, depth: usize, count: &mut usize) -> std::io::Result<u64> {
        if depth > MAX_DEPTH {
            return Ok(0);
        }
        if *count > MAX_FILES {
            return Ok(0);
        }

        let mut total = 0u64;
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.filter_map(|e| e.ok()) {
                *count += 1;
                if *count > MAX_FILES {
                    break;
                }
                let metadata = entry.metadata();
                if metadata.is_err() {
                    continue;
                }
                let metadata = metadata.unwrap();
                if metadata.is_dir() {
                    total += walk_dir(&entry.path(), depth + 1, count)?;
                } else {
                    total += metadata.len();
                }
            }
        }
        Ok(total)
    }

    let mut count = 0;
    walk_dir(path, 0, &mut count).unwrap_or(0)
}

pub fn update_local_files(state: &mut FileState) {
    let mut local_files = Vec::new();
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
            if !state.search_filter.is_empty()
                && !name
                    .to_lowercase()
                    .contains(&state.search_filter.to_lowercase())
            {
                continue;
            }

            if let Ok(m) = std::fs::metadata(&path).or_else(|_| entry.metadata()) {
                let is_dir = m.is_dir();
                let size = if is_dir {
                    calculate_folder_size(&path)
                } else {
                    m.len()
                };
                let meta = crate::app::FileMetadata {
                    size,
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
                    is_dir,
                };
                state.metadata.insert(path.clone(), meta);
                local_files.push(path);
            }
        }
    }

    // Sort local files (Dirs first, then by sort_column)
    state.files = local_files;
    sort_files(state);
    state.local_count = state.files.len();

    // Git Integration
    state.git_status.clear();
    state.git_branch = get_git_branch(&state.current_path);
    let (ahead, behind) = get_git_sync_status(&state.current_path);
    state.git_ahead = ahead;
    state.git_behind = behind;
    state.git_pending = get_git_status(&state.current_path);
}

pub fn perform_global_search(
    filter: String,
    current_path: std::path::PathBuf,
    show_hidden: bool,
    local_files: Vec<std::path::PathBuf>,
) -> (
    Vec<std::path::PathBuf>,
    std::collections::HashMap<std::path::PathBuf, crate::app::FileMetadata>,
) {
    let mut global_files = Vec::new();
    let mut metadata = std::collections::HashMap::new();
    let mut scored_global = Vec::new();
    let filter_lower = filter.to_lowercase();

    // Search the whole disk, but skip system/virtual folders for speed and safety
    let walker = walkdir::WalkDir::new("/")
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let path = e.path();
            if path.is_dir() {
                let p_str = path.to_string_lossy();
                // Expanded exclusions: system folders and build artifacts
                if p_str == "/proc"
                    || p_str == "/sys"
                    || p_str == "/dev"
                    || p_str == "/run"
                    || p_str == "/tmp"
                    || p_str == "/nix"
                    || p_str == "/var"
                    || p_str == "/usr"
                    || p_str == "/etc"
                    || p_str == "/boot"
                    || p_str == "/root"
                    || p_str == "/lost+found"
                    || p_str.ends_with("/target")
                    || p_str.ends_with("/node_modules")
                {
                    return false;
                }
            }

            let name = e.file_name().to_string_lossy();
            if !show_hidden && name.starts_with('.') {
                return false;
            }
            true
        });

    for entry in walker.filter_map(|e| e.ok()) {
        if scored_global.len() >= 10000 {
            break;
        }
        let path = entry.path().to_path_buf();

        if path == current_path || local_files.contains(&path) {
            continue;
        }

        // Match only against the FILENAME to prevent "everything inside matching folder" noise
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();

        if filename.contains(&filter_lower) {
            if let Ok(m) = entry.metadata() {
                // SCORING: lower is better
                let depth = entry.depth();
                let path_len = path.to_string_lossy().len();

                let is_exact = if filename == filter_lower { 0 } else { 100 };
                let hidden_penalty = if path.to_string_lossy().contains("/.") {
                    100
                } else {
                    0
                };

                // Boost files closer to the current directory
                let proximity_bonus = if path.starts_with(&current_path) {
                    0
                } else {
                    1000
                };

                let score = depth * 10 + path_len + is_exact + hidden_penalty + proximity_bonus;

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
                scored_global.push((score, path, meta));
            }
        }
    }

    // Sort by score and take top 100
    scored_global.sort_by_key(|(s, _, _)| *s);
    for (_, path, meta) in scored_global.into_iter().take(100) {
        metadata.insert(path.clone(), meta);
        global_files.push(path);
    }

    (global_files, metadata)
}

pub fn get_git_branch(path: &std::path::Path) -> Option<String> {
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

pub fn get_git_sync_status(path: &std::path::Path) -> (usize, usize) {
    let output = std::process::Command::new("git")
        .args(["rev-list", "--count", "--left-right", "HEAD...@{u}"])
        .current_dir(path)
        .output()
        .ok();

    if let Some(out) = output {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let parts: Vec<&str> = stdout.split_whitespace().collect();
            if parts.len() == 2 {
                let ahead = parts[0].parse().unwrap_or(0);
                let behind = parts[1].parse().unwrap_or(0);
                return (ahead, behind);
            }
        }
    }
    (0, 0)
}

pub fn get_git_history(path: &std::path::Path, limit: usize) -> Vec<crate::app::CommitInfo> {
    let mut cmd = std::process::Command::new("git");
    cmd.args([
        "log",
        &format!("-n{}", limit),
        "--pretty=format:COMMIT|%H|%an|%ct|%ad|%d|%s",
        "--shortstat",
    ]);

    if path.is_file() {
        if let Some(parent) = path.parent() {
            cmd.current_dir(parent);
            if let Some(filename) = path.file_name() {
                cmd.arg("--");
                cmd.arg(filename);
            }
        } else {
            cmd.current_dir(path);
        }
    } else {
        cmd.current_dir(path);
    }

    let output = cmd.output();

    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            parse_log_output(&stdout)
        }
        _ => Vec::new(),
    }
}

pub fn get_git_status(path: &std::path::Path) -> Vec<crate::app::GitStatus> {
    let output = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(path)
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout
                .lines()
                .filter(|l| l.len() > 3)
                .map(|l| crate::app::GitStatus {
                    status: l[..2].trim().to_string(),
                    path: l[3..].to_string(),
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

fn parse_log_output(stdout: &str) -> Vec<crate::app::CommitInfo> {
    let mut commits = Vec::new();
    let mut current_commit: Option<crate::app::CommitInfo> = None;

    for line in stdout.lines() {
        let line = line.trim();
        if line.starts_with("COMMIT|") {
            if let Some(c) = current_commit.take() {
                commits.push(c);
            }
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() >= 7 {
                let timestamp = parts[3].parse().unwrap_or(0);
                current_commit = Some(crate::app::CommitInfo {
                    hash: parts[1].to_string(),
                    author: parts[2].to_string(),
                    timestamp,
                    date: parts[4].to_string(),
                    refs: parts[5].trim().to_string(),
                    message: parts[6..].join("|"),
                    insertions: 0,
                    deletions: 0,
                    files_changed: 0,
                });
            }
        } else if let Some(ref mut c) = current_commit {
            if line.contains("changed") {
                let parts: Vec<&str> = line.split(',').collect();
                for part in parts {
                    let part = part.trim();
                    if part.contains("changed") {
                        if let Some(num_str) = part.split_whitespace().nth(0) {
                            c.files_changed = num_str.parse().unwrap_or(0);
                        }
                    } else if part.contains("insertion") {
                        if let Some(num_str) = part.split_whitespace().nth(0) {
                            c.insertions = num_str.parse().unwrap_or(0);
                        }
                    } else if part.contains("deletion") {
                        if let Some(num_str) = part.split_whitespace().nth(0) {
                            c.deletions = num_str.parse().unwrap_or(0);
                        }
                    }
                }
            }
        }
    }
    if let Some(c) = current_commit {
        commits.push(c);
    }
    commits
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
    terma::utils::copy_recursive(src, dst)
}

pub fn move_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    terma::utils::move_recursive(src, dst)
}

