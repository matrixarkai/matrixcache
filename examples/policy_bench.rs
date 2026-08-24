// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Throughput probe for the cache replacement policies.
//!
//! Every policy keeps its keys in intrusive doubly-linked lists over a shared
//! node arena, so a lookup or a delete unlinks an entry in constant time rather
//! than rescanning a list of keys. The segmented LRU additionally partitions
//! its index and lists into segments, each with its own byte budget, so
//! eviction only ever walks a segment-local list.
//!
//! This example measures both properties. It sweeps the working-set size at a
//! fixed shape to show that per-operation cost does not grow with the number of
//! cached entries, and it sweeps the segment count under eviction pressure.
//!
//! Keys and access orders are generated before the timed regions so the
//! measurement reflects the policy rather than key formatting, and each case is
//! repeated with the best run reported, which suppresses scheduler noise. Costs
//! are only comparable between runs of the same policy: the byte-budgeted
//! policies return a cloned buffer from `get`, while the item-budgeted one
//! returns a bool.
//!
//! ```text
//! cargo run --release --example policy_bench
//! cargo run --release --example policy_bench -- 65536
//! cargo run --release --example policy_bench -- 65536 scaling
//! ```
//!
//! The first argument caps the working-set sweep; passing `scaling` as the
//! second argument skips the eviction-pressure sweep.

use matrixcache::{
    CacheBuffer, ConcurrentReplacementSLRU, ReplacementArc, ReplacementFIFO, ReplacementSLRU,
};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const VALUE_BYTES: usize = 64;
const KEY_PREFIX: &str = "policy-bench-key-";
const KEY_DIGITS: usize = 12;
const REPEATS: usize = 3;

/// Bytes one entry charges against a byte budget: key plus value.
fn entry_space() -> usize {
    KEY_PREFIX.len() + KEY_DIGITS + VALUE_BYTES
}

/// Pre-generated keys plus a visit order that touches every key exactly once
/// but not in insertion order, so lookups cannot ride on list locality.
struct Workload {
    keys: Vec<String>,
    order: Vec<usize>,
}

impl Workload {
    fn new(entries: usize) -> Self {
        let keys = (0..entries)
            .map(|index| format!("{KEY_PREFIX}{index:012}"))
            .collect();
        // Multiplying by an odd constant modulo a power of two is a bijection,
        // so every index is visited exactly once when `entries` is a power of
        // two, and the order is still well spread otherwise.
        let order = (0..entries)
            .map(|index| index.wrapping_mul(2_654_435_761) % entries.max(1))
            .collect();
        Self { keys, order }
    }
}

fn buffer_for(key: &str) -> CacheBuffer {
    let mut buffer = CacheBuffer::new(vec![b'v'; VALUE_BYTES]);
    buffer.SetKey(key);
    buffer
}

fn ns_per_op(elapsed: Duration, ops: usize) -> f64 {
    if ops == 0 {
        return 0.0;
    }
    elapsed.as_nanos() as f64 / ops as f64
}

#[derive(Debug, Clone, Copy)]
struct CaseResult {
    put_ns: f64,
    get_ns: f64,
    churn_ns: f64,
    evicted: usize,
    resident: usize,
}

impl CaseResult {
    fn best(self, other: CaseResult) -> CaseResult {
        CaseResult {
            put_ns: self.put_ns.min(other.put_ns),
            get_ns: self.get_ns.min(other.get_ns),
            churn_ns: self.churn_ns.min(other.churn_ns),
            evicted: other.evicted,
            resident: other.resident,
        }
    }
}

/// Byte capacity for `entries` items, divided by `pressure`: a value above four
/// forces eviction on nearly every insert.
fn byte_capacity(entries: usize, pressure: usize) -> usize {
    (entries * entry_space() * 4 / pressure.max(1)).max(entry_space() * 4)
}

fn run_slru(workload: &Workload, segments: usize, pressure: usize) -> CaseResult {
    let entries = workload.keys.len();
    let mut policy = ReplacementSLRU::with_num_segments(byte_capacity(entries, pressure), segments);
    policy.init().expect("init segmented lru");

    let mut evicted = 0usize;
    let started = Instant::now();
    for key in &workload.keys {
        evicted += policy.put(buffer_for(key)).len();
    }
    let put_elapsed = started.elapsed();

    let started = Instant::now();
    for &index in &workload.order {
        let _ = policy.get(&workload.keys[index]);
    }
    let get_elapsed = started.elapsed();

    let churn = entries / 2;
    let started = Instant::now();
    for &index in workload.order.iter().take(churn) {
        let key = &workload.keys[index];
        let _ = policy.delete(key);
        evicted += policy.put(buffer_for(key)).len();
    }
    let churn_elapsed = started.elapsed();

    CaseResult {
        put_ns: ns_per_op(put_elapsed, entries),
        get_ns: ns_per_op(get_elapsed, entries),
        churn_ns: ns_per_op(churn_elapsed, churn * 2),
        evicted,
        resident: policy.get_item_num(),
    }
}

fn run_fifo(workload: &Workload, pressure: usize) -> CaseResult {
    let entries = workload.keys.len();
    let mut policy = ReplacementFIFO::new(byte_capacity(entries, pressure));
    policy.init().expect("init fifo");

    let mut evicted = 0usize;
    let started = Instant::now();
    for key in &workload.keys {
        evicted += policy.put(buffer_for(key)).len();
    }
    let put_elapsed = started.elapsed();

    let started = Instant::now();
    for &index in &workload.order {
        let _ = policy.get(&workload.keys[index]);
    }
    let get_elapsed = started.elapsed();

    // Overwriting in place is the interesting churn for this policy: it must
    // keep the queue position while replacing the payload.
    let churn = entries / 2;
    let started = Instant::now();
    for &index in workload.order.iter().take(churn) {
        let key = &workload.keys[index];
        let _ = policy.delete(key);
        evicted += policy.put(buffer_for(key)).len();
    }
    let churn_elapsed = started.elapsed();

    CaseResult {
        put_ns: ns_per_op(put_elapsed, entries),
        get_ns: ns_per_op(get_elapsed, entries),
        churn_ns: ns_per_op(churn_elapsed, churn * 2),
        evicted,
        resident: policy.get_item_num(),
    }
}

fn run_arc(workload: &Workload, pressure: usize) -> CaseResult {
    let entries = workload.keys.len();
    // This policy budgets by item count, not bytes.
    let mut policy = ReplacementArc::new((entries * 4 / pressure.max(1)).max(4));
    policy.init().expect("init arc");

    let started = Instant::now();
    for key in &workload.keys {
        policy.put(key.clone());
    }
    let put_elapsed = started.elapsed();

    let started = Instant::now();
    for &index in &workload.order {
        let _ = policy.get(&workload.keys[index]);
    }
    let get_elapsed = started.elapsed();

    let churn = entries / 2;
    let started = Instant::now();
    for &index in workload.order.iter().take(churn) {
        let key = &workload.keys[index];
        let _ = policy.delete(key);
        policy.put(key.clone());
    }
    let churn_elapsed = started.elapsed();

    CaseResult {
        put_ns: ns_per_op(put_elapsed, entries),
        get_ns: ns_per_op(get_elapsed, entries),
        churn_ns: ns_per_op(churn_elapsed, churn * 2),
        evicted: 0,
        resident: policy.get_active_tail(entries).len(),
    }
}

/// Operations each thread performs in the contention sweep.
const CONTENTION_OPS_PER_THREAD: usize = 8_192;

/// Drive the segmented policy from `threads` threads behind one lock over the
/// whole policy. This is what a caller has to do to share the `&mut self` form,
/// and it serialises every operation no matter how the keys are partitioned.
fn run_contention_global(keys: &[String], threads: usize) -> f64 {
    let capacity = keys.len() * entry_space() * 4;
    let policy = Mutex::new({
        let mut inner = ReplacementSLRU::with_num_segments(capacity, 256);
        inner.init().expect("init segmented lru");
        inner
    });
    let per_thread = keys.len() / threads;

    let started = Instant::now();
    std::thread::scope(|scope| {
        for thread in 0..threads {
            let policy = &policy;
            let slice = &keys[thread * per_thread..(thread + 1) * per_thread];
            scope.spawn(move || {
                for key in slice {
                    policy.lock().expect("policy lock").put(buffer_for(key));
                    let _ = policy.lock().expect("policy lock").get(key);
                }
            });
        }
    });
    ns_per_op(started.elapsed(), per_thread * threads * 2)
}

/// The same workload against per-segment locks. Keys that hash to different
/// segments proceed in parallel.
fn run_contention_sharded(keys: &[String], threads: usize, segments: usize) -> f64 {
    let capacity = keys.len() * entry_space() * 4;
    let policy = ConcurrentReplacementSLRU::with_num_segments(capacity, segments);
    policy.init().expect("init segmented lru");
    let per_thread = keys.len() / threads;

    let started = Instant::now();
    std::thread::scope(|scope| {
        for thread in 0..threads {
            let policy = &policy;
            let slice = &keys[thread * per_thread..(thread + 1) * per_thread];
            scope.spawn(move || {
                for key in slice {
                    policy.put(buffer_for(key));
                    let _ = policy.get(key);
                }
            });
        }
    });
    ns_per_op(started.elapsed(), per_thread * threads * 2)
}

fn best_ns<F: Fn() -> f64>(run: F) -> f64 {
    let mut best = run();
    for _ in 1..REPEATS {
        best = best.min(run());
    }
    best
}

fn best_of<F: Fn() -> CaseResult>(run: F) -> CaseResult {
    let mut best = run();
    for _ in 1..REPEATS {
        best = best.best(run());
    }
    best
}

fn print_row(label: &str, entries: usize, shape: &str, result: CaseResult) {
    println!(
        "{label:<10} {entries:>8} {shape:>9} {:>12.1} {:>12.1} {:>13.1} {:>10} {:>10}",
        result.put_ns, result.get_ns, result.churn_ns, result.evicted, result.resident,
    );
}

fn json_case(label: &str, entries: usize, shape: &str, result: CaseResult) -> String {
    format!(
        "{{\"case\":\"{label}\",\"entries\":{entries},\"shape\":\"{shape}\",\
\"put_ns_per_op\":{:.1},\"get_ns_per_op\":{:.1},\"churn_ns_per_op\":{:.1},\
\"evicted\":{},\"resident\":{}}}",
        result.put_ns, result.get_ns, result.churn_ns, result.evicted, result.resident,
    )
}

fn main() {
    let max_entries: usize = std::env::args()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(65_536);
    let scaling_only = std::env::args()
        .nth(2)
        .is_some_and(|mode| mode == "scaling");

    let mut sizes = Vec::new();
    let mut size = 1_024usize;
    while size <= max_entries {
        sizes.push(size);
        size *= 4;
    }
    if sizes.is_empty() {
        sizes.push(max_entries.max(1));
    }

    // Warm the allocator and the CPU before the first measured case.
    let _ = run_slru(&Workload::new(1_024), 256, 1);

    println!(
        "{:<10} {:>8} {:>9} {:>12} {:>12} {:>13} {:>10} {:>10}",
        "policy",
        "entries",
        "shape",
        "put ns/op",
        "get ns/op",
        "churn ns/op",
        "evicted",
        "resident"
    );

    let mut cases = Vec::new();

    // Scaling sweep: per-operation cost against working-set size, with enough
    // headroom that inserts do not evict. Intrusive links keep every phase
    // flat; rescanning a key list would make get and churn grow with the count.
    for &entries in &sizes {
        let workload = Workload::new(entries);

        let result = best_of(|| run_slru(&workload, 256, 1));
        print_row("slru", entries, "256 seg", result);
        cases.push(json_case("slru", entries, "256 seg", result));

        let result = best_of(|| run_fifo(&workload, 1));
        print_row("fifo", entries, "-", result);
        cases.push(json_case("fifo", entries, "-", result));

        let result = best_of(|| run_arc(&workload, 1));
        print_row("arc", entries, "-", result);
        cases.push(json_case("arc", entries, "-", result));
    }

    // Segment sweep at a fixed working set with the capacity tightened so that
    // inserts evict: more segments means a smaller list per eviction.
    if !scaling_only {
        let pressure_entries = sizes.last().copied().unwrap_or(1_024);
        let workload = Workload::new(pressure_entries);
        for &segments in &[1usize, 8, 64, 256, 1_024] {
            let result = best_of(|| run_slru(&workload, segments, 8));
            let shape = format!("{segments} seg");
            print_row("slru/press", pressure_entries, &shape, result);
            cases.push(json_case("slru-pressure", pressure_entries, &shape, result));
        }
    }

    // Contention sweep: the same shared workload behind one lock over the whole
    // policy versus one lock per segment. Sharding only pays off once the locks
    // are per segment, so this is what the partitioning is actually for.
    if !scaling_only {
        println!();
        println!(
            "{:<10} {:>8} {:>9} {:>12} {:>12} {:>13}",
            "contention", "threads", "shape", "global ns/op", "shard ns/op", "speedup"
        );
        for &threads in &[1usize, 2, 4, 8] {
            let keys = Workload::new(threads * CONTENTION_OPS_PER_THREAD).keys;
            let global = best_ns(|| run_contention_global(&keys, threads));
            let sharded = best_ns(|| run_contention_sharded(&keys, threads, 256));
            let speedup = if sharded > 0.0 { global / sharded } else { 0.0 };
            println!(
                "{:<10} {threads:>8} {:>9} {global:>12.1} {sharded:>12.1} {speedup:>12.2}x",
                "lock", "256 seg"
            );
            cases.push(format!(
                "{{\"case\":\"contention\",\"threads\":{threads},\"num_segments\":256,\
\"global_lock_ns_per_op\":{global:.1},\"per_segment_lock_ns_per_op\":{sharded:.1},\
\"speedup\":{speedup:.2}}}"
            ));
        }
    }

    println!();
    println!("{{\"benchmark\":\"policy_bench\",\"value_bytes\":{VALUE_BYTES},\"cases\":[");
    for (index, case) in cases.iter().enumerate() {
        let comma = if index + 1 == cases.len() { "" } else { "," };
        println!("  {case}{comma}");
    }
    println!("]}}");
}
