// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

use super::*;

#[test]
fn disabled_ssd_probes_preserve_resident_hits_and_missing_results() {
    let directory = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_options(CacheOptions::new(4096, 0, 0)
        .with_ssd_paths([directory.path().join("ssd")]));
    cache.start().unwrap();
    let present = CacheKey::hash(1, "entity", "value");
    let missing = CacheKey::hash(1, "entity", "missing");
    cache.put_memory_only(present.clone(), vec![7; 64]);
    assert_eq!(cache.peek_tier(&present), Some(CacheReadTier::Memory));
    assert!(!cache.peek(&missing));
    {
        let inner = cache.inner.read().unwrap();
        assert!(!inner.ssd_store.holds_blocks());
        assert!(!inner.ssd_block_exists(&present));
        assert!(!inner.ssd_block_exists(&missing));
    }
    assert_eq!(cache.get_shared(&present).unwrap().unwrap().as_ref(), &[7; 64]);
    assert!(cache.stop());
    assert!(!cache.peek(&present));
}

#[test]
fn enabled_ssd_probes_keep_single_and_multi_backend_visibility() {
    for count in [1, 2] {
        let directory = tempfile::tempdir().unwrap();
        let paths: Vec<_> = (0..count).map(|index| directory.path().join(format!("ssd-{index}"))).collect();
        let cache = MultiLayerCache::with_options(CacheOptions::new(0, 0, 4096).with_ssd_paths(paths));
        cache.start().unwrap();
        let present = CacheKey::hash(1, "entity", "value");
        let empty = CacheKey::hash(1, "entity", "empty");
        let missing = CacheKey::hash(1, "entity", "missing");
        cache.put(present.clone(), vec![9; 64]).unwrap();
        cache.put(empty.clone(), Vec::new()).unwrap();
        {
            let inner = cache.inner.read().unwrap();
            assert!(inner.ssd_store.holds_blocks());
            assert!(inner.ssd_block_exists(&present));
            assert!(inner.ssd_block_exists(&empty));
            assert!(!inner.ssd_block_exists(&missing));
        }
        assert_eq!(cache.peek_tier(&present), Some(CacheReadTier::Ssd));
        assert_eq!(cache.get(&present).unwrap().unwrap(), vec![9; 64]);
        assert_eq!(cache.get(&empty).unwrap().unwrap(), Vec::<u8>::new());
        assert!(cache.stop());
    }
}
