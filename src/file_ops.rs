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
