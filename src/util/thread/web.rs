//! Threads as Web Workers. Each imports the module's wasm-bindgen glue,
//! instantiates the module over the shared memory, runs one closure and
//! closes.

use std::cell::OnceCell;
use std::io;
use std::sync::OnceLock;

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

type Job = Box<dyn FnOnce() + Send>;

static GLUE_URL: OnceLock<String> = OnceLock::new();

const WORKER_JS: &str = include_str!("worker.js");

/// Where the module's wasm-bindgen glue, built with `--target web`, is
/// served from. A thread imports it to join the module, so no thread starts
/// before this is set. The first call wins.
pub fn set_glue_url(url: impl Into<String>) {
    let _ = GLUE_URL.set(url.into());
}

/// Run the closure at `job` on this thread. Only `worker.js` calls this,
/// once, with what [`spawn`] gave it.
#[wasm_bindgen(js_name = "__divvun_runtime_thread_start")]
pub fn thread_start(job: usize) {
    // SAFETY: `job` is the `Box<Job>` that `spawn` leaked for this worker,
    // and the worker hands it here exactly once.
    let job = unsafe { Box::from_raw(job as *mut Job) };
    job();
}

/// Start a Web Worker named `name` running `f`. Nothing waits for it to end.
///
/// The worker starts once the browser gets to it, which can be long after
/// this returns, and not at all while the thread that asked blocks without
/// yielding to its event loop.
pub(super) fn spawn<F>(name: &str, f: F) -> io::Result<()>
where
    F: FnOnce() + Send + 'static,
{
    let glue = GLUE_URL.get().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "no thread can start before util::thread::set_glue_url is called",
        )
    })?;

    let options = web_sys::WorkerOptions::new();
    options.set_type(web_sys::WorkerType::Module);
    options.set_name(name);
    let worker = script_url()
        .and_then(|url| web_sys::Worker::new_with_options(&url, &options))
        .map_err(js_error)?;

    let job = Box::into_raw(Box::new(Box::new(f) as Job));
    // `worker.js` posts a message only when it could not join the module, so
    // the closure is still here, never run.
    let on_failure = Closure::once_into_js(move |event: web_sys::MessageEvent| {
        // SAFETY: the worker never took `job`; nothing else holds it.
        drop(unsafe { Box::from_raw(job) });
        tracing::error!("thread failed to start: {:?}", event.data());
    });
    worker.set_onmessage(Some(on_failure.unchecked_ref()));

    let message = js_sys::Array::of4(
        &JsValue::from_str(glue),
        &wasm_bindgen::module(),
        &wasm_bindgen::memory(),
        &JsValue::from(job as usize),
    );
    if let Err(e) = worker.post_message(&message) {
        worker.terminate();
        // SAFETY: the message carrying `job` was never sent.
        drop(unsafe { Box::from_raw(job) });
        return Err(js_error(e));
    }
    Ok(())
}

/// A URL for `worker.js`. A blob URL belongs to the realm that made it, so
/// every thread makes its own.
fn script_url() -> Result<String, JsValue> {
    thread_local! {
        static URL: OnceCell<String> = const { OnceCell::new() };
    }
    URL.with(|url| {
        if let Some(url) = url.get() {
            return Ok(url.clone());
        }
        let options = web_sys::BlobPropertyBag::new();
        options.set_type("text/javascript");
        let blob = web_sys::Blob::new_with_str_sequence_and_options(
            &js_sys::Array::of1(&JsValue::from_str(WORKER_JS)),
            &options,
        )?;
        let made = web_sys::Url::create_object_url_with_blob(&blob)?;
        Ok(url.get_or_init(|| made).clone())
    })
}

fn js_error(e: JsValue) -> io::Error {
    io::Error::other(format!("{e:?}"))
}
