#![allow(dead_code, unused)]
use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};
use std::sync::{LazyLock, RwLock};

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

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RgbColor {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub const fn to_color(self) -> Color {
        Color::Rgb(self.r, self.g, self.b)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThemeStyle {
    pub accent_primary: RgbColor,
    pub accent_secondary: RgbColor,
    pub selection_bg: RgbColor,
    pub border_active: RgbColor,
    pub border_inactive: RgbColor,
    pub header_fg: RgbColor,
}

impl ThemeStyle {
    pub fn preset_warm() -> Self {
        Self {
            accent_primary: RgbColor::new(224, 164, 90), // Warm amber
            accent_secondary: RgbColor::new(94, 199, 178), // Mint-teal
            selection_bg: RgbColor::new(178, 122, 64),   // Muted bronze
            border_active: RgbColor::new(224, 164, 90),  // Warm amber
            border_inactive: RgbColor::new(86, 88, 98),  // Neutral slate
            header_fg: RgbColor::new(240, 196, 138),     // Sand
        }
    }

    pub fn preset_cool() -> Self {
        Self {
            accent_primary: RgbColor::new(160, 118, 255),   // Purple
            accent_secondary: RgbColor::new(116, 184, 255), // Ice blue
            selection_bg: RgbColor::new(104, 80, 150),      // Deep violet
            border_active: RgbColor::new(160, 118, 255),    // Purple
            border_inactive: RgbColor::new(82, 86, 104),    // Cool slate
            header_fg: RgbColor::new(198, 164, 255),        // Soft violet
        }
    }

    pub fn default_purple() -> Self {
        Self::preset_warm()
    }

    fn apply_to_theme(&self, theme: &mut DraconTheme) {
        theme.accent_primary = self.accent_primary.to_color();
        theme.accent_secondary = self.accent_secondary.to_color();
        theme.selection_bg = self.selection_bg.to_color();
        theme.border_active = self.border_active.to_color();
        theme.border_inactive = self.border_inactive.to_color();
        theme.header_fg = self.header_fg.to_color();
    }
}

impl DraconTheme {
    pub fn cyberpunk() -> Self {
        Self {
            bg: Color::Rgb(0, 0, 0),                    // True Color Pure Black
            fg: Color::Rgb(255, 255, 255),              // Pure White
            accent_primary: Color::Rgb(224, 164, 90),   // Warm Amber
            accent_secondary: Color::Rgb(94, 199, 178), // Mint-Teal
            selection_bg: Color::Rgb(178, 122, 64),     // Muted Bronze
            selection_fg: Color::Rgb(0, 0, 0),          // Black (for contrast)
            border_active: Color::Rgb(224, 164, 90),    // Primary Accent
            border_inactive: Color::Rgb(86, 88, 98),    // Neutral Slate
            header_fg: Color::Rgb(240, 196, 138),       // Sand
            file_code: Color::Rgb(236, 156, 116),       // Apricot
            file_config: Color::Rgb(132, 190, 255),     // Sky Blue
            file_media: Color::Rgb(201, 156, 244),      // Lilac
            file_archive: Color::Rgb(238, 132, 170),    // Rose
            file_exec: Color::Rgb(118, 203, 125),       // Green
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

static ACTIVE_STYLE: LazyLock<RwLock<ThemeStyle>> =
    LazyLock::new(|| RwLock::new(ThemeStyle::default_purple()));
static ACTIVE_THEME: LazyLock<RwLock<DraconTheme>> = LazyLock::new(|| {
    let mut theme = DraconTheme::cyberpunk();
    ThemeStyle::default_purple().apply_to_theme(&mut theme);
    RwLock::new(theme)
});

pub fn style_settings() -> ThemeStyle {
    ACTIVE_STYLE.read().unwrap().clone()
}

pub fn set_style_settings(style: ThemeStyle) {
    {
        let mut active_style = ACTIVE_STYLE.write().unwrap();
        *active_style = style.clone();
    }
    {
        let mut active_theme = ACTIVE_THEME.write().unwrap();
        let mut theme = DraconTheme::cyberpunk();
        style.apply_to_theme(&mut theme);
        *active_theme = theme;
    }
}

pub fn accent_primary() -> Color {
    ACTIVE_THEME.read().unwrap().accent_primary
}

pub fn accent_secondary() -> Color {
    ACTIVE_THEME.read().unwrap().accent_secondary
}

pub fn selection_bg() -> Color {
    ACTIVE_THEME.read().unwrap().selection_bg
}

pub fn border_active() -> Color {
    ACTIVE_THEME.read().unwrap().border_active
}

pub fn border_inactive() -> Color {
    ACTIVE_THEME.read().unwrap().border_inactive
}

pub fn header_fg() -> Color {
    ACTIVE_THEME.read().unwrap().header_fg
}
