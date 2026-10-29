//! Plugin readers for agent waits: when to read each live wait's source, and running one read.
//!
//! The schedule is runtime state, rebuilt from the live waits after a load; what a read found
//! is kept on the wait itself.

use std::collections::{BTreeSet, HashMap};
use std::time::{Duration, Instant};

/// One read may take this long before its process group is killed and it counts as failed.
pub(crate) const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// Failed reads in a row that fire a wait as `failed: source unreachable`.
pub(crate) const READ_ERROR_LIMIT: u32 = 3;
/// Reads running at once; more wait for the next tick.
pub(crate) const READS_IN_FLIGHT_LIMIT: usize = 16;
/// The liveness window of a wait whose kind no plugin reads any more.
pub(crate) const ORPHANED_LIVENESS_WINDOW: Duration = Duration::from_secs(15 * 60);
/// Bounds on a manifest's `interval`.
pub(crate) const MIN_READ_INTERVAL_MS: u64 = 10_000;
pub(crate) const MAX_READ_INTERVAL_MS: u64 = 86_400_000;
/// Output kept from a read; the answer is one short line.
const READ_OUTPUT_LIMIT: u64 = 64 * 1024;
/// When a read could not start for want of a slot, try again this soon.
const BUSY_RETRY: Duration = Duration::from_secs(5);

/// What one run of a reader said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReadOutcome {
    /// It reached the source, which is in this state.
    Read(String),
    /// It could not reach the source this time (exit 1, a timeout, unreadable output).
    Failed(String),
    /// It cannot read this source at all (exit 2): retrying will not help.
    Refused(String),
}

/// No read lands for this long, and the wait fires as `failed: reader dead`.
pub(crate) fn liveness_window(interval: Duration) -> Duration {
    (interval * 3).max(interval + READ_TIMEOUT)
}

/// Reads a reader's exit code and output per the read contract: `{ok, state, error}` on the
/// last line of stdout, exit 0 a read, 2 a source it cannot read, anything else a failed read.
pub(crate) fn parse_read_output(exit_code: Option<i32>, stdout: &str, stderr: &str) -> ReadOutcome {
    let answer = stdout
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .and_then(|line| serde_json::from_str::<serde_json::Value>(line).ok());
    let field = |key: &str| {
        answer
            .as_ref()
            .and_then(|answer| answer.get(key))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let complaint = |fallback: String| {
        field("error")
            .or_else(|| {
                stderr
                    .lines()
                    .rev()
                    .map(str::trim)
                    .find(|line| !line.is_empty())
                    .map(str::to_owned)
            })
            .unwrap_or(fallback)
    };
    match exit_code {
        Some(0) => {
            let ok = answer
                .as_ref()
                .and_then(|answer| answer.get("ok"))
                .and_then(serde_json::Value::as_bool);
            match (ok, field("state")) {
                (Some(true), Some(state)) => ReadOutcome::Read(state),
                _ => ReadOutcome::Failed(complaint("the reader printed no state".to_owned())),
            }
        }
        Some(2) => ReadOutcome::Refused(complaint("the reader cannot read this source".to_owned())),
        Some(code) => ReadOutcome::Failed(complaint(format!("the reader exited with {code}"))),
        None => ReadOutcome::Failed(complaint("the reader was killed".to_owned())),
    }
}

#[derive(Debug, Clone, Copy)]
struct Slot {
    due: Instant,
    last_landed: Instant,
    in_flight: bool,
}

/// When each live reader wait is next looked at. A look either starts a read, or finds that none
/// has landed within the liveness window.
#[derive(Debug, Default)]
pub(crate) struct ReaderSchedule {
    due: BTreeSet<(Instant, u64)>,
    slots: HashMap<u64, Slot>,
    in_flight: usize,
    /// Reads a test asked for, as (wait id, argv), in place of running them.
    #[cfg(test)]
    pub(crate) started_for_test: Vec<(u64, Vec<String>)>,
}

impl ReaderSchedule {
    /// Starts watching a wait: the first look is due now, and its liveness window starts now.
    pub(crate) fn add(&mut self, wait_id: u64, now: Instant) {
        self.remove(wait_id);
        self.slots.insert(
            wait_id,
            Slot {
                due: now,
                last_landed: now,
                in_flight: false,
            },
        );
        self.due.insert((now, wait_id));
    }

    pub(crate) fn remove(&mut self, wait_id: u64) {
        if let Some(slot) = self.slots.remove(&wait_id) {
            self.due.remove(&(slot.due, wait_id));
            if slot.in_flight {
                self.in_flight -= 1;
            }
        }
    }

    pub(crate) fn next_due(&self) -> Option<Instant> {
        self.due.first().map(|&(due, _)| due)
    }

    /// The waits whose look is due, earliest first. Each stays in the schedule, due again only
    /// once `reschedule` says when.
    pub(crate) fn take_due(&mut self, now: Instant) -> Vec<u64> {
        let due: Vec<(Instant, u64)> = self.due.range(..=(now, u64::MAX)).copied().collect();
        for entry in &due {
            self.due.remove(entry);
        }
        due.into_iter().map(|(_, wait_id)| wait_id).collect()
    }

    pub(crate) fn reschedule(&mut self, wait_id: u64, at: Instant) {
        if let Some(slot) = self.slots.get_mut(&wait_id) {
            self.due.remove(&(slot.due, wait_id));
            slot.due = at;
            self.due.insert((at, wait_id));
        }
    }

    /// Reschedules a look that could not start a read for want of a slot.
    pub(crate) fn retry_soon(&mut self, wait_id: u64, now: Instant) {
        self.reschedule(wait_id, now + BUSY_RETRY);
    }

    pub(crate) fn last_landed(&self, wait_id: u64) -> Option<Instant> {
        self.slots.get(&wait_id).map(|slot| slot.last_landed)
    }

    pub(crate) fn in_flight(&self, wait_id: u64) -> bool {
        self.slots.get(&wait_id).is_some_and(|slot| slot.in_flight)
    }

    pub(crate) fn has_room(&self) -> bool {
        self.in_flight < READS_IN_FLIGHT_LIMIT
    }

    pub(crate) fn started(&mut self, wait_id: u64) {
        if let Some(slot) = self.slots.get_mut(&wait_id) {
            if !slot.in_flight {
                slot.in_flight = true;
                self.in_flight += 1;
            }
        }
    }

    /// A read came back; false when the wait was no longer watched.
    pub(crate) fn landed(&mut self, wait_id: u64, now: Instant) -> bool {
        let Some(slot) = self.slots.get_mut(&wait_id) else {
            return false;
        };
        if slot.in_flight {
            slot.in_flight = false;
            self.in_flight -= 1;
        }
        slot.last_landed = now;
        true
    }

    #[cfg(test)]
    pub(crate) fn watched(&self) -> Vec<u64> {
        let mut ids: Vec<u64> = self.slots.keys().copied().collect();
        ids.sort_unstable();
        ids
    }
}

/// A read to run off the main loop: the reader's argv with `<kind>:<target>` last.
pub(crate) struct ReadRequest {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: std::path::PathBuf,
    pub env: Vec<(String, String)>,
    pub timeout: Duration,
}

/// Runs one read in its own process group (a kill-on-close job on Windows), so a timeout or a
/// server going away takes the reader's descendants with it.
pub(crate) async fn run_read(request: ReadRequest) -> ReadOutcome {
    let mut command = crate::plugin_command::command_for_argv_in_dir(
        &request.program,
        &request.args,
        &request.cwd,
    );
    command
        .envs(request.env)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    crate::platform::configure_status_command(&mut command);
    let mut command = tokio::process::Command::from(command);
    command.kill_on_drop(true);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => return ReadOutcome::Failed(format!("could not start the reader: {err}")),
    };
    let mut process_group = match crate::platform::StatusCommandGuard::new(&child) {
        Ok(guard) => guard,
        Err(err) => return ReadOutcome::Failed(format!("could not supervise the reader: {err}")),
    };
    let stdout = tokio::spawn(read_capped(child.stdout.take()));
    let stderr = tokio::spawn(read_capped(child.stderr.take()));
    let status = tokio::time::timeout(request.timeout, child.wait()).await;
    // The read ends when the reader does; descendants still holding its pipes go with it.
    process_group.terminate();
    let stdout = stdout.await.unwrap_or_default();
    let stderr = stderr.await.unwrap_or_default();
    match status {
        Ok(Ok(status)) => parse_read_output(status.code(), &stdout, &stderr),
        Ok(Err(err)) => ReadOutcome::Failed(format!("the reader failed: {err}")),
        Err(_) => ReadOutcome::Failed(format!(
            "the reader timed out after {}s",
            request.timeout.as_secs()
        )),
    }
}

async fn read_capped(pipe: Option<impl tokio::io::AsyncRead + Unpin>) -> String {
    use tokio::io::AsyncReadExt;
    let Some(pipe) = pipe else {
        return String::new();
    };
    let mut output = Vec::new();
    let _ = pipe.take(READ_OUTPUT_LIMIT).read_to_end(&mut output).await;
    String::from_utf8_lossy(&output).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_read_contract_maps_exit_codes_and_answers() {
        assert_eq!(
            parse_read_output(
                Some(0),
                "noise\n{\"ok\":true,\"ref\":\"pr:o/r#1\",\"state\":\"merged\",\"error\":null}\n",
                ""
            ),
            ReadOutcome::Read("merged".into())
        );
        assert_eq!(
            parse_read_output(
                Some(0),
                "{\"ok\":false,\"error\":\"gh: not logged in\"}",
                ""
            ),
            ReadOutcome::Failed("gh: not logged in".into())
        );
        assert_eq!(
            parse_read_output(Some(0), "not json", ""),
            ReadOutcome::Failed("the reader printed no state".into())
        );
        assert_eq!(
            parse_read_output(Some(1), "{\"ok\":false,\"error\":\"timeout\"}", "ignored"),
            ReadOutcome::Failed("timeout".into())
        );
        assert_eq!(
            parse_read_output(Some(1), "", "glab: 502\n"),
            ReadOutcome::Failed("glab: 502".into())
        );
        assert_eq!(
            parse_read_output(Some(7), "", ""),
            ReadOutcome::Failed("the reader exited with 7".into())
        );
        assert_eq!(
            parse_read_output(
                Some(2),
                "{\"ok\":false,\"error\":\"node: is not a source\"}",
                ""
            ),
            ReadOutcome::Refused("node: is not a source".into())
        );
        assert_eq!(
            parse_read_output(None, "", ""),
            ReadOutcome::Failed("the reader was killed".into())
        );
    }

    #[test]
    fn the_liveness_window_outlasts_a_slow_read() {
        assert_eq!(
            liveness_window(Duration::from_secs(300)),
            Duration::from_secs(900)
        );
        assert_eq!(
            liveness_window(Duration::from_secs(10)),
            Duration::from_secs(40)
        );
    }

    #[test]
    fn the_schedule_hands_out_due_looks_once_and_counts_reads_in_flight() {
        let start = Instant::now();
        let mut schedule = ReaderSchedule::default();
        schedule.add(1, start);
        schedule.add(2, start + Duration::from_secs(5));
        assert_eq!(schedule.next_due(), Some(start));
        assert_eq!(schedule.take_due(start), vec![1]);
        assert!(
            schedule.take_due(start).is_empty(),
            "a look is handed out once"
        );

        schedule.started(1);
        schedule.reschedule(1, start + Duration::from_secs(60));
        assert!(schedule.in_flight(1));
        assert_eq!(schedule.next_due(), Some(start + Duration::from_secs(5)));

        let later = start + Duration::from_secs(3);
        assert!(schedule.landed(1, later));
        assert!(!schedule.in_flight(1));
        assert_eq!(schedule.last_landed(1), Some(later));
        assert!(schedule.has_room());

        schedule.started(2);
        schedule.remove(2);
        assert!(!schedule.landed(2, later), "a removed wait is not watched");
        assert_eq!(schedule.in_flight, 0);
        assert_eq!(schedule.watched(), vec![1]);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_read_runs_with_its_source_last_and_a_slow_one_is_killed() {
        let dir = std::env::temp_dir().join(format!("herdr-reader-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let request = |script: &str, timeout| ReadRequest {
            program: "sh".into(),
            args: vec![
                "-c".into(),
                script.into(),
                "reader".into(),
                "pr:o/r#1".into(),
            ],
            cwd: dir.clone(),
            env: vec![("STATE".into(), "merged".into())],
            timeout,
        };

        let started = Instant::now();
        let read = run_read(request(
            r#"sleep 30 & printf '{"ok":true,"ref":"%s","state":"%s"}\n' "$1" "$STATE""#,
            Duration::from_secs(10),
        ))
        .await;
        assert_eq!(read, ReadOutcome::Read("merged".into()));
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "a descendant holding the pipe does not hold the read"
        );

        let pid_file = dir.join("child.pid");
        let started = Instant::now();
        let slow = run_read(request(
            &format!("sleep 30 & echo $! > {}; wait", pid_file.display()),
            Duration::from_millis(300),
        ))
        .await;
        assert_eq!(
            slow,
            ReadOutcome::Failed("the reader timed out after 0s".into())
        );
        assert!(started.elapsed() < Duration::from_secs(10));
        let child = std::fs::read_to_string(&pid_file).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let alive = || {
            std::process::Command::new("kill")
                .args(["-0", child.trim()])
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
        };
        while alive() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!alive(), "the reader's descendants die with it");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
