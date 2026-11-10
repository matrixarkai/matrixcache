// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

use super::*;

fn policy_with_value(dir: &Path) -> L2CachePolicy {
    let dram = CacheInstance::new(
        128,
        ReplacementPolicyKind::Fifo,
        StorageEngineKind::Dram,
        vec![],
    );
    dram.Put("message", b"payload".to_vec()).unwrap();
    let ssd = CacheInstance::new(
        256,
        ReplacementPolicyKind::Fifo,
        StorageEngineKind::Ssd,
        vec![dir.to_path_buf()],
    );
    let mut arc = ReplacementArc::new(8);
    arc.init().unwrap();
    let mut policy = L2CachePolicy::new(DramPmemL1Cache::new(dram, None), ssd, arc, 8, 8, 8);
    policy.set_access_interval_ms(2);
    policy.set_tail_interval_ms(2);
    policy.set_write_interval_ms(2);
    policy
}

#[test]
fn l2_worker_migrates_without_explicit_poll() {
    let dir = tempfile::tempdir().unwrap();
    let mut runtime = L2CachePolicyRuntime::start(policy_with_value(dir.path())).unwrap();
    assert!(runtime.on_access(AccessRecordKind::Put, "message"));
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if runtime.inspect(|policy| policy.l2_cache().Get("message").unwrap())
            == Some(b"payload".to_vec())
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "worker did not migrate the value"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    runtime.stop().unwrap();
    assert!(runtime.inspect(L2CachePolicy::stopped));
    assert!(!runtime.on_access(AccessRecordKind::Get, "message"));
}

#[test]
fn l2_worker_stop_drains_paused_evictions() {
    let dir = tempfile::tempdir().unwrap();
    let mut policy = policy_with_value(dir.path());
    policy.set_use_eviction_handler(true);
    let mut runtime = L2CachePolicyRuntime::start(policy).unwrap();
    runtime.pause();
    let mut buffer = CacheBuffer::new(b"migrated".to_vec());
    buffer.SetKey("evicted");
    runtime.on_evict(buffer).unwrap();
    runtime.stop().unwrap();
    assert_eq!(
        runtime.inspect(|policy| policy.l2_cache().Get("evicted").unwrap()),
        Some(b"migrated".to_vec())
    );
}

#[test]
fn l2_worker_stop_drains_every_accepted_access_batch() {
    let dir = tempfile::tempdir().unwrap();
    let mut policy = policy_with_value(dir.path());
    policy.access_buffer_capacity = 512;
    let mut runtime = L2CachePolicyRuntime::start(policy).unwrap();
    runtime.pause();
    for index in 0..300 {
        assert!(runtime.on_access(AccessRecordKind::Put, &format!("key-{index}")));
    }
    runtime.stop().unwrap();
    assert_eq!(runtime.inspect(L2CachePolicy::access_callback_count), 300);
}

#[test]
fn l2_worker_rejects_full_inbox_without_losing_evicted_buffer() {
    let dir = tempfile::tempdir().unwrap();
    let mut policy = policy_with_value(dir.path());
    policy.access_buffer_capacity = 2;
    policy.write_buffer_capacity = 1;
    policy.set_use_eviction_handler(true);
    let mut runtime = L2CachePolicyRuntime::start(policy).unwrap();
    runtime.pause();
    assert!(runtime.on_access(AccessRecordKind::Put, "a"));
    assert!(runtime.on_access(AccessRecordKind::Put, "b"));
    assert!(!runtime.on_access(AccessRecordKind::Put, "c"));
    let mut first = CacheBuffer::new(b"first".to_vec());
    first.SetKey("one");
    runtime.on_evict(first).unwrap();
    let mut second = CacheBuffer::new(b"second".to_vec());
    second.SetKey("two");
    let rejected = runtime.on_evict(second).unwrap_err();
    assert_eq!(rejected.Value(), b"second");
    runtime.stop().unwrap();
    assert_eq!(runtime.inspect(L2CachePolicy::access_callback_count), 2);
    assert_eq!(
        runtime.inspect(|policy| policy.l2_cache().Get("one").unwrap()),
        Some(b"first".to_vec())
    );
}

#[test]
fn l2_worker_accepts_concurrent_producers() {
    let dir = tempfile::tempdir().unwrap();
    let mut policy = policy_with_value(dir.path());
    policy.access_buffer_capacity = 4096;
    let mut runtime = L2CachePolicyRuntime::start(policy).unwrap();
    std::thread::scope(|scope| {
        for producer in 0..8 {
            let runtime = &runtime;
            scope.spawn(move || {
                for index in 0..256 {
                    assert!(runtime.on_access(
                        AccessRecordKind::Put,
                        &format!("producer-{producer}-key-{index}")
                    ));
                }
            });
        }
    });
    runtime.stop().unwrap();
    assert_eq!(runtime.inspect(L2CachePolicy::access_callback_count), 2048);
}
