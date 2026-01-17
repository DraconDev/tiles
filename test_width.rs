use unicode_width::UnicodeWidthChar;

fn main() {
    let icons = ["󰉋", "󰈔", "󰛫", "󰸉", "󰝚", "󰐊", "󰞷", "󰈙"];
    for icon in icons {
        let c = icon.chars().next().unwrap();
        println!("Icon: {} (U+{:X}), Width: {:?}", icon, c as u32, c.width());
    }
}
