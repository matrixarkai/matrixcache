// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

use super::*;

#[test]
fn heap_allocator_capacity_tracks_full_regions_across_partial_frees_and_clones() {
    let mut allocator = JeAllocator::new(32);
    let first = allocator.allocate(16).unwrap();
    let second = allocator.allocate(16).unwrap();
    assert!(matches!(
        allocator.allocate(1),
        Err(CacheError::CapacityExceeded)
    ));
    allocator.free(first, 1).unwrap();
    assert_eq!(allocator.stats().unwrap().num_occupied_bytes, 16);
    assert_eq!(allocator.stats().unwrap().num_freed_bytes, 1);
    let mut cloned = allocator.clone();
    cloned.free(second, 0).unwrap();
    assert_eq!(cloned.stats().unwrap().num_occupied_bytes, 0);
    assert_eq!(allocator.stats().unwrap().num_occupied_bytes, 16);
    assert!(matches!(
        allocator.free(first, 16),
        Err(CacheError::NotFound)
    ));
    assert!(allocator.allocate(16).is_ok());
    assert!(matches!(
        allocator.allocate(1),
        Err(CacheError::CapacityExceeded)
    ));
}

#[test]
fn log_allocator_capacity_reuses_full_region_after_partial_free_accounting() {
    let mut allocator = SimpleLogBasedMemoryAllocator::with_capacity(64);
    let addresses = (0..4)
        .map(|_| allocator.allocate(16).unwrap())
        .collect::<Vec<_>>();
    assert!(matches!(
        allocator.allocate(1),
        Err(CacheError::CapacityExceeded)
    ));
    allocator.free(addresses[0], 0).unwrap();
    let replacement = allocator.allocate(16).unwrap();
    assert!(matches!(
        allocator.allocate(1),
        Err(CacheError::CapacityExceeded)
    ));
    assert!(matches!(
        allocator.free(addresses[0], 16),
        Err(CacheError::NotFound)
    ));
    allocator.free(replacement, 1).unwrap();
    assert!(allocator.allocate(16).is_ok());
    assert!(matches!(
        allocator.allocate(1),
        Err(CacheError::CapacityExceeded)
    ));
}

#[test]
fn record_relocation_updates_cache_owner_with_original_identity() {
    let mut owner = CacheInstance::new(
        1024,
        ReplacementPolicyKind::Fifo,
        StorageEngineKind::Dram,
        Vec::new(),
    );
    owner.start().unwrap();
    owner.put("record", b"payload".to_vec()).unwrap();
    let expected = owner.get_cache_buffer("record").unwrap().unwrap();
    let record = MemStorage::do_put("record", b"payload");
    let mut allocator = SimpleLogBasedMemoryAllocator::with_capacity(1024);
    let old = allocator.allocate(record.len()).unwrap();
    allocator.write(old, &record).unwrap();
    let new = MemStorage::relocate_record(&mut allocator, old, &expected, &mut owner).unwrap();
    assert_eq!(owner.get("record").unwrap().unwrap(), b"payload");
    assert_eq!(expected.data(), b"payload");
    assert!(matches!(allocator.read(old), Err(CacheError::NotFound)));
    assert!(allocator.read(new).is_ok());
    assert!(matches!(
        MemStorage::relocate_record(&mut allocator, new, &expected, &mut owner),
        Err(CacheError::ReplaceMismatch)
    ));
    assert!(allocator.read(new).is_ok());
}

#[test]
fn controller_live_relocation_obeys_start_pause_and_stop() {
    struct Owner(AllocatorAddress);
    impl LogBasedAllocatorGcEventListener for Owner {
        fn on_gc_copy(
            &mut self,
            old: AllocatorAddress,
            new: AllocatorAddress,
        ) -> Result<(), CacheError> {
            if self.0 != old {
                return Err(CacheError::ReplaceMismatch);
            }
            self.0 = new;
            Ok(())
        }
    }
    let mut allocator = SimpleLogBasedMemoryAllocator::with_capacity(64);
    let old = allocator.allocate(8).unwrap();
    allocator.write(old, b"gc-owner").unwrap();
    let mut owner = Owner(old);
    let mut controller = StorageGcController::new(allocator, false);
    assert_eq!(
        controller.relocate_live_region(old, &mut owner).unwrap(),
        None
    );
    controller.start();
    controller.set_pause_gc(true);
    assert_eq!(
        controller.relocate_live_region(old, &mut owner).unwrap(),
        None
    );
    controller.set_pause_gc(false);
    let new = controller
        .relocate_live_region(old, &mut owner)
        .unwrap()
        .unwrap();
    assert_eq!(owner.0, new);
    assert_eq!(controller.allocator().read(new).unwrap(), b"gc-owner");
    controller.stop();
    assert_eq!(
        controller.relocate_live_region(new, &mut owner).unwrap(),
        None
    );
}

#[test]
fn allocator_relocation_capacity_failure_leaves_source_and_owner_untouched() {
    struct MustNotCall;
    impl LogBasedAllocatorGcEventListener for MustNotCall {
        fn on_gc_copy(
            &mut self,
            _: AllocatorAddress,
            _: AllocatorAddress,
        ) -> Result<(), CacheError> {
            panic!("owner called before a relocation target was allocated");
        }
    }
    let mut allocator = SimpleLogBasedMemoryAllocator::with_capacity(8);
    let old = allocator.allocate(8).unwrap();
    allocator.write(old, b"original").unwrap();
    assert!(matches!(
        allocator.relocate(old, &mut MustNotCall),
        Err(CacheError::CapacityExceeded)
    ));
    assert_eq!(allocator.read(old).unwrap(), b"original");
    assert_eq!(allocator.live_region_count(), 1);
    assert!(matches!(
        allocator.relocate(old + 100, &mut MustNotCall),
        Err(CacheError::NotFound)
    ));
}

#[test]
fn allocator_relocation_preserves_checksum_metadata() {
    struct Accept;
    impl LogBasedAllocatorGcEventListener for Accept {
        fn on_gc_copy(
            &mut self,
            _: AllocatorAddress,
            _: AllocatorAddress,
        ) -> Result<(), CacheError> {
            Ok(())
        }
    }
    let mut allocator = SimpleLogBasedMemoryAllocator::with_capacity(64);
    let old = allocator.allocate(8).unwrap();
    allocator.write(old, b"verified").unwrap();
    let checksum = crc32c(b"verified");
    allocator.seal_with_crc(old, 8, checksum).unwrap();
    let new = allocator.relocate(old, &mut Accept).unwrap();
    assert!(allocator.sealed(new));
    assert_eq!(allocator.crc32(new), Some(checksum));
    assert_eq!(allocator.read(new).unwrap(), b"verified");
}

#[test]
fn log_allocator_address_exhaustion_preserves_live_region() {
    let mut allocator =
        SimpleLogBasedMemoryAllocator::with_capacity_and_base(64, AllocatorAddress::MAX - 1);
    let address = allocator.allocate(8).unwrap();
    allocator.write(address, b"original").unwrap();
    assert!(matches!(
        allocator.allocate(8),
        Err(CacheError::CapacityExceeded)
    ));
    assert_eq!(allocator.read(address).unwrap(), b"original");
    assert_eq!(allocator.live_region_count(), 1);
}

#[test]
fn allocator_relocation_preserves_data_and_rolls_back_rejection() {
    struct Listener {
        reject: bool,
        moved: Option<(AllocatorAddress, AllocatorAddress)>,
    }
    impl LogBasedAllocatorGcEventListener for Listener {
        fn on_gc_copy(
            &mut self,
            old: AllocatorAddress,
            new: AllocatorAddress,
        ) -> Result<(), CacheError> {
            if self.reject {
                return Err(CacheError::ReplaceMismatch);
            }
            self.moved = Some((old, new));
            Ok(())
        }
    }
    let mut allocator = SimpleLogBasedMemoryAllocator::with_capacity(1024);
    let old = allocator.allocate(8).unwrap();
    allocator.write(old, b"payload!").unwrap();
    allocator.seal(old).unwrap();
    let mut listener = Listener {
        reject: true,
        moved: None,
    };
    assert!(matches!(
        allocator.relocate(old, &mut listener),
        Err(CacheError::ReplaceMismatch)
    ));
    assert_eq!(allocator.read(old).unwrap(), b"payload!");
    assert_eq!(allocator.live_region_count(), 1);
    listener.reject = false;
    let new = allocator.relocate(old, &mut listener).unwrap();
    assert_ne!(old, new);
    assert_eq!(listener.moved, Some((old, new)));
    assert_eq!(allocator.read(new).unwrap(), b"payload!");
    assert!(allocator.sealed(new));
    assert!(matches!(allocator.read(old), Err(CacheError::NotFound)));
    assert_eq!(allocator.live_region_count(), 1);
}

#[test]
fn stale_handle_release_after_reset_preserves_new_generation() {
    for batch in [false, true] {
        let (_dir, cache) = cache();
        cache.start().unwrap();
        let key = CacheKey::string(0, "reset-generation");
        let old = cache
            .insert_pinned(key.clone(), vec![1; 8])
            .unwrap()
            .unwrap();
        cache.reset().unwrap();
        let new = cache
            .insert_pinned(key.clone(), vec![2; 16])
            .unwrap()
            .unwrap();
        if batch {
            cache.release_batch(vec![old]);
        } else {
            cache.release(old);
        }
        assert_eq!(cache.stats().pinned_entries, 1);
        assert_eq!(cache.pin_registry.payload_bytes(), 16);
        cache.release(new);
        assert_eq!(cache.stats().pinned_entries, 0);
        assert_eq!(cache.pin_registry.payload_bytes(), 0);
    }
}

#[test]
fn atomic_payload_counter_cleans_up_explicit_unpin_and_reset() {
    let (_dir, cache) = cache();
    cache.start().unwrap();
    let key = CacheKey::string(0, "counter-cleanup");
    let first = cache
        .insert_pinned(key.clone(), vec![1; 8])
        .unwrap()
        .unwrap();
    cache.unpin(&key);
    assert_eq!(cache.pin_registry.payload_bytes(), 0);
    cache.release(first);
    let second = cache.insert_pinned(key, vec![2; 16]).unwrap().unwrap();
    assert_eq!(cache.pin_registry.payload_bytes(), 16);
    cache.reset().unwrap();
    assert_eq!(cache.pin_registry.payload_bytes(), 0);
    cache.release(second);
    assert_eq!(cache.pin_registry.payload_bytes(), 0);
}

#[test]
fn atomic_pin_payload_counter_tracks_versions_and_clones() {
    let (_dir, cache) = cache();
    cache.start().unwrap();
    let key = CacheKey::string(0, "atomic-payload");
    let first = cache
        .insert_pinned(key.clone(), vec![1; 8])
        .unwrap()
        .unwrap();
    let cloned = first.clone_with_cache(&cache);
    cache.invalidate(&key).unwrap();
    let second = cache
        .insert_pinned(key.clone(), vec![2; 16])
        .unwrap()
        .unwrap();
    assert_eq!(cache.pin_registry.payload_bytes(), 24);
    cache.release(second);
    assert_eq!(cache.pin_registry.payload_bytes(), 8);
    cache.release_batch(vec![first, cloned]);
    assert_eq!(cache.pin_registry.payload_bytes(), 0);
}

#[test]
fn allocation_byte_deltas_count_first_pin_and_last_release_only() {
    let value: Arc<[u8]> = Arc::from(vec![1; 16]);
    let mut entry = PinnedEntry {
        handles: 2,
        ..PinnedEntry::default()
    };
    assert_eq!(entry.remember_value(&value), 16);
    assert_eq!(entry.remember_value(&value), 0);
    assert_eq!(entry.release_value(&value), 0);
    assert_eq!(entry.release_value(&value), 16);
    assert_eq!(entry.release_value(&value), 0);
}

#[test]
fn upper_tier_bypass_admission_respects_retained_pressure() {
    let (_dir, cache) = cache();
    cache.start().unwrap();
    let old_key = CacheKey::string(0, "held-pressure");
    let incoming = CacheKey::string(0, "bypass-pressure");
    let mut handles = Vec::new();
    for index in 0..20 {
        handles.push(
            cache
                .insert_pinned(old_key.clone(), vec![index; 8])
                .unwrap()
                .unwrap(),
        );
        cache.invalidate(&old_key).unwrap();
    }
    cache.put_memory_only(incoming.clone(), vec![9; 8]);
    assert_eq!(cache.peek_tier(&incoming), None);
    for tier in [CacheTier::Memory, CacheTier::Pmem] {
        cache
            .put_bypass_storage_for_tier(tier, incoming.clone(), vec![9; 8])
            .unwrap();
        assert_eq!(cache.peek_tier(&incoming), None);
    }
    assert_eq!(handles[0].as_slice(), &[0; 8]);
    cache.release_batch(handles);
    cache.put_memory_only(incoming.clone(), vec![9; 8]);
    assert_eq!(cache.peek_tier(&incoming), Some(CacheReadTier::Memory));
}

#[test]
fn retained_pressure_allows_reusing_existing_shared_payload() {
    let (_dir, cache) = cache();
    cache.start().unwrap();
    cache.set_capacity_for_tier(CacheTier::Pmem, 0);
    let key = CacheKey::string(0, "shared-budget");
    let mut handles = Vec::new();
    for index in 0..10 {
        handles.push(
            cache
                .insert_pinned(key.clone(), vec![index; 8])
                .unwrap()
                .unwrap(),
        );
        cache.invalidate(&key).unwrap();
    }
    let value = Arc::clone(&handles[0].value);
    cache.put_shared(key.clone(), Arc::clone(&value)).unwrap();
    assert!(Arc::ptr_eq(
        &cache.get_shared(&key).unwrap().unwrap(),
        &value
    ));
    cache.release_batch(handles);
}

#[test]
fn ssd_fallback_does_not_bypass_pinned_retention_guard() {
    let dir = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(64, 0, 4096).with_ssd_paths([dir.path().join("ssd")]),
    );
    cache.start().unwrap();
    let key = CacheKey::string(0, "ssd-pin-budget");
    let mut handles = Vec::new();
    for index in 0..10 {
        handles.push(
            cache
                .insert_pinned(key.clone(), vec![index; 8])
                .unwrap()
                .unwrap(),
        );
        cache.invalidate(&key).unwrap();
    }
    assert!(matches!(
        cache.insert_pinned(key.clone(), vec![9; 8]),
        Err(CacheError::CapacityExceeded)
    ));
    cache.put(key.clone(), vec![7; 8]).unwrap();
    assert_eq!(cache.peek_tier(&key), Some(CacheReadTier::Ssd));
    cache.release_batch(handles);
    assert!(cache.insert_pinned(key, vec![8; 8]).unwrap().is_some());
}

#[test]
fn retained_payload_pressure_rejects_then_recovers_after_release() {
    let (_dir, cache) = cache();
    cache.start().unwrap();
    cache.set_capacity_for_tier(CacheTier::Pmem, 0);
    let key = CacheKey::string(0, "bounded-retention");
    let mut handles = Vec::new();
    let mut rejected = false;
    for index in 0..32 {
        match cache.insert_pinned(key.clone(), vec![index; 8]) {
            Ok(Some(handle)) => {
                handles.push(handle);
                cache.invalidate(&key).unwrap();
            }
            Err(CacheError::CapacityExceeded) => {
                rejected = true;
                break;
            }
            other => panic!("unexpected result: {other:?}"),
        }
    }
    assert!(rejected);
    assert!(cache.stats().pinned_bytes <= 83);
    cache.release_batch(handles);
    assert!(cache.insert_pinned(key, vec![9; 8]).unwrap().is_some());
}

#[test]
fn concurrent_pinned_versions_account_until_batch_release() {
    let (_dir, cache) = cache();
    let cache = Arc::new(cache);
    cache.start().unwrap();
    let key = CacheKey::string(0, "concurrent-generations");
    let workers = (0..8)
        .map(|index| {
            let cache = Arc::clone(&cache);
            let key = key.clone();
            std::thread::spawn(move || {
                let handle = cache.insert_pinned(key, vec![index; 8]).unwrap().unwrap();
                assert_eq!(handle.as_slice(), &[index; 8]);
                handle
            })
        })
        .collect::<Vec<_>>();
    let mut handles = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect::<Vec<_>>();
    cache.invalidate(&key).unwrap();
    assert_eq!(cache.stats().pinned_bytes, 64);
    assert_eq!(cache.size(), 64);
    let remaining = handles.split_off(4);
    assert_eq!(cache.release_batch(handles), 4);
    assert_eq!(cache.stats().pinned_bytes, 32);
    assert_eq!(cache.size(), 32);
    assert_eq!(cache.release_batch(remaining), 4);
    assert_eq!(cache.stats().pinned_bytes, 0);
    assert_eq!(cache.size(), 0);
}

#[test]
fn retained_clone_batch_release_does_not_count_unpinned_resident() {
    let (_dir, cache) = cache();
    cache.start().unwrap();
    let key = CacheKey::string(0, "clone-resident");
    let first = cache
        .insert_pinned(key.clone(), vec![1; 8])
        .unwrap()
        .unwrap();
    let cloned = first.clone_with_cache(&cache);
    cache.invalidate(&key).unwrap();
    let second = cache
        .insert_pinned(key.clone(), vec![2; 16])
        .unwrap()
        .unwrap();
    assert_eq!(cache.stats().pinned_bytes, 24);
    cache.release(second);
    assert_eq!(cache.stats().pinned_bytes, 8);
    assert_eq!(cache.size(), 24);
    assert_eq!(cache.release_batch(vec![first, cloned]), 2);
    assert_eq!(cache.stats().pinned_bytes, 0);
    assert_eq!(cache.size(), 16);
}

#[test]
fn retained_versions_release_larger_allocation_first() {
    let (_dir, cache) = cache();
    cache.start().unwrap();
    let key = CacheKey::string(0, "reverse-release");
    let first = cache
        .insert_pinned(key.clone(), vec![1; 8])
        .unwrap()
        .unwrap();
    cache.invalidate(&key).unwrap();
    let second = cache
        .insert_pinned(key.clone(), vec![2; 16])
        .unwrap()
        .unwrap();
    cache.invalidate(&key).unwrap();
    assert_eq!(cache.stats().pinned_bytes, 24);
    cache.release(second);
    assert_eq!(cache.stats().pinned_bytes, 8);
    cache.release(first);
    assert_eq!(cache.stats().pinned_bytes, 0);
}

#[test]
fn batch_and_no_promotion_versions_account_retained_values() {
    for mode in 0..3 {
        let (_dir, cache) = cache();
        cache.start().unwrap();
        let key = CacheKey::string(0, "batch-retained");
        let acquire = || match mode {
            0 => cache.acquire_no_promotion(&key).unwrap().unwrap(),
            1 => cache
                .acquire_batch(std::slice::from_ref(&key))
                .unwrap()
                .pop()
                .unwrap()
                .unwrap(),
            _ => cache
                .acquire_batch_no_promotion(std::slice::from_ref(&key))
                .unwrap()
                .pop()
                .unwrap()
                .unwrap(),
        };
        cache.put(key.clone(), vec![1; 8]).unwrap();
        let first = acquire();
        cache.invalidate(&key).unwrap();
        cache.put(key.clone(), vec![2; 16]).unwrap();
        let second = acquire();
        cache.invalidate(&key).unwrap();
        assert_eq!(cache.stats().pinned_bytes, 24, "mode={mode}");
        cache.release(first);
        assert_eq!(cache.stats().pinned_bytes, 16);
        cache.release(second);
        assert_eq!(cache.stats().pinned_bytes, 0);
    }
}

#[test]
fn acquired_versions_account_distinct_retained_values() {
    let (_dir, cache) = cache();
    cache.start().unwrap();
    let key = CacheKey::string(0, "acquired-versions");
    cache.put(key.clone(), vec![1; 8]).unwrap();
    let first = cache.acquire(&key).unwrap().unwrap();
    cache.invalidate(&key).unwrap();
    cache.put(key.clone(), vec![2; 16]).unwrap();
    let second = cache.acquire(&key).unwrap().unwrap();
    cache.invalidate(&key).unwrap();
    assert_eq!(cache.stats().pinned_bytes, 24);
    cache.release(first);
    assert_eq!(cache.stats().pinned_bytes, 16);
    cache.release(second);
    assert_eq!(cache.stats().pinned_bytes, 0);
}

#[test]
fn generation_tracker_deduplicates_and_releases_allocations() {
    let first: Arc<[u8]> = Arc::from(vec![1; 8]);
    let second: Arc<[u8]> = Arc::from(vec![2; 16]);
    let mut entry = PinnedEntry {
        handles: 3,
        ..PinnedEntry::default()
    };
    entry.remember_value(&first);
    entry.remember_value(&first);
    entry.remember_value(&second);
    assert_eq!(entry.retained_value_bytes(), 24);
    drop(first);
    assert_eq!(entry.retained_value_bytes(), 16);
    drop(second);
    assert_eq!(entry.retained_value_bytes(), 0);
}

#[test]
fn diagnostic_distinct_retained_pin_versions() {
    let (_dir, cache) = cache();
    cache.start().unwrap();
    let key = CacheKey::string(0, "retained-versions");
    let first = cache
        .insert_pinned(key.clone(), vec![1; 8])
        .unwrap()
        .unwrap();
    cache.invalidate(&key).unwrap();
    let second = cache
        .insert_pinned(key.clone(), vec![2; 16])
        .unwrap()
        .unwrap();
    cache.invalidate(&key).unwrap();
    assert_eq!(first.as_slice(), &[1; 8]);
    assert_eq!(second.as_slice(), &[2; 16]);
    assert_eq!(cache.stats().pinned_bytes, 24);
    cache.release(first);
    assert_eq!(cache.stats().pinned_bytes, 16);
    cache.release(second);
    assert_eq!(cache.stats().pinned_bytes, 0);
}

// rust-internal: reentrant callbacks must not change the inserted handle's bytes.
#[test]
fn pinned_insert_returns_its_value_before_reentrant_overwrite() {
    let (_dir, cache) = cache();
    let cache = Arc::new(cache);
    cache.start().unwrap();
    let key = CacheKey::string(0, "pinned-callback");
    let weak = Arc::downgrade(&cache);
    let callback_key = key.clone();
    cache.register_access_record_callback(move |_| {
        let cache = weak.upgrade().unwrap();
        cache.clear_access_record_callback();
        cache.put(callback_key.clone(), b"new".to_vec()).unwrap();
    });
    let handle = cache
        .insert_pinned(key.clone(), b"old".to_vec())
        .unwrap()
        .unwrap();
    assert_eq!(handle.as_slice(), b"old");
    assert_eq!(cache.get(&key).unwrap().unwrap(), b"new");
    cache.release(handle);
    assert_eq!(cache.stats().pinned_entries, 0);
}

fn cache() -> (tempfile::TempDir, MultiLayerCache) {
    let dir = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(64, 64, 0)
            .with_ssd_paths([dir.path().join("ssd")])
            .with_pmem_paths([dir.path().join("pmem")]),
    );
    (dir, cache)
}

// rust-internal: metadata must move with a resident value without being duplicated.
#[test]
fn metadata_has_one_authority_across_pmem_and_memory_transitions() {
    let (_dir, cache) = cache();
    let key = CacheKey::string(0, "metadata-authority");
    let mut inner = cache.inner.write().unwrap();

    assert!(inner.put_pmem(&key, b"value".to_vec()));
    let shared = inner.shared_key(&key);
    inner.set_metadata(
        shared,
        CacheEntryMeta::new(
            CacheBlockKind::Other,
            Some(7),
            11,
            13,
            17,
            CacheAdmissionReason::PersistentMemory,
        ),
    );

    let metadata_homes = |inner: &CacheInner| {
        usize::from(
            inner
                .memory
                .get(&key)
                .is_some_and(|resident| resident.meta.is_some()),
        ) + usize::from(
            inner
                .pmem
                .get(&key)
                .is_some_and(|resident| resident.meta.is_some()),
        ) + usize::from(inner.metadata.contains_key(&key))
    };

    assert_eq!(metadata_homes(&inner), 1);
    assert!(inner.pmem.get(&key).unwrap().meta.is_some());
    assert!(!inner.metadata.contains_key(&key));
    assert_eq!(inner.metadata_of(&key).unwrap().hits(), 13);

    let pmem_value = inner.pmem_value(&key).unwrap().clone();
    let shared = inner.shared_key(&key);
    inner.make_resident(shared, pmem_value);
    assert_eq!(metadata_homes(&inner), 1);
    assert!(inner.memory.get(&key).unwrap().meta.is_some());
    assert!(inner.pmem.get(&key).unwrap().meta.is_none());
    assert_eq!(inner.metadata_of(&key).unwrap().hits(), 13);

    inner.take_resident(&key).unwrap();
    assert_eq!(metadata_homes(&inner), 1);
    assert!(inner.pmem.get(&key).unwrap().meta.is_some());
    assert!(!inner.metadata.contains_key(&key));
    assert_eq!(inner.metadata_of(&key).unwrap().hits(), 13);

    inner.take_pmem_resident(&key).unwrap();
    assert_eq!(metadata_homes(&inner), 1);
    assert!(inner.metadata.contains_key(&key));
    assert_eq!(inner.metadata_of(&key).unwrap().hits(), 13);
}

// rust-internal: shared PMEM accounting is carried into promotion exactly once.
#[test]
fn accounted_pmem_promotion_preserves_one_hit_and_one_metadata_authority() {
    let (_dir, cache) = cache();
    let key = CacheKey::string(0, "accounted-promotion");
    let mut inner = cache.inner.write().unwrap();
    assert!(inner.put_pmem(&key, b"value".to_vec()));
    let shared = inner.shared_key(&key);
    inner.set_metadata(
        shared,
        CacheEntryMeta::new(
            CacheBlockKind::Other,
            None,
            1,
            0,
            CoarseClock::now_millis(),
            CacheAdmissionReason::PersistentMemory,
        ),
    );
    let value = inner.pmem_value(&key).unwrap().clone();
    let outcome = inner.record_known_hit_shared_occurrences_at(
        &key,
        inner.pmem.get(&key).unwrap().meta.as_ref(),
        1,
        CoarseClock::now_millis(),
    );
    assert_eq!(inner.metadata_of(&key).unwrap().hits(), 1);

    inner.promote_accounted_pmem_probe_if_current(&key, &value, outcome);

    assert_eq!(inner.metadata_of(&key).unwrap().hits(), 1);
    assert!(inner.memory.get(&key).unwrap().meta.is_some());
    assert!(inner.pmem.get(&key).unwrap().meta.is_none());
    assert!(!inner.metadata.contains_key(&key));
}

// rust-internal: reads do not retry a memory placement that cannot succeed.
#[test]
fn pmem_reads_skip_structurally_impossible_memory_refills() {
    for (memory_capacity, threshold, value_len) in [
        (0_usize, usize::MAX, 8_usize),
        (4, usize::MAX, 8),
        (64, 4, 8),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_options(
            CacheOptions::new(memory_capacity, 64, 0)
                .with_ssd_paths([dir.path().join("ssd")])
                .with_pmem_paths([dir.path().join("pmem")]),
        );
        let key = CacheKey::string(0, &format!("no-refill-{memory_capacity}-{threshold}"));
        let mut inner = cache.inner.write().unwrap();
        if threshold != usize::MAX {
            inner.tiering_policy.data_placement = CacheDataPlacement::SideBySide;
            inner.tiering_policy.data_placement_threshold_bytes = threshold;
        }
        assert!(inner.put_pmem(&key, vec![b'v'; value_len]));
        let value = inner.pmem_value(&key).unwrap().clone();
        let failures = inner.stats.refill_failures;
        let rejections = inner.stats.memory_admission_rejected;

        inner.promote_pmem_probe_if_current(&key, &value);

        assert!(inner.resident_value(&key).is_none());
        assert_eq!(inner.stats.refill_failures, failures);
        assert_eq!(inner.stats.memory_admission_rejected, rejections);
        assert_eq!(inner.metadata_of(&key).unwrap().hits(), 1);
    }
}

// rust-internal: changing the side-by-side threshold must not turn PMEM reads
// into memory promotions for entries placed under the previous threshold.
#[test]
fn side_by_side_pmem_hit_stays_in_pmem_after_threshold_change() {
    let (_dir, cache) = cache();
    cache.start().unwrap();
    let key = CacheKey::string(0, "placement-change");
    let mut policy = cache.production_tiering_policy();
    policy.data_placement = CacheDataPlacement::SideBySide;
    policy.data_placement_threshold_bytes = 4;
    cache.update_production_tiering_policy(policy);
    cache.put(key.clone(), b"12345678".to_vec()).unwrap();
    assert_eq!(cache.peek_tier(&key), Some(CacheReadTier::Pmem));

    cache.set_data_placement_threshold_bytes(16);
    assert_eq!(
        cache.get_shared(&key).unwrap().unwrap().as_ref(),
        b"12345678"
    );
    assert_eq!(cache.peek_tier(&key), Some(CacheReadTier::Pmem));
    assert_eq!(cache.size_for_tier(CacheTier::Memory), 0);
}

// rust-internal: PMEM-only shared reads stay lock-light and do not report false refill failures.
#[test]
fn pmem_only_public_reads_stay_in_pmem_without_refill_failures() {
    let dir = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(0, 4096, 0)
            .with_ssd_paths([dir.path().join("ssd")])
            .with_pmem_paths([dir.path().join("pmem")]),
    );
    cache.start().unwrap();
    let mut policy = cache.production_tiering_policy();
    policy.data_placement = CacheDataPlacement::SideBySide;
    policy.data_placement_threshold_bytes = 0;
    cache.update_production_tiering_policy(policy);
    let keys = (0..4)
        .map(|index| CacheKey::string(0, &format!("pmem-only-{index}")))
        .collect::<Vec<_>>();
    for (index, key) in keys.iter().enumerate() {
        cache.put(key.clone(), vec![index as u8; 64]).unwrap();
        assert_eq!(cache.peek_tier(key), Some(CacheReadTier::Pmem));
    }
    let before = cache.stats();

    for key in &keys {
        assert!(cache.get_shared(key).unwrap().is_some());
    }
    let before_batch = cache.stats();
    assert!(cache
        .get_shared_batch(&keys)
        .unwrap()
        .iter()
        .all(Option::is_some));
    let after_batch = cache.stats();
    assert_eq!(after_batch.pmem_hits - before_batch.pmem_hits, 4);
    assert_eq!(
        after_batch.shared_buffer_hits - before_batch.shared_buffer_hits,
        4
    );
    assert_eq!(
        after_batch.get_latency_samples - before_batch.get_latency_samples,
        4
    );
    assert_eq!(
        after_batch.read_through_latency_samples - before_batch.read_through_latency_samples,
        4
    );

    let after = after_batch;
    assert_eq!(after.refill_failures, before.refill_failures);
    assert_eq!(
        after.memory_admission_rejected,
        before.memory_admission_rejected
    );
    assert_eq!(after.memory_fills, before.memory_fills);
    assert!(keys
        .iter()
        .all(|key| cache.peek_tier(key) == Some(CacheReadTier::Pmem)));
}

// rust-internal: switching to tiered placement refills owned PMEM batches.
#[test]
fn owned_pmem_batch_refills_memory_after_switch_to_tiered() {
    let dir = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(8192, 8192, 0)
            .with_pmem_paths([dir.path().join("pmem")])
            .with_dram_pmem_data_placement(CacheDataPlacement::SideBySide, 0),
    );
    cache.start().unwrap();
    let keys = (0..8)
        .map(|index| CacheKey::string(0, &format!("pmem-owned-batch-{index}")))
        .collect::<Vec<_>>();
    for (index, key) in keys.iter().enumerate() {
        cache.put(key.clone(), vec![index as u8; 64]).unwrap();
        assert_eq!(cache.peek_tier(key), Some(CacheReadTier::Pmem));
    }
    cache.set_data_placement(CacheDataPlacement::Tiered);

    let before = cache.stats();
    let values = cache.get_batch(&keys).unwrap();
    for (index, value) in values.iter().enumerate() {
        let value = value.as_ref().expect("PMEM batch value");
        assert_eq!(value.len(), 64);
        assert!(value.iter().all(|byte| *byte == index as u8));
    }

    let after = cache.stats();
    assert_eq!(after.pmem_hits - before.pmem_hits, 8);
    assert_eq!(after.misses, before.misses);
    assert_eq!(after.memory_fills - before.memory_fills, 8);
    assert_eq!(after.refill_failures, before.refill_failures);
    assert_eq!(after.get_latency_samples - before.get_latency_samples, 8);
    assert_eq!(
        after.read_through_latency_samples - before.read_through_latency_samples,
        8
    );
    assert_eq!(
        after.refill_latency_samples - before.refill_latency_samples,
        8
    );
    assert!(keys
        .iter()
        .all(|key| cache.peek_tier(key) == Some(CacheReadTier::Memory)));
}

// rust-internal: mixed resident batches refill after switching to tiered placement.
#[test]
fn owned_mixed_memory_pmem_batch_refills_after_switch_to_tiered() {
    let dir = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(8192, 8192, 0)
            .with_pmem_paths([dir.path().join("pmem")])
            .with_dram_pmem_data_placement(CacheDataPlacement::SideBySide, usize::MAX),
    );
    cache.start().unwrap();
    let keys = (0..8)
        .map(|index| CacheKey::string(0, &format!("mixed-owned-batch-{index}")))
        .collect::<Vec<_>>();
    for (index, key) in keys.iter().enumerate() {
        cache.set_data_placement_threshold_bytes(if index % 2 == 0 { usize::MAX } else { 0 });
        cache.put(key.clone(), vec![index as u8; 64]).unwrap();
    }
    for (index, key) in keys.iter().enumerate() {
        assert_eq!(
            cache.peek_tier(key),
            Some(if index % 2 == 0 {
                CacheReadTier::Memory
            } else {
                CacheReadTier::Pmem
            })
        );
    }
    cache.set_data_placement(CacheDataPlacement::Tiered);

    let before = cache.stats();
    let values = cache.get_batch(&keys).unwrap();
    for (index, value) in values.iter().enumerate() {
        let value = value.as_ref().expect("mixed resident value");
        assert_eq!(value.len(), 64);
        assert!(value.iter().all(|byte| *byte == index as u8));
    }

    let after = cache.stats();
    assert_eq!(after.memory_hits - before.memory_hits, 4);
    assert_eq!(after.pmem_hits - before.pmem_hits, 4);
    assert_eq!(after.shared_buffer_hits - before.shared_buffer_hits, 8);
    assert_eq!(after.memory_fills - before.memory_fills, 4);
    assert_eq!(after.misses, before.misses);
    assert_eq!(after.refill_failures, before.refill_failures);
    assert_eq!(after.get_latency_samples - before.get_latency_samples, 8);
    assert_eq!(
        after.refill_latency_samples - before.refill_latency_samples,
        4
    );
    assert!(keys
        .iter()
        .all(|key| cache.peek_tier(key) == Some(CacheReadTier::Memory)));
}

// rust-internal: deterministically interleave mutations between probe and refill.
#[test]
fn stale_pmem_probe_does_not_replace_a_new_value() {
    let (_dir, cache) = cache();
    let key = CacheKey::string(0, "key");
    let mut inner = cache.inner.write().unwrap();
    assert!(inner.put_pmem(&key, b"old".to_vec()));
    let old = inner.pmem_value(&key).unwrap().clone();
    assert!(inner.put_pmem(&key, b"new".to_vec()));
    inner.promote_pmem_probe_if_current(&key, &old);
    assert!(inner.resident_value(&key).is_none());
    let current = inner.pmem_value(&key).unwrap().clone();
    inner.promote_pmem_probe_if_current(&key, &current);
    assert!(Arc::ptr_eq(inner.resident_value(&key).unwrap(), &current));
    assert!(Arc::ptr_eq(inner.pmem.get(&key).unwrap(), &current));
    assert!(inner.put_memory_shared(&key, Arc::from(&b"newer"[..])));
    inner.promote_pmem_probe_if_current(&key, &current);
    assert_eq!(inner.resident_value(&key).unwrap().as_ref(), b"newer");
    assert_eq!(inner.stats.refill_failures, 0);
}

// rust-internal: an invalidated probe must not recreate cache state.
#[test]
fn invalidated_pmem_probe_does_not_resurrect_an_entry() {
    let (_dir, cache) = cache();
    let key = CacheKey::string(0, "key");
    let mut inner = cache.inner.write().unwrap();
    assert!(inner.put_pmem(&key, b"old".to_vec()));
    let old = inner.pmem_value(&key).unwrap().clone();
    inner.invalidate_key_locked(&key, true);
    inner.promote_pmem_probe_if_current(&key, &old);
    assert!(inner.resident_value(&key).is_none());
    assert!(!inner.pmem.contains_key(&key));
    assert_eq!(inner.stats.refill_failures, 0);
}

// rust-internal: lifecycle and expiry can change while a captured value is held.
#[test]
fn pmem_probe_promotion_respects_stop_expiry_and_storage_only_mode() {
    let (_dir, cache) = cache();
    let key = CacheKey::string(0, "key");
    let mut inner = cache.inner.write().unwrap();
    assert!(inner.put_pmem(&key, b"value".to_vec()));
    let value = inner.pmem_value(&key).unwrap().clone();
    inner.started = false;
    inner.promote_pmem_probe_if_current(&key, &value);
    assert!(inner.resident_value(&key).is_none());
    inner.started = true;
    inner.ssd_instance_only = true;
    inner.promote_pmem_probe_if_current(&key, &value);
    assert!(inner.resident_value(&key).is_none());
    inner.ssd_instance_only = false;
    inner.ttl_in_use = true;
    inner.set_entry_expiry(&key, 1, 0);
    *inner.expiries.get_mut(&key).unwrap() = CoarseClock::now_millis().saturating_sub(1);
    assert!(inner.entry_expired(&key, CoarseClock::now_millis()));
    inner.promote_pmem_probe_if_current(&key, &value);
    assert!(inner.resident_value(&key).is_none());
}
