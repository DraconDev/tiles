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
}

impl DraconTheme {
    pub fn cyberpunk() -> Self {
        Self {
            bg: Color::Rgb(10, 10, 15), // Deep dark blue/black
            fg: Color::Rgb(220, 220, 230), // Off-white
            accent_primary: Color::Rgb(255, 0, 85), // Neon Red/Pink
            accent_secondary: Color::Rgb(0, 255, 200), // Cyan
            selection_bg: Color::Rgb(40, 40, 50), // Dark Grey
            selection_fg: Color::Rgb(255, 255, 255), // White
            border_active: Color::Rgb(255, 0, 85), // Primary Accent
            border_inactive: Color::Rgb(60, 60, 70), // Dim Grey
            header_fg: Color::Rgb(0, 255, 200), // Secondary Accent
        }
    }

    pub fn block_active(&self) -> Style {
        Style::default().fg(self.border_active).bg(self.bg)
    }

    pub fn block_inactive(&self) -> Style {
        Style::default().fg(self.border_inactive).bg(self.bg)
    }

    pub fn text_highlight(&self) -> Style {
        Style::default().fg(self.accent_primary).add_modifier(Modifier::BOLD)
    }
}

pub static THEME: std::sync::LazyLock<DraconTheme> = std::sync::LazyLock::new(DraconTheme::cyberpunk);