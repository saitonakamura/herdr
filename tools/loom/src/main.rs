#[cfg(unix)]
mod cli;
#[cfg(unix)]
mod runs;
#[cfg(unix)]
mod socket;

#[cfg(unix)]
fn run() -> Result<(), String> {
    let args = cli::command().get_matches();
    let caller = std::env::var("LOOM_NODE").ok();
    let path = socket::path()?;
    if let Some(("run", run)) = args.subcommand() {
        let caller = caller
            .as_deref()
            .filter(|id| !id.is_empty())
            .ok_or("LOOM_NODE is required")?;
        let name = run.get_one::<String>("name").ok_or("missing --name")?;
        let command = run
            .get_many::<String>("cmd")
            .ok_or("missing command")?
            .cloned()
            .collect::<Vec<_>>();
        let mut declaration = if run.contains_id("deadline") {
            Some(cli::request(&args, Some(caller))?)
        } else {
            None
        };
        let server = socket::call(&path, "ping", serde_json::json!({}))?;
        let probe = cli::Request {
            method: "wait.declare",
            params: serde_json::json!({"on":{"kind":"run"}}),
            json: false,
        };
        cli::check_capabilities(&server["capabilities"], &probe)?;
        let record = runs::start(caller, name, &command)?;
        let mut result = serde_json::json!({"run":record});
        if let Some(request) = declaration.as_mut() {
            request.params["on"]["target"] = runs::resolve(caller, name)?.into();
            let declared = socket::call(&path, request.method, request.params.clone())
                .map_err(|err| format!("run:{name} started (pid {}, log {}) but wait declaration failed: {err}; do not restart the command; inspect loom wait list --all before declaring its wait", record["pid"], record["log_file"]))?;
            result["wait"] = declared["wait"].clone();
        }
        if args.get_flag("json") {
            println!("{}", serde_json::json!({"result":result}));
        } else {
            println!(
                "run:{name} started (pid {}, log {})",
                record["pid"], record["log_file"]
            );
            if let Some(wait) = result.get("wait") {
                print_wait(wait);
            } else {
                println!(
                    "declare a wait with --on run:{name} --deadline D --then self|parent|user"
                );
            }
        }
        return Ok(());
    }
    let mut request = cli::request(&args, caller.as_deref())?;
    let server = socket::call(&path, "ping", serde_json::json!({}))?;
    cli::check_capabilities(&server["capabilities"], &request)?;
    if request.params["on"]["kind"] == "run" {
        let name = request.params["on"]["target"]
            .as_str()
            .ok_or("missing run name")?;
        request.params["on"]["target"] =
            runs::resolve(caller.as_deref().ok_or("LOOM_NODE is required")?, name)?.into();
    }
    let reader = request.method == "wait.declare"
        && !["node", "run"].contains(&request.params["on"]["kind"].as_str().unwrap_or("node"));
    // Monitoring, persistence, cycle checks and the two-extension budget stay server-owned.
    let mut result = socket::call(&path, request.method, request.params)?;
    if reader {
        result["wait"] = first_read(
            &path,
            caller.as_deref().unwrap_or_default(),
            &result["wait"],
            server["capabilities"]["wait_read_retries"]
                .as_u64()
                .unwrap_or(0),
        )?;
    }
    if request.json {
        println!("{}", serde_json::json!({"result":result}));
    } else if let Some(waits) = result["waits"].as_array() {
        if waits.is_empty() {
            println!("no waits");
        }
        for wait in waits {
            print_wait(wait);
        }
    } else if let Some(wait) = result.get("wait") {
        print_wait(wait);
    } else {
        println!("{result}");
    }
    Ok(())
}

/// How long the server's first read may take to land: its 30 s read timeout and a margin.
#[cfg(unix)]
const FIRST_READ: std::time::Duration = std::time::Duration::from_secs(40);

/// A reader wait counts as declared only once the server has read its source, which it does at
/// once: a reader that works in the caller's pane can still fail in the server's env (110). A
/// server that re-reads a failed first read (`retries`) gets the chance before a refusal (113).
#[cfg(unix)]
fn first_read(
    path: &std::path::Path,
    caller: &str,
    declared: &serde_json::Value,
    retries: u64,
) -> Result<serde_json::Value, String> {
    let id = declared["wait_id"]
        .as_u64()
        .ok_or("declare returned no wait id")?;
    let started = std::time::Instant::now();
    let refuse = |why: String| {
        let cancelled = socket::call(
            path,
            "wait.cancel",
            serde_json::json!({"wait_id": id, "by": caller}),
        );
        let after = match cancelled {
            Ok(_) => format!("wait {id} cancelled"),
            Err(err) => {
                format!("cancelling wait {id} failed too ({err}); run loom wait cancel {id}")
            }
        };
        Err(format!("refused: {why}; {after}"))
    };
    loop {
        let wait =
            socket::call(path, "wait.get", serde_json::json!({"wait_id": id}))?["wait"].clone();
        let errors = wait["errors"].as_u64().unwrap_or(0);
        match wait["state"].as_str() {
            Some("live") if wait["last_ok_at_ms"].is_u64() => return Ok(wait),
            Some("live") if errors > retries => return refuse(unreadable(&wait)),
            Some("live") => {}
            Some("fired") => {
                let reason = wait["fired"]["reason"].as_str().unwrap_or("the wait fired");
                let reason = reason.strip_prefix("failed: ").unwrap_or(reason);
                let why = match wait["last_error"].as_str() {
                    Some(error) if errors > 0 => format!("{reason}: {}", failure(error)),
                    _ => reason.to_owned(),
                };
                return Err(format!("refused: {why}; wait {id} already fired"));
            }
            _ => return Ok(wait),
        }
        if started.elapsed() >= FIRST_READ {
            return refuse(if errors > 0 {
                unreadable(&wait)
            } else {
                format!(
                    "the server cannot read this source: no read landed within {}s",
                    FIRST_READ.as_secs()
                )
            });
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

/// Why the server could not read a wait's source, from its run of failed reads.
#[cfg(unix)]
fn unreadable(wait: &serde_json::Value) -> String {
    let errors = wait["errors"].as_u64().unwrap_or(0);
    let error = wait["last_error"].as_str().unwrap_or("");
    let reads = if errors == 1 {
        "its read".to_owned()
    } else {
        format!("{errors} reads in a row")
    };
    format!(
        "the server cannot read this source: {reads} failed: {}",
        failure(error)
    )
}

/// A reader's error, said by what failed: only an auth-shaped one points at the server's env,
/// which an agent read as "the server has no access" after a mere timeout (113).
#[cfg(unix)]
fn failure(error: &str) -> String {
    let error = error.trim();
    let lower = error.to_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|needle| lower.contains(needle));
    if error.is_empty() || error.ends_with(':') || lower == "failed" {
        format!("the reader gave no reason ({error:?})")
    } else if has(&["timed out", "timeout", "deadline exceeded", "was killed"]) {
        format!("the read timed out ({error})")
    } else if has(&[
        "401",
        "403",
        "404",
        "token",
        "unauthorized",
        "forbidden",
        "not found",
        "not logged in",
    ]) {
        format!("{error}; herdr reads it with the server's env, not this pane's")
    } else {
        error.to_owned()
    }
}

#[cfg(unix)]
fn print_wait(wait: &serde_json::Value) {
    let read = match (wait["errors"].as_u64(), wait["last_state"].as_str()) {
        (Some(errors), _) if errors > 0 => format!(
            ", {errors} failed reads in a row: {}",
            wait["last_error"].as_str().unwrap_or("unknown error")
        ),
        (_, Some(state)) => format!(", last read {state}"),
        _ => String::new(),
    };
    println!(
        "{} {} — {} (deadline {} ms, then {}, extensions {}{read})",
        wait["wait_id"],
        wait["state"].as_str().unwrap_or("unknown"),
        wait["summary"].as_str().unwrap_or("agent wait"),
        wait["deadline_at_ms"],
        wait["then"].as_str().unwrap_or("unknown"),
        wait["extensions"]
    );
}

#[cfg(unix)]
fn main() {
    if let Err(error) = run() {
        eprintln!("loom: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(unix))]
fn main() {
    eprintln!("loom-rust requires a Unix Herdr socket");
    std::process::exit(1);
}
