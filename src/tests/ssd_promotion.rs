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
        let current = inner.ssd_probe_is_current(epoch);
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
