#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{json, Value};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn run(args: &[&str], exchanges: Vec<(&'static str, Value)>) -> std::process::Output {
    let dir = std::env::temp_dir().join(format!(
        "loom-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("s");
    let listener = UnixListener::bind(&path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        for (method, result) in exchanges {
            let start = std::time::Instant::now();
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(err)
                        if err.kind() == std::io::ErrorKind::WouldBlock
                            && start.elapsed().as_secs() < 10 =>
                    {
                        std::thread::yield_now()
                    }
                    Err(err) => panic!("client did not call {method}: {err}"),
                }
            };
            // macOS inherits the listener's nonblocking mode on accepted streams.
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut line = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut line)
                .unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["method"], method);
            if method == "wait.declare" {
                assert_eq!(request["params"]["waiter"], "caller");
                assert_eq!(
                    request["params"]["on"],
                    json!({"kind":"node","target":"child"})
                );
                assert_eq!(request["params"]["until"], "complete");
            }
            writeln!(stream, "{}", json!({"id": request["id"], "result":result})).unwrap();
        }
    });
    let output = Command::new(env!("CARGO_BIN_EXE_loom-rust"))
        .args(args)
        .env("LOOM_NODE", "caller")
        .env("HERDR_SOCKET_PATH", path)
        .output()
        .unwrap();
    server.join().unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    output
}

#[test]
fn a_declaration_probes_then_round_trips_a_satisfied_wait() {
    let output = run(
        &[
            "wait",
            "--on",
            "node:child",
            "--deadline",
            "1h",
            "--then",
            "self",
            "--json",
        ],
        vec![
            (
                "ping",
                json!({"capabilities":{"agent_waits":true,"wait_sources":["node"]}}),
            ),
            (
                "wait.declare",
                json!({"wait":{"wait_id":9,"state":"satisfied","extensions":0}}),
            ),
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["result"]["wait"]["state"], "satisfied");
}

#[test]
fn an_old_server_is_refused_before_mutation() {
    let output = run(
        &["watch", "child", "--deadline", "1h", "--then", "self"],
        vec![("ping", json!({"capabilities":{"agent_waits":true}}))],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("wait_sources"));
}

#[test]
fn other_sources_point_to_the_follow_up_even_when_advertised() {
    let output = run(
        &[
            "wait",
            "--on",
            "pr:owner/repo#1",
            "--until",
            "merged",
            "--deadline",
            "1h",
            "--then",
            "self",
        ],
        vec![(
            "ping",
            json!({"capabilities":{"agent_waits":true,"wait_sources":["node","pr"]}}),
        )],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("ticket 100"));
}
