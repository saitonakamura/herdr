//! Notifications a click can answer and herdr can take back, through `terminal-notifier`.
//!
//! macOS hands a click to whichever `terminal-notifier` instance is running, not the one that
//! posted, so a helper waiting on its own notification can be given another's click. A posted
//! `-execute` command travels with the notification instead: it sends the notification's group
//! to a socket this client listens on. Notifications for one key share a group, so a new one
//! replaces the last, and removal clears it from Notification Center.

use std::collections::HashMap;
use std::io::{self, BufRead};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{mpsc, LazyLock, Mutex, OnceLock};

use sha2::{Digest, Sha256};

use crate::platform::NotificationCallback;

const HELPER: &str = "terminal-notifier";
const SOCKET_PREFIX: &str = "herdr-notify-";
const SOCKET_SUFFIX: &str = ".sock";

type Job = Box<dyn FnOnce() + Send>;

/// What a click on each shown notification does, by group.
static CALLBACKS: LazyLock<Mutex<HashMap<String, NotificationCallback>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) fn show_actionable_desktop_notification(
    title: &str,
    body: Option<&str>,
    key: String,
    callback: NotificationCallback,
) -> io::Result<bool> {
    let title = title.to_owned();
    let body = body.map(str::to_owned);
    queue(Box::new(move || {
        let group = notification_group(&key);
        let bundle_id =
            super::verified_terminal_bundle_identifier(&mut |program: &str| Command::new(program));
        let Some(socket) = click_socket() else {
            fall_back(&title, body.as_deref());
            return;
        };
        lock_callbacks().insert(group.clone(), callback);
        let mut command = Command::new(HELPER);
        build_actionable_command(
            &mut command,
            &title,
            body.as_deref(),
            &group,
            bundle_id.as_deref(),
            socket,
        );
        if !super::run_notification_command(command).unwrap_or(false) {
            lock_callbacks().remove(&group);
            fall_back(&title, body.as_deref());
        }
    }))
}

pub(crate) fn remove_desktop_notification(key: &str) -> io::Result<()> {
    let group = notification_group(key);
    queue(Box::new(move || {
        // Never posted through the helper, or already clicked away.
        if lock_callbacks().remove(&group).is_none() {
            return;
        }
        let mut command = Command::new(HELPER);
        build_remove_command(&mut command, &group);
        if let Err(err) = super::run_notification_command(command) {
            tracing::warn!(err = %err, "failed to dismiss system notification");
        }
    }))
    .map(|_| ())
}

/// The helper raises the terminal itself on click, before herdr hears of it.
pub(crate) fn foreground_desktop_notification_host() {}

fn fall_back(title: &str, body: Option<&str>) {
    if let Err(err) = super::show_desktop_notification(title, body) {
        tracing::warn!(err = %err, "failed to emit system notification");
    }
}

/// Runs notification work off the client loop, in order, so a removal never overtakes the
/// notification it removes.
fn queue(job: Job) -> io::Result<bool> {
    static QUEUE: LazyLock<Mutex<Option<mpsc::Sender<Job>>>> = LazyLock::new(|| Mutex::new(None));
    let mut queue = QUEUE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if queue.is_none() {
        let (sender, receiver) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("herdr-macos-notification".into())
            .spawn(move || {
                while let Ok(job) = receiver.recv() {
                    job();
                }
            })?;
        *queue = Some(sender);
    }
    match queue.as_ref().map(|sender| sender.send(job)) {
        Some(Ok(())) => Ok(true),
        _ => {
            *queue = None;
            Ok(false)
        }
    }
}

fn lock_callbacks() -> std::sync::MutexGuard<'static, HashMap<String, NotificationCallback>> {
    CALLBACKS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The socket a clicked notification reports its group to, bound once per client.
fn click_socket() -> Option<&'static Path> {
    static SOCKET: OnceLock<Option<PathBuf>> = OnceLock::new();
    SOCKET
        .get_or_init(|| {
            let dir = std::env::temp_dir();
            remove_stale_sockets(&dir);
            let path = dir.join(socket_name(std::process::id()));
            let _ = std::fs::remove_file(&path);
            let listener = match UnixListener::bind(&path) {
                Ok(listener) => listener,
                Err(err) => {
                    tracing::warn!(err = %err, "failed to listen for notification clicks");
                    return None;
                }
            };
            let spawned = std::thread::Builder::new()
                .name("herdr-macos-notification-click".into())
                .spawn(move || listen_for_clicks(listener));
            if let Err(err) = spawned {
                tracing::warn!(err = %err, "failed to listen for notification clicks");
                let _ = std::fs::remove_file(&path);
                return None;
            }
            Some(path)
        })
        .as_deref()
}

fn listen_for_clicks(listener: UnixListener) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            continue;
        };
        let mut group = String::new();
        if io::BufReader::new(stream).read_line(&mut group).is_err() {
            continue;
        }
        let callback = lock_callbacks().remove(group.trim());
        if let Some(callback) = callback {
            callback();
        }
    }
}

fn socket_name(pid: u32) -> String {
    format!("{SOCKET_PREFIX}{pid}{SOCKET_SUFFIX}")
}

/// Sockets left by clients that exited; the per-user temp dir is never shared.
fn remove_stale_sockets(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name
            .to_str()
            .and_then(|name| name.strip_prefix(SOCKET_PREFIX))
            .and_then(|name| name.strip_suffix(SOCKET_SUFFIX))
            .and_then(|pid| pid.parse::<libc::pid_t>().ok())
        else {
            continue;
        };
        // SAFETY: signal 0 only checks that the process exists.
        let alive = unsafe { libc::kill(pid, 0) } == 0
            || io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH);
        if !alive {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// A stable group id that starts with a letter: the helper parses values as property lists.
fn notification_group(key: &str) -> String {
    let digest = Sha256::digest(key.as_bytes());
    let hex = digest[..12]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("herdr-{hex}")
}

/// The helper reads values through `NSUserDefaults`, which misreads a leading `[`, `(`, `{`,
/// `<` or `"` as a property list and drops the notification. A backslash escapes it.
fn helper_value(value: &str) -> String {
    match value.chars().next() {
        Some('[' | '(' | '{' | '<' | '"' | '\\') => format!("\\{value}"),
        _ => value.to_owned(),
    }
}

/// Run by the helper on click, from whichever instance macOS hands the click to.
fn click_command(group: &str, socket: &Path) -> String {
    format!(
        "printf '%s\\n' {group} | /usr/bin/nc -U -w 1 {}",
        super::shell_quote(&socket.to_string_lossy())
    )
}

fn build_actionable_command(
    command: &mut Command,
    title: &str,
    body: Option<&str>,
    group: &str,
    activate_bundle_id: Option<&str>,
    socket: &Path,
) {
    command.arg("-title").arg(helper_value(title));
    command.arg("-message").arg(helper_value(
        body.filter(|body| !body.is_empty()).unwrap_or(" "),
    ));
    command.arg("-group").arg(group);
    if let Some(bundle_id) = activate_bundle_id {
        command.arg("-activate").arg(bundle_id);
    }
    command.arg("-execute").arg(click_command(group, socket));
}

fn build_remove_command(command: &mut Command, group: &str) {
    command.arg("-remove").arg(group);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn actionable_command_groups_raises_the_terminal_and_reports_the_click() {
        let mut command = Command::new(HELPER);
        build_actionable_command(
            &mut command,
            "claude needs attention",
            Some("ws · tab"),
            "herdr-0011",
            Some("net.kovidgoyal.kitty"),
            Path::new("/tmp/herdr-notify-42.sock"),
        );

        assert_eq!(
            args(&command),
            [
                "-title",
                "claude needs attention",
                "-message",
                "ws · tab",
                "-group",
                "herdr-0011",
                "-activate",
                "net.kovidgoyal.kitty",
                "-execute",
                "printf '%s\\n' herdr-0011 | /usr/bin/nc -U -w 1 /tmp/herdr-notify-42.sock",
            ]
        );
    }

    #[test]
    fn actionable_command_escapes_values_the_helper_would_parse() {
        let mut command = Command::new(HELPER);
        build_actionable_command(
            &mut command,
            "[ws] done",
            None,
            "herdr-0011",
            None,
            Path::new("/tmp/with space/herdr-notify-42.sock"),
        );

        let args = args(&command);
        assert_eq!(&args[..4], ["-title", "\\[ws] done", "-message", " "]);
        assert!(!args.contains(&"-activate".to_owned()));
        assert!(args
            .last()
            .is_some_and(|execute| execute.ends_with("'/tmp/with space/herdr-notify-42.sock'")));
    }

    #[test]
    fn remove_command_names_the_group() {
        let mut command = Command::new(HELPER);
        build_remove_command(&mut command, "herdr-0011");

        assert_eq!(args(&command), ["-remove", "herdr-0011"]);
    }

    #[test]
    fn groups_are_stable_per_key_and_safe_to_pass() {
        let group = notification_group(r#"["local","boot","pane_1"]"#);

        assert_eq!(group, notification_group(r#"["local","boot","pane_1"]"#));
        assert_ne!(group, notification_group(r#"["local","boot","pane_2"]"#));
        assert!(group.starts_with("herdr-"));
        assert!(group[6..].chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn a_click_reported_on_the_socket_runs_its_callback_once() {
        let dir = std::env::temp_dir().join(format!("hn-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("test dir");
        let path = dir.join("c.sock");
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).expect("bind");
        std::thread::spawn(move || listen_for_clicks(listener));
        let (clicked, received) = mpsc::channel();
        let clicked = Mutex::new(clicked);
        lock_callbacks().insert(
            "herdr-test-click".into(),
            std::sync::Arc::new(move || {
                let _ = clicked.lock().map(|clicked| clicked.send(()));
            }),
        );

        let status = Command::new("/bin/sh")
            .arg("-c")
            .arg(click_command("herdr-test-click", &path))
            .status()
            .expect("click command runs");

        assert!(status.success());
        received
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("callback ran");
        assert!(!lock_callbacks().contains_key("herdr-test-click"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stale_sockets_of_exited_clients_are_removed() {
        let dir = std::env::temp_dir().join(format!("hn-stale-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("test dir");
        let live = dir.join(socket_name(std::process::id()));
        // Above any pid macOS hands out.
        let dead = dir.join(socket_name(99_999_999));
        let other = dir.join("unrelated.sock");
        for path in [&live, &dead, &other] {
            std::fs::write(path, b"").expect("fixture");
        }

        remove_stale_sockets(&dir);

        assert!(live.exists());
        assert!(!dead.exists());
        assert!(other.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
