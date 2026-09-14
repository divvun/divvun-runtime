//! A request/response worker thread for linguistic resources that are neither
//! `Send` nor re-entrant, and so have to live on one dedicated OS thread.
//!
//! Every response is delivered on a [`tokio::sync::oneshot`] channel that
//! travels *with* its request, so a response can only ever reach the caller
//! that asked for it. The older shape — a shared `mpsc` receiver behind a
//! `Mutex`, sent to before the lock was taken — could hand caller B the output
//! the worker computed for caller A whenever two pipeline tasks shared one
//! command instance.

use std::thread::JoinHandle;

use tokio::sync::{mpsc, oneshot};

/// The worker thread is no longer able to answer: either it has finished (the
/// command was dropped) or it died while handling a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerGone;

impl std::fmt::Display for WorkerGone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("worker thread is no longer running")
    }
}

impl std::error::Error for WorkerGone {}

/// A handle to a worker thread that answers one request at a time.
///
/// `Req` values are handed over one at a time (the channel holds a single
/// slot), and each carries the reply channel for its own response.
pub struct Worker<Req, Resp> {
    tx: mpsc::Sender<(Req, oneshot::Sender<Resp>)>,
    _thread: JoinHandle<()>,
}

impl<Req, Resp> Worker<Req, Resp>
where
    Req: Send + 'static,
    Resp: Send + 'static,
{
    /// Spawn the worker thread. `init` runs *on that thread* and returns the
    /// handler; put resource loading in there when the resource cannot cross a
    /// thread boundary, and close over it when it can.
    pub fn spawn<Init, Handler>(init: Init) -> Self
    where
        Init: FnOnce() -> Handler + Send + 'static,
        Handler: FnMut(Req) -> Resp + 'static,
    {
        let (tx, mut rx) = mpsc::channel::<(Req, oneshot::Sender<Resp>)>(1);

        let thread = std::thread::spawn(move || {
            let mut handler = init();

            // Ends when the last `Worker` handle is dropped and the channel
            // closes — the old explicit `None` shutdown message was never sent
            // by anyone.
            while let Some((req, reply)) = rx.blocking_recv() {
                // A dropped receiver means the caller went away (cancelled
                // future); the response is discarded rather than handed to
                // whoever asks next.
                let _ = reply.send(handler(req));
            }
        });

        Self {
            tx,
            _thread: thread,
        }
    }

    /// Hand `req` to the worker and await *this* request's response.
    pub async fn call(&self, req: Req) -> Result<Resp, WorkerGone> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send((req, reply_tx))
            .await
            .map_err(|_| WorkerGone)?;
        reply_rx.await.map_err(|_| WorkerGone)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;

    /// The regression test for response attribution: N callers share one
    /// worker, the handler is slow enough that all of them are in flight at
    /// once, and every caller must get the answer to its own question. Under
    /// the old send-then-lock-the-shared-receiver shape the answers crossed.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn each_caller_receives_its_own_response() {
        let worker: Arc<Worker<String, String>> = Arc::new(Worker::spawn(|| {
            |input: String| {
                // Slow enough that later callers queue up behind this one.
                std::thread::sleep(Duration::from_millis(20));
                input.chars().rev().collect::<String>()
            }
        }));

        let inputs: Vec<String> = (0..8).map(|i| format!("request-{i}")).collect();

        let mut tasks = Vec::new();
        for input in inputs.clone() {
            let worker = worker.clone();
            tasks.push(tokio::spawn(async move {
                let output = worker.call(input.clone()).await.expect("worker call");
                (input, output)
            }));
        }

        for task in tasks {
            let (input, output) = task.await.expect("join");
            let expected: String = input.chars().rev().collect();
            assert_eq!(
                output, expected,
                "caller for {input:?} received another caller's response"
            );
        }
    }

    /// The abandoned-request case, which is how the crossing shows up in a
    /// pipeline: caller A sends, then goes away (its future is dropped — a
    /// cancelled forward, a closed stream) before the worker answers. A's
    /// answer must not be handed to caller B.
    ///
    /// With a shared response channel this is deterministic corruption: A's
    /// output sits in the single output slot and every later caller reads one
    /// response behind.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn abandoned_request_does_not_poison_the_next_caller() {
        let worker: Arc<Worker<String, String>> = Arc::new(Worker::spawn(|| {
            |input: String| {
                std::thread::sleep(Duration::from_millis(50));
                input.chars().rev().collect::<String>()
            }
        }));

        // Caller A gives up waiting; the request is already with the worker.
        let abandoned = tokio::time::timeout(
            Duration::from_millis(5),
            worker.call("abandoned".to_string()),
        )
        .await;
        assert!(abandoned.is_err(), "the first call should have timed out");

        // Let the worker finish A's request and try to deliver it.
        tokio::time::sleep(Duration::from_millis(100)).await;

        for caller in ["second", "third"] {
            let output = worker.call(caller.to_string()).await.expect("worker call");
            let expected: String = caller.chars().rev().collect();
            assert_eq!(
                output, expected,
                "caller {caller:?} received an abandoned caller's response"
            );
        }
    }

    /// Interleaved-latency variant: the first caller is much slower than the
    /// rest, which is the ordering that made the old code hand caller A's
    /// output to caller B.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn slow_first_caller_does_not_lose_its_response() {
        let worker: Arc<Worker<u32, u32>> = Arc::new(Worker::spawn(|| {
            |n: u32| {
                if n == 0 {
                    std::thread::sleep(Duration::from_millis(50));
                }
                n * 10
            }
        }));

        let mut tasks = Vec::new();
        for n in 0..6u32 {
            let worker = worker.clone();
            tasks.push(tokio::spawn(async move {
                // Stagger the sends so the slow request is genuinely first.
                tokio::time::sleep(Duration::from_millis(u64::from(n) * 2)).await;
                (n, worker.call(n).await.expect("worker call"))
            }));
        }

        for task in tasks {
            let (n, got) = task.await.expect("join");
            assert_eq!(got, n * 10, "caller {n} received another caller's response");
        }
    }

    /// A dead worker surfaces as an error instead of the caller hanging (or
    /// the old `.expect()` panicking inside a pipeline task).
    #[tokio::test]
    async fn dead_worker_reports_error() {
        // The panic message this provokes on stderr is expected test output.
        let worker: Worker<u32, u32> = Worker::spawn(|| {
            |n: u32| {
                if n == 0 {
                    panic!("worker handler exploded");
                }
                n
            }
        });

        assert_eq!(worker.call(0).await, Err(WorkerGone));

        // The thread is gone, so the request channel is closed too.
        assert_eq!(worker.call(1).await, Err(WorkerGone));
    }

    /// `init` runs on the worker thread, so a handler may close over state that
    /// is not `Send`, and that state persists across calls.
    #[tokio::test]
    async fn init_runs_on_the_worker_thread_and_state_persists() {
        let worker: Worker<(), usize> = Worker::spawn(|| {
            let not_send = std::rc::Rc::new(std::cell::Cell::new(0usize));
            move |()| {
                not_send.set(not_send.get() + 1);
                not_send.get()
            }
        });

        assert_eq!(worker.call(()).await, Ok(1));
        assert_eq!(worker.call(()).await, Ok(2));
        assert_eq!(worker.call(()).await, Ok(3));
    }
}
