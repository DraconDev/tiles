use std::fs;
use std::path::PathBuf;
use crate::app::LicenseStatus;

pub fn check_license() -> LicenseStatus {
    let mut path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push(".config/tiles/license.key");

    if let Ok(content) = fs::read_to_string(path) {
        let content = content.trim();
        if !content.is_empty() {
            return LicenseStatus::Commercial(content.to_string());
        }
    }

    LicenseStatus::FreeMode
}