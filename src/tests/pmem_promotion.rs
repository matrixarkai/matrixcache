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

// rust-internal: shared PMEM accounting is carried into promotion exactly once.
#[test]
fn accounted_pmem_promotion_preserves_one_hit_and_one_metadata_authority() {
    let (_dir, cache) = cache();
    let key = CacheKey::string(0, "accounted-promotion");
    let mut inner = cache.inner.write().unwrap();
    assert!(inner.put_pmem(&key, b"value".to_vec()));
    let shared = inner.shared_key(&key);
    inner.set_metadata(
        shared,
        CacheEntryMeta::new(
            CacheBlockKind::Other,
            None,
            1,
            0,
            CoarseClock::now_millis(),
            CacheAdmissionReason::PersistentMemory,
        ),
    );
    let value = inner.pmem_value(&key).unwrap().clone();
    let outcome = inner.record_known_hit_shared_occurrences_at(
        &key,
        inner.pmem.get(&key).unwrap().meta.as_ref(),
        1,
        CoarseClock::now_millis(),
    );
    assert_eq!(inner.metadata_of(&key).unwrap().hits(), 1);

    inner.promote_accounted_pmem_probe_if_current(&key, &value, outcome);

    assert_eq!(inner.metadata_of(&key).unwrap().hits(), 1);
    assert!(inner.memory.get(&key).unwrap().meta.is_some());
    assert!(inner.pmem.get(&key).unwrap().meta.is_none());
    assert!(!inner.metadata.contains_key(&key));
}

// rust-internal: reads do not retry a memory placement that cannot succeed.
#[test]
fn pmem_reads_skip_structurally_impossible_memory_refills() {
    for (memory_capacity, threshold, value_len) in [
        (0_usize, usize::MAX, 8_usize),
        (4, usize::MAX, 8),
        (64, 4, 8),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_options(
            CacheOptions::new(memory_capacity, 64, 0)
                .with_ssd_paths([dir.path().join("ssd")])
                .with_pmem_paths([dir.path().join("pmem")]),
        );
        let key = CacheKey::string(0, &format!("no-refill-{memory_capacity}-{threshold}"));
        let mut inner = cache.inner.write().unwrap();
        if threshold != usize::MAX {
            inner.tiering_policy.data_placement = CacheDataPlacement::SideBySide;
            inner.tiering_policy.data_placement_threshold_bytes = threshold;
        }
        assert!(inner.put_pmem(&key, vec![b'v'; value_len]));
        let value = inner.pmem_value(&key).unwrap().clone();
        let failures = inner.stats.refill_failures;
        let rejections = inner.stats.memory_admission_rejected;

        inner.promote_pmem_probe_if_current(&key, &value);

        assert!(inner.resident_value(&key).is_none());
        assert_eq!(inner.stats.refill_failures, failures);
        assert_eq!(inner.stats.memory_admission_rejected, rejections);
        assert_eq!(inner.metadata_of(&key).unwrap().hits(), 1);
    }
}

// rust-internal: changing the side-by-side threshold must not turn PMEM reads
// into memory promotions for entries placed under the previous threshold.
#[test]
fn side_by_side_pmem_hit_stays_in_pmem_after_threshold_change() {
    let (_dir, cache) = cache();
    cache.start().unwrap();
    let key = CacheKey::string(0, "placement-change");
    let mut policy = cache.production_tiering_policy();
    policy.data_placement = CacheDataPlacement::SideBySide;
    policy.data_placement_threshold_bytes = 4;
    cache.update_production_tiering_policy(policy);
    cache.put(key.clone(), b"12345678".to_vec()).unwrap();
    assert_eq!(cache.peek_tier(&key), Some(CacheReadTier::Pmem));

    cache.set_data_placement_threshold_bytes(16);
    assert_eq!(
        cache.get_shared(&key).unwrap().unwrap().as_ref(),
        b"12345678"
    );
    assert_eq!(cache.peek_tier(&key), Some(CacheReadTier::Pmem));
    assert_eq!(cache.size_for_tier(CacheTier::Memory), 0);
}

// rust-internal: PMEM-only shared reads stay lock-light and do not report false refill failures.
#[test]
fn pmem_only_public_reads_stay_in_pmem_without_refill_failures() {
    let dir = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(0, 4096, 0)
            .with_ssd_paths([dir.path().join("ssd")])
            .with_pmem_paths([dir.path().join("pmem")]),
    );
    cache.start().unwrap();
    let mut policy = cache.production_tiering_policy();
    policy.data_placement = CacheDataPlacement::SideBySide;
    policy.data_placement_threshold_bytes = 0;
    cache.update_production_tiering_policy(policy);
    let keys = (0..4)
        .map(|index| CacheKey::string(0, &format!("pmem-only-{index}")))
        .collect::<Vec<_>>();
    for (index, key) in keys.iter().enumerate() {
        cache.put(key.clone(), vec![index as u8; 64]).unwrap();
        assert_eq!(cache.peek_tier(key), Some(CacheReadTier::Pmem));
    }
    let before = cache.stats();

    for key in &keys {
        assert!(cache.get_shared(key).unwrap().is_some());
    }
    let before_batch = cache.stats();
    assert!(cache
        .get_shared_batch(&keys)
        .unwrap()
        .iter()
        .all(Option::is_some));
    let after_batch = cache.stats();
    assert_eq!(after_batch.pmem_hits - before_batch.pmem_hits, 4);
    assert_eq!(
        after_batch.shared_buffer_hits - before_batch.shared_buffer_hits,
        4
    );
    assert_eq!(
        after_batch.get_latency_samples - before_batch.get_latency_samples,
        4
    );
    assert_eq!(
        after_batch.read_through_latency_samples - before_batch.read_through_latency_samples,
        4
    );

    let after = after_batch;
    assert_eq!(after.refill_failures, before.refill_failures);
    assert_eq!(
        after.memory_admission_rejected,
        before.memory_admission_rejected
    );
    assert_eq!(after.memory_fills, before.memory_fills);
    assert!(keys
        .iter()
        .all(|key| cache.peek_tier(key) == Some(CacheReadTier::Pmem)));
}

// rust-internal: switching to tiered placement refills owned PMEM batches.
#[test]
fn owned_pmem_batch_refills_memory_after_switch_to_tiered() {
    let dir = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(8192, 8192, 0)
            .with_pmem_paths([dir.path().join("pmem")])
            .with_dram_pmem_data_placement(CacheDataPlacement::SideBySide, 0),
    );
    cache.start().unwrap();
    let keys = (0..8)
        .map(|index| CacheKey::string(0, &format!("pmem-owned-batch-{index}")))
        .collect::<Vec<_>>();
    for (index, key) in keys.iter().enumerate() {
        cache.put(key.clone(), vec![index as u8; 64]).unwrap();
        assert_eq!(cache.peek_tier(key), Some(CacheReadTier::Pmem));
    }
    cache.set_data_placement(CacheDataPlacement::Tiered);

    let before = cache.stats();
    let values = cache.get_batch(&keys).unwrap();
    for (index, value) in values.iter().enumerate() {
        let value = value.as_ref().expect("PMEM batch value");
        assert_eq!(value.len(), 64);
        assert!(value.iter().all(|byte| *byte == index as u8));
    }

    let after = cache.stats();
    assert_eq!(after.pmem_hits - before.pmem_hits, 8);
    assert_eq!(after.misses, before.misses);
    assert_eq!(after.memory_fills - before.memory_fills, 8);
    assert_eq!(after.refill_failures, before.refill_failures);
    assert_eq!(after.get_latency_samples - before.get_latency_samples, 8);
    assert_eq!(
        after.read_through_latency_samples - before.read_through_latency_samples,
        8
    );
    assert_eq!(
        after.refill_latency_samples - before.refill_latency_samples,
        8
    );
    assert!(keys
        .iter()
        .all(|key| cache.peek_tier(key) == Some(CacheReadTier::Memory)));
}

// rust-internal: mixed resident batches refill after switching to tiered placement.
#[test]
fn owned_mixed_memory_pmem_batch_refills_after_switch_to_tiered() {
    let dir = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(8192, 8192, 0)
            .with_pmem_paths([dir.path().join("pmem")])
            .with_dram_pmem_data_placement(CacheDataPlacement::SideBySide, usize::MAX),
    );
    cache.start().unwrap();
    let keys = (0..8)
        .map(|index| CacheKey::string(0, &format!("mixed-owned-batch-{index}")))
        .collect::<Vec<_>>();
    for (index, key) in keys.iter().enumerate() {
        cache.set_data_placement_threshold_bytes(if index % 2 == 0 { usize::MAX } else { 0 });
        cache.put(key.clone(), vec![index as u8; 64]).unwrap();
    }
    for (index, key) in keys.iter().enumerate() {
        assert_eq!(
            cache.peek_tier(key),
            Some(if index % 2 == 0 {
                CacheReadTier::Memory
            } else {
                CacheReadTier::Pmem
            })
        );
    }
    cache.set_data_placement(CacheDataPlacement::Tiered);

    let before = cache.stats();
    let values = cache.get_batch(&keys).unwrap();
    for (index, value) in values.iter().enumerate() {
        let value = value.as_ref().expect("mixed resident value");
        assert_eq!(value.len(), 64);
        assert!(value.iter().all(|byte| *byte == index as u8));
    }

    let after = cache.stats();
    assert_eq!(after.memory_hits - before.memory_hits, 4);
    assert_eq!(after.pmem_hits - before.pmem_hits, 4);
    assert_eq!(after.shared_buffer_hits - before.shared_buffer_hits, 8);
    assert_eq!(after.memory_fills - before.memory_fills, 4);
    assert_eq!(after.misses, before.misses);
    assert_eq!(after.refill_failures, before.refill_failures);
    assert_eq!(after.get_latency_samples - before.get_latency_samples, 8);
    assert_eq!(
        after.refill_latency_samples - before.refill_latency_samples,
        4
    );
    assert!(keys
        .iter()
        .all(|key| cache.peek_tier(key) == Some(CacheReadTier::Memory)));
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
