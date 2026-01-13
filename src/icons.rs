pub enum Icon {
    Folder,
    File,
    Star,
    Storage,
    Remote,
    Git,
    Archive,
    Image,
    Audio,
    Video,
    Script,
    Document,
    Search,
    Split,
    Single,
    Back,
    Forward,
    Burger,
    Refresh,
}

impl Icon {
    pub fn get(&self, mode: IconMode) -> &str {
        match mode {
            IconMode::Nerd => match self {
                Icon::Folder => "󰉋 ",
                Icon::File => "󰈔 ",
                Icon::Star => "󰓎 ",
                Icon::Storage => "󰋊 ",
                Icon::Remote => "󰒍 ",
                Icon::Git => "󰊢 ",
                Icon::Archive => "󰛫 ",
                Icon::Image => "󰸉 ",
                Icon::Audio => "󰝚 ",
                Icon::Video => "󰐊 ",
                Icon::Script => "󰞷 ",
                Icon::Document => "󰈙 ",
                Icon::Search => "󰍉 ",
                Icon::Split => "󰙀 ",
                Icon::Single => "󰇄 ",
                Icon::Back => "󰁍 ",
                Icon::Forward => "󰁔 ",
                Icon::Burger => "󰍜 ",
                Icon::Refresh => "󰑓 ",
            },
            IconMode::Unicode => match self {
                Icon::Folder => "▸ ",
                Icon::File => "▪ ",
                Icon::Star => "★ ",
                Icon::Storage => "⛁ ",
                Icon::Remote => "☁ ",
                Icon::Git => "± ",
                Icon::Archive => "⚑ ",
                Icon::Image => "画像 ", // or "IMG "
                Icon::Audio => "♪ ",
                Icon::Video => "► ",
                Icon::Script => "$ ",
                Icon::Document => "≡ ",
                Icon::Search => "🔍 ",
                Icon::Split => "|| ",
                Icon::Single => "[] ",
                Icon::Back => "← ",
                Icon::Forward => "→ ",
                Icon::Burger => "≡ ",
                Icon::Refresh => "↻ ",
            },
            IconMode::ASCII => match self {
                Icon::Folder => "[D] ",
                Icon::File => "[F] ",
                Icon::Star => "[*] ",
                Icon::Storage => "[S] ",
                Icon::Remote => "[R] ",
                Icon::Git => "[G] ",
                Icon::Archive => "[Z] ",
                Icon::Image => "[I] ",
                Icon::Audio => "[A] ",
                Icon::Video => "[V] ",
                Icon::Script => "[X] ",
                Icon::Document => "[T] ",
                Icon::Search => "/ ",
                Icon::Split => "[S] ",
                Icon::Single => "[1] ",
                Icon::Back => "< ",
                Icon::Forward => "> ",
                Icon::Burger => "[=] ",
                Icon::Refresh => "[R] ",
            },
        }
    }
}

pub fn guess_icon_mode() -> IconMode {
    let term = std::env::var("TERM").unwrap_or_default().to_lowercase();
    let term_program = std::env::var("TERM_PROGRAM").unwrap_or_default().to_lowercase();
    
    // Modern high-end terminals almost always have nerd fonts configured by their users
    if term.contains("kitty") || 
       term.contains("alacritty") || 
       term.contains("wezterm") || 
       term.contains("konsole") ||
       term_program.contains("vscode") ||
       term_program.contains("iterm") ||
       std::env::var("KONSOLE_VERSION").is_ok() {
        return IconMode::Nerd;
    }
    
    // Fallback to unicode if we have color support (implies modern terminal)
    if std::env::var("COLORTERM").is_ok() {
        return IconMode::Unicode;
    }

    IconMode::ASCII
}
