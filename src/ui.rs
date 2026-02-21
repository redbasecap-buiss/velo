use crate::app::{App, FileEntry, InputMode};
use chrono::{DateTime, Local};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Breadcrumb
            Constraint::Min(5),    // Three panes
            Constraint::Length(2), // Info + status bar
        ])
        .split(f.area());

    draw_breadcrumb(f, app, chunks[0]);
    draw_panes(f, app, chunks[1]);
    draw_status_bar(f, app, chunks[2]);
}

fn draw_breadcrumb(f: &mut Frame, app: &App, area: Rect) {
    let breadcrumb = app.breadcrumb();
    let line = Line::from(Span::styled(
        format!(" {breadcrumb}"),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ));
    f.render_widget(Paragraph::new(line), area);
}

fn draw_panes(f: &mut Frame, app: &App, area: Rect) {
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(40),
            Constraint::Percentage(40),
        ])
        .split(area);

    draw_parent_pane(f, app, panes[0]);
    draw_current_pane(f, app, panes[1]);
    draw_preview_pane(f, app, panes[2]);
}

fn draw_parent_pane(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .parent_entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let style = if i == app.parent_cursor {
                Style::default().fg(Color::Black).bg(Color::White)
            } else {
                entry_style(entry)
            };
            ListItem::new(entry_display_name(entry)).style(style)
        })
        .collect();
    let block = Block::default().borders(Borders::RIGHT).title("Parent");
    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

fn draw_current_pane(f: &mut Frame, app: &App, area: Rect) {
    let visible = app.visible_entries();
    let items: Vec<ListItem> = visible
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let selected = app.selected.contains(&entry.path);
            let is_cursor = i == app.cursor;
            let mut style = if is_cursor {
                Style::default().fg(Color::Black).bg(Color::White)
            } else {
                entry_style(entry)
            };
            if selected {
                style = style.add_modifier(Modifier::BOLD).fg(Color::Yellow);
            }

            let mut name = entry_display_name(entry);
            if let Some(gs) = &entry.git_status {
                name = format!("[{}] {}", gs.icon(), name);
            }
            if selected && !is_cursor {
                name = format!("* {name}");
            }
            ListItem::new(name).style(style)
        })
        .collect();

    let title = if app.input_mode == InputMode::Filter {
        format!("/{}", app.input_buffer)
    } else {
        "Files".to_string()
    };
    let block = Block::default().borders(Borders::ALL).title(title);
    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

fn draw_preview_pane(f: &mut Frame, app: &App, area: Rect) {
    let lines: Vec<Line> = app
        .preview_lines
        .iter()
        .map(|pl| {
            let color = match pl.style {
                crate::preview::PreviewStyle::Header => Color::Yellow,
                crate::preview::PreviewStyle::Directory => Color::Blue,
                crate::preview::PreviewStyle::LineNumber => Color::DarkGray,
                crate::preview::PreviewStyle::Normal => Color::White,
            };
            Line::from(Span::styled(pl.text.clone(), Style::default().fg(color)))
        })
        .collect();
    let block = Block::default().borders(Borders::LEFT).title("Preview");
    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    // Info bar for selected file
    let info = if let Some(entry) = app.selected_entry() {
        let size = human_size(entry.size);
        let modified = entry
            .modified
            .map(|m| {
                let dt: DateTime<Local> = m.into();
                dt.format("%Y-%m-%d %H:%M").to_string()
            })
            .unwrap_or_else(|| "—".to_string());
        let symlink_info = if entry.is_symlink {
            format!(" → {}", entry.symlink_target.as_deref().unwrap_or("?"))
        } else {
            String::new()
        };
        format!(" {} │ {} │ {}{symlink_info}", entry.name, size, modified)
    } else {
        String::new()
    };
    f.render_widget(
        Paragraph::new(info).style(Style::default().bg(Color::DarkGray).fg(Color::White)),
        rows[0],
    );

    // Status bar
    let status = if let Some(msg) = &app.status_message {
        msg.clone()
    } else if app.input_mode != InputMode::Normal {
        match app.input_mode {
            InputMode::Rename => format!("Rename: {}", app.input_buffer),
            InputMode::CreateFile => format!("New file: {}", app.input_buffer),
            InputMode::CreateDir => format!("New dir: {}", app.input_buffer),
            InputMode::Bookmark => "Bookmark key?".to_string(),
            InputMode::JumpBookmark => "Jump to bookmark?".to_string(),
            _ => String::new(),
        }
    } else {
        format!(
            " {} files │ {} selected │ Sort: {:?}",
            app.file_count(),
            app.selection_count(),
            app.config.sort_by,
        )
    };
    f.render_widget(
        Paragraph::new(status).style(Style::default().bg(Color::Blue).fg(Color::White)),
        rows[1],
    );
}

fn entry_style(entry: &FileEntry) -> Style {
    if entry.is_symlink {
        Style::default().fg(Color::Cyan)
    } else if entry.is_dir {
        Style::default()
            .fg(Color::Blue)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    }
}

fn entry_display_name(entry: &FileEntry) -> String {
    let mut name = entry.name.clone();
    if entry.is_dir {
        name.push('/');
    }
    if entry.is_symlink {
        if let Some(target) = &entry.symlink_target {
            name = format!("{name} → {target}");
        }
    }
    name
}

fn human_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    for unit in UNITS {
        if size < 1024.0 {
            return format!("{size:.1} {unit}");
        }
        size /= 1024.0;
    }
    format!("{size:.1} PB")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_human_size() {
        assert_eq!(human_size(0), "0.0 B");
        assert_eq!(human_size(500), "500.0 B");
        assert_eq!(human_size(1024), "1.0 KB");
        assert_eq!(human_size(1048576), "1.0 MB");
        assert_eq!(human_size(1073741824), "1.0 GB");
    }

    #[test]
    fn test_entry_display_name_dir() {
        let entry = FileEntry {
            name: "docs".to_string(),
            path: std::path::PathBuf::from("/tmp/docs"),
            is_dir: true,
            is_symlink: false,
            symlink_target: None,
            size: 0,
            modified: None,
            git_status: None,
        };
        assert_eq!(entry_display_name(&entry), "docs/");
    }

    #[test]
    fn test_entry_display_name_symlink() {
        let entry = FileEntry {
            name: "link".to_string(),
            path: std::path::PathBuf::from("/tmp/link"),
            is_dir: false,
            is_symlink: true,
            symlink_target: Some("/tmp/target".to_string()),
            size: 0,
            modified: None,
            git_status: None,
        };
        assert_eq!(entry_display_name(&entry), "link → /tmp/target");
    }

    #[test]
    fn test_entry_style_dir() {
        let entry = FileEntry {
            name: "dir".to_string(),
            path: std::path::PathBuf::from("/tmp/dir"),
            is_dir: true,
            is_symlink: false,
            symlink_target: None,
            size: 0,
            modified: None,
            git_status: None,
        };
        let style = entry_style(&entry);
        assert_eq!(style.fg, Some(Color::Blue));
    }

    #[test]
    fn test_entry_style_symlink() {
        let entry = FileEntry {
            name: "link".to_string(),
            path: std::path::PathBuf::from("/tmp/link"),
            is_dir: false,
            is_symlink: true,
            symlink_target: None,
            size: 0,
            modified: None,
            git_status: None,
        };
        let style = entry_style(&entry);
        assert_eq!(style.fg, Some(Color::Cyan));
    }
}
