// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

use matrixcache::{
    AsyncStorageRuntime, CacheBuffer, CacheError, StorageEngineApi, StorageEngineSimple,
};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

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
        let completion = runtime
            .try_put(buffer("record", b"original"), move |result| {
                sender.send(result.unwrap()).unwrap();
            })
            .unwrap();
        let result = completion.wait().unwrap();
        let callback_buffer = receiver.recv().unwrap();
        assert_eq!(result.value(), b"original");
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
