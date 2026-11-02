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
    // Monitoring, persistence, cycle checks and the two-extension budget stay server-owned.
    let result = socket::call(&path, request.method, request.params)?;
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

#[cfg(unix)]
fn print_wait(wait: &serde_json::Value) {
    println!(
        "{} {} — {} (deadline {} ms, then {}, extensions {})",
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
