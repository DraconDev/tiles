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
            if !state.show_hidden && name.starts_with('.') { continue; }
            if !state.search_filter.is_empty() && !name.to_lowercase().contains(&state.search_filter.to_lowercase()) { continue; }

            // Metadata Cache (The Fix)
            if let Ok(m) = entry.metadata() {
                let mut meta = crate::app::FileMetadata {
                    size: m.len(),
                    modified: m.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                    created: m.created().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                    #[cfg(unix)]
                    permissions: { use std::os::unix::fs::PermissionsExt; m.permissions().mode() },
                    #[cfg(not(unix))]
                    permissions: 0,
                    extension: path.extension().and_then(|e| e.to_str()).unwrap_or("").to_string(),
                    is_dir: m.is_dir(),
                };
                state.metadata.insert(path.clone(), meta);
            }
            state.files.push(path);
        }

        state.files.sort_by(|a, b| {
            let a_is_dir = state.metadata.get(a).map(|m| m.is_dir).unwrap_or(false);
            let b_is_dir = state.metadata.get(b).map(|m| m.is_dir).unwrap_or(false);
            if a_is_dir && !b_is_dir {
                std::cmp::Ordering::Less
            } else if !a_is_dir && b_is_dir {
                std::cmp::Ordering::Greater
            } else {
                a.file_name().cmp(&b.file_name())
            }
        });

        // Git Integration
        state.git_status.clear();
        if let Ok(output) = std::process::Command::new("git")
            .args(&["status", "--porcelain"])
            .current_dir(&state.current_path)
            .output() 
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.len() < 4 { continue; }
                let status = line[0..2].trim();
                let relative_path = &line[3..];
                
                let path_buf = std::path::PathBuf::from(relative_path);
                if let Some(std::path::Component::Normal(first_component)) = path_buf.components().next() {
                     let full_path = state.current_path.join(first_component);
                     state.git_status.insert(full_path, status.to_string());
                }
            }
        }
    }
}

fn update_remote_files(state: &mut FileState, session: &ssh2::Session) {
    if let Ok(sftp) = session.sftp() {
        if let Ok(entries) = sftp.readdir(&state.current_path) {
            state.files.clear();
            state.metadata.clear();

            for (path, stat) in entries {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !state.show_hidden && name.starts_with('.') { continue; }
                if !state.search_filter.is_empty() && !name.to_lowercase().contains(&state.search_filter.to_lowercase()) { continue; }

                let meta = crate::app::FileMetadata {
                    size: stat.size.unwrap_or(0),
                    modified: stat.mtime.map(|t| std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(t)).unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                    created: std::time::SystemTime::UNIX_EPOCH, // SFTP usually doesn't provide birth time easily
                    permissions: stat.permissions.unwrap_or(0),
                    extension: path.extension().and_then(|e| e.to_str()).unwrap_or("").to_string(),
                    is_dir: stat.is_dir(),
                };
                state.metadata.insert(path.clone(), meta);
                state.files.push(path);
            }
            state.files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
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