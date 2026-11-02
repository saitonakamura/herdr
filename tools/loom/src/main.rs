#[cfg(unix)]
mod cli;
#[cfg(unix)]
mod socket;

#[cfg(unix)]
fn run() -> Result<(), String> {
    let args = cli::command().get_matches();
    let caller = std::env::var("LOOM_NODE").ok();
    let request = cli::request(&args, caller.as_deref())?;
    let path = socket::path()?;
    let server = socket::call(&path, "ping", serde_json::json!({}))?;
    cli::check_capabilities(&server["capabilities"], &request)?;
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
