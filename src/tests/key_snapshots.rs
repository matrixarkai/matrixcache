// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

use super::*;

#[test]
fn key_snapshot_matches_entry_snapshot_across_tiers_and_shards() {
    let directory = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(128, 128, 64 * 1024)
            .with_ssd_paths([directory.path().join("ssd")])
            .with_pmem_paths([directory.path().join("pmem")]),
    );
    cache.start().unwrap();
    cache.set_ssd_admission_filter_enabled(false);
    for shard in [1, 2] {
        for index in 0..16 {
            cache
                .put(
                    CacheKey::string(shard, &format!("key-{index}")),
                    vec![index; 64],
                )
                .unwrap();
        }
    }
    assert!(cache.item_count_for_tier(CacheTier::Memory) > 0);
    assert!(cache.item_count_for_tier(CacheTier::Pmem) > 0);
    assert!(cache.item_count_for_tier(CacheTier::Ssd) > 0);
    for shard in [1, 2, 3] {
        let keys = cache.keys_for_shard(shard);
        let expected = cache
            .entries_for_shard(shard)
            .into_iter()
            .map(|entry| CacheKey {
                shard_id: entry.shard_id,
                namespace: entry.namespace.into(),
                record_key: entry.record_key.into(),
                selector: entry.selector.into(),
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(keys.len(), expected.len());
        assert_eq!(keys.into_iter().collect::<BTreeSet<_>>(), expected);
    }
    let deleted = cache.keys_for_shard(1);
    cache.invalidate_batch(&deleted).unwrap();
    assert!(cache.keys_for_shard(1).is_empty());
    assert!(!cache.keys_for_shard(2).is_empty());
    for key in deleted {
        assert!(cache.get(&key).unwrap().is_none());
    }
    assert!(cache.stop());
}

#[test]
fn key_snapshot_is_deduplicated_and_does_not_touch_cache_state() {
    let directory = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::new(1024, directory.path());
    let key = CacheKey::string(7, "shared");
    cache.put(key.clone(), vec![9; 64]).unwrap();
    {
        let mut inner = cache.inner.write().unwrap();
        let block = encode_cache_block(&[9; 64], inner.block_options).unwrap();
        let bytes = block.len() as u64;
        inner.write_ssd_block(&key, block).unwrap();
        let shared = inner.shared_key(&key);
        inner.disk_index.insert(shared, bytes);
    }
    let handle = cache.acquire(&key).unwrap().unwrap();
    let before = cache.stats();
    assert_eq!(cache.keys_for_shard(7), vec![key.clone()]);
    assert_eq!(cache.stats().pinned_entries, before.pinned_entries);
    assert_eq!(cache.stats().memory_hits, before.memory_hits);
    assert_eq!(cache.stats().disk_hits, before.disk_hits);
    assert_eq!(cache.stats().misses, before.misses);
    assert_eq!(handle.as_slice(), &[9; 64]);
    cache.release(handle);
    cache.invalidate_batch(&[key]).unwrap();
    assert!(cache.keys_for_shard(7).is_empty());
}

#[test]
fn sharded_key_snapshot_preserves_every_selector() {
    let directory = tempfile::tempdir().unwrap();
    let cache = ShardedMultiLayerCache::with_options(
        CacheOptions::new(64 * 1024, 0, 0).with_ssd_paths([directory.path().join("ssd")]),
        4,
    );
    cache.start().unwrap();
    let mut expected = BTreeSet::new();
    for index in 0..64 {
        let key = CacheKey {
            shard_id: 5,
            namespace: "hash".into(),
            record_key: format!("record-{}", index / 4).into(),
            selector: format!("field-{index}").into(),
        };
        cache.put(key.clone(), vec![index; 16]).unwrap();
        expected.insert(key);
    }
    assert_eq!(
        cache.keys_for_shard(5).into_iter().collect::<BTreeSet<_>>(),
        expected
    );
    assert!(cache.keys_for_shard(6).is_empty());
    assert!(cache.stop());
}

#[test]
#[ignore = "release-scale snapshot measurement"]
fn key_snapshot_cost_at_scale() {
    for count in [4096, 20_000, 100_000] {
        let directory = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_options(
            CacheOptions::new(8 * 1024 * 1024, 0, 0).with_ssd_paths([directory.path().join("ssd")]),
        );
        cache.start().unwrap();
        for index in 0..count {
            cache
                .put(CacheKey::string(1, &format!("key-{index}")), vec![9; 16])
                .unwrap();
        }
        let started = Instant::now();
        for _ in 0..5 {
            assert_eq!(
                std::hint::black_box(cache.entries_for_shard(1)).len(),
                count
            );
        }
        let entries_us = started.elapsed().as_micros();
        let started = Instant::now();
        for _ in 0..5 {
            assert_eq!(std::hint::black_box(cache.keys_for_shard(1)).len(), count);
        }
        let keys_us = started.elapsed().as_micros();
        eprintln!("KEY_SNAPSHOT count={count} rounds=5 entries_us={entries_us} keys_us={keys_us}");
        assert!(cache.stop());
    }
}
