use git2::{Repository, Status};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct GitModule;

impl GitModule {
    pub fn get_repo_status(path: &Path) -> HashMap<PathBuf, String> {
        let mut status_map = HashMap::new();

        // Find repository root
        if let Ok(repo) = Repository::discover(path) {
            if let Ok(statuses) = repo.statuses(None) {
                for entry in statuses.iter() {
                    if let Some(path_str) = entry.path() {
                        let status = entry.status();
                        let status_code = convert_status(status);

                        // Git2 returns paths relative to repo root
                        // We need to map them to the full path if possible,
                        // or at least handle the mapping in files.rs

                        if let Some(workdir) = repo.workdir() {
                            let full_path = workdir.join(path_str);
                            status_map.insert(full_path, status_code);
                        }
                    }
                }
            }
        }

        status_map
    }
}

fn convert_status(s: Status) -> String {
    if s.contains(Status::INDEX_NEW) {
        return "A".to_string();
    }
    if s.contains(Status::INDEX_MODIFIED) {
        return "M".to_string();
    }
    if s.contains(Status::WT_MODIFIED) {
        return "M".to_string();
    }
    if s.contains(Status::WT_NEW) {
        return "??".to_string();
    }
    if s.contains(Status::IGNORED) {
        return "!!".to_string();
    }
    if s.contains(Status::CONFLICTED) {
        return "UU".to_string();
    }
    if s.contains(Status::INDEX_DELETED) || s.contains(Status::WT_DELETED) {
        return "D".to_string();
    }
    if s.contains(Status::INDEX_RENAMED) {
        return "R".to_string();
    }

    "".to_string()
}
