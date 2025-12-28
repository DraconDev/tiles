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
        state.files = entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                if state.show_hidden {
                    true
                } else {
                    !path.file_name()
                        .and_then(|n| n.to_str())
                        .map(|s| s.starts_with('.'))
                        .unwrap_or(false)
                }
            })
            .filter(|path| {
                if state.search_filter.is_empty() {
                    true
                } else {
                    path.file_name()
                        .and_then(|n| n.to_str())
                        .map(|s| s.to_lowercase().contains(&state.search_filter.to_lowercase()))
                        .unwrap_or(false)
                }
            })
            .collect();
        state.files.sort_by(|a, b| {
            let a_is_dir = a.is_dir();
            let b_is_dir = b.is_dir();
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
            state.files = entries
                .into_iter()
                .map(|(path, _stat)| path)
                .filter(|path| {
                    if state.show_hidden {
                        true
                    } else {
                        !path.file_name()
                            .and_then(|n| n.to_str())
                            .map(|s| s.starts_with('.'))
                            .unwrap_or(false)
                    }
                })
                .filter(|path| {
                    if state.search_filter.is_empty() {
                        true
                    } else {
                        path.file_name()
                            .and_then(|n| n.to_str())
                            .map(|s| s.to_lowercase().contains(&state.search_filter.to_lowercase()))
                            .unwrap_or(false)
                    }
                })
                .collect();
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