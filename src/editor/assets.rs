//! Central registry of embedded raster assets: Material Icon Theme file and folder icons,
//! provider marks and identity art. Each PNG is decoded once per egui context, uploaded
//! with mipmaps so 16 px icons stay crisp at every Windows scale, and reused by handle.
use eframe::egui::{self, Color32, Context, Rect, TextureHandle, TextureOptions};
use std::path::Path;

/// Material icon for a file, by exact file name first and extension second.
pub(super) fn file_name(path: &Path) -> &'static str {
    let name = path
        .file_name()
        .and_then(|p| p.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let by_name = match name.as_str() {
        "cargo.toml" | "rust-toolchain" | "rust-toolchain.toml" => Some("rust"),
        "package.json" => Some("nodejs"),
        "package-lock.json" | "npm-shrinkwrap.json" | ".npmrc" => Some("npm"),
        "yarn.lock" | ".yarnrc" | ".yarnrc.yml" => Some("yarn"),
        "dockerfile"
        | ".dockerignore"
        | "docker-compose.yml"
        | "docker-compose.yaml"
        | "compose.yml"
        | "compose.yaml" => Some("docker"),
        ".gitignore" | ".gitattributes" | ".gitmodules" | ".gitkeep" => Some("git"),
        ".editorconfig" => Some("editorconfig"),
        "makefile" | "gnumakefile" => Some("makefile"),
        "cmakelists.txt" => Some("cmake"),
        "pom.xml" => Some("maven"),
        "build.gradle"
        | "build.gradle.kts"
        | "settings.gradle"
        | "settings.gradle.kts"
        | "gradlew"
        | "gradlew.bat"
        | "gradle.properties" => Some("gradle"),
        _ => None,
    };
    if let Some(icon) = by_name {
        return icon;
    }
    if name.starts_with("readme") {
        return "readme";
    }
    if name.starts_with("license") || name.starts_with("licence") || name.starts_with("copying") {
        return "license";
    }
    if name.starts_with("tsconfig") || name.starts_with("jsconfig") {
        return "tsconfig";
    }
    if name.starts_with(".env") {
        return "tune";
    }
    for (prefix, icon) in [
        ("vite.config", "vite"),
        ("webpack.config", "webpack"),
        ("babel.config", "babel"),
        (".babelrc", "babel"),
        (".eslintrc", "eslint"),
        ("eslint.config", "eslint"),
        (".prettierrc", "prettier"),
        ("prettier.config", "prettier"),
    ] {
        if name.starts_with(prefix) {
            return icon;
        }
    }
    match path
        .extension()
        .and_then(|p| p.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "rs" => "rust",
        "py" | "pyw" | "pyi" => "python",
        "ipynb" => "jupyter",
        "java" => "java",
        "class" | "jar" => "javaclass",
        "js" | "mjs" | "cjs" => "javascript",
        "jsx" => "react",
        "ts" | "mts" | "cts" => "typescript",
        "tsx" => "react_ts",
        "c" => "c",
        "h" => "h",
        "cpp" | "cc" | "cxx" | "c++" => "cpp",
        "hpp" | "hh" | "hxx" => "hpp",
        "cs" | "csx" => "csharp",
        "sln" | "slnx" | "csproj" | "vbproj" | "fsproj" | "vcxproj" => "visualstudio",
        "go" => "go",
        "kt" | "kts" => "kotlin",
        "swift" => "swift",
        "php" => "php",
        "rb" => "ruby",
        "lua" => "lua",
        "dart" => "dart",
        "zig" => "zig",
        "nix" => "nix",
        "tf" | "tfvars" => "terraform",
        "vue" => "vue",
        "html" | "htm" => "html",
        "css" => "css",
        "scss" | "sass" => "sass",
        "json" | "jsonc" | "json5" => "json",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "xml" | "xaml" | "xsd" | "plist" => "xml",
        "md" | "mdx" | "markdown" => "markdown",
        "graphql" | "gql" => "graphql",
        "http" | "rest" => "http",
        "sql" | "db" | "sqlite" | "sqlite3" => "database",
        "sh" | "bash" | "zsh" | "fish" | "bat" | "cmd" => "console",
        "ps1" | "psm1" | "psd1" => "powershell",
        "svg" => "svg",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "ico" | "bmp" | "tiff" => "image",
        "pdf" => "pdf",
        "zip" | "7z" | "rar" | "tar" | "gz" | "tgz" | "xz" => "zip",
        "exe" | "dll" | "msi" => "exe",
        "ttf" | "otf" | "woff" | "woff2" => "font",
        "mp4" | "mov" | "webm" | "mkv" => "video",
        "mp3" | "wav" | "ogg" | "flac" => "audio",
        "lock" => "lock",
        "log" => "log",
        "ini" | "cfg" | "conf" | "config" | "properties" => "settings",
        "pem" | "crt" | "cer" => "certificate",
        "key" => "key",
        _ => "document",
    }
}

/// Material folder icon for a directory name, in its open or closed form.
pub(super) fn folder_name(path: &Path, open: bool) -> &'static str {
    let name = path
        .file_name()
        .and_then(|p| p.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let (closed, opened) = match name.as_str() {
        "src" | "source" | "sources" => ("folder-src", "folder-src-open"),
        ".git" => ("folder-git", "folder-git-open"),
        "node_modules" => ("folder-node", "folder-node-open"),
        "dist" | "build" | "out" | "output" | "release" | "bin" => {
            ("folder-dist", "folder-dist-open")
        }
        "target" => ("folder-target", "folder-target-open"),
        "docs" | "doc" | "documentation" => ("folder-docs", "folder-docs-open"),
        "test" | "tests" | "__tests__" | "spec" | "specs" => ("folder-test", "folder-test-open"),
        "config" | "configs" | ".config" | "settings" => ("folder-config", "folder-config-open"),
        "images" | "img" | "icons" | "screenshots" | "captures" => {
            ("folder-images", "folder-images-open")
        }
        ".github" => ("folder-github", "folder-github-open"),
        ".vscode" => ("folder-vscode", "folder-vscode-open"),
        "scripts" => ("folder-scripts", "folder-scripts-open"),
        "public" | "static" | "www" => ("folder-public", "folder-public-open"),
        "docker" | ".docker" => ("folder-docker", "folder-docker-open"),
        "gradle" | ".gradle" => ("folder-gradle", "folder-gradle-open"),
        "assets" | "resources" | "res" => ("folder-resource", "folder-resource-open"),
        "lib" | "libs" | "vendor" => ("folder-lib", "folder-lib-open"),
        "include" | "includes" => ("folder-include", "folder-include-open"),
        "components" => ("folder-components", "folder-components-open"),
        "tmp" | "temp" | ".cache" | "cache" => ("folder-temp", "folder-temp-open"),
        "tools" | "utils" => ("folder-tools", "folder-tools-open"),
        _ => ("folder", "folder-open"),
    };
    if open { opened } else { closed }
}

fn bytes(name: &str) -> &'static [u8] {
    match name {
        "luma" => include_bytes!("../../assets/identity/luma.png"),
        "brand_claude" => include_bytes!("../../assets/brands/claude.png"),
        "brand_claudecode" => include_bytes!("../../assets/brands/claudecode.png"),
        "brand_gemini" => include_bytes!("../../assets/brands/googlegemini.png"),
        "brand_antigravity" => include_bytes!("../../assets/brands/antigravity.png"),
        lucide if lucide.starts_with("lucide-") => super::ui_icons::LUCIDE
            .iter()
            .find(|(n, _)| *n == lucide)
            .map(|(_, b)| *b)
            .unwrap_or_else(|| panic!("{lucide} is not bundled")),
        _ => super::icons::MATERIAL
            .iter()
            .find(|(n, _)| *n == name)
            .or_else(|| {
                super::icons::MATERIAL
                    .iter()
                    .find(|(n, _)| *n == "document")
            })
            .map(|(_, b)| *b)
            .expect("document icon is bundled"),
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
    let options = TextureOptions {
        mipmap_mode: Some(egui::TextureFilter::Linear),
        ..TextureOptions::LINEAR
    };
    let texture = ctx.load_texture(name, image, options);
    ctx.data_mut(|data| data.insert_temp(key, texture.clone()));
    texture
}

/// Snap a square icon rect to whole physical pixels so small icons stay sharp.
fn snap(ctx: &Context, rect: Rect) -> Rect {
    let ppp = ctx.pixels_per_point();
    let side = (rect.width() * ppp).round() / ppp;
    let min = ((rect.center() - egui::Vec2::splat(side / 2.0)).to_vec2() * ppp).round() / ppp;
    Rect::from_min_size(min.to_pos2(), egui::Vec2::splat(side))
}

pub(super) fn paint(ui: &egui::Ui, rect: Rect, name: &'static str) {
    let texture = texture(ui.ctx(), name);
    ui.painter().image(
        texture.id(),
        snap(ui.ctx(), rect),
        Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );
}

/// Paint an embedded RGBA mask tinted with `color`.
pub(super) fn tinted(painter: &egui::Painter, rect: Rect, name: &'static str, color: Color32) {
    let texture = texture(painter.ctx(), name);
    painter.image(
        texture.id(),
        snap(painter.ctx(), rect),
        Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        color,
    );
}

pub(super) fn file(ui: &egui::Ui, center: egui::Pos2, path: &Path) {
    paint(
        ui,
        Rect::from_center_size(center, egui::vec2(16.0, 16.0)),
        file_name(path),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_mapped_icon_is_bundled_and_decodes_once() {
        let ctx = Context::default();
        let samples = [
            "main.rs",
            "app.py",
            "Main.java",
            "index.js",
            "app.ts",
            "view.tsx",
            "a.c",
            "a.h",
            "a.cpp",
            "a.hpp",
            "Program.cs",
            "App.sln",
            "main.go",
            "index.html",
            "site.css",
            "data.json",
            "Cargo.toml",
            "config.toml",
            "ci.yml",
            "pom.xml",
            "build.gradle",
            "README.md",
            "notes.md",
            ".gitignore",
            "Dockerfile",
            "tsconfig.json",
            "package.json",
            "LICENSE",
            ".env.local",
            "run.ps1",
            "x.unknown",
        ];
        for sample in samples {
            let icon = file_name(Path::new(sample));
            assert!(
                super::super::icons::MATERIAL
                    .iter()
                    .any(|(n, _)| *n == icon),
                "{sample} maps to missing icon {icon}"
            );
            let first = texture(&ctx, icon);
            assert_eq!(first.id(), texture(&ctx, icon).id());
        }
        for folder in ["src", "target", ".git", "docs", "tests", "assets", "random"] {
            for open in [false, true] {
                let icon = folder_name(Path::new(folder), open);
                assert!(
                    super::super::icons::MATERIAL
                        .iter()
                        .any(|(n, _)| *n == icon),
                    "{icon}"
                );
            }
        }
        assert_eq!(file_name(Path::new("Cargo.toml")), "rust");
        assert_eq!(file_name(Path::new("package.json")), "nodejs");
        assert_eq!(file_name(Path::new("App.sln")), "visualstudio");
        assert_eq!(file_name(Path::new("tsconfig.base.json")), "tsconfig");
        assert_eq!(folder_name(Path::new("src"), true), "folder-src-open");
        for brand in ["luma", "brand_claude", "brand_gemini", "brand_antigravity"] {
            assert!(texture(&ctx, brand).size()[0] <= 256);
        }
    }
}
