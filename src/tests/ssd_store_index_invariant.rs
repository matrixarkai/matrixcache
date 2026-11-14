// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! The SSD store and the disk index name the same keys, after every operation.
//!
//! Invalidation sends a delete to the SSD store for every key it names, including keys the index
//! does not hold. That is what makes a delete safe when the two have drifted apart. It is also a
//! cost on every invalidation, and skipping the delete for keys absent from the index is only
//! correct if no block can sit in the store without an index entry. This pins that fact, both ways,
//! across puts, rewrites, single and batch invalidations and the evictions a small tier forces, so
//! that a change which makes the two drift is caught here rather than as a key that comes back
//! after a restart.

use super::*;

/// A fixed sequence of "random" numbers, so a failure names the same step every time.
fn next(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn drift(cache: &MultiLayerCache, universe: &[CacheKey]) -> Vec<String> {
    let inner = cache.inner.read().unwrap();
    let mut found = Vec::new();
    for key in universe {
        let in_store = inner.ssd_block_exists(key);
        let in_index = inner.disk_index.contains_key(key);
        if in_store && !in_index {
            found.push(format!(
                "{key:?}: a block is in the SSD store with no index entry"
            ));
        }
        if in_index && !in_store {
            found.push(format!(
                "{key:?}: the index holds a key whose block is not in the store"
            ));
        }
    }
    found
}

#[test]
fn the_ssd_store_and_the_disk_index_hold_the_same_keys_across_recoveries() {
    // The same run, but every so often the tiers are recovered from what is on disk, which rebuilds
    // the index from the store. If a block could sit in the store with no index entry, recovery
    // would bring it back and the next invalidation would have to delete it; this checks that no
    // such block is there before or after a recovery, and that an invalidated key stays gone.
    let directory = tempfile::tempdir().unwrap();
    let block_len = encode_cache_block(&[7; 64], CacheBlockOptions::default())
        .unwrap()
        .len();
    let mut options =
        CacheOptions::new(0, 0, block_len * 40).with_ssd_paths([directory.path().join("ssd")]);
    options.cache_ssd_replacement_policy = "lru".into();
    options.ssd_block_durability = true;
    let cache = MultiLayerCache::with_options(options);
    cache.start().unwrap();
    cache.set_ssd_admission_filter_enabled(false);

    let universe: Vec<CacheKey> = (0..200)
        .map(|index| CacheKey::string(0, &format!("key-{index:03}")))
        .collect();
    let mut state = 0xD1B5_4A32_D192_ED03u64;
    let mut invalidated: Vec<CacheKey> = Vec::new();
    let mut recoveries = 0usize;
    for step in 0..1500usize {
        let key = universe[(next(&mut state) % universe.len() as u64) as usize].clone();
        match next(&mut state) % 4 {
            0 | 1 => {
                cache
                    .put(key.clone(), vec![(next(&mut state) % 250) as u8; 64])
                    .unwrap();
                invalidated.retain(|gone| gone != &key);
            }
            2 => {
                cache.invalidate(&key).unwrap();
                invalidated.push(key);
            }
            _ => {
                let keys: Vec<CacheKey> = (0..(1 + next(&mut state) % 4))
                    .map(|_| universe[(next(&mut state) % universe.len() as u64) as usize].clone())
                    .collect();
                cache.invalidate_batch(&keys).unwrap();
                for key in keys {
                    invalidated.push(key);
                }
            }
        }
        if step % 97 == 96 {
            let before = drift(&cache, &universe);
            assert!(
                before.is_empty(),
                "step {step}, before a recovery: {}",
                before.join("; ")
            );
            cache.recover_persistent_tiers().unwrap();
            recoveries += 1;
            let after = drift(&cache, &universe);
            assert!(
                after.is_empty(),
                "step {step}, after a recovery: {}",
                after.join("; ")
            );
            // an invalidated key that was not written since must not have come back
            let inner = cache.inner.read().unwrap();
            for gone in &invalidated {
                assert!(
                    !inner.ssd_block_exists(gone),
                    "step {step}: {gone:?} was invalidated and is back after a recovery"
                );
            }
        }
        let found = drift(&cache, &universe);
        assert!(found.is_empty(), "step {step}: {}", found.join("; "));
    }
    assert!(recoveries >= 10, "only {recoveries} recoveries ran");
}

#[test]
fn the_ssd_store_and_the_disk_index_hold_the_same_keys_after_every_operation() {
    let directory = tempfile::tempdir().unwrap();
    let block_len = encode_cache_block(&[7; 64], CacheBlockOptions::default())
        .unwrap()
        .len();
    // Room for about forty blocks, against a universe of two hundred keys: evictions are constant.
    let mut options =
        CacheOptions::new(0, 0, block_len * 40).with_ssd_paths([directory.path().join("ssd")]);
    options.cache_ssd_replacement_policy = "lru".into();
    let cache = MultiLayerCache::with_options(options);
    cache.start().unwrap();
    cache.set_ssd_admission_filter_enabled(false);

    let universe: Vec<CacheKey> = (0..200)
        .map(|index| CacheKey::string(0, &format!("key-{index:03}")))
        .collect();
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut operations = [0usize; 4];
    for step in 0..2000usize {
        let pick =
            |state: &mut u64| universe[(next(state) % universe.len() as u64) as usize].clone();
        match next(&mut state) % 8 {
            // most steps write, some of them a key already there with a new value
            0..=3 => {
                let value = vec![(next(&mut state) % 250) as u8; 64];
                cache.put(pick(&mut state), value).unwrap();
                operations[0] += 1;
            }
            4 | 5 => {
                let batch: Vec<(CacheKey, Vec<u8>)> = (0..(1 + next(&mut state) % 6))
                    .map(|_| (pick(&mut state), vec![(next(&mut state) % 250) as u8; 64]))
                    .collect();
                // a batch may name a key twice; the cache collapses it
                cache.put_batch(batch).unwrap();
                operations[1] += 1;
            }
            6 => {
                cache.invalidate(&pick(&mut state)).unwrap();
                operations[2] += 1;
            }
            _ => {
                let keys: Vec<CacheKey> = (0..(1 + next(&mut state) % 5))
                    .map(|_| pick(&mut state))
                    .collect();
                cache.invalidate_batch(&keys).unwrap();
                operations[3] += 1;
            }
        }
        let found = drift(&cache, &universe);
        assert!(found.is_empty(), "step {step}: {}", found.join("; "));
    }
    // The run must have exercised every kind of operation and left a tier that was under pressure.
    assert!(
        operations.iter().all(|count| *count > 100),
        "operations: {operations:?}"
    );
    let on_disk = cache.inner.read().unwrap().disk_index.len();
    assert!(
        on_disk > 5 && on_disk <= 41,
        "the tier held {on_disk} blocks at the end"
    );
}
