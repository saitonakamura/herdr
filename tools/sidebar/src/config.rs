//! `[sidebar]` from herdr's config files: `config.toml`, with `config.fork.toml`
//! beside it merged over it the way herdr merges the overlay (tables key by key,
//! anything else replaces).

use std::path::{Path, PathBuf};

use ratatui::style::Color;

use crate::glyphs::{parse_color, GlyphSet, Glyphs, Meaning};

/// The tokens a row's first line may carry, in the order given.
pub const LINE1_TOKENS: &[&str] = &[
    "name", "effort", "task", "ticket", "messages", "source", "age",
];
/// The tokens of the dim second line.
pub const LINE2_TOKENS: &[&str] = &["branch", "ahead_behind", "dirty", "worktree"];

#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub command: Vec<String>,
    pub popup: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub glyphs: Glyphs,
    pub line1: Vec<String>,
    pub line2: Vec<String>,
    /// The dim colour of tokens, line 2 and other panes.
    pub muted: Color,
    /// The selected row's background while the sidebar has keyboard focus.
    pub selection: Color,
    pub actions: Vec<(String, Action)>,
    /// Problems found while reading, shown once in the status line.
    pub diagnostics: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            glyphs: Glyphs::new(GlyphSet::Nerd, &[], &[]),
            line1: LINE1_TOKENS.iter().map(|t| t.to_string()).collect(),
            line2: LINE2_TOKENS.iter().map(|t| t.to_string()).collect(),
            // herdr's sidebar token colour in the dotfiles, legible on both tokyo-night themes
            muted: Color::Rgb(0x75, 0x7e, 0xad),
            // ANSI 8 follows the terminal's light/dark theme
            selection: Color::Indexed(8),
            actions: Vec::new(),
            diagnostics: Vec::new(),
        }
    }
}

impl Config {
    pub fn action(&self, name: &str) -> Option<&Action> {
        self.actions
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, action)| action)
    }
}

/// herdr's own rule: `HERDR_CONFIG_PATH`, else `$XDG_CONFIG_HOME/herdr`, else `~/.config/herdr`.
/// The server passes `HERDR_CONFIG_PATH` to the session pane, so this matches it.
pub fn config_path() -> PathBuf {
    if let Some(path) = std::env::var_os("HERDR_CONFIG_PATH") {
        return PathBuf::from(path);
    }
    let dir = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"),
    };
    dir.join("herdr").join("config.toml")
}

pub fn overlay_path(path: &Path) -> PathBuf {
    path.with_extension("fork.toml")
}

fn merge(base: &mut toml::Table, overlay: toml::Table) {
    for (key, value) in overlay {
        match (base.get_mut(&key), value) {
            (Some(toml::Value::Table(base)), toml::Value::Table(overlay)) => merge(base, overlay),
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}

fn read_table(path: &Path, diagnostics: &mut Vec<String>) -> toml::Table {
    match std::fs::read_to_string(path) {
        Ok(text) => match text.trim_start_matches('\u{feff}').parse::<toml::Table>() {
            Ok(table) => table,
            Err(err) => {
                diagnostics.push(format!("{}: {err}", path.display()));
                toml::Table::new()
            }
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => toml::Table::new(),
        Err(err) => {
            diagnostics.push(format!("{}: {err}", path.display()));
            toml::Table::new()
        }
    }
}

pub fn load(path: &Path) -> Config {
    let mut diagnostics = Vec::new();
    let mut table = read_table(path, &mut diagnostics);
    merge(
        &mut table,
        read_table(&overlay_path(path), &mut diagnostics),
    );
    let sidebar = match table.remove("sidebar") {
        Some(toml::Value::Table(sidebar)) => sidebar,
        Some(_) => {
            diagnostics.push("[sidebar] is not a table".into());
            toml::Table::new()
        }
        None => toml::Table::new(),
    };
    let mut config = from_table(&sidebar);
    config.diagnostics.splice(0..0, diagnostics);
    config
}

/// Builds the config from the `[sidebar]` table; anything it can't use is a diagnostic.
pub fn from_table(table: &toml::Table) -> Config {
    let mut config = Config::default();
    let mut diagnostics = Vec::new();
    let set = match table.get("glyphs").and_then(toml::Value::as_str) {
        None | Some("nerd") => GlyphSet::Nerd,
        Some("unicode") => GlyphSet::Unicode,
        Some(other) => {
            diagnostics.push(format!(
                "glyphs = {other:?}: expected \"nerd\" or \"unicode\""
            ));
            GlyphSet::Nerd
        }
    };
    let mut glyph_overrides = Vec::new();
    for (key, value) in table
        .get("glyph")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flatten()
    {
        match (Meaning::from_key(key), value.as_str()) {
            (Some(meaning), Some(text)) => glyph_overrides.push((meaning, text.to_string())),
            _ => diagnostics.push(format!(
                "[sidebar.glyph] {key}: unknown meaning or not a string"
            )),
        }
    }
    let mut color_overrides = Vec::new();
    for (key, value) in table
        .get("colors")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flatten()
    {
        let color = value.as_str().and_then(parse_color);
        match (key.as_str(), Meaning::from_key(key), color) {
            ("muted", _, Some(color)) => config.muted = color,
            ("selection", _, Some(color)) => config.selection = color,
            (_, Some(meaning), Some(color)) => color_overrides.push((meaning, color)),
            _ => diagnostics.push(format!("[sidebar.colors] {key}: unknown meaning or colour")),
        }
    }
    config.glyphs = Glyphs::new(set, &glyph_overrides, &color_overrides);
    for (key, allowed, slot) in [
        ("line1", LINE1_TOKENS, &mut config.line1),
        ("line2", LINE2_TOKENS, &mut config.line2),
    ] {
        let Some(value) = table.get(key) else {
            continue;
        };
        let Some(items) = value.as_array() else {
            diagnostics.push(format!("{key}: expected a list of tokens"));
            continue;
        };
        *slot = items
            .iter()
            .filter_map(|item| {
                let token = item.as_str()?;
                if allowed.contains(&token) {
                    Some(token.to_string())
                } else {
                    diagnostics.push(format!("{key}: unknown token {token:?}"));
                    None
                }
            })
            .collect();
    }
    for (name, value) in table
        .get("actions")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flatten()
    {
        let command = value
            .get("command")
            .and_then(toml::Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect::<Vec<_>>()
            })
            .filter(|command| !command.is_empty());
        let Some(command) = command else {
            diagnostics.push(format!("[sidebar.actions] {name}: needs command = [argv]"));
            continue;
        };
        let popup = value
            .get("popup")
            .and_then(toml::Value::as_bool)
            .unwrap_or(false);
        config
            .actions
            .push((name.clone(), Action { command, popup }));
    }
    for key in table.keys() {
        if !["glyphs", "glyph", "colors", "line1", "line2", "actions"].contains(&key.as_str()) {
            diagnostics.push(format!("[sidebar] {key}: unknown key"));
        }
    }
    config.diagnostics = diagnostics;
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(text: &str) -> toml::Table {
        text.parse().unwrap()
    }

    #[test]
    fn defaults_carry_every_token() {
        let config = from_table(&toml::Table::new());
        assert_eq!(config.line1, LINE1_TOKENS);
        assert_eq!(config.line2, LINE2_TOKENS);
        assert!(config.diagnostics.is_empty());
    }

    #[test]
    fn reads_glyphs_lines_colours_and_actions() {
        let config = from_table(&table(
            r##"
            glyphs = "unicode"
            line1 = ["name", "age"]
            line2 = []
            colors = { muted = "#000001", idle = "blue" }
            glyph = { idle = "o" }
            actions = { close = { command = ["loom", "close", "{agent_id}"], popup = true } }
            "##,
        ));
        assert!(config.diagnostics.is_empty(), "{:?}", config.diagnostics);
        assert_eq!(config.line1, ["name", "age"]);
        assert!(config.line2.is_empty());
        assert_eq!(config.muted, Color::Rgb(0, 0, 1));
        assert_eq!(config.glyphs.get(Meaning::Idle).text, "o");
        assert_eq!(config.glyphs.get(Meaning::Idle).color, Some(Color::Blue));
        let close = config.action("close").unwrap();
        assert!(close.popup);
        assert_eq!(close.command, ["loom", "close", "{agent_id}"]);
    }

    #[test]
    fn unusable_entries_become_diagnostics() {
        let config = from_table(&table(
            r#"
            glyphs = "emoji"
            line1 = ["name", "bogus"]
            actions = { reply = { popup = true } }
            extra = 1
            "#,
        ));
        assert_eq!(config.diagnostics.len(), 4, "{:?}", config.diagnostics);
        assert_eq!(config.line1, ["name"]);
        assert!(config.action("reply").is_none());
    }

    #[test]
    fn the_overlay_merges_over_the_base_file() {
        let dir = std::env::temp_dir().join(format!("herdr-sidebar-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "[sidebar]\nglyphs = \"unicode\"\nline2 = []\n").unwrap();
        std::fs::write(overlay_path(&path), "[sidebar]\nline1 = [\"name\"]\n").unwrap();
        let config = load(&path);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(config.glyphs.set, GlyphSet::Unicode);
        assert_eq!(config.line1, ["name"]);
        assert!(config.line2.is_empty());
    }
}
