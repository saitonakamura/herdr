//! Other processes' exe, argv and environment for teardown; empty off Linux and macOS.

use std::path::PathBuf;

pub(super) fn all_pids() -> std::io::Result<Vec<u32>> {
    let own_pid = std::process::id();
    let mut pids = imp::all_pids()?;
    pids.retain(|pid| *pid != own_pid && *pid > 1);
    Ok(pids)
}

pub(super) fn exe_path(pid: u32) -> Option<PathBuf> {
    imp::exe_path(pid)
}

pub(super) fn argv(pid: u32) -> std::io::Result<Vec<String>> {
    imp::argv(pid)
}

/// `KEY=value` entries; a process owned by another user is an error.
pub(super) fn environ(pid: u32) -> std::io::Result<Vec<String>> {
    imp::environ(pid)
}

#[cfg(target_os = "linux")]
mod imp {
    use std::fs;
    use std::path::PathBuf;

    pub(super) fn all_pids() -> std::io::Result<Vec<u32>> {
        let entries = match fs::read_dir("/proc") {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(err),
        };
        let mut pids = Vec::new();
        for entry in entries {
            if let Some(pid) = entry?
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<u32>().ok())
            {
                pids.push(pid);
            }
        }
        Ok(pids)
    }

    pub(super) fn exe_path(pid: u32) -> Option<PathBuf> {
        fs::read_link(format!("/proc/{pid}/exe")).ok()
    }

    pub(super) fn argv(pid: u32) -> std::io::Result<Vec<String>> {
        Ok(split_nul(&fs::read(format!("/proc/{pid}/cmdline"))?))
    }

    pub(super) fn environ(pid: u32) -> std::io::Result<Vec<String>> {
        Ok(split_nul(&fs::read(format!("/proc/{pid}/environ"))?))
    }

    fn split_nul(bytes: &[u8]) -> Vec<String> {
        bytes
            .split(|byte| *byte == 0)
            .filter(|chunk| !chunk.is_empty())
            .map(|chunk| String::from_utf8_lossy(chunk).to_string())
            .collect()
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::path::PathBuf;

    pub(super) fn all_pids() -> std::io::Result<Vec<u32>> {
        let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
        if count < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // Headroom for processes started between the two calls.
        let mut pids = vec![0 as libc::pid_t; count as usize + 256];
        let bytes = (pids.len() * std::mem::size_of::<libc::pid_t>()) as libc::c_int;
        let count = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes) };
        if count < 0 {
            return Err(std::io::Error::last_os_error());
        }
        pids.truncate(count as usize);
        Ok(pids
            .into_iter()
            .filter_map(|pid| u32::try_from(pid).ok())
            .collect())
    }

    pub(super) fn exe_path(pid: u32) -> Option<PathBuf> {
        let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
        let len = unsafe {
            libc::proc_pidpath(
                pid as libc::c_int,
                buf.as_mut_ptr().cast(),
                buf.len() as u32,
            )
        };
        if len <= 0 {
            return None;
        }
        buf.truncate(len as usize);
        Some(PathBuf::from(String::from_utf8_lossy(&buf).into_owned()))
    }

    pub(super) fn argv(pid: u32) -> std::io::Result<Vec<String>> {
        args_and_env(pid).map(|(argv, _)| argv)
    }

    pub(super) fn environ(pid: u32) -> std::io::Result<Vec<String>> {
        args_and_env(pid).map(|(_, env)| env)
    }

    // KERN_PROCARGS2 is argc, the exec path, NUL padding, argv, then env up to an empty string.
    fn args_and_env(pid: u32) -> std::io::Result<(Vec<String>, Vec<String>)> {
        let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid as libc::c_int];
        let mut size: libc::size_t = 0;
        let rc = unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                3,
                std::ptr::null_mut(),
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        };
        if rc != 0 {
            return Err(std::io::Error::last_os_error());
        }
        let mut buf = vec![0u8; size];
        let rc = unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                3,
                buf.as_mut_ptr().cast(),
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        };
        if rc != 0 {
            return Err(std::io::Error::last_os_error());
        }
        buf.truncate(size);
        parse_procargs2(&buf).ok_or_else(|| std::io::Error::other("malformed KERN_PROCARGS2"))
    }

    fn parse_procargs2(buf: &[u8]) -> Option<(Vec<String>, Vec<String>)> {
        let argc = i32::from_ne_bytes(buf.get(..4)?.try_into().ok()?);
        let mut rest = &buf[4..];
        let exec_path_end = rest.iter().position(|byte| *byte == 0)?;
        rest = &rest[exec_path_end..];
        let padding = rest
            .iter()
            .position(|byte| *byte != 0)
            .unwrap_or(rest.len());
        rest = &rest[padding..];
        let mut strings = rest.split(|byte| *byte == 0);
        let argv = strings
            .by_ref()
            .take(usize::try_from(argc).ok()?)
            .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
            .collect();
        let env = strings
            .take_while(|chunk| !chunk.is_empty())
            .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
            .collect();
        Some((argv, env))
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod imp {
    use std::path::PathBuf;

    pub(super) fn all_pids() -> std::io::Result<Vec<u32>> {
        Ok(Vec::new())
    }

    pub(super) fn exe_path(_pid: u32) -> Option<PathBuf> {
        None
    }

    pub(super) fn argv(_pid: u32) -> std::io::Result<Vec<String>> {
        Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
    }

    pub(super) fn environ(_pid: u32) -> std::io::Result<Vec<String>> {
        Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_table_sees_a_child_with_its_args_and_env() {
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .env("HERDR_TEST_PROCESS_TABLE_MARKER", "present")
            .spawn()
            .expect("spawn sleep");
        let pid = child.id();
        // Until the child execs, the table shows the test binary's args.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut args = Vec::new();
        while std::time::Instant::now() < deadline {
            args = argv(pid).unwrap_or_default();
            if args.last().map(String::as_str) == Some("30") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let found = all_pids().expect("list pids").contains(&pid);
        let env = environ(pid);
        let _ = child.kill();
        let _ = child.wait();

        assert!(found, "child pid {pid} missing from the process table");
        assert_eq!(args.last().map(String::as_str), Some("30"), "{args:?}");
        assert!(env
            .expect("environ")
            .iter()
            .any(|entry| entry == "HERDR_TEST_PROCESS_TABLE_MARKER=present"));
    }
}
