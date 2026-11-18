//! Threads: std's where std can start one, and Web Workers sharing the
//! module's memory on wasm32-unknown-unknown, where it cannot.
//!
//! A wasm module can run Rust on another thread only when it is built with
//! shared memory: `-C target-feature=+atomics,+bulk-memory,+mutable-globals`
//! over a std rebuilt to match (`-Z build-std=std,panic_abort`), linked with
//! `--shared-memory --import-memory --max-memory=<bytes>` and exporting
//! `__heap_base`, `__tls_base`, `__tls_size`, `__tls_align` and
//! `__wasm_init_tls` for wasm-bindgen. Without shared memory there is no
//! thread to start: [`spawn`] does not exist, and [`spawn_blocking`] runs
//! its work in place. build.rs sets the `threads` and `web_threads` cfgs that
//! tell these cases apart.
//!
//! On wasm, threads are pooled and reused: a thread that finishes its work
//! waits for more. On the web, a thread cannot start until the glue's URL is
//! known (see [`set_glue_url`]), and a Web Worker does not start while the
//! thread that asked for it is blocked. The runtime blocks while it waits for
//! a thread, so start the threads it will need with [`start`], and await
//! that, before running a pipeline. Each command that keeps a thread
//! ([`Worker`](crate::util::worker::Worker)) holds one for as long as it
//! lives, and every blocking task holds one while it runs. The page's main
//! thread may not block at all, so drive the runtime from a worker.

#[cfg(all(threads, target_family = "wasm"))]
mod pool;
#[cfg(web_threads)]
mod web;

#[cfg(all(threads, target_family = "wasm"))]
pub use pool::start;
#[cfg(web_threads)]
pub use web::set_glue_url;

/// A blocking task that ended without an answer: it panicked, or no thread
/// could be started to run it.
#[derive(Debug)]
pub struct BlockingTaskFailed(String);

impl std::fmt::Display for BlockingTaskFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "blocking task failed: {}", self.0)
    }
}

impl std::error::Error for BlockingTaskFailed {}

/// Start a thread named `name` running `f`. Nothing waits for it to end.
#[cfg(not(target_family = "wasm"))]
pub(crate) fn spawn<F>(name: &str, f: F) -> std::io::Result<()>
where
    F: FnOnce() + Send + 'static,
{
    std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(f)
        .map(drop)
}

/// Run `f` on a pooled thread, starting one if none is idle. Nothing waits
/// for it to end. Pooled threads are shared, so `name` names none of them.
#[cfg(all(threads, target_family = "wasm"))]
pub(crate) fn spawn<F>(_name: &str, f: F) -> std::io::Result<()>
where
    F: FnOnce() + Send + 'static,
{
    pool::execute(Box::new(f))
}

/// Run `f`, which blocks, off the async executor: on tokio's blocking pool.
#[cfg(not(target_family = "wasm"))]
pub(crate) async fn spawn_blocking<F, R>(f: F) -> Result<R, BlockingTaskFailed>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| BlockingTaskFailed(e.to_string()))
}

/// Run `f`, which blocks, off the async executor: on a pooled thread.
/// tokio's blocking pool starts its threads through std, which cannot start
/// one on wasm32-unknown-unknown.
#[cfg(all(threads, target_family = "wasm"))]
pub(crate) async fn spawn_blocking<F, R>(f: F) -> Result<R, BlockingTaskFailed>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    spawn("blocking", move || {
        let _ = tx.send(f());
    })
    .map_err(|e| BlockingTaskFailed(e.to_string()))?;
    rx.await
        .map_err(|_| BlockingTaskFailed("it panicked".to_owned()))
}

/// Run `f`, which blocks, in place: there is no other thread to run it on.
#[cfg(not(threads))]
pub(crate) async fn spawn_blocking<F, R>(f: F) -> Result<R, BlockingTaskFailed>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    Ok(f())
}
