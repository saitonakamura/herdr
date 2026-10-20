use std::path::Path;

use serde::{Deserialize, Serialize};

const MAX_SESSION_ID_LEN: usize = 512;
const MAX_SESSION_PATH_LEN: usize = 4096;
const MAX_RESUME_ARGS: usize = 64;
const MAX_RESUME_ARGV_BYTES: usize = 8192;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSessionRef {
    pub kind: AgentSessionRefKind,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentSessionRefKind {
    Id,
    Path,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentResumePlan {
    pub agent: String,
    pub argv: Vec<String>,
    pub dedupe_key: String,
    /// Set (`Some`) or removed (`None`) for the resumed command only, after the
    /// shell's own startup, so a directory's environment cannot override it.
    pub env: Vec<(String, Option<String>)>,
}

/// The arguments an agent was started with, kept so a resume starts it the same way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentLaunch {
    pub agent: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
}

/// The profile directory an agent's session is written to, which the
/// directory's own environment may not select on a resume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentHome {
    pub agent: String,
    pub path: String,
}

/// A resume command reported by the agent itself, run in the restored pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportedAgentResume {
    pub source: String,
    pub agent: String,
    pub argv: Vec<String>,
}

impl ReportedAgentResume {
    /// The same command can name different sessions in different directories,
    /// for example `agent --continue`, so the directory is part of its identity.
    pub fn plan(&self, cwd: &Path) -> AgentResumePlan {
        AgentResumePlan {
            agent: self.agent.clone(),
            argv: self.argv.clone(),
            dedupe_key: format!(
                "{}\u{0}{}\u{0}{}\u{0}argv\u{0}{}",
                self.source,
                self.agent,
                cwd.display(),
                self.argv.join("\u{0}")
            ),
            env: Vec::new(),
        }
    }
}

/// Restore types the command into the pane's shell, so the executable must be a
/// bare command name: shells disagree on how to invoke a quoted path.
pub fn validate_resume_argv(argv: &[String]) -> Result<(), String> {
    let Some(command) = argv.first() else {
        return Err("resume_argv must not be empty".into());
    };
    if argv.len() > MAX_RESUME_ARGS {
        return Err(format!(
            "resume_argv allows at most {MAX_RESUME_ARGS} arguments"
        ));
    }
    if argv.iter().map(String::len).sum::<usize>() > MAX_RESUME_ARGV_BYTES {
        return Err(format!(
            "resume_argv allows at most {MAX_RESUME_ARGV_BYTES} bytes"
        ));
    }
    if argv.iter().any(|arg| arg.chars().any(char::is_control)) {
        return Err("resume_argv must not contain control characters".into());
    }
    // Restore quotes arguments POSIX-style, which PowerShell reads differently
    // only when an argument itself contains an apostrophe.
    if argv.iter().any(|arg| arg.contains('\'')) {
        return Err("resume_argv must not contain apostrophes".into());
    }
    let plain_command = !command.is_empty()
        && !command.starts_with('-')
        && command
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'));
    if !plain_command {
        return Err("resume_argv must start with a plain command name, not a path".into());
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedAgentSession {
    pub source: String,
    pub agent: String,
    pub session_ref: AgentSessionRef,
}

impl AgentSessionRef {
    pub fn id(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        valid_session_id(&value).then_some(Self {
            kind: AgentSessionRefKind::Id,
            value,
        })
    }

    pub fn path(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        valid_session_path(&value).then_some(Self {
            kind: AgentSessionRefKind::Path,
            value,
        })
    }
}

pub fn session_ref_from_report(
    source: &str,
    agent: &str,
    agent_session_id: Option<String>,
    _agent_session_path: Option<String>,
) -> Option<AgentSessionRef> {
    if !is_official_agent_source(source, agent) {
        return None;
    }

    if agent == "pi" || agent == "omp" {
        return _agent_session_path
            .and_then(AgentSessionRef::path)
            .or_else(|| agent_session_id.and_then(AgentSessionRef::id));
    }

    agent_session_id.and_then(AgentSessionRef::id)
}

pub fn persisted_session_from_launch_args(
    agent: crate::detect::Agent,
    args: &[String],
) -> Option<PersistedAgentSession> {
    let [command, session_id] = args else {
        return None;
    };
    if agent != crate::detect::Agent::Codex || command != "resume" || session_id.starts_with('-') {
        return None;
    }

    Some(PersistedAgentSession {
        source: "herdr:codex".into(),
        agent: "codex".into(),
        session_ref: AgentSessionRef::id(session_id.clone())?,
    })
}

pub fn normalize_session_start_source(value: Option<String>) -> Option<String> {
    match value.as_deref().map(str::trim) {
        Some(
            source @ ("startup" | "resume" | "clear" | "compact" | "branch" | "new" | "fork"
            | "select"),
        ) => Some(source.to_string()),
        _ => None,
    }
}

pub fn is_reserved_native_state_source(source: &str, agent: &str) -> bool {
    matches!(
        (source, agent),
        ("herdr:codex", "codex")
            | ("herdr:copilot", "copilot")
            | ("herdr:devin", "devin")
            | ("herdr:droid", "droid")
            | ("herdr:qodercli", "qodercli")
            | ("herdr:qwen", "qwen")
            | ("herdr:cursor", "cursor")
            | ("herdr:grok", "grok")
    )
}

pub fn session_ref_from_snapshot(
    source: &str,
    agent: &str,
    kind: AgentSessionRefKind,
    value: &str,
) -> Option<PersistedAgentSession> {
    if !is_official_agent_source(source, agent) {
        return None;
    }
    let session_ref = match (agent, kind) {
        ("pi" | "omp", AgentSessionRefKind::Path) => AgentSessionRef::path(value)?,
        (_, AgentSessionRefKind::Id) => AgentSessionRef::id(value)?,
        _ => return None,
    };
    Some(PersistedAgentSession {
        source: source.to_string(),
        agent: agent.to_string(),
        session_ref,
    })
}

pub fn plan(source: &str, agent: &str, session_ref: &AgentSessionRef) -> Option<AgentResumePlan> {
    if !is_official_agent_source(source, agent) {
        return None;
    }

    let argv = match (source, agent, session_ref.kind) {
        ("herdr:claude", "claude", AgentSessionRefKind::Id) => {
            vec![
                "claude".into(),
                "--resume".into(),
                session_ref.value.clone(),
            ]
        }
        ("herdr:codex", "codex", AgentSessionRefKind::Id) => {
            vec!["codex".into(), "resume".into(), session_ref.value.clone()]
        }
        ("herdr:copilot", "copilot", AgentSessionRefKind::Id) => {
            vec!["copilot".into(), format!("--resume={}", session_ref.value)]
        }
        ("herdr:devin", "devin", AgentSessionRefKind::Id) => {
            vec!["devin".into(), "--resume".into(), session_ref.value.clone()]
        }
        ("herdr:droid", "droid", AgentSessionRefKind::Id) => {
            vec!["droid".into(), "--resume".into(), session_ref.value.clone()]
        }
        ("herdr:kimi", "kimi", AgentSessionRefKind::Id) => {
            vec!["kimi".into(), "--session".into(), session_ref.value.clone()]
        }
        ("herdr:mastracode", "mastracode", AgentSessionRefKind::Id) => {
            vec![
                "mastracode".into(),
                "--thread".into(),
                session_ref.value.clone(),
            ]
        }
        ("herdr:pi", "pi", AgentSessionRefKind::Path | AgentSessionRefKind::Id) => {
            vec!["pi".into(), "--session".into(), session_ref.value.clone()]
        }
        ("herdr:omp", "omp", AgentSessionRefKind::Path | AgentSessionRefKind::Id) => {
            // omp resume is `-r, --resume=<value>` (ID prefix or path); it has no
            // `--session` flag, unlike pi.
            vec!["omp".into(), format!("--resume={}", session_ref.value)]
        }
        ("herdr:hermes", "hermes", AgentSessionRefKind::Id) => {
            vec![
                "hermes".into(),
                "--resume".into(),
                session_ref.value.clone(),
            ]
        }
        ("herdr:opencode", "opencode", AgentSessionRefKind::Id) => {
            vec![
                "opencode".into(),
                "--session".into(),
                session_ref.value.clone(),
            ]
        }
        ("herdr:qodercli", "qodercli", AgentSessionRefKind::Id) => {
            vec![
                "qodercli".into(),
                "--resume".into(),
                session_ref.value.clone(),
            ]
        }
        ("herdr:qwen", "qwen", AgentSessionRefKind::Id) => {
            vec!["qwen".into(), "--resume".into(), session_ref.value.clone()]
        }
        ("herdr:kilo", "kilo", AgentSessionRefKind::Id) => {
            vec!["kilo".into(), "--session".into(), session_ref.value.clone()]
        }
        ("herdr:cursor", "cursor", AgentSessionRefKind::Id) => {
            vec![
                if cfg!(windows) {
                    "cursor-agent.cmd"
                } else {
                    "cursor-agent"
                }
                .into(),
                "--resume".into(),
                session_ref.value.clone(),
            ]
        }
        ("herdr:antigravity_cli", "agy", AgentSessionRefKind::Id) => {
            vec![
                "agy".into(),
                "--conversation".into(),
                session_ref.value.clone(),
            ]
        }
        ("herdr:grok", "grok", AgentSessionRefKind::Id) => {
            vec!["grok".into(), "--resume".into(), session_ref.value.clone()]
        }
        ("herdr:letta", "letta", AgentSessionRefKind::Id) => {
            if let Some(agent_id) = session_ref.value.strip_prefix("default:") {
                if agent_id.is_empty() {
                    return None;
                }
                vec![
                    "letta".into(),
                    "--conversation".into(),
                    "default".into(),
                    "--agent".into(),
                    agent_id.into(),
                ]
            } else {
                vec![
                    "letta".into(),
                    "--conversation".into(),
                    session_ref.value.clone(),
                ]
            }
        }
        _ => return None,
    };

    Some(AgentResumePlan {
        agent: agent.to_string(),
        argv,
        dedupe_key: dedupe_key(source, agent, session_ref),
        env: Vec::new(),
    })
}

impl AgentResumePlan {
    /// Resumes with the agent's own start arguments and profile directory.
    /// Only agents whose session-selecting flags are known get their arguments
    /// back, so a resume never also asks for another session.
    pub fn with_launch(
        mut self,
        launch: Option<&AgentLaunch>,
        home: Option<&AgentHome>,
        user_home: Option<&Path>,
    ) -> Self {
        if let Some(args) = launch
            .filter(|launch| launch.agent == self.agent)
            .and_then(|launch| resume_args(&launch.agent, &launch.args))
        {
            self.argv.extend(args);
        }
        if let Some(entry) = home
            .filter(|home| home.agent == self.agent)
            .and_then(|home| home_env(home, user_home))
        {
            self.env.push(entry);
        }
        self
    }
}

/// The start arguments a resume keeps: everything except what selects or forks
/// a session, runs headless, or names a prompt or subcommand ahead of the first
/// flag. `None` when the agent's flags are not known.
pub fn resume_args(agent: &str, args: &[String]) -> Option<Vec<String>> {
    // Per agent, because a flag means different things: `-c` is claude's
    // continue and codex's config, `-p` claude's print and codex's profile.
    let (valued, bare): (&[&str], &[&str]) = match agent {
        "claude" => (
            &["--session-id", "--resume", "-r"],
            &["--continue", "-c", "--fork-session", "-p", "--print"],
        ),
        "codex" => (&["--image", "-i"], &["--worktree"]),
        "pi" => (
            &[
                "--session",
                "--session-id",
                "--fork",
                "--export",
                "--mode",
                "--name",
                "-n",
            ],
            &[
                "--continue",
                "-c",
                "--resume",
                "-r",
                "--no-session",
                "--print",
                "-p",
            ],
        ),
        _ => return None,
    };
    let mut kept = Vec::new();
    let mut args = args
        .iter()
        .skip_while(|arg| !arg.starts_with('-'))
        .peekable();
    while let Some(arg) = args.next() {
        if !arg.starts_with('-') {
            kept.push(arg.clone());
            continue;
        }
        let (flag, inline_value) = match arg.split_once('=') {
            Some((flag, _)) => (flag, true),
            None => (arg.as_str(), false),
        };
        if valued.contains(&flag) {
            if !inline_value && args.peek().is_some_and(|next| !next.starts_with('-')) {
                args.next();
            }
        } else if !bare.contains(&flag) {
            kept.push(arg.clone());
        }
    }
    Some(kept)
}

pub fn home_env_var(agent: &str) -> Option<&'static str> {
    match agent {
        "claude" => Some("CLAUDE_CONFIG_DIR"),
        "codex" => Some("CODEX_HOME"),
        _ => None,
    }
}

fn default_home_dir(agent: &str) -> Option<&'static str> {
    match agent {
        "claude" => Some(".claude"),
        "codex" => Some(".codex"),
        _ => None,
    }
}

/// The profile directory a session file lives in: claude writes
/// `<home>/projects/<project>/<id>.jsonl`, codex
/// `<home>/sessions/<year>/<month>/<day>/rollout-<...>.jsonl`.
pub fn agent_home_from_session_path(agent: &str, session_path: &str) -> Option<AgentHome> {
    let path = Path::new(session_path);
    if !valid_session_path(session_path) {
        return None;
    }
    let (marker, depth) = match agent {
        "claude" => ("projects", 2),
        "codex" => ("sessions", 4),
        _ => return None,
    };
    let marker_dir = path.ancestors().nth(depth)?;
    if marker_dir.file_name()? != marker {
        return None;
    }
    Some(AgentHome {
        agent: agent.to_string(),
        path: marker_dir.parent()?.to_str()?.to_string(),
    })
}

/// The variable that selects the session's profile: the recorded directory,
/// or removed when that is the agent's default, which reads differently from
/// naming the default explicitly (claude then keeps its config inside it).
fn home_env(home: &AgentHome, user_home: Option<&Path>) -> Option<(String, Option<String>)> {
    let var = home_env_var(&home.agent)?;
    let is_default = user_home
        .zip(default_home_dir(&home.agent))
        .is_some_and(|(user_home, dir)| same_dir(Path::new(&home.path), &user_home.join(dir)));
    Some((var.to_string(), (!is_default).then(|| home.path.clone())))
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

pub fn dedupe_key(source: &str, agent: &str, session_ref: &AgentSessionRef) -> String {
    format!(
        "{source}\u{0}{agent}\u{0}{:?}\u{0}{}",
        session_ref.kind, session_ref.value
    )
}

pub(crate) fn is_official_agent_source(source: &str, agent: &str) -> bool {
    matches!(
        (source, agent),
        ("herdr:claude", "claude")
            | ("herdr:codex", "codex")
            | ("herdr:copilot", "copilot")
            | ("herdr:devin", "devin")
            | ("herdr:droid", "droid")
            | ("herdr:kimi", "kimi")
            | ("herdr:omp", "omp")
            | ("herdr:mastracode", "mastracode")
            | ("herdr:pi", "pi")
            | ("herdr:hermes", "hermes")
            | ("herdr:opencode", "opencode")
            | ("herdr:qodercli", "qodercli")
            | ("herdr:qwen", "qwen")
            | ("herdr:kilo", "kilo")
            | ("herdr:cursor", "cursor")
            | ("herdr:antigravity_cli", "agy")
            | ("herdr:grok", "grok")
            | ("herdr:letta", "letta")
    )
}

fn valid_session_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_SESSION_ID_LEN && !value.chars().any(char::is_control)
}

fn valid_session_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SESSION_PATH_LEN
        && !value.chars().any(char::is_control)
        && Path::new(value).is_absolute()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn absolute_test_path(name: &str) -> String {
        std::env::current_dir()
            .unwrap()
            .join(name)
            .display()
            .to_string()
    }

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| part.to_string()).collect()
    }

    #[test]
    fn reported_resume_argv_requires_a_plain_command_name() {
        assert!(validate_resume_argv(&argv(&[
            "prime-agent",
            "--resume",
            "01a0de21",
            "--model",
            "gpt 6"
        ]))
        .is_ok());
        assert!(validate_resume_argv(&argv(&["cursor-agent.cmd", "--resume", "id"])).is_ok());

        for invalid in [
            argv(&[]),
            argv(&[""]),
            argv(&["/usr/bin/prime-agent", "--resume", "id"]),
            argv(&["C:\\Program Files\\Prime\\prime.exe"]),
            argv(&["prime agent"]),
            argv(&["-prime"]),
            argv(&["prime-agent", "bad\nline"]),
            argv(&["prime-agent", "--name", "can's session"]),
            vec!["prime-agent".to_string(); MAX_RESUME_ARGS + 1],
            argv(&["prime-agent", &"x".repeat(MAX_RESUME_ARGV_BYTES)]),
        ] {
            assert!(validate_resume_argv(&invalid).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn resume_args_drop_what_selects_a_session_per_agent() {
        assert_eq!(
            resume_args(
                "claude",
                &argv(&[
                    "--model",
                    "opus",
                    "--resume",
                    "old",
                    "-c",
                    "--session-id=x",
                    "--effort",
                    "high",
                    "-p",
                ])
            ),
            Some(argv(&["--model", "opus", "--effort", "high"]))
        );
        // codex's -c is its config and -p its profile, so both stay
        assert_eq!(
            resume_args(
                "codex",
                &argv(&[
                    "resume",
                    "old-id",
                    "-c",
                    "check_for_update_on_startup=false",
                    "-p",
                    "work",
                    "-i",
                    "shot.png",
                    "--worktree",
                ])
            ),
            Some(argv(&[
                "-c",
                "check_for_update_on_startup=false",
                "-p",
                "work"
            ]))
        );
        assert_eq!(
            resume_args(
                "pi",
                &argv(&["--model", "x", "--no-session", "--session", "/s.jsonl"])
            ),
            Some(argv(&["--model", "x"]))
        );
        assert_eq!(resume_args("opencode", &argv(&["--model", "x"])), None);
    }

    #[test]
    fn agent_home_comes_from_the_session_file_layout() {
        let home = absolute_test_path("profile");
        let claude = format!("{home}/projects/-repo/abc.jsonl");
        assert_eq!(
            agent_home_from_session_path("claude", &claude).map(|home| home.path),
            Some(home.clone())
        );
        let codex = format!("{home}/sessions/2026/10/16/rollout-x-abc.jsonl");
        assert_eq!(
            agent_home_from_session_path("codex", &codex).map(|home| home.path),
            Some(home.clone())
        );
        assert!(agent_home_from_session_path("claude", &codex).is_none());
        assert!(agent_home_from_session_path("pi", &claude).is_none());
        assert!(agent_home_from_session_path("claude", "projects/-repo/abc.jsonl").is_none());
    }

    #[test]
    fn resume_plan_takes_the_agents_args_and_profile() {
        let session = AgentSessionRef::id("abc").unwrap();
        let base = || plan("herdr:claude", "claude", &session).unwrap();
        let launch = AgentLaunch {
            agent: "claude".into(),
            args: argv(&["--model", "opus", "--resume", "old"]),
        };
        let user_home = std::env::temp_dir().join("herdr-resume-home");
        let other = AgentHome {
            agent: "claude".into(),
            path: absolute_test_path("profile-personal"),
        };

        let resumed = base().with_launch(Some(&launch), Some(&other), Some(&user_home));
        assert_eq!(
            resumed.argv,
            argv(&["claude", "--resume", "abc", "--model", "opus"])
        );
        assert_eq!(
            resumed.env,
            vec![("CLAUDE_CONFIG_DIR".to_string(), Some(other.path.clone()))]
        );
        assert_eq!(resumed.dedupe_key, base().dedupe_key);

        // the default profile is selected by leaving the variable unset
        let default = AgentHome {
            agent: "claude".into(),
            path: user_home.join(".claude").display().to_string(),
        };
        assert_eq!(
            base()
                .with_launch(None, Some(&default), Some(&user_home))
                .env,
            vec![("CLAUDE_CONFIG_DIR".to_string(), None)]
        );

        // args and home recorded for another agent are not this session's
        let codex_launch = AgentLaunch {
            agent: "codex".into(),
            args: argv(&["-c", "x=y"]),
        };
        let codex_home = AgentHome {
            agent: "codex".into(),
            path: other.path.clone(),
        };
        assert_eq!(
            base().with_launch(Some(&codex_launch), Some(&codex_home), Some(&user_home)),
            base()
        );
    }

    #[test]
    fn native_state_reservation_excludes_full_lifecycle_sources() {
        assert!(!is_reserved_native_state_source("herdr:claude", "claude"));
        assert!(is_reserved_native_state_source("herdr:codex", "codex"));
        assert!(is_reserved_native_state_source("herdr:devin", "devin"));
        assert!(!is_reserved_native_state_source("herdr:kimi", "kimi"));
        assert!(!is_reserved_native_state_source(
            "herdr:opencode",
            "opencode"
        ));
    }

    #[test]
    fn codex_noncanonical_resume_launch_has_no_explicit_session() {
        assert_eq!(
            persisted_session_from_launch_args(
                crate::detect::Agent::Codex,
                &["resume".into(), "codex-session".into()]
            )
            .unwrap()
            .session_ref
            .value,
            "codex-session"
        );
        assert!(persisted_session_from_launch_args(
            crate::detect::Agent::Codex,
            &["resume".into(), "--last".into()]
        )
        .is_none());
        assert!(persisted_session_from_launch_args(
            crate::detect::Agent::Codex,
            &["resume".into(), "not-a-session".into(), "--last".into()]
        )
        .is_none());
        assert!(persisted_session_from_launch_args(
            crate::detect::Agent::Codex,
            &[
                "--remote".into(),
                "ws://example.test".into(),
                "resume".into(),
                "remote-session".into(),
            ]
        )
        .is_none());
    }

    #[test]
    fn planner_allows_supported_agents() {
        let pi_session = absolute_test_path("pi-session.jsonl");
        let omp_session = absolute_test_path("omp-session.jsonl");
        assert_eq!(
            plan(
                "herdr:claude",
                "claude",
                &AgentSessionRef::id("claude-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["claude", "--resume", "claude-session"]
        );
        assert_eq!(
            plan(
                "herdr:codex",
                "codex",
                &AgentSessionRef::id("codex-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["codex", "resume", "codex-session"]
        );
        assert_eq!(
            plan(
                "herdr:copilot",
                "copilot",
                &AgentSessionRef::id("copilot-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["copilot", "--resume=copilot-session"]
        );
        assert_eq!(
            plan(
                "herdr:devin",
                "devin",
                &AgentSessionRef::id("devin-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["devin", "--resume", "devin-session"]
        );
        assert_eq!(
            plan(
                "herdr:droid",
                "droid",
                &AgentSessionRef::id("droid-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["droid", "--resume", "droid-session"]
        );
        assert_eq!(
            plan(
                "herdr:kimi",
                "kimi",
                &AgentSessionRef::id("kimi-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["kimi", "--session", "kimi-session"]
        );
        assert_eq!(
            plan(
                "herdr:mastracode",
                "mastracode",
                &AgentSessionRef::id("mastracode-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["mastracode", "--thread", "mastracode-session"]
        );
        assert_eq!(
            plan(
                "herdr:pi",
                "pi",
                &AgentSessionRef::path(&pi_session).unwrap()
            )
            .unwrap()
            .argv,
            vec!["pi", "--session", pi_session.as_str()]
        );
        assert_eq!(
            plan(
                "herdr:omp",
                "omp",
                &AgentSessionRef::path(&omp_session).unwrap()
            )
            .unwrap()
            .argv,
            vec!["omp", format!("--resume={omp_session}").as_str()]
        );
        assert_eq!(
            plan(
                "herdr:hermes",
                "hermes",
                &AgentSessionRef::id("hermes-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["hermes", "--resume", "hermes-session"]
        );
        assert_eq!(
            plan(
                "herdr:opencode",
                "opencode",
                &AgentSessionRef::id("opencode-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["opencode", "--session", "opencode-session"]
        );
        assert_eq!(
            plan(
                "herdr:qodercli",
                "qodercli",
                &AgentSessionRef::id("qoder-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["qodercli", "--resume", "qoder-session"]
        );
        assert_eq!(
            plan(
                "herdr:qwen",
                "qwen",
                &AgentSessionRef::id("qwen-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["qwen", "--resume", "qwen-session"]
        );
        assert_eq!(
            plan(
                "herdr:kilo",
                "kilo",
                &AgentSessionRef::id("kilo-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["kilo", "--session", "kilo-session"]
        );
        assert_eq!(
            plan(
                "herdr:cursor",
                "cursor",
                &AgentSessionRef::id("cursor-session").unwrap()
            )
            .unwrap()
            .argv,
            vec![
                if cfg!(windows) {
                    "cursor-agent.cmd"
                } else {
                    "cursor-agent"
                },
                "--resume",
                "cursor-session",
            ]
        );
        assert_eq!(
            plan(
                "herdr:antigravity_cli",
                "agy",
                &AgentSessionRef::id("agy-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["agy", "--conversation", "agy-session"]
        );
        assert_eq!(
            plan(
                "herdr:grok",
                "grok",
                &AgentSessionRef::id("grok-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["grok", "--resume", "grok-session"]
        );
        assert_eq!(
            plan(
                "herdr:letta",
                "letta",
                &AgentSessionRef::id("conversation-123").unwrap()
            )
            .unwrap()
            .argv,
            vec!["letta", "--conversation", "conversation-123"]
        );
        assert_eq!(
            plan(
                "herdr:letta",
                "letta",
                &AgentSessionRef::id("default:agent-123").unwrap()
            )
            .unwrap()
            .argv,
            vec!["letta", "--conversation", "default", "--agent", "agent-123"]
        );
        assert!(plan(
            "herdr:letta",
            "letta",
            &AgentSessionRef::id("default:").unwrap()
        )
        .is_none());
    }

    #[test]
    fn planner_rejects_custom_and_unsupported_path_refs() {
        let claude_session = absolute_test_path("claude-session");
        assert!(plan(
            "custom:claude",
            "claude",
            &AgentSessionRef::id("session").unwrap()
        )
        .is_none());
        assert!(plan(
            "herdr:claude",
            "claude",
            &AgentSessionRef::path(&claude_session).unwrap()
        )
        .is_none());
    }

    #[test]
    fn report_ref_prefers_pi_and_omp_paths_and_validates_values() {
        let pi_session = absolute_test_path("pi-session.jsonl");
        let omp_session = absolute_test_path("omp-session.jsonl");
        let claude_session = absolute_test_path("claude-session");
        let copilot_session = absolute_test_path("copilot-session");
        let session_ref = session_ref_from_report(
            "herdr:pi",
            "pi",
            Some("pi-id".into()),
            Some(pi_session.clone()),
        )
        .unwrap();
        assert_eq!(session_ref.kind, AgentSessionRefKind::Path);
        assert_eq!(session_ref.value, pi_session);

        assert!(session_ref_from_report("herdr:pi", "pi", Some("bad\nid".into()), None).is_none());
        assert!(
            session_ref_from_report("herdr:pi", "pi", None, Some("relative.jsonl".into()))
                .is_none()
        );
        assert!(session_ref_from_report("custom:pi", "pi", Some("pi-id".into()), None).is_none());

        let session_ref = session_ref_from_report(
            "herdr:omp",
            "omp",
            Some("omp-id".into()),
            Some(omp_session.clone()),
        )
        .unwrap();
        assert_eq!(session_ref.kind, AgentSessionRefKind::Path);
        assert_eq!(session_ref.value, omp_session);

        let session_ref =
            session_ref_from_report("herdr:omp", "omp", Some("omp-id".into()), None).unwrap();
        assert_eq!(session_ref.kind, AgentSessionRefKind::Id);
        assert_eq!(session_ref.value, "omp-id");
        let session_ref = session_ref_from_report(
            "herdr:omp",
            "omp",
            Some("omp-id".into()),
            Some("relative.jsonl".into()),
        )
        .unwrap();
        assert_eq!(session_ref.kind, AgentSessionRefKind::Id);
        assert_eq!(session_ref.value, "omp-id");
        assert!(
            session_ref_from_report("herdr:omp", "omp", None, Some("relative.jsonl".into()))
                .is_none()
        );

        assert!(
            session_ref_from_report("herdr:claude", "claude", None, Some(claude_session)).is_none()
        );

        let session_ref =
            session_ref_from_report("herdr:copilot", "copilot", Some("copilot-id".into()), None)
                .unwrap();
        assert_eq!(session_ref.kind, AgentSessionRefKind::Id);
        assert_eq!(session_ref.value, "copilot-id");
        assert!(
            session_ref_from_report("herdr:copilot", "copilot", None, Some(copilot_session))
                .is_none()
        );

        let session_ref =
            session_ref_from_report("herdr:devin", "devin", Some("devin-id".into()), None).unwrap();
        assert_eq!(session_ref.kind, AgentSessionRefKind::Id);
        assert_eq!(session_ref.value, "devin-id");

        let session_ref =
            session_ref_from_report("herdr:droid", "droid", Some("droid-id".into()), None).unwrap();
        assert_eq!(session_ref.kind, AgentSessionRefKind::Id);
        assert_eq!(session_ref.value, "droid-id");
        assert!(session_ref_from_report(
            "herdr:droid",
            "droid",
            None,
            Some("/tmp/droid-session".into())
        )
        .is_none());

        let session_ref =
            session_ref_from_report("herdr:kimi", "kimi", Some("kimi-id".into()), None).unwrap();
        assert_eq!(session_ref.kind, AgentSessionRefKind::Id);
        assert_eq!(session_ref.value, "kimi-id");

        let session_ref = session_ref_from_report(
            "herdr:mastracode",
            "mastracode",
            Some("mastracode-id".into()),
            None,
        )
        .unwrap();
        assert_eq!(session_ref.kind, AgentSessionRefKind::Id);
        assert_eq!(session_ref.value, "mastracode-id");

        let session_ref =
            session_ref_from_report("herdr:kilo", "kilo", Some("kilo-id".into()), None).unwrap();
        assert_eq!(session_ref.kind, AgentSessionRefKind::Id);
        assert_eq!(session_ref.value, "kilo-id");

        let session_ref =
            session_ref_from_report("herdr:qodercli", "qodercli", Some("qoder-id".into()), None)
                .unwrap();
        assert_eq!(session_ref.kind, AgentSessionRefKind::Id);
        assert_eq!(session_ref.value, "qoder-id");

        let session_ref =
            session_ref_from_report("herdr:qwen", "qwen", Some("qwen-id".into()), None).unwrap();
        assert_eq!(session_ref.kind, AgentSessionRefKind::Id);
        assert_eq!(session_ref.value, "qwen-id");

        let session_ref =
            session_ref_from_report("herdr:antigravity_cli", "agy", Some("agy-id".into()), None)
                .unwrap();
        assert_eq!(session_ref.kind, AgentSessionRefKind::Id);
        assert_eq!(session_ref.value, "agy-id");
    }

    #[test]
    fn normalize_session_start_source_allows_known_values() {
        assert_eq!(
            normalize_session_start_source(Some("startup".into())),
            Some("startup".into())
        );
        assert_eq!(
            normalize_session_start_source(Some("resume".into())),
            Some("resume".into())
        );
        assert_eq!(
            normalize_session_start_source(Some("clear".into())),
            Some("clear".into())
        );
        assert_eq!(
            normalize_session_start_source(Some("compact".into())),
            Some("compact".into())
        );
        assert_eq!(
            normalize_session_start_source(Some("branch".into())),
            Some("branch".into())
        );
        assert_eq!(
            normalize_session_start_source(Some("new".into())),
            Some("new".into())
        );
        assert_eq!(
            normalize_session_start_source(Some("fork".into())),
            Some("fork".into())
        );
        assert_eq!(
            normalize_session_start_source(Some("select".into())),
            Some("select".into())
        );
        assert_eq!(
            normalize_session_start_source(Some(" resume ".into())),
            Some("resume".into())
        );
        assert_eq!(normalize_session_start_source(Some("other".into())), None);
        assert_eq!(normalize_session_start_source(None), None);
    }

    #[test]
    fn ids_are_data_not_shell_text() {
        let id = "abc; rm -rf /";
        let codex_plan = plan("herdr:codex", "codex", &AgentSessionRef::id(id).unwrap()).unwrap();
        assert_eq!(codex_plan.argv, vec!["codex", "resume", id]);

        let copilot_plan = plan(
            "herdr:copilot",
            "copilot",
            &AgentSessionRef::id(id).unwrap(),
        )
        .unwrap();
        assert_eq!(copilot_plan.argv, vec!["copilot", "--resume=abc; rm -rf /"]);

        let devin_plan = plan("herdr:devin", "devin", &AgentSessionRef::id(id).unwrap()).unwrap();
        assert_eq!(devin_plan.argv, vec!["devin", "--resume", id]);
    }

    #[test]
    fn planner_rejects_path_refs_for_id_only_agents() {
        let hermes_session = absolute_test_path("hermes-session");
        let opencode_session = absolute_test_path("opencode-session");
        let kilo_session = absolute_test_path("kilo-session");
        let copilot_session = absolute_test_path("copilot-session");
        let devin_session = absolute_test_path("devin-session");
        assert!(plan(
            "herdr:hermes",
            "hermes",
            &AgentSessionRef::path(&hermes_session).unwrap()
        )
        .is_none());
        assert!(plan(
            "herdr:opencode",
            "opencode",
            &AgentSessionRef::path(&opencode_session).unwrap()
        )
        .is_none());
        assert!(plan(
            "herdr:kilo",
            "kilo",
            &AgentSessionRef::path(&kilo_session).unwrap()
        )
        .is_none());
        assert!(plan(
            "herdr:copilot",
            "copilot",
            &AgentSessionRef::path(&copilot_session).unwrap()
        )
        .is_none());
        assert!(plan(
            "herdr:devin",
            "devin",
            &AgentSessionRef::path(&devin_session).unwrap()
        )
        .is_none());
        assert!(session_ref_from_snapshot(
            "herdr:mastracode",
            "mastracode",
            AgentSessionRefKind::Id,
            "mastracode-session"
        )
        .is_some());
        assert!(session_ref_from_snapshot(
            "herdr:hermes",
            "hermes",
            AgentSessionRefKind::Id,
            "hermes-session"
        )
        .is_some());
        assert!(session_ref_from_snapshot(
            "herdr:opencode",
            "opencode",
            AgentSessionRefKind::Id,
            "opencode-session"
        )
        .is_some());
        assert!(session_ref_from_snapshot(
            "herdr:kilo",
            "kilo",
            AgentSessionRefKind::Id,
            "kilo-session"
        )
        .is_some());
        assert!(session_ref_from_snapshot(
            "herdr:copilot",
            "copilot",
            AgentSessionRefKind::Id,
            "copilot-session"
        )
        .is_some());
        assert!(session_ref_from_snapshot(
            "herdr:devin",
            "devin",
            AgentSessionRefKind::Id,
            "devin-session"
        )
        .is_some());
        assert!(session_ref_from_snapshot(
            "herdr:antigravity_cli",
            "agy",
            AgentSessionRefKind::Id,
            "agy-session"
        )
        .is_some());
        let agy_session = absolute_test_path("agy-session");
        assert!(plan(
            "herdr:antigravity_cli",
            "agy",
            &AgentSessionRef::path(&agy_session).unwrap()
        )
        .is_none());
    }
}
