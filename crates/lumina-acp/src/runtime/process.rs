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

/// Windows Job Object owning an Agent subtree, configured to kill every
/// member when the last handle closes.
///
/// `HANDLE` is a raw pointer, so it is not `Send`/`Sync` by inference. A job
/// handle is just a kernel object reference with no thread affinity, and the
/// Win32 calls below are thread-safe, so an owning wrapper may cross threads.
#[cfg(windows)]
struct JobHandle(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
unsafe impl Send for JobHandle {}
#[cfg(windows)]
unsafe impl Sync for JobHandle {}

#[cfg(windows)]
impl Drop for JobHandle {
    fn drop(&mut self) {
        // KILL_ON_JOB_CLOSE: closing the last handle terminates whatever is
        // still in the job, so an abandoned AgentProcess cannot leak a tree.
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}

/// An Agent child process plus whatever the host offers for whole-subtree
/// termination.
///
/// Killing only the direct child is never enough: a Windows ACP adapter runs
/// `codex-acp.exe -> shim -> node -> codex.exe`, and it is `codex.exe` that
/// holds Codex's per-thread rollout writer lock. A surviving `codex.exe` keeps
/// that lock forever, which makes the conversation permanently unresumable
/// (`thread <id> already has an active writer`). `taskkill /T` cannot be
/// trusted here because it walks live parent links: once an intermediate
/// layer exits, its children are orphaned and fall out of the walk entirely.
pub struct AgentProcess {
    child: Child,
    #[cfg(windows)]
    job: Option<JobHandle>,
}

impl AgentProcess {
    /// Put a freshly spawned child under subtree-termination control.
    ///
    /// Descendants created between `spawn` and this call would escape the job,
    /// but an ACP adapter needs milliseconds of interpreter startup before it
    /// spawns anything, so the window is not reachable in practice. Job setup
    /// failure degrades to the legacy best-effort kill rather than refusing to
    /// run the Agent.
    pub fn adopt(child: Child) -> Self {
        #[cfg(windows)]
        {
            let job = Self::create_job(&child);
            Self { child, job }
        }
        #[cfg(not(windows))]
        {
            Self { child }
        }
    }

    #[cfg(windows)]
    fn create_job(child: &Child) -> Option<JobHandle> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };

        // SAFETY: every call below is a documented Win32 entry point. The
        // handle is owned by JobHandle from the moment it is non-null, the
        // info struct is a local zeroed POD of the size we pass, and the
        // process handle stays valid because `child` outlives this call.
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                tracing::warn!("could not create Agent job object; subtree kill degraded");
                return None;
            }
            let job = JobHandle(handle);

            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                std::ptr::addr_of!(info).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) == 0
            {
                tracing::warn!("could not configure Agent job object; subtree kill degraded");
                return None;
            }

            if AssignProcessToJobObject(job.0, child.as_raw_handle().cast()) == 0 {
                tracing::warn!("could not assign Agent to job object; subtree kill degraded");
                return None;
            }
            Some(job)
        }
    }

    pub fn id(&self) -> u32 {
        self.child.id()
    }

    pub fn stdin(&mut self) -> Option<std::process::ChildStdin> {
        self.child.stdin.take()
    }

    pub fn stdout(&mut self) -> Option<std::process::ChildStdout> {
        self.child.stdout.take()
    }

    pub fn stderr(&mut self) -> Option<std::process::ChildStderr> {
        self.child.stderr.take()
    }

    /// Terminate the Agent and every descendant.
    ///
    /// `wait=false` does not reap the direct child; the subtree is still
    /// signalled synchronously so no caller can return while it is alive.
    pub fn terminate(&mut self, wait: bool) {
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::JobObjects::TerminateJobObject;

            let killed = match self.job.as_ref() {
                // SAFETY: `job.0` is a live job handle owned by `self`.
                Some(job) => unsafe { TerminateJobObject(job.0, 1) != 0 },
                None => false,
            };
            if !killed {
                self.terminate_without_job();
            }
        }

        #[cfg(not(windows))]
        {
            let _ = self.child.kill();
        }

        if wait {
            let _ = self.child.wait();
        }
    }

    /// Legacy best-effort path, used only when the job object is unavailable.
    /// Known to orphan re-parented grandchildren; kept so a sandbox that
    /// denies job objects still degrades instead of leaving the Agent running.
    #[cfg(windows)]
    fn terminate_without_job(&mut self) {
        let pid = self.child.id().to_string();
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
            tracing::warn!(pid = %pid, "taskkill failed, falling back to single-process kill");
            let _ = self.child.kill();
        }
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
    ///
    /// Adoption happens before the pid handshake on purpose: job membership is
    /// only inherited by descendants spawned after assignment, so adopting
    /// late would silently leave the grandchild outside the job. Production
    /// adopts immediately after spawn for the same reason.
    #[cfg(windows)]
    fn spawn_recorded_tree(tag: &str) -> (AgentProcess, std::path::PathBuf, u32) {
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
        let wrapper = command("powershell")
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
        let mut agent = AgentProcess::adopt(wrapper);
        // Wait for the grandchild pid record (powershell cold start ~1-2s).
        let deadline = Instant::now() + Duration::from_secs(10);
        let descendant = loop {
            if let Ok(raw) = std::fs::read_to_string(&pid_file) {
                if let Ok(pid) = raw.trim().parse::<u32>() {
                    break pid;
                }
            }
            if Instant::now() >= deadline {
                agent.terminate(false);
                panic!("{tag}: grandchild pid never recorded");
            }
            std::thread::sleep(Duration::from_millis(200));
        };
        (agent, pid_file, descendant)
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
                if text.contains(&format!("\"{pid}\",")) {
                    return true;
                }
                let script = format!(
                    "if (Get-Process -Id {pid} -ErrorAction SilentlyContinue) {{ exit 0 }} else {{ exit 1 }}"
                );
                command("powershell")
                    .args([
                        "-NoProfile",
                        "-NonInteractive",
                        "-ExecutionPolicy",
                        "Bypass",
                        "-Command",
                        &script,
                    ])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .map(|status| status.success())
                    .unwrap_or(true)
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
    fn terminate_without_wait_returns_promptly_and_kills_tree() {
        let mut agent = AgentProcess::adopt(sleeper());
        let started = Instant::now();
        agent.terminate(false);
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "wait=false must not synchronously wait"
        );
        wait_dead(&mut agent.child, "wait=false");
    }

    #[test]
    #[cfg(windows)]
    fn terminate_with_wait_reaps_before_return() {
        let mut agent = AgentProcess::adopt(sleeper());
        agent.terminate(true);
        assert!(
            agent.child.try_wait().expect("try_wait").is_some(),
            "wait=true must reap before returning"
        );
    }

    #[test]
    #[cfg(windows)]
    fn terminate_without_wait_kills_descendant() {
        let (mut agent, pid_file, descendant) = spawn_recorded_tree("nowait");
        agent.terminate(false);
        wait_dead(&mut agent.child, "wait=false wrapper");
        wait_gone(descendant, "wait=false descendant");
        let _ = std::fs::remove_file(&pid_file);
    }

    #[test]
    #[cfg(windows)]
    fn terminate_with_wait_kills_descendant() {
        let (mut agent, pid_file, descendant) = spawn_recorded_tree("wait");
        agent.terminate(true);
        wait_dead(&mut agent.child, "wait=true wrapper");
        wait_gone(descendant, "wait=true descendant");
        let _ = std::fs::remove_file(&pid_file);
    }

    /// The bug that bricked Codex conversations: the real Agent tree is
    /// `codex-acp -> shim -> node -> codex.exe`, and intermediate layers exit
    /// early. Once the tracked child is gone its descendants are orphaned, so
    /// `taskkill /T` walks nothing and the orphan survives holding Codex's
    /// thread writer lock. Only whole-job termination reaches it.
    #[test]
    #[cfg(windows)]
    fn terminate_kills_orphaned_descendant_after_direct_child_exits() {
        // Adoption has to happen at spawn time, exactly as in production:
        // job membership is inherited by descendants and outlives the tracked
        // child, whereas a dead process can no longer be assigned to a job.
        let (mut agent, pid_file, descendant) = spawn_orphaning_tree();
        wait_dead(&mut agent.child, "wrapper should exit on its own");
        assert!(
            pid_alive(descendant),
            "test precondition: orphan must outlive its parent"
        );

        agent.terminate(true);

        wait_gone(descendant, "orphaned descendant");
        let _ = std::fs::remove_file(&pid_file);
    }

    /// Spawn `wrapper -> ping` where the wrapper exits immediately, leaving
    /// the ping orphaned and unreachable through parent links.
    #[cfg(windows)]
    fn spawn_orphaning_tree() -> (AgentProcess, std::path::PathBuf, u32) {
        let pid_file =
            std::env::temp_dir().join(format!("lumina-orphan-test-{}.pid", std::process::id()));
        let _ = std::fs::remove_file(&pid_file);
        let script = format!(
            "$p = Start-Process -FilePath ping -ArgumentList '-n','60','127.0.0.1' \
             -WindowStyle Hidden -PassThru; \
             $p.Id | Set-Content -Path '{}' -Encoding Ascii -NoNewline",
            pid_file.to_string_lossy().replace('\\', "/")
        );
        let wrapper = command("powershell")
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
            .expect("spawn orphaning wrapper");
        let agent = AgentProcess::adopt(wrapper);

        let deadline = Instant::now() + Duration::from_secs(10);
        let descendant = loop {
            if let Ok(raw) = std::fs::read_to_string(&pid_file) {
                if let Ok(pid) = raw.trim().parse::<u32>() {
                    break pid;
                }
            }
            if Instant::now() >= deadline {
                panic!("orphan grandchild pid never recorded");
            }
            std::thread::sleep(Duration::from_millis(200));
        };
        (agent, pid_file, descendant)
    }
}
