#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_scroll_logic() {
        // Setup
        let mut fs = FileState {
            current_path: PathBuf::from("/"),
            remote_session: None,
            selected_index: None,
            table_state: ratatui::widgets::TableState::default(),
            files: (0..100).map(|i| PathBuf::from(format!("/file_{}", i))).collect(),
            metadata: std::collections::HashMap::new(),
            show_hidden: false,
            git_status: std::collections::HashMap::new(),
            clipboard: None,
            search_filter: String::new(),
            starred: std::collections::HashSet::new(),
            columns: vec![],
            history: vec![],
            history_index: 0,
            view_height: 20, // Mock height
        };

        // Standard Logic: Stop at bottom
        // Capacity = 20 - 2 = 18.
        // Max Offset = 100 - 18 = 82.
        
        let capacity = fs.view_height.saturating_sub(2);
        let max_offset = fs.files.len().saturating_sub(capacity);
        
        assert_eq!(capacity, 18);
        assert_eq!(max_offset, 82);

        // Scroll Down 1 (offset 0 -> 3)
        let new_offset = (fs.table_state.offset() + 3).min(max_offset);
        *fs.table_state.offset_mut() = new_offset;
        assert_eq!(fs.table_state.offset(), 3);

        // Scroll Down many times
        for _ in 0..30 {
            let n = (fs.table_state.offset() + 3).min(max_offset);
            *fs.table_state.offset_mut() = n;
        }
        
        // Should be capped at 82
        assert_eq!(fs.table_state.offset(), 82);
        
        // Scroll Up
        let n_up = fs.table_state.offset().saturating_sub(3);
        *fs.table_state.offset_mut() = n_up;
        assert_eq!(fs.table_state.offset(), 79);
    }

    #[test]
    fn test_scroll_logic_small_files() {
        // Setup: Files fit in view
        let mut fs = FileState {
            current_path: PathBuf::from("/"),
            remote_session: None,
            selected_index: None,
            table_state: ratatui::widgets::TableState::default(),
            files: (0..10).map(|i| PathBuf::from(format!("/file_{}", i))).collect(),
            metadata: std::collections::HashMap::new(),
            show_hidden: false,
            git_status: std::collections::HashMap::new(),
            clipboard: None,
            search_filter: String::new(),
            starred: std::collections::HashSet::new(),
            columns: vec![],
            history: vec![],
            history_index: 0,
            view_height: 20, 
        };

        // Capacity = 18.
        // Files = 10.
        // Max Offset = 10 - 18 = 0 (saturating).

        let capacity = fs.view_height.saturating_sub(2);
        let max_offset = fs.files.len().saturating_sub(capacity);
        
        assert_eq!(max_offset, 0);

        // Scroll Down
        let new_offset = (fs.table_state.offset() + 3).min(max_offset);
        *fs.table_state.offset_mut() = new_offset;
        assert_eq!(fs.table_state.offset(), 0); // Should stay 0
    }
}
