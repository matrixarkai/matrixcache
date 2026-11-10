// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

use matrixcache::{
    CacheBuffer, CacheError, CacheInstance, ReplacementPolicyKind, StorageEngineKind,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

#[test]
fn write_results_survive_reentrant_eviction_rewrites() {
    for callback_write in [false, true] {
        let instance = CacheInstance::new(
            8,
            ReplacementPolicyKind::Fifo,
            StorageEngineKind::Dram,
            Vec::new(),
        );
        instance.start().unwrap();
        instance.put("seed", b"12345678".to_vec()).unwrap();
        let rewrite = instance.clone();
        let rewritten = Arc::new(AtomicBool::new(false));
        let did_rewrite = Arc::clone(&rewritten);
        instance.register_eviction_handler(move |_| {
            if !did_rewrite.swap(true, Ordering::AcqRel) {
                rewrite.put("target", b"later".to_vec()).unwrap();
            }
        });
        let result = if callback_write {
            let mut input = CacheBuffer::new(b"new".to_vec());
            input.set_key("target");
            let observed = Arc::new(Mutex::new(Vec::new()));
            let callback_observed = Arc::clone(&observed);
            let result = instance
                .async_put_buffer(input, "caller", move |result| {
                    *callback_observed.lock().unwrap() = result.unwrap().value().to_vec();
                })
                .unwrap();
            assert_eq!(*observed.lock().unwrap(), b"new");
            result
        } else {
            instance
                .put_returning_buffer("target", b"new".to_vec())
                .unwrap()
        };
        assert!(rewritten.load(Ordering::Acquire));
        assert_eq!(result.value(), b"new");
        assert_eq!(instance.get("target").unwrap().unwrap(), b"later");
        drop(result);
        instance.register_eviction_handler(|_| {});
        instance.stop().unwrap();
    }
}

#[test]
fn rejected_buffer_write_does_not_return_previous_value() {
    let instance = CacheInstance::new(
        4,
        ReplacementPolicyKind::Fifo,
        StorageEngineKind::Dram,
        Vec::new(),
    );
    instance.start().unwrap();
    instance.put("key", b"old".to_vec()).unwrap();
    assert!(matches!(
        instance.put_returning_buffer("key", b"oversized".to_vec()),
        Err(CacheError::CapacityExceeded)
    ));
    instance.stop().unwrap();
}
