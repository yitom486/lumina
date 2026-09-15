//! Small bounded worker pool for MCP requests.
//!
//! The MCP transport is stdio, but a client may issue multiple JSON-RPC
//! requests without waiting for each response. Keeping a fixed worker pool
//! lets the server use that concurrency without creating an unbounded thread
//! per request.

use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};

type Job = Box<dyn FnOnce() + Send + 'static>;

pub(crate) struct TaskExecutor {
    sender: Option<mpsc::Sender<Job>>,
    workers: Vec<JoinHandle<()>>,
}

impl TaskExecutor {
    pub(crate) fn new(worker_count: usize) -> Self {
        let worker_count = worker_count.max(1);
        let (sender, receiver) = mpsc::channel::<Job>();
        let receiver = Arc::new(Mutex::new(receiver));
        let mut workers = Vec::with_capacity(worker_count);

        for _ in 0..worker_count {
            let receiver = Arc::clone(&receiver);
            workers.push(thread::spawn(move || loop {
                let job = {
                    let receiver = match receiver.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    receiver.recv()
                };
                match job {
                    Ok(job) => job(),
                    Err(_) => break,
                }
            }));
        }

        Self {
            sender: Some(sender),
            workers,
        }
    }

    pub(crate) fn submit<F>(&self, job: F) -> Result<(), String>
    where
        F: FnOnce() + Send + 'static,
    {
        self.sender
            .as_ref()
            .ok_or_else(|| "MCP tool executor is shutting down".to_string())?
            .send(Box::new(job))
            .map_err(|_| "MCP tool executor is unavailable".to_string())
    }
}

impl Drop for TaskExecutor {
    fn drop(&mut self) {
        // Dropping the last sender wakes every worker's recv call. Join them
        // so no tool task outlives the MCP process or its stdout handle.
        self.sender.take();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TaskExecutor;
    use std::sync::{Arc, Barrier, Mutex};

    #[test]
    fn executor_runs_submitted_jobs_before_shutdown() {
        let executor = TaskExecutor::new(2);
        let completed = Arc::new(Mutex::new(0_u32));
        for _ in 0..4 {
            let completed = Arc::clone(&completed);
            executor
                .submit(move || {
                    let mut count = completed.lock().expect("test counter");
                    *count += 1;
                })
                .expect("submit job");
        }
        drop(executor);
        assert_eq!(*completed.lock().expect("test counter"), 4);
    }

    #[test]
    fn executor_runs_independent_jobs_concurrently() {
        let executor = TaskExecutor::new(2);
        let barrier = Arc::new(Barrier::new(3));
        for _ in 0..2 {
            let barrier = Arc::clone(&barrier);
            executor
                .submit(move || {
                    barrier.wait();
                    barrier.wait();
                })
                .expect("submit job");
        }
        barrier.wait();
        barrier.wait();
        drop(executor);
    }
}
