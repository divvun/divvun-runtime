//! The threads of a wasm module, kept and reused: a thread that finishes its
//! job waits for the next. Starting one is slow (a Web Worker fetches the
//! glue and instantiates the module) and, on the web, cannot happen while
//! the thread that asked waits for it, so [`start`] starts them ahead of
//! need.

use std::collections::VecDeque;
use std::io;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

type Job = Box<dyn FnOnce() + Send>;

struct State {
    queue: VecDeque<Job>,
    /// Threads waiting for a job.
    idle: usize,
}

static STATE: Mutex<State> = Mutex::new(State {
    queue: VecDeque::new(),
    idle: 0,
});
static READY: Condvar = Condvar::new();

fn lock() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Start a thread that runs `first`, then takes jobs from the queue.
#[cfg(web_threads)]
fn start_thread(first: impl FnOnce() + Send + 'static) -> io::Result<()> {
    super::web::spawn("divvun-runtime", move || {
        first();
        work();
    })
}

/// Start a thread that runs `first`, then takes jobs from the queue.
#[cfg(not(web_threads))]
fn start_thread(first: impl FnOnce() + Send + 'static) -> io::Result<()> {
    std::thread::Builder::new()
        .name("divvun-runtime".to_owned())
        .spawn(move || {
            first();
            work();
        })
        .map(drop)
}

/// Queue `job`, starting a thread for it if no thread is idle.
pub(super) fn execute(job: Job) -> io::Result<()> {
    let mut state = lock();
    if state.idle > state.queue.len() {
        state.queue.push_back(job);
        READY.notify_one();
        return Ok(());
    }
    drop(state);
    // A thread of its own, whose first job is this one.
    start_thread(job)
}

/// Start `n` threads, and wait until each is running and idle. On the web,
/// await this before running a pipeline, which blocks while it waits for a
/// thread and so cannot wait for one to start.
pub async fn start(n: usize) -> io::Result<()> {
    let mut started = Vec::with_capacity(n);
    for _ in 0..n {
        let (tx, rx) = tokio::sync::oneshot::channel();
        start_thread(move || {
            let _ = tx.send(());
        })?;
        started.push(rx);
    }
    for rx in started {
        rx.await
            .map_err(|_| io::Error::other("a thread failed to start"))?;
    }
    Ok(())
}

fn work() {
    let mut state = lock();
    loop {
        if let Some(job) = state.queue.pop_front() {
            drop(state);
            job();
            state = lock();
            continue;
        }
        state.idle += 1;
        state = READY.wait(state).unwrap_or_else(PoisonError::into_inner);
        state.idle -= 1;
    }
}
