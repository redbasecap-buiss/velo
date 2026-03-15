use crate::preview::format_size;
use crate::app::{App, FileEntry, InputMode, MouseAreas};
#[cfg(unix)]
use crate::app::format_mode;
use chrono::{DateTime, Local};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

pub fn draw(f: &mut Frame, app: &mut App) {
    let has_tabs = app.tabs.len() > 1;
    let tab_bar_height = if has_tabs { 1 } else { 0 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(tab_bar_height), // Tab bar (only if multiple tabs)
            Constraint::Length(1),              // Breadcrumb
            Constraint::Min(5),                 // Three panes
            Constraint::Length(2),              // Info + status bar
        ])
        .split(f.area());

    // Reset mouse areas
    app.mouse_areas = MouseAreas::default();

    if has_tabs {
        draw_tab_bar(f, app, chunks[0]);
    }
    draw_breadcrumb(f, app, chunks[1]);
    draw_panes(f, app, chunks[2]);
    draw_status_bar(f, app, chunks[3]);

    if app.show_help {
        draw_help_overlay(f, app);
    }
}

fn draw_tab_bar(f: &mut Frame, app: &mut App, area: Rect) {
    let theme = &app.theme;
    let mut spans = Vec::new();
    let mut tab_positions = Vec::new();
    let mut x = area.x;

    for (i, tab) in app.tabs.iter().enumerate() {
        let title = tab.tab_title();
        let label = format!(" {} {} ", i + 1, title);
        let width = label.len() as u16;

        tab_positions.push((x, width, i));

        let style = if i == app.active_tab {
            Style::default()
                .fg(theme.tab_active_fg)
                .bg(theme.tab_active_bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(theme.tab_inactive_fg)
                .bg(theme.tab_inactive_bg)
        };
        spans.push(Span::styled(label, style));
        spans.push(Span::raw(" "));
        x += width + 1;
    }

    // Hint
    spans.push(Span::styled(
        " Ctrl-T:new  Ctrl-W:close  Ctrl-←→:switch",
        Style::default().fg(theme.border),
    ));

    app.mouse_areas.tab_bar = Some((area.x, area.y, area.width, area.height));
    app.mouse_areas.tab_positions = tab_positions;

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_breadcrumb(f: &mut Frame, app: &App, area: Rect) {
    let breadcrumb = app.breadcrumb();
    let line = Line::from(Span::styled(
        format!(" {breadcrumb}"),
        Style::default()
            .fg(app.theme.breadcrumb)
            .add_modifier(Modifier::BOLD),
    ));
    f.render_widget(Paragraph::new(line), area);
}

fn draw_panes(f: &mut Frame, app: &mut App, area: Rect) {
    if app.input_mode == InputMode::SearchResults {
        // Full-width search results view
        draw_search_results(f, app, area);
        return;
    }

    if app.dual_pane {
        draw_dual_panes(f, app, area);
        return;
    }

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

fn draw_dual_panes(f: &mut Frame, app: &mut App, area: Rect) {
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Left pane = main tab
    draw_dual_file_pane(f, app, panes[0], false);
    // Right pane = dual tab
    draw_dual_file_pane(f, app, panes[1], true);
}

fn draw_dual_file_pane(f: &mut Frame, app: &mut App, area: Rect, is_right: bool) {
    let tab = if is_right {
        match app.dual_tab.as_ref() {
            Some(t) => t,
            None => return,
        }
    } else {
        &app.tabs[app.active_tab]
    };
    let is_active = if is_right {
        app.dual_right_active
    } else {
        !app.dual_right_active
    };
    let theme = &app.theme;

    // Set mouse area for the active pane
    if is_active {
        app.mouse_areas.current_pane = Some((area.x, area.y, area.width, area.height));
    }

    let visible = tab.visible_entries();
    let cursor = tab.cursor;
    let selected_set = &tab.selected;

    let items: Vec<ListItem> = visible
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let selected = selected_set.contains(&entry.path);
            let is_cursor = i == cursor;
            let mut style = if is_cursor && is_active {
                Style::default().fg(theme.cursor_fg).bg(theme.cursor_bg)
            } else if is_cursor {
                Style::default().fg(theme.cursor_fg).bg(theme.border)
            } else {
                entry_style_themed(entry, theme)
            };
            if selected {
                style = style.add_modifier(Modifier::BOLD).fg(theme.selected);
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

    let dir_name = tab
        .current_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "/".to_string());
    let indicator = if is_active { "▶ " } else { "  " };
    let title = format!("{indicator}{dir_name}");
    let border_color = if is_active {
        theme.cursor_bg
    } else {
        theme.border
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(border_color));
    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

fn draw_parent_pane(f: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    let items: Vec<ListItem> = app
        .parent_entries()
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let style = if i == app.parent_cursor() {
                Style::default().fg(theme.cursor_fg).bg(theme.cursor_bg)
            } else {
                entry_style_themed(entry, theme)
            };
            ListItem::new(entry_display_name(entry)).style(style)
        })
        .collect();
    let block = Block::default()
        .borders(Borders::RIGHT)
        .title("Parent")
        .border_style(Style::default().fg(theme.border));
    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

fn draw_current_pane(f: &mut Frame, app: &mut App, area: Rect) {
    // Record mouse area for click handling
    app.mouse_areas.current_pane = Some((area.x, area.y, area.width, area.height));

    if app.tab().tree_mode {
        draw_tree_pane(f, app, area);
        return;
    }

    let visible = app.visible_entries();
    let cursor = app.cursor();
    let selected_set = app.selected().clone();
    let theme = app.theme.clone();

    let items: Vec<ListItem> = visible
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let selected = selected_set.contains(&entry.path);
            let is_cursor = i == cursor;
            let mut style = if is_cursor {
                Style::default().fg(theme.cursor_fg).bg(theme.cursor_bg)
            } else {
                entry_style_themed(entry, &theme)
            };
            if selected {
                style = style.add_modifier(Modifier::BOLD).fg(theme.selected);
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
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(theme.border));
    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

fn draw_tree_pane(f: &mut Frame, app: &App, area: Rect) {
    let selected_set = app.selected().clone();
    let tree_cursor = app.tab().tree_cursor;
    let theme = &app.theme;

    let items: Vec<ListItem> = app
        .tab()
        .tree_nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let is_cursor = i == tree_cursor;
            let selected = selected_set.contains(&node.entry.path);
            let mut style = if is_cursor {
                Style::default().fg(theme.cursor_fg).bg(theme.cursor_bg)
            } else {
                entry_style_themed(&node.entry, theme)
            };
            if selected {
                style = style.add_modifier(Modifier::BOLD).fg(theme.selected);
            }

            let indent = "  ".repeat(node.depth);
            let icon = if node.entry.is_dir {
                if node.expanded {
                    "▼ "
                } else if node.has_children {
                    "▶ "
                } else {
                    "▷ "
                }
            } else {
                "  "
            };
            let mut name = entry_display_name(&node.entry);
            if selected && !is_cursor {
                name = format!("* {name}");
            }
            ListItem::new(format!("{indent}{icon}{name}")).style(style)
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title("🌳 Tree")
        .border_style(Style::default().fg(theme.border));
    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

fn draw_search_results(f: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    let visible_height = area.height.saturating_sub(2) as usize; // borders
    let scroll = if app.search_cursor >= visible_height {
        app.search_cursor - visible_height + 1
    } else {
        0
    };

    let items: Vec<ListItem> = app
        .search_results
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible_height)
        .map(|(i, result)| {
            let is_cursor = i == app.search_cursor;
            let text = format!(
                "{}:{} {}",
                result.path.display(),
                result.line_number,
                result.line_text.trim()
            );
            let style = if is_cursor {
                Style::default()
                    .fg(theme.cursor_fg)
                    .bg(theme.search_highlight)
            } else {
                Style::default().fg(theme.fg)
            };
            ListItem::new(text).style(style)
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            "🔍 Search Results ({} matches)",
            app.search_results.len()
        ))
        .border_style(Style::default().fg(theme.border));
    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

fn draw_preview_pane(f: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    let lines: Vec<Line> = app
        .preview_lines()
        .iter()
        .map(|pl| {
            let color = match pl.style {
                crate::preview::PreviewStyle::Header => theme.preview_header,
                crate::preview::PreviewStyle::Directory => theme.directory,
                crate::preview::PreviewStyle::LineNumber => theme.preview_line_no,
                crate::preview::PreviewStyle::Normal => theme.fg,
            };
            Line::from(Span::styled(pl.text.clone(), Style::default().fg(color)))
        })
        .collect();
    let block = Block::default()
        .borders(Borders::LEFT)
        .title("Preview")
        .border_style(Style::default().fg(theme.border));
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

    let info = if let Some(entry) = app.selected_entry() {
        let size = format_size(entry.size);
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
        {
            #[cfg(unix)]
            let perms = entry.mode.map(format_mode).unwrap_or_default();
            #[cfg(not(unix))]
            let perms = String::new();
            if perms.is_empty() {
                format!(" {} │ {} │ {}{symlink_info}", entry.name, size, modified)
            } else {
                format!(" {} │ {} │ {} │ {}{symlink_info}", entry.name, size, modified, perms)
            }
        }
    } else {
        String::new()
    };
    let theme = &app.theme;
    f.render_widget(
        Paragraph::new(info).style(Style::default().bg(theme.status_bg).fg(theme.status_fg)),
        rows[0],
    );

    let tab_info = if app.tabs.len() > 1 {
        format!(" Tab {}/{} │", app.active_tab + 1, app.tabs.len())
    } else {
        String::new()
    };

    let status = if let Some(msg) = &app.status_message {
        msg.clone()
    } else if app.input_mode != InputMode::Normal {
        match app.input_mode {
            InputMode::Rename => format!("Rename: {}", app.input_buffer),
            InputMode::CreateFile => format!("New file: {}", app.input_buffer),
            InputMode::CreateDir => format!("New dir: {}", app.input_buffer),
            InputMode::Bookmark => "Bookmark key?".to_string(),
            InputMode::JumpBookmark => "Jump to bookmark?".to_string(),
            InputMode::Chmod => format!("chmod (octal): {}", app.input_buffer),
            InputMode::Search => format!("Search: {}", app.input_buffer),
            InputMode::SearchResults => {
                format!(
                    "Search results: {}/{} — j/k navigate, Enter open, Esc close",
                    app.search_cursor + 1,
                    app.search_results.len()
                )
            }
            InputMode::Normal | InputMode::Filter => String::new(),
        }
    } else {
        format!(
            "{} {} files │ {} selected │ Sort: {:?}",
            tab_info,
            app.file_count(),
            app.selection_count(),
            app.sort_by(),
        )
    };
    // Add undo info to status when idle
    let undo_info = if app.input_mode == InputMode::Normal && app.status_message.is_none() {
        let u = app.undo_stack.undo_count();
        let r = app.undo_stack.redo_count();
        if u > 0 || r > 0 {
            format!(" │ Undo:{u} Redo:{r}")
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    f.render_widget(
        Paragraph::new(format!("{status}{undo_info}"))
            .style(Style::default().bg(theme.status_bg).fg(theme.status_fg)),
        rows[1],
    );
}

#[allow(dead_code)]
fn entry_style(entry: &FileEntry) -> Style {
    // Fallback — callers with theme access should use entry_style_themed
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

fn entry_style_themed(entry: &FileEntry, theme: &crate::theme::Theme) -> Style {
    if entry.is_symlink {
        Style::default().fg(theme.symlink)
    } else if entry.is_dir {
        Style::default()
            .fg(theme.directory)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.file)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1048576), "1.0 MB");
        assert_eq!(format_size(1073741824), "1.0 GB");
        assert_eq!(format_size(1099511627776), "1.0 TB");
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
            #[cfg(unix)]
            mode: Some(0o644),
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
            #[cfg(unix)]
            mode: Some(0o644),
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
            #[cfg(unix)]
            mode: Some(0o644),
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
            #[cfg(unix)]
            mode: Some(0o644),
        };
        let style = entry_style(&entry);
        assert_eq!(style.fg, Some(Color::Cyan));
    }
}

fn draw_help_overlay(f: &mut Frame, _app: &App) {
    let area = f.area();
    // Center a box roughly 60x28
    let width = 60u16.min(area.width.saturating_sub(4));
    let height = 29u16.min(area.height.saturating_sub(2));
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let popup = Rect::new(x, y, width, height);

    let help_text = vec![
        Line::from(Span::styled("  Keybindings", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from("  j/k  ↑/↓        Navigate files"),
        Line::from("  h/l  ←/→/Enter  Parent / Open"),
        Line::from("  g g             Go to top"),
        Line::from("  G               Go to bottom"),
        Line::from("  Space           Toggle selection"),
        Line::from("  /               Filter"),
        Line::from("  F               Recursive search"),
        Line::from("  .               Toggle hidden files"),
        Line::from("  s               Cycle sort mode"),
        Line::from("  R               Reverse sort order"),
        Line::from("  r               Rename"),
        Line::from("  n / N           New file / directory"),
        Line::from("  d d             Delete to trash"),
        Line::from("  y y             Yank (copy)"),
        Line::from("  p p             Paste"),
        Line::from("  c               chmod (octal)"),
        Line::from("  t               Toggle tree view"),
        Line::from("  T               Cycle theme"),
        Line::from("  S               Disk usage"),
        Line::from("  Y               Copy path to clipboard"),
        Line::from("  Ctrl+Y          Copy file content"),
        Line::from("  D               Toggle dual pane"),
        Line::from("  Tab             Switch pane (dual)"),
        Line::from("  m / '           Set / jump bookmark"),
        Line::from("  u / U           Undo / Redo"),
        Line::from("  ~               Go to home directory"),
        Line::from("  X               Extract archive"),
        Line::from("  Z               Compress selection"),
        Line::from("  Ctrl+T/W        New / close tab"),
        Line::from("  q               Quit"),
        Line::from(""),
        Line::from(Span::styled("  Press any key to dismiss", Style::default().fg(Color::DarkGray))),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Help (?) ")
        .style(Style::default().bg(Color::Black));

    let paragraph = Paragraph::new(help_text)
        .block(block)
        .wrap(Wrap { trim: false });

    // Clear the area first
    f.render_widget(ratatui::widgets::Clear, popup);
    f.render_widget(paragraph, popup);
}
