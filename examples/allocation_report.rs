// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! What each hot path allocates, counted rather than argued.
//!
//! This crate forbids `unsafe_code`, so for a long stretch of work on payload
//! copies the guards had to be structural -- a test reading the source, or a
//! compile-level contract that a function takes a borrow. The claims
//! themselves ("this read hands back the stored bytes without copying them")
//! were never measured.
//!
//! They can be. A counting allocator is an `unsafe impl GlobalAlloc`, but it
//! does not have to be **ours**: the `stats_alloc` dev-dependency carries the
//! unsafe, and installing it here is a safe static declaration. So this
//! example measures allocations and bytes per operation, and its self-checks
//! are the first ones in this crate that can fail because a copy came back.
//!
//! ```text
//! cargo run --release --no-default-features --example allocation_report
//! cargo run --release --no-default-features --example allocation_report -- --json-output /tmp/matrixcache-allocations.json --require-passed
//! ```

use std::alloc::System;

use matrixcache::{
    CacheBlockOptions, CacheDataPlacement, CacheKey, CacheOptions, CacheTieringPolicy,
    MultiLayerCache,
};
use stats_alloc::{Region, StatsAlloc, INSTRUMENTED_SYSTEM};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

const VALUE_BYTES: usize = 1024;
const RESIDENT: usize = 4096;
const REPEATS: usize = 200;
const BATCH: usize = 8;
/// Entries resident in the cache that evicts on every write.
const EVICTING_RESIDENT: usize = 256;

fn key(index: usize) -> CacheKey {
    CacheKey::string(0, &format!("allocation-report-{index:08}"))
}

struct Row {
    path: &'static str,
    allocations: f64,
    bytes: f64,
    /// Allocations per distinct key asked for, which is the figure that says
    /// whether a batch path scales in its bookkeeping.
    per_key: f64,
    keys_asked: usize,
}

/// Measures one path, with the closure already built so that building it is
/// not counted.
fn measure<F>(path: &'static str, keys_asked: usize, mut body: F) -> Row
where
    F: FnMut(),
{
    // One pass outside the region, so a first-call allocation that never
    // happens again -- a table growing, a lazy buffer -- is not charged to
    // every call.
    body();
    let region = Region::new(GLOBAL);
    for _ in 0..REPEATS {
        body();
    }
    let change = region.change();
    let allocations = change.allocations as f64 / REPEATS as f64;
    Row {
        path,
        allocations,
        bytes: change.bytes_allocated as f64 / REPEATS as f64,
        per_key: allocations / keys_asked as f64,
        keys_asked,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let json_output = args
        .windows(2)
        .find(|pair| pair[0] == "--json-output")
        .map(|pair| pair[1].clone());
    let require_passed = args.iter().any(|arg| arg == "--require-passed");

    let cache = MultiLayerCache::with_options(CacheOptions::new(64 << 20, 0, 0));
    let value = vec![0u8; VALUE_BYTES];
    for index in 0..RESIDENT {
        cache
            .put(key(index), value.clone())
            .expect("a cache with room must accept a put");
    }

    let probe = key(7);
    let batch: Vec<CacheKey> = (0..BATCH).map(key).collect();
    // Every position a distinct key, so the per-key column below is about the
    // bookkeeping rather than about duplicate coalescing.
    let mut rows = Vec::new();

    rows.push(measure("get, copying", 1, || {
        let _ = cache.get(&probe);
    }));
    rows.push(measure("get_shared", 1, || {
        let _ = cache.get_shared(&probe).expect("get_shared");
    }));
    rows.push(measure("get_memory_shared", 1, || {
        let _ = cache.get_memory_shared(&probe);
    }));
    rows.push(measure("acquire and release", 1, || {
        if let Some(handle) = cache.acquire(&probe).expect("acquire") {
            cache.release(handle);
        }
    }));
    rows.push(measure("get_batch, copying", BATCH, || {
        let _ = cache.get_batch(&batch).expect("get_batch");
    }));
    rows.push(measure("get_shared_batch", BATCH, || {
        let _ = cache.get_shared_batch(&batch).expect("get_shared_batch");
    }));
    rows.push(measure("acquire_batch_no_promotion", BATCH, || {
        let handles = cache
            .acquire_batch_no_promotion(&batch)
            .expect("acquire_batch_no_promotion");
        cache.release_batch(handles.into_iter().flatten().collect());
    }));
    rows.push(measure("acquire_batch", BATCH, || {
        let handles = cache.acquire_batch(&batch).expect("acquire_batch");
        cache.release_batch(handles.into_iter().flatten().collect());
    }));
    // A key asked for twice is what the grouping exists for, and the one case
    // where its position list has to hold more than one position.
    let with_duplicates: Vec<CacheKey> =
        batch.iter().chain(batch.iter().take(2)).cloned().collect();
    rows.push(measure(
        "acquire_batch, 2 duplicated",
        with_duplicates.len(),
        || {
            let handles = cache
                .acquire_batch(&with_duplicates)
                .expect("acquire_batch");
            cache.release_batch(handles.into_iter().flatten().collect());
        },
    ));
    rows.push(measure("put, overwriting", 1, || {
        cache
            .put(probe.clone(), value.clone())
            .expect("put must be accepted");
    }));
    // The zero-copy write, against the copying one above: the caller already
    // holds the bytes behind a handle, so nothing has to be allocated at all.
    let shared: std::sync::Arc<[u8]> = std::sync::Arc::from(value.clone());
    rows.push(measure("put_shared, overwriting", 1, || {
        cache
            .put_shared(probe.clone(), std::sync::Arc::clone(&shared))
            .expect("put must be accepted");
    }));
    // A cache at its capacity, so every write evicts. Selection weighs a
    // window of candidates in a table, which is the largest allocation on any
    // path in this report.
    let tight = MultiLayerCache::with_options(CacheOptions::new(
        EVICTING_RESIDENT * (VALUE_BYTES + 64),
        0,
        0,
    ));
    for index in 0..EVICTING_RESIDENT {
        tight
            .put(key(index), value.clone())
            .expect("a cache with room must accept a put");
    }
    let mut evicting = 1_000_000usize;
    rows.push(measure("put, evicting", 1, || {
        evicting += 1;
        tight
            .put(key(evicting), value.clone())
            .expect("put must be accepted");
    }));

    // A write that reaches the SSD tier, which no row above does. Everything
    // measured so far is the memory tier: this path also compresses, frames a
    // block, builds a manifest line and writes it, so it is the most expensive
    // write the cache has -- and the one where a payload copy is easiest to
    // hide.
    //
    // The two feature arms are NOT comparable here. With `rocksdb-ssd` the
    // store's own allocations happen in a native library the counting
    // allocator cannot see, so this row reports the Rust side only; without it
    // the store is Rust and every byte it keeps is in the figure.
    let ssd_dir = std::env::temp_dir().join("matrixcache-allocation-report-ssd");
    let _ = std::fs::remove_dir_all(&ssd_dir);
    let tiered = MultiLayerCache::with_tiering_policy(
        ssd_dir.clone(),
        CacheTieringPolicy {
            // Room for a handful in memory, so writes go through to the tier
            // rather than piling up in it.
            memory_capacity_bytes: 16 * VALUE_BYTES,
            pmem_capacity_bytes: 1 << 20,
            ssd_capacity_bytes: 1 << 28,
            data_placement: CacheDataPlacement::Tiered,
            data_placement_threshold_bytes: 0,
            memory_hotness_threshold: 0,
            pmem_admit_hotness_threshold: 0,
            ssd_admit_hotness_threshold: 0,
            max_memory_block_bytes: 1 << 20,
            max_pmem_block_bytes: 1 << 20,
            max_ssd_block_bytes: 1 << 20,
            ssd_write_through: true,
        },
        CacheBlockOptions::default(),
    );
    // Incompressible, deliberately, and only for this row. The tier
    // compresses what it writes, so a block of zeros makes every copy of the
    // payload on this path cost almost nothing -- which is exactly what the
    // row is here to measure. Measured on this fixture: removing two payload
    // copies moved 54,274.6 bytes to 54,008.1 with zeros, a change of 266
    // bytes for two copies of a 1,024-byte value, because the blocks being
    // copied had compressed to about fifty bytes each.
    //
    // The rows above keep the zero-filled value, so their figures stay
    // comparable with the ones already published for them.
    let mut fill = 0x2545_f491_4f6c_dd1du64;
    let incompressible: Vec<u8> = (0..VALUE_BYTES)
        .map(|_| {
            fill ^= fill << 13;
            fill ^= fill >> 7;
            fill ^= fill << 17;
            (fill & 0xff) as u8
        })
        .collect();
    // A read that misses memory and finds the entry on the SSD tier.
    //
    // Every read row above this one is a memory hit. A lower-tier hit also
    // promotes the entry back into memory before it returns, which is an
    // admission -- and an admission into a full tier evicts. That work is on
    // the reader, and this row is what it costs.
    let promote_dir = std::env::temp_dir().join("matrixcache-allocation-report-promote");
    let _ = std::fs::remove_dir_all(&promote_dir);
    let promoting = MultiLayerCache::with_tiering_policy(
        promote_dir.clone(),
        CacheTieringPolicy {
            // Room for sixteen in memory, so a read of anything else has to
            // promote into a full tier.
            memory_capacity_bytes: 16 * VALUE_BYTES,
            pmem_capacity_bytes: 0,
            ssd_capacity_bytes: 1 << 28,
            data_placement: CacheDataPlacement::Tiered,
            data_placement_threshold_bytes: 0,
            memory_hotness_threshold: 0,
            pmem_admit_hotness_threshold: 0,
            ssd_admit_hotness_threshold: 0,
            max_memory_block_bytes: 1 << 20,
            max_pmem_block_bytes: 1 << 20,
            max_ssd_block_bytes: 1 << 20,
            ssd_write_through: true,
        },
        CacheBlockOptions::default(),
    );
    const PROMOTE_KEYS: usize = 512;
    for index in 0..PROMOTE_KEYS {
        promoting
            .put(key(index), incompressible.clone())
            .expect("a cache with room must accept a put");
    }
    // Keys drawn from the demoted range, so each read is a lower-tier hit that
    // promotes. Round-robin rather than one key, or the second read is a
    // memory hit.
    let mut promote_cursor = 0usize;
    rows.push(measure("get, promoting from a lower tier", 1, || {
        promote_cursor += 1;
        let found = promoting
            .get(&key(promote_cursor % PROMOTE_KEYS))
            .expect("a read of a key the fixture wrote must succeed");
        assert!(
            found.is_some(),
            "the fixture has to still hold the key for this row to be measuring a promotion"
        );
    }));
    // What the reads above actually did, asked of the cache rather than
    // assumed from the fixture. Checked below.
    let promote_disk_hits = promoting.stats().disk_hits;

    // The same read asked for eight keys at once. The batch path reads the
    // blocks together, decodes each one once, and hands the bytes to every
    // position that asked for that key -- so what it must not do is a copy per
    // position on top of the one each position gets.
    //
    // The groups are built outside the measured closure, as the resident batch
    // row builds its key list outside: a batch row that formats eight keys a
    // call measures `format!`.
    let promote_groups: Vec<Vec<CacheKey>> = (0..PROMOTE_KEYS / BATCH)
        .map(|group| {
            ((group * BATCH)..(group * BATCH + BATCH))
                .map(key)
                .collect()
        })
        .collect();
    let mut promote_group_cursor = 0usize;
    let batch_promotion_hits_before = promoting.stats().disk_hits;
    rows.push(measure(
        "get_batch, promoting from a lower tier",
        BATCH,
        || {
            promote_group_cursor += 1;
            let group = &promote_groups[promote_group_cursor % promote_groups.len()];
            let found = promoting
                .get_batch(group)
                .expect("a batch read of keys the fixture wrote must succeed");
            assert!(
                found.iter().all(|slot| slot.is_some()),
                "the fixture has to still hold every key of the group"
            );
        },
    ));
    let batch_promotion_hits = promoting.stats().disk_hits - batch_promotion_hits_before;

    let mut reaching_ssd = 2_000_000usize;
    rows.push(measure("put, reaching the SSD tier", 1, || {
        reaching_ssd += 1;
        tiered
            .put(key(reaching_ssd), incompressible.clone())
            .expect("put must be accepted");
    }));

    println!("Per operation, memory tier only, {VALUE_BYTES}-byte values, {RESIDENT} resident.");
    println!("One untimed pass first, so a table growing once is not charged to every call.");
    println!();
    println!(
        "  {:>28}  {:>5}  {:>12}  {:>10}  {:>13}",
        "path", "keys", "allocations", "bytes", "alloc per key"
    );
    for row in &rows {
        println!(
            "  {:>28}  {:>5}  {:>12.2}  {:>10.1}  {:>13.2}",
            row.path, row.keys_asked, row.allocations, row.bytes, row.per_key
        );
    }
    println!();

    // ---- Self-checks ------------------------------------------------------
    let mut failures: Vec<String> = Vec::new();
    let find = |path: &str| rows.iter().find(|row| row.path == path);

    // The positive control, first and loudest. If the allocator is not
    // installed every row reads zero, and a zero would then confirm every
    // claim below by measuring nothing at all.
    match find("get, copying") {
        Some(row) if row.allocations >= 1.0 && row.bytes >= VALUE_BYTES as f64 => {}
        Some(row) => failures.push(format!(
            "the copying read reports {:.2} allocations of {:.1} bytes, which is below the one \
             buffer of {VALUE_BYTES} bytes its signature forces -- the counting allocator is \
             probably not installed, and every zero below means nothing",
            row.allocations, row.bytes
        )),
        None => failures.push("the copying read is missing, so there is no control".to_string()),
    }

    // The claim the zero-copy reads exist for. `get_shared` hands back the
    // `Arc` the memory tier holds; a copy here is the bug this crate spent
    // several changes removing.
    for path in ["get_shared", "get_memory_shared"] {
        match find(path) {
            Some(row) if row.allocations == 0.0 && row.bytes == 0.0 => {}
            Some(row) => failures.push(format!(
                "{path} allocates {:.2} times and {:.1} bytes on a memory hit; it is supposed to \
                 hand back the bytes the tier already holds",
                row.allocations, row.bytes
            )),
            None => failures.push(format!("{path} is missing from the table")),
        }
    }

    // A handle is an `Arc` and a key, and the pin bookkeeping is a map that
    // has already grown. Taking one and giving it back should not allocate
    // per call.
    if let Some(row) = find("acquire and release") {
        if row.allocations > 0.5 {
            failures.push(format!(
                "acquire and release allocates {:.2} times a call, where the handle is a shared \
                 pointer and the pin map is already grown",
                row.allocations
            ));
        }
    }

    // And the batch bookkeeping: whatever the batch paths allocate, it has to
    // be bounded per key asked for. This is the figure that moves if a reader
    // goes back to copying its keys.
    for path in [
        "get_batch, copying",
        "get_shared_batch",
        "acquire_batch_no_promotion",
        "acquire_batch",
        "acquire_batch, 2 duplicated",
    ] {
        if let Some(row) = find(path) {
            if row.per_key > 4.0 {
                failures.push(format!(
                    "{path} allocates {:.2} times per key asked for, which is more bookkeeping \
                     than a batch read should need",
                    row.per_key
                ));
            }
        }
    }

    // The ratchet.
    //
    // The checks above and below are relational -- zero where a copy would
    // show, bounded per key asked for -- and a path that went from two
    // allocations to five would pass every one of them. These are the figures
    // as measured, each with a little room, so that giving back what the copy
    // elisions won fails here instead of being noticed much later.
    //
    // Identical across three runs of the same binary and across both feature
    // arms (only the SSD row differs between arms, and it is bounded
    // separately below), so there is nothing to tolerate but a change.
    //
    //   pinned batch of 8        15 -> 7 allocations
    //   a write that evicts      11.48 -> 5.93 allocations, 44,140.8 -> 19,537.3 bytes
    //   a batch of 8 shared      2 allocations for 8 keys
    //   a promoting read         13.00 -> 12.00 allocations, 25,334.0 -> 24,310.0 bytes
    //
    // Raising one is a deliberate act, and the number that replaced it goes
    // in the table beside it.
    for (path, max_allocations, max_bytes) in [
        ("get, copying", 1.5, 1_100.0),
        ("get_batch, copying", 11.5, 9_400.0),
        ("get_shared_batch", 2.5, 500.0),
        ("acquire_batch_no_promotion", 7.5, 3_000.0),
        ("acquire_batch", 7.5, 3_100.0),
        ("acquire_batch, 2 duplicated", 9.5, 4_300.0),
        ("put, overwriting", 2.5, 2_200.0),
        ("put, evicting", 6.5, 20_500.0),
        ("get, promoting from a lower tier", 13.0, 25_500.0),
    ] {
        match find(path) {
            Some(row) if row.allocations <= max_allocations && row.bytes <= max_bytes => {}
            Some(row) => failures.push(format!(
                "{path} allocates {:.2} times and {:.1} bytes, against the {max_allocations:.1} \
                 and {max_bytes:.0} it has been held to -- raise the ceiling on purpose or do \
                 not give the allocations back",
                row.allocations, row.bytes
            )),
            None => failures.push(format!("{path} is missing, so its ceiling checks nothing")),
        }
    }

    // A write that reaches the SSD tier compresses, frames a block, builds a
    // manifest line and writes it, so it is never going to be cheap. What it
    // must not be paying for is copies of the payload that nothing reads. The
    // bounds sit between what the path costs now and what it cost while
    // `StorageEngineApi::put` built a `CacheBuffer` around every value for
    // the tier to throw away, measured on this row's incompressible value:
    //
    //         backend   allocations       bytes
    //      Rust before         33.12    62,580.2
    //       Rust after         28.11    59,288.2
    //   RocksDB before         16.11    58,193.6
    //    RocksDB after         12.11    55,951.6
    //
    // And again when framing a block stopped copying the payload into a buffer
    // of its own first:
    //
    //         backend   allocations       bytes
    //       Rust after         27.11    58,223.8
    //    RocksDB after         11.11    54,914.2
    //
    // Per arm, because the two are not comparable: this allocator sees Rust
    // only, and with RocksDB the store's own bytes are allocated elsewhere.
    // Three runs of the same binary moved the figures by 0.01 allocations and
    // 8 bytes, so the margin here is wide rather than lucky.
    #[cfg(not(feature = "rocksdb-ssd"))]
    const BATCH_PROMOTION_ALLOCATIONS: f64 = 65.0;
    #[cfg(feature = "rocksdb-ssd")]
    const BATCH_PROMOTION_ALLOCATIONS: f64 = 80.0;
    match find("get_batch, promoting from a lower tier") {
        Some(row) if row.allocations <= BATCH_PROMOTION_ALLOCATIONS && row.bytes <= 190_000.0 => {}
        Some(row) => failures.push(format!(
            "a batch of {BATCH} promoting reads allocates {:.2} times and {:.1} bytes, against \
             {BATCH_PROMOTION_ALLOCATIONS:.0} and 190,000",
            row.allocations, row.bytes
        )),
        None => failures.push("the promoting batch read is missing from the table".to_string()),
    }

    #[cfg(not(feature = "rocksdb-ssd"))]
    const SSD_WRITE_ALLOCATIONS: f64 = 28.5;
    #[cfg(not(feature = "rocksdb-ssd"))]
    const SSD_WRITE_BYTES: f64 = 59_000.0;
    #[cfg(feature = "rocksdb-ssd")]
    const SSD_WRITE_ALLOCATIONS: f64 = 12.5;
    #[cfg(feature = "rocksdb-ssd")]
    const SSD_WRITE_BYTES: f64 = 55_700.0;
    match find("put, reaching the SSD tier") {
        Some(row) if row.allocations <= SSD_WRITE_ALLOCATIONS && row.bytes <= SSD_WRITE_BYTES => {}
        Some(row) => failures.push(format!(
            "a write reaching the SSD tier allocates {:.2} times and {:.1} bytes, against \
             {SSD_WRITE_ALLOCATIONS:.0} and {SSD_WRITE_BYTES:.0} -- a copy of the payload has \
             come back to that path",
            row.allocations, row.bytes
        )),
        None => failures
            .push("the write that reaches the SSD tier is missing from the table".to_string()),
    }

    // A batch over distinct keys asks for exactly one position per key, and a
    // list holding one position does not need an allocation of its own. This is
    // the figure that moves if it goes back to a vector a key: measured at one
    // allocation and eight bytes a key, which was over half of what a pinned
    // batch read allocated.
    if let (Some(pinned), Some(single)) = (
        find("acquire_batch_no_promotion"),
        find("acquire and release"),
    ) {
        let bookkeeping = pinned.allocations - single.allocations * pinned.keys_asked as f64;
        if bookkeeping > pinned.keys_asked as f64 {
            failures.push(format!(
                "a pinned batch over {} distinct keys allocates {:.2} times beyond what the \
                 single read does, which is more than one a key -- a position list is being \
                 given its own allocation again",
                pinned.keys_asked, bookkeeping
            ));
        }
    }

    // The promoting row is a row about a lower-tier hit only if the reads went
    // to a lower tier. Memory holds sixteen of the five hundred and twelve
    // keys and the reads walk the whole range, so all but a handful have to
    // come off the SSD tier. Without this the fixture could stop demoting --
    // a larger memory capacity, a placement change -- and the row would go on
    // reporting the cost of a memory hit under the promotion label.
    let promote_reads = REPEATS as u64 + 1;
    if promote_disk_hits * 10 < promote_reads * 9 {
        failures.push(format!(
            "the promoting row made {promote_reads} reads and only {promote_disk_hits} of them reached a lower tier, so it is not measuring a promotion"
        ));
    }

    // The same check for the batch row, over its own reads: eight keys asked
    // for on each of two hundred and one calls, all but a handful of which
    // have to come off the SSD tier.
    let batch_promotion_reads = (REPEATS as u64 + 1) * BATCH as u64;
    if batch_promotion_hits * 10 < batch_promotion_reads * 9 {
        failures.push(format!(
            "the promoting batch row asked for {batch_promotion_reads} keys and only {batch_promotion_hits} came off a lower tier"
        ));
    }

    // The zero-copy write exists so that a caller holding an `Arc` can hand
    // it over without a copy. A single allocation here means the handle is
    // being copied after all.
    match find("put_shared, overwriting") {
        Some(row) if row.allocations == 0.0 && row.bytes == 0.0 => {}
        Some(row) => failures.push(format!(
            "put_shared allocates {:.2} times and {:.1} bytes for a value the caller already \
             holds behind a handle",
            row.allocations, row.bytes
        )),
        None => failures.push("the shared write is missing from the table".to_string()),
    }

    // And a write that evicts: selection weighs a window of candidates in a
    // table, which was built from empty on every eviction -- 11.48
    // allocations and 44,140 bytes for one write before it was sized for its
    // window and its group key narrowed. The bound here is what stops it
    // growing back.
    if let (Some(evicting), Some(plain)) = (find("put, evicting"), find("put, overwriting")) {
        if evicting.allocations > plain.allocations + 6.0 {
            failures.push(format!(
                "a write that evicts allocates {:.2} times against {:.2} for one that does not, \
                 which is more than the eviction needs",
                evicting.allocations, plain.allocations
            ));
        }
        if evicting.bytes > 32_768.0 {
            failures.push(format!(
                "a write that evicts allocates {:.1} bytes, so the candidate table is being \
                 grown from empty again",
                evicting.bytes
            ));
        }
    }

    if failures.is_empty() {
        println!("self-checks: passed");
        println!();
        println!("The two shared reads allocate nothing at all on a memory hit, which is what");
        println!("they exist for and what could only be argued from the source until now.");
    } else {
        println!("self-checks: FAILED");
        for failure in &failures {
            println!("  - {failure}");
        }
    }

    if let Some(path) = json_output {
        let mut json = String::from("{\n");
        json.push_str(&format!("  \"value_bytes\": {VALUE_BYTES},\n"));
        json.push_str(&format!("  \"resident_entries\": {RESIDENT},\n"));
        json.push_str(&format!("  \"repeats\": {REPEATS},\n"));
        json.push_str("  \"rows\": [\n");
        for (index, row) in rows.iter().enumerate() {
            json.push_str(&format!(
                "    {{\"path\": \"{}\", \"keys_asked\": {}, \"allocations\": {:.3}, \
                 \"bytes\": {:.1}, \"allocations_per_key\": {:.3}}}{}\n",
                row.path,
                row.keys_asked,
                row.allocations,
                row.bytes,
                row.per_key,
                if index + 1 == rows.len() { "" } else { "," }
            ));
        }
        json.push_str("  ],\n");
        json.push_str(&format!(
            "  \"passed\": {}\n",
            if failures.is_empty() { "true" } else { "false" }
        ));
        json.push('}');
        std::fs::write(&path, json).expect("json output must be writable");
        println!("wrote {path}");
    }

    if require_passed && !failures.is_empty() {
        std::process::exit(1);
    }
}
