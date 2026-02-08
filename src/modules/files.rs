use crate::app::FileMetadata;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn parse_git_shortstat(line: &str) -> (usize, usize, usize) {
    let mut files_changed = 0usize;
    let mut insertions = 0usize;
    let mut deletions = 0usize;

    for segment in line.split(',').map(str::trim) {
        let num = segment
            .split_whitespace()
            .next()
            .and_then(|s| s.parse::<usize>().ok());
        let Some(value) = num else {
            continue;
        };

        if segment.contains("file changed") || segment.contains("files changed") {
            files_changed = value;
        } else if segment.contains("insertion") {
            insertions = value;
        } else if segment.contains("deletion") {
            deletions = value;
        }
    }

    (files_changed, insertions, deletions)
}

fn parse_git_log_record(line: &str) -> Option<crate::app::CommitInfo> {
    let parts: Vec<&str> = line.split('\x1f').collect();
    if parts.len() < 5 {
        return None;
    }

    Some(crate::app::CommitInfo {
        hash: parts[0].to_string(),
        author: parts[1].to_string(),
        date: parts[2].to_string(),
        message: parts[3].to_string(),
        decorations: parts[4].to_string(),
        files_changed: 0,
        insertions: 0,
        deletions: 0,
    })
}

fn metadata_permissions(m: &std::fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return m.permissions().mode();
    }
    #[cfg(not(unix))]
    {
        let _ = m;
        0
    }
}

pub fn read_dir_with_metadata(path: &Path) -> (Vec<PathBuf>, HashMap<PathBuf, FileMetadata>) {
    let mut files = Vec::new();
    let mut metadata = HashMap::new();

    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.filter_map(|e| e.ok()) {
            if let Ok(p) = std::fs::canonicalize(entry.path()) {
                if let Ok(m) = entry.metadata() {
                    let meta = FileMetadata {
                        size: m.len(),
                        modified: m.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                        created: m.created().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                        permissions: metadata_permissions(&m),
                        is_dir: m.is_dir(),
                    };
                    files.push(p.clone());
                    metadata.insert(p, meta);
                }
            }
        }
    }

    (files, metadata)
}

pub fn get_file_category(path: &Path) -> crate::app::FileCategory {
    terma::utils::get_file_category(path)
}

pub fn fetch_git_data(
    path: &Path,
) -> Option<(
    Vec<crate::app::CommitInfo>,
    Vec<crate::app::GitPendingChange>,
    String,
    usize,
    usize,
    String,
    Vec<String>, // Remotes
    Vec<String>, // Stashes
)> {
    let output = std::process::Command::new("git")
        .args(&["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Ahead/Behind
    let (ahead, behind) = if let Ok(out) = std::process::Command::new("git")
        .args(&["rev-list", "--left-right", "--count", "HEAD...@{u}"])
        .current_dir(path)
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout);
            let parts: Vec<&str> = s.split_whitespace().collect();
            if parts.len() == 2 {
                (parts[0].parse().unwrap_or(0), parts[1].parse().unwrap_or(0))
            } else {
                (0, 0)
            }
        } else {
            (0, 0)
        }
    } else {
        (0, 0)
    };

    // Log with Graph
    let mut history = Vec::new();
    if let Ok(out) = std::process::Command::new("git")
        .args(&[
            "--no-pager",
            "log",
            "-n",
            "100",
            "--pretty=format:%H%x1f%an%x1f%ar%x1f%s%x1f%d",
            "--shortstat",
        ])
        .current_dir(path)
        .output()
    {
        let out_str = String::from_utf8_lossy(&out.stdout);
        let mut current_commit: Option<crate::app::CommitInfo> = None;

        for line in out_str.lines() {
            let line = line.trim();
            if let Some(parsed) = parse_git_log_record(line) {
                if let Some(c) = current_commit.take() {
                    history.push(c);
                }
                current_commit = Some(parsed);
            } else if let Some(ref mut c) = current_commit {
                if line.contains("changed") {
                    let (files_changed, insertions, deletions) = parse_git_shortstat(line);
                    c.files_changed = files_changed;
                    c.insertions = insertions;
                    c.deletions = deletions;
                }
            }
        }
        if let Some(c) = current_commit {
            history.push(c);
        }
    }

    // Status & Detailed Stats
    let mut pending = Vec::new();
    let mut stats_map = HashMap::new();

    if let Ok(out) = std::process::Command::new("git")
        .args(&["diff", "--numstat"])
        .current_dir(path)
        .output()
    {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let ins = parts[0].parse::<usize>().unwrap_or(0);
                let del = parts[1].parse::<usize>().unwrap_or(0);
                let file = parts[2].to_string();
                stats_map.insert(file, (ins, del));
            }
        }
    }
    if let Ok(out) = std::process::Command::new("git")
        .args(&["diff", "--staged", "--numstat"])
        .current_dir(path)
        .output()
    {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let ins = parts[0].parse::<usize>().unwrap_or(0);
                let del = parts[1].parse::<usize>().unwrap_or(0);
                let file = parts[2].to_string();
                let entry = stats_map.entry(file).or_insert((0, 0));
                entry.0 += ins;
                entry.1 += del;
            }
        }
    }

    if let Ok(out) = std::process::Command::new("git")
        .args(&["status", "--porcelain"])
        .current_dir(path)
        .output()
    {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            if line.len() > 3 {
                let status = line[0..2].trim().to_string();
                let file = line[3..].to_string();
                let (ins, del) = stats_map.get(&file).cloned().unwrap_or((0, 0));
                pending.push(crate::app::GitPendingChange {
                    status,
                    path: file,
                    insertions: ins,
                    deletions: del,
                });
            }
        }
    }

    let summary = if let Ok(out) = std::process::Command::new("git")
        .args(&["diff", "HEAD", "--shortstat"])
        .current_dir(path)
        .output()
    {
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    } else {
        String::new()
    };

    let remotes = if let Ok(out) = std::process::Command::new("git")
        .args(&["remote", "-v"])
        .current_dir(path)
        .output()
    {
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|s| s.to_string())
            .collect()
    } else {
        Vec::new()
    };

    let stashes = if let Ok(out) = std::process::Command::new("git")
        .args(&["stash", "list"])
        .current_dir(path)
        .output()
    {
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|s| s.to_string())
            .collect()
    } else {
        Vec::new()
    };

    Some((
        history,
        pending,
        branch,
        ahead,
        behind,
        summary,
        remotes,
        stashes,
    ))
}

#[cfg(test)]
mod tests {
    #[test]
    fn parse_shortstat_like_git_formats() {
        assert_eq!(
            super::parse_git_shortstat("1 file changed, 5 insertions(+), 2 deletions(-)"),
            (1, 5, 2)
        );
        assert_eq!(
            super::parse_git_shortstat("3 files changed, 27 insertions(+)"),
            (3, 27, 0)
        );
        assert_eq!(super::parse_git_shortstat("1 file changed"), (1, 0, 0));
    }

    #[test]
    fn parse_log_record_with_pipe_in_subject() {
        let line =
            "abcdef123456\x1fAlice\x1f2 days ago\x1ffeat: a | b | c\x1f (HEAD -> main, origin/main)";
        let record = super::parse_git_log_record(line).expect("record should parse");
        assert_eq!(record.hash, "abcdef123456");
        assert_eq!(record.author, "Alice");
        assert_eq!(record.date, "2 days ago");
        assert_eq!(record.message, "feat: a | b | c");
        assert_eq!(record.decorations, " (HEAD -> main, origin/main)");
    }
}

pub fn global_search(
    root: &Path,
    query: &str,
) -> (Vec<PathBuf>, HashMap<PathBuf, FileMetadata>) {
    let mut results = Vec::new();
    let mut metadata = HashMap::new();
    let query_lower = query.trim().to_lowercase();
    if query_lower.is_empty() {
        return (results, metadata);
    }

    let mut stack = vec![root.to_path_buf()];
    let max_results = 100;

    while let Some(current_dir) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&current_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let p = entry.path();
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");

                if name.to_lowercase().contains(&query_lower) {
                    if let Ok(m) = entry.metadata() {
                        let meta = FileMetadata {
                            size: m.len(),
                            modified: m.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                            created: m.created().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                            permissions: metadata_permissions(&m),
                            is_dir: m.is_dir(),
                        };
                        let abs_p = p.canonicalize().unwrap_or(p.clone());
                        results.push(abs_p.clone());
                        metadata.insert(abs_p, meta);

                        if results.len() >= max_results {
                            return (results, metadata);
                        }
                    }
                }

                if p.is_dir() {
                    // Avoid large/system/uninteresting dirs for performance
                    let name_lower = name.to_lowercase();
                    if name != "target" && name != ".git" && name != "node_modules" 
                        && name != "Library" && name != ".cache" && name != ".cargo"
                        && name_lower != "pictures" && name_lower != "videos" && name_lower != "music"
                    {
                        stack.push(p);
                    }
                }
            }
        }
    }

    (results, metadata)
}

pub fn copy_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    terma::utils::copy_recursive(src, dst)
}
