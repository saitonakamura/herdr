use std::fs::{self, OpenOptions};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{json, Value};

fn component(value: &str) -> Result<&str, String> {
    if value.is_empty()
        || value.len() > 64
        || !value.as_bytes()[0].is_ascii_alphanumeric()
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("run names and caller ids must be 1 to 64 ASCII letters, digits, - or _, starting with a letter or digit".into());
    }
    Ok(value)
}

fn directory(caller: &str, name: &str) -> Result<PathBuf, String> {
    component(caller)?;
    component(name)?;
    let base = if let Some(dir) = std::env::var_os("LOOM_RUN_DIR").filter(|p| !p.is_empty()) {
        PathBuf::from(dir)
    } else if let Some(dir) = std::env::var_os("XDG_STATE_HOME").filter(|p| !p.is_empty()) {
        PathBuf::from(dir).join("loom/runs")
    } else {
        PathBuf::from(std::env::var_os("HOME").ok_or("HOME is unset")?)
            .join(".local/state/loom/runs")
    };
    if !base.is_absolute() {
        return Err("run state directory must be absolute".into());
    }
    Ok(base.join(caller).join(name))
}

pub fn resolve(caller: &str, name: &str) -> Result<String, String> {
    let dir = directory(caller, name)?;
    let record: Value = serde_json::from_slice(
        &fs::read(dir.join("run.json"))
            .map_err(|err| format!("run {name:?}: {err}; start it with loom run first"))?,
    )
    .map_err(|err| format!("invalid run record: {err}"))?;
    let pid = record["pid"]
        .as_u64()
        .filter(|pid| *pid > 0 && *pid <= u32::MAX as u64)
        .ok_or("invalid run pid")?;
    let exit = record["exit_file"]
        .as_str()
        .ok_or("missing run exit file")?;
    if Path::new(exit) != dir.join("exit") {
        return Err("run exit file does not match its record directory".into());
    }
    Ok(format!("{pid}:{exit}"))
}

pub fn start(caller: &str, name: &str, argv: &[String]) -> Result<Value, String> {
    if argv.is_empty() {
        return Err("run requires a command after --".into());
    }
    let dir = directory(caller, name)?;
    let parent = dir.parent().ok_or("run directory has no parent")?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)
        .map_err(|err| err.to_string())?;
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&dir)
        .map_err(|err| format!("cannot reserve run {name:?}: {err}; names cannot be reused"))?;
    let exit = dir.join("exit");
    let log = dir.join("output.log");
    let output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&log)
        .map_err(|err| err.to_string())?;
    // This shell stays alive until the command finishes; Herdr only observes its pid and exit file.
    let mut child = Command::new("/bin/sh")
        .arg("-c")
        .arg("trap '' HUP; exit_file=$1; shift; \"$@\"; code=$?; printf '%s\\n' \"$code\" > \"${exit_file}.tmp\" && mv -f -- \"${exit_file}.tmp\" \"$exit_file\"")
        .arg("loom-run")
        .arg(&exit)
        .args(argv)
        .stdin(Stdio::null())
        .stdout(Stdio::from(output.try_clone().map_err(|err| err.to_string())?))
        .stderr(Stdio::from(output))
        .process_group(0)
        .spawn()
        .map_err(|err| format!("cannot start run {name:?}: {err}"))?;
    let record =
        json!({"name":name,"pid":child.id(),"exit_file":exit,"log_file":log,"command":argv});
    let temp = dir.join("run.json.tmp");
    let save = || -> Result<(), String> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)
            .map_err(|err| err.to_string())?;
        use std::io::Write;
        writeln!(file, "{record}").map_err(|err| err.to_string())?;
        fs::rename(&temp, dir.join("run.json")).map_err(|err| err.to_string())
    };
    save().map_err(|err| format!("run started (pid {}, exit {}, log {}) but its record could not be saved: {err}; do not retry", child.id(), exit.display(), log.display()))?;
    // Reap quick commands; dropping Child never kills a still-running command.
    let _ = child.try_wait();
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_names_cannot_escape_the_callers_directory() {
        for name in ["", ".", "../other", "/tmp/other", "a:b", "a b", "-x", "é"] {
            assert!(component(name).is_err(), "{name:?}");
        }
        assert!(component("build-42_ok").is_ok());
    }
}
