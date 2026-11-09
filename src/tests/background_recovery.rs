// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

use super::*;

fn persistent_options(path: &Path) -> CacheOptions {
    CacheOptions::new(64, 0, 8 * 1024 * 1024)
        .with_ssd_paths([path.to_path_buf()])
        .with_auto_recover_on_start(true)
        .with_ssd_recovery_in_background(true)
        .with_ssd_recovery_batch_size(7)
}

fn numbered_key(shard_id: ShardId, prefix: &str, index: usize) -> CacheKey {
    CacheKey::string(shard_id, &format!("{prefix}-{index}"))
}

#[test]
fn background_ssd_recovery_restores_values_in_bounded_batches() {
    let dir = tempfile::tempdir().unwrap();
    let options = persistent_options(dir.path());
    let cache = MultiLayerCache::try_with_options(options.clone()).unwrap();
    for index in 0..128 {
        cache
            .put(numbered_key(17, "recovery", index), vec![index as u8; 128])
            .unwrap();
    }
    cache.stop();
    drop(cache);

    let restarted = MultiLayerCache::try_with_options(options).unwrap();
    assert!(restarted.ssd_recovery_in_background());
    assert_eq!(restarted.ssd_recovery_batch_size(), 7);
    let status = restarted.wait_for_ssd_recovery().unwrap();
    assert!(status.completed, "{status:?}");
    assert!(!status.running);
    assert!(!status.cancelled);
    assert_eq!(status.error, None);
    assert_eq!(status.report.recovered_files, 128);
    for index in 0..128 {
        assert_eq!(
            restarted.get(&numbered_key(17, "recovery", index)).unwrap(),
            Some(vec![index as u8; 128])
        );
    }
}

#[test]
fn live_write_wins_while_ssd_recovery_is_running() {
    let dir = tempfile::tempdir().unwrap();
    let key = CacheKey::string(19, "live-write");
    let base = CacheOptions::new(64, 0, 8 * 1024 * 1024).with_ssd_paths([dir.path().to_path_buf()]);
    let cache = MultiLayerCache::try_with_options(base.clone()).unwrap();
    cache.put(key.clone(), b"old".to_vec()).unwrap();
    for index in 0..256 {
        cache
            .put(numbered_key(19, "filler", index), vec![index as u8; 256])
            .unwrap();
    }
    cache.stop();
    drop(cache);

    let restarted = MultiLayerCache::try_with_options(base).unwrap();
    assert!(restarted.start_ssd_recovery_worker(1).unwrap());
    restarted.put(key.clone(), b"new-value".to_vec()).unwrap();
    let status = restarted.wait_for_ssd_recovery().unwrap();
    assert!(status.completed, "{status:?}");
    assert_eq!(restarted.get(&key).unwrap(), Some(b"new-value".to_vec()));
}

#[test]
fn stopping_ssd_recovery_joins_the_worker() {
    let dir = tempfile::tempdir().unwrap();
    let base = CacheOptions::new(64, 0, 8 * 1024 * 1024).with_ssd_paths([dir.path().to_path_buf()]);
    let cache = MultiLayerCache::try_with_options(base.clone()).unwrap();
    for index in 0..512 {
        cache
            .put(numbered_key(23, "cancel", index), vec![index as u8; 128])
            .unwrap();
    }
    cache.stop();
    drop(cache);

    let restarted = MultiLayerCache::try_with_options(base).unwrap();
    assert!(restarted.start_ssd_recovery_worker(1).unwrap());
    assert!(restarted.stop_ssd_recovery_worker());
    assert!(!restarted.ssd_recovery_worker_running());
    assert!(!restarted.stop_ssd_recovery_worker());
}

#[test]
fn sharded_recovery_lifecycle_reaches_every_shard() {
    let dir = tempfile::tempdir().unwrap();
    let options = CacheOptions::new(4 * 64, 0, 4 * 4096).with_ssd_paths([dir.path().to_path_buf()]);
    let cache = ShardedMultiLayerCache::with_options(options, 4);
    assert_eq!(cache.start_ssd_recovery_workers(3).unwrap(), 4);
    let statuses = cache.wait_for_ssd_recovery_workers();
    assert_eq!(statuses.len(), 4);
    assert!(statuses
        .iter()
        .all(|status| status.as_ref().is_some_and(|status| status.completed)));
    assert_eq!(cache.stop_ssd_recovery_workers(), 4);
    assert_eq!(cache.stop_ssd_recovery_workers(), 0);
}
