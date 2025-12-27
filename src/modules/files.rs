use crate::app::FileState;

pub fn update_files(state: &mut FileState) {
    if let Ok(entries) = std::fs::read_dir(&state.current_path) {
        state.files = entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
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
    }
}