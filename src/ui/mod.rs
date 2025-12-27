use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph},
    Frame,
};

use crate::app::{App, AppMode, CurrentView, LicenseStatus};

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(3), // Tabs + Footer
        ])
        .split(f.area());

    let workspace = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20), // Sidebar
            Constraint::Min(0), // Main Stage
        ])
        .split(chunks[0]);

    draw_sidebar(f, workspace[0], app);
    draw_main_stage(f, workspace[1], app);
    
    draw_bottom_bar(f, chunks[1], app);

    if matches!(app.mode, AppMode::CommandPalette) {
        draw_command_palette(f, app);
    }
    
    // Modals
    if matches!(app.mode, AppMode::Rename) {
        draw_rename_modal(f, app);
    }
    if matches!(app.mode, AppMode::Delete) {
        draw_delete_modal(f, app);
    }
    if matches!(app.mode, AppMode::Properties) {
        draw_properties_modal(f, app);
    }
    if matches!(app.mode, AppMode::NewFolder) {
        draw_new_folder_modal(f, app);
    }
}

fn draw_bottom_bar(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Tabs
            Constraint::Length(1), // Hints
            Constraint::Length(1), // Spacing/Border
        ])
        .split(area);
        
    draw_tabs(f, chunks[0], app);
    draw_footer(f, chunks[1], app);
}

fn draw_tabs(f: &mut Frame, area: Rect, app: &App) {
    let tabs = vec![" [F]iles ", " [C]onsole ", " [P]rocesses ", " [D]ocker "];
    let mode_str = match app.current_view {
        CurrentView::Files => " [F]iles ",
        CurrentView::System => " [P]rocesses ",
        CurrentView::Docker => " [D]ocker ",
    };
    
    let mut spans = Vec::new();
    for tab in tabs {
        let style = if tab == mode_str || (tab == " [C]onsole " && matches!(app.mode, AppMode::CommandPalette)) {
            Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        spans.push(ratatui::text::Span::styled(tab, style));
        spans.push(ratatui::text::Span::raw(" "));
    }
    
    let p = Paragraph::new(ratatui::text::Line::from(spans));
    f.render_widget(p, area);
}

fn draw_sidebar(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Sidebar ")
        .border_style(if app.sidebar_focus && app.current_view == CurrentView::Files { Style::default().fg(Color::Cyan) } else { Style::default() });
    
    f.render_widget(block, area);
    
    let inner = area.inner(ratatui::layout::Margin { vertical: 1, horizontal: 1 });

    match app.current_view {
        CurrentView::Files => {
            let sidebar_items = vec!["Home", "Downloads", "Documents", "Pictures"];
            let items: Vec<ListItem> = sidebar_items.iter().enumerate().map(|(i, name)| {
                let style = if i == app.sidebar_index && app.sidebar_focus {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(*name).style(style)
            }).collect();
            f.render_widget(List::new(items), inner);
        },
        CurrentView::Docker => {
             let items = vec![
                ListItem::new("Containers").style(Style::default().add_modifier(Modifier::BOLD)),
                ListItem::new("  All"),
                ListItem::new("  Running").style(Style::default().fg(Color::Green)),
                ListItem::new("  Stopped").style(Style::default().fg(Color::Red)),
                ListItem::new(""),
                ListItem::new("Images"),
                ListItem::new("Volumes"),
                ListItem::new("Networks"),
            ];
            f.render_widget(List::new(items), inner);
        },
        CurrentView::System => {
             let items = vec![
                ListItem::new("Overview").style(Style::default().add_modifier(Modifier::BOLD)),
                ListItem::new("Processes"),
                ListItem::new("Disks"),
                ListItem::new("Network"),
            ];
            f.render_widget(List::new(items), inner);
        }
    }
}

fn draw_main_stage(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {:?} ", app.current_view))
        .border_style(if !app.sidebar_focus { Style::default().fg(Color::Cyan) } else { Style::default() });
    
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.current_view == CurrentView::Files {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Path Bar
                Constraint::Min(0),    // File List
            ])
            .split(inner);

        let path_text = if matches!(app.mode, AppMode::Location) {
            format!("Location: {}", app.input)
        } else {
            format!("Path: {}", app.file_state.current_path.display())
        };

        let path_bar = Paragraph::new(path_text)
            .block(Block::default().borders(Borders::ALL).border_style(
                if matches!(app.mode, AppMode::Location) { Style::default().fg(Color::Yellow) } else { Style::default() }
            ));
        f.render_widget(path_bar, chunks[0]);

        draw_file_view(f, chunks[1], app);
    } else {
        match app.current_view {
            CurrentView::System => draw_system_view(f, inner, app),
            CurrentView::Docker => draw_docker_view(f, inner, app),
            _ => {}
        }
    }
}

fn draw_file_view(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app.file_state.files.iter().enumerate().map(|(i, path)| {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("..");
        let mut display_name = name.to_string();

        let mut style = if path.is_dir() {
            Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        
        if let Some(status) = app.file_state.git_status.get(path) {
            display_name.push_str(&format!(" [{}]",