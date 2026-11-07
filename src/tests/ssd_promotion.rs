// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

use super::*;

fn cache() -> (tempfile::TempDir, MultiLayerCache) {
    let dir = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(128, 128, 4096)
            .with_ssd_paths([dir.path().join("ssd")])
            .with_pmem_paths([dir.path().join("pmem")]),
    );
    (dir, cache)
}

fn capture(inner: &mut CacheInner, key: &CacheKey) -> (Arc<[u8]>, u64) {
    let block = encode_cache_block(b"old", inner.block_options).unwrap();
    inner.write_ssd_block(key, block).unwrap();
    let read = inner.read_ssd_block(key).unwrap().unwrap();
    (
        decode_cache_block_shared(&read).unwrap(),
        inner.ssd_probe_epoch,
    )
}

// rust-internal: deterministic interleaving between disk read and promotion.
#[test]
fn ssd_probe_rejects_mutations_before_refill() {
    for operation in 0..6 {
        let (_dir, cache) = cache();
        let key = CacheKey::string(0, "key");
        let mut inner = cache.inner.write().unwrap();
        let (value, epoch) = capture(&mut inner, &key);
        match operation {
            0 => {
                assert!(inner.put_memory(&key, b"new".to_vec()));
            }
            1 => {
                assert!(inner.put_pmem(&key, b"new".to_vec()));
            }
            2 => {
                let block = encode_cache_block(b"new", inner.block_options).unwrap();
                inner.write_ssd_block(&key, block).unwrap();
            }
            3 => inner.invalidate_key_locked(&key, true),
            4 => inner.clear_all_locked(false).unwrap(),
            _ => {
                let block = encode_cache_block(b"new", inner.block_options).unwrap();
                inner.write_ssd_blocks(vec![(key.clone(), block)]).unwrap();
                inner.delete_ssd_blocks(std::slice::from_ref(&key)).unwrap();
            }
        }
        let current = inner.ssd_probe_is_current_for(&key, epoch);
        assert!(!current, "mutation {operation} must invalidate the probe");
        assert!(inner.refill_ssd_probe(&key, value, current));
        assert_ne!(
            inner.resident_value(&key).map(|value| value.as_ref()),
            Some(&b"old"[..])
        );
        assert_ne!(
            inner.pmem.get(&key).map(|value| value.as_ref()),
            Some(&b"old"[..])
        );
        assert_eq!(inner.stats.refill_failures, 0);
    }
}

// rust-internal: unchanged probes share bytes; existing upper tiers win.
#[test]
fn current_ssd_probe_refills_without_overwriting_residents() {
    let (_dir, cache) = cache();
    let key = CacheKey::string(0, "key");
    let mut inner = cache.inner.write().unwrap();
    let (value, epoch) = capture(&mut inner, &key);
    assert!(inner.ssd_probe_is_current(epoch));
    assert!(inner.refill_ssd_probe(&key, value.clone(), true));
    assert!(Arc::ptr_eq(inner.resident_value(&key).unwrap(), &value));
    assert!(inner.put_memory(&key, b"new".to_vec()));
    assert!(inner.refill_ssd_probe(&key, value, true));
    assert_eq!(inner.resident_value(&key).unwrap().as_ref(), b"new");
}

// rust-internal: epoch exhaustion must fail closed instead of wrapping.
#[test]
fn ssd_probe_epoch_saturates_and_disables_promotion() {
    let (_dir, cache) = cache();
    let mut inner = cache.inner.write().unwrap();
    inner.ssd_probe_epoch = u64::MAX - 1;
    inner.bump_ssd_probe_epoch();
    inner.bump_ssd_probe_epoch();
    assert_eq!(inner.ssd_probe_epoch, u64::MAX);
    assert!(!inner.ssd_probe_is_current(u64::MAX));
}

// rust-internal: independent pending reads must not cancel pure memory fills.
#[test]
fn ssd_refill_into_spare_memory_preserves_pending_probes() {
    let (_dir, cache) = cache();
    let first = CacheKey::string(0, "first");
    let second = CacheKey::string(0, "second");
    let mut inner = cache.inner.write().unwrap();
    let (value, _) = capture(&mut inner, &first);
    let (other, epoch) = capture(&mut inner, &second);
    assert!(inner.refill_ssd_probe(&first, value, true));
    assert!(inner.ssd_probe_is_current(epoch));
    assert!(inner.refill_ssd_probe(&second, other, true));
    assert!(inner.ssd_probe_is_current(epoch));
    assert!(inner.put_memory(&first, b"new".to_vec()));
    assert!(!inner.ssd_probe_is_current(epoch));
}

// rust-internal: capacity pressure must retain conservative invalidation.
#[test]
fn ssd_refill_with_eviction_invalidates_pending_probes() {
    let (_dir, cache) = cache();
    let first = CacheKey::string(0, "first");
    let second = CacheKey::string(0, "second");
    let mut inner = cache.inner.write().unwrap();
    let (value, _) = capture(&mut inner, &first);
    let (other, _) = capture(&mut inner, &second);
    inner.memory_capacity_bytes = value.len();
    assert!(inner.refill_ssd_probe(&first, value, true));
    let epoch = inner.ssd_probe_epoch;
    assert!(inner.refill_ssd_probe(&second, other, true));
    assert!(!inner.ssd_probe_is_current(epoch));
    assert!(inner.stats.memory_evictions > 0);
}

// rust-internal: a resident appearing before the disk read invalidates the batch.
#[test]
fn ssd_batch_probe_does_not_restore_an_evicted_newer_resident() {
    let (_dir, cache) = cache();
    let key = CacheKey::string(0, "key");
    let mut inner = cache.inner.write().unwrap();
    let (old, _) = capture(&mut inner, &key);
    assert!(inner.put_memory(&key, b"new".to_vec()));
    let epoch = inner.ssd_batch_probe_epoch(&[&key]);
    assert!(!inner.ssd_probe_is_current(epoch));
    inner.take_resident(&key);
    let current = inner.ssd_probe_is_current(epoch);
    assert!(inner.refill_ssd_probe(&key, old, current));
    assert!(inner.resident_value(&key).is_none());
    assert!(inner.put_pmem(&key, b"new".to_vec()));
    let epoch = inner.ssd_batch_probe_epoch(&[&key]);
    inner.pmem.remove(&key);
    assert!(!inner.ssd_probe_is_current(epoch));
}

// rust-internal: unrelated writes must not cancel a retained key's read ticket.
#[test]
fn ssd_probe_journal_distinguishes_keys_and_global_changes() {
    let (_dir, cache) = cache();
    let key = CacheKey::string(0, "read");
    let other = CacheKey::string(0, "write");
    let mut inner = cache.inner.write().unwrap();
    let (value, epoch) = capture(&mut inner, &key);
    assert!(inner.put_memory(&other, b"new".to_vec()));
    assert!(inner.ssd_probe_is_current_for(&key, epoch));
    assert!(!inner.ssd_probe_is_current_for(&other, epoch));
    assert!(inner.refill_ssd_probe(&key, value, true));
    let epoch = inner.ssd_probe_epoch;
    inner.clear_resident();
    assert!(!inner.ssd_probe_is_current_for(&key, epoch));
    assert!(!inner.ssd_probe_is_current_for(&other, epoch));
}

// rust-internal: bounded history must reject old tickets, not assume no change.
#[test]
fn ssd_probe_journal_rejects_lost_history() {
    let (_dir, cache) = cache();
    let key = CacheKey::string(0, "read");
    let other = CacheKey::string(0, "write");
    let mut inner = cache.inner.write().unwrap();
    let epoch = inner.ssd_probe_epoch;
    for _ in 0..inner.ssd_probe_mutations.len() {
        inner.note_ssd_probe_mutation(&other);
    }
    assert!(inner.ssd_probe_is_current_for(&key, epoch));
    inner.note_ssd_probe_mutation(&other);
    assert!(!inner.ssd_probe_is_current_for(&key, epoch));
}

// rust-internal: warmup must restore a journal slot as well as its sequence.
#[test]
fn ssd_probe_warmup_preserves_oldest_retained_mutation() {
    let (_dir, cache) = cache();
    let changed = CacheKey::string(0, "changed");
    let other = CacheKey::string(0, "other");
    let fill = CacheKey::string(0, "fill");
    let mut inner = cache.inner.write().unwrap();
    let epoch = inner.ssd_probe_epoch;
    inner.note_ssd_probe_mutation(&changed);
    for _ in 1..inner.ssd_probe_mutations.len() {
        inner.note_ssd_probe_mutation(&other);
    }
    assert!(!inner.ssd_probe_is_current_for(&changed, epoch));
    assert!(inner.refill_ssd_probe(&fill, Arc::from(&b"value"[..]), true));
    assert!(!inner.ssd_probe_is_current_for(&changed, epoch));
    assert!(inner.ssd_probe_is_current_for(&fill, epoch));
}

// rust-internal: disabled SSD mutations need a fence, not key-hash bookkeeping.
#[test]
fn disabled_ssd_uses_a_global_fence_and_resumes_key_history() {
    let (_dir, cache) = cache();
    let key = CacheKey::string(0, "read");
    let writer = CacheKey::string(0, "write");
    let mut inner = cache.inner.write().unwrap();
    let (_, epoch) = capture(&mut inner, &key);
    let history = inner.ssd_probe_mutations;
    inner.ssd_capacity_bytes = 0;
    assert!(inner.put_memory(&writer, b"value".to_vec()));
    assert_eq!(inner.ssd_probe_mutations, history);
    assert!(!inner.ssd_probe_is_current_for(&key, epoch));
    assert_eq!(inner.ssd_probe_global_epoch, inner.ssd_probe_epoch);

    inner.ssd_capacity_bytes = 4096;
    let epoch = inner.ssd_probe_epoch;
    assert!(inner.put_memory(&writer, b"updated".to_vec()));
    assert!(inner.ssd_probe_is_current_for(&key, epoch));
    assert!(!inner.ssd_probe_is_current_for(&writer, epoch));
}

// rust-internal: warm SSD-only shared reads need no exclusive cache ownership.
#[test]
fn ssd_only_shared_reads_stay_on_the_shared_lock_path() {
    let dir = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_tiering_policy(
        dir.path(),
        CacheTieringPolicy {
            memory_capacity_bytes: 128,
            pmem_capacity_bytes: 128,
            ssd_capacity_bytes: 4096,
            ..CacheTieringPolicy::default()
        },
        CacheBlockOptions {
            compression: CacheCompression::Zstd { level: 1 },
            min_compress_bytes: 1,
        },
    );
    cache.set_ssd_instance_only(true);
    let keys = (0..4)
        .map(|index| CacheKey::string(0, &format!("ssd-only-{index}")))
        .collect::<Vec<_>>();
    for (index, key) in keys.iter().enumerate() {
        cache.put(key.clone(), vec![index as u8; 64]).unwrap();
        assert_eq!(cache.peek_tier(key), Some(CacheReadTier::Ssd));
    }

    // The first reads establish metadata and access-order state. Subsequent
    // reads can account through atomics while another reader owns the lock.
    assert!(cache.get_shared(&keys[0]).unwrap().is_some());
    assert!(cache
        .get_shared_batch(&keys)
        .unwrap()
        .iter()
        .all(Option::is_some));
    let before = cache.stats();

    let single_completed = std::thread::scope(|scope| {
        let guard = cache.inner.read().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let cache = &cache;
        let key = &keys[0];
        let worker = scope.spawn(move || {
            sender.send(cache.get_shared(key).unwrap()).unwrap();
        });
        let completed = receiver
            .recv_timeout(std::time::Duration::from_secs(2))
            .is_ok();
        drop(guard);
        worker.join().unwrap();
        completed
    });
    assert!(
        single_completed,
        "SSD-only shared read requested the write lock"
    );

    let batch_completed = std::thread::scope(|scope| {
        let guard = cache.inner.read().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let cache = &cache;
        let keys = &keys;
        let worker = scope.spawn(move || {
            sender.send(cache.get_shared_batch(keys).unwrap()).unwrap();
        });
        let completed = receiver
            .recv_timeout(std::time::Duration::from_secs(2))
            .is_ok();
        drop(guard);
        worker.join().unwrap();
        completed
    });
    assert!(
        batch_completed,
        "SSD-only shared batch requested the write lock"
    );

    let after = cache.stats();
    assert_eq!(after.memory_fills, before.memory_fills);
    assert_eq!(after.pmem_fills, before.pmem_fills);
    assert_eq!(after.refill_failures, before.refill_failures);
    assert_eq!(after.disk_hits - before.disk_hits, 5);
    assert_eq!(after.compressed_hits - before.compressed_hits, 5);
    assert_eq!(after.get_latency_samples - before.get_latency_samples, 5);
    assert_eq!(
        after.read_through_latency_samples - before.read_through_latency_samples,
        5
    );
    assert_eq!(
        after.refill_latency_samples - before.refill_latency_samples,
        5
    );
    assert!(keys
        .iter()
        .all(|key| cache.peek_tier(key) == Some(CacheReadTier::Ssd)));
}

// rust-internal: warm SSD-only owned reads need no exclusive cache ownership.
#[test]
fn ssd_only_owned_reads_stay_on_the_shared_lock_path() {
    let dir = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_tiering_policy(
        dir.path(),
        CacheTieringPolicy {
            memory_capacity_bytes: 128,
            pmem_capacity_bytes: 128,
            ssd_capacity_bytes: 4096,
            ..CacheTieringPolicy::default()
        },
        CacheBlockOptions {
            compression: CacheCompression::Zstd { level: 1 },
            min_compress_bytes: 1,
        },
    );
    cache.set_ssd_instance_only(true);
    let key = CacheKey::string(0, "ssd-only-owned");
    cache.put(key.clone(), vec![7; 64]).unwrap();
    assert_eq!(cache.peek_tier(&key), Some(CacheReadTier::Ssd));

    // Establish metadata and access-order state before another reader keeps
    // the cache shared. The next read should need only atomic accounting.
    assert_eq!(cache.get(&key).unwrap(), Some(vec![7; 64]));
    let before = cache.stats();
    let result = std::thread::scope(|scope| {
        let guard = cache.inner.read().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let cache = &cache;
        let key = &key;
        let worker = scope.spawn(move || {
            sender.send(cache.get_with_tier(key).unwrap()).unwrap();
        });
        let result = receiver.recv_timeout(std::time::Duration::from_secs(2));
        drop(guard);
        worker.join().unwrap();
        result
    })
    .expect("SSD-only owned read requested the write lock")
    .expect("SSD value missing");

    assert_eq!(result.value, vec![7; 64]);
    assert_eq!(result.tier, CacheReadTier::Ssd);
    let after = cache.stats();
    assert_eq!(after.memory_fills, before.memory_fills);
    assert_eq!(after.pmem_fills, before.pmem_fills);
    assert_eq!(after.refill_failures, before.refill_failures);
    assert_eq!(after.compressed_hits - before.compressed_hits, 1);
    assert_eq!(cache.peek_tier(&key), Some(CacheReadTier::Ssd));
}

// rust-internal: warm SSD-only owned batches need no exclusive cache ownership.
#[test]
fn ssd_only_owned_batches_stay_on_the_shared_lock_path() {
    let dir = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_tiering_policy(
        dir.path(),
        CacheTieringPolicy {
            memory_capacity_bytes: 128,
            pmem_capacity_bytes: 128,
            ssd_capacity_bytes: 4096,
            ..CacheTieringPolicy::default()
        },
        CacheBlockOptions {
            compression: CacheCompression::Zstd { level: 1 },
            min_compress_bytes: 1,
        },
    );
    cache.set_ssd_instance_only(true);
    let keys = (0..4)
        .map(|index| CacheKey::string(0, &format!("ssd-only-owned-batch-{index}")))
        .collect::<Vec<_>>();
    for (index, key) in keys.iter().enumerate() {
        cache.put(key.clone(), vec![index as u8; 64]).unwrap();
        assert_eq!(cache.peek_tier(key), Some(CacheReadTier::Ssd));
    }

    assert!(cache.get_batch(&keys).unwrap().iter().all(Option::is_some));
    let before = cache.stats();
    let result = std::thread::scope(|scope| {
        let guard = cache.inner.read().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let cache = &cache;
        let keys = &keys;
        let worker = scope.spawn(move || {
            sender.send(cache.get_batch(keys).unwrap()).unwrap();
        });
        let result = receiver.recv_timeout(std::time::Duration::from_secs(2));
        drop(guard);
        worker.join().unwrap();
        result
    })
    .expect("SSD-only owned batch requested the write lock");

    for (index, value) in result.into_iter().enumerate() {
        assert_eq!(value, Some(vec![index as u8; 64]));
    }
    let after = cache.stats();
    assert_eq!(after.memory_fills, before.memory_fills);
    assert_eq!(after.pmem_fills, before.pmem_fills);
    assert_eq!(after.refill_failures, before.refill_failures);
    assert_eq!(after.compressed_hits - before.compressed_hits, 4);
    assert!(keys
        .iter()
        .all(|key| cache.peek_tier(key) == Some(CacheReadTier::Ssd)));
}

// rust-internal: warm SSD-only pinned reads use striped pin ownership only.
#[test]
fn ssd_only_pinned_reads_stay_on_the_shared_lock_path() {
    let dir = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_tiering_policy(
        dir.path(),
        CacheTieringPolicy {
            memory_capacity_bytes: 128,
            pmem_capacity_bytes: 128,
            ssd_capacity_bytes: 4096,
            ..CacheTieringPolicy::default()
        },
        CacheBlockOptions {
            compression: CacheCompression::Zstd { level: 1 },
            min_compress_bytes: 1,
        },
    );
    cache.set_ssd_instance_only(true);
    let key = CacheKey::string(0, "ssd-only-pinned");
    cache.put(key.clone(), vec![9; 64]).unwrap();

    let warm = cache.acquire(&key).unwrap().unwrap();
    cache.release(warm);
    let before = cache.stats();
    let handle = std::thread::scope(|scope| {
        let guard = cache.inner.read().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let cache = &cache;
        let key = &key;
        let worker = scope.spawn(move || {
            sender.send(cache.acquire(key).unwrap()).unwrap();
        });
        let result = receiver.recv_timeout(std::time::Duration::from_secs(2));
        drop(guard);
        worker.join().unwrap();
        result
    })
    .expect("SSD-only pinned read requested the write lock")
    .expect("SSD value missing");

    assert_eq!(handle.value(), &[9; 64]);
    assert_eq!(handle.tier(), CacheReadTier::Ssd);
    cache.release(handle);
    let after = cache.stats();
    assert_eq!(after.memory_fills, before.memory_fills);
    assert_eq!(after.pmem_fills, before.pmem_fills);
    assert_eq!(after.refill_failures, before.refill_failures);
    assert_eq!(after.compressed_hits - before.compressed_hits, 1);
    assert_eq!(after.pinned_entries, 0);
    assert_eq!(cache.peek_tier(&key), Some(CacheReadTier::Ssd));
}

// rust-internal: warm SSD-only pinned batches use striped pin ownership only.
#[test]
fn ssd_only_pinned_batches_stay_on_the_shared_lock_path() {
    let dir = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_tiering_policy(
        dir.path(),
        CacheTieringPolicy {
            memory_capacity_bytes: 128,
            pmem_capacity_bytes: 128,
            ssd_capacity_bytes: 8192,
            ..CacheTieringPolicy::default()
        },
        CacheBlockOptions {
            compression: CacheCompression::Zstd { level: 1 },
            min_compress_bytes: 1,
        },
    );
    cache.set_ssd_instance_only(true);
    let keys = (0..8)
        .map(|index| CacheKey::string(0, &format!("ssd-only-pinned-batch-{index}")))
        .collect::<Vec<_>>();
    for (index, key) in keys.iter().enumerate() {
        cache.put(key.clone(), vec![index as u8; 64]).unwrap();
    }

    let warm = cache.acquire_batch(&keys).unwrap();
    cache.release_batch(warm.into_iter().flatten().collect());
    let before = cache.stats();
    let handles = std::thread::scope(|scope| {
        let guard = cache.inner.read().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let cache = &cache;
        let keys = &keys;
        let worker = scope.spawn(move || {
            sender.send(cache.acquire_batch(keys).unwrap()).unwrap();
        });
        let result = receiver.recv_timeout(std::time::Duration::from_secs(2));
        drop(guard);
        worker.join().unwrap();
        result
    })
    .expect("SSD-only pinned batch requested the write lock");

    assert!(handles.iter().all(Option::is_some));
    assert!(handles
        .iter()
        .flatten()
        .all(|handle| handle.tier() == CacheReadTier::Ssd));
    assert_eq!(
        cache.release_batch(handles.into_iter().flatten().collect()),
        8
    );
    let after = cache.stats();
    assert_eq!(after.memory_fills, before.memory_fills);
    assert_eq!(after.pmem_fills, before.pmem_fills);
    assert_eq!(after.refill_failures, before.refill_failures);
    assert_eq!(after.compressed_hits - before.compressed_hits, 8);
    assert_eq!(after.pinned_entries, 0);
    assert!(keys
        .iter()
        .all(|key| cache.peek_tier(key) == Some(CacheReadTier::Ssd)));
}

// rust-internal: SSD-only pinned batches preserve duplicate, miss, and expiry accounting.
#[test]
fn ssd_only_pinned_batches_preserve_mixed_results_and_counters() {
    let dir = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(0, 0, 8192)
            .with_ssd_paths([dir.path().join("ssd")])
            .with_ssd_instance_only(true),
    );
    let repeated = CacheKey::string(0, "ssd-only-batch-repeated");
    let expired = CacheKey::string(0, "ssd-only-batch-expired");
    let missing = CacheKey::string(0, "ssd-only-batch-missing");
    cache.put(repeated.clone(), b"repeat".to_vec()).unwrap();
    cache
        .put_with_ttl(
            expired.clone(),
            b"old".to_vec(),
            std::time::Duration::from_millis(20),
        )
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(80));

    let before = cache.stats();
    let handles = cache
        .acquire_batch(&[repeated.clone(), missing, repeated.clone(), expired.clone()])
        .unwrap();
    assert_eq!(handles[0].as_ref().unwrap().value(), b"repeat");
    assert!(handles[1].is_none());
    assert_eq!(handles[2].as_ref().unwrap().value(), b"repeat");
    assert!(handles[3].is_none());

    let after = cache.stats();
    assert_eq!(after.disk_hits - before.disk_hits, 2);
    assert_eq!(after.misses - before.misses, 2);
    assert_eq!(
        after.zero_copy_handle_misses - before.zero_copy_handle_misses,
        1
    );
    assert_eq!(after.pin_operations - before.pin_operations, 2);
    assert_eq!(
        after.zero_copy_handle_hits - before.zero_copy_handle_hits,
        1
    );
    assert_eq!(after.expired_reads - before.expired_reads, 1);
    assert_eq!(after.pinned_entries, 1);
    assert_eq!(
        cache.release_batch(handles.into_iter().flatten().collect()),
        2
    );
    assert_eq!(cache.stats().pinned_entries, 0);
    assert_eq!(cache.peek_tier(&expired), None);
}

// rust-internal: SSD-only pinned misses update atomic read counters under a shared lock.
#[test]
fn ssd_only_pinned_misses_stay_on_the_shared_lock_path() {
    let dir = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(0, 0, 8192)
            .with_ssd_paths([dir.path().join("ssd")])
            .with_ssd_instance_only(true),
    );
    let key = CacheKey::string(0, "ssd-only-pinned-miss");
    let before = cache.stats();
    let result = std::thread::scope(|scope| {
        let guard = cache.inner.read().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let cache = &cache;
        let key = &key;
        let worker = scope.spawn(move || sender.send(cache.acquire(key).unwrap()).unwrap());
        let result = receiver.recv_timeout(std::time::Duration::from_secs(2));
        drop(guard);
        worker.join().unwrap();
        result
    })
    .expect("SSD-only pinned miss requested the write lock");

    assert!(result.is_none());
    let after = cache.stats();
    assert_eq!(after.misses - before.misses, 1);
    assert_eq!(
        after.zero_copy_handle_misses - before.zero_copy_handle_misses,
        1
    );
}

// rust-internal: SSD-only pinned batch misses do not serialize on the cache lock.
#[test]
fn ssd_only_pinned_batch_misses_stay_on_the_shared_lock_path() {
    let dir = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(0, 0, 8192)
            .with_ssd_paths([dir.path().join("ssd")])
            .with_ssd_instance_only(true),
    );
    let keys = (0..8)
        .map(|index| CacheKey::string(0, &format!("ssd-only-pinned-miss-{index}")))
        .collect::<Vec<_>>();
    let before = cache.stats();
    let results = std::thread::scope(|scope| {
        let guard = cache.inner.read().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let cache = &cache;
        let keys = &keys;
        let worker = scope.spawn(move || sender.send(cache.acquire_batch(keys).unwrap()).unwrap());
        let results = receiver.recv_timeout(std::time::Duration::from_secs(2));
        drop(guard);
        worker.join().unwrap();
        results
    })
    .expect("SSD-only pinned batch miss requested the write lock");

    assert!(results.iter().all(Option::is_none));
    let after = cache.stats();
    assert_eq!(after.misses - before.misses, 8);
    assert_eq!(
        after.zero_copy_handle_misses - before.zero_copy_handle_misses,
        8
    );
}

// rust-internal: absent multi-tier shared batches do not request the cache write lock.
#[test]
fn multi_tier_shared_batch_misses_stay_on_the_shared_lock_path() {
    let dir = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(8192, 0, 8192).with_ssd_paths([dir.path().join("ssd")]),
    );
    let keys = (0..8)
        .map(|index| CacheKey::string(0, &format!("multi-tier-shared-miss-{index}")))
        .collect::<Vec<_>>();
    let before = cache.stats();
    let results = std::thread::scope(|scope| {
        let guard = cache.inner.read().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let cache = &cache;
        let keys = &keys;
        let worker = scope.spawn(move || sender.send(cache.get_shared_batch(keys)).unwrap());
        let results = receiver.recv_timeout(std::time::Duration::from_secs(2));
        drop(guard);
        worker.join().unwrap();
        results
    })
    .expect("multi-tier shared batch miss requested the write lock")
    .unwrap();

    assert!(results.iter().all(Option::is_none));
    let after = cache.stats();
    assert_eq!(after.misses - before.misses, 8);
    assert_eq!(after.shared_buffer_misses - before.shared_buffer_misses, 8);
}

// rust-internal: absent multi-tier pinned batches do not request the cache write lock.
#[test]
fn multi_tier_pinned_batch_misses_stay_on_the_shared_lock_path() {
    let dir = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(8192, 0, 8192).with_ssd_paths([dir.path().join("ssd")]),
    );
    let keys = (0..8)
        .map(|index| CacheKey::string(0, &format!("multi-tier-pinned-miss-{index}")))
        .collect::<Vec<_>>();
    let before = cache.stats();
    let results = std::thread::scope(|scope| {
        let guard = cache.inner.read().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let cache = &cache;
        let keys = &keys;
        let worker = scope.spawn(move || sender.send(cache.acquire_batch(keys)).unwrap());
        let results = receiver.recv_timeout(std::time::Duration::from_secs(2));
        drop(guard);
        worker.join().unwrap();
        results
    })
    .expect("multi-tier pinned batch miss requested the write lock")
    .unwrap();

    assert!(results.iter().all(Option::is_none));
    let after = cache.stats();
    assert_eq!(after.misses - before.misses, 8);
    assert_eq!(
        after.zero_copy_handle_misses - before.zero_copy_handle_misses,
        8
    );
}

// rust-internal: absent multi-tier owned batches do not request the cache write lock.
#[test]
fn multi_tier_owned_batch_misses_stay_on_the_shared_lock_path() {
    let dir = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(8192, 0, 8192).with_ssd_paths([dir.path().join("ssd")]),
    );
    let keys = (0..8)
        .map(|index| CacheKey::string(0, &format!("multi-tier-owned-miss-{index}")))
        .collect::<Vec<_>>();
    let before = cache.stats();
    let results = std::thread::scope(|scope| {
        let guard = cache.inner.read().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let cache = &cache;
        let keys = &keys;
        let worker = scope.spawn(move || sender.send(cache.get_batch(keys)).unwrap());
        let results = receiver.recv_timeout(std::time::Duration::from_secs(2));
        drop(guard);
        worker.join().unwrap();
        results
    })
    .expect("multi-tier owned batch miss requested the write lock")
    .unwrap();

    assert!(results.iter().all(Option::is_none));
    let after = cache.stats();
    assert_eq!(after.misses - before.misses, 8);
}
