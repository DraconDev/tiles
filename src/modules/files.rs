use crate::app::FileState;

pub fn update_files(state: &mut FileState) {
    // This now requires the session to be passed if remote.
    // In a real implementation, we'd pull the session from the App's active_sessions.
}

pub fn update_files_remote(state: &mut FileState, session: &ssh2::Session) {
    if let Ok(mut sftp) = session.sftp() {
        if let Ok(entries) = sftp.readdir(&state.current_path) {
            state.files = entries.into_iter().map(|(path, _stat)| path).collect();
            // Sorting logic similar to local
            state.files.sort_by(|a, b| {
                // Remote stat is expensive, so we might skip dir-first sorting for now 
                // or cache stats. For MVP, just name sort.
                a.file_name().cmp(&b.file_name())
            });
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