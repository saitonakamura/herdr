//! A tab with exactly one agent follows a slug of that agent's task title,
//! until a hand rename pins it.

use std::path::Path;

pub(crate) const DEFAULT_TAB_NAME_MAX_LENGTH: usize = 24;

/// Where a tab's shown label comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabLabelSource {
    Pinned,
    Following,
    Number,
}

/// An empty label or a bare number gives the tab back to its agent.
pub(crate) fn label_unpins(label: &str) -> bool {
    let label = label.trim();
    label.is_empty() || label.bytes().all(|byte| byte.is_ascii_digit())
}

/// The name a tab follows for its one agent, or `None` when the title says
/// nothing beyond the agent or its directory.
pub(crate) fn followed_tab_name(
    title_stripped: Option<&str>,
    cwd: Option<&Path>,
    foreground_cwd: Option<&Path>,
    max_length: usize,
) -> Option<String> {
    let title = task_title(title_stripped?, cwd, foreground_cwd)?;
    let name = slug(title, max_length);
    (!name.is_empty()).then_some(name)
}

fn dir_name(path: Option<&Path>) -> &str {
    path.and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or("")
}

fn task_title<'a>(
    title: &'a str,
    cwd: Option<&Path>,
    foreground_cwd: Option<&Path>,
) -> Option<&'a str> {
    let base = dir_name(cwd);
    let foreground_base = dir_name(foreground_cwd);
    let mut title = title.trim();
    // pi titles itself "π - <session name> - <cwd>", or "π - <cwd>" until the session is named
    if let Some(rest) = title
        .strip_prefix("π - ")
        .or_else(|| title.strip_prefix("pi - "))
    {
        title = [base, foreground_base]
            .into_iter()
            .filter(|dir| !dir.is_empty())
            .find_map(|dir| rest.strip_suffix(dir)?.strip_suffix(" - "))?;
    }
    // codex titles itself with the cwd, and a fresh claude says only its own name
    let bare = title.is_empty()
        || title == base
        || (!foreground_base.is_empty() && title == foreground_base)
        || ["claude code", "claude", "codex", "pi", "π"]
            .iter()
            .any(|agent| title.eq_ignore_ascii_case(agent));
    (!bare).then_some(title)
}

/// Lower-case words joined by `-`, starting with a letter, at most
/// `max_length` long and cut between words where that keeps half of it.
pub(crate) fn slug(text: &str, max_length: usize) -> String {
    let mut slug = String::with_capacity(text.len());
    for ch in text.chars() {
        let ch = ch.to_ascii_lowercase();
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' {
            slug.push(ch);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let start = slug
        .find(|ch: char| ch.is_ascii_lowercase())
        .unwrap_or(slug.len());
    let slug = slug[start..].trim_end_matches('-');
    if slug.len() <= max_length {
        return slug.to_string();
    }
    let head = &slug[..=max_length];
    let cut = if head.ends_with('-') {
        &head[..max_length]
    } else if let Some(dash) = head.rfind('-') {
        &head[..dash]
    } else {
        &head[..max_length]
    };
    let cut = if cut.len() < max_length / 2 {
        &head[..max_length]
    } else {
        cut
    };
    cut.trim_end_matches(['-', '_']).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_joins_lowercase_words_and_starts_with_a_letter() {
        assert_eq!(slug("  42 Fix the Tab-bar!! ", 24), "fix-the-tab-bar");
        assert_eq!(slug("修复 tab names", 24), "tab-names");
        assert_eq!(slug("snake_case_name", 24), "snake_case_name");
        assert_eq!(slug("123 ---", 24), "");
    }

    #[test]
    fn slug_cuts_between_words_within_the_limit() {
        assert_eq!(
            slug("Loom tab names follow task titles", 24),
            "loom-tab-names-follow"
        );
        assert_eq!(
            slug("exactly twenty four chrs", 24),
            "exactly-twenty-four-chrs"
        );
        assert_eq!(slug("abcdefghij abcdefghijklm", 10), "abcdefghij");
    }

    #[test]
    fn slug_cuts_mid_word_when_a_word_cut_would_lose_half() {
        assert_eq!(slug("ab supercalifragilistic", 12), "ab-supercali");
        assert_eq!(slug("supercalifragilistic", 8), "supercal");
    }

    #[test]
    fn bare_numbers_and_empty_labels_unpin() {
        assert!(label_unpins(""));
        assert!(label_unpins("  "));
        assert!(label_unpins("3"));
        assert!(!label_unpins("3a"));
        assert!(!label_unpins("main"));
    }

    #[test]
    fn a_title_naming_only_the_directory_is_not_followed() {
        let cwd = Path::new("/code/herdr");
        assert_eq!(followed_tab_name(Some("herdr"), Some(cwd), None, 24), None);
        assert_eq!(followed_tab_name(None, Some(cwd), None, 24), None);
        assert_eq!(
            followed_tab_name(Some("Review the diff"), Some(cwd), None, 24).as_deref(),
            Some("review-the-diff")
        );
    }
}
