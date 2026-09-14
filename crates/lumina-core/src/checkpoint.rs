//! Durable per-batch checkpoint port for long workshop jobs (translation,
//! proofreading).
//!
//! Jobs keep batch outputs in memory for speed; a checkpoint store persists
//! each finished batch so a later run resumes instead of restarting from
//! zero (a single failed batch used to nuke 30+ completed ones). Home is
//! `lumina-core` so AI crates and cache owners share one port without
//! dependency cycles.

use std::collections::BTreeMap;

/// One finished batch: texts in cue order plus reported (source, target)
/// name pairs for glossary backfill.
#[derive(Debug, Clone)]
pub struct CheckpointBatch {
    pub texts: Vec<String>,
    pub reported: Vec<(String, String)>,
}

/// Durable per-batch store, keyed by the owner (media + source + target +
/// model). Best-effort durability: the owner validates the fingerprint and
/// loads mismatches as empty; save failures are logged by the caller and
/// never fail the job (it continues in memory, like before checkpoints).
pub trait BatchCheckpoint: Send + Sync {
    /// Completed batches by 0-based batch index.
    fn load_completed(&self) -> BTreeMap<usize, CheckpointBatch>;
    fn save_batch(&self, index: usize, batch: &CheckpointBatch) -> Result<(), String>;
    /// Drop the whole checkpoint (called after a fully successful job).
    fn clear(&self) -> Result<(), String>;
}
