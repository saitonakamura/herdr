//! `herdr-sidebar`: the program the fork's sidebar pane runs (ADR 0006). It
//! reads only herdr's API on `HERDR_SOCKET_PATH`.

// Off unix only the stub `run` is built, so the model and its IO helpers go unused there.
#![cfg_attr(not(unix), allow(dead_code))]

mod api;
mod app;
mod config;
mod glyphs;
mod keys;
mod layout;
mod model;
#[cfg(unix)]
mod run;
#[cfg(test)]
mod schema_tests;
mod state;

const USAGE: &str = "\
usage: herdr-sidebar [run]                the sidebar, as the sidebar pane runs it
       herdr-sidebar ctl fold [<pane>]    fold or unfold that pane's node (default: the focused pane)
       herdr-sidebar ctl scope [<pane>]   scope to that pane's subtree, or clear the scope
       herdr-sidebar glyphs               print the glyph table in use
       herdr-sidebar --version";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let code = match args.as_slice() {
        ["--version" | "-V"] => {
            println!("herdr-sidebar {}", env!("CARGO_PKG_VERSION"));
            0
        }
        ["--help" | "-h" | "help"] => {
            println!("{USAGE}");
            0
        }
        ["glyphs"] => {
            print!(
                "{}",
                config::load(&config::config_path()).glyphs.table_text()
            );
            0
        }
        ["ctl", verb @ ("fold" | "scope"), rest @ ..] if rest.len() <= 1 => {
            ctl(verb, rest.first().copied())
        }
        [] | ["run"] => run(),
        _ => {
            eprintln!("{USAGE}");
            2
        }
    };
    std::process::exit(code);
}

#[cfg(unix)]
fn ctl(verb: &str, pane: Option<&str>) -> i32 {
    match run::ctl(verb, pane) {
        Ok(_) => 0,
        Err(err) => {
            eprintln!("herdr-sidebar: {err}");
            1
        }
    }
}

#[cfg(unix)]
fn run() -> i32 {
    match run::run(run::Paths::from_env()) {
        Ok(run::Exit::Quit) => 0,
        Ok(run::Exit::Upgrade(exe)) => upgrade(&exe),
        Err(err) => {
            eprintln!("herdr-sidebar: {err}");
            1
        }
    }
}

/// Runs the newly installed binary in this process, so the pane keeps going
/// without herdr's restart backoff. If that fails, exiting lets herdr restart it.
#[cfg(unix)]
fn upgrade(exe: &std::path::Path) -> i32 {
    use std::os::unix::process::CommandExt;
    let err = std::process::Command::new(exe)
        .args(std::env::args_os().skip(1))
        .exec();
    eprintln!("herdr-sidebar: upgrade: {err}");
    1
}

#[cfg(not(unix))]
fn ctl(_verb: &str, _pane: Option<&str>) -> i32 {
    run()
}

// Windows keeps herdr's native sidebar (ADR 0006); only the pure parts build there.
#[cfg(not(unix))]
fn run() -> i32 {
    eprintln!("herdr-sidebar: unix only");
    1
}
