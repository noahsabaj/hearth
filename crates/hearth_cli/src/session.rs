//! Sessions: apps started with `hearth run --detach`, recorded on disk so later commands can find
//! them, read their logs and stop them.
//!
//! Records live in the CLI's state directory, which `HEARTH_CLI_HOME` overrides.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::output::{CliError, Exit};

/// An app started with `hearth run --detach`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Session {
    /// The name other commands use for it.
    pub name: String,
    /// The process id.
    pub pid: u32,
    /// The HRP port it was asked to listen on.
    pub port: u16,
    /// The directory of the package it was built from.
    pub project: PathBuf,
    /// The package name.
    pub package: String,
    /// What was run, such as `bin my_game` or `example server`.
    pub target: String,
    /// The executable.
    pub executable: PathBuf,
    /// Arguments passed to the app.
    pub args: Vec<String>,
    /// The file its stdout and stderr go to.
    pub log: PathBuf,
    /// When it started, in milliseconds since the Unix epoch.
    pub started_at: u64,
}

impl Session {
    /// Whether the process is still running.
    ///
    /// The process name is compared with the executable's, so a recycled process id doesn't
    /// count as the app.
    pub fn is_running(&self) -> bool {
        let Some(name) = process_name(self.pid) else {
            return false;
        };
        let expected = self
            .executable
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let actual = Path::new(&name)
            .file_name()
            .map(|file| file.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let actual = actual.strip_suffix(".exe").unwrap_or(&actual);
        // Linux truncates process names to 15 bytes.
        !expected.is_empty()
            && (actual == expected || (actual.len() >= 15 && expected.starts_with(actual)))
    }

    /// Writes the record.
    pub fn save(&self) -> Result<(), CliError> {
        let dir = sessions_dir();
        std::fs::create_dir_all(&dir).map_err(|error| CliError::io(dir.display(), error))?;
        let path = record_path(&self.name);
        let text = serde_json::to_string_pretty(self)
            .map_err(|error| CliError::internal(error.to_string()))?;
        std::fs::write(&path, text).map_err(|error| CliError::io(path.display(), error))
    }

    /// Deletes the record, keeping the log.
    pub fn forget(&self) {
        let _ = std::fs::remove_file(record_path(&self.name));
    }

    /// Seconds since the session started.
    pub fn uptime_secs(&self) -> u64 {
        now_millis().saturating_sub(self.started_at) / 1000
    }
}

/// The CLI's state directory.
pub fn home() -> PathBuf {
    if let Some(dir) = std::env::var_os("HEARTH_CLI_HOME").filter(|dir| !dir.is_empty()) {
        return PathBuf::from(dir);
    }
    let var = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
    let base = if cfg!(windows) {
        var("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        var("HOME").map(|home| {
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
        })
    } else {
        var("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| var("HOME").map(|home| PathBuf::from(home).join(".local").join("state")))
    };
    base.unwrap_or_else(std::env::temp_dir).join("hearth-cli")
}

fn sessions_dir() -> PathBuf {
    home().join("sessions")
}

fn record_path(name: &str) -> PathBuf {
    sessions_dir().join(format!("{name}.json"))
}

/// The directory session logs are written to.
pub fn logs_dir() -> PathBuf {
    home().join("logs")
}

/// Every recorded session, oldest first, whether or not it's still running.
pub fn list() -> Vec<Session> {
    let Ok(entries) = std::fs::read_dir(sessions_dir()) else {
        return Vec::new();
    };
    let mut sessions: Vec<Session> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
        .filter_map(|text| serde_json::from_str(&text).ok())
        .collect();
    sessions.sort_by_key(|session| session.started_at);
    sessions
}

/// The session called `name`.
pub fn find(name: &str) -> Option<Session> {
    let text = std::fs::read_to_string(record_path(&sanitize(name))).ok()?;
    serde_json::from_str(&text).ok()
}

/// Turns `text` into a name that is safe as a file name: letters, digits, `-` and `_`.
pub fn sanitize(text: &str) -> String {
    let name: String = text
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let name = name.trim_matches('-');
    if name.is_empty() {
        "app".to_string()
    } else {
        name.to_string()
    }
}

/// A name based on `base` that no running session uses. A finished session's name is reused.
pub fn unique_name(base: &str) -> String {
    let base = sanitize(base);
    let taken = |name: &str| find(name).is_some_and(|session| session.is_running());
    if !taken(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|name| !taken(name))
        .unwrap_or(base)
}

/// Milliseconds since the Unix epoch.
pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}

fn quiet_command(program: &str) -> Command {
    let mut command = Command::new(program);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

/// The name of the process with id `pid`, if one is running.
pub fn process_name(pid: u32) -> Option<String> {
    #[cfg(windows)]
    {
        windows_process_name(pid)
    }
    #[cfg(not(windows))]
    {
        let output = quiet_command("ps")
            .args(["-p", &pid.to_string(), "-o", "comm="])
            .output()
            .ok()?;
        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (output.status.success() && !name.is_empty()).then_some(name)
    }
}

/// The path of the executable of the process with id `pid`, if one is running.
///
/// Asking Windows directly takes microseconds, where running `tasklist` takes a fifth of a second,
/// and every command that finds its app checks the sessions this way.
#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "The standard library can't look up another process by its id."
)]
fn windows_process_name(pid: u32) -> Option<String> {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStringExt;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn GetExitCodeProcess(process: *mut c_void, code: *mut u32) -> i32;
        fn QueryFullProcessImageNameW(
            process: *mut c_void,
            flags: u32,
            name: *mut u16,
            size: *mut u32,
        ) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }

    // SAFETY: `OpenProcess` takes no pointers, and returns null when there is no such process.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return None;
    }
    let mut code = 0;
    let mut name = [0u16; 1024];
    let mut size = name.len() as u32;
    // SAFETY: `process` is the handle opened above, and each pointer is to a local that outlives
    // the call, with `size` holding the length of `name`.
    let (exited, named) = unsafe {
        let exited = GetExitCodeProcess(process, &mut code) == 0 || code != STILL_ACTIVE;
        let named = QueryFullProcessImageNameW(process, 0, name.as_mut_ptr(), &mut size) != 0;
        CloseHandle(process);
        (exited, named)
    };
    if exited || !named {
        return None;
    }
    let name = std::ffi::OsString::from_wide(&name[..size as usize]);
    Some(name.to_string_lossy().into_owned())
}

/// Asks the process to end, or ends it right away when `force` is set.
///
/// Without `force`, Windows closes the app's windows and other platforms send `SIGTERM`; Hearth
/// apps exit cleanly on both.
pub fn terminate(pid: u32, force: bool) -> Result<(), CliError> {
    let output = if cfg!(windows) {
        let mut command = quiet_command("taskkill");
        command.args(["/PID", &pid.to_string()]);
        if force {
            command.arg("/F");
        }
        command.output()
    } else {
        quiet_command("kill")
            .args([if force { "-KILL" } else { "-TERM" }, &pid.to_string()])
            .output()
    }
    .map_err(|error| CliError::io(format!("could not stop process {pid}"), error))?;
    if output.status.success() {
        return Ok(());
    }
    let reason = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(CliError::new(
        "STOP_FAILED",
        Exit::Error,
        format!("Could not stop process {pid}: {reason}"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_names() {
        assert_eq!(sanitize("my game!"), "my-game");
        assert_eq!(sanitize("../x"), "x");
        assert_eq!(sanitize("???"), "app");
        assert_eq!(sanitize("server_2"), "server_2");
    }

    #[test]
    fn finds_this_process() {
        let name = process_name(std::process::id()).expect("this process is running");
        assert!(!name.is_empty());
    }
}
