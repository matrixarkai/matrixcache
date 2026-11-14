// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! MatrixCache is a Rust-native multi-tier cache library. It manages a hot
//! in-memory (Dram) tier, a persistent-memory-like resident tier, and an Ssd tier
//! (RocksDB by default), with admission control, cross-tier eviction, read-through
//! refill, pinned handles, invalidation, and asynchronous writeback with
//! backpressure accounting.
//!
//! # Example
//!
//! ```no_run
//! use matrixcache::{CacheKey, MultiLayerCache};
//!
//! // 1 MiB in-memory tier; the Ssd tier is persisted under `dir`.
//! let dir = std::env::temp_dir().join("matrixcache-doc");
//! let cache = MultiLayerCache::new(1 << 20, dir);
//!
//! let key = CacheKey::string(0, "greeting");
//! cache.put(key.clone(), b"hello".to_vec())?;
//! assert_eq!(cache.get(&key)?, Some(b"hello".to_vec()));
//! # Ok::<(), matrixcache::CacheError>(())
//! ```
//!
//! The Ssd backend is RocksDB via the default `rocksdb-ssd` feature; build with
//! `--no-default-features` for a lightweight file-backed compatibility store.

use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
#[cfg(not(feature = "rocksdb-ssd"))]
use std::fs::OpenOptions;
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
#[cfg(not(feature = "rocksdb-ssd"))]
use std::io::Write;
#[cfg(not(feature = "rocksdb-ssd"))]
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
#[cfg(feature = "rocksdb-ssd")]
use std::str;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock, Weak};
use std::time::{Duration, Instant};

use ahash::{AHashMap, AHashSet, RandomState};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use thiserror::Error;

include!("core/rdma.rs");
include!("core/concurrency_stats.rs");
include!("runtime/allocators_executors.rs");
include!("core/storage_config.rs");
include!("runtime/replacement.rs");
include!("runtime/storage_engines.rs");
#[path = "runtime/async_storage.rs"]
mod async_storage;
pub use async_storage::{
    AsyncStorageRuntime, StorageDeleteCompletion, StorageWriteCompletion, StorageWriteRejected,
};
include!("runtime/multilayer_cache.rs");
include!("runtime/record_key_index.rs");
include!("runtime/background_recovery.rs");
include!("runtime/cold_tier_migration.rs");
include!("runtime/cache_facades.rs");
include!("runtime/l2_worker.rs");
include!("runtime/builder_and_gc.rs");
include!("runtime/manifest_writer.rs");
include!("runtime/zoned_store.rs");
include!("runtime/zoned_gc.rs");
include!("core/frequency_sketch.rs");
include!("core/metrics.rs");
include!("core/write_budget.rs");
include!("core/health.rs");
include!("core/config_check.rs");
include!("core/cluster.rs");
include!("core/legacy_names.rs");
include!("tests/mod.rs");
#[cfg(test)]
#[path = "tests/l2_worker.rs"]
mod l2_worker_tests;
#[cfg(test)]
#[path = "tests/pmem_promotion.rs"]
mod pmem_promotion_tests;

#[cfg(test)]
#[path = "tests/ssd_promotion.rs"]
mod ssd_promotion_tests;

#[cfg(test)]
#[path = "tests/background_recovery.rs"]
mod background_recovery_tests;

#[cfg(test)]
#[path = "tests/key_snapshots.rs"]
mod key_snapshot_tests;

#[cfg(test)]
#[path = "tests/invalidate_disk_order.rs"]
mod invalidate_disk_order_tests;

#[cfg(test)]
#[path = "tests/put_disk_order.rs"]
mod put_disk_order_tests;

#[cfg(test)]
#[path = "tests/ssd_store_index_invariant.rs"]
mod ssd_store_index_invariant_tests;

#[cfg(test)]
#[path = "tests/disabled_ssd_peeks.rs"]
mod disabled_ssd_peek_tests;

#[cfg(test)]
#[path = "tests/record_key_index.rs"]
mod record_key_index_tests;
