//! The session pane: one terminal per server that belongs to no workspace or tab.
//! Clients that negotiate it draw it in place of their native sidebar.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use tracing::{info, warn};

use crate::app::App;
use crate::layout::PaneId;
use crate::pane::PaneLaunchEnv;
use crate::terminal::{TerminalId, TerminalRuntime, TerminalState};

/// A run at least this long counts as healthy and resets the backoff.
const HEALTHY_RUN: Duration = Duration::from_secs(30);
const FIRST_BACKOFF: Duration = Duration::from_millis(500);
const MAX_BACKOFF: Duration = Duration::from_secs(30);
/// Consecutive quick failures after which the program is left down.
const CRASH_LOOP_FAILURES: u32 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionPaneConfig {
    pub argv: Option<Vec<String>>,
    /// Expanded width, already clamped by the sidebar bounds.
    pub width: u16,
    pub collapsed_width: u16,
}

impl SessionPaneConfig {
    pub(crate) fn from_config(config: &crate::config::Config) -> Self {
        let (min, max) = crate::config::validated_sidebar_bounds(
            config.ui.sidebar_min_width,
            config.ui.sidebar_max_width,
        )
        .unwrap_or((18, 36));
        Self {
            argv: config.ui.sidebar.pane_argv().map(<[String]>::to_vec),
            width: config.ui.sidebar_width.clamp(min, max),
            collapsed_width: config.ui.sidebar.pane_collapsed_width(),
        }
    }
}

impl Default for SessionPaneConfig {
    fn default() -> Self {
        Self::from_config(&crate::config::Config::default())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionPaneStatus {
    /// No program is configured.
    Off,
    Running,
    /// The program exited or failed to start; it starts again at `restart_at`.
    Restarting,
    /// The program kept failing; it stays down until the config changes or reloads.
    Failed,
}

impl SessionPaneStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Running => "running",
            Self::Restarting => "restarting",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionPaneProcess {
    pub pane_id: PaneId,
    pub terminal_id: TerminalId,
    pub started_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionPaneState {
    pub config: SessionPaneConfig,
    pub process: Option<SessionPaneProcess>,
    pub status: SessionPaneStatus,
    /// Shared keyboard focus: while set, every client sends its keys to the program.
    pub focused: bool,
    /// Size the program was last given, as (cols, rows).
    pub size: (u16, u16),
    pub quick_failures: u32,
    pub restarts: u64,
    pub restart_at: Option<Instant>,
    /// Set once when a crash loop gives up, for the server to tell clients.
    pub crash_loop_notice: Option<String>,
}

impl Default for SessionPaneState {
    fn default() -> Self {
        let config = SessionPaneConfig::default();
        Self {
            size: (config.width, crate::config::DEFAULT_HEADLESS_ROWS),
            config,
            process: None,
            status: SessionPaneStatus::Off,
            focused: false,
            quick_failures: 0,
            restarts: 0,
            restart_at: None,
            crash_loop_notice: None,
        }
    }
}

/// What to do after the program exited or failed to start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionPaneExitPlan {
    RestartAt(Instant),
    GiveUp,
}

impl SessionPaneState {
    pub(crate) fn is_pane(&self, pane_id: PaneId) -> bool {
        self.process
            .as_ref()
            .is_some_and(|process| process.pane_id == pane_id)
    }

    pub(crate) fn terminal_id(&self) -> Option<&TerminalId> {
        self.process.as_ref().map(|process| &process.terminal_id)
    }

    pub(crate) fn pane_id(&self) -> Option<PaneId> {
        self.process.as_ref().map(|process| process.pane_id)
    }

    /// Records a failure and plans the next start. `ran_for` is `None` when the
    /// program never started.
    pub(crate) fn plan_after_failure(
        &mut self,
        now: Instant,
        ran_for: Option<Duration>,
    ) -> SessionPaneExitPlan {
        if ran_for.is_some_and(|ran_for| ran_for >= HEALTHY_RUN) {
            self.quick_failures = 0;
        }
        self.quick_failures = self.quick_failures.saturating_add(1);
        if self.quick_failures >= CRASH_LOOP_FAILURES {
            self.status = SessionPaneStatus::Failed;
            self.restart_at = None;
            return SessionPaneExitPlan::GiveUp;
        }
        let exponent = self.quick_failures.saturating_sub(1).min(16);
        let delay = FIRST_BACKOFF
            .saturating_mul(1u32 << exponent)
            .min(MAX_BACKOFF);
        let at = now + delay;
        self.status = SessionPaneStatus::Restarting;
        self.restart_at = Some(at);
        SessionPaneExitPlan::RestartAt(at)
    }
}

impl App {
    /// Starts the configured program, if any. Called once the server is bound.
    pub(crate) fn start_session_pane_if_configured(&mut self) {
        if self.state.session_pane.config.argv.is_some()
            && self.state.session_pane.process.is_none()
        {
            self.state.session_pane.quick_failures = 0;
            self.start_session_pane(Instant::now());
        }
    }

    fn start_session_pane(&mut self, now: Instant) {
        let Some(argv) = self.state.session_pane.config.argv.clone() else {
            self.state.session_pane.status = SessionPaneStatus::Off;
            return;
        };
        self.state.session_pane.restart_at = None;
        let (cols, rows) = self.state.session_pane.size;
        let pane_id = PaneId::alloc();
        let terminal_id = TerminalId::alloc();
        let cwd = session_pane_cwd();
        let session = crate::session::active_name()
            .unwrap_or_else(|| crate::session::DEFAULT_SESSION_NAME.to_string());
        // The config path lets the program read its own table from the same files.
        let config_path = crate::config::config_path().to_string_lossy().into_owned();
        let launch_env = PaneLaunchEnv::from_extra(vec![
            (crate::session::SESSION_ENV_VAR.to_string(), session),
            (crate::config::CONFIG_PATH_ENV_VAR.to_string(), config_path),
        ])
        .without_pane_identity();
        match TerminalRuntime::spawn_argv_command(
            pane_id,
            rows.max(1),
            cols.max(1),
            cwd.clone(),
            &argv,
            &launch_env,
            crate::pane::AgentDetection::Disabled,
            self.state.pane_scrollback_limit_bytes,
            self.state.host_terminal_theme,
            self.state.host_terminal_appearance,
            self.event_tx.clone(),
            self.render_notify.clone(),
            self.render_dirty.clone(),
        ) {
            Ok(runtime) => {
                info!(
                    event = "session_pane.started",
                    program = %argv[0],
                    restarts = self.state.session_pane.restarts,
                    "session pane started"
                );
                self.terminal_runtimes.insert(terminal_id.clone(), runtime);
                self.state.terminals.insert(
                    terminal_id.clone(),
                    TerminalState::new(terminal_id.clone(), cwd).with_launch_argv(argv),
                );
                self.state.session_pane.process = Some(SessionPaneProcess {
                    pane_id,
                    terminal_id,
                    started_at: now,
                });
                self.state.session_pane.status = SessionPaneStatus::Running;
                if self.state.session_pane.focused {
                    self.send_session_pane_focus_event(crate::ghostty::FocusEvent::Gained);
                }
            }
            Err(err) => {
                warn!(
                    event = "session_pane.start_failed",
                    program = %argv[0],
                    err = %err,
                    "session pane failed to start"
                );
                self.plan_session_pane_restart(now, None);
            }
        }
        self.render_dirty.request_generic();
        self.render_notify.notify_one();
    }

    fn stop_session_pane_process(&mut self) {
        if let Some(process) = self.state.session_pane.process.take() {
            self.state.terminals.remove(&process.terminal_id);
            self.state
                .direct_attach_resize_locks
                .remove(&process.terminal_id);
            self.shutdown_terminal_runtime(process.terminal_id);
        }
    }

    /// Handles the program's exit. Returns false when `pane_id` is not the session pane.
    pub(crate) fn handle_session_pane_died(&mut self, pane_id: PaneId, exit_reason: &str) -> bool {
        if !self.state.session_pane.is_pane(pane_id) {
            return false;
        }
        let now = Instant::now();
        let ran_for = self
            .state
            .session_pane
            .process
            .as_ref()
            .map(|process| now.saturating_duration_since(process.started_at));
        warn!(
            event = "session_pane.exited",
            exit_reason,
            ran_for_ms = ran_for.map(|ran_for| ran_for.as_millis() as u64),
            "session pane exited"
        );
        self.stop_session_pane_process();
        // A restarted program starts unfocused; keys go back to the panes meanwhile.
        self.state.session_pane.focused = false;
        self.plan_session_pane_restart(now, ran_for);
        self.render_dirty.request_generic();
        self.render_notify.notify_one();
        true
    }

    fn plan_session_pane_restart(&mut self, now: Instant, ran_for: Option<Duration>) {
        match self.state.session_pane.plan_after_failure(now, ran_for) {
            SessionPaneExitPlan::RestartAt(at) => {
                info!(
                    event = "session_pane.restart_scheduled",
                    delay_ms = at.saturating_duration_since(now).as_millis() as u64,
                    "session pane restart scheduled"
                );
            }
            SessionPaneExitPlan::GiveUp => {
                let message = format!(
                    "sidebar pane program failed {CRASH_LOOP_FAILURES} times in a row; \
                     using the native sidebar until the config is reloaded"
                );
                warn!(event = "session_pane.crash_loop", "{message}");
                self.state.session_pane.focused = false;
                self.state.session_pane.crash_loop_notice = Some(message);
            }
        }
    }

    pub(crate) fn session_pane_restart_deadline(&self) -> Option<Instant> {
        self.state.session_pane.restart_at
    }

    /// Restarts the program once its backoff has elapsed.
    pub(crate) fn run_session_pane_restart_if_due(&mut self, now: Instant) -> bool {
        if self.state.session_pane.restart_at.is_none_or(|at| now < at) {
            return false;
        }
        self.state.session_pane.restarts = self.state.session_pane.restarts.saturating_add(1);
        self.start_session_pane(now);
        true
    }

    /// Applies a reloaded config. A changed argv restarts the program, a removed one
    /// stops it, and a reload of any kind gives a failed program another try.
    pub(crate) fn apply_session_pane_config(&mut self, config: SessionPaneConfig) -> bool {
        let previous = std::mem::replace(&mut self.state.session_pane.config, config);
        let current = &self.state.session_pane.config;
        let argv_changed = previous.argv != current.argv;
        let width_changed =
            previous.width != current.width || previous.collapsed_width != current.collapsed_width;
        let retry_failed = self.state.session_pane.status == SessionPaneStatus::Failed;
        if !argv_changed && !retry_failed {
            return width_changed;
        }
        self.stop_session_pane_process();
        self.state.session_pane.restart_at = None;
        self.state.session_pane.quick_failures = 0;
        if self.state.session_pane.config.argv.is_some() {
            info!(
                event = "session_pane.config_changed",
                "session pane restarting for new config"
            );
            self.start_session_pane(Instant::now());
        } else {
            self.state.session_pane.status = SessionPaneStatus::Off;
            self.state.session_pane.focused = false;
            self.render_dirty.request_generic();
            self.render_notify.notify_one();
        }
        true
    }

    /// Gives the program shared keyboard focus, or takes it back. Returns whether
    /// focus changed.
    pub(crate) fn set_session_pane_focus(&mut self, focused: bool) -> bool {
        if self.state.session_pane.focused == focused {
            return false;
        }
        if focused && self.state.session_pane.status != SessionPaneStatus::Running {
            return false;
        }
        self.state.session_pane.focused = focused;
        self.send_session_pane_focus_event(if focused {
            crate::ghostty::FocusEvent::Gained
        } else {
            crate::ghostty::FocusEvent::Lost
        });
        self.render_dirty.request_generic();
        self.render_notify.notify_one();
        true
    }

    fn send_session_pane_focus_event(&self, event: crate::ghostty::FocusEvent) {
        if let Some(runtime) = self
            .state
            .session_pane
            .terminal_id()
            .and_then(|terminal_id| self.terminal_runtimes.get(terminal_id))
        {
            runtime.try_send_focus_event(event);
        }
    }

    pub(crate) fn session_pane_runtime(&self) -> Option<&TerminalRuntime> {
        self.state
            .session_pane
            .terminal_id()
            .and_then(|terminal_id| self.terminal_runtimes.get(terminal_id))
    }

    /// Resizes the program to a client's slot. Zero-sized slots (a hidden sidebar)
    /// keep the previous size.
    pub(crate) fn resize_session_pane(
        &mut self,
        cols: u16,
        rows: u16,
        cell_size: crate::kitty_graphics::HostCellSize,
    ) -> bool {
        if cols == 0 || rows == 0 || self.state.session_pane.size == (cols, rows) {
            return false;
        }
        self.state.session_pane.size = (cols, rows);
        if let Some(runtime) = self.session_pane_runtime() {
            runtime.resize(rows, cols, cell_size.width_px, cell_size.height_px);
        }
        true
    }

    pub(crate) fn take_session_pane_notice(&mut self) -> Option<String> {
        self.state.session_pane.crash_loop_notice.take()
    }
}

impl App {
    pub(crate) fn session_pane_info(&self) -> crate::api::schema::SessionPaneInfo {
        let pane = &self.state.session_pane;
        crate::api::schema::SessionPaneInfo {
            status: pane.status.as_str().to_string(),
            argv: pane.config.argv.clone(),
            focused: pane.focused,
            cols: pane.size.0,
            rows: pane.size.1,
            width: pane.config.width,
            collapsed_width: pane.config.collapsed_width,
            restarts: pane.restarts,
        }
    }
}

/// Methods that move keyboard focus to a pane also take it from the session pane,
/// so a program can focus a pane and leave in one call.
pub(crate) fn method_moves_focus_to_a_pane(method: &crate::api::schema::Method) -> bool {
    use crate::api::schema::Method;
    matches!(
        method,
        Method::WorkspaceFocus(_)
            | Method::TabFocus(_)
            | Method::PaneFocus(_)
            | Method::PaneFocusDirection(_)
            | Method::AgentFocus(_)
            | Method::PluginPaneFocus(_)
            | Method::PopupOpen(_)
    )
}

#[cfg(test)]
impl App {
    pub(crate) fn install_test_session_pane(&mut self, runtime: TerminalRuntime) -> TerminalId {
        let pane_id = PaneId::alloc();
        let terminal_id = TerminalId::alloc();
        let (rows, cols) = runtime.current_size();
        self.state.session_pane.size = (cols, rows);
        self.terminal_runtimes.insert(terminal_id.clone(), runtime);
        self.state.terminals.insert(
            terminal_id.clone(),
            TerminalState::new(terminal_id.clone(), PathBuf::from("/session-pane")),
        );
        self.state.session_pane.process = Some(SessionPaneProcess {
            pane_id,
            terminal_id: terminal_id.clone(),
            started_at: Instant::now(),
        });
        self.state.session_pane.status = SessionPaneStatus::Running;
        terminal_id
    }
}

fn session_pane_cwd() -> PathBuf {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> App {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        App::new(
            &crate::config::Config::default(),
            crate::app::AppPolicy::TEST,
            None,
            api_rx,
            crate::api::EventHub::default(),
        )
    }

    fn configured_state() -> SessionPaneState {
        SessionPaneState {
            config: SessionPaneConfig {
                argv: Some(vec!["prog".into()]),
                width: 26,
                collapsed_width: 8,
            },
            ..SessionPaneState::default()
        }
    }

    #[test]
    fn backoff_doubles_then_gives_up_after_quick_failures() {
        let mut state = configured_state();
        let now = Instant::now();
        let quick = Some(Duration::from_millis(10));
        let mut delays = Vec::new();
        for _ in 0..(CRASH_LOOP_FAILURES - 1) {
            match state.plan_after_failure(now, quick) {
                SessionPaneExitPlan::RestartAt(at) => delays.push(at - now),
                SessionPaneExitPlan::GiveUp => panic!("gave up too early"),
            }
            assert_eq!(state.status, SessionPaneStatus::Restarting);
        }
        assert_eq!(
            delays,
            vec![
                Duration::from_millis(500),
                Duration::from_secs(1),
                Duration::from_secs(2),
                Duration::from_secs(4),
            ]
        );
        assert_eq!(
            state.plan_after_failure(now, quick),
            SessionPaneExitPlan::GiveUp
        );
        assert_eq!(state.status, SessionPaneStatus::Failed);
        assert_eq!(state.restart_at, None);
    }

    #[test]
    fn a_healthy_run_resets_the_backoff() {
        let mut state = configured_state();
        let now = Instant::now();
        for _ in 0..3 {
            state.plan_after_failure(now, None);
        }
        assert_eq!(
            state.plan_after_failure(now, Some(HEALTHY_RUN)),
            SessionPaneExitPlan::RestartAt(now + FIRST_BACKOFF)
        );
        assert_eq!(state.quick_failures, 1);
    }

    #[test]
    fn config_clamps_width_and_reads_the_collapsed_width() {
        let config: crate::config::Config = toml::from_str(
            "[ui]\nsidebar_width = 90\nsidebar_max_width = 40\n[ui.sidebar]\npane = [\"herdr-sidebar\", \"--x\"]\npane_collapsed_width = 6\n",
        )
        .expect("config");
        let pane = SessionPaneConfig::from_config(&config);
        assert_eq!(
            pane,
            SessionPaneConfig {
                argv: Some(vec!["herdr-sidebar".into(), "--x".into()]),
                width: 40,
                collapsed_width: 6,
            }
        );
        assert!(config.collect_diagnostics().is_empty());
    }

    #[test]
    fn unusable_pane_config_is_flagged_and_left_native() {
        let config: crate::config::Config =
            toml::from_str("[ui.sidebar]\npane = []\npane_collapsed_width = 0\n").expect("config");
        let pane = SessionPaneConfig::from_config(&config);
        assert_eq!(pane.argv, None);
        assert_eq!(pane.collapsed_width, 8);
        let diagnostics = config.collect_diagnostics();
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.starts_with("ui.sidebar.pane must name")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.starts_with("ui.sidebar.pane_collapsed_width")));
    }

    #[test]
    fn unset_pane_keeps_the_native_sidebar() {
        let pane = SessionPaneConfig::default();
        assert_eq!(pane.argv, None);
        assert_eq!(pane.width, 26);
        assert_eq!(pane.collapsed_width, 8);
    }

    #[test]
    fn focus_needs_a_running_program() {
        let mut app = test_app();
        assert!(!app.set_session_pane_focus(true));
        app.state.session_pane.status = SessionPaneStatus::Running;
        assert!(app.set_session_pane_focus(true));
        assert!(!app.set_session_pane_focus(true));
        assert!(app.set_session_pane_focus(false));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn an_exiting_program_is_restarted_then_given_up_on() {
        let mut app = test_app();
        app.apply_session_pane_config(SessionPaneConfig {
            argv: Some(vec!["/bin/sh".into(), "-c".into(), "exit 3".into()]),
            width: 26,
            collapsed_width: 8,
        });
        assert_eq!(app.state.session_pane.status, SessionPaneStatus::Running);
        let pane_id = app.state.session_pane.pane_id().expect("running pane");
        let terminal_id = app
            .state
            .session_pane
            .terminal_id()
            .cloned()
            .expect("terminal");
        assert!(app.state.terminals.contains_key(&terminal_id));
        app.state.assert_invariants_for_test();

        assert!(app.handle_session_pane_died(pane_id, "exit 3"));
        assert_eq!(app.state.session_pane.status, SessionPaneStatus::Restarting);
        assert!(app.state.session_pane.process.is_none());
        assert!(!app.state.terminals.contains_key(&terminal_id));
        assert!(!app.handle_session_pane_died(pane_id, "stale"));

        let mut now = Instant::now();
        for _ in 1..CRASH_LOOP_FAILURES {
            now = app
                .session_pane_restart_deadline()
                .expect("restart deadline");
            assert!(app.run_session_pane_restart_if_due(now));
            let pane_id = app.state.session_pane.pane_id().expect("restarted");
            app.handle_session_pane_died(pane_id, "exit 3");
        }
        assert_eq!(app.state.session_pane.status, SessionPaneStatus::Failed);
        assert!(app.take_session_pane_notice().is_some());
        assert!(app.take_session_pane_notice().is_none());
        assert!(!app.run_session_pane_restart_if_due(now + MAX_BACKOFF));

        // A reload gives a failed program another try.
        let config = app.state.session_pane.config.clone();
        assert!(app.apply_session_pane_config(config));
        assert_eq!(app.state.session_pane.status, SessionPaneStatus::Running);
        let config = SessionPaneConfig {
            argv: None,
            ..app.state.session_pane.config.clone()
        };
        assert!(app.apply_session_pane_config(config));
        assert_eq!(app.state.session_pane.status, SessionPaneStatus::Off);
        assert!(app.state.session_pane.process.is_none());
    }
}
