// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

use matrixcache::{
    AsyncStorageRuntime, CacheBuffer, CacheError, StorageEngineApi, StorageEngineSimple,
};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

#[cfg(feature = "rocksdb-ssd")]
#[test]
fn managed_ssd_write_buffers_share_pressure_and_release_arenas_after_close() {
    let (completed, completion) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        use matrixcache::{SsdReadCache, StorageEngineRocksDb};
        assert!(SsdReadCache::new_with_write_buffer_budget(1024, 0, true).is_err());
        assert!(SsdReadCache::new_with_write_buffer_budget(1024, 2048, true).is_err());
        assert_eq!(SsdReadCache::new(1024).write_buffer_usage_bytes(), None);
        let directory = tempfile::tempdir().unwrap();
        let pool =
            SsdReadCache::new_with_write_buffer_budget(16 * 1024 * 1024, 2 * 1024 * 1024, true)
                .unwrap();
        assert_eq!(pool.write_buffer_capacity_bytes(), Some(2 * 1024 * 1024));
        assert_eq!(pool.write_buffer_usage_bytes(), Some(0));
        let mut stores = Vec::new();
        for i in 0..4 {
            let mut store = StorageEngineRocksDb::new(
                directory.path().join(format!("db-{i}")).to_string_lossy(),
            );
            store.set_read_cache(pool.clone()).unwrap();
            assert!(store.start());
            stores.push(store);
        }
        let mut alias = StorageEngineRocksDb::new(directory.path().join("db-0").to_string_lossy());
        assert!(alias.start());
        assert_eq!(alias.read_cache(), Some(&pool));
        alias.put("alias", vec![9; 4096]).unwrap();
        assert_eq!(alias.get("alias").unwrap().value(), vec![9; 4096]);
        assert!(alias.stop());
        let mut observed_arena_bytes = false;
        // More than the shared target, interleaved across independently opened DBs.
        for key in 0..512 {
            for (index, store) in stores.iter_mut().enumerate() {
                store
                    .put(&format!("key-{key}"), vec![index as u8; 4096])
                    .unwrap();
                observed_arena_bytes |= pool.write_buffer_usage_bytes().unwrap() > 0;
            }
        }
        assert!(observed_arena_bytes);
        let barrier = Arc::new(std::sync::Barrier::new(stores.len()));
        let writers: Vec<_> = stores
            .into_iter()
            .enumerate()
            .map(|(index, mut store)| {
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    for key in 512..1024 {
                        let mut retries = 0;
                        loop {
                            match store.put(&format!("key-{key}"), vec![index as u8; 4096]) {
                                Ok(_) => break,
                                Err(CacheError::CapacityExceeded) if retries < 500 => {
                                    retries += 1;
                                    std::thread::sleep(Duration::from_millis(1));
                                }
                                Err(error) => {
                                    panic!("write failed after {retries} retries: {error}")
                                }
                            }
                        }
                    }
                    store
                })
            })
            .collect();
        let mut stores: Vec<_> = writers
            .into_iter()
            .map(|writer| writer.join().unwrap())
            .collect();
        for (index, store) in stores.iter_mut().enumerate() {
            for key in 0..1024 {
                assert_eq!(
                    store.get(&format!("key-{key}")).unwrap().value(),
                    vec![index as u8; 4096]
                );
            }
            assert!(store.stop());
        }
        assert_eq!(pool.write_buffer_usage_bytes(), Some(0));
        completed.send(()).unwrap();
    });
    completion
        .recv_timeout(Duration::from_secs(30))
        .expect("managed writes must make flush progress");
    worker.join().unwrap();
}

#[cfg(feature = "rocksdb-ssd")]
#[test]
fn shared_ssd_read_cache_is_used_across_independent_stores() {
    use matrixcache::{SsdReadCache, StorageEngineRocksDb};
    let directory = tempfile::tempdir().unwrap();
    let pool = SsdReadCache::new(512 * 1024);
    let initial_pinned_bytes = pool.pinned_bytes();
    let mut stores = Vec::new();
    for device in 0..4 {
        let path = directory.path().join(format!("store-{device}"));
        let mut store = StorageEngineRocksDb::new(path.to_string_lossy());
        store.set_read_cache(pool.clone()).unwrap();
        assert!(store.start());
        store
            .put_batch(
                (0..128)
                    .map(|i| (format!("key-{i}"), vec![device; 4096]))
                    .collect(),
            )
            .unwrap();
        assert!(store.stop());
        assert!(store.start());
        stores.push(store);
    }
    for (device, store) in stores.iter().enumerate() {
        for i in 0..128 {
            assert_eq!(
                store.get(&format!("key-{i}")).unwrap().value(),
                vec![device as u8; 4096]
            );
        }
    }
    assert!(
        pool.usage_bytes() > 0,
        "reads must populate the configured pool"
    );
    assert!(pool.usage_bytes() <= pool.capacity_bytes());
    for store in &mut stores {
        assert!(store.stop());
    }
    assert_eq!(pool.pinned_bytes(), initial_pinned_bytes);
}

#[cfg(feature = "rocksdb-ssd")]
#[test]
fn shared_ssd_read_cache_refuses_conflicting_open_path_configuration() {
    use matrixcache::{SsdReadCache, StorageEngineRocksDb};
    let directory = tempfile::tempdir().unwrap();
    let path = directory
        .path()
        .join("shared")
        .to_string_lossy()
        .into_owned();
    let pool = SsdReadCache::new(1024 * 1024);
    let mut first = StorageEngineRocksDb::new(path.clone());
    first.set_read_cache(pool.clone()).unwrap();
    assert!(first.start());
    first.put("key", vec![7; 4096]).unwrap();
    assert!(first.set_read_cache(pool.clone()).is_err());
    let mut second = StorageEngineRocksDb::new(path);
    second
        .set_read_cache(SsdReadCache::new(1024 * 1024))
        .unwrap();
    assert!(!second.start());
    second.set_read_cache(pool).unwrap();
    assert!(second.start());
    assert_eq!(second.get("key").unwrap().value(), vec![7; 4096]);
    assert!(first.stop());
    assert!(second.stop());
    second
        .set_read_cache(SsdReadCache::new(512 * 1024))
        .unwrap();
    assert!(second.start());
    assert!(second.stop());
}

#[cfg(feature = "rocksdb-ssd")]
#[test]
fn shared_ssd_read_cache_survives_shard_cloning_and_disabled_tier_activation() {
    use matrixcache::{CacheKey, CacheOptions, CacheTier, ShardedMultiLayerCache, SsdReadCache};
    for pool in [
        SsdReadCache::new(512 * 1024),
        SsdReadCache::new_with_write_buffer_budget(8 * 1024 * 1024, 2 * 1024 * 1024, true).unwrap(),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let mut options = CacheOptions::new(0, 0, 0).with_ssd_paths([directory.path().join("ssd")]);
        options.ssd_read_cache = Some(pool.clone());
        assert_eq!(options.clone().ssd_read_cache, Some(pool.clone()));
        let cache = ShardedMultiLayerCache::with_options(options, 3);
        cache.start().unwrap();
        cache.set_capacity_for_tier(CacheTier::Ssd, 8 * 1024 * 1024);
        for i in 0..64 {
            cache
                .put(
                    CacheKey::string(0, &format!("key-{i}")),
                    vec![i as u8; 16384],
                )
                .unwrap();
        }
        assert!(cache.stop());
        cache.start().unwrap();
        for i in 0..64 {
            assert_eq!(
                cache
                    .get(&CacheKey::string(0, &format!("key-{i}")))
                    .unwrap()
                    .unwrap(),
                vec![i as u8; 16384]
            );
        }
        assert!(pool.usage_bytes() > 0);
        assert!(pool.usage_bytes() <= pool.capacity_bytes());
        assert!(cache.stop());
        if pool.write_buffer_capacity_bytes().is_some() {
            assert_eq!(pool.write_buffer_usage_bytes(), Some(0));
        }
    }
}

#[cfg(feature = "rocksdb-ssd")]
#[test]
fn sharded_ssd_write_policy_reaches_newly_enabled_tiers() {
    use matrixcache::{CacheKey, CacheOptions, CacheTier, ShardedMultiLayerCache, SsdWritePolicy};
    let directory = tempfile::tempdir().unwrap();
    let options = CacheOptions::new(0, 0, 0).with_ssd_paths([directory.path().join("ssd")]);
    let cache = ShardedMultiLayerCache::with_options(options, 3);
    assert_eq!(
        cache.ssd_write_policies(),
        vec![SsdWritePolicy::Buffered; 3]
    );
    cache.set_ssd_write_policy(SsdWritePolicy::Rebuildable);
    cache.start().unwrap();
    cache.set_capacity_for_tier(CacheTier::Ssd, 256 * 1024);
    assert_eq!(
        cache.ssd_write_policies(),
        vec![SsdWritePolicy::Rebuildable; 3]
    );
    for i in 0..64 {
        let key = CacheKey::string(0, &format!("key-{i}"));
        cache.put(key.clone(), vec![i as u8; 1024]).unwrap();
        assert_eq!(cache.get(&key).unwrap().unwrap(), vec![i as u8; 1024]);
    }
    cache.set_ssd_write_policy(SsdWritePolicy::Durable);
    assert_eq!(cache.ssd_write_policies(), vec![SsdWritePolicy::Durable; 3]);
    assert!(cache.stop());
}

#[cfg(feature = "rocksdb-ssd")]
#[test]
fn multilayer_ssd_policy_survives_enable_and_restart_on_single_and_multi_device_stores() {
    use matrixcache::{CacheKey, CacheOptions, CacheTier, MultiLayerCache, SsdWritePolicy};
    for devices in [1, 3] {
        for capacity in [0, 128 * 1024] {
            let directory = tempfile::tempdir().unwrap();
            let paths: Vec<_> = (0..devices)
                .map(|i| directory.path().join(format!("ssd-{i}")))
                .collect();
            let options = CacheOptions::new(0, 0, capacity).with_ssd_paths(paths.clone());
            let cache = MultiLayerCache::with_options(options);
            assert_eq!(cache.ssd_write_policy(), SsdWritePolicy::Buffered);
            cache.set_ssd_write_policy(SsdWritePolicy::Rebuildable);
            cache.start().unwrap();
            if capacity == 0 {
                cache.set_capacity_for_tier(CacheTier::Ssd, 128 * 1024);
            }
            assert_eq!(cache.ssd_write_policy(), SsdWritePolicy::Rebuildable);
            for i in 0..64 {
                cache
                    .put(CacheKey::string(0, &format!("key-{i}")), vec![7; 1024])
                    .unwrap();
            }
            let wal_bytes = || -> u64 {
                paths
                    .iter()
                    .map(|path| {
                        std::fs::read_dir(path.join("rocksdb-cache-blocks"))
                            .unwrap()
                            .map(|file| file.unwrap())
                            .filter(|file| file.path().extension().is_some_and(|ext| ext == "log"))
                            .map(|file| file.metadata().unwrap().len())
                            .sum::<u64>()
                    })
                    .sum()
            };
            assert_eq!(wal_bytes(), 0);
            for i in 0..64 {
                let key = CacheKey::string(0, &format!("key-{i}"));
                assert_eq!(
                    cache.get_no_promotion(&key).unwrap().unwrap().value,
                    vec![7; 1024]
                );
            }
            cache.set_ssd_write_policy(SsdWritePolicy::Durable);
            let durable = CacheKey::string(0, "durable");
            cache.put(durable.clone(), vec![8; 1024]).unwrap();
            assert!(wal_bytes() > 0);
            assert!(cache.stop());
            cache.start().unwrap();
            assert_eq!(cache.ssd_write_policy(), SsdWritePolicy::Durable);
            assert_eq!(
                cache.get_no_promotion(&durable).unwrap().unwrap().value,
                vec![8; 1024]
            );
            cache.set_ssd_write_policy(SsdWritePolicy::Buffered);
            assert_eq!(cache.ssd_write_policy(), SsdWritePolicy::Buffered);
            assert!(cache.stop());
        }
    }
}

#[cfg(feature = "rocksdb-ssd")]
#[test]
fn native_ssd_write_policies_control_wal_for_single_and_multi_device_writes() {
    use matrixcache::{SsdWritePolicy, StorageEngineMultiSsd, StorageEngineRocksDb};
    fn wal_bytes(path: &std::path::Path) -> u64 {
        std::fs::read_dir(path)
            .unwrap()
            .map(|file| file.unwrap())
            .filter(|file| file.path().extension().is_some_and(|ext| ext == "log"))
            .map(|file| file.metadata().unwrap().len())
            .sum()
    }
    for policy in [
        SsdWritePolicy::Buffered,
        SsdWritePolicy::Durable,
        SsdWritePolicy::Rebuildable,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("single");
        let mut engine = StorageEngineRocksDb::new(path.to_string_lossy());
        assert_eq!(engine.write_policy(), SsdWritePolicy::Buffered);
        engine = engine.with_write_policy(policy);
        assert_eq!(engine.clone().write_policy(), policy);
        assert!(engine.start());
        engine.put("one", vec![1; 1024]).unwrap();
        engine.async_put(buffer("two", &[2; 1024]), |_| {}).unwrap();
        for result in engine.put_buffers_batch(vec![
            buffer("three", &[3; 1024]),
            buffer("four", &[4; 1024]),
        ]) {
            result.unwrap();
        }
        assert_eq!(engine.get("one").unwrap().value(), &[1; 1024]);
        engine.delete("one").unwrap();
        assert!(matches!(engine.get("one"), Err(CacheError::NotFound)));
        engine.reset().unwrap();
        assert!(matches!(engine.get("two"), Err(CacheError::NotFound)));
        assert_eq!(wal_bytes(&path) == 0, policy == SsdWritePolicy::Rebuildable);
        assert!(engine.stop());

        let paths: Vec<_> = (0..3)
            .map(|i| directory.path().join(format!("multi-{i}")))
            .collect();
        let mut multi =
            StorageEngineMultiSsd::with_paths(paths[..2].to_vec(), 0).with_write_policy(policy);
        assert!(multi.start());
        assert!(multi.add_device(paths[2].to_string_lossy()));
        let inputs = (0..64)
            .map(|i| buffer(&format!("key-{i}"), &[7; 1024]))
            .collect();
        for result in multi.put_buffers_batch(inputs) {
            result.unwrap();
        }
        for i in 0..64 {
            assert_eq!(multi.get(&format!("key-{i}")).unwrap().value(), &[7; 1024]);
        }
        for path in &paths {
            assert_eq!(wal_bytes(path) == 0, policy == SsdWritePolicy::Rebuildable);
        }
        assert!(multi.stop());
    }
}

#[test]
fn queued_rocksdb_batches_preserve_versions_delete_order_and_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory
        .path()
        .join("queued-batches")
        .to_string_lossy()
        .into_owned();
    assert!(matches!(
        AsyncStorageRuntime::start_batched(
            matrixcache::StorageEngineRocksDb::new(path.clone()),
            8,
            4096,
            0,
            1024
        ),
        Err(CacheError::CapacityExceeded)
    ));
    let mut runtime = AsyncStorageRuntime::start_batched(
        matrixcache::StorageEngineRocksDb::new(path.clone()),
        8,
        4096,
        8,
        1024,
    )
    .unwrap();
    let (entered, ready) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let first = runtime
        .try_put(buffer("gate", b"gate"), move |_| {
            entered.send(()).unwrap();
            gate.recv_timeout(Duration::from_secs(5)).unwrap();
        })
        .unwrap();
    ready.recv_timeout(Duration::from_secs(5)).unwrap();
    let one = runtime.try_put(buffer("same", b"first"), |_| {}).unwrap();
    let two = runtime.try_put(buffer("same", b"second"), |_| {}).unwrap();
    let delete = runtime.try_delete("same", |_| {}).unwrap();
    let three = runtime.try_put(buffer("same", b"last"), |_| {}).unwrap();
    release.send(()).unwrap();
    first.wait().unwrap();
    assert_eq!(one.wait().unwrap().value(), b"first");
    assert_eq!(two.wait().unwrap().value(), b"second");
    delete.wait().unwrap();
    assert_eq!(three.wait().unwrap().value(), b"last");
    assert!(runtime.shutdown().unwrap());
    let mut engine = matrixcache::StorageEngineRocksDb::new(path);
    assert!(engine.start());
    assert_eq!(engine.get("same").unwrap().value(), b"last");
    assert!(engine.stop());
}

#[test]
fn multi_device_batches_keep_order_payloads_and_restart_values() {
    let directory = tempfile::tempdir().unwrap();
    let paths: Vec<_> = (0..3)
        .map(|i| directory.path().join(format!("device-{i}")))
        .collect();
    let mut engine = matrixcache::StorageEngineMultiSsd::with_paths(paths.clone(), 0);
    assert!(engine.start());
    let mut inputs = Vec::new();
    let mut expected = Vec::new();
    let mut devices = std::collections::BTreeSet::new();
    for round in 0..2 {
        for key in 0..64 {
            let name = format!("key-{key}");
            devices.insert(engine.device_for_key(&name).unwrap().to_string());
            let value = format!("value-{round}-{key}");
            let input = buffer(&name, value.as_bytes());
            expected.push((name, value, input.data_ptr() as usize));
            inputs.push(input);
        }
    }
    assert_eq!(devices.len(), 3);
    let results = engine.put_buffers_batch(inputs);
    assert_eq!(results.len(), expected.len());
    for (result, (key, value, address)) in results.into_iter().zip(expected) {
        let result = result.unwrap();
        assert_eq!(result.key(), key);
        assert_eq!(result.value(), value.as_bytes());
        #[cfg(feature = "rocksdb-ssd")]
        assert_eq!(result.data_ptr() as usize, address);
        #[cfg(not(feature = "rocksdb-ssd"))]
        let _ = address;
    }
    assert!(engine.stop());
    let mut reopened = matrixcache::StorageEngineMultiSsd::with_paths(paths, 0);
    assert!(reopened.start());
    for key in 0..64 {
        assert_eq!(
            reopened.get(&format!("key-{key}")).unwrap().value(),
            format!("value-1-{key}").as_bytes()
        );
    }
    assert!(reopened.stop());
    assert!(reopened.put_buffers_batch(Vec::new()).is_empty());
    let results = reopened.put_buffers_batch(vec![buffer("a", b"a"), buffer("b", b"b")]);
    assert_eq!(results.len(), 2);
    assert!(results
        .iter()
        .all(|result| matches!(result, Err(CacheError::Stopped))));
}

#[test]
fn shared_storage_batch_preserves_input_results_and_persisted_last_write() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory
        .path()
        .join("buffer-batch")
        .to_string_lossy()
        .into_owned();
    let mut engine = matrixcache::StorageEngineRocksDb::new(path.clone());
    assert!(engine.start());
    let first = buffer("same", b"first");
    let first_address = first.data_ptr() as usize;
    let last = buffer("same", b"last");
    let last_address = last.data_ptr() as usize;
    let results = engine.put_buffers_batch(vec![first, buffer("other", b"other"), last]);
    assert_eq!(results.len(), 3);
    for (result, expected) in results.iter().zip([b"first".as_slice(), b"other", b"last"]) {
        assert_eq!(result.as_ref().unwrap().value(), expected);
    }
    #[cfg(feature = "rocksdb-ssd")]
    {
        assert_eq!(
            results[0].as_ref().unwrap().data_ptr() as usize,
            first_address
        );
        assert_eq!(
            results[2].as_ref().unwrap().data_ptr() as usize,
            last_address
        );
    }
    #[cfg(not(feature = "rocksdb-ssd"))]
    let _ = (first_address, last_address);
    assert!(engine.stop());
    let mut reopened = matrixcache::StorageEngineRocksDb::new(path);
    assert!(reopened.start());
    assert_eq!(reopened.get("same").unwrap().value(), b"last");
    assert_eq!(reopened.get("other").unwrap().value(), b"other");
    assert!(reopened.stop());
    let failed = reopened.put_buffers_batch(vec![buffer("one", b"one"), buffer("two", b"two")]);
    assert_eq!(failed.len(), 2);
    assert!(failed
        .iter()
        .all(|result| matches!(result, Err(CacheError::Stopped))));
}

#[test]
fn default_storage_batch_keeps_partial_failures_in_input_order() {
    let mut engine = StorageEngineSimple::with_capacity(24);
    assert!(engine.start());
    let results = engine.put_buffers_batch(vec![
        buffer("a", b"first"),
        buffer("large", &[0; 100]),
        buffer("a", b"last"),
    ]);
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].as_ref().unwrap().value(), b"first");
    assert!(matches!(results[1], Err(CacheError::CapacityExceeded)));
    assert_eq!(results[2].as_ref().unwrap().value(), b"last");
    assert_eq!(engine.get("a").unwrap().value(), b"last");
    assert!(engine.stop());
}

#[test]
fn rejected_submissions_return_original_payload_and_release_reservations() {
    for byte_rejection in [false, true] {
        let limit = if byte_rejection { 8 } else { 32 };
        let mut runtime =
            AsyncStorageRuntime::start_with_byte_limit(StorageEngineSimple::new(), 1, limit)
                .unwrap();
        let (entered, ready) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let first = runtime
            .try_put(buffer("active", b"12345678"), move |_| {
                entered.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(5)).unwrap();
            })
            .unwrap();
        ready.recv_timeout(Duration::from_secs(5)).unwrap();
        let queued = if byte_rejection {
            None
        } else {
            Some(
                runtime
                    .try_put(buffer("queued", b"12345678"), |_| {})
                    .unwrap(),
            )
        };
        let input = buffer("retry", b"original");
        let address = input.data_ptr() as usize;
        let called = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let callback_called = Arc::clone(&called);
        let rejected = match runtime.try_put_recoverable(input, move |_| {
            callback_called.store(true, std::sync::atomic::Ordering::SeqCst);
        }) {
            Err(rejected) => rejected,
            Ok(_) => panic!("submission should be rejected"),
        };
        assert!(matches!(rejected.error, CacheError::CapacityExceeded));
        assert_eq!(rejected.buffer.key(), "retry");
        assert_eq!(rejected.buffer.data_ptr() as usize, address);
        release.send(()).unwrap();
        first.wait().unwrap();
        if let Some(queued) = queued {
            queued.wait().unwrap();
        }
        let result = runtime
            .try_put_recoverable(rejected.buffer, |_| {})
            .unwrap()
            .wait()
            .unwrap();
        assert_eq!(result.data_ptr() as usize, address);
        runtime
            .try_put(buffer("full-budget", &vec![0; limit]), |_| {})
            .unwrap()
            .wait()
            .unwrap();
        assert!(runtime.shutdown().unwrap());
        assert!(!called.load(std::sync::atomic::Ordering::SeqCst));
        let stopped = match runtime.try_put_recoverable(result, |_| {}) {
            Err(rejected) => rejected,
            Ok(_) => panic!("stopped submission accepted"),
        };
        assert!(matches!(stopped.error, CacheError::Stopped));
        assert_eq!(stopped.buffer.data_ptr() as usize, address);
    }
}

#[test]
fn queued_cache_instances_preserve_callback_pins_and_delete_order() {
    use matrixcache::{CacheInstance, ReplacementPolicyKind, StorageEngineKind};
    for kind in [
        StorageEngineKind::Dram,
        StorageEngineKind::Pmem,
        StorageEngineKind::Ssd,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let instance = CacheInstance::new(
            4096,
            ReplacementPolicyKind::Fifo,
            kind,
            vec![directory.path().join("tier")],
        );
        let observer = instance.clone();
        let mut runtime = AsyncStorageRuntime::start_cache_instance(instance, 4, 4096).unwrap();
        let (sender, receiver) = mpsc::channel();
        let input = buffer("record", b"original");
        let input_address = input.data_ptr() as usize;
        let completion = runtime
            .try_put(input, move |result| {
                sender.send(result.unwrap()).unwrap();
            })
            .unwrap();
        let result = completion.wait().unwrap();
        let callback_buffer = receiver.recv().unwrap();
        assert_eq!(result.value(), b"original");
        if kind != StorageEngineKind::Ssd {
            assert_eq!(result.data_ptr() as usize, input_address);
            assert_eq!(callback_buffer.data_ptr() as usize, input_address);
        }
        drop(result);
        observer
            .update("record", &callback_buffer, buffer("record", b"updated"))
            .unwrap();
        assert_eq!(observer.get("record").unwrap().unwrap(), b"updated");
        assert_eq!(callback_buffer.value(), b"original");
        drop(callback_buffer);
        runtime
            .try_delete("record", |_| {})
            .unwrap()
            .wait()
            .unwrap();
        assert!(observer.get("record").unwrap().is_none());
        assert!(runtime.shutdown().unwrap());
        assert!(matches!(observer.get("record"), Err(CacheError::Stopped)));
    }
}

#[test]
fn write_and_delete_futures_wake_without_blocking_poll() {
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll, Wake, Waker};
    struct Notify(mpsc::Sender<()>);
    impl Wake for Notify {
        fn wake(self: Arc<Self>) {
            let _ = self.0.send(());
        }
    }
    let mut runtime = AsyncStorageRuntime::start(StorageEngineSimple::new(), 4).unwrap();
    let (entered, ready) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let mut write = runtime
        .try_put(buffer("awaitable", b"payload"), move |_| {
            entered.send(()).unwrap();
            gate.recv_timeout(Duration::from_secs(5)).unwrap();
        })
        .unwrap();
    ready.recv_timeout(Duration::from_secs(5)).unwrap();
    let mut delete = runtime.try_delete("awaitable", |_| {}).unwrap();
    let (notify, wakes) = mpsc::channel();
    let waker = Waker::from(Arc::new(Notify(notify)));
    let mut cx = Context::from_waker(&waker);
    assert!(matches!(Pin::new(&mut write).poll(&mut cx), Poll::Pending));
    assert!(matches!(Pin::new(&mut delete).poll(&mut cx), Poll::Pending));
    release.send(()).unwrap();
    for _ in 0..2 {
        wakes.recv_timeout(Duration::from_secs(5)).unwrap();
    }
    assert!(
        matches!(Pin::new(&mut write).poll(&mut cx), Poll::Ready(Ok(result)) if result.value() == b"payload")
    );
    assert!(matches!(
        Pin::new(&mut delete).poll(&mut cx),
        Poll::Ready(Ok(()))
    ));
    assert!(runtime.shutdown().unwrap());
}

#[test]
fn dropped_completion_does_not_cancel_accepted_write() {
    let mut runtime = AsyncStorageRuntime::start(StorageEngineSimple::new(), 2).unwrap();
    let (sender, receiver) = mpsc::channel();
    drop(
        runtime
            .try_put(buffer("unobserved", b"payload"), move |result| {
                sender.send(result.unwrap().value().to_vec()).unwrap();
            })
            .unwrap(),
    );
    assert!(runtime.shutdown().unwrap());
    assert_eq!(receiver.recv().unwrap(), b"payload");
}

#[test]
fn queued_delete_follows_write_and_survives_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory
        .path()
        .join("delete-order")
        .to_string_lossy()
        .into_owned();
    let mut runtime =
        AsyncStorageRuntime::start(matrixcache::StorageEngineRocksDb::new(path.clone()), 4)
            .unwrap();
    let put = runtime
        .try_put(buffer("record", b"old-payload"), |_| {})
        .unwrap();
    let (sender, receiver) = mpsc::channel();
    let delete = runtime
        .try_delete("record", move |result| {
            sender.send(result.is_ok()).unwrap();
        })
        .unwrap();
    assert_eq!(put.wait().unwrap().value(), b"old-payload");
    delete.wait().unwrap();
    assert!(receiver.recv().unwrap());
    assert!(runtime.shutdown().unwrap());
    let mut reopened = matrixcache::StorageEngineRocksDb::new(path);
    assert!(reopened.start());
    assert!(matches!(reopened.get("record"), Err(CacheError::NotFound)));
    assert!(reopened.stop());
    assert!(matches!(
        runtime.try_delete("record", |_| {}),
        Err(CacheError::Stopped)
    ));
}

#[test]
fn queued_disk_write_survives_engine_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory
        .path()
        .join("storage")
        .to_string_lossy()
        .into_owned();
    let engine = matrixcache::StorageEngineRocksDb::new(path.clone());
    let mut runtime = AsyncStorageRuntime::start(engine, 4).unwrap();
    let completed = runtime
        .try_put(buffer("persisted", b"disk-payload"), |_| {})
        .unwrap();
    assert_eq!(completed.wait().unwrap().value(), b"disk-payload");
    assert!(runtime.shutdown().unwrap());
    let mut reopened = matrixcache::StorageEngineRocksDb::new(path);
    assert!(reopened.start());
    assert_eq!(reopened.get("persisted").unwrap().value(), b"disk-payload");
    assert!(reopened.stop());
}

#[cfg(feature = "rocksdb-ssd")]
#[test]
fn rocksdb_worker_callback_and_completion_share_input_payload() {
    let directory = tempfile::tempdir().unwrap();
    let engine = matrixcache::StorageEngineRocksDb::new(
        directory
            .path()
            .join("shared")
            .to_string_lossy()
            .into_owned(),
    );
    assert_worker_shares_payload(engine);
    let engine = matrixcache::StorageEngineMultiSsd::with_paths(
        [
            directory.path().join("device-a"),
            directory.path().join("device-b"),
        ],
        2 * 1024 * 1024,
    );
    assert_worker_shares_payload(engine);
}

fn assert_worker_shares_payload<E: StorageEngineApi + Send + 'static>(engine: E) {
    let mut runtime = AsyncStorageRuntime::start_with_byte_limit(engine, 2, 1024 * 1024).unwrap();
    let input = buffer("shared", &vec![7; 1024 * 1024]);
    let input_address = input.data_ptr() as usize;
    let (sender, receiver) = mpsc::channel();
    let completion = runtime
        .try_put(input, move |result| {
            let result = result.unwrap();
            sender.send(result.data_ptr() as usize).unwrap();
        })
        .unwrap();
    let result = completion.wait().unwrap();
    assert_eq!(result.data_ptr() as usize, input_address);
    assert_eq!(receiver.recv().unwrap(), input_address);
    assert_eq!(result.value(), &vec![7; 1024 * 1024]);
    assert!(runtime.shutdown().unwrap());
}

#[test]
fn memory_worker_callback_and_completion_share_input_payload() {
    assert_worker_shares_payload(StorageEngineSimple::new());
}

fn buffer(key: &str, value: &[u8]) -> CacheBuffer {
    let mut buffer = CacheBuffer::new(value.to_vec());
    buffer.set_key(key);
    buffer
}

#[test]
fn payload_budget_includes_active_callbacks_and_recovers_after_completion() {
    let mut runtime =
        AsyncStorageRuntime::start_with_byte_limit(StorageEngineSimple::new(), 8, 8).unwrap();
    let (entered, ready) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let first = runtime
        .try_put(buffer("first", b"12345678"), move |_| {
            entered.send(()).unwrap();
            gate.recv_timeout(Duration::from_secs(5)).unwrap();
        })
        .unwrap();
    ready.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(matches!(
        runtime.try_put(buffer("blocked", b"x"), |_| panic!("rejected callback")),
        Err(CacheError::CapacityExceeded)
    ));
    release.send(()).unwrap();
    first.wait().unwrap();
    runtime
        .try_put(buffer("next", b"12345678"), |_| {})
        .unwrap()
        .wait()
        .unwrap();
    assert!(matches!(
        runtime.try_put(buffer("oversized", b"123456789"), |_| {}),
        Err(CacheError::CapacityExceeded)
    ));
    assert!(runtime.shutdown().unwrap());
}

#[test]
fn worker_bounds_pending_writes_and_drains_in_order() {
    let mut runtime = AsyncStorageRuntime::start(StorageEngineSimple::new(), 1).unwrap();
    let caller = std::thread::current().id();
    let (entered, started) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let order = Arc::new(Mutex::new(Vec::new()));
    let first_order = Arc::clone(&order);
    let first = runtime
        .try_put(buffer("one", b"first"), move |result| {
            assert_eq!(result.unwrap().value(), b"first");
            assert_ne!(std::thread::current().id(), caller);
            entered.send(()).unwrap();
            gate.recv_timeout(Duration::from_secs(5)).unwrap();
            first_order.lock().unwrap().push(1);
        })
        .unwrap();
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    let second_order = Arc::clone(&order);
    let second = runtime
        .try_put(buffer("two", b"second"), move |result| {
            assert_eq!(result.unwrap().value(), b"second");
            second_order.lock().unwrap().push(2);
        })
        .unwrap();
    assert!(matches!(
        runtime.try_put(buffer("three", b"rejected"), |_| panic!(
            "rejected callback"
        )),
        Err(CacheError::CapacityExceeded)
    ));
    release.send(()).unwrap();
    assert!(runtime.shutdown().unwrap());
    assert_eq!(first.wait().unwrap().value(), b"first");
    assert_eq!(second.wait().unwrap().value(), b"second");
    assert_eq!(*order.lock().unwrap(), vec![1, 2]);
    assert!(matches!(
        runtime.try_put(buffer("late", b"stopped"), |_| {}),
        Err(CacheError::Stopped)
    ));
}

#[test]
fn storage_errors_reach_callback_and_completion() {
    let mut engine = StorageEngineSimple::new();
    engine.set_capacity(1);
    let mut runtime = AsyncStorageRuntime::start(engine, 2).unwrap();
    let (sender, receiver) = mpsc::channel();
    let completed = runtime
        .try_put(buffer("large", b"too large"), move |result| {
            sender
                .send(matches!(result, Err(CacheError::CapacityExceeded)))
                .unwrap();
        })
        .unwrap();
    assert!(matches!(
        completed.wait(),
        Err(CacheError::CapacityExceeded)
    ));
    assert!(receiver.recv().unwrap());
    assert!(runtime.shutdown().unwrap());
}

#[test]
fn callback_panic_does_not_strand_later_writes() {
    let mut runtime = AsyncStorageRuntime::start(StorageEngineSimple::new(), 2).unwrap();
    let first = runtime
        .try_put(buffer("one", b"first"), |_| {
            panic!("caller callback failed")
        })
        .unwrap();
    let second = runtime.try_put(buffer("two", b"second"), |_| {}).unwrap();
    assert_eq!(first.wait().unwrap().value(), b"first");
    assert_eq!(second.wait().unwrap().value(), b"second");
    assert!(runtime.shutdown().unwrap());
}
