//! Process-wide sharing of loaded linguistic assets.
//!
//! Every pipeline handle owns its own `Context` over the same bundle, so a
//! `Context`-level cache would miss the point: without a process-wide one,
//! each handle pays a full load of the same model. Each module keeps its own
//! `static` map keyed by [`Context::file_identity`](crate::modules::Context::file_identity);
//! entries are `Weak`, so an asset lives exactly as long as some pipeline
//! uses it.
//!
//! Loads are single-flight per key. A host growing a pool builds its handles
//! concurrently, and a lookup-then-insert cache turns that into a thundering
//! herd: every handle misses before any has interned, and each pays the full
//! load — the copies converge afterwards, but the transient peak is N times
//! the asset and can be gigabytes. Here the first caller loads under the
//! key's own lock and everyone else waits for the result instead.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

/// One cache: each key owns a slot whose lock serialises loading, so waiting
/// on one asset never blocks loading another.
pub(crate) type AssetCache<T> = Mutex<HashMap<String, Arc<Mutex<Weak<T>>>>>;

fn slot<T>(map: &AssetCache<T>, key: String) -> Arc<Mutex<Weak<T>>> {
    let mut map = map.lock().expect("asset cache map lock poisoned");
    // Prune slots whose asset is gone and nobody is loading: a dead Weak with
    // no other Arc to the slot means no caller is inside it.
    map.retain(|_, s| Arc::strong_count(s) > 1 || s.lock().is_ok_and(|w| w.strong_count() > 0));
    Arc::clone(map.entry(key).or_default())
}

/// The live asset under `key`, if some pipeline holds it — never loads and
/// never waits out a load in flight. The fast path for callers whose load
/// needs async preparation (an mmap) before [`cache_get_or_load`] can run:
/// peek first, prepare only on miss. The race between the peek and the load
/// call is harmless — the load is single-flight either way.
pub(crate) fn cache_peek<T>(map: &AssetCache<T>, key: &str) -> Option<Arc<T>> {
    let map = map.lock().expect("asset cache map lock poisoned");
    let slot = map.get(key)?;
    let weak = slot.try_lock().ok()?;
    weak.upgrade()
}

/// The live asset under `key`, loading it if no caller has yet — or if every
/// pipeline that used it has since dropped it. Concurrent callers for one key
/// wait for the first's load rather than repeating it; a failed load releases
/// the key, so the next caller retries. The wait blocks the thread, so call
/// this where the load itself would be acceptable — a `spawn_blocking`
/// closure, or a constructor already loading synchronously.
pub(crate) fn cache_get_or_load<T, E>(
    map: &AssetCache<T>,
    key: String,
    load: impl FnOnce() -> Result<Arc<T>, E>,
) -> Result<Arc<T>, E> {
    let slot = slot(map, key);
    let mut weak = slot
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(live) = weak.upgrade() {
        return Ok(live);
    }
    let fresh = load()?;
    *weak = Arc::downgrade(&fresh);
    Ok(fresh)
}

#[cfg(test)]
mod tests {
    use super::{AssetCache, cache_get_or_load};
    use std::convert::Infallible;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier, Mutex};

    fn map() -> AssetCache<String> {
        Mutex::new(std::collections::HashMap::new())
    }

    fn ok(value: &str) -> Result<Arc<String>, Infallible> {
        Ok(Arc::new(value.to_string()))
    }

    #[test]
    fn loaded_value_is_shared() {
        let m = map();
        let a = cache_get_or_load(&m, "k".into(), || ok("v")).unwrap();
        let b = cache_get_or_load(&m, "k".into(), || ok("other")).unwrap();
        assert!(Arc::ptr_eq(&a, &b));
        assert_eq!(*b, "v");
    }

    #[test]
    fn dropped_entries_expire_and_are_pruned() {
        let m = map();
        drop(cache_get_or_load(&m, "dead".into(), || ok("v")).unwrap());
        let reloaded = cache_get_or_load(&m, "dead".into(), || ok("w")).unwrap();
        assert_eq!(*reloaded, "w");
        let _live = cache_get_or_load(&m, "live".into(), || ok("x")).unwrap();
        assert_eq!(m.lock().expect("test map lock").len(), 2);
        drop(reloaded);
        let _pruned = cache_get_or_load(&m, "live".into(), || ok("y")).unwrap();
        assert_eq!(m.lock().expect("test map lock").len(), 1);
    }

    #[test]
    fn a_failed_load_releases_the_key_for_a_retry() {
        let m = map();
        let failed: Result<Arc<String>, &str> = cache_get_or_load(&m, "k".into(), || Err("no"));
        assert_eq!(failed.unwrap_err(), "no");
        let retried = cache_get_or_load(&m, "k".into(), || Ok::<_, &str>(Arc::new("v".into())));
        assert_eq!(*retried.unwrap(), "v");
    }

    /// The single-flight property this module exists for: N racing callers of
    /// one key produce ONE load, not N loads that converge afterwards.
    #[test]
    fn racing_callers_share_one_load() {
        const CALLERS: usize = 8;
        static MAP: std::sync::LazyLock<AssetCache<String>> =
            std::sync::LazyLock::new(Default::default);
        let loads = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(CALLERS));

        let handles: Vec<_> = (0..CALLERS)
            .map(|_| {
                let loads = Arc::clone(&loads);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    cache_get_or_load(&MAP, "k".into(), || {
                        loads.fetch_add(1, Ordering::SeqCst);
                        std::thread::sleep(std::time::Duration::from_millis(50));
                        Ok::<_, Infallible>(Arc::new("v".to_string()))
                    })
                    .unwrap()
                })
            })
            .collect();

        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(loads.load(Ordering::SeqCst), 1);
        assert!(results.windows(2).all(|w| Arc::ptr_eq(&w[0], &w[1])));
    }
}
