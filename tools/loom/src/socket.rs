use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::{json, Value};

pub fn path() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("HERDR_SOCKET_PATH").filter(|p| !p.is_empty()) {
        return Ok(path.into());
    }
    let config = if let Some(path) = std::env::var_os("HERDR_CONFIG_PATH").filter(|p| !p.is_empty())
    {
        PathBuf::from(path)
            .parent()
            .ok_or("HERDR_CONFIG_PATH has no parent")?
            .to_path_buf()
    } else if let Some(base) = std::env::var_os("XDG_CONFIG_HOME").filter(|p| !p.is_empty()) {
        PathBuf::from(base).join("herdr")
    } else {
        PathBuf::from(std::env::var_os("HOME").ok_or("HOME is unset; set HERDR_SOCKET_PATH")?)
            .join(".config/herdr")
    };
    match std::env::var("HERDR_SESSION")
        .ok()
        .filter(|s| !s.is_empty() && s != "default")
    {
        Some(session) => Ok(config.join("sessions").join(session).join("herdr.sock")),
        None => Ok(config.join("herdr.sock")),
    }
}

pub fn call(path: &std::path::Path, method: &str, params: Value) -> Result<Value, String> {
    let mut stream =
        UnixStream::connect(path).map_err(|err| format!("{}: {err}", path.display()))?;
    let timeout = Some(Duration::from_secs(10));
    stream
        .set_read_timeout(timeout)
        .map_err(|err| err.to_string())?;
    stream
        .set_write_timeout(timeout)
        .map_err(|err| err.to_string())?;
    writeln!(
        stream,
        "{}",
        json!({"id":"loom", "method":method, "params":params})
    )
    .map_err(|err| err.to_string())?;
    let mut line = String::new();
    // A bad endpoint must not make a one-shot CLI allocate an unbounded response.
    BufReader::new(stream)
        .take(8 * 1024 * 1024)
        .read_line(&mut line)
        .map_err(|err| err.to_string())?;
    response(&line)
}

fn response(line: &str) -> Result<Value, String> {
    let value: Value =
        serde_json::from_str(line).map_err(|err| format!("invalid Herdr response: {err}"))?;
    if value["id"] != "loom" {
        return Err("Herdr response id does not match request".into());
    }
    if let Some(error) = value.get("error") {
        return Err(format!(
            "{}: {}",
            error["code"].as_str().unwrap_or("error"),
            error["message"].as_str().unwrap_or("Herdr request failed")
        ));
    }
    value
        .get("result")
        .cloned()
        .ok_or_else(|| "Herdr response has neither result nor error".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_and_malformed_replies_do_not_look_like_success() {
        for line in [
            "",
            "{}",
            r#"{"id":"other","result":{}}"#,
            r#"{"id":"loom","error":{"code":"wait_cycle","message":"cycle refused"}}"#,
        ] {
            assert!(response(line).is_err());
        }
        assert_eq!(
            response(r#"{"id":"loom","result":{"wait":{"state":"satisfied"}}}"#).unwrap()["wait"]
                ["state"],
            "satisfied"
        );
    }
}
