//! Process-wide sharing of loaded linguistic assets.
//!
//! Every pipeline handle owns its own `Context` over the same bundle, so a
//! `Context`-level cache would miss the point: without a process-wide one,
//! each handle pays a full load of the same model. Each module keeps its own
//! `static` map keyed by [`Context::file_identity`](crate::modules::Context::file_identity);
//! entries are `Weak`, so an asset lives exactly as long as some pipeline
//! uses it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

pub(crate) fn cache_lookup<T>(map: &Mutex<HashMap<String, Weak<T>>>, key: &str) -> Option<Arc<T>> {
    map.lock()
        .expect("core cache lock poisoned")
        .get(key)
        .and_then(Weak::upgrade)
}

/// Insert `fresh` under `key`, unless a live entry raced in first — then the
/// existing core wins, so concurrent first loads still converge on one copy.
pub(crate) fn cache_intern<T>(
    map: &Mutex<HashMap<String, Weak<T>>>,
    key: String,
    fresh: Arc<T>,
) -> Arc<T> {
    let mut map = map.lock().expect("core cache lock poisoned");
    map.retain(|_, w| w.strong_count() > 0);
    match map.entry(key) {
        std::collections::hash_map::Entry::Occupied(mut e) => match e.get().upgrade() {
            Some(existing) => existing,
            None => {
                e.insert(Arc::downgrade(&fresh));
                fresh
            }
        },
        std::collections::hash_map::Entry::Vacant(e) => {
            e.insert(Arc::downgrade(&fresh));
            fresh
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{cache_intern, cache_lookup};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, Weak};

    fn map() -> Mutex<HashMap<String, Weak<String>>> {
        Mutex::new(HashMap::new())
    }

    #[test]
    fn interned_value_is_shared() {
        let m = map();
        let a = cache_intern(&m, "k".into(), Arc::new("v".to_string()));
        let b = cache_lookup(&m, "k").expect("cached entry upgrades");
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn racing_intern_prefers_the_existing_entry() {
        let m = map();
        let first = cache_intern(&m, "k".into(), Arc::new("v1".to_string()));
        let second = cache_intern(&m, "k".into(), Arc::new("v2".to_string()));
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(*second, "v1");
    }

    #[test]
    fn dropped_entries_expire_and_are_pruned() {
        let m = map();
        drop(cache_intern(&m, "dead".into(), Arc::new("v".to_string())));
        assert!(cache_lookup(&m, "dead").is_none());
        let _live = cache_intern(&m, "live".into(), Arc::new("w".to_string()));
        assert_eq!(m.lock().expect("test map lock").len(), 1);
    }
}
