use futures_util::future::{FusedFuture, Future};
use futures_util::task::ArcWake;
use futures_util::task::{Context, Poll, Waker, waker_ref};
use slab::Slab;
use std::cell::UnsafeCell;
use std::fmt;
use std::hash::Hasher;
use std::pin::Pin;
use std::ptr;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{Acquire, SeqCst};
use std::sync::{Arc, Mutex, Weak};

pub(crate) fn assert_future<T, F>(future: F) -> F
where
    F: Future<Output = T>,
{
    future
}

pub trait FutureExt: Future {
    fn boxed_shared(self: Pin<Box<Self>>) -> SharedBox<Self>
    where
        Self::Output: Clone,
    {
        assert_future::<Self::Output, _>(SharedBox::new(self))
    }
}

impl<T: ?Sized> FutureExt for T where T: Future {}

/// Future for the [`shared`](super::FutureExt::shared) method.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct SharedBox<Fut: ?Sized + Future> {
    inner: Option<Arc<Inner<Fut>>>,
    waker_key: usize,
}

struct Inner<Fut: ?Sized + Future> {
    future_or_output: UnsafeCell<FutureOrOutput<Fut>>,
    notifier: Arc<Notifier>,
}

struct Notifier {
    state: AtomicUsize,
    wakers: Mutex<Option<Slab<Option<Waker>>>>,
}

/// A weak reference to a [`Shared`] that can be upgraded much like an `Arc`.
pub struct WeakShared<Fut: ?Sized + Future>(Weak<Inner<Fut>>);

impl<Fut: Future> Clone for WeakShared<Fut> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<Fut: Future> fmt::Debug for SharedBox<Fut> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Shared")
            .field("inner", &self.inner)
            .field("waker_key", &self.waker_key)
            .finish()
    }
}

impl<Fut: Future> fmt::Debug for Inner<Fut> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Inner").finish()
    }
}

impl<Fut: Future> fmt::Debug for WeakShared<Fut> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WeakShared").finish()
    }
}

enum FutureOrOutput<Fut: ?Sized + Future> {
    Future(Pin<Box<Fut>>),
    Output(Fut::Output),
}

unsafe impl<Fut: ?Sized> Send for Inner<Fut>
where
    Fut: Future + Send,
    Fut::Output: Send + Sync,
{
}

unsafe impl<Fut: ?Sized> Sync for Inner<Fut>
where
    Fut: Future + Send,
    Fut::Output: Send + Sync,
{
}

const IDLE: usize = 0;
const POLLING: usize = 1;
const COMPLETE: usize = 2;
const POISONED: usize = 3;

const NULL_WAKER_KEY: usize = usize::max_value();

impl<Fut: ?Sized + Future> SharedBox<Fut> {
    pub(super) fn new(future: Pin<Box<Fut>>) -> Self {
        let inner = Inner {
            future_or_output: UnsafeCell::new(FutureOrOutput::Future(future)),
            notifier: Arc::new(Notifier {
                state: AtomicUsize::new(IDLE),
                wakers: Mutex::new(Some(Slab::new())),
            }),
        };

        Self {
            inner: Some(Arc::new(inner)),
            waker_key: NULL_WAKER_KEY,
        }
    }
}

impl<Fut: ?Sized> SharedBox<Fut>
where
    Fut: Future,
{
    /// Returns [`Some`] containing a reference to this [`Shared`]'s output if
    /// it has already been computed by a clone or [`None`] if it hasn't been
    /// computed yet or this [`Shared`] already returned its output from
    /// [`poll`](Future::poll).
    pub fn peek(&self) -> Option<&Fut::Output> {
        if let Some(inner) = self.inner.as_ref() {
            match inner.notifier.state.load(SeqCst) {
                COMPLETE => unsafe { return Some(inner.output()) },
                POISONED => panic!("inner future panicked during poll"),
                _ => {}
            }
        }
        None
    }

    /// Creates a new [`WeakShared`] for this [`Shared`].
    ///
    /// Returns [`None`] if it has already been polled to completion.
    pub fn downgrade(&self) -> Option<WeakShared<Fut>> {
        if let Some(inner) = self.inner.as_ref() {
            return Some(WeakShared(Arc::downgrade(inner)));
        }
        None
    }

    /// Gets the number of strong pointers to this allocation.
    ///
    /// Returns [`None`] if it has already been polled to completion.
    ///
    /// # Safety
    ///
    /// This method by itself is safe, but using it correctly requires extra care. Another thread
    /// can change the strong count at any time, including potentially between calling this method
    /// and acting on the result.
    #[allow(clippy::unnecessary_safety_doc)]
    pub fn strong_count(&self) -> Option<usize> {
        self.inner.as_ref().map(|arc| Arc::strong_count(arc))
    }

    /// Gets the number of weak pointers to this allocation.
    ///
    /// Returns [`None`] if it has already been polled to completion.
    ///
    /// # Safety
    ///
    /// This method by itself is safe, but using it correctly requires extra care. Another thread
    /// can change the weak count at any time, including potentially between calling this method
    /// and acting on the result.
    #[allow(clippy::unnecessary_safety_doc)]
    pub fn weak_count(&self) -> Option<usize> {
        self.inner.as_ref().map(|arc| Arc::weak_count(arc))
    }

    /// Hashes the internal state of this `Shared` in a way that's compatible with `ptr_eq`.
    pub fn ptr_hash<H: Hasher>(&self, state: &mut H) {
        match self.inner.as_ref() {
            Some(arc) => {
                state.write_u8(1);
                ptr::hash(Arc::as_ptr(arc), state);
            }
            None => {
                state.write_u8(0);
            }
        }
    }

    /// Returns `true` if the two `Shared`s point to the same future (in a vein similar to
    /// `Arc::ptr_eq`).
    ///
    /// Returns `false` if either `Shared` has terminated.
    pub fn ptr_eq(&self, rhs: &Self) -> bool {
        let lhs = match self.inner.as_ref() {
            Some(lhs) => lhs,
            None => return false,
        };
        let rhs = match rhs.inner.as_ref() {
            Some(rhs) => rhs,
            None => return false,
        };
        Arc::ptr_eq(lhs, rhs)
    }
}

impl<Fut: ?Sized> Inner<Fut>
where
    Fut: Future,
{
    /// Safety: callers must first ensure that `self.inner.state`
    /// is `COMPLETE`
    unsafe fn output(&self) -> &Fut::Output {
        match unsafe { &*self.future_or_output.get() } {
            FutureOrOutput::Output(item) => item,
            FutureOrOutput::Future(_) => unreachable!(),
        }
    }
}

impl<Fut: ?Sized> Inner<Fut>
where
    Fut: Future,
    Fut::Output: Clone,
{
    /// Registers the current task to receive a wakeup when we are awoken.
    fn record_waker(&self, waker_key: &mut usize, cx: &mut Context<'_>) {
        let mut wakers_guard = self.notifier.wakers.lock().unwrap();

        let wakers = match wakers_guard.as_mut() {
            Some(wakers) => wakers,
            None => return,
        };

        let new_waker = cx.waker();

        if *waker_key == NULL_WAKER_KEY {
            *waker_key = wakers.insert(Some(new_waker.clone()));
        } else {
            match wakers[*waker_key] {
                Some(ref old_waker) if new_waker.will_wake(old_waker) => {}
                // Could use clone_from here, but Waker doesn't specialize it.
                ref mut slot => *slot = Some(new_waker.clone()),
            }
        }
        debug_assert!(*waker_key != NULL_WAKER_KEY);
    }

    /// Safety: callers must first ensure that `inner.state`
    /// is `COMPLETE`
    unsafe fn take_or_clone_output(self: Arc<Self>) -> Fut::Output {
        match Arc::try_unwrap(self) {
            Ok(inner) => match inner.future_or_output.into_inner() {
                FutureOrOutput::Output(item) => item,
                FutureOrOutput::Future(_) => unreachable!(),
            },
            Err(inner) => unsafe { inner.output().clone() },
        }
    }
}

impl<Fut: ?Sized> FusedFuture for SharedBox<Fut>
where
    Fut: Future,
    Fut::Output: Clone,
{
    fn is_terminated(&self) -> bool {
        self.inner.is_none()
    }
}

impl<Fut: ?Sized> Future for SharedBox<Fut>
where
    Fut: Future,
    Fut::Output: Clone,
{
    type Output = Fut::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;

        let inner = this
            .inner
            .take()
            .expect("Shared future polled again after completion");

        // Fast path for when the wrapped future has already completed
        if inner.notifier.state.load(Acquire) == COMPLETE {
            // Safety: We're in the COMPLETE state
            return unsafe { Poll::Ready(inner.take_or_clone_output()) };
        }

        inner.record_waker(&mut this.waker_key, cx);

        match inner
            .notifier
            .state
            .compare_exchange(IDLE, POLLING, SeqCst, SeqCst)
            .unwrap_or_else(|x| x)
        {
            IDLE => {
                // Lock acquired, fall through
            }
            POLLING => {
                // Another task is currently polling, at this point we just want
                // to ensure that the waker for this task is registered
                this.inner = Some(inner);
                return Poll::Pending;
            }
            COMPLETE => {
                // Safety: We're in the COMPLETE state
                return unsafe { Poll::Ready(inner.take_or_clone_output()) };
            }
            POISONED => panic!("inner future panicked during poll"),
            _ => unreachable!(),
        }

        let waker = waker_ref(&inner.notifier);
        let mut cx = Context::from_waker(&waker);

        struct Reset<'a> {
            state: &'a AtomicUsize,
            did_not_panic: bool,
        }

        impl Drop for Reset<'_> {
            fn drop(&mut self) {
                if !self.did_not_panic {
                    self.state.store(POISONED, SeqCst);
                }
            }
        }

        let mut reset = Reset {
            state: &inner.notifier.state,
            did_not_panic: false,
        };

        let output = {
            let future = unsafe {
                match &mut *inner.future_or_output.get() {
                    FutureOrOutput::Future(fut) => Pin::new_unchecked(fut),
                    _ => unreachable!(),
                }
            };

            let poll_result = future.poll(&mut cx);
            reset.did_not_panic = true;

            match poll_result {
                Poll::Pending => {
                    if inner
                        .notifier
                        .state
                        .compare_exchange(POLLING, IDLE, SeqCst, SeqCst)
                        .is_ok()
                    {
                        // Success
                        drop(reset);
                        this.inner = Some(inner);
                        return Poll::Pending;
                    } else {
                        unreachable!()
                    }
                }
                Poll::Ready(output) => output,
            }
        };

        unsafe {
            *inner.future_or_output.get() = FutureOrOutput::Output(output);
        }

        inner.notifier.state.store(COMPLETE, SeqCst);

        // Wake all tasks and drop the slab
        let mut wakers_guard = inner.notifier.wakers.lock().unwrap();
        let mut wakers = wakers_guard.take().unwrap();
        for waker in wakers.drain().flatten() {
            waker.wake();
        }

        drop(reset); // Make borrow checker happy
        drop(wakers_guard);

        // Safety: We're in the COMPLETE state
        unsafe { Poll::Ready(inner.take_or_clone_output()) }
    }
}

impl<Fut: ?Sized> Clone for SharedBox<Fut>
where
    Fut: Future,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            waker_key: NULL_WAKER_KEY,
        }
    }
}

impl<Fut: ?Sized> Drop for SharedBox<Fut>
where
    Fut: Future,
{
    fn drop(&mut self) {
        if self.waker_key != NULL_WAKER_KEY {
            if let Some(ref inner) = self.inner {
                if let Ok(mut wakers) = inner.notifier.wakers.lock() {
                    if let Some(wakers) = wakers.as_mut() {
                        wakers.remove(self.waker_key);
                    }
                }
            }
        }
    }
}

impl ArcWake for Notifier {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        let wakers = &mut *arc_self.wakers.lock().unwrap();
        if let Some(wakers) = wakers.as_mut() {
            for (_key, opt_waker) in wakers {
                if let Some(waker) = opt_waker.take() {
                    waker.wake();
                }
            }
        }
    }
}

impl<Fut: ?Sized + Future> WeakShared<Fut> {
    /// Attempts to upgrade this [`WeakShared`] into a [`Shared`].
    ///
    /// Returns [`None`] if all clones of the [`Shared`] have been dropped or polled
    /// to completion.
    pub fn upgrade(&self) -> Option<SharedBox<Fut>> {
        Some(SharedBox {
            inner: Some(self.0.upgrade()?),
            waker_key: NULL_WAKER_KEY,
        })
    }
}

pub fn parse_accept_language(header: &str) -> Vec<(unic_langid::LanguageIdentifier, f32)> {
    let mut languages = Vec::new();

    for entry in header.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }

        let (lang_tag, quality) = if let Some(semicolon_pos) = entry.find(';') {
            let lang_part = entry[..semicolon_pos].trim();
            let q_part = entry[semicolon_pos + 1..].trim();

            let quality = if let Some(q_value) = q_part.strip_prefix("q=") {
                q_value.parse::<f32>().unwrap_or(1.0).clamp(0.0, 1.0)
            } else {
                1.0
            };

            (lang_part, quality)
        } else {
            (entry, 1.0)
        };

        if let Ok(lang_id) = lang_tag.parse::<unic_langid::LanguageIdentifier>() {
            languages.push((lang_id, quality));
        }
    }

    // Sort by quality value (highest first)
    languages.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    languages
}

pub mod fluent_loader {
    use std::{collections::HashMap, io::Read, sync::Arc};

    use fluent::FluentResource;
    use fluent_bundle::{FluentArgs, concurrent::FluentBundle};
    use unic_langid::LanguageIdentifier;

    use crate::modules::{Context, Error};

    #[derive(Clone)]
    pub struct FluentLoader {
        bundles: HashMap<String, Arc<FluentBundle<FluentResource>>>,
        default_locale: String,
    }

    impl FluentLoader {
        pub fn new(
            context: Arc<Context>,
            pattern: &str,
            default_locale: &str,
        ) -> Result<Self, Error> {
            let mut bundles = HashMap::new();
            let files = context.load_files_glob(pattern)?;

            for (path, mut reader) in files {
                let filename = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .ok_or_else(|| Error("Invalid filename".to_string()))?;

                // Extract language code from filename like "errors-en.ftl" -> "en"
                if let Some(lang_code) = extract_language_code(filename) {
                    let mut content = String::new();
                    reader
                        .read_to_string(&mut content)
                        .map_err(|e| Error(format!("Failed to read file {}: {}", filename, e)))?;

                    // Try to parse the Fluent resource, but continue even if it fails
                    match FluentResource::try_new(content) {
                        Ok(resource) => {
                            let lang_id: LanguageIdentifier = lang_code.parse().map_err(|e| {
                                Error(format!("Invalid language identifier {}: {}", lang_code, e))
                            })?;

                            let mut bundle = FluentBundle::new_concurrent(vec![lang_id]);
                            match bundle.add_resource(resource) {
                                Ok(_) => {
                                    tracing::info!(
                                        "Successfully loaded Fluent resource: {}",
                                        filename
                                    );
                                }
                                Err(errors) => {
                                    // Check if errors are only "Overriding" errors (which are non-fatal)
                                    let non_fatal = errors.iter().all(|e| {
                                        matches!(e, fluent_bundle::FluentError::Overriding { .. })
                                    });
                                    if non_fatal {
                                        tracing::debug!(
                                            "Fluent resource {} has overriding messages (normal for localization): {:?}",
                                            filename,
                                            errors
                                        );
                                    } else {
                                        tracing::warn!(
                                            "Fluent resource {} has errors: {:?}",
                                            filename,
                                            errors
                                        );
                                    }
                                }
                            }
                            // Add the bundle regardless of overriding errors
                            bundles.insert(lang_code, Arc::new(bundle));
                        }
                        Err((_, errors)) => {
                            tracing::warn!(
                                "Failed to parse Fluent resource {}: {} error(s). Skipping this file.",
                                filename,
                                errors.len()
                            );
                            for (i, error) in errors.iter().enumerate() {
                                tracing::warn!("  Error {}: {:?}", i + 1, error);
                            }
                        }
                    }
                }
            }

            if bundles.is_empty() {
                tracing::warn!("No valid Fluent resources loaded from pattern: {}", pattern);
            }

            Ok(Self {
                bundles,
                default_locale: default_locale.to_string(),
            })
        }

        pub fn get_message(
            &self,
            locale: Option<&str>,
            message_id: &str,
            args: Option<&FluentArgs>,
        ) -> Result<(String, String), Error> {
            let locale = locale.unwrap_or(&self.default_locale);

            // If no bundles loaded, fall back to error ID
            if self.bundles.is_empty() {
                tracing::debug!(
                    "No Fluent bundles available, falling back to error ID: {}",
                    message_id
                );
                return Ok((message_id.to_string(), message_id.to_string()));
            }

            let bundle = self
                .bundles
                .get(locale)
                .or_else(|| self.bundles.get(&self.default_locale))
                .or_else(|| self.bundles.values().next()) // Use any available bundle as last resort
                .ok_or_else(|| {
                    Error(format!(
                        "No bundle found for locale {} or default {}",
                        locale, self.default_locale
                    ))
                })?;

            let message = bundle.get_message(message_id).ok_or_else(|| {
                Error(format!(
                    "Message {} not found in locale {}",
                    message_id, locale
                ))
            })?;

            let pattern = message
                .value()
                .ok_or_else(|| Error(format!("Message {} has no value", message_id)))?;

            let title = bundle.format_pattern(pattern, args, &mut vec![]);

            // Try to get description from .desc attribute
            let desc_pattern = message
                .attributes()
                .find(|attr| attr.id() == "desc")
                .and_then(|attr| Some(attr.value()));

            let description = if let Some(desc_pattern) = desc_pattern {
                bundle.format_pattern(desc_pattern, args, &mut vec![])
            } else {
                title.clone()
            };

            Ok((title.into_owned(), description.into_owned()))
        }
    }

    fn extract_language_code(filename: &str) -> Option<String> {
        // Extract language code from filename like "errors-en.ftl" -> "en"
        if let Some(stem) = filename.strip_suffix(".ftl") {
            if let Some(dash_pos) = stem.rfind('-') {
                return Some(stem[dash_pos + 1..].to_string());
            }
        }
        None
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_extract_language_code() {
            assert_eq!(
                extract_language_code("errors-en.ftl"),
                Some("en".to_string())
            );
            assert_eq!(
                extract_language_code("errors-se.ftl"),
                Some("se".to_string())
            );
            assert_eq!(extract_language_code("errors.ftl"), None);
            assert_eq!(
                extract_language_code("errors-en-US.ftl"),
                Some("US".to_string())
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unic_langid::langid;

    #[test]
    fn test_parse_accept_language_simple() {
        let result = parse_accept_language("en-US");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, langid!("en-US"));
        assert_eq!(result[0].1, 1.0);
    }

    #[test]
    fn test_parse_accept_language_with_quality() {
        let result = parse_accept_language("en-US,en;q=0.9,se;q=0.8");
        assert_eq!(result.len(), 3);

        // Should be sorted by quality (highest first)
        assert_eq!(result[0].0, langid!("en-US"));
        assert_eq!(result[0].1, 1.0);

        assert_eq!(result[1].0, langid!("en"));
        assert_eq!(result[1].1, 0.9);

        assert_eq!(result[2].0, langid!("se"));
        assert_eq!(result[2].1, 0.8);
    }

    #[test]
    fn test_parse_accept_language_complex() {
        let result = parse_accept_language("fr-CH, fr;q=0.9, en;q=0.8, de;q=0.7, *;q=0.5");
        assert_eq!(result.len(), 4); // * is not a valid language identifier

        assert_eq!(result[0].0, langid!("fr-CH"));
        assert_eq!(result[0].1, 1.0);

        assert_eq!(result[1].0, langid!("fr"));
        assert_eq!(result[1].1, 0.9);

        assert_eq!(result[2].0, langid!("en"));
        assert_eq!(result[2].1, 0.8);

        assert_eq!(result[3].0, langid!("de"));
        assert_eq!(result[3].1, 0.7);
    }

    #[test]
    fn test_parse_accept_language_empty() {
        let result = parse_accept_language("");
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_parse_accept_language_invalid() {
        let result = parse_accept_language("invalid-lang-tag, en");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, langid!("en"));
        assert_eq!(result[0].1, 1.0);
    }

    #[test]
    fn test_parse_accept_language_quality_bounds() {
        let result = parse_accept_language("en;q=1.5,se;q=-0.1");
        assert_eq!(result.len(), 2);

        // Quality should be clamped to [0.0, 1.0]
        assert_eq!(result[0].1, 1.0); // clamped from 1.5
        assert_eq!(result[1].1, 0.0); // clamped from -0.1
    }

    #[test]
    fn test_parse_accept_language_whitespace() {
        let result = parse_accept_language(" en-US , fr ; q=0.8 , de ");
        assert_eq!(result.len(), 3);

        // Both en-US and de have quality 1.0, fr has quality 0.8
        // So the first two should be the 1.0 quality languages, last should be fr
        assert_eq!(result[0].1, 1.0);
        assert_eq!(result[1].1, 1.0);
        assert_eq!(result[2].0, langid!("fr"));
        assert_eq!(result[2].1, 0.8);

        // Verify that the 1.0 quality languages are present (order may vary)
        let quality_1_langs: std::collections::HashSet<_> =
            result[..2].iter().map(|x| &x.0).collect();
        assert!(quality_1_langs.contains(&langid!("en-US")));
        assert!(quality_1_langs.contains(&langid!("de")));
    }
}
