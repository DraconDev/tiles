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
            fg: Color::Rgb(220, 220, 230),             // Off-white
            accent_primary: Color::Rgb(255, 0, 85),    // Neon Red/Pink
            accent_secondary: Color::Rgb(0, 255, 200), // Cyan
            selection_bg: Color::Rgb(255, 0, 85),      // Neon Red/Pink
            selection_fg: Color::Rgb(0, 0, 0),         // Black (for contrast)
            border_active: Color::Rgb(255, 0, 85),     // Primary Accent
            border_inactive: Color::Rgb(60, 60, 70),   // Dim Grey
            header_fg: Color::Rgb(0, 255, 200),        // Secondary Accent
            file_code: Color::Rgb(255, 128, 0),        // Orange (Code)
            file_config: Color::Rgb(255, 215, 0),      // Gold (Config)
            file_media: Color::Rgb(138, 43, 226),      // Violet (Media)
            file_archive: Color::Rgb(255, 105, 180),   // Hot Pink (Archive)
            file_exec: Color::Rgb(50, 205, 50),        // Lime Green (Exec)
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
