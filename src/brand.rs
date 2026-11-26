//! Centralized product identity for COVALOS.
//!
//! All user-facing product names, window metadata, and the provisional brand icon live here
//! so updating the final approved isotype requires replacing `assets/brand/covalos-mark.svg`
//! and regenerating the derived assets via `tools/generate_brand_assets.py`.
use eframe::egui::IconData;
use std::sync::Arc;

pub const NAME: &str = "Covalos";
pub const TITLE: &str = "Covalos";
pub const DESCRIPTION: &str = "Native Rust IDE with multi-agent orchestration";
/// Internal build version (Cargo, Windows file version, installer). It keeps
/// increasing independently of the public release name.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Public release name shown to people. See docs/RELEASE.md.
pub const RELEASE: &str = "Beta 0.8.9";
pub const USER_AGENT: &str = "COVALOS-Library-Studio";

pub const APP_ICON_PNG: &[u8] = include_bytes!("../assets/brand/covalos-app-256.png");
pub const MARK_PNG: &[u8] = include_bytes!("../assets/brand/covalos-mark-256.png");
pub const MARK_MONO_PNG: &[u8] = include_bytes!("../assets/brand/covalos-mark-mono-64.png");

/// Decode the embedded 256x256 RGBA8 application icon for the OS window and taskbar.
pub fn window_icon() -> Arc<IconData> {
    let decoder = png::Decoder::new(std::io::Cursor::new(APP_ICON_PNG));
    let mut reader = decoder.read_info().expect("valid COVALOS app icon PNG");
    let mut rgba = vec![0; reader.output_buffer_size().expect("bounded icon buffer")];
    let info = reader
        .next_frame(&mut rgba)
        .expect("decodable COVALOS app icon");
    rgba.truncate(info.buffer_size());
    Arc::new(IconData {
        rgba,
        width: info.width,
        height: info.height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_window_icon_decodes_to_rgba8_square() {
        let icon = window_icon();
        assert_eq!(icon.width, 256);
        assert_eq!(icon.height, 256);
        assert_eq!(icon.rgba.len(), (icon.width * icon.height * 4) as usize);
    }

    /// The release workflow tags `v<Cargo version>`, so the public name must carry the
    /// same number or About and the top bar show the previous release.
    #[test]
    fn public_release_name_matches_the_build_version() {
        assert!(
            RELEASE.ends_with(VERSION),
            "brand::RELEASE ({RELEASE}) does not end with the Cargo version ({VERSION})"
        );
    }
}
