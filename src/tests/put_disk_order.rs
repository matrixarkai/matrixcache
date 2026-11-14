// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Putting a batch into the SSD tier moves its keys to the back of the disk order by key, not by
//! walking the order.
//!
//! The order is what eviction reads, so after a batch the keys that were not in it have to keep
//! their places and the batch has to follow them in the order it was staged. The walk that used to
//! do this removed the staged keys wherever they were; the result it left is the oracle here.

use super::*;

fn ssd_only_cache(directory: &std::path::Path) -> MultiLayerCache {
    let mut options = CacheOptions::new(0, 0, 1 << 20).with_ssd_paths([directory.join("ssd")]);
    options.cache_ssd_replacement_policy = "lru".into();
    let cache = MultiLayerCache::with_options(options);
    cache.start().unwrap();
    cache.set_ssd_admission_filter_enabled(false);
    cache
}

fn disk_order(cache: &MultiLayerCache) -> Vec<CacheKey> {
    cache
        .inner
        .read()
        .unwrap()
        .disk_order
        .iter()
        .cloned()
        .collect()
}

#[test]
fn a_batch_leaves_the_disk_order_as_the_walk_that_removed_its_keys_would() {
    // Batch sizes either side of the eight the old code switched to a set at, rewriting keys that
    // are already on disk, and one batch that mixes rewrites with keys the cache never held.
    for (rewritten, fresh) in [
        (1usize, 0usize),
        (2, 0),
        (7, 0),
        (8, 0),
        (9, 0),
        (40, 0),
        (5, 4),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let cache = ssd_only_cache(directory.path());
        let keys: Vec<CacheKey> = (0..120)
            .map(|index| CacheKey::string(0, &format!("resident-{index:03}")))
            .collect();
        for key in &keys {
            cache.put(key.clone(), vec![7; 64]).unwrap();
        }
        let before = disk_order(&cache);
        assert_eq!(
            before.len(),
            keys.len(),
            "the corpus must all sit in the disk tier"
        );

        // Not contiguous, not in order: every third key from the back, with a new value each.
        let mut batch: Vec<(CacheKey, Vec<u8>)> = keys
            .iter()
            .rev()
            .step_by(3)
            .take(rewritten)
            .map(|key| (key.clone(), vec![9; 64]))
            .collect();
        for index in 0..fresh {
            batch.push((CacheKey::string(0, &format!("fresh-{index}")), vec![9; 64]));
        }
        let staged: Vec<CacheKey> = batch.iter().map(|(key, _)| key.clone()).collect();
        cache.put_batch(batch).unwrap();

        let mut expected: Vec<CacheKey> = before
            .iter()
            .filter(|held| !staged.contains(held))
            .cloned()
            .collect();
        expected.extend(staged.iter().cloned());
        let inner = cache.inner.read().unwrap();
        let after: Vec<CacheKey> = inner.disk_order.iter().cloned().collect();
        assert_eq!(
            after, expected,
            "{rewritten} rewritten and {fresh} new keys"
        );
        assert_eq!(
            after
                .iter()
                .cloned()
                .collect::<std::collections::HashSet<_>>(),
            inner
                .disk_index
                .keys()
                .map(|held| CacheKey::clone(held))
                .collect(),
            "the order and the index must hold the same keys after {rewritten} + {fresh}"
        );
    }
}

#[test]
fn putting_a_batch_does_not_walk_the_whole_disk_order() {
    // A cost guard. A behavioural test cannot see a walk that finds the right answer, so the source
    // is read: between the store write and the index update, the batch path must not call `retain`
    // on the order. Comment lines are dropped first, because the reason it is gone is written beside
    // it.
    let source = include_str!("../runtime/multilayer_cache.rs");
    let start = source
        .find("self.write_ssd_blocks(storage_entries)?;")
        .expect("the call this guards must exist");
    let end = start
        + source[start..]
            .find("for entry in staged_ssd {")
            .expect("the index update it precedes must exist");
    let body: String = source[start..end]
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !body.contains(".retain("),
        "the batch path walks the disk order again"
    );
    assert!(
        body.contains("disk_order.remove("),
        "it must remove each staged key by handle"
    );
}
