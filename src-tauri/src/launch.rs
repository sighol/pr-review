//! Running in the background, like `code` or `zeditor`: the command you type starts a copy of
//! itself, relays that copy's startup output (progress and errors) to your terminal until the
//! window is up, and then exits so the terminal is free. `--wait` and `--debug` skip all this.

/// Set in the background copy, so it knows it must report back and detach.
#[cfg(unix)]
const CHILD_ENV: &str = "PR_REVIEW_BACKGROUND_COPY";
/// The background copy prints this line on stderr when the window is up.
#[cfg(unix)]
const READY: &str = "@@pr-review-window-ready@@";

pub fn is_background_copy() -> bool {
    #[cfg(unix)]
    return std::env::var_os(CHILD_ENV).is_some();
    #[cfg(not(unix))]
    false
}

/// Starts the background copy and relays its stderr until it says the window is up (exit 0) or
/// dies (exit with its code). Until then the copy shares the terminal's process group, so Ctrl-C
/// stops both. Not available off Unix: there it returns and the program runs in the foreground.
#[cfg(unix)]
pub fn start_in_background() -> ! {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio, exit};

    let fail = |what: &str, e: std::io::Error| -> ! {
        eprintln!("error: could not {what}: {e}");
        exit(1)
    };
    let exe = std::env::current_exe().unwrap_or_else(|e| fail("find pr-review's own path", e));
    let mut child = Command::new(exe)
        .args(std::env::args_os().skip(1))
        .env(CHILD_ENV, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| fail("start pr-review in the background", e));

    let stderr = child.stderr.take().expect("stderr was piped");
    for line in BufReader::new(stderr).split(b'\n') {
        let Ok(line) = line else { break };
        if line == READY.as_bytes() {
            exit(0);
        }
        eprintln!("{}", String::from_utf8_lossy(&line));
    }
    exit(child.wait().ok().and_then(|s| s.code()).unwrap_or(1))
}

#[cfg(not(unix))]
pub fn start_in_background() {}

/// Called once the window exists. In the background copy: leaves the terminal's session, tells
/// the waiting command to exit, and stops writing to the pipe nobody reads anymore.
pub fn window_is_up() {
    #[cfg(unix)]
    if is_background_copy() {
        use std::os::fd::AsRawFd;

        // SAFETY: setsid and dup2 take plain integers and have no memory-safety preconditions.
        unsafe { libc::setsid() };
        eprintln!("{READY}");
        if let Ok(null) = std::fs::OpenOptions::new().write(true).open("/dev/null") {
            unsafe { libc::dup2(null.as_raw_fd(), 2) };
        }
    }
}
