use crate::config::{Config, SortBy};
use crate::file_ops::{self, OpKind, PendingOp, SearchResult};
use crate::git_status::{self, GitFileStatus};
use crate::preview::{self, PreviewLine};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::collections::{HashMap, HashSet};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
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
    Chmod,
    Search,
    SearchResults,
}

/// A node in the tree view
#[derive(Debug, Clone)]
pub struct TreeNode {
    pub entry: FileEntry,
    pub depth: usize,
    pub expanded: bool,
    pub has_children: bool,
}

/// Per-tab state
#[derive(Debug, Clone)]
pub struct Tab {
    pub current_dir: PathBuf,
    pub entries: Vec<FileEntry>,
    pub filtered_entries: Vec<usize>,
    pub cursor: usize,
    pub parent_entries: Vec<FileEntry>,
    pub parent_cursor: usize,
    pub preview_lines: Vec<PreviewLine>,
    pub show_hidden: bool,
    pub sort_by: SortBy,
    pub selected: HashSet<PathBuf>,
    pub git_statuses: HashMap<String, GitFileStatus>,
    pub filter_text: String,
    pub tree_mode: bool,
    pub tree_nodes: Vec<TreeNode>,
    pub tree_cursor: usize,
    pub tree_expanded: HashSet<PathBuf>,
}

impl Tab {
    pub fn new(
        dir: PathBuf,
        show_hidden: bool,
        sort_by: SortBy,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut tab = Self {
            current_dir: dir,
            entries: Vec::new(),
            filtered_entries: Vec::new(),
            cursor: 0,
            parent_entries: Vec::new(),
            parent_cursor: 0,
            preview_lines: Vec::new(),
            show_hidden,
            sort_by,
            selected: HashSet::new(),
            git_statuses: HashMap::new(),
            filter_text: String::new(),
            tree_mode: false,
            tree_nodes: Vec::new(),
            tree_cursor: 0,
            tree_expanded: HashSet::new(),
        };
        tab.refresh()?;
        Ok(tab)
    }

    pub fn refresh(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.entries = read_dir(&self.current_dir, self.show_hidden)?;
        self.sort_entries();
        self.git_statuses = git_status::get_git_statuses(&self.current_dir);
        for entry in &mut self.entries {
            entry.git_status = self.git_statuses.get(&entry.name).copied();
        }
        self.apply_filter();

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

    pub fn apply_filter(&mut self) {
        if !self.filter_text.is_empty() {
            let matcher = SkimMatcherV2::default();
            let query = &self.filter_text;
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

    pub fn update_preview(&mut self) {
        if let Some(entry) = self.selected_entry() {
            self.preview_lines = preview::preview_path(&entry.path);
        } else {
            self.preview_lines.clear();
        }
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

    pub fn toggle_tree_mode(&mut self) {
        self.tree_mode = !self.tree_mode;
        if self.tree_mode {
            self.rebuild_tree();
        }
    }

    pub fn rebuild_tree(&mut self) {
        self.tree_nodes.clear();
        self.build_tree_recursive(&self.current_dir.clone(), 0);
        if self.tree_cursor >= self.tree_nodes.len() {
            self.tree_cursor = self.tree_nodes.len().saturating_sub(1);
        }
        self.update_preview_for_tree();
    }

    fn build_tree_recursive(&mut self, dir: &Path, depth: usize) {
        let mut entries = read_dir(dir, self.show_hidden).unwrap_or_default();
        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        for entry in entries {
            let is_dir = entry.is_dir;
            let path = entry.path.clone();
            let expanded = self.tree_expanded.contains(&path);
            let has_children = is_dir
                && fs::read_dir(&path)
                    .map(|mut rd| rd.next().is_some())
                    .unwrap_or(false);
            self.tree_nodes.push(TreeNode {
                entry,
                depth,
                expanded,
                has_children,
            });
            if is_dir && expanded && depth < 5 {
                self.build_tree_recursive(&path, depth + 1);
            }
        }
    }

    pub fn tree_toggle_expand(&mut self) {
        if let Some(node) = self.tree_nodes.get(self.tree_cursor) {
            if node.entry.is_dir {
                let path = node.entry.path.clone();
                if self.tree_expanded.contains(&path) {
                    self.tree_expanded.remove(&path);
                } else {
                    self.tree_expanded.insert(path);
                }
                self.rebuild_tree();
            }
        }
    }

    pub fn selected_tree_entry(&self) -> Option<&FileEntry> {
        self.tree_nodes.get(self.tree_cursor).map(|n| &n.entry)
    }

    fn update_preview_for_tree(&mut self) {
        if let Some(entry) = self.selected_tree_entry() {
            self.preview_lines = preview::preview_path(&entry.path);
        } else {
            self.preview_lines.clear();
        }
    }

    pub fn tab_title(&self) -> String {
        self.current_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "/".to_string())
    }
}

pub struct App {
    pub tabs: Vec<Tab>,
    pub active_tab: usize,
    #[allow(dead_code)]
    pub config: Config,
    pub pending_op: Option<PendingOp>,
    pub input_mode: InputMode,
    pub input_buffer: String,
    pub status_message: Option<String>,
    pub bookmarks: HashMap<char, PathBuf>,
    pub pending_g: bool,
    pub pending_d: bool,
    pub pending_y: bool,
    pub pending_p: bool,
    /// Layout areas for mouse hit-testing (set during draw)
    pub mouse_areas: MouseAreas,
    /// Recursive search results
    pub search_results: Vec<SearchResult>,
    /// Cursor position in search results
    pub search_cursor: usize,
}

#[derive(Debug, Clone, Default)]
pub struct MouseAreas {
    pub tab_bar: Option<(u16, u16, u16, u16)>, // x, y, w, h
    pub tab_positions: Vec<(u16, u16, usize)>, // x, width, tab_index
    pub current_pane: Option<(u16, u16, u16, u16)>, // x, y, w, h
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
        let tab = Tab::new(current_dir, show_hidden, sort_by)?;
        Ok(Self {
            tabs: vec![tab],
            active_tab: 0,
            config,
            pending_op: None,
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
            status_message: None,
            bookmarks: HashMap::new(),
            pending_g: false,
            pending_d: false,
            pending_y: false,
            pending_p: false,
            mouse_areas: MouseAreas::default(),
            search_results: Vec::new(),
            search_cursor: 0,
        })
    }

    /// Access the active tab
    pub fn tab(&self) -> &Tab {
        &self.tabs[self.active_tab]
    }

    /// Access the active tab mutably
    pub fn tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active_tab]
    }

    // Convenience delegations for backward compat in UI code
    #[allow(dead_code)]
    pub fn current_dir(&self) -> &Path {
        &self.tab().current_dir
    }

    #[allow(dead_code)]
    pub fn entries(&self) -> &[FileEntry] {
        &self.tab().entries
    }

    pub fn visible_entries(&self) -> Vec<&FileEntry> {
        self.tab().visible_entries()
    }

    pub fn selected_entry(&self) -> Option<&FileEntry> {
        self.tab().selected_entry()
    }

    pub fn cursor(&self) -> usize {
        self.tab().cursor
    }

    pub fn parent_entries(&self) -> &[FileEntry] {
        &self.tab().parent_entries
    }

    pub fn parent_cursor(&self) -> usize {
        self.tab().parent_cursor
    }

    pub fn preview_lines(&self) -> &[PreviewLine] {
        &self.tab().preview_lines
    }

    pub fn selected(&self) -> &HashSet<PathBuf> {
        &self.tab().selected
    }

    #[allow(dead_code)]
    pub fn git_statuses(&self) -> &HashMap<String, GitFileStatus> {
        &self.tab().git_statuses
    }

    pub fn file_count(&self) -> usize {
        self.tab().file_count()
    }

    pub fn selection_count(&self) -> usize {
        self.tab().selection_count()
    }

    pub fn breadcrumb(&self) -> String {
        self.tab().breadcrumb()
    }

    #[allow(dead_code)]
    pub fn show_hidden(&self) -> bool {
        self.tab().show_hidden
    }

    pub fn sort_by(&self) -> SortBy {
        self.tab().sort_by
    }

    #[allow(dead_code)]
    pub fn refresh(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.tab_mut().refresh()
    }

    /// Create a new tab in the same directory as current
    pub fn new_tab(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let dir = self.tab().current_dir.clone();
        let show_hidden = self.tab().show_hidden;
        let sort_by = self.tab().sort_by;
        let tab = Tab::new(dir, show_hidden, sort_by)?;
        self.active_tab += 1;
        self.tabs.insert(self.active_tab, tab);
        Ok(())
    }

    /// Close the current tab. Returns true if the app should quit (last tab closed).
    pub fn close_tab(&mut self) -> bool {
        if self.tabs.len() <= 1 {
            return true; // quit
        }
        self.tabs.remove(self.active_tab);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        false
    }

    pub fn next_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active_tab = (self.active_tab + 1) % self.tabs.len();
        }
    }

    pub fn prev_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active_tab = if self.active_tab == 0 {
                self.tabs.len() - 1
            } else {
                self.active_tab - 1
            };
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Result<bool, Box<dyn std::error::Error>> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Ok(true);
        }

        // Tab keybinds (work in normal mode)
        if self.input_mode == InputMode::Normal && key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('y') => {
                    let entry = if self.tab().tree_mode {
                        self.tab().selected_tree_entry().cloned()
                    } else {
                        self.tab().selected_entry().cloned()
                    };
                    if let Some(entry) = entry {
                        match file_ops::copy_content_to_clipboard(&entry.path) {
                            Ok(_) => {
                                self.status_message =
                                    Some(format!("Content copied: {}", entry.name))
                            }
                            Err(e) => self.status_message = Some(format!("Error: {e}")),
                        }
                    }
                    return Ok(false);
                }
                KeyCode::Char('t') => {
                    self.new_tab()?;
                    self.status_message = Some(format!("Tab {} opened", self.active_tab + 1));
                    return Ok(false);
                }
                KeyCode::Char('w') => {
                    if self.close_tab() {
                        return Ok(true);
                    }
                    self.status_message =
                        Some(format!("Tab closed ({} remaining)", self.tabs.len()));
                    return Ok(false);
                }
                KeyCode::Right => {
                    self.next_tab();
                    return Ok(false);
                }
                KeyCode::Left => {
                    self.prev_tab();
                    return Ok(false);
                }
                _ => {}
            }
        }

        // Alt+1..9 to switch tabs
        if self.input_mode == InputMode::Normal && key.modifiers.contains(KeyModifiers::ALT) {
            if let KeyCode::Char(c @ '1'..='9') = key.code {
                let idx = (c as usize) - ('1' as usize);
                if idx < self.tabs.len() {
                    self.active_tab = idx;
                }
                return Ok(false);
            }
        }

        match self.input_mode {
            InputMode::Normal => self.handle_normal_key(key),
            InputMode::Filter => self.handle_filter_key(key),
            InputMode::Rename | InputMode::CreateFile | InputMode::CreateDir => {
                self.handle_input_key(key)
            }
            InputMode::Bookmark => self.handle_bookmark_key(key),
            InputMode::JumpBookmark => self.handle_jump_bookmark_key(key),
            InputMode::Chmod => self.handle_chmod_key(key),
            InputMode::Search => self.handle_search_key(key),
            InputMode::SearchResults => self.handle_search_results_key(key),
        }
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent) -> Result<bool, Box<dyn std::error::Error>> {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let col = mouse.column;
                let row = mouse.row;

                // Check tab bar clicks
                for &(tx, tw, idx) in &self.mouse_areas.tab_positions {
                    if row == 0 && col >= tx && col < tx + tw && idx < self.tabs.len() {
                        self.active_tab = idx;
                        return Ok(false);
                    }
                }

                // Check current pane clicks
                if let Some((px, py, pw, ph)) = self.mouse_areas.current_pane {
                    if col >= px && col < px + pw && row >= py && row < py + ph {
                        // +1 for border, calculate which file entry was clicked
                        let file_row = (row - py).saturating_sub(1) as usize; // -1 for top border
                        let tab = self.tab_mut();
                        if file_row < tab.filtered_entries.len() {
                            tab.cursor = file_row;
                            tab.update_preview();
                        }
                    }
                }
            }
            MouseEventKind::ScrollDown => {
                let tab = self.tab_mut();
                if tab.cursor < tab.filtered_entries.len().saturating_sub(1) {
                    tab.cursor += 1;
                    tab.update_preview();
                }
            }
            MouseEventKind::ScrollUp => {
                let tab = self.tab_mut();
                if tab.cursor > 0 {
                    tab.cursor -= 1;
                    tab.update_preview();
                }
            }
            MouseEventKind::Down(MouseButton::Right) => {
                // Right-click = toggle selection
                if let Some(entry) = self.tab().selected_entry().cloned() {
                    let tab = self.tab_mut();
                    if tab.selected.contains(&entry.path) {
                        tab.selected.remove(&entry.path);
                    } else {
                        tab.selected.insert(entry.path);
                    }
                }
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_tree_common_key(
        &mut self,
        key: KeyEvent,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        // Handle keys that work the same in tree mode as normal (t, Y, sort, etc.)
        match key.code {
            KeyCode::Char('t') => {
                self.tab_mut().toggle_tree_mode();
                let mode = if self.tab().tree_mode { "Tree" } else { "List" };
                self.status_message = Some(format!("View: {mode}"));
            }
            KeyCode::Char('Y') => {
                if let Some(entry) = self.tab().selected_tree_entry().cloned() {
                    match file_ops::copy_path_to_clipboard(&entry.path) {
                        Ok(_) => {
                            self.status_message =
                                Some(format!("Path copied: {}", entry.path.display()))
                        }
                        Err(e) => self.status_message = Some(format!("Clipboard error: {e}")),
                    }
                }
            }
            KeyCode::Char('.') => {
                let new_hidden = !self.tab().show_hidden;
                self.tab_mut().show_hidden = new_hidden;
                self.tab_mut().rebuild_tree();
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_normal_key(&mut self, key: KeyEvent) -> Result<bool, Box<dyn std::error::Error>> {
        self.status_message = None;

        if self.pending_g {
            self.pending_g = false;
            if key.code == KeyCode::Char('g') {
                if self.tab().tree_mode {
                    self.tab_mut().tree_cursor = 0;
                    self.tab_mut().update_preview_for_tree();
                } else {
                    self.tab_mut().cursor = 0;
                    self.tab_mut().update_preview();
                }
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

        // Tree mode navigation
        if self.tab().tree_mode {
            match key.code {
                KeyCode::Char('q') => return Ok(true),
                KeyCode::Char('j') | KeyCode::Down => {
                    let tab = self.tab_mut();
                    if tab.tree_cursor < tab.tree_nodes.len().saturating_sub(1) {
                        tab.tree_cursor += 1;
                        tab.update_preview_for_tree();
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    let tab = self.tab_mut();
                    if tab.tree_cursor > 0 {
                        tab.tree_cursor -= 1;
                        tab.update_preview_for_tree();
                    }
                }
                KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => {
                    if let Some(entry) = self.tab().selected_tree_entry().cloned() {
                        if entry.is_dir {
                            self.tab_mut().tree_toggle_expand();
                        } else {
                            let _ = open::that(&entry.path);
                        }
                    }
                }
                KeyCode::Char('h') | KeyCode::Left => {
                    // Collapse current dir or go to parent node
                    if let Some(node) = self.tab().tree_nodes.get(self.tab().tree_cursor).cloned() {
                        if node.entry.is_dir && node.expanded {
                            self.tab_mut().tree_toggle_expand();
                        } else if node.depth > 0 {
                            // Find parent node
                            let tab = self.tab_mut();
                            for i in (0..tab.tree_cursor).rev() {
                                if tab.tree_nodes[i].depth < node.depth {
                                    tab.tree_cursor = i;
                                    tab.update_preview_for_tree();
                                    break;
                                }
                            }
                        }
                    }
                }
                KeyCode::Char('g') => self.pending_g = true,
                KeyCode::Char('G') => {
                    let len = self.tab().tree_nodes.len();
                    let tab = self.tab_mut();
                    tab.tree_cursor = len.saturating_sub(1);
                    tab.update_preview_for_tree();
                }
                KeyCode::Char(' ') => {
                    if let Some(entry) = self.tab().selected_tree_entry().cloned() {
                        let tab = self.tab_mut();
                        if tab.selected.contains(&entry.path) {
                            tab.selected.remove(&entry.path);
                        } else {
                            tab.selected.insert(entry.path);
                        }
                        if tab.tree_cursor < tab.tree_nodes.len().saturating_sub(1) {
                            tab.tree_cursor += 1;
                            tab.update_preview_for_tree();
                        }
                    }
                }
                _ => {
                    return self.handle_tree_common_key(key);
                }
            }
            return Ok(false);
        }

        match key.code {
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Char('j') | KeyCode::Down => {
                let tab = self.tab_mut();
                if tab.cursor < tab.filtered_entries.len().saturating_sub(1) {
                    tab.cursor += 1;
                    tab.update_preview();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let tab = self.tab_mut();
                if tab.cursor > 0 {
                    tab.cursor -= 1;
                    tab.update_preview();
                }
            }
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => {
                if let Some(entry) = self.tab().selected_entry().cloned() {
                    if entry.is_dir {
                        let tab = self.tab_mut();
                        tab.current_dir = entry.path;
                        tab.cursor = 0;
                        tab.refresh()?;
                    } else {
                        let _ = open::that(&entry.path);
                    }
                }
            }
            KeyCode::Char('h') | KeyCode::Left => {
                let parent = self.tab().current_dir.parent().map(|p| p.to_path_buf());
                if let Some(parent) = parent {
                    let old_name = self
                        .tab()
                        .current_dir
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string());
                    let tab = self.tab_mut();
                    tab.current_dir = parent;
                    tab.cursor = 0;
                    tab.refresh()?;
                    if let Some(name) = old_name {
                        if let Some(pos) = tab.visible_entries().iter().position(|e| e.name == name)
                        {
                            tab.cursor = pos;
                            tab.update_preview();
                        }
                    }
                }
            }
            KeyCode::Char('g') => self.pending_g = true,
            KeyCode::Char('G') => {
                let len = self.tab().filtered_entries.len();
                let tab = self.tab_mut();
                tab.cursor = len.saturating_sub(1);
                tab.update_preview();
            }
            KeyCode::Char('/') => {
                self.input_mode = InputMode::Filter;
                self.input_buffer.clear();
            }
            KeyCode::Char('d') => self.pending_d = true,
            KeyCode::Char('y') => self.pending_y = true,
            KeyCode::Char('p') => self.pending_p = true,
            KeyCode::Char(' ') => {
                if let Some(entry) = self.tab().selected_entry().cloned() {
                    let tab = self.tab_mut();
                    if tab.selected.contains(&entry.path) {
                        tab.selected.remove(&entry.path);
                    } else {
                        tab.selected.insert(entry.path);
                    }
                    if tab.cursor < tab.filtered_entries.len().saturating_sub(1) {
                        tab.cursor += 1;
                        tab.update_preview();
                    }
                }
            }
            KeyCode::Char('s') => {
                let new_sort = match self.tab().sort_by {
                    SortBy::Name => SortBy::Size,
                    SortBy::Size => SortBy::Date,
                    SortBy::Date => SortBy::Extension,
                    SortBy::Extension => SortBy::Name,
                };
                self.tab_mut().sort_by = new_sort;
                self.status_message = Some(format!("Sort: {new_sort:?}"));
                self.tab_mut().refresh()?;
            }
            KeyCode::Char('.') => {
                let new_hidden = !self.tab().show_hidden;
                self.tab_mut().show_hidden = new_hidden;
                self.tab_mut().refresh()?;
            }
            KeyCode::Char('r') => {
                if self.tab().selected_entry().is_some() {
                    self.input_mode = InputMode::Rename;
                    self.input_buffer = self
                        .tab()
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
            KeyCode::Char('c') => {
                #[cfg(unix)]
                if let Some(entry) = self.tab().selected_entry() {
                    if let Ok(meta) = std::fs::metadata(&entry.path) {
                        let mode = meta.permissions().mode() & 0o777;
                        self.input_buffer = format!("{:o}", mode);
                        self.input_mode = InputMode::Chmod;
                        self.status_message = Some(format!("chmod (octal): {}", self.input_buffer));
                    }
                }
                #[cfg(not(unix))]
                {
                    self.status_message = Some("chmod not supported on this platform".to_string());
                }
            }
            KeyCode::Char('F') => {
                self.input_mode = InputMode::Search;
                self.input_buffer.clear();
                self.status_message = Some("Search: ".to_string());
            }
            KeyCode::Char('t') => {
                self.tab_mut().toggle_tree_mode();
                let mode = if self.tab().tree_mode { "Tree" } else { "List" };
                self.status_message = Some(format!("View: {mode}"));
            }
            KeyCode::Char('Y') => {
                let entry = if self.tab().tree_mode {
                    self.tab().selected_tree_entry().cloned()
                } else {
                    self.tab().selected_entry().cloned()
                };
                if let Some(entry) = entry {
                    match file_ops::copy_path_to_clipboard(&entry.path) {
                        Ok(_) => {
                            self.status_message =
                                Some(format!("Path copied: {}", entry.path.display()))
                        }
                        Err(e) => self.status_message = Some(format!("Clipboard error: {e}")),
                    }
                }
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
                self.tab_mut().filter_text.clear();
                self.tab_mut().apply_filter();
                self.tab_mut().update_preview();
            }
            KeyCode::Enter => {
                self.input_mode = InputMode::Normal;
                if let Some(entry) = self.tab().selected_entry().cloned() {
                    if entry.is_dir {
                        let tab = self.tab_mut();
                        tab.current_dir = entry.path;
                        tab.cursor = 0;
                        tab.filter_text.clear();
                        self.input_buffer.clear();
                        self.tab_mut().refresh()?;
                    }
                }
            }
            KeyCode::Backspace => {
                self.input_buffer.pop();
                self.tab_mut().filter_text = self.input_buffer.clone();
                self.tab_mut().apply_filter();
                self.tab_mut().update_preview();
            }
            KeyCode::Char(c) => {
                self.input_buffer.push(c);
                self.tab_mut().filter_text = self.input_buffer.clone();
                self.tab_mut().apply_filter();
                self.tab_mut().update_preview();
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
                let current_dir = self.tab().current_dir.clone();
                match mode {
                    InputMode::Rename => {
                        if let Some(entry) = self.tab().selected_entry() {
                            match file_ops::rename_file(&entry.path, &name) {
                                Ok(_) => self.status_message = Some("Renamed".to_string()),
                                Err(e) => self.status_message = Some(format!("Error: {e}")),
                            }
                        }
                    }
                    InputMode::CreateFile => match file_ops::create_file(&current_dir, &name) {
                        Ok(_) => self.status_message = Some("File created".to_string()),
                        Err(e) => self.status_message = Some(format!("Error: {e}")),
                    },
                    InputMode::CreateDir => match file_ops::create_dir(&current_dir, &name) {
                        Ok(_) => self.status_message = Some("Directory created".to_string()),
                        Err(e) => self.status_message = Some(format!("Error: {e}")),
                    },
                    _ => {}
                }
                self.tab_mut().refresh()?;
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
            let dir = self.tab().current_dir.clone();
            self.bookmarks.insert(c, dir);
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
                    let tab = self.tab_mut();
                    tab.current_dir = path;
                    tab.cursor = 0;
                    tab.refresh()?;
                } else {
                    self.status_message = Some(format!("Bookmark '{c}' no longer exists"));
                }
            } else {
                self.status_message = Some(format!("No bookmark '{c}'"));
            }
        }
        Ok(false)
    }

    fn handle_chmod_key(&mut self, key: KeyEvent) -> Result<bool, Box<dyn std::error::Error>> {
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.input_buffer.clear();
                self.status_message = None;
            }
            KeyCode::Enter => {
                let mode_str = self.input_buffer.clone();
                self.input_mode = InputMode::Normal;
                self.input_buffer.clear();
                if let Some(entry) = self.tab().selected_entry() {
                    match file_ops::chmod_file(&entry.path, &mode_str) {
                        Ok(_) => self.status_message = Some(format!("chmod {mode_str} applied")),
                        Err(e) => self.status_message = Some(format!("chmod error: {e}")),
                    }
                }
            }
            KeyCode::Backspace => {
                self.input_buffer.pop();
                self.status_message = Some(format!("chmod (octal): {}", self.input_buffer));
            }
            KeyCode::Char(c) if c.is_ascii_digit() && c < '8' => {
                if self.input_buffer.len() < 4 {
                    self.input_buffer.push(c);
                    self.status_message = Some(format!("chmod (octal): {}", self.input_buffer));
                }
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_search_key(&mut self, key: KeyEvent) -> Result<bool, Box<dyn std::error::Error>> {
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.input_buffer.clear();
                self.status_message = None;
            }
            KeyCode::Enter => {
                let pattern = self.input_buffer.clone();
                self.input_buffer.clear();
                if pattern.is_empty() {
                    self.input_mode = InputMode::Normal;
                    self.status_message = None;
                } else {
                    let dir = self.tab().current_dir.clone();
                    self.search_results = file_ops::search_recursive(&dir, &pattern, 200);
                    self.search_cursor = 0;
                    if self.search_results.is_empty() {
                        self.input_mode = InputMode::Normal;
                        self.status_message = Some(format!("No results for \"{pattern}\""));
                    } else {
                        let count = self.search_results.len();
                        self.input_mode = InputMode::SearchResults;
                        self.status_message = Some(format!("{count} results for \"{pattern}\" — j/k navigate, Enter open, Esc close"));
                    }
                }
            }
            KeyCode::Backspace => {
                self.input_buffer.pop();
                self.status_message = Some(format!("Search: {}", self.input_buffer));
            }
            KeyCode::Char(c) => {
                self.input_buffer.push(c);
                self.status_message = Some(format!("Search: {}", self.input_buffer));
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_search_results_key(
        &mut self,
        key: KeyEvent,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.input_mode = InputMode::Normal;
                self.search_results.clear();
                self.status_message = None;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.search_cursor < self.search_results.len().saturating_sub(1) {
                    self.search_cursor += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.search_cursor = self.search_cursor.saturating_sub(1);
            }
            KeyCode::Enter => {
                if let Some(result) = self.search_results.get(self.search_cursor).cloned() {
                    // Navigate to the file's parent directory
                    if let Some(parent) = result.path.parent() {
                        let tab = self.tab_mut();
                        tab.current_dir = parent.to_path_buf();
                        tab.cursor = 0;
                        tab.refresh()?;
                        // Try to select the file
                        let file_name = result
                            .path
                            .file_name()
                            .map(|f| f.to_string_lossy().to_string())
                            .unwrap_or_default();
                        if let Some(pos) = tab
                            .filtered_entries
                            .iter()
                            .position(|&idx| tab.entries[idx].name == file_name)
                        {
                            tab.cursor = pos;
                            tab.update_preview();
                        }
                    }
                    self.input_mode = InputMode::Normal;
                    self.search_results.clear();
                    self.status_message = Some(format!("Opened: {}", result.path.display()));
                }
            }
            KeyCode::Char('G') => {
                self.search_cursor = self.search_results.len().saturating_sub(1);
            }
            KeyCode::Char('g') => {
                self.search_cursor = 0;
            }
            _ => {}
        }
        Ok(false)
    }

    fn delete_selected(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.tab().selected.is_empty() {
            if let Some(entry) = self.tab().selected_entry() {
                match file_ops::delete_to_trash(&entry.path) {
                    Ok(_) => self.status_message = Some("Deleted to trash".to_string()),
                    Err(e) => self.status_message = Some(format!("Error: {e}")),
                }
            }
        } else {
            let paths: Vec<_> = self.tab_mut().selected.drain().collect();
            let mut count = 0;
            for p in &paths {
                if file_ops::delete_to_trash(p).is_ok() {
                    count += 1;
                }
            }
            self.status_message = Some(format!("Deleted {count} items to trash"));
        }
        self.tab_mut().refresh()?;
        Ok(())
    }

    fn yank_selected(&mut self) {
        let sources = if self.tab().selected.is_empty() {
            self.tab()
                .selected_entry()
                .map(|e| vec![e.path.clone()])
                .unwrap_or_default()
        } else {
            self.tab().selected.iter().cloned().collect()
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
            let current_dir = self.tab().current_dir.clone();
            let mut count = 0;
            for src in &op.sources {
                let result = match op.kind {
                    OpKind::Copy => file_ops::copy_file(src, &current_dir),
                    OpKind::Move => file_ops::move_file(src, &current_dir),
                };
                if result.is_ok() {
                    count += 1;
                }
            }
            self.tab_mut().selected.clear();
            self.status_message = Some(format!("Pasted {count} item(s)"));
            self.tab_mut().refresh()?;
        } else {
            self.status_message = Some("Nothing to paste".to_string());
        }
        Ok(())
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
        assert_eq!(*app.current_dir(), dir);
        assert!(!app.entries().is_empty());
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
        let names: Vec<_> = app.entries().iter().map(|e| &e.name).collect();
        assert_eq!(names, vec!["a.txt", "b.txt"]);
    }

    #[test]
    fn test_sort_cycle() {
        let tmp = TempDir::new().unwrap();
        let mut app = make_app(&tmp);
        assert_eq!(app.sort_by(), SortBy::Name);
        app.tab_mut().sort_by = SortBy::Size;
        assert_eq!(app.sort_by(), SortBy::Size);
    }

    #[test]
    fn test_toggle_hidden() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join(".hidden"), "").unwrap();
        let mut app = make_app(&tmp);
        assert!(!app.show_hidden());
        app.tab_mut().show_hidden = true;
        app.tab_mut().refresh().unwrap();
        assert!(app.entries().iter().any(|e| e.name == ".hidden"));
    }

    #[test]
    fn test_filter() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("apple.txt"), "").unwrap();
        fs::write(tmp.path().join("banana.txt"), "").unwrap();
        let mut app = make_app(&tmp);
        app.input_mode = InputMode::Filter;
        app.tab_mut().filter_text = "app".to_string();
        app.tab_mut().apply_filter();
        assert_eq!(app.tab().filtered_entries.len(), 1);
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
        let path = app.entries()[0].path.clone();
        app.tab_mut().selected.insert(path.clone());
        assert_eq!(app.selection_count(), 1);
        app.tab_mut().selected.remove(&path);
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

    // Tab tests
    #[test]
    fn test_new_tab() {
        let tmp = TempDir::new().unwrap();
        let mut app = make_app(&tmp);
        assert_eq!(app.tabs.len(), 1);
        app.new_tab().unwrap();
        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.active_tab, 1);
    }

    #[test]
    fn test_close_tab() {
        let tmp = TempDir::new().unwrap();
        let mut app = make_app(&tmp);
        app.new_tab().unwrap();
        assert_eq!(app.tabs.len(), 2);
        assert!(!app.close_tab());
        assert_eq!(app.tabs.len(), 1);
    }

    #[test]
    fn test_close_last_tab_quits() {
        let tmp = TempDir::new().unwrap();
        let mut app = make_app(&tmp);
        assert!(app.close_tab()); // should return true = quit
    }

    #[test]
    fn test_next_prev_tab() {
        let tmp = TempDir::new().unwrap();
        let mut app = make_app(&tmp);
        app.new_tab().unwrap();
        app.new_tab().unwrap();
        assert_eq!(app.active_tab, 2);
        app.next_tab();
        assert_eq!(app.active_tab, 0); // wraps
        app.prev_tab();
        assert_eq!(app.active_tab, 2); // wraps back
    }

    #[test]
    fn test_tab_title() {
        let tmp = TempDir::new().unwrap();
        let app = make_app(&tmp);
        let title = app.tab().tab_title();
        assert!(!title.is_empty());
    }

    #[test]
    fn test_tree_mode_toggle() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().canonicalize().unwrap();
        fs::write(dir.join("a.txt"), "hello").unwrap();
        fs::create_dir(dir.join("subdir")).unwrap();
        fs::write(dir.join("subdir").join("b.txt"), "world").unwrap();
        let mut app = make_app(&tmp);
        assert!(!app.tab().tree_mode);
        app.tab_mut().toggle_tree_mode();
        assert!(app.tab().tree_mode);
        assert!(!app.tab().tree_nodes.is_empty());
        // Should have subdir and a.txt at minimum
        let names: Vec<_> = app
            .tab()
            .tree_nodes
            .iter()
            .map(|n| n.entry.name.clone())
            .collect();
        assert!(names.contains(&"a.txt".to_string()));
        assert!(names.contains(&"subdir".to_string()));
    }

    #[test]
    fn test_tree_expand_collapse() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().canonicalize().unwrap();
        fs::create_dir(dir.join("subdir")).unwrap();
        fs::write(dir.join("subdir").join("inner.txt"), "").unwrap();
        let mut app = make_app(&tmp);
        app.tab_mut().toggle_tree_mode();
        // Find subdir
        let sub_idx = app
            .tab()
            .tree_nodes
            .iter()
            .position(|n| n.entry.name == "subdir")
            .unwrap();
        app.tab_mut().tree_cursor = sub_idx;
        let initial_count = app.tab().tree_nodes.len();
        // Expand
        app.tab_mut().tree_toggle_expand();
        assert!(app.tab().tree_nodes.len() > initial_count);
        // inner.txt should now be visible
        assert!(app
            .tab()
            .tree_nodes
            .iter()
            .any(|n| n.entry.name == "inner.txt"));
        // Collapse
        let sub_idx = app
            .tab()
            .tree_nodes
            .iter()
            .position(|n| n.entry.name == "subdir")
            .unwrap();
        app.tab_mut().tree_cursor = sub_idx;
        app.tab_mut().tree_toggle_expand();
        assert!(!app
            .tab()
            .tree_nodes
            .iter()
            .any(|n| n.entry.name == "inner.txt"));
    }

    #[test]
    fn test_tree_toggle_back_to_list() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        let mut app = make_app(&tmp);
        app.tab_mut().toggle_tree_mode();
        assert!(app.tab().tree_mode);
        app.tab_mut().toggle_tree_mode();
        assert!(!app.tab().tree_mode);
    }

    #[test]
    fn test_tree_selected_entry() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().canonicalize().unwrap();
        fs::write(dir.join("file.txt"), "content").unwrap();
        let mut app = make_app(&tmp);
        app.tab_mut().toggle_tree_mode();
        let entry = app.tab().selected_tree_entry();
        assert!(entry.is_some());
    }

    #[test]
    fn test_mouse_scroll() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        fs::write(tmp.path().join("b.txt"), "").unwrap();
        let mut app = make_app(&tmp);
        assert_eq!(app.cursor(), 0);
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        })
        .unwrap();
        assert_eq!(app.cursor(), 1);
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        })
        .unwrap();
        assert_eq!(app.cursor(), 0);
    }

    // Chmod tests
    #[test]
    fn test_chmod_mode_entry() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "hello").unwrap();
        let mut app = make_app(&tmp);
        // Enter chmod mode
        app.input_mode = InputMode::Chmod;
        app.input_buffer = "755".to_string();
        // Simulate Enter
        app.handle_chmod_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.status_message.is_some());
    }

    #[test]
    fn test_chmod_only_accepts_octal() {
        let tmp = TempDir::new().unwrap();
        let mut app = make_app(&tmp);
        app.input_mode = InputMode::Chmod;
        app.input_buffer.clear();
        // '8' should not be accepted (not octal)
        app.handle_chmod_key(KeyEvent::new(KeyCode::Char('8'), KeyModifiers::NONE))
            .unwrap();
        assert!(app.input_buffer.is_empty());
        // '7' should be accepted
        app.handle_chmod_key(KeyEvent::new(KeyCode::Char('7'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.input_buffer, "7");
    }

    #[test]
    fn test_chmod_esc_cancels() {
        let tmp = TempDir::new().unwrap();
        let mut app = make_app(&tmp);
        app.input_mode = InputMode::Chmod;
        app.input_buffer = "644".to_string();
        app.handle_chmod_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.input_buffer.is_empty());
    }

    // Search tests
    #[test]
    fn test_search_mode_entry() {
        let tmp = TempDir::new().unwrap();
        let mut app = make_app(&tmp);
        app.input_mode = InputMode::Search;
        app.input_buffer.clear();
        app.handle_search_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.input_buffer, "h");
    }

    #[test]
    fn test_search_finds_results() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().canonicalize().unwrap();
        fs::write(dir.join("needle.txt"), "find the needle here").unwrap();
        fs::write(dir.join("other.txt"), "nothing here").unwrap();
        let mut app = make_app(&tmp);
        app.input_mode = InputMode::Search;
        app.input_buffer = "needle".to_string();
        app.handle_search_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.input_mode, InputMode::SearchResults);
        assert!(!app.search_results.is_empty());
    }

    #[test]
    fn test_search_no_results() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "hello").unwrap();
        let mut app = make_app(&tmp);
        app.input_mode = InputMode::Search;
        app.input_buffer = "zzzznotfound".to_string();
        app.handle_search_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.search_results.is_empty());
    }

    #[test]
    fn test_search_results_navigation() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().canonicalize().unwrap();
        fs::write(dir.join("a.txt"), "line1\nline2\nline3").unwrap();
        let mut app = make_app(&tmp);
        app.input_mode = InputMode::Search;
        app.input_buffer = "line".to_string();
        app.handle_search_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.input_mode, InputMode::SearchResults);
        assert!(app.search_results.len() >= 3);
        assert_eq!(app.search_cursor, 0);
        // Navigate down
        app.handle_search_results_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.search_cursor, 1);
        // Navigate up
        app.handle_search_results_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.search_cursor, 0);
    }

    #[test]
    fn test_search_results_esc() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().canonicalize().unwrap();
        fs::write(dir.join("a.txt"), "test content").unwrap();
        let mut app = make_app(&tmp);
        app.search_results = vec![file_ops::SearchResult {
            path: dir.join("a.txt"),
            line_number: 1,
            line_text: "test content".to_string(),
        }];
        app.search_cursor = 0;
        app.input_mode = InputMode::SearchResults;
        app.handle_search_results_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.search_results.is_empty());
    }

    #[test]
    fn test_search_results_enter_navigates() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().canonicalize().unwrap();
        let sub = dir.join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("target.txt"), "found it").unwrap();
        let mut app = make_app(&tmp);
        app.search_results = vec![file_ops::SearchResult {
            path: sub.join("target.txt"),
            line_number: 1,
            line_text: "found it".to_string(),
        }];
        app.search_cursor = 0;
        app.input_mode = InputMode::SearchResults;
        app.handle_search_results_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.input_mode, InputMode::Normal);
        assert_eq!(*app.tab().current_dir, sub);
    }
}
