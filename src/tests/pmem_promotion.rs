// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

use super::*;

fn cache() -> (tempfile::TempDir, MultiLayerCache) {
    let dir = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(64, 64, 0)
            .with_ssd_paths([dir.path().join("ssd")])
            .with_pmem_paths([dir.path().join("pmem")]),
    );
    (dir, cache)
}

// rust-internal: metadata must move with a resident value without being duplicated.
#[test]
fn metadata_has_one_authority_across_pmem_and_memory_transitions() {
    let (_dir, cache) = cache();
    let key = CacheKey::string(0, "metadata-authority");
    let mut inner = cache.inner.write().unwrap();

    assert!(inner.put_pmem(&key, b"value".to_vec()));
    let shared = inner.shared_key(&key);
    inner.set_metadata(
        shared,
        CacheEntryMeta::new(
            CacheBlockKind::Other,
            Some(7),
            11,
            13,
            17,
            CacheAdmissionReason::PersistentMemory,
        ),
    );

    let metadata_homes = |inner: &CacheInner| {
        usize::from(
            inner
                .memory
                .get(&key)
                .is_some_and(|resident| resident.meta.is_some()),
        ) + usize::from(
            inner
                .pmem
                .get(&key)
                .is_some_and(|resident| resident.meta.is_some()),
        ) + usize::from(inner.metadata.contains_key(&key))
    };

    assert_eq!(metadata_homes(&inner), 1);
    assert!(inner.pmem.get(&key).unwrap().meta.is_some());
    assert!(!inner.metadata.contains_key(&key));
    assert_eq!(inner.metadata_of(&key).unwrap().hits(), 13);

    let pmem_value = inner.pmem_value(&key).unwrap().clone();
    let shared = inner.shared_key(&key);
    inner.make_resident(shared, pmem_value);
    assert_eq!(metadata_homes(&inner), 1);
    assert!(inner.memory.get(&key).unwrap().meta.is_some());
    assert!(inner.pmem.get(&key).unwrap().meta.is_none());
    assert_eq!(inner.metadata_of(&key).unwrap().hits(), 13);

    inner.take_resident(&key).unwrap();
    assert_eq!(metadata_homes(&inner), 1);
    assert!(inner.pmem.get(&key).unwrap().meta.is_some());
    assert!(!inner.metadata.contains_key(&key));
    assert_eq!(inner.metadata_of(&key).unwrap().hits(), 13);

    inner.take_pmem_resident(&key).unwrap();
    assert_eq!(metadata_homes(&inner), 1);
    assert!(inner.metadata.contains_key(&key));
    assert_eq!(inner.metadata_of(&key).unwrap().hits(), 13);
}

// rust-internal: deterministically interleave mutations between probe and refill.
#[test]
fn stale_pmem_probe_does_not_replace_a_new_value() {
    let (_dir, cache) = cache();
    let key = CacheKey::string(0, "key");
    let mut inner = cache.inner.write().unwrap();
    assert!(inner.put_pmem(&key, b"old".to_vec()));
    let old = inner.pmem_value(&key).unwrap().clone();
    assert!(inner.put_pmem(&key, b"new".to_vec()));
    inner.promote_pmem_probe_if_current(&key, &old);
    assert!(inner.resident_value(&key).is_none());
    let current = inner.pmem_value(&key).unwrap().clone();
    inner.promote_pmem_probe_if_current(&key, &current);
    assert!(Arc::ptr_eq(inner.resident_value(&key).unwrap(), &current));
    assert!(Arc::ptr_eq(inner.pmem.get(&key).unwrap(), &current));
    assert!(inner.put_memory_shared(&key, Arc::from(&b"newer"[..])));
    inner.promote_pmem_probe_if_current(&key, &current);
    assert_eq!(inner.resident_value(&key).unwrap().as_ref(), b"newer");
    assert_eq!(inner.stats.refill_failures, 0);
}

// rust-internal: an invalidated probe must not recreate cache state.
#[test]
fn invalidated_pmem_probe_does_not_resurrect_an_entry() {
    let (_dir, cache) = cache();
    let key = CacheKey::string(0, "key");
    let mut inner = cache.inner.write().unwrap();
    assert!(inner.put_pmem(&key, b"old".to_vec()));
    let old = inner.pmem_value(&key).unwrap().clone();
    inner.invalidate_key_locked(&key, true);
    inner.promote_pmem_probe_if_current(&key, &old);
    assert!(inner.resident_value(&key).is_none());
    assert!(!inner.pmem.contains_key(&key));
    assert_eq!(inner.stats.refill_failures, 0);
}

// rust-internal: lifecycle and expiry can change while a captured value is held.
#[test]
fn pmem_probe_promotion_respects_stop_expiry_and_storage_only_mode() {
    let (_dir, cache) = cache();
    let key = CacheKey::string(0, "key");
    let mut inner = cache.inner.write().unwrap();
    assert!(inner.put_pmem(&key, b"value".to_vec()));
    let value = inner.pmem_value(&key).unwrap().clone();
    inner.started = false;
    inner.promote_pmem_probe_if_current(&key, &value);
    assert!(inner.resident_value(&key).is_none());
    inner.started = true;
    inner.ssd_instance_only = true;
    inner.promote_pmem_probe_if_current(&key, &value);
    assert!(inner.resident_value(&key).is_none());
    inner.ssd_instance_only = false;
    inner.ttl_in_use = true;
    inner.set_entry_expiry(&key, 1, 0);
    *inner.expiries.get_mut(&key).unwrap() = CoarseClock::now_millis().saturating_sub(1);
    assert!(inner.entry_expired(&key, CoarseClock::now_millis()));
    inner.promote_pmem_probe_if_current(&key, &value);
    assert!(inner.resident_value(&key).is_none());
}
