//! The program's IO: the terminal, herdr's socket, the control socket and
//! signals, feeding one loop that coalesces redraws.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::event::{
    DisableFocusChange, DisableMouseCapture, EnableFocusChange, EnableMouseCapture, Event,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use serde_json::json;

use crate::api::{self, ApiError, Client, MessageKind};
use crate::app::{App, Effect};
use crate::model::AgeClock;
use crate::{config, state};

/// At most about 20 refreshes and redraws a second (018).
const FRAME: Duration = Duration::from_millis(50);

enum Msg {
    Term(Event),
    /// Something changed; `pane.exited` carries its pane.
    Invalidate {
        exited: Option<String>,
        closed: Option<String>,
    },
    SubscriptionDown(ApiError),
    Ctl {
        verb: String,
        pane: Option<String>,
        reply: Sender<Result<String, String>>,
    },
    Reload,
    Quit,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

pub struct Paths {
    pub herdr_socket: PathBuf,
    pub config: PathBuf,
    pub state: PathBuf,
    pub ctl: PathBuf,
}

impl Paths {
    pub fn from_env() -> Self {
        let herdr_socket = state::herdr_socket();
        Paths {
            ctl: state::ctl_socket(&herdr_socket),
            herdr_socket,
            config: config::config_path(),
            state: state::state_path(&state::state_dir(), &state::session_name()),
        }
    }
}

pub fn run(paths: Paths) -> std::io::Result<()> {
    let client = Client {
        socket: paths.herdr_socket.clone(),
    };
    let mut app = App::new(config::load(&paths.config), state::load(&paths.state));
    let (tx, rx) = mpsc::channel();

    spawn_input(tx.clone());
    spawn_subscription(client.clone(), tx.clone());
    spawn_signals(tx.clone())?;
    let _ctl = CtlSocket::bind(&paths.ctl, tx.clone());

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableFocusChange
    )?;
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        previous_hook(info);
    }));
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    terminal.hide_cursor()?;
    let status = StatusWatch::spawn(client.clone(), tx.clone());
    let result = event_loop(&mut app, &client, &paths, &rx, status, &mut terminal);
    restore_terminal();
    result
}

fn restore_terminal() {
    let _ = execute!(
        std::io::stdout(),
        DisableFocusChange,
        DisableMouseCapture,
        LeaveAlternateScreen,
        crossterm::cursor::Show
    );
    let _ = disable_raw_mode();
}

fn event_loop(
    app: &mut App,
    client: &Client,
    paths: &Paths,
    rx: &Receiver<Msg>,
    mut status: StatusWatch,
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
) -> std::io::Result<()> {
    let mut clock = AgeClock::default();
    let mut stale = true;
    let mut dirty = true;
    let mut last_refresh = Instant::now() - FRAME;
    let mut last_draw = Instant::now() - FRAME;
    loop {
        let due = [stale.then_some(last_refresh), dirty.then_some(last_draw)]
            .into_iter()
            .flatten()
            .map(|at| (at + FRAME).saturating_duration_since(Instant::now()))
            .min();
        let msg = match due {
            Some(wait) => rx.recv_timeout(wait),
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match msg {
            Ok(Msg::Quit) | Err(RecvTimeoutError::Disconnected) => return Ok(()),
            Ok(Msg::Term(event)) => {
                let effects = match event {
                    Event::Key(key) => app.key(key),
                    Event::Mouse(mouse) => app.mouse(mouse),
                    Event::FocusGained => {
                        app.focused = true;
                        app.ensure_selection();
                        Vec::new()
                    }
                    Event::FocusLost => {
                        app.focused = false;
                        app.mode = crate::app::Mode::Normal;
                        Vec::new()
                    }
                    _ => Vec::new(),
                };
                stale |= execute_all(app, client, paths, effects);
                dirty = true;
            }
            Ok(Msg::Invalidate { exited, closed }) => {
                if let Some(pane) = exited {
                    app.extras.exited.insert(pane);
                }
                if let Some(pane) = closed {
                    app.extras.exited.remove(&pane);
                }
                stale = true;
            }
            Ok(Msg::SubscriptionDown(err)) => {
                if err.code != "events_lost" {
                    app.set_status(format!("herdr: {err}; reconnecting"));
                }
                stale = true;
                dirty = true;
            }
            Ok(Msg::Ctl { verb, pane, reply }) => {
                let result = app.ctl(&verb, pane.as_deref()).map(|effects| {
                    stale |= execute_all(app, client, paths, effects);
                    "ok".to_string()
                });
                let _ = reply.send(result);
                dirty = true;
            }
            Ok(Msg::Reload) => {
                app.config = config::load(&paths.config);
                if !app.config.diagnostics.is_empty() {
                    app.set_status(format!("config: {}", app.config.diagnostics.join("; ")));
                }
                dirty = true;
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
        if stale && last_refresh.elapsed() >= FRAME {
            last_refresh = Instant::now();
            stale = false;
            match refresh(client) {
                Ok((snapshot, pending)) => {
                    status.watch(&snapshot);
                    clock.observe(&snapshot, now_secs());
                    app.extras.ages = clock.ages(now_secs());
                    app.extras.pending = pending;
                    let live: std::collections::HashSet<&str> =
                        snapshot.panes.iter().map(|p| p.pane_id.as_str()).collect();
                    app.extras
                        .exited
                        .retain(|pane| live.contains(pane.as_str()));
                    let effects = app.set_snapshot(snapshot);
                    execute_all(app, client, paths, effects);
                }
                Err(err) => app.set_status(format!("herdr: {err}")),
            }
            dirty = true;
        }
        if dirty && last_draw.elapsed() >= FRAME {
            last_draw = Instant::now();
            dirty = false;
            terminal.draw(|frame| app.draw(frame.buffer_mut()))?;
        }
    }
}

fn refresh(client: &Client) -> Result<(api::SessionSnapshot, HashMap<String, usize>), ApiError> {
    let snapshot = api::snapshot_from_result(&client.call("session.snapshot", json!({}))?)?;
    let mut pending: HashMap<String, usize> = HashMap::new();
    // pending counts are decoration; a server without messages still draws
    if let Ok(result) = client.call("message.list", json!({ "pending": true })) {
        for message in api::messages_from_result(&result) {
            *pending.entry(message.to_agent_id).or_default() += 1;
        }
    }
    Ok((snapshot, pending))
}

/// Runs the effects; true when herdr's state may have changed.
fn execute_all(app: &mut App, client: &Client, paths: &Paths, effects: Vec<Effect>) -> bool {
    let mut changed = false;
    for effect in effects {
        changed |= execute(app, client, paths, effect);
    }
    changed
}

fn execute(app: &mut App, client: &Client, paths: &Paths, effect: Effect) -> bool {
    match effect {
        Effect::Persist => {
            if let Err(err) = state::save(&paths.state, &app.ui) {
                app.set_status(format!("state: {err}"));
            }
            false
        }
        Effect::Call(method, params) => {
            if let Err(err) = client.call(method, params) {
                app.set_status(format!("{method}: {err}"));
            }
            true
        }
        Effect::Unfocus => {
            if let Err(err) = client.call("session_pane.unfocus", json!({})) {
                app.set_status(format!("unfocus: {err}"));
            }
            false
        }
        Effect::Reply { agent_id, summary } => {
            let reply_to = client
                .call("message.list", json!({ "from": agent_id }))
                .ok()
                .and_then(|result| {
                    api::messages_from_result(&result)
                        .into_iter()
                        .filter(|m| m.kind == MessageKind::Report)
                        .map(|m| m.message_id)
                        .max()
                });
            let mut params = json!({ "to": agent_id, "summary": summary, "kind": "reply" });
            if let Some(id) = reply_to {
                params["reply_to"] = json!(id);
            }
            if let Err(err) = client.call("message.send", params) {
                app.set_status(format!("reply: {err}"));
            }
            true
        }
        Effect::ClosePreview { agent_id, name } => {
            let result = client
                .call(
                    "agent.close",
                    json!({ "targets": [agent_id], "dry_run": true }),
                )
                .map(|result| closed_names(&result));
            app.close_preview(agent_id, name, result);
            false
        }
        Effect::Close {
            agent_id,
            name,
            force,
        } => {
            match client.call(
                "agent.close",
                json!({ "targets": [agent_id], "force": force }),
            ) {
                Ok(result) => app.set_status(format!(
                    "closed {name}: {} pane(s)",
                    closed_names(&result).len()
                )),
                Err(err) => app.close_failed(agent_id, name, force, err),
            }
            true
        }
        Effect::Run { argv, popup } => {
            if popup {
                match client.call("popup.open", json!({ "argv": argv })) {
                    Ok(_) => return true,
                    // a server without `popup.open` runs it in the background instead
                    Err(err) if err.code == "invalid_request" => {}
                    Err(err) => {
                        app.set_status(format!("popup: {err}"));
                        return false;
                    }
                }
            }
            if let Err(err) = spawn_detached(&argv) {
                app.set_status(format!("{}: {err}", argv[0]));
            }
            true
        }
    }
}

fn closed_names(result: &serde_json::Value) -> Vec<String> {
    result
        .get("agents")
        .and_then(|agents| agents.as_array())
        .map(|agents| {
            agents
                .iter()
                .map(|agent| {
                    agent["name"]
                        .as_str()
                        .or(agent["agent_id"].as_str())
                        .or(agent["pane_id"].as_str())
                        .unwrap_or("?")
                        .to_string()
                })
                .collect()
        })
        .unwrap_or_default()
}

fn spawn_detached(argv: &[String]) -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;
    let mut child = Process::new(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

fn spawn_input(tx: Sender<Msg>) {
    std::thread::spawn(move || loop {
        match crossterm::event::read() {
            Ok(event) => {
                if tx.send(Msg::Term(event)).is_err() {
                    return;
                }
            }
            Err(_) => {
                let _ = tx.send(Msg::Quit);
                return;
            }
        }
    });
}

fn spawn_subscription(client: Client, tx: Sender<Msg>) {
    std::thread::spawn(move || {
        let mut backoff = Duration::from_millis(250);
        loop {
            let started = Instant::now();
            let err = client.subscribe(
                api::lifecycle_subscriptions(),
                |_| {},
                // events since the last snapshot are unknown: read again
                || {
                    let _ = tx.send(Msg::Invalidate {
                        exited: None,
                        closed: None,
                    });
                },
                |line| {
                    if let Some(event) = api::parse_event_line(&line) {
                        let exited = (event.event == "pane.exited")
                            .then(|| event.pane_id.clone())
                            .flatten();
                        let closed = (event.event == "pane.closed")
                            .then_some(event.pane_id)
                            .flatten();
                        let _ = tx.send(Msg::Invalidate { exited, closed });
                    }
                },
            );
            if tx.send(Msg::SubscriptionDown(err)).is_err() {
                return;
            }
            if started.elapsed() > Duration::from_secs(5) {
                backoff = Duration::from_millis(250);
            }
            std::thread::sleep(backoff);
            backoff = (backoff * 2).min(Duration::from_secs(5));
        }
    });
}

/// The per-pane state subscription. `watch` resubscribes whenever the set of
/// agent panes changes, shutting the old connection down to unblock its reader.
struct StatusWatch {
    panes: Vec<String>,
    tx: Sender<Vec<String>>,
    stream: Arc<Mutex<Option<UnixStream>>>,
}

impl StatusWatch {
    fn spawn(client: Client, events: Sender<Msg>) -> Self {
        let (tx, rx) = mpsc::channel::<Vec<String>>();
        let stream: Arc<Mutex<Option<UnixStream>>> = Arc::default();
        let slot = stream.clone();
        std::thread::spawn(move || {
            let Ok(mut panes) = rx.recv() else { return };
            loop {
                while let Ok(newer) = rx.try_recv() {
                    panes = newer;
                }
                if panes.is_empty() {
                    match rx.recv() {
                        Ok(newer) => panes = newer,
                        Err(_) => return,
                    }
                    continue;
                }
                let mut newer = None;
                let err = client.subscribe(
                    api::status_subscriptions(&panes),
                    |stream| newer = connected(&slot, &rx, stream),
                    || {},
                    |_| {
                        let _ = events.send(Msg::Invalidate {
                            exited: None,
                            closed: None,
                        });
                    },
                );
                if let Ok(mut slot) = slot.lock() {
                    slot.take();
                }
                if let Some(newer) = newer {
                    panes = newer;
                    continue;
                }
                // a pane closed under the request, or the server went away: wait
                // for a newer set, else retry this one
                match rx.recv_timeout(Duration::from_secs(1)) {
                    Ok(newer) => panes = newer,
                    Err(RecvTimeoutError::Timeout) => {
                        if err.code != "closed" {
                            let _ = events.send(Msg::Invalidate {
                                exited: None,
                                closed: None,
                            });
                        }
                    }
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
        });
        StatusWatch {
            panes: Vec::new(),
            tx,
            stream,
        }
    }

    fn watch(&mut self, snapshot: &api::SessionSnapshot) {
        let mut panes: Vec<String> = snapshot
            .panes
            .iter()
            .filter(|pane| crate::model::is_agent(pane))
            .map(|pane| pane.pane_id.clone())
            .collect();
        panes.sort();
        if panes == self.panes {
            return;
        }
        self.panes = panes.clone();
        let _ = self.tx.send(panes);
        if let Some(stream) = self.stream.lock().ok().and_then(|mut slot| slot.take()) {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
    }
}

/// Publishes a fresh connection for `watch` to shut down, then looks for a set
/// queued before it was published: `watch` found nothing to close then, so this
/// connection would subscribe to a stale set. It shuts itself down and hands back
/// the newest set instead.
fn connected(
    slot: &Mutex<Option<UnixStream>>,
    rx: &Receiver<Vec<String>>,
    stream: &UnixStream,
) -> Option<Vec<String>> {
    if let (Ok(mut slot), Ok(clone)) = (slot.lock(), stream.try_clone()) {
        *slot = Some(clone);
    }
    let mut newest = None;
    while let Ok(newer) = rx.try_recv() {
        newest = Some(newer);
    }
    if newest.is_some() {
        let _ = stream.shutdown(std::net::Shutdown::Both);
    }
    newest
}

fn spawn_signals(tx: Sender<Msg>) -> std::io::Result<()> {
    use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
    let mut signals = signal_hook::iterator::Signals::new([SIGHUP, SIGINT, SIGTERM])?;
    std::thread::spawn(move || {
        for signal in signals.forever() {
            let msg = if signal == SIGHUP {
                Msg::Reload
            } else {
                Msg::Quit
            };
            if tx.send(msg).is_err() {
                return;
            }
        }
    });
    Ok(())
}

/// The control socket; removed again when dropped.
struct CtlSocket {
    path: PathBuf,
}

impl CtlSocket {
    fn bind(path: &Path, tx: Sender<Msg>) -> Option<Self> {
        // a socket nobody answers on is left over from a run that died
        if path.exists() && UnixStream::connect(path).is_err() {
            let _ = std::fs::remove_file(path);
        }
        let listener = UnixListener::bind(path).ok()?;
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let tx = tx.clone();
                std::thread::spawn(move || serve_ctl(stream, tx));
            }
        });
        Some(CtlSocket {
            path: path.to_path_buf(),
        })
    }
}

impl Drop for CtlSocket {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn serve_ctl(stream: UnixStream, tx: Sender<Msg>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(stream) => stream,
        Err(_) => return,
    });
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let reply = match serde_json::from_str::<serde_json::Value>(&line) {
        Ok(request) => {
            let (reply_tx, reply_rx) = mpsc::channel();
            let sent = tx.send(Msg::Ctl {
                verb: request["verb"].as_str().unwrap_or_default().to_string(),
                pane: request["pane"].as_str().map(str::to_string),
                reply: reply_tx,
            });
            match sent
                .ok()
                .and_then(|_| reply_rx.recv_timeout(Duration::from_secs(2)).ok())
            {
                Some(Ok(text)) => text,
                Some(Err(err)) => format!("error: {err}"),
                None => "error: the sidebar did not answer".into(),
            }
        }
        Err(err) => format!("error: {err}"),
    };
    let mut stream = stream;
    let _ = writeln!(stream, "{reply}");
}

/// `herdr-sidebar ctl <verb> [<pane>]`, run from a keybind or a shell.
pub fn ctl(verb: &str, pane: Option<&str>) -> Result<String, String> {
    let path = state::ctl_socket(&state::herdr_socket());
    let mut stream = UnixStream::connect(&path).map_err(|err| {
        format!(
            "no sidebar is running for this session ({}: {err})",
            path.display()
        )
    })?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|err| err.to_string())?;
    writeln!(stream, "{}", json!({ "verb": verb, "pane": pane })).map_err(|err| err.to_string())?;
    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .map_err(|err| err.to_string())?;
    let line = line.trim().to_string();
    match line.strip_prefix("error: ") {
        Some(err) => Err(err.to_string()),
        None => Ok(line),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_set_queued_before_the_connection_is_published_wins() {
        let slot = Mutex::new(None);
        let (tx, rx) = mpsc::channel();
        let (stream, mut peer) = UnixStream::pair().unwrap();
        tx.send(vec!["w1:p1".to_string()]).unwrap();
        tx.send(vec!["w1:p1".to_string(), "w1:p2".to_string()])
            .unwrap();

        let newest = connected(&slot, &rx, &stream);

        assert_eq!(newest, Some(vec!["w1:p1".to_string(), "w1:p2".to_string()]));
        // the stale connection is shut down, so its reader returns at once
        let mut buf = [0u8; 1];
        assert_eq!(std::io::Read::read(&mut peer, &mut buf).unwrap(), 0);
        assert!(slot.lock().unwrap().is_some());
    }

    #[test]
    fn with_nothing_queued_the_connection_stays_up() {
        let slot = Mutex::new(None);
        let (_tx, rx) = mpsc::channel::<Vec<String>>();
        let (stream, peer) = UnixStream::pair().unwrap();

        assert_eq!(connected(&slot, &rx, &stream), None);

        peer.set_nonblocking(true).unwrap();
        let mut buf = [0u8; 1];
        let read = std::io::Read::read(&mut &peer, &mut buf);
        assert_eq!(read.unwrap_err().kind(), std::io::ErrorKind::WouldBlock);
    }
}
