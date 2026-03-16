use crate::app::{CommitInfo, FileMetadata, GitPendingChange};
use dracon_files::{
    FileCategory as LibFileCategory, FileCopyContract, FileInspectContract, FileSearchContract,
    FsCatalog,
};
use dracon_git::{CliGitSnapshotProvider, GitPreviewContract, GitSnapshotContract};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Git data: (history, pending, branch, ahead, behind, summary, remotes, stashes)
type GitData = (
    Vec<CommitInfo>,
    Vec<GitPendingChange>,
    String,
    usize,
    usize,
    String,
    Vec<String>,
    Vec<String>,
);

fn map_metadata(meta: dracon_files::contracts::EntryMetadata) -> FileMetadata {
    FileMetadata {
        size: meta.size,
        modified: meta.modified,
        created: meta.created,
        permissions: meta.permissions,
        is_dir: meta.is_dir,
    }
}

pub fn read_dir_with_metadata(path: &Path) -> (Vec<PathBuf>, HashMap<PathBuf, FileMetadata>) {
    let mut files = Vec::new();
    let mut metadata = HashMap::new();

    let Ok(entries) = std::fs::read_dir(path) else {
        return (files, metadata);
    };

    for entry in entries.flatten() {
        let p = entry.path();
        let symlink_meta = std::fs::symlink_metadata(&p).ok();
        let target_meta = std::fs::metadata(&p).ok();
        let meta = target_meta.as_ref().or(symlink_meta.as_ref());

        files.push(p.clone());

        if let Some(m) = meta {
            let is_dir = target_meta
                .as_ref()
                .map(|tm| tm.is_dir())
                .or_else(|| symlink_meta.as_ref().map(|sm| sm.file_type().is_dir()))
                .unwrap_or(false);
            metadata.insert(
                p,
                FileMetadata {
                    size: m.len(),
                    modified: m.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                    created: m.created().unwrap_or(SystemTime::UNIX_EPOCH),
                    permissions: permissions_bits(m),
                    is_dir,
                },
            );
        }
    }

    (files, metadata)
}

fn permissions_bits(meta: &std::fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode()
    }
    #[cfg(not(unix))]
    {
        if meta.permissions().readonly() {
            0o444
        } else {
            0o666
        }
    }
}

pub fn get_file_category(path: &Path) -> crate::app::FileCategory {
    let catalog = FsCatalog;
    match catalog.get_file_category(path) {
        LibFileCategory::Archive => crate::app::FileCategory::Archive,
        LibFileCategory::Image => crate::app::FileCategory::Image,
        LibFileCategory::Script => crate::app::FileCategory::Script,
        LibFileCategory::Text => crate::app::FileCategory::Text,
        LibFileCategory::Document => crate::app::FileCategory::Document,
        LibFileCategory::Audio => crate::app::FileCategory::Audio,
        LibFileCategory::Video => crate::app::FileCategory::Video,
        LibFileCategory::Other => crate::app::FileCategory::Other,
    }
}

pub fn fetch_git_data(
    path: &Path,
) -> Option<(
    Vec<CommitInfo>,
    Vec<GitPendingChange>,
    String,
    usize,
    usize,
    String,
    Vec<String>,
    Vec<String>,
)> {
    let provider = CliGitSnapshotProvider;
    let snapshot = provider.fetch_snapshot(path).ok().flatten()?;

    let history = snapshot
        .history
        .into_iter()
        .map(|c| CommitInfo {
            hash: c.hash,
            author: c.author,
            date: c.date,
            message: c.message,
            decorations: c.decorations,
            files_changed: c.files_changed,
            insertions: c.insertions,
            deletions: c.deletions,
        })
        .collect();

    let pending = snapshot
        .pending
        .into_iter()
        .map(|p| GitPendingChange {
            status: p.status,
            path: p.path,
            insertions: p.insertions,
            deletions: p.deletions,
        })
        .collect();

    Some((
        history,
        pending,
        snapshot.branch,
        snapshot.ahead,
        snapshot.behind,
        snapshot.summary,
        snapshot.remotes,
        snapshot.stashes,
    ))
}

pub fn global_search(root: &Path, query: &str) -> (Vec<PathBuf>, HashMap<PathBuf, FileMetadata>) {
    let catalog = FsCatalog;
    match catalog.global_search(root, query) {
        Ok((files, metadata)) => (
            files,
            metadata
                .into_iter()
                .map(|(k, v)| (k, map_metadata(v)))
                .collect(),
        ),
        Err(_) => (Vec::new(), HashMap::new()),
    }
}

pub fn copy_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    let catalog = FsCatalog;
    catalog.copy_recursive(src, dst)
}

pub fn check_file_suitability(path: &Path, max_bytes: u64) -> (bool, bool, u64) {
    let catalog = FsCatalog;
    let s = catalog.check_file_suitability(path, max_bytes);
    (s.is_binary, s.is_too_large, s.size_mb)
}

pub fn show_commit_patch(repo_path: &Path, hash: &str) -> std::io::Result<String> {
    let provider = CliGitSnapshotProvider;
    provider.show_commit_patch(repo_path, hash)
}

pub fn show_file_diff(repo_path: &Path, file_path: &str) -> std::io::Result<String> {
    let provider = CliGitSnapshotProvider;
    provider.show_file_diff(repo_path, file_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn read_dir_includes_symlink_entries() {
        use std::os::unix::fs::symlink;
        use std::time::{SystemTime, UNIX_EPOCH};

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("tiles-symlink-test-{unique}"));
        let target = root.join("real_ssh_dir");
        std::fs::create_dir_all(&target).expect("create target dir");
        let link = root.join(".ssh");
        symlink(&target, &link).expect("create symlink");

        let (files, metadata) = read_dir_with_metadata(&root);
        assert!(files.iter().any(|p| p == &link), "symlink should be listed");
        assert!(metadata.contains_key(&link), "symlink should have metadata");
        assert_eq!(
            metadata.get(&link).map(|m| m.is_dir),
            Some(true),
            "symlink to dir should behave as directory"
        );

        let _ = std::fs::remove_file(&link);
        let _ = std::fs::remove_dir_all(&root);
    }
}
