use std::sync::LazyLock;
use std::fs;
use std::path::Path;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

const MAX_PREVIEW_LINES: usize = 100;
const MAX_FILE_SIZE: u64 = 1024 * 1024; // 1 MB

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

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
    } else if is_archive(path) {
        preview_archive(path)
    } else if is_markdown(path) {
        preview_markdown(path)
    } else {
        preview_text_file(path)
    }
}

fn is_archive(path: &Path) -> bool {
    let name = path.to_string_lossy().to_lowercase();
    name.ends_with(".zip")
        || name.ends_with(".tar")
        || name.ends_with(".tar.gz")
        || name.ends_with(".tgz")
}

fn is_markdown(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .as_deref(),
        Some("md" | "markdown" | "mkd")
    )
}

fn preview_archive(path: &Path) -> Vec<PreviewLine> {
    let name = path.to_string_lossy().to_lowercase();
    let mut lines = vec![PreviewLine {
        text: format!(
            "📦 Archive: {}",
            path.file_name().unwrap_or_default().to_string_lossy()
        ),
        style: PreviewStyle::Header,
    }];
    if let Ok(meta) = fs::metadata(path) {
        lines.push(PreviewLine {
            text: format!("Size: {}", format_size(meta.len())),
            style: PreviewStyle::Normal,
        });
    }
    lines.push(PreviewLine {
        text: String::new(),
        style: PreviewStyle::Normal,
    });
    lines.push(PreviewLine {
        text: "Contents:".to_string(),
        style: PreviewStyle::Header,
    });

    let entries = if name.ends_with(".zip") {
        list_zip_contents(path)
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        list_tar_gz_contents(path)
    } else if name.ends_with(".tar") {
        list_tar_contents(path)
    } else {
        Err("Unsupported format".to_string())
    };

    match entries {
        Ok(entries) => {
            lines.push(PreviewLine {
                text: format!("{} entries", entries.len()),
                style: PreviewStyle::Normal,
            });
            lines.push(PreviewLine {
                text: String::new(),
                style: PreviewStyle::Normal,
            });
            for entry in entries.into_iter().take(MAX_PREVIEW_LINES - lines.len()) {
                let icon = if entry.ends_with('/') { "📁" } else { "📄" };
                lines.push(PreviewLine {
                    text: format!("  {icon} {entry}"),
                    style: if entry.ends_with('/') {
                        PreviewStyle::Directory
                    } else {
                        PreviewStyle::Normal
                    },
                });
            }
        }
        Err(e) => {
            lines.push(PreviewLine {
                text: format!("Error reading archive: {e}"),
                style: PreviewStyle::Normal,
            });
        }
    }
    lines
}

fn list_zip_contents(path: &Path) -> Result<Vec<String>, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut names = Vec::new();
    for i in 0..archive.len() {
        if let Ok(entry) = archive.by_index(i) {
            names.push(entry.name().to_string());
        }
    }
    Ok(names)
}

fn list_tar_gz_contents(path: &Path) -> Result<Vec<String>, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    let mut names = Vec::new();
    for entry in archive.entries().map_err(|e| e.to_string())?.flatten() {
        if let Ok(p) = entry.path() {
            names.push(p.to_string_lossy().to_string());
        }
    }
    Ok(names)
}

fn list_tar_contents(path: &Path) -> Result<Vec<String>, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = tar::Archive::new(file);
    let mut names = Vec::new();
    for entry in archive.entries().map_err(|e| e.to_string())?.flatten() {
        if let Ok(p) = entry.path() {
            names.push(p.to_string_lossy().to_string());
        }
    }
    Ok(names)
}

fn preview_markdown(path: &Path) -> Vec<PreviewLine> {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            return vec![PreviewLine {
                text: format!("Error: {e}"),
                style: PreviewStyle::Normal,
            }];
        }
    };

    let mut lines = Vec::new();
    for line in content.lines().take(MAX_PREVIEW_LINES) {
        let (text, style) = render_markdown_line(line);
        lines.push(PreviewLine { text, style });
    }
    if content.lines().count() > MAX_PREVIEW_LINES {
        lines.push(PreviewLine {
            text: format!(
                "... ({} more lines)",
                content.lines().count() - MAX_PREVIEW_LINES
            ),
            style: PreviewStyle::Header,
        });
    }
    lines
}

fn render_markdown_line(line: &str) -> (String, PreviewStyle) {
    if let Some(rest) = line.strip_prefix("### ") {
        (format!("▒ {rest}"), PreviewStyle::Header)
    } else if let Some(rest) = line.strip_prefix("## ") {
        (format!("▓ {rest}"), PreviewStyle::Header)
    } else if let Some(rest) = line.strip_prefix("# ") {
        (format!("█ {rest}"), PreviewStyle::Header)
    } else if line.starts_with("#### ") || line.starts_with("##### ") || line.starts_with("###### ")
    {
        let content = line.trim_start_matches('#').trim_start();
        (format!("░ {content}"), PreviewStyle::Header)
    } else if line.starts_with("```") {
        (
            format!("  ╌╌╌ {}", line.trim_start_matches('`')),
            PreviewStyle::LineNumber,
        )
    } else if let Some(rest) = line.strip_prefix("- ") {
        (format!("  • {rest}"), PreviewStyle::Normal)
    } else if let Some(rest) = line.strip_prefix("* ") {
        (format!("  • {rest}"), PreviewStyle::Normal)
    } else if let Some(rest) = line.strip_prefix("> ") {
        (format!("  │ {rest}"), PreviewStyle::Directory)
    } else if line.starts_with("---") || line.starts_with("***") || line.starts_with("___") {
        (
            "  ────────────────────────────".to_string(),
            PreviewStyle::Header,
        )
    } else if line.trim().is_empty() {
        (String::new(), PreviewStyle::Normal)
    } else {
        (format!("  {line}"), PreviewStyle::Normal)
    }
}

pub fn format_size(bytes: u64) -> String {
    const TB: u64 = 1024 * 1024 * 1024 * 1024;
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= TB {
        format!("{:.1} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
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

    let ss = &*SYNTAX_SET;
    let ts = &*THEME_SET;
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
        let _ = h.highlight_line(line, ss);
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

/// Calculate disk usage of a directory recursively
pub fn calculate_disk_usage(path: &Path) -> Vec<PreviewLine> {
    let mut lines = vec![PreviewLine {
        text: format!("💾 Disk Usage: {}", path.display()),
        style: PreviewStyle::Header,
    }];

    if !path.is_dir() {
        if let Ok(meta) = fs::metadata(path) {
            lines.push(PreviewLine {
                text: format!("  Total: {}", format_size(meta.len())),
                style: PreviewStyle::Normal,
            });
        }
        return lines;
    }

    let mut entries: Vec<(String, u64, bool)> = Vec::new();
    if let Ok(dir) = fs::read_dir(path) {
        for entry in dir.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let size = if is_dir {
                dir_size(&entry.path())
            } else {
                entry.metadata().map(|m| m.len()).unwrap_or(0)
            };
            entries.push((name, size, is_dir));
        }
    }

    entries.sort_by(|a, b| b.1.cmp(&a.1));
    let total: u64 = entries.iter().map(|e| e.1).sum();

    lines.push(PreviewLine {
        text: format!("  Total: {}", format_size(total)),
        style: PreviewStyle::Header,
    });
    lines.push(PreviewLine {
        text: String::new(),
        style: PreviewStyle::Normal,
    });

    let max_bar = 30;
    for (name, size, is_dir) in entries.iter().take(MAX_PREVIEW_LINES - 4) {
        let pct = if total > 0 {
            *size as f64 / total as f64 * 100.0
        } else {
            0.0
        };
        let bar_len = if total > 0 {
            (*size as f64 / total as f64 * max_bar as f64) as usize
        } else {
            0
        };
        let bar: String = "█".repeat(bar_len);
        let pad: String = "░".repeat(max_bar - bar_len);
        let icon = if *is_dir { "📁" } else { "📄" };
        lines.push(PreviewLine {
            text: format!(
                "  {icon} {bar}{pad} {:>5.1}% {:>8} {name}",
                pct,
                format_size(*size)
            ),
            style: if *is_dir {
                PreviewStyle::Directory
            } else {
                PreviewStyle::Normal
            },
        });
    }
    lines
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.filter_map(|e| e.ok()) {
            let ft = entry.file_type().unwrap_or_else(|_| {
                // fallback: treat as file
                fs::metadata(entry.path())
                    .map(|m| m.file_type())
                    .unwrap_or_else(|_| fs::metadata(".").unwrap().file_type())
            });
            if ft.is_dir() {
                total += dir_size(&entry.path());
            } else {
                total += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    total
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

    #[test]
    fn test_preview_archive_zip() {
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("test.zip");
        {
            let file = fs::File::create(&zip_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("hello.txt", options).unwrap();
            std::io::Write::write_all(&mut zip, b"hello world").unwrap();
            zip.finish().unwrap();
        }
        let lines = preview_path(&zip_path);
        assert!(lines[0].text.contains("Archive"));
        assert!(lines.iter().any(|l| l.text.contains("hello.txt")));
    }

    #[test]
    fn test_preview_markdown() {
        let tmp = TempDir::new().unwrap();
        let f = tmp.path().join("readme.md");
        fs::write(&f, "# Title\n\nSome text\n\n- item 1\n- item 2\n").unwrap();
        let lines = preview_path(&f);
        assert!(lines[0].text.contains("Title"));
        assert_eq!(lines[0].style, PreviewStyle::Header);
        assert!(lines.iter().any(|l| l.text.contains("•")));
    }

    #[test]
    fn test_is_markdown() {
        assert!(is_markdown(Path::new("README.md")));
        assert!(is_markdown(Path::new("doc.markdown")));
        assert!(!is_markdown(Path::new("file.txt")));
    }

    #[test]
    fn test_is_archive() {
        assert!(is_archive(Path::new("file.zip")));
        assert!(is_archive(Path::new("file.tar.gz")));
        assert!(is_archive(Path::new("file.tgz")));
        assert!(is_archive(Path::new("file.tar")));
        assert!(!is_archive(Path::new("file.txt")));
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1048576), "1.0 MB");
        assert_eq!(format_size(1073741824), "1.0 GB");
        assert_eq!(format_size(1099511627776), "1.0 TB");
    }

    #[test]
    fn test_disk_usage() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "hello").unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("sub/b.txt"), "world!").unwrap();
        let lines = calculate_disk_usage(tmp.path());
        assert!(lines[0].text.contains("Disk Usage"));
        assert!(lines.iter().any(|l| l.text.contains("Total")));
        assert!(lines.iter().any(|l| l.text.contains("a.txt")));
    }

    #[test]
    fn test_render_markdown_lines() {
        assert_eq!(render_markdown_line("# Hello").1, PreviewStyle::Header);
        assert_eq!(render_markdown_line("## Sub").1, PreviewStyle::Header);
        assert_eq!(render_markdown_line("### Sub2").1, PreviewStyle::Header);
        assert_eq!(render_markdown_line("- item").0, "  • item");
        assert_eq!(render_markdown_line("> quote").1, PreviewStyle::Directory);
        assert_eq!(render_markdown_line("---").1, PreviewStyle::Header);
        assert_eq!(render_markdown_line("plain text").0, "  plain text");
    }
}
