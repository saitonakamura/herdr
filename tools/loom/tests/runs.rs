#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::process::Command;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

#[test]
fn named_runs_are_caller_scoped_non_reusable_and_resolve_for_later_waits() {
    let dir = std::env::temp_dir().join(format!("loom-runs-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let socket = dir.join("s");
    let listener = UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        let mut declaration = Value::Null;
        for index in 0..5 {
            let started = Instant::now();
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(err)
                        if err.kind() == std::io::ErrorKind::WouldBlock
                            && started.elapsed() < Duration::from_secs(10) =>
                    {
                        std::thread::yield_now()
                    }
                    Err(err) => panic!("missing request {index}: {err}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut line = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut line)
                .unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            let result = if index == 4 {
                assert_eq!(request["method"], "wait.declare");
                declaration = request["params"].clone();
                json!({"wait":{"wait_id":1,"state":"satisfied"}})
            } else {
                assert_eq!(request["method"], "ping");
                json!({"capabilities":{"agent_waits":true,"wait_sources":["node","run"]}})
            };
            writeln!(stream, "{}", json!({"id":request["id"],"result":result})).unwrap();
        }
        declaration
    });
    let invoke = |caller: &str, args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_loom-rust"))
            .args(args)
            .env("LOOM_NODE", caller)
            .env("HERDR_SOCKET_PATH", &socket)
            .env("LOOM_RUN_DIR", dir.join("runs"))
            .output()
            .unwrap()
    };
    let started = invoke(
        "caller",
        &[
            "run",
            "--name",
            "build",
            "--json",
            "--",
            "/bin/sh",
            "-c",
            "printf '%s\\n' \"$1\"",
            "sh",
            "spaces; literal $(not-evaluated)",
        ],
    );
    assert!(
        started.status.success(),
        "{}",
        String::from_utf8_lossy(&started.stderr)
    );
    let record: Value =
        serde_json::from_slice::<Value>(&started.stdout).unwrap()["result"]["run"].clone();
    let duplicate = invoke(
        "caller",
        &[
            "run",
            "--name",
            "build",
            "--",
            "touch",
            dir.join("unexpected").to_str().unwrap(),
        ],
    );
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("names cannot be reused"));
    assert!(!dir.join("unexpected").exists());
    let foreign = invoke(
        "other-caller",
        &[
            "wait",
            "--on",
            "run:build",
            "--deadline",
            "1h",
            "--then",
            "self",
        ],
    );
    assert!(!foreign.status.success());
    assert!(String::from_utf8_lossy(&foreign.stderr).contains("start it with loom run first"));
    let wait = invoke(
        "caller",
        &[
            "wait",
            "--on",
            "run:build",
            "--deadline",
            "1h",
            "--then",
            "self",
        ],
    );
    assert!(
        wait.status.success(),
        "{}",
        String::from_utf8_lossy(&wait.stderr)
    );
    let declaration = server.join().unwrap();
    assert_eq!(
        declaration["on"],
        json!({"kind":"run","target":format!("{}:{}", record["pid"], record["exit_file"].as_str().unwrap())})
    );
    assert_eq!(declaration["waiter"], "caller");
    assert_eq!(declaration["until"], "exit");
    let exit = std::path::Path::new(record["exit_file"].as_str().unwrap());
    let deadline = Instant::now() + Duration::from_secs(5);
    while !exit.exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(std::fs::read_to_string(exit).unwrap(), "0\n");
    assert_eq!(
        std::fs::read_to_string(record["log_file"].as_str().unwrap()).unwrap(),
        "spaces; literal $(not-evaluated)\n"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
