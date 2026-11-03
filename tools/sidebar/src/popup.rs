//! The menu popup's IO: the popup's terminal, herdr's socket, the commands the
//! menu runs and the sidebar's control socket.

use std::process::{Command as Process, Stdio};
use std::time::{Duration, Instant};

use crossterm::event::{DisableMouseCapture, EnableMouseCapture, Event};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use serde_json::{json, Value};

use crate::api::{self, ApiError, Client};
use crate::menu::{self, Exit, Io, Menu, Output, Target};
use crate::{config, run, state};

struct Live {
    client: Client,
}

impl Io for Live {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, ApiError> {
        self.client.call(method, params)
    }

    fn run(&mut self, argv: &[String]) -> Output {
        match Process::new(&argv[0])
            .args(&argv[1..])
            .stdin(Stdio::null())
            .output()
        {
            Ok(out) => Output {
                ok: out.status.success(),
                stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            },
            Err(err) => Output {
                ok: false,
                stdout: String::new(),
                stderr: format!("{}: {err}", argv[0]),
            },
        }
    }

    fn spawn(&mut self, argv: &[String]) -> Result<(), String> {
        run::spawn_detached(argv).map_err(|err| err.to_string())
    }

    fn ctl(&mut self, verb: &str, arg: &str) -> Result<(), String> {
        run::ctl(verb, Some(arg)).map(|_| ())
    }
}

/// `herdr-sidebar menu pane|workspace <id> [--refocus]`, as the sidebar opens it in
/// a popup. `refocus` gives keyboard focus back to the sidebar when the menu ends.
pub fn show(target: Target, refocus: bool) -> std::io::Result<i32> {
    let client = Client {
        socket: state::herdr_socket(),
    };
    let config = config::load(&config::config_path());
    let loaded = client
        .call("session.snapshot", json!({}))
        .and_then(|r| api::snapshot_from_result(&r))
        .map_err(|err| format!("herdr: {err}"))
        .and_then(|snapshot| {
            menu::subject(&snapshot, &target, |agent| {
                let result = client.call("message.list", json!({ "from": agent })).ok()?;
                menu::latest_report(api::messages_from_result(&result))
            })
        });

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        previous_hook(info);
    }));
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    terminal.hide_cursor()?;

    let mut io = Live {
        client: client.clone(),
    };
    let exit = match loaded {
        Ok(subject) => event_loop(&mut Menu::new(subject, config), &mut io, &mut terminal),
        Err(err) => show_error(&err, &mut terminal).map(|_| Exit::Quit),
    };
    restore_terminal();
    match exit? {
        Exit::Quit => {
            if refocus {
                // the popup is still open, but the flag outlives it: keys come back to the sidebar
                let _ = client.call("session_pane.focus", json!({}));
            }
            Ok(0)
        }
        Exit::Exec(argv) => {
            use std::os::unix::process::CommandExt;
            let err = Process::new(&argv[0]).args(&argv[1..]).exec();
            eprintln!("herdr-sidebar: {}: {err}", argv[0]);
            std::thread::sleep(Duration::from_secs(2));
            Ok(1)
        }
        Exit::After(method, params) => {
            let exe = std::env::current_exe()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| "herdr-sidebar".into());
            let _ = run::spawn_detached(&[
                exe,
                "after-popup".into(),
                method.into(),
                params.to_string(),
            ]);
            Ok(0)
        }
    }
}

fn event_loop(
    menu: &mut Menu,
    io: &mut Live,
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
) -> std::io::Result<Exit> {
    loop {
        terminal.draw(|frame| menu.draw(frame.buffer_mut()))?;
        if let Some(exit) = menu.exit.take() {
            return Ok(exit);
        }
        if menu.job.is_some() {
            menu.work(io);
            continue;
        }
        match crossterm::event::read()? {
            Event::Key(key) => menu.key(key),
            Event::Mouse(mouse) => menu.mouse(mouse),
            _ => {}
        }
    }
}

fn show_error(
    err: &str,
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
) -> std::io::Result<()> {
    terminal.draw(|frame| {
        let buf = frame.buffer_mut();
        let width = buf.area.width.saturating_sub(2) as usize;
        for (row, line) in menu::wrap(&format!("menu: {err}"), width)
            .into_iter()
            .chain(["".into(), "any key closes".into()])
            .enumerate()
            .take(buf.area.height as usize)
        {
            buf.set_stringn(1, row as u16, line, width, ratatui::style::Style::default());
        }
    })?;
    loop {
        if let Event::Key(_) = crossterm::event::read()? {
            return Ok(());
        }
    }
}

fn restore_terminal() {
    let _ = execute!(
        std::io::stdout(),
        DisableMouseCapture,
        LeaveAlternateScreen,
        crossterm::cursor::Show
    );
    let _ = disable_raw_mode();
}

/// `herdr-sidebar after-popup <method> <params>`: makes the request once the
/// menu's popup has closed, which herdr says by no longer answering "busy".
pub fn after_popup(method: &str, params: &str) -> Result<(), String> {
    let params: Value = serde_json::from_str(params).map_err(|err| err.to_string())?;
    let client = Client {
        socket: state::herdr_socket(),
    };
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match client.call(method, params.clone()) {
            Ok(_) => return Ok(()),
            Err(err)
                if matches!(err.code.as_str(), "ui_busy" | "popup_already_open")
                    && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(err) => return Err(err.to_string()),
        }
    }
}
