#![allow(dead_code, unused)]
use ratatui::style::{Color, Modifier, Style};

pub struct DraconTheme {
    pub bg: Color,
    pub fg: Color,
    pub accent_primary: Color,
    pub accent_secondary: Color,
    pub selection_bg: Color,
    pub selection_fg: Color,
    pub border_active: Color,
    pub border_inactive: Color,
    pub header_fg: Color,
    pub file_code: Color,
    pub file_config: Color,
    pub file_media: Color,
    pub file_archive: Color,
    pub file_exec: Color,
}

impl DraconTheme {
    pub fn cyberpunk() -> Self {
        Self {
            bg: Color::Rgb(0, 0, 0),                   // True Color Pure Black
            fg: Color::Rgb(255, 255, 255),             // Pure White
            accent_primary: Color::Rgb(168, 118, 255), // Focus Purple
            accent_secondary: Color::Rgb(120, 186, 255), // Secondary Ice Blue
            selection_bg: Color::Rgb(168, 118, 255),   // Focus Purple
            selection_fg: Color::Rgb(0, 0, 0),         // Black (for contrast)
            border_active: Color::Rgb(168, 118, 255),  // Primary Accent
            border_inactive: Color::Rgb(92, 94, 104),  // Dark Grey
            header_fg: Color::Rgb(196, 160, 255),      // Soft Violet
            file_code: Color::Rgb(176, 152, 230),      // Muted Purple
            file_config: Color::Rgb(132, 186, 255),    // Soft Blue
            file_media: Color::Rgb(200, 138, 255),     // Violet
            file_archive: Color::Rgb(228, 140, 210),   // Pink-Violet
            file_exec: Color::Rgb(116, 198, 130),      // Green
        }
    }

    pub fn block_active(&self) -> Style {
        Style::default().fg(self.border_active).bg(self.bg)
    }

    pub fn block_inactive(&self) -> Style {
        Style::default().fg(self.border_inactive).bg(self.bg)
    }

    pub fn text_highlight(&self) -> Style {
        Style::default()
            .fg(self.accent_primary)
            .add_modifier(Modifier::BOLD)
    }
}

pub static THEME: std::sync::LazyLock<DraconTheme> =
    std::sync::LazyLock::new(DraconTheme::cyberpunk);
