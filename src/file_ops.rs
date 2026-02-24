use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct PendingOp {
    pub kind: OpKind,
    pub sources: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum OpKind {
    Copy,
    Move,
}

pub fn copy_file(src: &Path, dest_dir: &Path) -> Result<PathBuf, String> {
    let file_name = src.file_name().ok_or_else(|| "No filename".to_string())?;
    let dest = dest_dir.join(file_name);
    if src.is_dir() {
        copy_dir_recursive(src, &dest).map_err(|e| e.to_string())?;
    } else {
        fs::copy(src, &dest).map_err(|e| e.to_string())?;
    }
    Ok(dest)
}

pub fn move_file(src: &Path, dest_dir: &Path) -> Result<PathBuf, String> {
    let file_name = src.file_name().ok_or_else(|| "No filename".to_string())?;
    let dest = dest_dir.join(file_name);
    fs::rename(src, &dest).map_err(|e| e.to_string())?;
    Ok(dest)
}

pub fn delete_to_trash(path: &Path) -> Result<(), String> {
    trash::delete(path).map_err(|e| e.to_string())
}

pub fn rename_file(path: &Path, new_name: &str) -> Result<PathBuf, String> {
    let parent = path.parent().ok_or_else(|| "No parent".to_string())?;
    let dest = parent.join(new_name);
    fs::rename(path, &dest).map_err(|e| e.to_string())?;
    Ok(dest)
}

pub fn create_file(dir: &Path, name: &str) -> Result<PathBuf, String> {
    let path = dir.join(name);
    fs::File::create(&path).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn create_dir(dir: &Path, name: &str) -> Result<PathBuf, String> {
    let path = dir.join(name);
    fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn copy_path_to_clipboard(path: &Path) -> Result<(), String> {
    let text = path.display().to_string();
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text).map_err(|e| e.to_string())
}

pub fn copy_content_to_clipboard(path: &Path) -> Result<(), String> {
    if path.is_dir() {
        return Err("Cannot copy directory content".to_string());
    }
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(content).map_err(|e| e.to_string())
}

#[cfg(unix)]
#[allow(dead_code)]
pub fn get_permissions(path: &Path) -> Result<u32, String> {
    use std::os::unix::fs::PermissionsExt;
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    Ok(meta.permissions().mode() & 0o7777)
}

#[cfg(not(unix))]
#[allow(dead_code)]
pub fn get_permissions(_path: &Path) -> Result<u32, String> {
    Err("Permissions not supported on this platform".to_string())
}

#[cfg(unix)]
#[allow(dead_code)]
pub fn set_permissions(path: &Path, mode: u32) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let perms = fs::Permissions::from_mode(mode);
    fs::set_permissions(path, perms).map_err(|e| e.to_string())
}

#[cfg(not(unix))]
#[allow(dead_code)]
pub fn set_permissions(_path: &Path, _mode: u32) -> Result<(), String> {
    Err("Permissions not supported on this platform".to_string())
}

/// Format a unix permission mode as rwx string (e.g., "rwxr-xr--")
#[allow(dead_code)]
pub fn format_permissions(mode: u32) -> String {
    let mut s = String::with_capacity(9);
    for shift in [6, 3, 0] {
        let bits = (mode >> shift) & 0o7;
        s.push(if bits & 4 != 0 { 'r' } else { '-' });
        s.push(if bits & 2 != 0 { 'w' } else { '-' });
        s.push(if bits & 1 != 0 { 'x' } else { '-' });
    }
    s
}

/// Parse an octal string like "755" into a mode
#[allow(dead_code)]
pub fn parse_octal_mode(s: &str) -> Option<u32> {
    u32::from_str_radix(s, 8).ok().filter(|&m| m <= 0o7777)
}

/// Toggle a specific permission bit. Position 0-8 maps to rwxrwxrwx.
#[allow(dead_code)]
pub fn toggle_permission_bit(mode: u32, position: usize) -> u32 {
    if position > 8 {
        return mode;
    }
    // position 0 = owner r (bit 8), position 8 = other x (bit 0)
    let bit = 8 - position;
    mode ^ (1 << bit)
}

/// Change file permissions (octal string like "755")
pub fn chmod_file(path: &Path, mode_str: &str) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = u32::from_str_radix(mode_str, 8)
            .map_err(|_| format!("Invalid octal mode: {mode_str}"))?;
        let perms = std::fs::Permissions::from_mode(mode);
        std::fs::set_permissions(path, perms)?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode_str);
        Err("chmod not supported on this platform".into())
    }
}

/// Recursively search for a pattern in files under a directory
pub fn search_recursive(dir: &Path, pattern: &str, max_results: usize) -> Vec<SearchResult> {
    let mut results = Vec::new();
    let pattern_lower = pattern.to_lowercase();
    search_recursive_inner(dir, &pattern_lower, max_results, &mut results);
    results
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub path: PathBuf,
    pub line_number: usize,
    pub line_text: String,
}

fn search_recursive_inner(
    dir: &Path,
    pattern: &str,
    max_results: usize,
    results: &mut Vec<SearchResult>,
) {
    if results.len() >= max_results {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        if results.len() >= max_results {
            return;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            search_recursive_inner(&path, pattern, max_results, results);
        } else if path.is_file() {
            // Skip large/binary files
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            if size > 1024 * 1024 || size == 0 {
                continue;
            }
            if let Ok(content) = fs::read_to_string(&path) {
                for (i, line) in content.lines().enumerate() {
                    if results.len() >= max_results {
                        return;
                    }
                    if line.to_lowercase().contains(pattern) {
                        results.push(SearchResult {
                            path: path.clone(),
                            line_number: i + 1,
                            line_text: line.to_string(),
                        });
                    }
                }
            }
        }
    }
}

/// Check if a path is an extractable archive
pub fn is_archive(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();
    matches!(ext.as_str(), "zip" | "gz" | "tar" | "tgz")
        || path.to_string_lossy().to_lowercase().ends_with(".tar.gz")
}

/// Extract an archive (zip, tar.gz, tar, tgz) into dest_dir
pub fn extract_archive(archive: &Path, dest_dir: &Path) -> Result<Vec<String>, String> {
    let name = archive.to_string_lossy().to_lowercase();
    if name.ends_with(".zip") {
        extract_zip(archive, dest_dir)
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        extract_tar_gz(archive, dest_dir)
    } else if name.ends_with(".tar") {
        extract_tar(archive, dest_dir)
    } else if name.ends_with(".gz") {
        extract_gz(archive, dest_dir)
    } else {
        Err("Unsupported archive format".to_string())
    }
}

fn extract_zip(archive: &Path, dest_dir: &Path) -> Result<Vec<String>, String> {
    let file = fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut extracted = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        let out_path = dest_dir.join(&name);
        if entry.is_dir() {
            fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut out = fs::File::create(&out_path).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        }
        extracted.push(name);
    }
    Ok(extracted)
}

fn extract_tar_gz(archive: &Path, dest_dir: &Path) -> Result<Vec<String>, String> {
    let file = fs::File::open(archive).map_err(|e| e.to_string())?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(gz);
    let mut extracted = Vec::new();
    for entry in tar.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?.to_path_buf();
        let name = path.display().to_string();
        entry.unpack_in(dest_dir).map_err(|e| e.to_string())?;
        extracted.push(name);
    }
    Ok(extracted)
}

fn extract_tar(archive: &Path, dest_dir: &Path) -> Result<Vec<String>, String> {
    let file = fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut tar = tar::Archive::new(file);
    let mut extracted = Vec::new();
    for entry in tar.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?.to_path_buf();
        let name = path.display().to_string();
        entry.unpack_in(dest_dir).map_err(|e| e.to_string())?;
        extracted.push(name);
    }
    Ok(extracted)
}

fn extract_gz(archive: &Path, dest_dir: &Path) -> Result<Vec<String>, String> {
    let file = fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut gz = flate2::read::GzDecoder::new(file);
    let stem = archive
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let out_path = dest_dir.join(stem);
    let mut out = fs::File::create(&out_path).map_err(|e| e.to_string())?;
    std::io::copy(&mut gz, &mut out).map_err(|e| e.to_string())?;
    Ok(vec![stem.to_string()])
}

/// Compress files into a zip archive at dest_path
pub fn compress_zip(paths: &[PathBuf], dest_path: &Path) -> Result<usize, String> {
    let file = fs::File::create(dest_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let mut count = 0;
    for path in paths {
        if path.is_dir() {
            count += add_dir_to_zip(&mut zip, path, path.parent().unwrap_or(path), options)?;
        } else {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
            zip.start_file(name, options).map_err(|e| e.to_string())?;
            let content = fs::read(path).map_err(|e| e.to_string())?;
            std::io::Write::write_all(&mut zip, &content).map_err(|e| e.to_string())?;
            count += 1;
        }
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(count)
}

fn add_dir_to_zip(
    zip: &mut zip::ZipWriter<fs::File>,
    dir: &Path,
    base: &Path,
    options: zip::write::SimpleFileOptions,
) -> Result<usize, String> {
    let mut count = 0;
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let rel = path
            .strip_prefix(base)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .to_string();
        if path.is_dir() {
            zip.add_directory(format!("{rel}/"), options)
                .map_err(|e| e.to_string())?;
            count += add_dir_to_zip(zip, &path, base, options)?;
        } else {
            zip.start_file(&rel, options).map_err(|e| e.to_string())?;
            let content = fs::read(&path).map_err(|e| e.to_string())?;
            std::io::Write::write_all(zip, &content).map_err(|e| e.to_string())?;
            count += 1;
        }
    }
    Ok(count)
}

/// Compress files into a tar.gz archive at dest_path
#[allow(dead_code)]
pub fn compress_tar_gz(paths: &[PathBuf], dest_path: &Path) -> Result<usize, String> {
    let file = fs::File::create(dest_path).map_err(|e| e.to_string())?;
    let gz = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut tar = tar::Builder::new(gz);
    let mut count = 0;
    for path in paths {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        if path.is_dir() {
            tar.append_dir_all(name, path).map_err(|e| e.to_string())?;
            count += 1; // count dir as 1
        } else {
            let mut f = fs::File::open(path).map_err(|e| e.to_string())?;
            tar.append_file(name, &mut f).map_err(|e| e.to_string())?;
            count += 1;
        }
    }
    tar.finish().map_err(|e| e.to_string())?;
    Ok(count)
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let target = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_create_file() {
        let tmp = TempDir::new().unwrap();
        let result = create_file(tmp.path(), "test.txt");
        assert!(result.is_ok());
        assert!(result.unwrap().exists());
    }

    #[test]
    fn test_create_dir() {
        let tmp = TempDir::new().unwrap();
        let result = create_dir(tmp.path(), "subdir");
        assert!(result.is_ok());
        assert!(result.unwrap().is_dir());
    }

    #[test]
    fn test_copy_file() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src.txt");
        fs::write(&src, "hello").unwrap();
        let dest_dir = tmp.path().join("dest");
        fs::create_dir(&dest_dir).unwrap();
        let result = copy_file(&src, &dest_dir);
        assert!(result.is_ok());
        let dest = result.unwrap();
        assert_eq!(fs::read_to_string(dest).unwrap(), "hello");
    }

    #[test]
    fn test_move_file() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src.txt");
        fs::write(&src, "world").unwrap();
        let dest_dir = tmp.path().join("dest");
        fs::create_dir(&dest_dir).unwrap();
        let result = move_file(&src, &dest_dir);
        assert!(result.is_ok());
        assert!(!src.exists());
        assert_eq!(fs::read_to_string(result.unwrap()).unwrap(), "world");
    }

    #[test]
    fn test_rename_file() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("old.txt");
        fs::write(&src, "data").unwrap();
        let result = rename_file(&src, "new.txt");
        assert!(result.is_ok());
        assert!(!src.exists());
        assert!(tmp.path().join("new.txt").exists());
    }

    #[test]
    fn test_copy_dir_recursive() {
        let tmp = TempDir::new().unwrap();
        let src_dir = tmp.path().join("src");
        fs::create_dir(&src_dir).unwrap();
        fs::write(src_dir.join("a.txt"), "a").unwrap();
        let sub = src_dir.join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("b.txt"), "b").unwrap();

        let dest_dir = tmp.path().join("dest");
        copy_dir_recursive(&src_dir, &dest_dir).unwrap();
        assert!(dest_dir.join("a.txt").exists());
        assert!(dest_dir.join("sub").join("b.txt").exists());
    }

    // Note: trash::delete test skipped — may trigger macOS Finder permission dialogs

    #[test]
    fn test_copy_file_preserves_content() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("data.txt");
        fs::write(&src, "important data here").unwrap();
        let dest_dir = tmp.path().join("out");
        fs::create_dir(&dest_dir).unwrap();
        let dest = copy_file(&src, &dest_dir).unwrap();
        assert_eq!(fs::read_to_string(&src).unwrap(), "important data here");
        assert_eq!(fs::read_to_string(dest).unwrap(), "important data here");
    }

    #[test]
    fn test_create_nested_dir() {
        let tmp = TempDir::new().unwrap();
        let result = create_dir(tmp.path(), "a/b/c");
        assert!(result.is_ok());
        assert!(tmp.path().join("a/b/c").is_dir());
    }

    #[test]
    fn test_create_file_in_nonexistent_dir() {
        let result = create_file(Path::new("/nonexistent_dir_xyz"), "test.txt");
        assert!(result.is_err());
    }

    #[test]
    fn test_op_kind_eq() {
        assert_eq!(OpKind::Copy, OpKind::Copy);
        assert_ne!(OpKind::Copy, OpKind::Move);
    }

    #[test]
    fn test_pending_op_clone() {
        let op = PendingOp {
            kind: OpKind::Copy,
            sources: vec![PathBuf::from("/tmp/test")],
        };
        let cloned = op.clone();
        assert_eq!(cloned.kind, OpKind::Copy);
        assert_eq!(cloned.sources.len(), 1);
    }
}
