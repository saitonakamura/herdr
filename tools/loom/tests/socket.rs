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
            ("wait.get", json!({"wait":{"wait_id":10,"state":"live"}})),
            (
                "wait.get",
                json!({"wait":{"wait_id":10,"state":"live","last_ok_at_ms":1,"last_state":"open"}}),
            ),
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["result"]["wait"]["last_state"], "open",
        "the declare answers once the server's first read has landed"
    );
}

fn reader_declare(exchanges: Vec<(&'static str, Value)>) -> std::process::Output {
    reader_declare_on(0, exchanges)
}

/// A reader declare on a server that makes `retries` quick re-reads of a failed first read.
fn reader_declare_on(retries: u64, exchanges: Vec<(&'static str, Value)>) -> std::process::Output {
    let mut all = vec![
        (
            "ping",
            json!({"capabilities":{"agent_waits":true,"wait_sources":["node","run"],"wait_readers":true,
                "wait_read_retries":retries}}),
        ),
        (
            "wait.declare",
            json!({"wait":{"wait_id":10,"state":"live"}}),
        ),
    ];
    all.extend(exchanges);
    run(
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
        ],
        all,
    )
}

#[test]
fn a_reader_wait_whose_first_read_fails_is_refused_and_cancelled() {
    // 110: the work server's reader had no GITLAB_TOKEN, every read was a 404, and the wait sat live
    let output = reader_declare(vec![
        (
            "wait.get",
            json!({"wait":{"wait_id":10,"state":"live","errors":1,
                "last_error":"glab MR gl:group/project!7 failed: 404 Project Not Found"}}),
        ),
        (
            "wait.cancel",
            json!({"wait":{"wait_id":10,"state":"cancelled"}}),
        ),
    ]);
    assert!(
        !output.status.success(),
        "a wait the server cannot read is not live"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("404 Project Not Found"), "{stderr}");
    assert!(stderr.contains("cancelled"), "{stderr}");
    assert!(
        stderr.contains("the server's env"),
        "a 404 is auth-shaped: {stderr}"
    );
    assert!(output.stdout.is_empty(), "nothing reads as declared");
}

#[test]
fn a_failed_first_read_the_server_re_reads_is_waited_out() {
    // 113: one transient glab failure refused a wait whose source read `open` seconds later
    let output = reader_declare_on(
        2,
        vec![
            (
                "wait.get",
                json!({"wait":{"wait_id":10,"state":"live","errors":1,
                    "last_error":"glab MR gl:group/project!7 failed:"}}),
            ),
            (
                "wait.get",
                json!({"wait":{"wait_id":10,"state":"live","errors":0,"last_ok_at_ms":2,"last_state":"open"}}),
            ),
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("10 live"), "{stdout}");
    assert!(stdout.contains("last read open"), "{stdout}");
}

#[test]
fn a_refusal_says_what_failed_and_blames_the_env_only_for_auth() {
    for (error, says) in [
        (
            "glab MR gl:group/project!7 timed out after 20s",
            "the read timed out (glab MR gl:group/project!7 timed out after 20s)",
        ),
        (
            "glab MR gl:group/project!7 failed:",
            "the reader gave no reason (\"glab MR gl:group/project!7 failed:\")",
        ),
    ] {
        let output = reader_declare(vec![
            (
                "wait.get",
                json!({"wait":{"wait_id":10,"state":"live","errors":1,"last_error":error}}),
            ),
            (
                "wait.cancel",
                json!({"wait":{"wait_id":10,"state":"cancelled"}}),
            ),
        ]);
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(says), "{stderr}");
        assert!(!stderr.contains("env"), "not an auth failure: {stderr}");
    }
}

#[test]
fn a_source_unreachable_after_the_re_reads_is_refused_with_the_last_error() {
    let output = reader_declare_on(
        2,
        vec![(
            "wait.get",
            json!({"wait":{"wait_id":10,"state":"fired","errors":3,
                "last_error":"glab MR gl:group/project!7 timed out after 20s",
                "fired":{"reason":"failed: source unreachable","to":[]}}}),
        )],
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("refused: source unreachable: the read timed out"),
        "{stderr}"
    );
    assert!(stderr.contains("already fired"), "{stderr}");
}

#[test]
fn an_until_the_source_never_reports_is_refused_with_the_servers_hint() {
    // 113: pr waits declared --until passed sat live, reading open, until their deadlines
    let why = "`passed` is a ci: state, not one pr: reports (open, merged, closed); did you mean --on ci:o/r#59?";
    let output = reader_declare_on(
        2,
        vec![(
            "wait.get",
            json!({"wait":{"wait_id":10,"state":"fired","errors":0,"last_ok_at_ms":1,"last_state":"open",
                "fired":{"reason":format!("failed: {why}"),"to":[]}}}),
        )],
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!("refused: {why}; wait 10")),
        "{stderr}"
    );
}

#[test]
fn a_reader_wait_the_reader_refuses_is_refused() {
    let output = reader_declare(vec![(
        "wait.get",
        json!({"wait":{"wait_id":10,"state":"fired","errors":1,"last_error":"pr: bad ref",
            "fired":{"reason":"failed: the reader cannot read this source","to":[]}}}),
    )]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("pr: bad ref"), "{stderr}");
}

#[test]
fn a_reader_wait_settled_by_its_first_read_says_so() {
    let output = reader_declare(vec![(
        "wait.get",
        json!({"wait":{"wait_id":10,"state":"satisfied","summary":"mr:gl:group/project!7 reaches merged",
            "last_ok_at_ms":1,"last_state":"merged","deadline_at_ms":2,"then":"self","extensions":0}}),
    )]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("10 satisfied"), "{stdout}");
    assert!(stdout.contains("last read merged"), "{stdout}");
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
