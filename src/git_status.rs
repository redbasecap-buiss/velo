use git2::{Repository, StatusOptions};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum GitFileStatus {
    Modified,
    Staged,
    Untracked,
    Conflict,
    Deleted,
    Renamed,
    Ignored,
}

impl GitFileStatus {
    pub fn icon(self) -> &'static str {
        match self {
            Self::Modified => "M",
            Self::Staged => "S",
            Self::Untracked => "?",
            Self::Conflict => "!",
            Self::Deleted => "D",
            Self::Renamed => "R",
            Self::Ignored => "I",
        }
    }
}

pub fn get_git_statuses(dir: &Path) -> HashMap<String, GitFileStatus> {
    let mut map = HashMap::new();
    let repo = match Repository::discover(dir) {
        Ok(r) => r,
        Err(_) => return map,
    };
    let workdir = match repo.workdir() {
        Some(w) => w.to_path_buf(),
        None => return map,
    };

    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);

    let statuses = match repo.statuses(Some(&mut opts)) {
        Ok(s) => s,
        Err(_) => return map,
    };

    for entry in statuses.iter() {
        let path_str = match entry.path() {
            Some(p) => p.to_string(),
            None => continue,
        };
        let status = entry.status();
        let full_path = workdir.join(&path_str);

        // Get the component relative to `dir`
        let rel = match full_path.strip_prefix(dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        // We only care about the first component (the direct child)
        let first = match rel.components().next() {
            Some(c) => c.as_os_str().to_string_lossy().to_string(),
            None => continue,
        };

        let file_status = if status.is_conflicted() {
            GitFileStatus::Conflict
        } else if status.is_index_new()
            || status.is_index_modified()
            || status.is_index_deleted()
            || status.is_index_renamed()
        {
            GitFileStatus::Staged
        } else if status.is_wt_modified() || status.is_wt_renamed() {
            GitFileStatus::Modified
        } else if status.is_wt_deleted() {
            GitFileStatus::Deleted
        } else if status.is_wt_new() {
            GitFileStatus::Untracked
        } else {
            continue;
        };

        // Don't override a higher-priority status
        map.entry(first).or_insert(file_status);
    }

    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_git_file_status_icon() {
        assert_eq!(GitFileStatus::Modified.icon(), "M");
        assert_eq!(GitFileStatus::Staged.icon(), "S");
        assert_eq!(GitFileStatus::Untracked.icon(), "?");
        assert_eq!(GitFileStatus::Conflict.icon(), "!");
        assert_eq!(GitFileStatus::Deleted.icon(), "D");
        assert_eq!(GitFileStatus::Renamed.icon(), "R");
        assert_eq!(GitFileStatus::Ignored.icon(), "I");
    }

    #[test]
    fn test_non_git_dir_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let statuses = get_git_statuses(tmp.path());
        assert!(statuses.is_empty());
    }

    #[test]
    fn test_git_repo_with_untracked() {
        let tmp = TempDir::new().unwrap();
        // Canonicalize to handle macOS /private/var symlinks
        let dir = tmp.path().canonicalize().unwrap();
        Repository::init(&dir).unwrap();
        std::fs::write(dir.join("new.txt"), "hello").unwrap();
        let statuses = get_git_statuses(&dir);
        assert_eq!(statuses.get("new.txt"), Some(&GitFileStatus::Untracked));
    }

    #[test]
    fn test_git_repo_with_staged() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().canonicalize().unwrap();
        let repo = Repository::init(&dir).unwrap();
        std::fs::write(dir.join("staged.txt"), "staged").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("staged.txt")).unwrap();
        index.write().unwrap();
        let statuses = get_git_statuses(&dir);
        assert_eq!(statuses.get("staged.txt"), Some(&GitFileStatus::Staged));
    }
}
