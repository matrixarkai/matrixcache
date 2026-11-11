// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

use super::*;

fn query_key(shard_id: ShardId, namespace: &'static str, record: &str, selector: &str) -> CacheKey {
    CacheKey {
        shard_id,
        namespace: namespace.into(),
        record_key: record.into(),
        selector: selector.to_owned().into(),
    }
}

fn assert_exact_index(cache: &MultiLayerCache) {
    let inner = cache.inner.read().unwrap();
    let expected = inner
        .memory
        .keys()
        .chain(inner.pmem.keys())
        .chain(inner.disk_index.keys())
        .filter(|key| {
            inner
                .record_key_index
                .namespaces
                .contains_key(key.namespace.as_ref())
        })
        .map(|key| key.as_ref())
        .collect::<BTreeSet<_>>();
    let actual = inner
        .record_key_index
        .keys()
        .map(|key| key.as_ref())
        .collect::<BTreeSet<_>>();
    assert_eq!(actual, expected);
}

#[test]
fn record_key_index_backfills_deduplicates_and_isolates_namespaces() {
    let directory = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(1 << 20, 0, 0).with_ssd_paths([directory.path().to_path_buf()]),
    );
    let mut expected = Vec::new();
    for shard in [1, 2] {
        for namespace in ["hash", "feature", "page"] {
            for selector in ["a", "b", "c"] {
                let key = query_key(shard, namespace, "shared", selector);
                cache.put(key.clone(), vec![7; 16]).unwrap();
                if shard == 1 && namespace == "hash" {
                    expected.push(key);
                }
            }
        }
    }
    let fallback = cache.keys_for_records(1, "hash", &["shared", "shared"]);
    assert!(!fallback.indexed);
    assert_eq!(fallback.entries_examined, 18);
    assert_eq!(fallback.keys, expected);
    cache.enable_record_key_index("hash");
    cache.enable_record_key_index("hash");
    let stats = cache.stats();
    let indexed = cache.keys_for_records(1, "hash", &["absent", "shared", "shared"]);
    assert!(indexed.indexed);
    assert_eq!(indexed.entries_examined, 3);
    assert_eq!(indexed.keys, expected);
    assert_eq!(cache.stats().memory_hits, stats.memory_hits);
    assert_eq!(cache.stats().misses, stats.misses);
    assert!(!cache.keys_for_records(1, "feature", &["shared"]).indexed);
    cache.enable_record_key_index("feature");
    assert_eq!(
        cache.keys_for_records(1, "feature", &["shared"]).keys.len(),
        3
    );
    cache.invalidate_batch(&expected).unwrap();
    assert!(cache
        .keys_for_records(1, "hash", &["shared"])
        .keys
        .is_empty());
    assert_eq!(cache.keys_for_records(2, "hash", &["shared"]).keys.len(), 3);
    assert_eq!(
        cache.keys_for_records(1, "feature", &["shared"]).keys.len(),
        3
    );
    assert_exact_index(&cache);
}

#[test]
fn record_key_index_tracks_tier_eviction_recovery_and_clear() {
    let directory = tempfile::tempdir().unwrap();
    let options = CacheOptions::new(128, 128, 4096)
        .with_ssd_paths([directory.path().join("ssd")])
        .with_pmem_paths([directory.path().join("pmem")])
        .with_auto_recover_on_start(true);
    {
        let cache = MultiLayerCache::with_options(options.clone());
        cache.enable_record_key_index("hash");
        cache.start().unwrap();
        cache.set_ssd_admission_filter_enabled(false);
        for index in 0..96 {
            let key = query_key(
                1,
                "hash",
                &format!("record-{}", index / 3),
                &format!("field-{index}"),
            );
            cache.put(key, vec![index; 64]).unwrap();
            assert_exact_index(&cache);
        }
        assert!(cache.item_count_for_tier(CacheTier::Memory) > 0);
        assert!(cache.item_count_for_tier(CacheTier::Pmem) > 0);
        assert!(cache.item_count_for_tier(CacheTier::Ssd) > 0);
        let keys = cache.keys_for_shard(1);
        for key in keys.iter().step_by(3) {
            cache.get(key).unwrap();
            assert_exact_index(&cache);
        }
        cache
            .invalidate_batch(&keys.iter().step_by(4).cloned().collect::<Vec<_>>())
            .unwrap();
        assert_exact_index(&cache);
        assert!(cache.stop());
    }
    let cache = MultiLayerCache::with_options(options);
    cache.enable_record_key_index("hash");
    cache.start().unwrap();
    assert_exact_index(&cache);
    assert!(!cache.keys_for_shard(1).is_empty());
    cache.recover_disk_index().unwrap();
    assert_exact_index(&cache);
    cache.invalidate_shard(1).unwrap();
    assert_exact_index(&cache);
    assert!(cache
        .keys_for_records(1, "hash", &["record-31"])
        .keys
        .is_empty());
    assert!(cache.stop());
}

#[test]
fn record_key_index_has_no_per_entry_cost_for_unregistered_pages() {
    let directory = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(32 << 20, 0, 0).with_ssd_paths([directory.path().to_path_buf()]),
    );
    cache.enable_record_key_index("hash");
    let key = query_key(1, "hash", "target", "field");
    cache.put(key.clone(), vec![1; 16]).unwrap();
    let before = cache.footprint();
    for index in 0..4096 {
        cache
            .put(
                query_key(1, "page", &format!("page-{index}"), "data"),
                vec![2; 16],
            )
            .unwrap();
    }
    let after = cache.footprint();
    assert_eq!(
        before.table_bytes_by_map.metadata,
        after.table_bytes_by_map.metadata
    );
    let result = cache.keys_for_records(1, "hash", &["target"]);
    assert_eq!(result.entries_examined, 1);
    assert_eq!(result.keys, vec![key.clone()]);
    cache.invalidate_batch(&[key]).unwrap();
    assert_exact_index(&cache);
    assert!(cache.inner.read().unwrap().record_key_index.namespaces["hash"].is_empty());
}

#[test]
fn record_key_index_preserves_sharded_selectors() {
    let directory = tempfile::tempdir().unwrap();
    let cache = ShardedMultiLayerCache::with_options(
        CacheOptions::new(1 << 20, 0, 0).with_ssd_paths([directory.path().to_path_buf()]),
        4,
    );
    cache.enable_record_key_index("hash");
    let mut expected = Vec::new();
    for index in 0..32 {
        let key = query_key(5, "hash", "shared", &format!("field-{index:02}"));
        cache.put(key.clone(), vec![1; 16]).unwrap();
        expected.push(key);
    }
    let result = cache.keys_for_records(5, "hash", &["shared", "shared"]);
    assert!(result.indexed);
    assert_eq!(result.entries_examined, 32);
    assert_eq!(result.keys, expected);
}

#[test]
fn record_key_index_removes_only_the_last_tier_copy() {
    let directory = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::new(1024, directory.path());
    cache.enable_record_key_index("hash");
    let key = Arc::new(query_key(1, "hash", "record", "field"));
    {
        let mut inner = cache.inner.write().unwrap();
        inner.make_resident(Arc::clone(&key), Arc::from([7; 16]));
        inner.make_pmem_resident(Arc::clone(&key), Arc::from([7; 16]));
        inner.insert_disk_index(Arc::clone(&key), 16);
        inner.take_resident(&key);
        assert_eq!(inner.record_key_index.keys().count(), 1);
        inner.take_pmem_resident(&key);
        assert_eq!(inner.record_key_index.keys().count(), 1);
        inner.remove_disk_index(&key);
        assert_eq!(inner.record_key_index.keys().count(), 0);
        // Overwrites and tier clears must preserve another tier's membership.
        inner.make_resident(Arc::clone(&key), Arc::from([8; 16]));
        inner.make_resident(Arc::clone(&key), Arc::from([9; 16]));
        inner.make_pmem_resident(Arc::clone(&key), Arc::from([9; 16]));
        inner.clear_resident();
        assert_eq!(inner.record_key_index.keys().count(), 1);
        inner.clear_pmem_resident();
        assert_eq!(inner.record_key_index.keys().count(), 0);
        inner.make_resident(Arc::clone(&key), Arc::from([9; 16]));
        inner.clear_all_locked(false).unwrap();
        assert!(inner.record_key_index.namespaces.contains_key("hash"));
        assert_eq!(inner.record_key_index.keys().count(), 0);
    }
    cache.put(key.as_ref().clone(), vec![1; 16]).unwrap();
    assert_exact_index(&cache);
    assert_eq!(cache.keys_for_records(1, "hash", &["record"]).keys.len(), 1);
}

#[test]
#[ignore = "release-scale record lookup measurement"]
fn record_key_index_cost_at_scale() {
    for count in [4096, 100_000] {
        let directory = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_options(
            CacheOptions::new(128 << 20, 0, 0).with_ssd_paths([directory.path().to_path_buf()]),
        );
        for index in 0..count {
            cache
                .put(
                    query_key(1, "page", &format!("page-{index}"), "data"),
                    vec![2; 16],
                )
                .unwrap();
        }
        let records = (0..8).map(|i| format!("target-{i}")).collect::<Vec<_>>();
        let requested = records.iter().map(String::as_str).collect::<Vec<_>>();
        for record in &records {
            cache
                .put(query_key(1, "hash", record, "field"), vec![1; 16])
                .unwrap();
        }
        let before = cache.footprint();
        let started = Instant::now();
        for _ in 0..20 {
            let snapshot = cache.keys_for_records(1, "hash", &requested);
            assert_eq!(snapshot.keys.len(), 8);
            assert_eq!(snapshot.entries_examined, count + 8);
        }
        let scan_us = started.elapsed().as_micros();
        cache.enable_record_key_index("hash");
        let after = cache.footprint();
        let started = Instant::now();
        for _ in 0..20 {
            let snapshot = cache.keys_for_records(1, "hash", &requested);
            assert_eq!(snapshot.keys.len(), 8);
            assert_eq!(snapshot.entries_examined, 8);
        }
        let indexed_us = started.elapsed().as_micros();
        assert_eq!(after.payload_bytes, before.payload_bytes);
        assert_eq!(after.key_bytes, before.key_bytes);
        eprintln!("RECORD_LOOKUP pages={count} records=8 rounds=20 scan_us={scan_us} indexed_us={indexed_us} index_bytes={}", after.total_bytes - before.total_bytes);
    }
}

#[test]
fn record_key_index_tracks_expiry_and_rejected_admission() {
    let directory = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(128, 0, 0).with_ssd_paths([directory.path().to_path_buf()]),
    );
    cache.enable_record_key_index("hash");
    let key = query_key(1, "hash", "expiring", "field");
    cache
        .put_with_ttl(key.clone(), vec![1; 16], Duration::from_millis(1))
        .unwrap();
    assert_eq!(
        cache.keys_for_records(1, "hash", &["expiring"]).keys.len(),
        1
    );
    let expires_at = cache
        .inner
        .read()
        .unwrap()
        .expiries
        .get(&key)
        .copied()
        .unwrap();
    let limit = Instant::now() + Duration::from_secs(2);
    while CoarseClock::now_millis() < expires_at {
        assert!(
            Instant::now() < limit,
            "cache clock did not reach the deadline"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(cache.get(&key).unwrap().is_none());
    assert_exact_index(&cache);
    assert!(cache
        .keys_for_records(1, "hash", &["expiring"])
        .keys
        .is_empty());
    let _ = cache.put(query_key(1, "hash", "oversized", "field"), vec![2; 4096]);
    assert_exact_index(&cache);
    assert!(cache
        .keys_for_records(1, "hash", &["oversized"])
        .keys
        .is_empty());
}

#[test]
fn record_key_index_tracks_background_recovery() {
    let directory = tempfile::tempdir().unwrap();
    let options =
        CacheOptions::new(64, 0, 1 << 20).with_ssd_paths([directory.path().to_path_buf()]);
    let cache = MultiLayerCache::with_options(options.clone());
    cache.start().unwrap();
    cache.set_ssd_admission_filter_enabled(false);
    for index in 0..32 {
        cache
            .put(
                query_key(1, "hash", "record", &format!("field-{index}")),
                vec![index; 128],
            )
            .unwrap();
    }
    assert!(cache.stop());
    drop(cache);
    let cache = MultiLayerCache::with_options(options);
    cache.enable_record_key_index("hash");
    cache.start().unwrap();
    assert!(cache.start_ssd_recovery_worker(3).unwrap());
    let status = cache.wait_for_ssd_recovery().unwrap();
    assert!(status.completed, "{status:?}");
    assert_eq!(status.report.recovered_files, 32);
    assert_exact_index(&cache);
    let snapshot = cache.keys_for_records(1, "hash", &["record"]);
    assert_eq!(snapshot.entries_examined, 32);
    assert_eq!(snapshot.keys.len(), 32);
    assert!(cache.stop());
}

#[test]
fn record_invalidation_preserves_other_namespaces_shards_and_shared_values() {
    for indexed in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_options(
            CacheOptions::new(1 << 20, 0, 0).with_ssd_paths([directory.path().to_path_buf()]),
        );
        for shard in [1, 2] {
            for namespace in ["feature", "hash", "page"] {
                for record in ["shared", "other"] {
                    for selector in ["a", "b", "c"] {
                        cache
                            .put(query_key(shard, namespace, record, selector), vec![9; 16])
                            .unwrap();
                    }
                }
            }
        }
        if indexed {
            cache.enable_record_key_index("feature");
            cache.enable_record_key_index("hash");
        }
        let target = query_key(1, "feature", "shared", "a");
        let held = cache.get_shared(&target).unwrap().unwrap();
        for namespace in ["feature", "hash"] {
            cache.invalidate_record(1, namespace, "shared").unwrap();
            cache.invalidate_record(1, namespace, "shared").unwrap();
            for selector in ["a", "b", "c"] {
                assert!(cache
                    .get(&query_key(1, namespace, "shared", selector))
                    .unwrap()
                    .is_none());
                assert_eq!(
                    cache
                        .get(&query_key(2, namespace, "shared", selector))
                        .unwrap(),
                    Some(vec![9; 16])
                );
                assert_eq!(
                    cache
                        .get(&query_key(1, namespace, "other", selector))
                        .unwrap(),
                    Some(vec![9; 16])
                );
                assert_eq!(
                    cache
                        .get(&query_key(1, "page", "shared", selector))
                        .unwrap(),
                    Some(vec![9; 16])
                );
            }
            assert_exact_index(&cache);
        }
        assert_eq!(&*held, &[9; 16]);
        cache.inner.write().unwrap().started = false;
        assert!(cache.invalidate_record(1, "feature", "absent").is_ok());
        assert!(matches!(
            cache.invalidate_record(1, "feature", "other"),
            Err(CacheError::Stopped)
        ));
        cache.inner.write().unwrap().started = true;
    }
}

#[test]
fn indexed_record_invalidation_survives_tier_recovery() {
    let directory = tempfile::tempdir().unwrap();
    let options = CacheOptions::new(128, 128, 1 << 20)
        .with_ssd_paths([directory.path().join("ssd")])
        .with_pmem_paths([directory.path().join("pmem")])
        .with_auto_recover_on_start(true);
    let removed = ["record-0", "record-31"];
    {
        let cache = MultiLayerCache::with_options(options.clone());
        cache.enable_record_key_index("feature");
        cache.start().unwrap();
        cache.set_ssd_admission_filter_enabled(false);
        for index in 0..96 {
            cache
                .put(
                    query_key(
                        1,
                        "feature",
                        &format!("record-{}", index / 3),
                        &format!("field-{index}"),
                    ),
                    vec![index; 64],
                )
                .unwrap();
        }
        assert!(cache.item_count_for_tier(CacheTier::Memory) > 0);
        assert!(cache.item_count_for_tier(CacheTier::Pmem) > 0);
        assert!(cache.item_count_for_tier(CacheTier::Ssd) > 0);
        for record in removed {
            let keys = cache.keys_for_records(1, "feature", &[record]).keys;
            assert_eq!(keys.len(), 3);
            cache.invalidate_record(1, "feature", record).unwrap();
            for key in keys {
                assert!(cache.get(&key).unwrap().is_none());
            }
            assert_exact_index(&cache);
        }
        assert!(cache.stop());
    }
    let cache = MultiLayerCache::with_options(options);
    cache.enable_record_key_index("feature");
    cache.start().unwrap();
    assert_exact_index(&cache);
    for record in removed {
        assert!(cache
            .keys_for_records(1, "feature", &[record])
            .keys
            .is_empty());
    }
    assert!(!cache
        .keys_for_records(1, "feature", &["record-10"])
        .keys
        .is_empty());
    assert!(cache.stop());
}
