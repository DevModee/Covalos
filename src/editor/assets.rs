//! Embedded, pre-rasterized upstream icons. Decode once per egui context, never per frame.
use eframe::egui::{self, Color32, Context, Rect, TextureHandle, TextureOptions};
use std::path::Path;

pub(super) fn file_name(path: &Path) -> &'static str {
    let name = path
        .file_name()
        .and_then(|p| p.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match name.as_str() {
        "cargo.toml" | "cargo.lock" | "rust-toolchain.toml" => return "file_type_rust",
        "package.json" | "package-lock.json" | "npm-shrinkwrap.json" => return "file_type_npm",
        "dockerfile" | "docker-compose.yml" | "compose.yaml" => return "file_type_docker",
        ".gitignore" | ".gitattributes" | ".gitmodules" => return "file_type_git",
        "readme" | "readme.md" | "changelog.md" => return "file_type_markdown",
        _ => {}
    }
    match path
        .extension()
        .and_then(|p| p.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "rs" => "file_type_rust",
        "cs" | "csproj" | "fsproj" | "vbproj" => "file_type_csharp",
        "sln" | "slnx" => "file_type_sln",
        "js" | "jsx" | "mjs" | "cjs" => "file_type_js_official",
        "ts" | "tsx" => "file_type_typescript_official",
        "py" | "pyi" => "file_type_python",
        "java" | "jar" => "file_type_java",
        "c" | "h" | "cpp" | "hpp" | "cc" => "file_type_cpp",
        "html" | "htm" => "file_type_html",
        "css" | "scss" => "file_type_css",
        "json" | "jsonc" => "file_type_json",
        "yaml" | "yml" => "file_type_yaml",
        "md" | "mdx" => "file_type_markdown",
        "toml" => "file_type_toml",
        "ini" | "conf" | "cfg" | "config" | "env" => "file_type_config",
        _ => "default_file",
    }
}
fn bytes(name: &str) -> &'static [u8] {
    match name {
        "default_file" => include_bytes!("../../assets/icons/vscode/default_file.png"),
        "default_folder" => include_bytes!("../../assets/icons/vscode/default_folder.png"),
        "default_folder_opened" => {
            include_bytes!("../../assets/icons/vscode/default_folder_opened.png")
        }
        "file_type_rust" => include_bytes!("../../assets/icons/vscode/file_type_rust.png"),
        "file_type_csharp" => include_bytes!("../../assets/icons/vscode/file_type_csharp.png"),
        "file_type_js_official" => {
            include_bytes!("../../assets/icons/vscode/file_type_js_official.png")
        }
        "file_type_typescript_official" => {
            include_bytes!("../../assets/icons/vscode/file_type_typescript_official.png")
        }
        "file_type_python" => include_bytes!("../../assets/icons/vscode/file_type_python.png"),
        "file_type_java" => include_bytes!("../../assets/icons/vscode/file_type_java.png"),
        "file_type_cpp" => include_bytes!("../../assets/icons/vscode/file_type_cpp.png"),
        "file_type_html" => include_bytes!("../../assets/icons/vscode/file_type_html.png"),
        "file_type_css" => include_bytes!("../../assets/icons/vscode/file_type_css.png"),
        "file_type_json" => include_bytes!("../../assets/icons/vscode/file_type_json.png"),
        "file_type_yaml" => include_bytes!("../../assets/icons/vscode/file_type_yaml.png"),
        "file_type_markdown" => include_bytes!("../../assets/icons/vscode/file_type_markdown.png"),
        "file_type_toml" => include_bytes!("../../assets/icons/vscode/file_type_toml.png"),
        "file_type_git" => include_bytes!("../../assets/icons/vscode/file_type_git.png"),
        "file_type_config" => include_bytes!("../../assets/icons/vscode/file_type_config.png"),
        "file_type_npm" => include_bytes!("../../assets/icons/vscode/file_type_npm.png"),
        "file_type_docker" => include_bytes!("../../assets/icons/vscode/file_type_docker.png"),
        "file_type_sln" => include_bytes!("../../assets/icons/vscode/file_type_sln.png"),
        "luma" => include_bytes!("../../assets/identity/luma.png"),
        "brand_claude" => include_bytes!("../../assets/brands/claude.png"),
        "brand_claudecode" => include_bytes!("../../assets/brands/claudecode.png"),
        "brand_gemini" => include_bytes!("../../assets/brands/googlegemini.png"),
        "brand_antigravity" => include_bytes!("../../assets/brands/antigravity.png"),
        _ => include_bytes!("../../assets/icons/vscode/default_file.png"),
    }
}
pub(super) fn texture(ctx: &Context, name: &'static str) -> TextureHandle {
    let key = egui::Id::new(("embedded-asset", name));
    if let Some(texture) = ctx.data(|data| data.get_temp::<TextureHandle>(key)) {
        return texture;
    }
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes(name)));
    let mut reader = decoder.read_info().expect("validated PNG header");
    let mut rgba = vec![0; reader.output_buffer_size().expect("bounded embedded PNG")];
    let info = reader.next_frame(&mut rgba).expect("validated bundled PNG");
    assert_eq!(
        info.color_type,
        png::ColorType::Rgba,
        "embedded assets must be RGBA8"
    );
    let image = egui::ColorImage::from_rgba_unmultiplied(
        [info.width as usize, info.height as usize],
        &rgba[..info.buffer_size()],
    );
    let texture = ctx.load_texture(name, image, TextureOptions::LINEAR);
    ctx.data_mut(|data| data.insert_temp(key, texture.clone()));
    texture
}
pub(super) fn paint(ui: &egui::Ui, rect: Rect, name: &'static str) {
    let texture = texture(ui.ctx(), name);
    ui.painter().image(
        texture.id(),
        rect,
        Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        if name == "default_file" && super::theme::colors().canvas.r() > 200 {
            Color32::from_gray(60)
        } else {
            Color32::WHITE
        },
    );
}
/// Paint an embedded RGBA mask tinted with `color`.
pub(super) fn tinted(painter: &egui::Painter, rect: Rect, name: &'static str, color: Color32) {
    let texture = texture(painter.ctx(), name);
    painter.image(
        texture.id(),
        rect,
        Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        color,
    );
}
pub(super) fn file(ui: &egui::Ui, center: egui::Pos2, path: &Path) {
    paint(
        ui,
        Rect::from_center_size(center, egui::vec2(17.0, 17.0)),
        file_name(path),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_icons_and_character_decode_and_reuse_their_texture() {
        let ctx = Context::default();
        for name in [
            "default_file",
            "default_folder",
            "default_folder_opened",
            "file_type_rust",
            "file_type_csharp",
            "file_type_js_official",
            "file_type_typescript_official",
            "file_type_python",
            "file_type_java",
            "file_type_cpp",
            "file_type_html",
            "file_type_css",
            "file_type_json",
            "file_type_yaml",
            "file_type_markdown",
            "file_type_toml",
            "file_type_git",
            "file_type_config",
            "file_type_npm",
            "file_type_docker",
            "file_type_sln",
            "luma",
            "brand_claude",
            "brand_claudecode",
            "brand_gemini",
            "brand_antigravity",
        ] {
            let first = texture(&ctx, name);
            let second = texture(&ctx, name);
            assert_eq!(first.id(), second.id());
            assert!(first.size()[0] <= 256 && first.size()[1] <= 256);
        }
        assert_eq!(file_name(Path::new("package.json")), "file_type_npm");
        assert_eq!(file_name(Path::new("Cargo.toml")), "file_type_rust");
        assert_eq!(file_name(Path::new("solution.sln")), "file_type_sln");
    }
}
