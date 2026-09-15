//! Job-scoped ACP session pool for subtitle workshop tasks.
//!
//! Each job owns up to four lazily-created isolated ACP sessions. A slot keeps
//! its session after a batch completes, so later batches assigned to that slot
//! continue the same conversation. A conversation claims a slot for the
//! complete batch, including content/validation retries.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::domain::model::{AcpEvent, AcpSessionModelSelection, AgentProfilesHint};
use crate::error::{AcpError, AcpErrorCode};
use crate::jobs::isolated::isolated_client_settings;
use crate::runtime::service::AcpService;
use lumina_core::{AgentConversation, AgentTaskError, IsolatedAgentTask};

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

pub(crate) trait SlotRunner: Send + Sync {
    fn run(&self, prompt: String, task_label: Option<String>) -> Result<String, AcpError>;
    fn shutdown(&self);
}

pub(crate) struct RealRunner {
    service: AcpService,
    profile_id: String,
    profiles: AgentProfilesHint,
    model_selection: Option<AcpSessionModelSelection>,
    session_ids: Arc<Mutex<BTreeSet<String>>>,
}

impl RealRunner {
    fn new(
        profile_id: String,
        profiles: AgentProfilesHint,
        model_selection: Option<AcpSessionModelSelection>,
        session_ids: Arc<Mutex<BTreeSet<String>>>,
    ) -> Self {
        Self {
            service: AcpService::new_isolated(model_selection.clone()),
            profile_id,
            profiles,
            model_selection,
            session_ids,
        }
    }
}

impl SlotRunner for RealRunner {
    fn run(&self, prompt: String, task_label: Option<String>) -> Result<String, AcpError> {
        // A live service/session is intentionally reused. Re-arm only matters
        // if a previous transport failure forced a fresh spawn.
        self.service
            .rearm_isolated_model_selection(self.model_selection.clone());
        let session_ids = Arc::clone(&self.session_ids);
        self.service.prompt_with_label(
            prompt,
            None,
            Some(self.profile_id.clone()),
            None,
            None,
            None,
            isolated_client_settings(),
            self.profiles.clone(),
            move |event| {
                if let AcpEvent::SessionSaved { session_id, .. } = event {
                    if let Ok(mut ids) = session_ids.lock() {
                        ids.insert(session_id);
                    }
                }
            },
            task_label.as_deref(),
        )
    }

    fn shutdown(&self) {
        let _ = self.service.close_session();
    }
}

struct PoolSlot {
    runner: Mutex<Box<dyn SlotRunner>>,
    claimed: AtomicBool,
    bootstrapped: AtomicBool,
}

struct PoolConversation {
    slot: Arc<PoolSlot>,
    slot_index: usize,
    job_id: String,
}

impl AgentConversation for PoolConversation {
    fn prompt(&mut self, task: IsolatedAgentTask) -> Result<String, AgentTaskError> {
        let runner = self
            .slot
            .runner
            .lock()
            .map_err(|_| AgentTaskError::Failed {
                details: Some("workshop slot mutex poisoned".into()),
            })?;
        match runner.run(task.prompt.clone(), task.task_label.clone()) {
            Ok(text) => {
                self.slot.bootstrapped.store(true, Ordering::Release);
                Ok(text)
            }
            Err(error) if is_transport_retryable(&error) => {
                // ACP service drops an uncertain live session on transport
                // errors. The retry stays in this logical slot; the next
                // prompt must bootstrap again if the session was replaced.
                self.slot.bootstrapped.store(false, Ordering::Release);
                let retry_label = task.retry_task_label.or(task.task_label);
                tracing::warn!(
                    job_id = %self.job_id,
                    slot = self.slot_index,
                    task_label = ?retry_label,
                    code = ?error.code,
                    "workshop transport failure, retrying on same slot"
                );
                std::thread::sleep(Duration::from_secs(2));
                let recovery_prompt = task.bootstrap_prompt.unwrap_or(task.prompt);
                let text = runner
                    .run(recovery_prompt, retry_label)
                    .map_err(map_agent_error)?;
                self.slot.bootstrapped.store(true, Ordering::Release);
                Ok(text)
            }
            Err(error) => Err(map_agent_error(error)),
        }
    }

    fn needs_bootstrap(&self) -> bool {
        !self.slot.bootstrapped.load(Ordering::Acquire)
    }
}

impl Drop for PoolConversation {
    fn drop(&mut self) {
        self.slot.claimed.store(false, Ordering::Release);
    }
}

pub struct WorkshopPool {
    slots: Vec<Arc<PoolSlot>>,
    next: AtomicUsize,
    closed: AtomicBool,
    job_id: String,
    session_ids: Arc<Mutex<BTreeSet<String>>>,
}

/// New name for the workshop pool; old name retained for compatibility.
pub type IsolatedSessionPool = WorkshopPool;

impl WorkshopPool {
    pub fn new(config: PoolConfig, job_id: String) -> Self {
        let size = config.size.max(1);
        let session_ids = Arc::new(Mutex::new(BTreeSet::new()));
        let slots = (0..size)
            .map(|_| {
                Arc::new(PoolSlot {
                    runner: Mutex::new(Box::new(RealRunner::new(
                        config.profile_id.clone(),
                        config.profiles.clone(),
                        config.model_selection.clone(),
                        Arc::clone(&session_ids),
                    ))),
                    claimed: AtomicBool::new(false),
                    bootstrapped: AtomicBool::new(false),
                })
            })
            .collect();
        Self {
            slots,
            next: AtomicUsize::new(0),
            closed: AtomicBool::new(false),
            job_id,
            session_ids,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_runners(runners: Vec<Box<dyn SlotRunner>>, job_id: &str) -> Self {
        assert!(!runners.is_empty(), "pool needs at least one slot");
        Self {
            slots: runners
                .into_iter()
                .map(|runner| {
                    Arc::new(PoolSlot {
                        runner: Mutex::new(runner),
                        claimed: AtomicBool::new(false),
                        bootstrapped: AtomicBool::new(false),
                    })
                })
                .collect(),
            next: AtomicUsize::new(0),
            closed: AtomicBool::new(false),
            job_id: job_id.to_string(),
            session_ids: Arc::new(Mutex::new(BTreeSet::new())),
        }
    }

    fn claim_slot(&self) -> Result<(usize, Arc<PoolSlot>), AcpError> {
        loop {
            if self.closed.load(Ordering::Acquire) {
                return Err(AcpError::internal(Some("workshop pool closed")));
            }
            let start = self.next.fetch_add(1, Ordering::Relaxed) % self.slots.len();
            for offset in 0..self.slots.len() {
                let index = (start + offset) % self.slots.len();
                let slot = Arc::clone(&self.slots[index]);
                if slot
                    .claimed
                    .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                    .is_ok()
                {
                    return Ok((index, slot));
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Pin one complete batch to a dynamically selected slot. The underlying
    /// ACP session remains live after the conversation drops and can be
    /// reused by a later batch assigned to the same slot.
    pub fn open_conversation(
        self: &Arc<Self>,
        task_label: Option<String>,
    ) -> Result<Box<dyn AgentConversation>, AcpError> {
        let (slot_index, slot) = self.claim_slot()?;
        tracing::debug!(
            job_id = %self.job_id,
            slot = slot_index,
            task_label = ?task_label,
            "workshop slot claimed"
        );
        Ok(Box::new(PoolConversation {
            slot,
            slot_index,
            job_id: self.job_id.clone(),
        }))
    }

    /// Compatibility entry point for callers that still have one prompt.
    pub fn submit(
        self: &Arc<Self>,
        prompt: String,
        task_label: Option<String>,
        retry_label: Option<String>,
    ) -> Result<String, AcpError> {
        let mut conversation = self.open_conversation(task_label.clone())?;
        conversation
            .prompt(IsolatedAgentTask {
                prompt,
                bootstrap_prompt: None,
                profile_id: String::new(),
                model_id: None,
                reasoning_effort: None,
                task_label,
                retry_task_label: retry_label,
            })
            .map_err(|error| match error {
                AgentTaskError::NotConfigured { details }
                | AgentTaskError::Failed { details }
                | AgentTaskError::NoOutput { details } => AcpError::protocol(details.as_deref()),
            })
    }

    pub fn shutdown(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        for (index, slot) in self.slots.iter().enumerate() {
            match slot.runner.lock() {
                Ok(runner) => runner.shutdown(),
                Err(_) => tracing::warn!(
                    job_id = %self.job_id,
                    slot = index,
                    "slot lock poisoned during shutdown"
                ),
            }
        }
        let session_ids = self
            .session_ids
            .lock()
            .map(|ids| ids.clone())
            .unwrap_or_default();
        crate::jobs::rollout::cleanup_codex_rollouts(&session_ids, &self.job_id);
    }
}

impl Drop for WorkshopPool {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn is_transport_retryable(error: &AcpError) -> bool {
    matches!(
        error.code,
        AcpErrorCode::SpawnFailed | AcpErrorCode::ProtocolError
    )
}

fn map_agent_error(error: AcpError) -> AgentTaskError {
    if error.code == AcpErrorCode::NotConfigured {
        AgentTaskError::NotConfigured {
            details: error.details,
        }
    } else if error.code == AcpErrorCode::NoOutput {
        AgentTaskError::NoOutput {
            details: error.details,
        }
    } else {
        AgentTaskError::Failed {
            details: error.details,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    struct FakeRunner {
        script: Mutex<VecDeque<Result<String, AcpError>>>,
        delay: Duration,
    }

    impl FakeRunner {
        fn new(script: Vec<Result<String, AcpError>>) -> Self {
            Self {
                script: Mutex::new(script.into()),
                delay: Duration::ZERO,
            }
        }

        fn delayed(script: Vec<Result<String, AcpError>>, delay: Duration) -> Self {
            Self {
                script: Mutex::new(script.into()),
                delay,
            }
        }
    }

    impl SlotRunner for FakeRunner {
        fn run(&self, _prompt: String, _label: Option<String>) -> Result<String, AcpError> {
            if !self.delay.is_zero() {
                std::thread::sleep(self.delay);
            }
            self.script
                .lock()
                .expect("script")
                .pop_front()
                .expect("script exhausted")
        }

        fn shutdown(&self) {}
    }

    fn ok(text: &str) -> Result<String, AcpError> {
        Ok(text.into())
    }

    fn failed() -> Result<String, AcpError> {
        Err(AcpError::protocol(Some("transport")))
    }

    fn task(prompt: &str, label: &str) -> IsolatedAgentTask {
        IsolatedAgentTask {
            prompt: prompt.into(),
            bootstrap_prompt: None,
            profile_id: "p".into(),
            model_id: None,
            reasoning_effort: None,
            task_label: Some(label.into()),
            retry_task_label: Some(format!("{label} transport_attempt=2")),
        }
    }

    #[test]
    fn lease_stays_claimed_until_batch_conversation_drops() {
        let pool = Arc::new(WorkshopPool::with_runners(
            vec![Box::new(FakeRunner::new(vec![ok("a"), ok("b")]))],
            "job",
        ));
        let mut first = pool.open_conversation(None).expect("first");
        assert_eq!(first.prompt(task("p1", "batch=1")).expect("p1"), "a");
        let pool2 = Arc::clone(&pool);
        let blocked = std::thread::spawn(move || pool2.open_conversation(None));
        std::thread::sleep(Duration::from_millis(25));
        assert!(!blocked.is_finished());
        drop(first);
        let second = blocked.join().expect("join").expect("second");
        drop(second);
        pool.shutdown();
    }

    #[test]
    fn transport_retry_and_later_prompt_use_same_slot() {
        let pool = Arc::new(WorkshopPool::with_runners(
            vec![Box::new(FakeRunner::new(vec![
                failed(),
                ok("healed"),
                ok("next"),
            ]))],
            "job",
        ));
        let mut conversation = pool.open_conversation(None).expect("conversation");
        assert_eq!(
            conversation
                .prompt(task("same", "job=x batch=1"))
                .expect("retry"),
            "healed"
        );
        assert_eq!(
            conversation
                .prompt(task("next", "job=x batch=2"))
                .expect("next"),
            "next"
        );
        drop(conversation);
        pool.shutdown();
    }

    #[test]
    fn completed_batch_releases_slot_but_keeps_its_session_for_next_batch() {
        let pool = Arc::new(WorkshopPool::with_runners(
            vec![Box::new(FakeRunner::new(vec![ok("first"), ok("second")]))],
            "job",
        ));
        let mut first_batch = pool
            .open_conversation(Some("batch=1".into()))
            .expect("first");
        assert!(first_batch.needs_bootstrap());
        assert_eq!(
            first_batch.prompt(task("one", "batch=1")).expect("one"),
            "first"
        );
        drop(first_batch);

        let mut second_batch = pool
            .open_conversation(Some("batch=2".into()))
            .expect("second");
        assert!(
            !second_batch.needs_bootstrap(),
            "the free slot keeps its successful ACP session for the next batch"
        );
        assert_eq!(
            second_batch.prompt(task("two", "batch=2")).expect("two"),
            "second"
        );
        drop(second_batch);
        pool.shutdown();
    }

    #[test]
    fn dynamic_scheduler_uses_idle_sibling() {
        let pool = Arc::new(WorkshopPool::with_runners(
            vec![
                Box::new(FakeRunner::delayed(
                    vec![ok("slow")],
                    Duration::from_millis(100),
                )),
                Box::new(FakeRunner::new(vec![ok("fast")])),
            ],
            "job",
        ));
        let slow_pool = Arc::clone(&pool);
        let slow = std::thread::spawn(move || {
            let mut conversation = slow_pool.open_conversation(None).expect("slow");
            conversation.prompt(task("slow", "batch=1"))
        });
        std::thread::sleep(Duration::from_millis(10));
        let mut fast = pool.open_conversation(None).expect("fast");
        assert_eq!(fast.prompt(task("fast", "batch=2")).expect("fast"), "fast");
        drop(fast);
        assert_eq!(slow.join().expect("join").expect("slow"), "slow");
        pool.shutdown();
    }

    #[test]
    fn closed_pool_rejects_new_conversations() {
        let pool = Arc::new(WorkshopPool::with_runners(
            vec![Box::new(FakeRunner::new(vec![ok("x")]))],
            "job",
        ));
        pool.shutdown();
        assert!(pool.open_conversation(None).is_err());
    }
}
