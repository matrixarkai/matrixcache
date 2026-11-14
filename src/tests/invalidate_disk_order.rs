// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Invalidating keys takes them out of the disk order by key, not by walking the order.
//!
//! The order is what eviction reads, so what survives an invalidation, and in what order, has to be
//! exactly what a walk that keeps every key not named would have left. That walk is kept here as the
//! oracle.

use super::*;

fn ssd_only_cache(directory: &std::path::Path) -> MultiLayerCache {
    let mut options = CacheOptions::new(0, 0, 1 << 20).with_ssd_paths([directory.join("ssd")]);
    options.cache_ssd_replacement_policy = "lru".into();
    let cache = MultiLayerCache::with_options(options);
    cache.start().unwrap();
    cache.set_ssd_admission_filter_enabled(false);
    cache
}

#[test]
fn an_invalidation_leaves_the_disk_order_as_a_walk_that_keeps_the_rest_would() {
    // Batch sizes either side of the eight the old code switched to a set at, and one that names
    // keys the cache never held.
    for named in [1usize, 2, 7, 8, 9, 40] {
        let directory = tempfile::tempdir().unwrap();
        let cache = ssd_only_cache(directory.path());
        let keys: Vec<CacheKey> = (0..120)
            .map(|index| CacheKey::string(0, &format!("resident-{index:03}")))
            .collect();
        for key in &keys {
            cache.put(key.clone(), vec![7; 64]).unwrap();
        }
        let before: Vec<CacheKey> = cache
            .inner
            .read()
            .unwrap()
            .disk_order
            .iter()
            .cloned()
            .collect();
        assert_eq!(before.len(), keys.len(), "the corpus must all sit in the disk tier");

        // Not contiguous, not in order: every third key from the back, then absent keys.
        let mut batch: Vec<CacheKey> = keys.iter().rev().step_by(3).take(named).cloned().collect();
        batch.push(CacheKey::string(0, "never-held"));
        cache.invalidate_batch(&batch).unwrap();

        let expected: Vec<CacheKey> = before
            .iter()
            .filter(|held| !batch.contains(held))
            .cloned()
            .collect();
        let inner = cache.inner.read().unwrap();
        let after: Vec<CacheKey> = inner.disk_order.iter().cloned().collect();
        assert_eq!(after, expected, "{named} named keys");
        assert_eq!(
            after.iter().cloned().collect::<std::collections::HashSet<_>>(),
            inner.disk_index.keys().map(|held| CacheKey::clone(held)).collect(),
            "the order and the index must hold the same keys after {named} named keys"
        );
        drop(inner);
        for key in &batch {
            assert_eq!(cache.get(key).unwrap(), None, "a named key is still served");
        }
        for key in expected.iter().take(5) {
            assert_eq!(cache.get(key).unwrap(), Some(vec![7; 64]), "a survivor was lost");
        }
    }
}

#[test]
fn invalidating_keys_does_not_walk_the_whole_disk_order() {
    // A cost guard. A behavioural test cannot see a walk that finds the right answer, so the
    // source is read: `invalidate_keys_locked` must not call `retain` on the order. Comment lines
    // are dropped first, because the reason it is gone is written beside it.
    let source = include_str!("../runtime/builder_and_gc.rs");
    let start = source
        .find("fn invalidate_keys_locked(")
        .expect("the function this guards must exist");
    let body: String = source[start..]
        .lines()
        .take_while(|line| !line.starts_with("    fn clear_all_locked"))
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !body.contains("disk_order") || !body.contains(".retain("),
        "invalidate_keys_locked walks the disk order again"
    );
    assert!(body.contains("disk_order.remove("), "it must remove each key by handle");
}
