use std::fs;
use std::path::{Path, PathBuf};

/// A completed file operation that can be undone
#[derive(Debug, Clone)]
pub enum UndoAction {
    /// File was copied from src to dest — undo = delete dest
    Copy { dest: PathBuf },
    /// File was moved from src to dest — undo = move back
    Move { src: PathBuf, dest: PathBuf },
    /// File was renamed from old to new (same directory)
    Rename {
        old_path: PathBuf,
        new_path: PathBuf,
    },
    /// File was created at path — undo = delete
    CreateFile { path: PathBuf },
    /// Directory was created at path — undo = remove
    CreateDir { path: PathBuf },
}

impl UndoAction {
    pub fn description(&self) -> String {
        match self {
            Self::Copy { dest } => format!("Copy → {}", dest.display()),
            Self::Move { src, dest } => {
                format!("{} → {}", src.display(), dest.display())
            }
            Self::Rename { old_path, new_path } => {
                let old_name = old_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let new_name = new_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                format!("Rename {old_name} → {new_name}")
            }
            Self::CreateFile { path } => format!("Create {}", path.display()),
            Self::CreateDir { path } => format!("Create dir {}", path.display()),
        }
    }
}

/// Undo/redo stack
#[derive(Debug, Default)]
pub struct UndoStack {
    undo: Vec<UndoAction>,
    redo: Vec<UndoAction>,
    max_size: usize,
}

impl UndoStack {
    pub fn new() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            max_size: 100,
        }
    }

    /// Record a completed action (clears redo stack)
    pub fn push(&mut self, action: UndoAction) {
        self.undo.push(action);
        self.redo.clear();
        if self.undo.len() > self.max_size {
            self.undo.remove(0);
        }
    }

    /// Undo the last action. Returns description on success.
    pub fn undo(&mut self) -> Result<String, String> {
        let action = self.undo.pop().ok_or("Nothing to undo")?;
        let desc = action.description();
        let reverse = perform_undo(&action)?;
        self.redo.push(reverse);
        Ok(format!("Undo: {desc}"))
    }

    /// Redo the last undone action. Returns description on success.
    pub fn redo(&mut self) -> Result<String, String> {
        let action = self.redo.pop().ok_or("Nothing to redo")?;
        let desc = action.description();
        let reverse = perform_undo(&action)?;
        self.undo.push(reverse);
        Ok(format!("Redo: {desc}"))
    }

    #[allow(dead_code)]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    #[allow(dead_code)]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_count(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_count(&self) -> usize {
        self.redo.len()
    }

    #[allow(dead_code)]
    pub fn last_undo_desc(&self) -> Option<String> {
        self.undo.last().map(|a| a.description())
    }
}

/// Perform the reverse of an action. Returns the reverse action for redo.
fn perform_undo(action: &UndoAction) -> Result<UndoAction, String> {
    match action {
        UndoAction::Copy { dest } => {
            // Undo copy = delete the copy
            if dest.is_dir() {
                fs::remove_dir_all(dest)
                    .map_err(|e| format!("Failed to remove {}: {e}", dest.display()))?;
            } else {
                fs::remove_file(dest)
                    .map_err(|e| format!("Failed to remove {}: {e}", dest.display()))?;
            }
            Ok(UndoAction::CreateFile { path: dest.clone() })
        }
        UndoAction::Move { src, dest } => {
            // Undo move = move it back
            fs::rename(dest, src).map_err(|e| format!("Failed to move back: {e}"))?;
            Ok(UndoAction::Move {
                src: dest.clone(),
                dest: src.clone(),
            })
        }
        UndoAction::Rename { old_path, new_path } => {
            fs::rename(new_path, old_path).map_err(|e| format!("Failed to rename back: {e}"))?;
            Ok(UndoAction::Rename {
                old_path: new_path.clone(),
                new_path: old_path.clone(),
            })
        }
        UndoAction::CreateFile { path } => {
            if path.exists() {
                fs::remove_file(path).map_err(|e| format!("Failed to remove: {e}"))?;
            }
            Ok(UndoAction::CreateFile { path: path.clone() })
        }
        UndoAction::CreateDir { path } => {
            if path.exists() {
                fs::remove_dir(path).map_err(|e| format!("Failed to remove dir: {e}"))?;
            }
            Ok(UndoAction::CreateDir { path: path.clone() })
        }
    }
}

/// Helper: record a copy operation
pub fn record_copy(dest: &Path) -> UndoAction {
    UndoAction::Copy {
        dest: dest.to_path_buf(),
    }
}

/// Helper: record a move operation
pub fn record_move(src: &Path, dest: &Path) -> UndoAction {
    UndoAction::Move {
        src: src.to_path_buf(),
        dest: dest.to_path_buf(),
    }
}

/// Helper: record a rename
pub fn record_rename(old_path: &Path, new_path: &Path) -> UndoAction {
    UndoAction::Rename {
        old_path: old_path.to_path_buf(),
        new_path: new_path.to_path_buf(),
    }
}

/// Helper: record file creation
pub fn record_create_file(path: &Path) -> UndoAction {
    UndoAction::CreateFile {
        path: path.to_path_buf(),
    }
}

/// Helper: record dir creation
pub fn record_create_dir(path: &Path) -> UndoAction {
    UndoAction::CreateDir {
        path: path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_undo_stack_push_and_count() {
        let mut stack = UndoStack::new();
        assert_eq!(stack.undo_count(), 0);
        assert!(!stack.can_undo());
        stack.push(record_create_file(Path::new("/tmp/x")));
        assert_eq!(stack.undo_count(), 1);
        assert!(stack.can_undo());
    }

    #[test]
    fn test_undo_clears_redo() {
        let mut stack = UndoStack::new();
        // We can't easily test full undo/redo without real files,
        // but we can test the redo-clearing behavior
        stack.push(record_create_file(Path::new("/tmp/x")));
        // Manually add to redo
        stack.redo.push(record_create_file(Path::new("/tmp/y")));
        assert!(stack.can_redo());
        // New push clears redo
        stack.push(record_create_file(Path::new("/tmp/z")));
        assert!(!stack.can_redo());
    }

    #[test]
    fn test_undo_rename() {
        let tmp = TempDir::new().unwrap();
        let old = tmp.path().join("old.txt");
        let new = tmp.path().join("new.txt");
        fs::write(&new, "data").unwrap();

        let mut stack = UndoStack::new();
        stack.push(record_rename(&old, &new));
        let result = stack.undo();
        assert!(result.is_ok());
        assert!(old.exists());
        assert!(!new.exists());
        assert!(stack.can_redo());
    }

    #[test]
    fn test_undo_create_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("created.txt");
        fs::write(&path, "").unwrap();

        let mut stack = UndoStack::new();
        stack.push(record_create_file(&path));
        let result = stack.undo();
        assert!(result.is_ok());
        assert!(!path.exists());
    }

    #[test]
    fn test_undo_create_dir() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("newdir");
        fs::create_dir(&path).unwrap();

        let mut stack = UndoStack::new();
        stack.push(record_create_dir(&path));
        let result = stack.undo();
        assert!(result.is_ok());
        assert!(!path.exists());
    }

    #[test]
    fn test_undo_move() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src.txt");
        let dest = tmp.path().join("dest.txt");
        fs::write(&dest, "moved").unwrap();

        let mut stack = UndoStack::new();
        stack.push(record_move(&src, &dest));
        let result = stack.undo();
        assert!(result.is_ok());
        assert!(src.exists());
        assert!(!dest.exists());
    }

    #[test]
    fn test_undo_copy() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("copy.txt");
        fs::write(&dest, "copied").unwrap();

        let mut stack = UndoStack::new();
        stack.push(record_copy(&dest));
        let result = stack.undo();
        assert!(result.is_ok());
        assert!(!dest.exists());
    }

    #[test]
    fn test_redo_rename() {
        let tmp = TempDir::new().unwrap();
        let old = tmp.path().join("old.txt");
        let new = tmp.path().join("new.txt");
        fs::write(&new, "data").unwrap();

        let mut stack = UndoStack::new();
        stack.push(record_rename(&old, &new));
        stack.undo().unwrap();
        assert!(old.exists());
        // Now redo
        let result = stack.redo();
        assert!(result.is_ok());
        assert!(new.exists());
        assert!(!old.exists());
    }

    #[test]
    fn test_nothing_to_undo() {
        let mut stack = UndoStack::new();
        assert!(stack.undo().is_err());
    }

    #[test]
    fn test_nothing_to_redo() {
        let mut stack = UndoStack::new();
        assert!(stack.redo().is_err());
    }

    #[test]
    fn test_max_size() {
        let mut stack = UndoStack::new();
        for i in 0..150 {
            stack.push(record_create_file(&PathBuf::from(format!("/tmp/{i}"))));
        }
        assert!(stack.undo_count() <= 100);
    }

    #[test]
    fn test_action_descriptions() {
        let a = record_copy(Path::new("/tmp/file.txt"));
        assert!(a.description().contains("Copy"));

        let a = record_move(Path::new("/a"), Path::new("/b"));
        assert!(a.description().contains("/a"));

        let a = record_rename(Path::new("/dir/old.txt"), Path::new("/dir/new.txt"));
        assert!(a.description().contains("Rename"));

        let a = record_create_file(Path::new("/tmp/x"));
        assert!(a.description().contains("Create"));

        let a = record_create_dir(Path::new("/tmp/d"));
        assert!(a.description().contains("dir"));
    }

    #[test]
    fn test_last_undo_desc() {
        let mut stack = UndoStack::new();
        assert!(stack.last_undo_desc().is_none());
        stack.push(record_create_file(Path::new("/tmp/x")));
        assert!(stack.last_undo_desc().is_some());
    }
}
