use unicode_width::UnicodeWidthStr;

fn main() {
    let icons = [
        ("Nerd Folder", "󰉋 "),
        ("Nerd File", "󰈔 "),
        ("Unicode Folder", "▸ "),
        ("Unicode File", "▪ "),
    ];

    for (name, s) in icons {
        println!("{}: '{}' width={}", name, s, s.width());
    }
}
