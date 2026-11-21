// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

use super::*;

fn populate(directory: &Path, spec: u8) -> (MultiLayerCache, Vec<CacheKey>) {
    let cache = MultiLayerCache::with_options(CacheOptions {
        dram_capacity: 512,
        pmem_capacity: 2048,
        pmem_paths: vec![directory.join("pmem")],
        ssd_capacity: 1 << 20,
        ssd_paths: vec![directory.join("ssd")],
        ..CacheOptions::default()
    });
    cache.start().unwrap();
    cache.set_ssd_admission_filter_enabled(false);
    {
        let mut inner = cache.inner.write().unwrap();
        inner.memory_order.set_insertion_spec(spec);
        inner.pmem_order.set_insertion_spec(spec);
        inner.disk_order.set_insertion_spec(spec);
    }
    let keys = (0..96)
        .map(|index| {
            CacheKey::page_with_slot(
                (index % 3) as u64,
                (index % 4) as u64,
                index * 96,
                96,
                Some((index % 5) as u32),
            )
        })
        .collect::<Vec<_>>();
    for (index, key) in keys.iter().enumerate() {
        cache.put(key.clone(), vec![index as u8; 96]).unwrap();
    }
    let entries = cache.all_entries();
    assert!(entries.iter().any(|entry| entry.memory_bytes > 0));
    assert!(entries.iter().any(|entry| entry.pmem_bytes > 0));
    assert!(entries.iter().any(|entry| entry.disk_bytes > 0));
    (cache, keys)
}

#[test]
fn matching_inspection_preserves_rows_order_pins_and_stats() {
    let directory = tempfile::tempdir().unwrap();
    let (cache, keys) = populate(directory.path(), 0);
    cache.pin(keys[3].clone());
    let before = cache.stats();
    let full = cache.entries_for_shard(0);
    let expected = full
        .iter()
        .filter(|entry| entry.selector.starts_with("slot-3:"))
        .cloned()
        .collect::<Vec<_>>();
    assert!(!expected.is_empty() && expected.len() < full.len());
    assert!(expected.iter().any(|entry| entry.pinned));
    let actual = cache.entries_for_shard_matching(0, |key| key.selector.starts_with("slot-3:"));
    assert_eq!(actual, expected);
    assert_eq!(cache.entries_for_shard_matching(0, |_| true), full);
    assert!(cache.entries_for_shard_matching(0, |_| false).is_empty());
    assert!(cache.entries_for_shard_matching(99, |_| true).is_empty());
    let after = cache.stats();
    assert_eq!(before.memory_hits, after.memory_hits);
    assert_eq!(before.pmem_hits, after.pmem_hits);
    assert_eq!(before.disk_hits, after.disk_hits);
    assert_eq!(before.misses, after.misses);
    assert_eq!(before.invalidations, after.invalidations);
    cache.unpin(&keys[3]);
    cache.stop();
}

#[test]
fn unrelated_memory_entries_do_not_trigger_inspection_disk_probes() {
    for count in [64, 8192] {
        let directory = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_options(
            CacheOptions::new(4 << 20, 0, 0).with_ssd_paths([directory.path().to_path_buf()]),
        );
        for index in 0..count {
            cache.put_memory_only(CacheKey::string(1, &format!("row-{index:05}")), vec![7; 16]);
        }
        INSPECTION_DISK_METADATA_PROBES.with(|probes| probes.set(0));
        let selected =
            cache.entries_for_shard_matching(1, |key| key.record_key.as_ref() == "row-00000");
        assert_eq!(selected.len(), 1);
        assert_eq!(
            INSPECTION_DISK_METADATA_PROBES.with(|probes| probes.get()),
            1
        );
        INSPECTION_DISK_METADATA_PROBES.with(|probes| probes.set(0));
        assert_eq!(cache.entries_for_shard(1).len(), count);
        assert_eq!(
            INSPECTION_DISK_METADATA_PROBES.with(|probes| probes.get()),
            count as u64
        );
    }
}

#[test]
fn sharded_matching_inspection_keeps_the_full_listings_sort_order() {
    let directory = tempfile::tempdir().unwrap();
    let cache = ShardedMultiLayerCache::with_options(
        CacheOptions::new(1 << 20, 0, 0).with_ssd_paths([directory.path().to_path_buf()]),
        4,
    );
    // Each cache shard participates, including a repeated key in two shards.
    for (index, shard) in cache.shards.iter().enumerate() {
        for name in ["shared", "wanted", "other"] {
            shard.put_memory_only(
                CacheKey::string(1, &format!("{name}-{index}")),
                vec![index as u8],
            );
        }
        shard.put_memory_only(CacheKey::string(1, "wanted-shared"), vec![1]);
        shard.put_memory_only(CacheKey::string(2, "wanted-other-shard"), vec![2]);
    }
    let expected = cache
        .entries_for_shard(1)
        .into_iter()
        .filter(|entry| entry.record_key.starts_with("wanted"))
        .collect::<Vec<_>>();
    assert_eq!(
        cache.entries_for_shard_matching(1, |key| key.record_key.starts_with("wanted")),
        expected
    );
}

fn orders(cache: &MultiLayerCache) -> [Vec<CacheKey>; 6] {
    let inner = cache.inner.read().unwrap();
    [
        inner.memory_order.iter().cloned().collect(),
        inner.memory_order.iter_access().cloned().collect(),
        inner.pmem_order.iter().cloned().collect(),
        inner.pmem_order.iter_access().cloned().collect(),
        inner.disk_order.iter().cloned().collect(),
        inner.disk_order.iter_access().cloned().collect(),
    ]
}

#[test]
fn targeted_invalidations_unlink_only_named_entries_from_every_order() {
    for spec in [0, 2] {
        for slot in [true, false] {
            let directory = tempfile::tempdir().unwrap();
            let (cache, keys) = populate(directory.path(), spec);
            let before = orders(&cache);
            let remove = |key: &CacheKey| {
                key.shard_id == 0
                    && if slot {
                        key.selector.starts_with("slot-3:")
                    } else {
                        key.namespace == "page"
                            && key.record_key.as_ref() == "segment-00000000000000000003"
                    }
            };
            assert!(keys.iter().any(remove));
            let expected = before.map(|rows| {
                rows.into_iter()
                    .filter(|key| !remove(key))
                    .collect::<Vec<_>>()
            });
            CACHE_ORDER_RETAIN_VISITS.with(|visits| visits.set(0));
            if slot {
                cache.invalidate_slot(0, 3).unwrap();
            } else {
                cache.invalidate_page_segment(0, 3).unwrap();
            }
            assert_eq!(
                CACHE_ORDER_RETAIN_VISITS.with(|visits| visits.get()),
                0,
                "targeted invalidation must not sweep an LRU list"
            );
            assert_eq!(
                orders(&cache),
                expected,
                "slot={slot} insertion_spec={spec}"
            );
            for (index, key) in keys.iter().enumerate() {
                let value = cache.get(key).unwrap();
                assert_eq!(value, (!remove(key)).then(|| vec![index as u8; 96]));
            }
            cache.stop();
        }
    }
}

#[test]
fn batched_slots_walk_each_tier_key_once_and_match_single_slot_results() {
    for spec in [0, 2] {
        let left_dir = tempfile::tempdir().unwrap();
        let right_dir = tempfile::tempdir().unwrap();
        let (batched, keys) = populate(left_dir.path(), spec);
        let (single, _) = populate(right_dir.path(), spec);
        let selected = [1, 3, 1, u32::MAX];
        let remove = |key: &CacheKey| {
            key.shard_id == 0
                && ["slot-1:", "slot-3:", "slot-4294967295:"]
                    .iter()
                    .any(|prefix| key.selector.starts_with(prefix))
        };
        let expected_batched = orders(&batched).map(|rows| {
            rows.into_iter()
                .filter(|key| !remove(key))
                .collect::<Vec<_>>()
        });
        let expected_single = orders(&single).map(|rows| {
            rows.into_iter()
                .filter(|key| !remove(key))
                .collect::<Vec<_>>()
        });
        let expected_removal = |cache: &MultiLayerCache| {
            let inner = cache.inner.read().unwrap();
            let unique = inner
                .memory
                .keys()
                .chain(inner.pmem.keys())
                .chain(inner.disk_index.keys())
                .filter(|key| remove(key))
                .collect::<BTreeSet<_>>()
                .len();
            (
                inner.memory.keys().filter(|key| remove(key)).count(),
                inner
                    .disk_index
                    .iter()
                    .filter(|(key, _)| remove(key))
                    .map(|(_, bytes)| *bytes)
                    .sum::<u64>(),
                inner.stats.invalidations + unique as u64,
            )
        };
        let expected_batch_counts = expected_removal(&batched);
        let expected_single_counts = expected_removal(&single);
        let before = batched.inner.read().unwrap();
        let visits = before.memory.len() + before.pmem.len() + before.disk_index.len();
        drop(before);
        SLOT_INVALIDATION_KEY_VISITS.with(|count| count.set(0));
        let report = batched.invalidate_slots(0, &selected).unwrap();
        assert_eq!(
            SLOT_INVALIDATION_KEY_VISITS.with(|count| count.get()),
            visits as u64
        );
        let mut memory_removed = 0;
        let mut disk_removed = 0;
        for slot in [1, 3, u32::MAX] {
            let report = single.invalidate_slot(0, slot).unwrap();
            memory_removed += report.memory_entries_removed;
            disk_removed += report.disk_bytes_removed;
        }
        assert_eq!(report.shard_id, 0);
        assert_eq!(report.memory_entries_removed, expected_batch_counts.0);
        assert_eq!(report.disk_bytes_removed, expected_batch_counts.1);
        assert_eq!(memory_removed, expected_single_counts.0);
        assert_eq!(disk_removed, expected_single_counts.1);
        assert_eq!(
            orders(&batched),
            expected_batched,
            "batch insertion_spec={spec}"
        );
        assert_eq!(
            orders(&single),
            expected_single,
            "single insertion_spec={spec}"
        );
        assert_eq!(batched.stats().invalidations, expected_batch_counts.2);
        assert_eq!(single.stats().invalidations, expected_single_counts.2);
        for (index, key) in keys.iter().enumerate() {
            let expected = (!remove(key)).then(|| vec![index as u8; 96]);
            assert_eq!(batched.get(key).unwrap(), expected);
            assert_eq!(single.get(key).unwrap(), expected);
        }
        batched.stop();
        single.stop();
    }
}

#[test]
fn batched_slot_selection_keeps_exact_prefix_and_shard_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::new(1 << 20, dir.path());
    let selectors = [
        "slot-3:x",
        "slot-30:x",
        "slot-03:x",
        "slot-3",
        "slot-3:",
        "slot-4294967295:x",
        "slot-42949672950:x",
        "slot-3:x:y",
    ];
    for shard in [1, 2] {
        for (index, selector) in selectors.iter().enumerate() {
            let mut key = CacheKey::string(shard, &format!("key-{index}"));
            key.selector = (*selector).into();
            cache.put_memory_only(key, vec![index as u8]);
        }
    }
    let report = cache.invalidate_slots(1, &[3, u32::MAX, 3]).unwrap();
    assert_eq!(report.memory_entries_removed, 4);
    for shard in [1, 2] {
        for (index, selector) in selectors.iter().enumerate() {
            let mut key = CacheKey::string(shard, &format!("key-{index}"));
            key.selector = (*selector).into();
            let removed = shard == 1
                && (selector.starts_with("slot-3:") || selector.starts_with("slot-4294967295:"));
            assert_eq!(
                cache.get(&key).unwrap(),
                (!removed).then(|| vec![index as u8])
            );
        }
    }
}

#[test]
fn empty_slot_batch_does_not_scan_or_publish_a_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::new(1 << 20, dir.path());
    let key = CacheKey::page_with_slot(1, 1, 0, 3, Some(3));
    cache.put_memory_only(key.clone(), b"hot".to_vec());
    #[cfg(not(feature = "rocksdb-ssd"))]
    {
        let manifest = cache.inner.read().unwrap().manifest_path();
        fs::create_dir_all(&manifest).unwrap();
    }
    SLOT_INVALIDATION_KEY_VISITS.with(|count| count.set(0));
    let report = cache.invalidate_slots(1, &[]).unwrap();
    assert_eq!(report.memory_entries_removed, 0);
    assert_eq!(report.disk_bytes_removed, 0);
    assert_eq!(SLOT_INVALIDATION_KEY_VISITS.with(|count| count.get()), 0);
    assert_eq!(cache.get(&key).unwrap(), Some(b"hot".to_vec()));
}

#[test]
#[cfg(not(feature = "rocksdb-ssd"))]
fn batched_slot_callbacks_keep_values_even_when_manifest_publish_fails() {
    let dir = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::new(1 << 20, dir.path());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let output = Arc::clone(&seen);
    cache.register_eviction_callback(move |record| {
        output
            .lock()
            .unwrap()
            .push((record.key.clone(), record.cause, record.value.clone()));
    });
    let keys = [3, 7].map(|slot| CacheKey::page_with_slot(1, u32::MAX as u64, 0, 3, Some(slot)));
    for (index, key) in keys.iter().enumerate() {
        cache.put_memory_only(key.clone(), vec![index as u8; 3]);
    }
    let manifest = cache.inner.read().unwrap().manifest_path();
    fs::create_dir_all(&manifest).unwrap();
    assert!(cache.invalidate_slots(1, &[3, 7, 3]).is_err());
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    for (index, key) in keys.iter().enumerate() {
        assert!(seen.iter().any(|(actual, cause, value)| actual == key
            && *cause == CacheRemovalCause::Evicted
            && *value == vec![index as u8; 3]));
        assert!(!cache.inner.read().unwrap().memory.contains_key(key));
    }
}

#[test]
fn sharded_slot_batch_removes_each_selected_value_and_keeps_other_shards() {
    let dir = tempfile::tempdir().unwrap();
    let cache = ShardedMultiLayerCache::with_options(
        CacheOptions::new(1 << 20, 0, 0).with_ssd_paths([dir.path().to_path_buf()]),
        4,
    );
    let seen = Arc::new(Mutex::new(Vec::new()));
    for (index, shard) in cache.shards.iter().enumerate() {
        let output = Arc::clone(&seen);
        shard.register_eviction_callback(move |record| {
            output
                .lock()
                .unwrap()
                .push((record.key.clone(), record.value.clone()));
        });
        for logical_shard in [1, 2] {
            for slot in [3, 7, 9] {
                let key = CacheKey::page_with_slot(logical_shard, index as u64, 0, 3, Some(slot));
                shard.put_memory_only(key, vec![index as u8; 3]);
            }
        }
    }
    let report = cache.invalidate_slots(1, &[3, 7, 3]).unwrap();
    assert_eq!(report.memory_entries_removed, 8);
    assert_eq!(seen.lock().unwrap().len(), 8);
    assert_eq!(cache.entries_for_shard(1).len(), 4);
    assert_eq!(cache.entries_for_shard(2).len(), 12);
    assert!(cache
        .entries_for_shard(1)
        .iter()
        .all(|entry| entry.selector.starts_with("slot-9:")));
    assert_eq!(
        cache
            .invalidate_slots(1, &[])
            .unwrap()
            .memory_entries_removed,
        0
    );
}
