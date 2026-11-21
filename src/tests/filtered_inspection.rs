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
    let keys = (0..96).map(|index| {
        CacheKey::page_with_slot((index % 3) as u64, (index % 4) as u64,
                                index * 96, 96, Some((index % 5) as u32))
    }).collect::<Vec<_>>();
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
    let expected = full.iter().filter(|entry| entry.selector.starts_with("slot-3:"))
        .cloned().collect::<Vec<_>>();
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
        let selected = cache.entries_for_shard_matching(1, |key| key.record_key.as_ref() == "row-00000");
        assert_eq!(selected.len(), 1);
        assert_eq!(INSPECTION_DISK_METADATA_PROBES.with(|probes| probes.get()), 1);
        INSPECTION_DISK_METADATA_PROBES.with(|probes| probes.set(0));
        assert_eq!(cache.entries_for_shard(1).len(), count);
        assert_eq!(INSPECTION_DISK_METADATA_PROBES.with(|probes| probes.get()), count as u64);
    }
}

#[test]
fn sharded_matching_inspection_keeps_the_full_listings_sort_order() {
    let directory = tempfile::tempdir().unwrap();
    let cache = ShardedMultiLayerCache::with_options(
        CacheOptions::new(1 << 20, 0, 0).with_ssd_paths([directory.path().to_path_buf()]), 4,
    );
    // Each cache shard participates, including a repeated key in two shards.
    for (index, shard) in cache.shards.iter().enumerate() {
        for name in ["shared", "wanted", "other"] {
            shard.put_memory_only(CacheKey::string(1, &format!("{name}-{index}")), vec![index as u8]);
        }
        shard.put_memory_only(CacheKey::string(1, "wanted-shared"), vec![1]);
        shard.put_memory_only(CacheKey::string(2, "wanted-other-shard"), vec![2]);
    }
    let expected = cache.entries_for_shard(1).into_iter()
        .filter(|entry| entry.record_key.starts_with("wanted")).collect::<Vec<_>>();
    assert_eq!(cache.entries_for_shard_matching(1, |key| key.record_key.starts_with("wanted")), expected);
}

fn orders(cache: &MultiLayerCache) -> [Vec<CacheKey>; 6] {
    let inner = cache.inner.read().unwrap();
    [inner.memory_order.iter().cloned().collect(),
     inner.memory_order.iter_access().cloned().collect(),
     inner.pmem_order.iter().cloned().collect(),
     inner.pmem_order.iter_access().cloned().collect(),
     inner.disk_order.iter().cloned().collect(),
     inner.disk_order.iter_access().cloned().collect()]
}

#[test]
fn targeted_invalidations_unlink_only_named_entries_from_every_order() {
    for spec in [0, 2] {
        for slot in [true, false] {
            let directory = tempfile::tempdir().unwrap();
            let (cache, keys) = populate(directory.path(), spec);
            let before = orders(&cache);
            let remove = |key: &CacheKey| key.shard_id == 0 && if slot {
                key.selector.starts_with("slot-3:")
            } else {
                key.namespace == "page" && key.record_key.as_ref() == "segment-00000000000000000003"
            };
            assert!(keys.iter().any(remove));
            let expected = before.map(|rows| rows.into_iter().filter(|key| !remove(key)).collect::<Vec<_>>());
            CACHE_ORDER_RETAIN_VISITS.with(|visits| visits.set(0));
            if slot { cache.invalidate_slot(0, 3).unwrap(); }
            else { cache.invalidate_page_segment(0, 3).unwrap(); }
            assert_eq!(CACHE_ORDER_RETAIN_VISITS.with(|visits| visits.get()), 0,
                       "targeted invalidation must not sweep an LRU list");
            assert_eq!(orders(&cache), expected, "slot={slot} insertion_spec={spec}");
            for (index, key) in keys.iter().enumerate() {
                let value = cache.get(key).unwrap();
                assert_eq!(value, (!remove(key)).then(|| vec![index as u8; 96]));
            }
            cache.stop();
        }
    }
}
