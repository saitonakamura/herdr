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
                match request["params"]["on"]["kind"].as_str().unwrap() {
                    "node" => {
                        assert_eq!(request["params"]["on"]["target"], "child");
                        assert_eq!(request["params"]["until"], "complete");
                    }
                    "mr" => {
                        assert_eq!(request["params"]["on"]["target"], "gl:group/project!7");
                        assert_eq!(request["params"]["until"], "merged");
                    }
                    "run" => {
                        let target = request["params"]["on"]["target"].as_str().unwrap();
                        let (pid, exit) = target.split_once(':').unwrap();
                        assert!(pid.parse::<u32>().unwrap() > 0);
                        assert!(std::path::Path::new(exit).is_absolute());
                        assert!(exit.ends_with("/caller/test-job/exit"));
                        assert_eq!(request["params"]["until"], "success");
                    }
                    other => panic!("unexpected source {other}"),
                }
            }
            writeln!(stream, "{}", json!({"id": request["id"], "result":result})).unwrap();
        }
    });
    let output = Command::new(env!("CARGO_BIN_EXE_loom-rust"))
        .args(args)
        .env("LOOM_NODE", "caller")
        .env("HERDR_SOCKET_PATH", path)
        .env("LOOM_RUN_DIR", dir.join("runs"))
        .output()
        .unwrap();
    server.join().unwrap();
    if let Ok(value) = serde_json::from_slice::<Value>(&output.stdout) {
        if let Some(exit_file) = value["result"]["run"]["exit_file"].as_str() {
            let started = std::time::Instant::now();
            while !std::path::Path::new(exit_file).exists() {
                assert!(
                    started.elapsed().as_secs() < 10,
                    "supervisor never wrote its exit file"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            assert_eq!(std::fs::read_to_string(exit_file).unwrap(), "7\n");
            assert_eq!(
                std::fs::read_to_string(value["result"]["run"]["log_file"].as_str().unwrap())
                    .unwrap(),
                "command output\n"
            );
            assert!(std::path::Path::new(exit_file).is_absolute());
        }
    }
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
fn reader_support_is_required_before_mutation() {
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
            json!({"capabilities":{"agent_waits":true,"wait_sources":["node","run"]}}),
        )],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("plugin readers"));
}

#[test]
fn a_reader_source_round_trips_with_normalized_shorthand() {
    let output = run(
        &[
            "wait",
            "--on",
            "mr:group/project!7",
            "--until",
            "merged",
            "--deadline",
            "1h",
            "--then",
            "self",
            "--json",
        ],
        vec![
            (
                "ping",
                json!({"capabilities":{"agent_waits":true,"wait_sources":["node","run"],"wait_readers":true}}),
            ),
            (
                "wait.declare",
                json!({"wait":{"wait_id":10,"state":"live"}}),
            ),
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn run_is_detached_and_retains_exit_code_and_output() {
    let output = run(
        &[
            "run",
            "--name",
            "test-job",
            "--json",
            "--",
            "/bin/sh",
            "-c",
            "sleep 0.1; printf 'command output\\n'; exit 7",
        ],
        vec![(
            "ping",
            json!({"capabilities":{"agent_waits":true,"wait_sources":["node","run"]}}),
        )],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["result"]["run"]["name"], "test-job");
}

#[test]
fn run_can_declare_its_wait_in_the_same_call() {
    let output = run(
        &[
            "run",
            "--name",
            "test-job",
            "--deadline",
            "1h",
            "--then",
            "parent",
            "--until",
            "success",
            "--json",
            "--",
            "/bin/sh",
            "-c",
            "sleep 0.1; printf 'command output\\n'; exit 7",
        ],
        vec![
            (
                "ping",
                json!({"capabilities":{"agent_waits":true,"wait_sources":["node","run"]}}),
            ),
            (
                "wait.declare",
                json!({"wait":{"wait_id":11,"state":"live"}}),
            ),
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["result"]["wait"]["wait_id"], 11);
}

#[test]
fn run_is_not_started_on_an_old_server() {
    let output = run(
        &[
            "run", "--name", "test-job", "--", "/bin/sh", "-c", "exit 99",
        ],
        vec![(
            "ping",
            json!({"capabilities":{"agent_waits":true,"wait_sources":["node"]}}),
        )],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("wait_sources"));
}
