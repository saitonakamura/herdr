//! Build identity helpers.

/// The full package version, build metadata included (`0.9.5+fork.1`).
pub const PACKAGE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The semver core (`0.9.5`): what version comparisons and plugin
/// `min_herdr_version` checks use.
pub const BASE_VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION_MAJOR"),
    ".",
    env!("CARGO_PKG_VERSION_MINOR"),
    ".",
    env!("CARGO_PKG_VERSION_PATCH")
);

pub fn channel() -> &'static str {
    non_empty(option_env!("HERDR_BUILD_CHANNEL")).unwrap_or("stable")
}

pub fn build_id() -> Option<&'static str> {
    non_empty(option_env!("HERDR_BUILD_ID"))
}

pub fn version() -> String {
    match channel() {
        "stable" => PACKAGE_VERSION.to_string(),
        channel => match build_id() {
            Some(build_id) => format!("{BASE_VERSION}-{channel}.{build_id}"),
            None => format!("{BASE_VERSION}-{channel}"),
        },
    }
}

pub fn is_preview() -> bool {
    channel() == "preview"
}

/// Builds of the saitonakamura/herdr fork carry `+fork.N` build metadata and
/// never update themselves from upstream's channels.
pub fn is_fork() -> bool {
    is_fork_version(PACKAGE_VERSION)
}

fn is_fork_version(version: &str) -> bool {
    version
        .split_once('+')
        .is_some_and(|(_, metadata)| metadata.split('.').next() == Some("fork"))
}

fn non_empty(value: Option<&'static str>) -> Option<&'static str> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn stable_version_defaults_to_cargo_version() {
        assert!(!super::version().is_empty());
    }

    #[test]
    fn base_version_is_the_semver_core_of_the_package_version() {
        assert!(super::PACKAGE_VERSION.starts_with(super::BASE_VERSION));
        assert!(!super::BASE_VERSION.contains('+'));
    }

    #[test]
    fn fork_builds_are_recognized_by_build_metadata() {
        assert!(super::is_fork_version("0.9.5+fork.1"));
        assert!(super::is_fork_version("0.9.5+fork"));
        assert!(!super::is_fork_version("0.9.5"));
        assert!(!super::is_fork_version("0.9.5+forked.1"));
        assert!(!super::is_fork_version("0.9.5-preview.fork"));
    }
}
