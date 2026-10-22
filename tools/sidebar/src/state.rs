//! Where the sidebar keeps its own presentation state, and its control socket.

use std::path::{Path, PathBuf};

use crate::model::UiState;

pub fn session_name() -> String {
    std::env::var("HERDR_SESSION")
        .ok()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "default".into())
}

/// `$XDG_STATE_HOME/herdr/sidebar`, else `~/.local/state/herdr/sidebar`.
pub fn state_dir() -> PathBuf {
    let base = match std::env::var_os("XDG_STATE_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state"),
    };
    base.join("herdr").join("sidebar")
}

/// A session name is a path segment; anything else in it is replaced.
fn file_stem(session: &str) -> String {
    session
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "-_.".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub fn state_path(dir: &Path, session: &str) -> PathBuf {
    dir.join(format!("{}.json", file_stem(session)))
}

/// herdr's socket, resolved the way herdr does: `HERDR_SOCKET_PATH`, then
/// `HERDR_SESSION`, then the default session, all under the config directory.
pub fn herdr_socket() -> PathBuf {
    if let Some(path) = std::env::var_os("HERDR_SOCKET_PATH").filter(|p| !p.is_empty()) {
        return PathBuf::from(path);
    }
    let config = crate::config::config_path();
    let dir = config.parent().map(Path::to_path_buf).unwrap_or_default();
    match std::env::var("HERDR_SESSION")
        .ok()
        .filter(|name| !name.is_empty() && name != "default")
    {
        Some(name) => dir.join("sessions").join(name).join("herdr.sock"),
        None => dir.join("herdr.sock"),
    }
}

/// The program's control socket sits beside herdr's, so anything holding
/// `HERDR_SOCKET_PATH` (every pane and command) finds the right session's sidebar.
pub fn ctl_socket(herdr_socket: &Path) -> PathBuf {
    let mut path = herdr_socket.as_os_str().to_owned();
    path.push(".sidebar");
    PathBuf::from(path)
}

pub fn load(path: &Path) -> UiState {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Written beside and renamed over, so a crash never leaves half a file.
pub fn save(path: &Path, state: &UiState) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(
        &tmp,
        serde_json::to_vec_pretty(state).map_err(std::io::Error::other)?,
    )?;
    std::fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Scope;

    #[test]
    fn state_round_trips_and_a_missing_file_is_empty() {
        let dir = std::env::temp_dir().join(format!("herdr-sidebar-state-{}", std::process::id()));
        let path = state_path(&dir, "fork/side");
        assert!(path.ends_with("fork_side.json"));
        assert_eq!(load(&path), UiState::default());
        let mut state = UiState::default();
        state.collapsed.insert("w2".into());
        state.folded.insert("a".into());
        state.scope = Some(Scope::Subtree("a".into()));
        save(&path, &state).unwrap();
        assert_eq!(load(&path), state);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_ctl_socket_sits_beside_herdrs() {
        assert_eq!(
            ctl_socket(Path::new("/cfg/herdr/sessions/fork/herdr.sock")),
            PathBuf::from("/cfg/herdr/sessions/fork/herdr.sock.sidebar")
        );
    }
}
