fn toggle_column(file_state: &mut crate::app::FileState, col: crate::app::FileColumn) {
    if file_state.columns.contains(&col) {
        file_state.columns.retain(|c| *c != col);
    } else {
        file_state.columns.push(col);
    }
}