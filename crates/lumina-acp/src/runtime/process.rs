//! Spawn Agent processes without flashing a console on Windows.
//!
//! Kept crate-local so `lumina-acp` never depends on app helpers
//! (same pattern as `lumina-media::process`).

use std::ffi::OsStr;
use std::process::{Child, Command, Stdio};

pub fn command<P: AsRef<OsStr>>(program: P) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut cmd = Command::new(program);
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd
    }
    #[cfg(not(windows))]
    {
        Command::new(program)
    }
}

/// Terminate an Agent process and its descendants where the host provides a
/// supported process-tree primitive. Windows ACP adapters commonly spawn a
/// second Codex process, so killing only the wrapper is insufficient.
///
/// `wait=false` does not wait for or reap the target child. On Windows it
/// waits only for the short `taskkill` command to acknowledge the tree kill:
/// dropping that helper process immediately proved unreliable under the
/// desktop sandbox and could leave the Agent tree alive. A failed command
/// falls back to a plain kill with a warn, never silently.
pub fn terminate_tree(child: &mut Child, wait: bool) {
    #[cfg(windows)]
    {
        let pid = child.id().to_string();
        let mut taskkill = command("taskkill");
        taskkill
            .args(["/PID", &pid, "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let tree_killed = taskkill
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if !tree_killed {
            tracing::warn!(pid = %pid, "taskkill failed, falling back to kill");
            let _ = child.kill();
        }
    }

    #[cfg(not(windows))]
    {
        let _ = child.kill();
    }

    if wait {
        let _ = child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    fn sleeper() -> Child {
        command("cmd")
            .args(["/C", "ping", "-n", "30", "127.0.0.1"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn sleeper")
    }

    fn wait_dead(child: &mut Child, what: &str) {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(200))
                }
                Ok(None) => panic!("{what}: tree still alive 15s after terminate"),
                Err(error) => panic!("{what}: try_wait failed: {error}"),
            }
        }
    }

    /// Spawn `wrapper -> ping` and record the grandchild pid to a file, so
    /// tests can verify the DESCENDANT died, not just the wrapper. A wrapper
    ///-only kill (the pre-P1 bug) leaves the ping alive and fails these tests.
    #[cfg(windows)]
    fn spawn_recorded_tree(tag: &str) -> (Child, std::path::PathBuf, u32) {
        let pid_file = std::env::temp_dir().join(format!(
            "lumina-tree-test-{}-{}.pid",
            std::process::id(),
            tag
        ));
        let _ = std::fs::remove_file(&pid_file);
        let script = format!(
            "$p = Start-Process -FilePath ping -ArgumentList '-n','60','127.0.0.1' \
             -WindowStyle Hidden -PassThru; \
             $p.Id | Set-Content -Path '{}' -Encoding Ascii -NoNewline; \
             $p.WaitForExit()",
            pid_file.to_string_lossy().replace('\\', "/")
        );
        let mut wrapper = command("powershell")
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &script,
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn wrapper");
        // Wait for the grandchild pid record (powershell cold start ~1-2s).
        let deadline = Instant::now() + Duration::from_secs(10);
        let descendant = loop {
            if let Ok(raw) = std::fs::read_to_string(&pid_file) {
                if let Ok(pid) = raw.trim().parse::<u32>() {
                    break pid;
                }
            }
            if Instant::now() >= deadline {
                let _ = wrapper.kill();
                panic!("{tag}: grandchild pid never recorded");
            }
            std::thread::sleep(Duration::from_millis(200));
        };
        (wrapper, pid_file, descendant)
    }

    /// True while a process with this pid is visible to tasklist.
    #[cfg(windows)]
    fn pid_alive(pid: u32) -> bool {
        let output = command("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .output();
        match output {
            Ok(output) => {
                let text = String::from_utf8_lossy(&output.stdout);
                text.contains(&format!("\"{pid}\","))
            }
            Err(_) => true,
        }
    }

    #[cfg(windows)]
    fn wait_gone(pid: u32, what: &str) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while pid_alive(pid) {
            if Instant::now() >= deadline {
                panic!("{what}: descendant pid {pid} still alive 15s after terminate");
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    #[test]
    #[cfg(windows)]
    fn terminate_tree_without_wait_returns_promptly_and_kills_tree() {
        let mut child = sleeper();
        let started = Instant::now();
        terminate_tree(&mut child, false);
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "wait=false must not synchronously wait"
        );
        wait_dead(&mut child, "wait=false");
    }

    #[test]
    #[cfg(windows)]
    fn terminate_tree_with_wait_reaps_before_return() {
        let mut child = sleeper();
        terminate_tree(&mut child, true);
        assert!(
            child.try_wait().expect("try_wait").is_some(),
            "wait=true must reap before returning"
        );
    }

    #[test]
    #[cfg(windows)]
    fn terminate_tree_without_wait_kills_descendant() {
        let (mut wrapper, pid_file, descendant) = spawn_recorded_tree("nowait");
        terminate_tree(&mut wrapper, false);
        wait_dead(&mut wrapper, "wait=false wrapper");
        wait_gone(descendant, "wait=false descendant");
        let _ = std::fs::remove_file(&pid_file);
    }

    #[test]
    #[cfg(windows)]
    fn terminate_tree_with_wait_kills_descendant() {
        let (mut wrapper, pid_file, descendant) = spawn_recorded_tree("wait");
        terminate_tree(&mut wrapper, true);
        wait_dead(&mut wrapper, "wait=true wrapper");
        wait_gone(descendant, "wait=true descendant");
        let _ = std::fs::remove_file(&pid_file);
    }
}
