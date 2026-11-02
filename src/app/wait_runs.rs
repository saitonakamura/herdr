//! Run waits: a command the waiter started itself (`loom run`), watched through its pid and the
//! exit file it leaves behind. herdr never starts, signals or reads the output of the command.

use std::collections::{BTreeSet, HashMap};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::app::waits::WaitOutcome;

/// The source kind of a wait on a run; its target is `<pid>:<absolute exit file>`.
pub(crate) const RUN_SOURCE: &str = "run";
/// What a run wait waits for when `until` is left out: the run ending, however it ends.
pub(crate) const RUN_UNTIL_EXIT: &str = "exit";
/// The run ending with exit code 0.
pub(crate) const RUN_UNTIL_SUCCESS: &str = "success";
/// How often a live run wait looks at its exit file and pid.
pub(crate) const RUN_CHECK_INTERVAL: Duration = Duration::from_secs(2);
/// An exit file holds a decimal exit code; anything longer is not one.
const EXIT_FILE_LIMIT: u64 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunUntil {
    Exit,
    Success,
}

pub(crate) fn run_until(until: Option<&str>) -> Option<RunUntil> {
    match until.unwrap_or(RUN_UNTIL_EXIT) {
        RUN_UNTIL_EXIT => Some(RunUntil::Exit),
        RUN_UNTIL_SUCCESS => Some(RunUntil::Success),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunTarget {
    pub pid: u32,
    pub exit_file: PathBuf,
}

/// `<pid>:<exit file>`; the path must be absolute, since the server's directory is not the waiter's.
pub(crate) fn parse_run_target(target: &str) -> Result<RunTarget, &'static str> {
    let (pid, exit_file) = target
        .split_once(':')
        .ok_or("a run wait's target is PID:EXIT_FILE")?;
    let pid = pid
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|pid| *pid > 0)
        .ok_or("a run wait's pid must be a positive number")?;
    let exit_file = PathBuf::from(exit_file);
    if !exit_file.is_absolute() {
        return Err("a run wait's exit file must be an absolute path");
    }
    Ok(RunTarget { pid, exit_file })
}

/// What one look at a run found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RunCheck {
    Running,
    Exited(i32),
    /// The pid is gone and left no exit file.
    Vanished,
    /// The exit file is there but does not hold an exit code.
    Unreadable(String),
}

/// Looks at the exit file, then the pid, then the exit file once more: a run that writes its exit
/// file as it ends may do so between the first two.
pub(crate) fn check_run(target: &RunTarget, alive: impl Fn(u32) -> bool) -> RunCheck {
    match read_exit_file(&target.exit_file) {
        Some(check) => check,
        None if alive(target.pid) => RunCheck::Running,
        None => read_exit_file(&target.exit_file).unwrap_or(RunCheck::Vanished),
    }
}

/// Whether `pid` is still the process the wait was declared on.
pub(crate) fn run_alive(pid: u32, start_token: Option<u64>) -> bool {
    if !crate::platform::process_exists(pid) {
        return false;
    }
    // A pid whose start cannot be read now is taken at its word.
    start_token.is_none_or(|token| {
        crate::platform::process_start_token(pid).is_none_or(|now| now == token)
    })
}

/// The exit code in `path`, or None while there is none yet. An empty file is one still being
/// written.
fn read_exit_file(path: &Path) -> Option<RunCheck> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return None,
        Err(err) => return Some(RunCheck::Unreadable(err.to_string())),
    };
    // Opening a FIFO for reading would block the server until someone writes to it.
    if !metadata.is_file() {
        return Some(RunCheck::Unreadable("it is not a regular file".to_owned()));
    }
    let mut contents = String::new();
    if let Err(err) = std::fs::File::open(path)
        .and_then(|file| file.take(EXIT_FILE_LIMIT).read_to_string(&mut contents))
    {
        return Some(RunCheck::Unreadable(err.to_string()));
    }
    let contents = contents.trim();
    if contents.is_empty() {
        return None;
    }
    Some(match contents.parse::<i32>() {
        Ok(code) => RunCheck::Exited(code),
        Err(_) => RunCheck::Unreadable("it does not hold an exit code".to_owned()),
    })
}

/// What a look at a run does to a wait on it.
pub(crate) fn run_outcome(pid: u32, until: RunUntil, check: &RunCheck) -> Option<WaitOutcome> {
    match check {
        RunCheck::Running => None,
        RunCheck::Exited(code) => {
            let reason = format!("run {pid} exited {code}");
            Some(match until {
                RunUntil::Exit => WaitOutcome::Satisfied(reason),
                RunUntil::Success if *code == 0 => WaitOutcome::Satisfied(reason),
                RunUntil::Success => WaitOutcome::Fired(reason),
            })
        }
        RunCheck::Vanished => Some(WaitOutcome::Fired(format!(
            "failed: run {pid} ended without an exit file"
        ))),
        RunCheck::Unreadable(why) => Some(WaitOutcome::Fired(format!(
            "failed: run {pid}'s exit file is unreadable: {why}"
        ))),
    }
}

/// How a look at a run reads in a wait's message.
pub(crate) fn run_check_line(check: &RunCheck) -> String {
    match check {
        RunCheck::Running => "running".to_owned(),
        RunCheck::Exited(code) => format!("exited {code}"),
        RunCheck::Vanished => "gone, without an exit file".to_owned(),
        RunCheck::Unreadable(why) => format!("exit file unreadable: {why}"),
    }
}

/// When each live run wait is next looked at.
#[derive(Debug, Default)]
pub(crate) struct RunSchedule {
    due: BTreeSet<(Instant, u64)>,
    at: HashMap<u64, Instant>,
}

impl RunSchedule {
    pub(crate) fn add(&mut self, wait_id: u64, at: Instant) {
        self.remove(wait_id);
        self.at.insert(wait_id, at);
        self.due.insert((at, wait_id));
    }

    pub(crate) fn remove(&mut self, wait_id: u64) {
        if let Some(at) = self.at.remove(&wait_id) {
            self.due.remove(&(at, wait_id));
        }
    }

    pub(crate) fn next_due(&self) -> Option<Instant> {
        self.due.first().map(|&(due, _)| due)
    }

    /// The waits whose look is due, earliest first, taken out of the schedule.
    pub(crate) fn take_due(&mut self, now: Instant) -> Vec<u64> {
        let due: Vec<(Instant, u64)> = self.due.range(..=(now, u64::MAX)).copied().collect();
        for (at, wait_id) in &due {
            self.due.remove(&(*at, *wait_id));
            self.at.remove(wait_id);
        }
        due.into_iter().map(|(_, wait_id)| wait_id).collect()
    }

    #[cfg(test)]
    pub(crate) fn watched(&self) -> Vec<u64> {
        let mut ids: Vec<u64> = self.at.keys().copied().collect();
        ids.sort_unstable();
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "herdr-wait-runs-{name}-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_run_target_is_a_pid_and_an_absolute_exit_file() {
        let exit_file = std::env::temp_dir().join("run").join("exit");
        let target = format!("4242:{}", exit_file.display());
        assert_eq!(
            parse_run_target(&target),
            Ok(RunTarget {
                pid: 4242,
                exit_file
            })
        );
        assert!(parse_run_target("4242").is_err());
        assert!(parse_run_target("0:/tmp/exit").is_err());
        assert!(parse_run_target("x:/tmp/exit").is_err());
        assert!(parse_run_target("4242:runs/exit").is_err());
    }

    #[test]
    fn until_is_exit_by_default_or_success() {
        assert_eq!(run_until(None), Some(RunUntil::Exit));
        assert_eq!(run_until(Some("exit")), Some(RunUntil::Exit));
        assert_eq!(run_until(Some("success")), Some(RunUntil::Success));
        assert_eq!(run_until(Some("merged")), None);
    }

    #[test]
    fn a_run_is_read_from_its_exit_file_and_then_its_pid() {
        let dir = temp_dir("check");
        let target = RunTarget {
            pid: 4242,
            exit_file: dir.join("exit"),
        };
        assert_eq!(check_run(&target, |_| true), RunCheck::Running);
        assert_eq!(check_run(&target, |_| false), RunCheck::Vanished);

        std::fs::write(&target.exit_file, "").unwrap();
        assert_eq!(
            check_run(&target, |_| true),
            RunCheck::Running,
            "an empty exit file is still being written"
        );
        std::fs::write(&target.exit_file, "3\n").unwrap();
        assert_eq!(check_run(&target, |_| true), RunCheck::Exited(3));
        assert_eq!(check_run(&target, |_| false), RunCheck::Exited(3));
        std::fs::write(&target.exit_file, "done").unwrap();
        assert!(matches!(
            check_run(&target, |_| false),
            RunCheck::Unreadable(_)
        ));

        let dir_as_file = RunTarget {
            pid: 4242,
            exit_file: dir.clone(),
        };
        assert!(matches!(
            check_run(&dir_as_file, |_| true),
            RunCheck::Unreadable(_)
        ));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_exit_file_written_as_the_run_ends_is_not_taken_for_a_vanished_run() {
        let dir = temp_dir("race");
        let target = RunTarget {
            pid: 4242,
            exit_file: dir.join("exit"),
        };
        let check = check_run(&target, |_| {
            std::fs::write(&target.exit_file, "0").unwrap();
            false
        });
        assert_eq!(check, RunCheck::Exited(0));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn this_process_is_alive_and_a_different_start_token_is_not() {
        let pid = std::process::id();
        assert!(run_alive(pid, None));
        if let Some(token) = crate::platform::process_start_token(pid) {
            assert!(run_alive(pid, Some(token)));
            assert!(!run_alive(pid, Some(token.wrapping_add(1))));
        }
    }

    #[test]
    fn until_decides_what_an_exit_does() {
        let exited = |code| RunCheck::Exited(code);
        assert_eq!(
            run_outcome(7, RunUntil::Exit, &exited(1)),
            Some(WaitOutcome::Satisfied("run 7 exited 1".into()))
        );
        assert_eq!(
            run_outcome(7, RunUntil::Success, &exited(0)),
            Some(WaitOutcome::Satisfied("run 7 exited 0".into()))
        );
        assert_eq!(
            run_outcome(7, RunUntil::Success, &exited(2)),
            Some(WaitOutcome::Fired("run 7 exited 2".into()))
        );
        assert_eq!(
            run_outcome(7, RunUntil::Exit, &RunCheck::Vanished),
            Some(WaitOutcome::Fired(
                "failed: run 7 ended without an exit file".into()
            ))
        );
        assert_eq!(run_outcome(7, RunUntil::Exit, &RunCheck::Running), None);
    }

    #[test]
    fn the_schedule_hands_out_due_looks_once() {
        let now = Instant::now();
        let mut schedule = RunSchedule::default();
        schedule.add(2, now + Duration::from_secs(2));
        schedule.add(1, now);
        assert_eq!(schedule.next_due(), Some(now));
        assert_eq!(schedule.take_due(now), vec![1]);
        assert_eq!(schedule.watched(), vec![2]);
        schedule.add(2, now + Duration::from_secs(5));
        assert_eq!(
            schedule.take_due(now + Duration::from_secs(3)),
            Vec::<u64>::new()
        );
        schedule.remove(2);
        assert_eq!(schedule.next_due(), None);
    }
}
