//! Job-scoped ACP session pool for pipeline tasks (P2).
//!
//! Translation jobs fan out dozens of batches; spawning one agent process per
//! batch wastes processes, repeats authentication, and orphans grandchildren
//! on failure. A pool owns a fixed set of slots (default 4, configurable);
//! each slot runs one prompt at a time against a long-lived isolated session
//! on a reused process. The pool never touches the interactive chat session.
//!
//! Responsibility split (locked division of labor):
//! - pool: slot scheduling, session lifecycle (rotate/rebuild/shutdown),
//!   transport retry (identical replay, at most once), attempt logging;
//! - caller (lumina-ai): batch content, prompts, content-retry policy;
//! - adapter/commands: pool creation per job and explicit shutdown.
//!
//! `submit` blocks and must run off async executors (callers already use
//! `spawn_blocking` for workshop work).

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::error::{AcpError, AcpErrorCode};
use crate::model::{AcpSessionModelSelection, AgentProfilesHint};
use crate::service::AcpService;

/// Default slot count: matches the workshop fan-out width. Configurable per
/// pool via [`PoolConfig::size`]; callers running fewer threads leave slots
/// idle, more threads queue on slot locks (correct backpressure, no errors).
pub const DEFAULT_POOL_SIZE: usize = 4;

pub struct PoolConfig {
    pub size: usize,
    pub profile_id: String,
    pub profiles: AgentProfilesHint,
    pub model_selection: Option<AcpSessionModelSelection>,
}

impl PoolConfig {
    pub fn new(profile_id: impl Into<String>, profiles: AgentProfilesHint) -> Self {
        Self {
            size: DEFAULT_POOL_SIZE,
            profile_id: profile_id.into(),
            profiles,
            model_selection: None,
        }
    }
}

/// Execution backend for one slot. Production runs isolated sessions; tests
/// substitute scripted behavior. Kept crate-visible so only the pool drives it.
pub(crate) trait SlotRunner: Send + Sync {
    fn run(&self, prompt: String, task_label: Option<String>) -> Result<String, AcpError>;
    fn shutdown(&self);
}

pub(crate) struct RealRunner {
    service: AcpService,
    profile_id: String,
    profiles: AgentProfilesHint,
    model_selection: Option<AcpSessionModelSelection>,
}

impl RealRunner {
    fn new(
        profile_id: String,
        profiles: AgentProfilesHint,
        model_selection: Option<AcpSessionModelSelection>,
    ) -> Self {
        Self {
            service: AcpService::new_isolated(model_selection.clone()),
            profile_id,
            profiles,
            model_selection,
        }
    }
}

impl SlotRunner for RealRunner {
    fn run(&self, prompt: String, task_label: Option<String>) -> Result<String, AcpError> {
        // The fresh-spawn path consumes the one-shot model cell, so re-arm
        // the slot's selection on every run: rotation failures and transport
        // retries spawn fresh and must keep the user's model, not the agent
        // default. Rotate to a fresh session on the reused process next;
        // rotation failure leaves the slot empty and the prompt path below
        // spawns fresh — never run on a half-open session.
        self.service
            .rearm_isolated_model_selection(self.model_selection.clone());
        if let Err(error) = self.service.rotate_isolated_session(
            &self.profile_id,
            self.model_selection.as_ref(),
            &mut |_| {},
        ) {
            tracing::warn!(%error, "slot rotation failed; continuing on fresh spawn");
        }
        self.service.prompt_with_label(
            prompt,
            None,
            Some(self.profile_id.clone()),
            None,
            None,
            None,
            AcpService::isolated_client_settings(),
            self.profiles.clone(),
            |_| {},
            task_label.as_deref(),
        )
    }

    fn shutdown(&self) {
        let _ = self.service.close_session();
    }
}

pub struct WorkshopPool {
    slots: Vec<Mutex<Box<dyn SlotRunner>>>,
    next: AtomicUsize,
    closed: Arc<AtomicBool>,
    job_id: String,
}

impl WorkshopPool {
    pub fn new(config: PoolConfig, job_id: String) -> Self {
        let size = config.size.max(1);
        let mut slots = Vec::with_capacity(size);
        for _ in 0..size {
            let runner: Box<dyn SlotRunner> = Box::new(RealRunner::new(
                config.profile_id.clone(),
                config.profiles.clone(),
                config.model_selection.clone(),
            ));
            slots.push(Mutex::new(runner));
        }
        Self {
            slots,
            next: AtomicUsize::new(0),
            closed: Arc::new(AtomicBool::new(false)),
            job_id,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_runners(runners: Vec<Box<dyn SlotRunner>>, job_id: &str) -> Self {
        assert!(!runners.is_empty(), "pool needs at least one slot");
        Self {
            slots: runners.into_iter().map(Mutex::new).collect(),
            next: AtomicUsize::new(0),
            closed: Arc::new(AtomicBool::new(false)),
            job_id: job_id.to_string(),
        }
    }

    /// Submit one prompt; claims a truly idle slot (round-robin start). At
    /// most one transport-identical retry; content retries belong to the
    /// caller as new submits. Closed pools reject without touching slots.
    ///
    /// The retry send carries `retry_label` (the caller precomputes it, e.g.
    /// `transport_attempt=2`) so both real sends correlate independently;
    /// `None` reuses `task_label`. Labels are opaque here — never parsed.
    pub fn submit(
        &self,
        prompt: String,
        task_label: Option<String>,
        retry_label: Option<String>,
    ) -> Result<String, AcpError> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(AcpError::internal(Some("workshop pool closed")));
        }
        // Idle claiming: scan from the round-robin offset with try_lock so a
        // slow batch on one slot never parks tasks behind it while siblings
        // sit idle. Full circle with everything busy → block on the
        // round-robin slot until it frees.
        let start = self.next.fetch_add(1, Ordering::SeqCst) % self.slots.len();
        let mut idx = start;
        let (idx, slot) = loop {
            match self.slots[idx].try_lock() {
                Ok(guard) => break (idx, guard),
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    return Err(AcpError::internal(Some("pool slot mutex poisoned")));
                }
                Err(std::sync::TryLockError::WouldBlock) => {
                    idx = (idx + 1) % self.slots.len();
                    if idx == start {
                        let guard = self.slots[start]
                            .lock()
                            .map_err(|_| AcpError::internal(Some("pool slot mutex poisoned")))?;
                        break (start, guard);
                    }
                }
            }
        };
        match slot.run(prompt.clone(), task_label.clone()) {
            Ok(text) => Ok(text),
            Err(error) if is_transport_retryable(&error) => {
                let retry_label = retry_label.or(task_label);
                tracing::warn!(
                    job_id = %self.job_id,
                    task_label = ?retry_label,
                    slot = idx,
                    code = ?error.code,
                    "workshop slot transport failure, retrying once"
                );
                std::thread::sleep(Duration::from_secs(2));
                slot.run(prompt, retry_label)
            }
            Err(error) => Err(error),
        }
    }

    /// Close every slot; idempotent. Prefer explicit calls at job end; `Drop`
    /// is the backstop for error unwinds.
    pub fn shutdown(&self) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        for (idx, slot) in self.slots.iter().enumerate() {
            match slot.lock() {
                Ok(slot) => slot.shutdown(),
                Err(_) => tracing::warn!(
                    job_id = %self.job_id,
                    slot = idx,
                    "slot lock poisoned during shutdown"
                ),
            }
        }
    }
}

impl Drop for WorkshopPool {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Transport failures (identical replay is safe) vs deterministic states.
/// `NoOutput`/validation/cancel/busy never retry here; content retries live
/// with the caller as new submits.
fn is_transport_retryable(error: &AcpError) -> bool {
    use AcpErrorCode::*;
    matches!(error.code, SpawnFailed | ProtocolError)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    type CallLog = Arc<Mutex<Vec<(String, Option<String>)>>>;

    struct FakeRunner {
        calls: CallLog,
        script: Mutex<VecDeque<Result<String, AcpError>>>,
        shutdowns: AtomicUsize,
        first_call_delay: Duration,
    }

    impl FakeRunner {
        fn new(script: Vec<Result<String, AcpError>>) -> Self {
            Self::with_log(script, Arc::new(Mutex::new(Vec::new())), Duration::ZERO)
        }

        fn delayed(script: Vec<Result<String, AcpError>>, delay: Duration) -> Self {
            Self::with_log(script, Arc::new(Mutex::new(Vec::new())), delay)
        }

        fn with_log(script: Vec<Result<String, AcpError>>, calls: CallLog, first_call_delay: Duration) -> Self {
            Self {
                calls,
                script: Mutex::new(script.into()),
                shutdowns: AtomicUsize::new(0),
                first_call_delay,
            }
        }
    }

    impl SlotRunner for FakeRunner {
        fn run(&self, prompt: String, task_label: Option<String>) -> Result<String, AcpError> {
            let mut calls = self.calls.lock().expect("lock");
            if calls.is_empty() && !self.first_call_delay.is_zero() {
                std::thread::sleep(self.first_call_delay);
            }
            calls.push((prompt, task_label));
            drop(calls);
            self.script
                .lock()
                .expect("lock")
                .pop_front()
                .expect("script exhausted")
        }

        fn shutdown(&self) {
            self.shutdowns.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn ok(text: &str) -> Result<String, AcpError> {
        Ok(text.to_string())
    }

    fn failed() -> Result<String, AcpError> {
        Err(AcpError::protocol(Some("boom")))
    }

    fn pool_with_probes(
        script_per_slot: Vec<Vec<Result<String, AcpError>>>,
    ) -> (WorkshopPool, Vec<CallLog>) {
        let mut logs = Vec::new();
        let runners: Vec<Box<dyn SlotRunner>> = script_per_slot
            .into_iter()
            .map(|script| {
                let log = Arc::new(Mutex::new(Vec::new()));
                logs.push(Arc::clone(&log));
                Box::new(FakeRunner::with_log(script, log, Duration::ZERO)) as Box<dyn SlotRunner>
            })
            .collect();
        (WorkshopPool::with_runners(runners, "test-job"), logs)
    }

    fn pool_with(script_per_slot: Vec<Vec<Result<String, AcpError>>>) -> WorkshopPool {
        pool_with_probes(script_per_slot).0
    }

    #[test]
    fn transport_failure_retries_once_on_same_slot() {
        let pool = pool_with(vec![vec![failed(), ok("healed")]]);
        let text = pool
            .submit("prompt".into(), Some("job=x batch=1".into()), None)
            .expect("heals");
        assert_eq!(text, "healed");
        pool.shutdown();
    }

    #[test]
    fn transport_retry_send_carries_retry_label() {
        // Both real sends must correlate independently: first T=1, retry
        // T=2. No label is parsed anywhere; the caller precomputes both.
        let (pool, logs) = pool_with_probes(vec![vec![failed(), ok("healed")]]);
        let first = "job=x batch=1 content_attempt=1 transport_attempt=1";
        let retry = "job=x batch=1 content_attempt=1 transport_attempt=2";
        let text = pool
            .submit(
                "prompt".into(),
                Some(first.to_string()),
                Some(retry.to_string()),
            )
            .expect("heals");
        assert_eq!(text, "healed");
        let calls = logs[0].lock().expect("lock");
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].1.as_deref(), Some(first));
        assert_eq!(calls[1].1.as_deref(), Some(retry));
        pool.shutdown();
    }

    #[test]
    fn transport_retry_without_retry_label_reuses_first() {
        let (pool, logs) = pool_with_probes(vec![vec![failed(), ok("healed")]]);
        pool.submit("prompt".into(), Some("L".to_string()), None)
            .expect("heals");
        let calls = logs[0].lock().expect("lock");
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].1.as_deref(), Some("L"));
        assert_eq!(calls[1].1.as_deref(), Some("L"));
        pool.shutdown();
    }

    #[test]
    fn slow_slot_does_not_block_idle_siblings() {
        // Slot 0 sleeps on its first run; later submits must claim idle
        // slot 1 instead of parking behind slot 0 — including the third
        // submit, whose round-robin offset points back at busy slot 0.
        let runners: Vec<Box<dyn SlotRunner>> = vec![
            Box::new(FakeRunner::delayed(
                vec![ok("slow")],
                Duration::from_secs(5),
            )) as Box<dyn SlotRunner>,
            Box::new(FakeRunner::new(vec![ok("fast"), ok("third")])) as Box<dyn SlotRunner>,
        ];
        let pool = Arc::new(WorkshopPool::with_runners(runners, "idle-job"));
        let slow_pool = Arc::clone(&pool);
        let slow = std::thread::spawn(move || slow_pool.submit("s1".into(), None, None));
        std::thread::sleep(Duration::from_millis(200));
        let start = std::time::Instant::now();
        let fast = pool.submit("s2".into(), None, None).expect("idle slot");
        assert_eq!(fast, "fast");
        assert!(
            start.elapsed() < Duration::from_secs(4),
            "second submit parked behind the slow slot"
        );
        // Round-robin offset is back at slot 0 (still busy): must still find
        // idle slot 1 rather than block.
        let start = std::time::Instant::now();
        let third = pool.submit("s3".into(), None, None).expect("idle slot");
        assert_eq!(third, "third");
        assert!(
            start.elapsed() < Duration::from_secs(4),
            "third submit parked behind the slow slot"
        );
        assert_eq!(slow.join().expect("thread").expect("slow"), "slow");
        pool.shutdown();
    }

    #[test]
    fn deterministic_errors_pass_through_without_retry() {
        for make_err in [
            || AcpError::no_output(None),
            || AcpError::cancelled(),
            || AcpError::not_configured(None),
        ] {
            let pool = pool_with(vec![vec![Err(make_err())]]);
            let err = pool
                .submit("prompt".into(), None, None)
                .expect_err("must not retry");
            assert!(!pool.closed.load(Ordering::SeqCst));
            let _ = err;
            pool.shutdown();
        }
    }

    #[test]
    fn closed_pool_rejects_without_touching_slots() {
        let pool = pool_with(vec![vec![ok("x")]]);
        pool.shutdown();
        let err = pool
            .submit("prompt".into(), None, None)
            .expect_err("closed");
        assert_eq!(err.code, AcpErrorCode::InternalError);
    }

    #[test]
    fn sequential_batches_reuse_live_slot() {
        // Single slot serves batch 1 then batch 2: the second submit lands
        // on the same (rotated, live) slot instead of a fresh spawn.
        let pool = pool_with(vec![vec![ok("batch-1"), ok("batch-2")]]);
        let first = pool.submit("p1".into(), None, None).expect("batch 1");
        let second = pool.submit("p2".into(), None, None).expect("batch 2");
        assert_eq!(
            (first, second),
            ("batch-1".to_string(), "batch-2".to_string())
        );
        pool.shutdown();
    }

    #[test]
    fn drop_closes_pool_without_explicit_shutdown() {
        let pool = pool_with(vec![vec![ok("x")]]);
        let closed = Arc::clone(&pool.closed);
        drop(pool);
        assert!(closed.load(Ordering::SeqCst));
    }

    fn test_profiles() -> AgentProfilesHint {
        AgentProfilesHint {
            active_profile_id: "p".to_string(),
            profiles: Vec::new(),
        }
    }

    #[test]
    fn frozen_pool_defaults_hold() {
        // P2 frozen spec: pool default 4, job-scoped; zero size clamps to 1
        // so dispatch never divides by zero.
        assert_eq!(DEFAULT_POOL_SIZE, 4);
        assert_eq!(PoolConfig::new("p", test_profiles()).size, 4);
        let pool = WorkshopPool::new(
            PoolConfig {
                size: 0,
                ..PoolConfig::new("p", test_profiles())
            },
            "clamp-job".to_string(),
        );
        assert_eq!(pool.slots.len(), 1);
        pool.shutdown();
    }

    #[test]
    fn round_robin_spreads_sequential_submits() {
        let pool = pool_with(vec![
            vec![ok("a")],
            vec![ok("b")],
            vec![ok("c")],
            vec![ok("d")],
        ]);
        // Submission order is deterministic for sequential calls.
        let t1 = pool.submit("p1".into(), None, None).expect("s1");
        let t2 = pool.submit("p2".into(), None, None).expect("s2");
        assert_eq!((t1, t2), ("a".to_string(), "b".to_string()));
        pool.shutdown();
    }

    #[test]
    fn shutdown_is_idempotent() {
        let pool = pool_with(vec![vec![ok("x")]]);
        pool.shutdown();
        pool.shutdown();
    }
}
