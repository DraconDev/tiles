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
            accent_primary: Color::Rgb(0, 150, 255),    // GitHub Blue
            accent_secondary: Color::Rgb(0, 255, 150), // Mint Green
            selection_bg: Color::Rgb(0, 150, 255),      // GitHub Blue
            selection_fg: Color::Rgb(0, 0, 0),         // Black
            border_active: Color::Rgb(0, 150, 255),     // Blue
            border_inactive: Color::Rgb(40, 40, 50),   // Dark Grey
            header_fg: Color::Rgb(0, 255, 150),        // Green
            file_code: Color::Rgb(255, 128, 0),        // Orange
            file_config: Color::Rgb(255, 215, 0),      // Gold
            file_media: Color::Rgb(180, 50, 255),      // Purple
            file_archive: Color::Rgb(255, 50, 80),     // Red
            file_exec: Color::Rgb(0, 255, 100),        // Matrix Green
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
