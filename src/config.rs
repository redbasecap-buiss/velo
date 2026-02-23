use crate::theme::ThemeName;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_show_hidden")]
    pub show_hidden: bool,
    #[serde(default = "default_sort_by")]
    pub sort_by: SortBy,
    #[serde(default)]
    pub colors: ColorConfig,
    #[serde(default)]
    pub keybinds: HashMap<String, String>,
    #[serde(default = "default_theme")]
    pub theme: ThemeName,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortBy {
    Name,
    Size,
    Date,
    Extension,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorConfig {
    #[serde(default = "default_dir_color")]
    pub directory: String,
    #[serde(default = "default_file_color")]
    pub file: String,
    #[serde(default = "default_symlink_color")]
    pub symlink: String,
    #[serde(default = "default_selected_color")]
    pub selected: String,
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self {
            directory: default_dir_color(),
            file: default_file_color(),
            symlink: default_symlink_color(),
            selected: default_selected_color(),
        }
    }
}

fn default_show_hidden() -> bool {
    false
}
fn default_sort_by() -> SortBy {
    SortBy::Name
}
fn default_dir_color() -> String {
    "blue".into()
}
fn default_file_color() -> String {
    "white".into()
}
fn default_symlink_color() -> String {
    "cyan".into()
}
fn default_selected_color() -> String {
    "yellow".into()
}
fn default_theme() -> ThemeName {
    ThemeName::Default
}

impl Default for Config {
    fn default() -> Self {
        Self {
            show_hidden: false,
            sort_by: SortBy::Name,
            colors: ColorConfig::default(),
            keybinds: HashMap::new(),
            theme: ThemeName::Default,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(config) = toml::from_str(&content) {
                    return config;
                }
            }
        }
        Self::default()
    }

    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("velo")
            .join("config.toml")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(!config.show_hidden);
        assert_eq!(config.sort_by, SortBy::Name);
    }

    #[test]
    fn test_config_deserialize() {
        let toml_str = r#"
            show_hidden = true
            sort_by = "size"
            [colors]
            directory = "green"
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.show_hidden);
        assert_eq!(config.sort_by, SortBy::Size);
        assert_eq!(config.colors.directory, "green");
    }

    #[test]
    fn test_config_serialize() {
        let config = Config::default();
        let s = toml::to_string(&config).unwrap();
        assert!(s.contains("show_hidden"));
    }

    #[test]
    fn test_sort_by_variants() {
        let cases = [
            ("\"name\"", SortBy::Name),
            ("\"size\"", SortBy::Size),
            ("\"date\"", SortBy::Date),
            ("\"extension\"", SortBy::Extension),
        ];
        for (s, expected) in cases {
            let v: SortBy = serde_json::from_str(s).unwrap();
            assert_eq!(v, expected);
        }
    }

    #[test]
    fn test_config_path_not_empty() {
        let p = Config::config_path();
        assert!(p.to_str().unwrap().contains("velo"));
    }
}
