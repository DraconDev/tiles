use crate::app::{CommitInfo, FileMetadata, GitPendingChange};
use dracon_files::{
    DirectoryCatalogContract, FileCategory as LibFileCategory, FileCopyContract,
    FileInspectContract, FileSearchContract, FsCatalog,
};
use dracon_git::{CliGitSnapshotProvider, GitPreviewContract, GitSnapshotContract};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
    let catalog = FsCatalog;
    match catalog.read_dir_with_metadata(path) {
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
