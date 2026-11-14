// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use stats_alloc::{Region, StatsAlloc, INSTRUMENTED_SYSTEM};
use std::alloc::System;

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

#[test]
fn selective_snapshot_allocations_follow_matches_not_cache_population() {
    for count in [4096, 100_000] {
        let directory = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_options(
            CacheOptions::new(8 * 1024 * 1024, 0, 0).with_ssd_paths([directory.path().join("ssd")]),
        );
        cache.start().unwrap();
        for index in 0..count {
            cache
                .put(
                    CacheKey {
                        shard_id: 1,
                        namespace: "hash".to_owned().into(),
                        record_key: format!("object-{}", index / 8).into(),
                        selector: format!("field-{index}").into(),
                    },
                    vec![1; 16],
                )
                .unwrap();
        }
        let region = Region::new(GLOBAL);
        let missing = cache.keys_for_shard_matching(1, |_| false);
        let no_match = region.change();
        assert!(missing.is_empty());
        assert!(no_match.allocations <= 16, "{count}: {no_match:?}");

        let region = Region::new(GLOBAL);
        let selected =
            cache.keys_for_shard_matching(1, |key| key.record_key.as_ref() == "object-3");
        let matches = region.change();
        assert_eq!(selected.len(), 8);
        assert!(selected
            .iter()
            .all(|key| key.namespace == "hash" && key.record_key.as_ref() == "object-3"));
        assert!(matches.allocations <= 128, "{count}: {matches:?}");
        assert!(matches.bytes_allocated <= 32 * 1024, "{count}: {matches:?}");
        eprintln!("SELECTIVE_ALLOC count={count} selected=8 allocations={} bytes={} no_match_allocations={}", matches.allocations, matches.bytes_allocated, no_match.allocations);
        let before_index = cache.footprint();
        cache.enable_record_key_index("hash");
        let index_bytes = cache.footprint().total_bytes - before_index.total_bytes;
        let region = Region::new(GLOBAL);
        let indexed = cache.keys_for_records(1, "hash", &["object-3"]);
        let indexed_allocations = region.change();
        assert!(indexed.indexed);
        assert_eq!(indexed.entries_examined, 8);
        assert_eq!(indexed.keys, selected);
        assert!(
            indexed_allocations.allocations <= 128,
            "{count}: {indexed_allocations:?}"
        );
        assert!(
            indexed_allocations.bytes_allocated <= 32 * 1024,
            "{count}: {indexed_allocations:?}"
        );
        eprintln!("RECORD_INDEX_ALLOC count={count} selected=8 allocations={} bytes={} index_bytes={index_bytes}", indexed_allocations.allocations, indexed_allocations.bytes_allocated);
        assert!(cache.stop());
    }
}
