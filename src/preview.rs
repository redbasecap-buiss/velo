use std::fs;
use std::path::Path;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

const MAX_PREVIEW_LINES: usize = 100;
const MAX_FILE_SIZE: u64 = 1024 * 1024; // 1 MB

#[derive(Debug, Clone)]
pub struct PreviewLine {
    pub text: String,
    pub style: PreviewStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum PreviewStyle {
    Normal,
    Directory,
    Header,
    LineNumber,
}

pub fn preview_path(path: &Path) -> Vec<PreviewLine> {
    if path.is_dir() {
        preview_directory(path)
    } else if is_image(path) {
        preview_image_meta(path)
    } else {
        preview_text_file(path)
    }
}

fn preview_directory(path: &Path) -> Vec<PreviewLine> {
    let mut lines = vec![PreviewLine {
        text: format!("📁 Directory: {}", path.display()),
        style: PreviewStyle::Header,
    }];
    match fs::read_dir(path) {
        Ok(entries) => {
            let mut names: Vec<_> = entries
                .filter_map(|e| e.ok())
                .map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    (name, is_dir)
                })
                .collect();
            names.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
            for (name, is_dir) in names.into_iter().take(MAX_PREVIEW_LINES) {
                let prefix = if is_dir { "📁 " } else { "📄 " };
                lines.push(PreviewLine {
                    text: format!("{prefix}{name}"),
                    style: if is_dir {
                        PreviewStyle::Directory
                    } else {
                        PreviewStyle::Normal
                    },
                });
            }
        }
        Err(e) => lines.push(PreviewLine {
            text: format!("Error: {e}"),
            style: PreviewStyle::Normal,
        }),
    }
    lines
}

fn preview_text_file(path: &Path) -> Vec<PreviewLine> {
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) => {
            return vec![PreviewLine {
                text: format!("Error: {e}"),
                style: PreviewStyle::Normal,
            }];
        }
    };
    if meta.len() > MAX_FILE_SIZE {
        return vec![PreviewLine {
            text: format!("File too large ({} bytes)", meta.len()),
            style: PreviewStyle::Header,
        }];
    }
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => {
            return vec![PreviewLine {
                text: "Binary file".to_string(),
                style: PreviewStyle::Header,
            }];
        }
    };

    let ss = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let syntax = ss
        .find_syntax_for_file(path)
        .ok()
        .flatten()
        .unwrap_or_else(|| ss.find_syntax_plain_text());
    let mut h = HighlightLines::new(syntax, &ts.themes["base16-ocean.dark"]);

    let mut lines = Vec::new();
    for (i, line) in LinesWithEndings::from(&content).enumerate() {
        if i >= MAX_PREVIEW_LINES {
            lines.push(PreviewLine {
                text: format!("... ({} more lines)", content.lines().count() - i),
                style: PreviewStyle::Header,
            });
            break;
        }
        // We just use the text; terminal coloring would need styled spans
        let _ = h.highlight_line(line, &ss);
        lines.push(PreviewLine {
            text: format!("{:>4} │ {}", i + 1, line.trim_end()),
            style: PreviewStyle::Normal,
        });
    }
    lines
}

fn is_image(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg" | "ico")
    )
}

fn preview_image_meta(path: &Path) -> Vec<PreviewLine> {
    let mut lines = vec![PreviewLine {
        text: format!(
            "🖼️  Image: {}",
            path.file_name().unwrap_or_default().to_string_lossy()
        ),
        style: PreviewStyle::Header,
    }];
    if let Ok(meta) = fs::metadata(path) {
        lines.push(PreviewLine {
            text: format!("Size: {} bytes", meta.len()),
            style: PreviewStyle::Normal,
        });
    }
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        lines.push(PreviewLine {
            text: format!("Format: {}", ext.to_uppercase()),
            style: PreviewStyle::Normal,
        });
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_preview_directory() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "hello").unwrap();
        fs::create_dir(tmp.path().join("subdir")).unwrap();
        let lines = preview_path(tmp.path());
        assert!(!lines.is_empty());
        assert!(lines[0].text.contains("Directory"));
    }

    #[test]
    fn test_preview_text_file() {
        let tmp = TempDir::new().unwrap();
        let f = tmp.path().join("test.rs");
        fs::write(&f, "fn main() {}").unwrap();
        let lines = preview_path(&f);
        assert!(!lines.is_empty());
        assert!(lines[0].text.contains("main"));
    }

    #[test]
    fn test_preview_binary_file() {
        let tmp = TempDir::new().unwrap();
        let f = tmp.path().join("binary.bin");
        fs::write(&f, &[0u8, 1, 2, 255, 254]).unwrap();
        let lines = preview_path(&f);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_preview_image_meta() {
        let tmp = TempDir::new().unwrap();
        let f = tmp.path().join("photo.png");
        fs::write(&f, "fake png").unwrap();
        let lines = preview_path(&f);
        assert!(lines[0].text.contains("Image"));
    }

    #[test]
    fn test_is_image() {
        assert!(is_image(Path::new("test.png")));
        assert!(is_image(Path::new("test.JPG")));
        assert!(!is_image(Path::new("test.txt")));
        assert!(!is_image(Path::new("test.rs")));
    }

    #[test]
    fn test_preview_nonexistent() {
        let lines = preview_path(Path::new("/nonexistent_file_xyz"));
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_preview_style_eq() {
        assert_eq!(PreviewStyle::Normal, PreviewStyle::Normal);
        assert_ne!(PreviewStyle::Normal, PreviewStyle::Header);
    }
}
