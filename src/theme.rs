use ratatui::style::Color;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeName {
    Default,
    Dracula,
    Catppuccin,
    Nord,
    Gruvbox,
}

impl ThemeName {
    pub fn next(self) -> Self {
        match self {
            Self::Default => Self::Dracula,
            Self::Dracula => Self::Catppuccin,
            Self::Catppuccin => Self::Nord,
            Self::Nord => Self::Gruvbox,
            Self::Gruvbox => Self::Default,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Dracula => "Dracula",
            Self::Catppuccin => "Catppuccin",
            Self::Nord => "Nord",
            Self::Gruvbox => "Gruvbox",
        }
    }
}

/// Resolved color palette for rendering
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Theme {
    pub bg: Color,
    pub fg: Color,
    pub directory: Color,
    pub symlink: Color,
    pub file: Color,
    pub selected: Color,
    pub cursor_fg: Color,
    pub cursor_bg: Color,
    pub status_bg: Color,
    pub status_fg: Color,
    pub breadcrumb: Color,
    pub tab_active_fg: Color,
    pub tab_active_bg: Color,
    pub tab_inactive_fg: Color,
    pub tab_inactive_bg: Color,
    pub preview_header: Color,
    pub preview_line_no: Color,
    pub search_highlight: Color,
    pub border: Color,
    pub git_modified: Color,
    pub git_added: Color,
}

impl Theme {
    pub fn from_name(name: ThemeName) -> Self {
        match name {
            ThemeName::Default => Self::default_theme(),
            ThemeName::Dracula => Self::dracula(),
            ThemeName::Catppuccin => Self::catppuccin(),
            ThemeName::Nord => Self::nord(),
            ThemeName::Gruvbox => Self::gruvbox(),
        }
    }

    fn default_theme() -> Self {
        Self {
            bg: Color::Reset,
            fg: Color::White,
            directory: Color::Blue,
            symlink: Color::Cyan,
            file: Color::White,
            selected: Color::Yellow,
            cursor_fg: Color::Black,
            cursor_bg: Color::White,
            status_bg: Color::Blue,
            status_fg: Color::White,
            breadcrumb: Color::Cyan,
            tab_active_fg: Color::Black,
            tab_active_bg: Color::Cyan,
            tab_inactive_fg: Color::Gray,
            tab_inactive_bg: Color::DarkGray,
            preview_header: Color::Yellow,
            preview_line_no: Color::DarkGray,
            search_highlight: Color::Yellow,
            border: Color::Reset,
            git_modified: Color::Yellow,
            git_added: Color::Green,
        }
    }

    fn dracula() -> Self {
        // Dracula palette
        let bg = Color::Rgb(40, 42, 54);
        let fg = Color::Rgb(248, 248, 242);
        let purple = Color::Rgb(189, 147, 249);
        let green = Color::Rgb(80, 250, 123);
        let cyan = Color::Rgb(139, 233, 253);
        let pink = Color::Rgb(255, 121, 198);
        let yellow = Color::Rgb(241, 250, 140);
        let orange = Color::Rgb(255, 184, 108);
        let comment = Color::Rgb(98, 114, 164);
        let selection = Color::Rgb(68, 71, 90);

        Self {
            bg,
            fg,
            directory: purple,
            symlink: cyan,
            file: fg,
            selected: pink,
            cursor_fg: bg,
            cursor_bg: fg,
            status_bg: selection,
            status_fg: fg,
            breadcrumb: cyan,
            tab_active_fg: bg,
            tab_active_bg: purple,
            tab_inactive_fg: comment,
            tab_inactive_bg: selection,
            preview_header: yellow,
            preview_line_no: comment,
            search_highlight: yellow,
            border: comment,
            git_modified: orange,
            git_added: green,
        }
    }

    fn catppuccin() -> Self {
        // Catppuccin Mocha
        let base = Color::Rgb(30, 30, 46);
        let text = Color::Rgb(205, 214, 244);
        let blue = Color::Rgb(137, 180, 250);
        let teal = Color::Rgb(148, 226, 213);
        let pink = Color::Rgb(245, 194, 231);
        let yellow = Color::Rgb(249, 226, 175);
        let green = Color::Rgb(166, 227, 161);
        let peach = Color::Rgb(250, 179, 135);
        let overlay0 = Color::Rgb(108, 112, 134);
        let surface0 = Color::Rgb(49, 50, 68);
        let mauve = Color::Rgb(203, 166, 247);

        Self {
            bg: base,
            fg: text,
            directory: blue,
            symlink: teal,
            file: text,
            selected: pink,
            cursor_fg: base,
            cursor_bg: text,
            status_bg: surface0,
            status_fg: text,
            breadcrumb: mauve,
            tab_active_fg: base,
            tab_active_bg: mauve,
            tab_inactive_fg: overlay0,
            tab_inactive_bg: surface0,
            preview_header: yellow,
            preview_line_no: overlay0,
            search_highlight: yellow,
            border: overlay0,
            git_modified: peach,
            git_added: green,
        }
    }

    fn nord() -> Self {
        // Nord palette
        let polar0 = Color::Rgb(46, 52, 64);
        let polar1 = Color::Rgb(59, 66, 82);
        let snow0 = Color::Rgb(216, 222, 233);
        let snow2 = Color::Rgb(236, 239, 244);
        let frost0 = Color::Rgb(143, 188, 187);
        let frost1 = Color::Rgb(136, 192, 208);
        let frost2 = Color::Rgb(129, 161, 193);
        let frost3 = Color::Rgb(94, 129, 172);
        let aurora_red = Color::Rgb(191, 97, 106);
        let aurora_orange = Color::Rgb(208, 135, 112);
        let aurora_yellow = Color::Rgb(235, 203, 139);
        let aurora_green = Color::Rgb(163, 190, 140);

        Self {
            bg: polar0,
            fg: snow0,
            directory: frost3,
            symlink: frost0,
            file: snow0,
            selected: aurora_red,
            cursor_fg: polar0,
            cursor_bg: snow2,
            status_bg: polar1,
            status_fg: snow0,
            breadcrumb: frost1,
            tab_active_fg: polar0,
            tab_active_bg: frost2,
            tab_inactive_fg: frost0,
            tab_inactive_bg: polar1,
            preview_header: aurora_yellow,
            preview_line_no: frost0,
            search_highlight: aurora_yellow,
            border: polar1,
            git_modified: aurora_orange,
            git_added: aurora_green,
        }
    }

    fn gruvbox() -> Self {
        // Gruvbox Dark
        let bg0 = Color::Rgb(40, 40, 40);
        let bg1 = Color::Rgb(60, 56, 54);
        let fg0 = Color::Rgb(251, 241, 199);
        let gray = Color::Rgb(146, 131, 116);
        let red = Color::Rgb(251, 73, 52);
        let green = Color::Rgb(184, 187, 38);
        let yellow = Color::Rgb(250, 189, 47);
        let blue = Color::Rgb(131, 165, 152);
        let _purple = Color::Rgb(211, 134, 155);
        let aqua = Color::Rgb(142, 192, 124);
        let orange = Color::Rgb(254, 128, 25);

        Self {
            bg: bg0,
            fg: fg0,
            directory: blue,
            symlink: aqua,
            file: fg0,
            selected: red,
            cursor_fg: bg0,
            cursor_bg: fg0,
            status_bg: bg1,
            status_fg: fg0,
            breadcrumb: yellow,
            tab_active_fg: bg0,
            tab_active_bg: yellow,
            tab_inactive_fg: gray,
            tab_inactive_bg: bg1,
            preview_header: orange,
            preview_line_no: gray,
            search_highlight: yellow,
            border: gray,
            git_modified: orange,
            git_added: green,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_from_all_names() {
        for name in [
            ThemeName::Default,
            ThemeName::Dracula,
            ThemeName::Catppuccin,
            ThemeName::Nord,
            ThemeName::Gruvbox,
        ] {
            let theme = Theme::from_name(name);
            // Just verify they don't panic and have distinct directories
            assert_ne!(theme.directory, theme.symlink);
        }
    }

    #[test]
    fn test_theme_cycle() {
        let mut name = ThemeName::Default;
        let mut visited = Vec::new();
        for _ in 0..5 {
            visited.push(name);
            name = name.next();
        }
        assert_eq!(visited.len(), 5);
        assert_eq!(name, ThemeName::Default); // cycles back
    }

    #[test]
    fn test_theme_labels() {
        assert_eq!(ThemeName::Dracula.label(), "Dracula");
        assert_eq!(ThemeName::Catppuccin.label(), "Catppuccin");
        assert_eq!(ThemeName::Nord.label(), "Nord");
        assert_eq!(ThemeName::Gruvbox.label(), "Gruvbox");
        assert_eq!(ThemeName::Default.label(), "Default");
    }

    #[test]
    fn test_theme_serialize() {
        let s = serde_json::to_string(&ThemeName::Dracula).unwrap();
        assert_eq!(s, "\"dracula\"");
        let v: ThemeName = serde_json::from_str("\"nord\"").unwrap();
        assert_eq!(v, ThemeName::Nord);
    }

    #[test]
    fn test_default_theme_colors() {
        let t = Theme::from_name(ThemeName::Default);
        assert_eq!(t.directory, Color::Blue);
        assert_eq!(t.symlink, Color::Cyan);
        assert_eq!(t.file, Color::White);
    }

    #[test]
    fn test_dracula_has_rgb_colors() {
        let t = Theme::from_name(ThemeName::Dracula);
        matches!(t.directory, Color::Rgb(_, _, _));
        matches!(t.bg, Color::Rgb(_, _, _));
    }
}
