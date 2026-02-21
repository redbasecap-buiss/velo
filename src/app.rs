use crate::config::{Config, SortBy};
use crate::file_ops::{self, OpKind, PendingOp};
use crate::git_status::{self, GitFileStatus};
use crate::preview::{self, PreviewLine};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub symlink_target: Option<String>,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub git_status: Option<GitFileStatus>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Filter,
    Rename,
    CreateFile,
    CreateDir,
    Bookmark,
    JumpBookmark,
}

pub struct App {
    pub current_dir: PathBuf,
    pub entries: Vec<FileEntry>,
    pub filtered_entries: Vec<usize>,
    pub cursor: usize,
    pub parent_entries: Vec<FileEntry>,
    pub parent_cursor: usize,
    pub preview_lines: Vec<PreviewLine>,
    pub config: Config,
    pub show_hidden: bool,
    pub sort_by: SortBy,
    pub selected: HashSet<PathBuf>,
    pub pending_op: Option<PendingOp>,
    pub input_mode: InputMode,
    pub input_buffer: String,
    pub status_message: Option<String>,
    pub bookmarks: HashMap<char, PathBuf>,
    pub git_statuses: HashMap<String, GitFileStatus>,
    pub pending_g: bool,
    pub pending_d: bool,
    pub pending_y: bool,
    pub pending_p: bool,
}

impl App {
    pub fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let current_dir = std::env::current_dir()?;
        Self::with_dir(config, current_dir)
    }

    pub fn with_dir(
        config: Config,
        current_dir: PathBuf,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let show_hidden = config.show_hidden;
        let sort_by = config.sort_by;
        let mut app = Self {
            current_dir: current_dir.clone(),
            entries: Vec::new(),
            filtered_entries: Vec::new(),
            cursor: 0,
            parent_entries: Vec::new(),
            parent_cursor: 0,
            preview_lines: Vec::new(),
            config,
            show_hidden,
            sort_by,
            selected: HashSet::new(),
            pending_op: None,
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
            status_message: None,
            bookmarks: HashMap::new(),
            git_statuses: HashMap::new(),
            pending_g: false,
            pending_d: false,
            pending_y: false,
            pending_p: false,
        };
        app.refresh()?;
        Ok(app)
    }

    pub fn refresh(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.entries = read_dir(&self.current_dir, self.show_hidden)?;
        self.sort_entries();
        self.git_statuses = git_status::get_git_statuses(&self.current_dir);
        for entry in &mut self.entries {
            entry.git_status = self.git_statuses.get(&entry.name).copied();
        }
        self.apply_filter();

        // Parent
        if let Some(parent) = self.current_dir.parent() {
            self.parent_entries = read_dir(parent, self.show_hidden).unwrap_or_default();
            self.parent_entries.sort_by(|a, b| {
                b.is_dir
                    .cmp(&a.is_dir)
                    .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
            self.parent_cursor = self
                .parent_entries
                .iter()
                .position(|e| e.path == self.current_dir)
                .unwrap_or(0);
        } else {
            self.parent_entries.clear();
            self.parent_cursor = 0;
        }

        self.update_preview();
        Ok(())
    }

    fn sort_entries(&mut self) {
        let sort_by = self.sort_by;
        self.entries.sort_by(|a, b| {
            // Dirs first always
            let dir_cmp = b.is_dir.cmp(&a.is_dir);
            if dir_cmp != std::cmp::Ordering::Equal {
                return dir_cmp;
            }
            match sort_by {
                SortBy::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortBy::Size => b.size.cmp(&a.size),
                SortBy::Date => b.modified.cmp(&a.modified),
                SortBy::Extension => {
                    let ext_a = Path::new(&a.name)
                        .extension()
                        .map(|e| e.to_string_lossy().to_lowercase())
                        .unwrap_or_default();
                    let ext_b = Path::new(&b.name)
                        .extension()
                        .map(|e| e.to_string_lossy().to_lowercase())
                        .unwrap_or_default();
                    ext_a
                        .cmp(&ext_b)
                        .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                }
            }
        });
    }

    fn apply_filter(&mut self) {
        if self.input_mode == InputMode::Filter && !self.input_buffer.is_empty() {
            let matcher = SkimMatcherV2::default();
            let query = &self.input_buffer;
            self.filtered_entries = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| matcher.fuzzy_match(&e.name, query).is_some())
                .map(|(i, _)| i)
                .collect();
        } else {
            self.filtered_entries = (0..self.entries.len()).collect();
        }
        if self.cursor >= self.filtered_entries.len() {
            self.cursor = self.filtered_entries.len().saturating_sub(1);
        }
    }

    pub fn visible_entries(&self) -> Vec<&FileEntry> {
        self.filtered_entries
            .iter()
            .filter_map(|&i| self.entries.get(i))
            .collect()
    }

    pub fn selected_entry(&self) -> Option<&FileEntry> {
        self.filtered_entries
            .get(self.cursor)
            .and_then(|&i| self.entries.get(i))
    }

    fn update_preview(&mut self) {
        if let Some(entry) = self.selected_entry() {
            self.preview_lines = preview::preview_path(&entry.path);
        } else {
            self.preview_lines.clear();
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Result<bool, Box<dyn std::error::Error>> {
        // Ctrl+C / q in normal mode => quit
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Ok(true);
        }

        match self.input_mode {
            InputMode::Normal => self.handle_normal_key(key),
            InputMode::Filter => self.handle_filter_key(key),
            InputMode::Rename | InputMode::CreateFile | InputMode::CreateDir => {
                self.handle_input_key(key)
            }
            InputMode::Bookmark => self.handle_bookmark_key(key),
            InputMode::JumpBookmark => self.handle_jump_bookmark_key(key),
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) -> Result<bool, Box<dyn std::error::Error>> {
        self.status_message = None;

        // Handle pending sequences
        if self.pending_g {
            self.pending_g = false;
            if key.code == KeyCode::Char('g') {
                self.cursor = 0;
                self.update_preview();
            }
            return Ok(false);
        }
        if self.pending_d {
            self.pending_d = false;
            if key.code == KeyCode::Char('d') {
                self.delete_selected()?;
            }
            return Ok(false);
        }
        if self.pending_y {
            self.pending_y = false;
            if key.code == KeyCode::Char('y') {
                self.yank_selected();
            }
            return Ok(false);
        }
        if self.pending_p {
            self.pending_p = false;
            if key.code == KeyCode::Char('p') {
                self.paste()?;
            }
            return Ok(false);
        }

        match key.code {
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Char('j') | KeyCode::Down => {
                if self.cursor < self.filtered_entries.len().saturating_sub(1) {
                    self.cursor += 1;
                    self.update_preview();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.update_preview();
                }
            }
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => {
                if let Some(entry) = self.selected_entry().cloned() {
                    if entry.is_dir {
                        self.current_dir = entry.path;
                        self.cursor = 0;
                        self.refresh()?;
                    } else {
                        let _ = open::that(&entry.path);
                    }
                }
            }
            KeyCode::Char('h') | KeyCode::Left => {
                if let Some(parent) = self.current_dir.parent().map(|p| p.to_path_buf()) {
                    let old_name = self
                        .current_dir
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string());
                    self.current_dir = parent;
                    self.cursor = 0;
                    self.refresh()?;
                    if let Some(name) = old_name {
                        if let Some(pos) =
                            self.visible_entries().iter().position(|e| e.name == name)
                        {
                            self.cursor = pos;
                            self.update_preview();
                        }
                    }
                }
            }
            KeyCode::Char('g') => self.pending_g = true,
            KeyCode::Char('G') => {
                self.cursor = self.filtered_entries.len().saturating_sub(1);
                self.update_preview();
            }
            KeyCode::Char('/') => {
                self.input_mode = InputMode::Filter;
                self.input_buffer.clear();
            }
            KeyCode::Char('d') => self.pending_d = true,
            KeyCode::Char('y') => self.pending_y = true,
            KeyCode::Char('p') => self.pending_p = true,
            KeyCode::Char(' ') => {
                if let Some(entry) = self.selected_entry().cloned() {
                    if self.selected.contains(&entry.path) {
                        self.selected.remove(&entry.path);
                    } else {
                        self.selected.insert(entry.path);
                    }
                    // Move cursor down
                    if self.cursor < self.filtered_entries.len().saturating_sub(1) {
                        self.cursor += 1;
                        self.update_preview();
                    }
                }
            }
            KeyCode::Char('s') => {
                self.sort_by = match self.sort_by {
                    SortBy::Name => SortBy::Size,
                    SortBy::Size => SortBy::Date,
                    SortBy::Date => SortBy::Extension,
                    SortBy::Extension => SortBy::Name,
                };
                self.status_message = Some(format!("Sort: {:?}", self.sort_by));
                self.refresh()?;
            }
            KeyCode::Char('.') => {
                self.show_hidden = !self.show_hidden;
                self.refresh()?;
            }
            KeyCode::Char('r') => {
                if self.selected_entry().is_some() {
                    self.input_mode = InputMode::Rename;
                    self.input_buffer = self
                        .selected_entry()
                        .map(|e| e.name.clone())
                        .unwrap_or_default();
                }
            }
            KeyCode::Char('n') => {
                self.input_mode = InputMode::CreateFile;
                self.input_buffer.clear();
                self.status_message = Some("New file: ".to_string());
            }
            KeyCode::Char('N') => {
                self.input_mode = InputMode::CreateDir;
                self.input_buffer.clear();
                self.status_message = Some("New directory: ".to_string());
            }
            KeyCode::Char('m') => {
                self.input_mode = InputMode::Bookmark;
                self.status_message = Some("Bookmark key: ".to_string());
            }
            KeyCode::Char('\'') => {
                self.input_mode = InputMode::JumpBookmark;
                self.status_message = Some("Jump to bookmark: ".to_string());
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_filter_key(&mut self, key: KeyEvent) -> Result<bool, Box<dyn std::error::Error>> {
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.input_buffer.clear();
                self.apply_filter();
                self.update_preview();
            }
            KeyCode::Enter => {
                self.input_mode = InputMode::Normal;
                // Keep filter applied, enter selected
                if let Some(entry) = self.selected_entry().cloned() {
                    if entry.is_dir {
                        self.current_dir = entry.path;
                        self.cursor = 0;
                        self.input_buffer.clear();
                        self.refresh()?;
                    }
                }
            }
            KeyCode::Backspace => {
                self.input_buffer.pop();
                self.apply_filter();
                self.update_preview();
            }
            KeyCode::Char(c) => {
                self.input_buffer.push(c);
                self.apply_filter();
                self.update_preview();
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_input_key(&mut self, key: KeyEvent) -> Result<bool, Box<dyn std::error::Error>> {
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.input_buffer.clear();
                self.status_message = None;
            }
            KeyCode::Enter => {
                let name = self.input_buffer.clone();
                let mode = std::mem::replace(&mut self.input_mode, InputMode::Normal);
                self.input_buffer.clear();
                match mode {
                    InputMode::Rename => {
                        if let Some(entry) = self.selected_entry() {
                            match file_ops::rename_file(&entry.path, &name) {
                                Ok(_) => self.status_message = Some("Renamed".to_string()),
                                Err(e) => self.status_message = Some(format!("Error: {e}")),
                            }
                        }
                    }
                    InputMode::CreateFile => {
                        match file_ops::create_file(&self.current_dir, &name) {
                            Ok(_) => self.status_message = Some("File created".to_string()),
                            Err(e) => self.status_message = Some(format!("Error: {e}")),
                        }
                    }
                    InputMode::CreateDir => match file_ops::create_dir(&self.current_dir, &name) {
                        Ok(_) => self.status_message = Some("Directory created".to_string()),
                        Err(e) => self.status_message = Some(format!("Error: {e}")),
                    },
                    _ => {}
                }
                self.refresh()?;
            }
            KeyCode::Backspace => {
                self.input_buffer.pop();
            }
            KeyCode::Char(c) => {
                self.input_buffer.push(c);
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_bookmark_key(&mut self, key: KeyEvent) -> Result<bool, Box<dyn std::error::Error>> {
        self.input_mode = InputMode::Normal;
        if let KeyCode::Char(c) = key.code {
            self.bookmarks.insert(c, self.current_dir.clone());
            self.status_message = Some(format!("Bookmark '{c}' set"));
        }
        Ok(false)
    }

    fn handle_jump_bookmark_key(
        &mut self,
        key: KeyEvent,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        self.input_mode = InputMode::Normal;
        if let KeyCode::Char(c) = key.code {
            if let Some(path) = self.bookmarks.get(&c).cloned() {
                if path.is_dir() {
                    self.current_dir = path;
                    self.cursor = 0;
                    self.refresh()?;
                } else {
                    self.status_message = Some(format!("Bookmark '{c}' no longer exists"));
                }
            } else {
                self.status_message = Some(format!("No bookmark '{c}'"));
            }
        }
        Ok(false)
    }

    fn delete_selected(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.selected.is_empty() {
            if let Some(entry) = self.selected_entry() {
                match file_ops::delete_to_trash(&entry.path) {
                    Ok(_) => self.status_message = Some("Deleted to trash".to_string()),
                    Err(e) => self.status_message = Some(format!("Error: {e}")),
                }
            }
        } else {
            let paths: Vec<_> = self.selected.drain().collect();
            let mut count = 0;
            for p in &paths {
                if file_ops::delete_to_trash(p).is_ok() {
                    count += 1;
                }
            }
            self.status_message = Some(format!("Deleted {count} items to trash"));
        }
        self.refresh()?;
        Ok(())
    }

    fn yank_selected(&mut self) {
        let sources = if self.selected.is_empty() {
            self.selected_entry()
                .map(|e| vec![e.path.clone()])
                .unwrap_or_default()
        } else {
            self.selected.iter().cloned().collect()
        };
        let count = sources.len();
        self.pending_op = Some(PendingOp {
            kind: OpKind::Copy,
            sources,
        });
        self.status_message = Some(format!("Yanked {count} item(s)"));
    }

    fn paste(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(op) = self.pending_op.take() {
            let mut count = 0;
            for src in &op.sources {
                let result = match op.kind {
                    OpKind::Copy => file_ops::copy_file(src, &self.current_dir),
                    OpKind::Move => file_ops::move_file(src, &self.current_dir),
                };
                if result.is_ok() {
                    count += 1;
                }
            }
            self.selected.clear();
            self.status_message = Some(format!("Pasted {count} item(s)"));
            self.refresh()?;
        } else {
            self.status_message = Some("Nothing to paste".to_string());
        }
        Ok(())
    }

    pub fn file_count(&self) -> usize {
        self.filtered_entries.len()
    }

    pub fn selection_count(&self) -> usize {
        self.selected.len()
    }

    pub fn breadcrumb(&self) -> String {
        self.current_dir.display().to_string()
    }
}

fn read_dir(path: &Path, show_hidden: bool) -> Result<Vec<FileEntry>, Box<dyn std::error::Error>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let name = entry.file_name().to_string_lossy().to_string();
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let is_symlink = entry.file_type().map(|t| t.is_symlink()).unwrap_or(false);
        let symlink_target = if is_symlink {
            fs::read_link(entry.path())
                .ok()
                .map(|p| p.display().to_string())
        } else {
            None
        };
        entries.push(FileEntry {
            name,
            path: entry.path(),
            is_dir: metadata.is_dir(),
            is_symlink,
            symlink_target,
            size: metadata.len(),
            modified: metadata.modified().ok(),
            git_status: None,
        });
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_app(tmp: &TempDir) -> App {
        let dir = tmp.path().canonicalize().unwrap();
        App::with_dir(Config::default(), dir).unwrap()
    }

    #[test]
    fn test_app_new() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().canonicalize().unwrap();
        fs::write(dir.join("a.txt"), "hello").unwrap();
        let app = make_app(&tmp);
        assert_eq!(app.current_dir, dir);
        assert!(!app.entries.is_empty());
    }

    #[test]
    fn test_read_dir_hidden() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join(".hidden"), "").unwrap();
        fs::write(tmp.path().join("visible"), "").unwrap();
        let entries = read_dir(tmp.path(), false).unwrap();
        assert!(entries.iter().all(|e| !e.name.starts_with('.')));
        let entries = read_dir(tmp.path(), true).unwrap();
        assert!(entries.iter().any(|e| e.name == ".hidden"));
    }

    #[test]
    fn test_sort_by_name() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("b.txt"), "").unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        let app = make_app(&tmp);
        let names: Vec<_> = app.entries.iter().map(|e| &e.name).collect();
        assert_eq!(names, vec!["a.txt", "b.txt"]);
    }

    #[test]
    fn test_sort_cycle() {
        let tmp = TempDir::new().unwrap();
        let mut app = make_app(&tmp);
        assert_eq!(app.sort_by, SortBy::Name);
        app.sort_by = match app.sort_by {
            SortBy::Name => SortBy::Size,
            _ => SortBy::Name,
        };
        assert_eq!(app.sort_by, SortBy::Size);
    }

    #[test]
    fn test_toggle_hidden() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join(".hidden"), "").unwrap();
        let mut app = make_app(&tmp);
        assert!(!app.show_hidden);
        app.show_hidden = true;
        app.refresh().unwrap();
        assert!(app.entries.iter().any(|e| e.name == ".hidden"));
    }

    #[test]
    fn test_filter() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("apple.txt"), "").unwrap();
        fs::write(tmp.path().join("banana.txt"), "").unwrap();
        let mut app = make_app(&tmp);
        app.input_mode = InputMode::Filter;
        app.input_buffer = "app".to_string();
        app.apply_filter();
        assert_eq!(app.filtered_entries.len(), 1);
    }

    #[test]
    fn test_breadcrumb() {
        let tmp = TempDir::new().unwrap();
        let app = make_app(&tmp);
        assert!(app.breadcrumb().contains(tmp.path().to_str().unwrap()));
    }

    #[test]
    fn test_selection() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        let mut app = make_app(&tmp);
        let path = app.entries[0].path.clone();
        app.selected.insert(path.clone());
        assert_eq!(app.selection_count(), 1);
        app.selected.remove(&path);
        assert_eq!(app.selection_count(), 0);
    }

    #[test]
    fn test_file_count() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        fs::write(tmp.path().join("b.txt"), "").unwrap();
        let app = make_app(&tmp);
        assert_eq!(app.file_count(), 2);
    }

    #[test]
    fn test_bookmarks() {
        let tmp = TempDir::new().unwrap();
        let mut app = make_app(&tmp);
        app.bookmarks.insert('a', tmp.path().to_path_buf());
        assert_eq!(app.bookmarks.get(&'a'), Some(&tmp.path().to_path_buf()));
    }

    #[test]
    fn test_yank_and_paste() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "content").unwrap();
        let dest = tmp.path().join("sub");
        fs::create_dir(&dest).unwrap();
        let mut app = make_app(&tmp);
        app.yank_selected();
        assert!(app.pending_op.is_some());
    }

    #[test]
    fn test_input_mode_eq() {
        assert_eq!(InputMode::Normal, InputMode::Normal);
        assert_ne!(InputMode::Normal, InputMode::Filter);
    }

    #[test]
    fn test_file_entry_symlink() {
        let entry = FileEntry {
            name: "link".to_string(),
            path: PathBuf::from("/tmp/link"),
            is_dir: false,
            is_symlink: true,
            symlink_target: Some("/tmp/target".to_string()),
            size: 0,
            modified: None,
            git_status: None,
        };
        assert!(entry.is_symlink);
        assert_eq!(entry.symlink_target.as_deref(), Some("/tmp/target"));
    }
}
