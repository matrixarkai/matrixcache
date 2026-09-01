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

use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
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
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use thiserror::Error;

include!("core/rdma.rs");
include!("core/concurrency_stats.rs");
include!("runtime/allocators_executors.rs");
include!("core/storage_config.rs");
include!("runtime/replacement.rs");
include!("runtime/storage_engines.rs");
include!("runtime/multilayer_cache.rs");
include!("runtime/cache_facades.rs");
include!("runtime/builder_and_gc.rs");
include!("runtime/zoned_store.rs");
include!("runtime/zoned_gc.rs");
include!("core/frequency_sketch.rs");
include!("core/metrics.rs");
include!("core/write_budget.rs");
include!("core/health.rs");
include!("core/config_check.rs");
include!("core/legacy_names.rs");
include!("tests/mod.rs");
