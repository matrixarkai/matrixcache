// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! What does sharding a cache cost it, as the shard count grows?
//!
//! `ShardedMultiLayerCache` spreads keys over independent shards so readers of
//! different shards do not queue behind one another. Every measurement in this
//! repository that uses it fixes the count at sixteen, so the question it exists
//! to answer -- how many -- has never been asked.
//!
//! Two things move in opposite directions, and the answer is where they cross.
//!
//! **Concurrency improves.** One lock serialises readers; sixteen do not. That
//! is the whole point, and the read table shows it.
//!
//! **Capacity fragments.** A sharded cache divides its byte budget by its shard
//! count, and each shard evicts against its own share. A workload whose keys
//! land evenly does not care. A skewed one does: the shard holding the hot keys
//! runs out while the others sit half empty, and the hit rate falls for reasons
//! that have nothing to do with how much memory the cache was given.
//!
//! That second effect is the one a shard count is usually chosen without, so it
//! gets the larger table. The skew is Zipf-like -- a small share of the keys
//! taking most of the reads -- which is what a cache is for.
//!
//! **Read the hit rate as the subject and the throughput as context.** Hit rate
//! here is deterministic: the same keys in the same order against the same
//! capacity, no clock involved. Throughput is a timing on a machine that may be
//! doing other things, and the spread is printed so a reader can see when it has
//! not separated anything.
//!
//! ```text
//! cargo run --release --no-default-features --example shard_count_bench
//! cargo run --release --no-default-features --example shard_count_bench -- 65536
//! ```

use matrixcache::{CacheKey, CacheOptions, ShardedMultiLayerCache};
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

/// Shard counts the tables step through.
const SHARD_STEPS: [usize; 6] = [1, 4, 16, 64, 256, 1_024];
/// Bytes per value.
const VALUE_BYTES: usize = 256;
/// Entries the cache has room for, before it is divided by the shard count.
const RESIDENT: usize = 16_384;
/// Keys the workload draws from.
const KEY_SPACE: usize = 65_536;
/// Reads per measurement.
const READS: usize = 200_000;
/// Passes a timing is the median of.
const PASSES: usize = 3;
/// Threads the concurrent read arm uses.
///
/// Sharding exists so readers of different shards do not queue behind one
/// another, so measuring it at one thread reports the cost and none of the
/// reason. Four, because this machine is shared and sixteen of anything on it
/// has caused harm before.
const THREADS: usize = 4;

/// A key drawn with a skew: most reads land in a small share of the space.
///
/// Deterministic, so the hit rate it produces is a property of the cache and not
/// of a random seed. The shape is the one caches are built for -- a few keys
/// taking most of the traffic -- and it is what makes a shard's own budget
/// matter.
fn skewed_key(step: usize) -> CacheKey {
    // Three quarters of reads into the first eighth of the space.
    let index = if step.is_multiple_of(4) {
        (step / 4) % KEY_SPACE
    } else {
        (step * 7) % (KEY_SPACE / 8)
    };
    CacheKey::string((index % 16) as u64, &format!("key-{index:06}"))
}

/// A key drawn evenly, as a control: whatever the skewed table shows has to be
/// absent here, or it is the sharding and not the skew.
///
/// Drawn evenly from a working set that *fits*, which is what makes it a
/// control: an even workload inside capacity should not care how many shards
/// the capacity was divided into.
///
/// Two earlier versions read 0.00% at every shard count and said nothing. The
/// first strode the whole space with a factor coprime to it -- a permutation, so
/// every read was a first sighting. The second halved the space, which still
/// left a reuse distance of 32,768 against capacity for 16,384: a key's next
/// visit always arrived after it had been evicted. A control has to be able to
/// hit before its flatness means anything, which is why there is now an
/// assertion that it did.
fn even_key(step: usize) -> CacheKey {
    let index = (step * 7) % (RESIDENT / 2);
    CacheKey::string((index % 16) as u64, &format!("key-{index:06}"))
}

fn median(mut values: Vec<f64>) -> (f64, f64, f64) {
    values.sort_by(f64::total_cmp);
    (
        values[values.len() / 2],
        values[0],
        values[values.len() - 1],
    )
}

struct Row {
    shards: usize,
    per_shard_entries: usize,
    skewed_hit_percent: f64,
    even_hit_percent: f64,
    read_ns: f64,
    read_low_ns: f64,
    read_high_ns: f64,
    concurrent_ns: f64,
    stats_us: f64,
}

/// Nanoseconds per read with [`THREADS`] readers going at once.
///
/// Per operation across all threads, so a cache that serialises its readers
/// reports a larger number here than at one thread and a cache that does not
/// reports a smaller one.
fn concurrent_read_ns(cache: &Arc<ShardedMultiLayerCache>, reads: usize) -> f64 {
    let per_thread = reads / THREADS;
    let started = Instant::now();
    let mut handles = Vec::with_capacity(THREADS);
    for thread_index in 0..THREADS {
        let cache = Arc::clone(cache);
        handles.push(thread::spawn(move || {
            let mut found = 0_usize;
            for step in 0..per_thread {
                let key = skewed_key(step * THREADS + thread_index);
                if cache.get(&key).expect("get").is_some() {
                    found += 1;
                }
            }
            found
        }));
    }
    let found: usize = handles
        .into_iter()
        .map(|handle| handle.join().expect("reader thread"))
        .sum();
    let elapsed = started.elapsed();
    assert!(found > 0, "every pass should have found something");
    elapsed.as_secs_f64() * 1e9 / (per_thread * THREADS) as f64
}

/// Hit rate for one shard count under one key pattern.
///
/// Deterministic: the same keys, the same order, the same capacity.
fn hit_percent(shards: usize, key_space_keys: usize, skewed: bool) -> f64 {
    let dir = std::env::temp_dir().join(format!("matrixcache-shardsweep-{shards}-{skewed}"));
    let _ = std::fs::remove_dir_all(&dir);
    let cache = ShardedMultiLayerCache::with_options(
        CacheOptions::new(RESIDENT * VALUE_BYTES, 0, 0),
        shards,
    );
    cache.start().expect("start");
    let value = vec![b'v'; VALUE_BYTES];
    let mut hits = 0_usize;
    let mut reads = 0_usize;
    for step in 0..key_space_keys {
        let key = if skewed {
            skewed_key(step)
        } else {
            even_key(step)
        };
        match cache.get(&key).expect("get") {
            Some(_) => hits += 1,
            None => cache.put(key, value.clone()).expect("put"),
        }
        reads += 1;
    }
    let _ = std::fs::remove_dir_all(&dir);
    hits as f64 * 100.0 / reads as f64
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut reads = READS;
    let mut json_output: Option<PathBuf> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json-output" => {
                index += 1;
                json_output = Some(PathBuf::from(
                    args.get(index).expect("--json-output needs a path"),
                ));
            }
            other => reads = other.parse().expect("the read count takes a number"),
        }
        index += 1;
    }

    println!(
        "{RESIDENT} entries of {VALUE_BYTES} bytes, {KEY_SPACE} keys, {reads} reads, \
median of {PASSES}\n"
    );

    let mut rows = Vec::new();
    for shards in SHARD_STEPS {
        let cache = ShardedMultiLayerCache::with_options(
            CacheOptions::new(RESIDENT * VALUE_BYTES, 0, 0),
            shards,
        );
        cache.start().expect("start");
        let value = vec![b'v'; VALUE_BYTES];
        for step in 0..KEY_SPACE {
            let _ = cache.put(skewed_key(step), value.clone());
        }

        let mut read_samples = Vec::with_capacity(PASSES);
        for _ in 0..PASSES {
            let started = Instant::now();
            let mut found = 0_usize;
            for step in 0..reads {
                if cache.get(&skewed_key(step)).expect("get").is_some() {
                    found += 1;
                }
            }
            let elapsed = started.elapsed();
            assert!(found > 0, "every pass should have found something");
            read_samples.push(elapsed.as_secs_f64() * 1e9 / reads as f64);
        }
        let (read_ns, read_low, read_high) = median(read_samples);

        // What a metrics scrape costs: the fold walks every shard and adds up
        // every counter.
        let mut stats_samples = Vec::with_capacity(PASSES);
        for _ in 0..PASSES {
            let started = Instant::now();
            for _ in 0..50 {
                let stats = cache.stats();
                assert!(stats.puts > 0);
            }
            stats_samples.push(started.elapsed().as_secs_f64() * 1e6 / 50.0);
        }
        let (stats_us, _, _) = median(stats_samples);

        let shared = Arc::new(cache);
        let mut concurrent_samples = Vec::with_capacity(PASSES);
        for _ in 0..PASSES {
            concurrent_samples.push(concurrent_read_ns(&shared, reads));
        }
        let (concurrent_ns, _, _) = median(concurrent_samples);

        rows.push(Row {
            shards,
            per_shard_entries: RESIDENT / shards.max(1),
            skewed_hit_percent: hit_percent(shards, KEY_SPACE, true),
            even_hit_percent: hit_percent(shards, KEY_SPACE, false),
            read_ns,
            read_low_ns: read_low,
            read_high_ns: read_high,
            concurrent_ns,
            stats_us,
        });
    }

    println!(
        "{:>7}  {:>10}  {:>11}  {:>10}  {:>9}  {:>14}  {:>11}  {:>9}",
        "shards",
        "entries ea",
        "hit% skew",
        "hit% even",
        "1-thread",
        "spread",
        "4-thread",
        "stats us"
    );
    for row in &rows {
        println!(
            "{:>7}  {:>10}  {:>10.2}%  {:>9.2}%  {:>9.1}  {:>6.0}..{:<6.0}  {:>11.1}  {:>9.1}",
            row.shards,
            row.per_shard_entries,
            row.skewed_hit_percent,
            row.even_hit_percent,
            row.read_ns,
            row.read_low_ns,
            row.read_high_ns,
            row.concurrent_ns,
            row.stats_us
        );
    }

    let first = rows.first().expect("a row");
    let last = rows.last().expect("a row");
    println!();
    println!(
        "skewed hit rate {:.2}% at {} shard to {:.2}% at {} -- {:+.2} points",
        first.skewed_hit_percent,
        first.shards,
        last.skewed_hit_percent,
        last.shards,
        last.skewed_hit_percent - first.skewed_hit_percent
    );
    println!(
        "even   hit rate {:.2}% to {:.2}% -- {:+.2} points, which is the control: \
whatever sharding costs a skewed workload should be absent here",
        first.even_hit_percent,
        last.even_hit_percent,
        last.even_hit_percent - first.even_hit_percent
    );
    assert!(
        first.even_hit_percent > 1.0,
        "the control never hit, so it is not a control"
    );
    println!(
        "{THREADS} readers: {:.0}ns per read at {} shard, {:.0}ns at {} -- {:.2}x",
        first.concurrent_ns,
        first.shards,
        last.concurrent_ns,
        last.shards,
        first.concurrent_ns / last.concurrent_ns
    );
    let read_separated =
        last.read_low_ns > first.read_high_ns || first.read_low_ns > last.read_high_ns;
    println!(
        "read cost {}: {:.0}..{:.0}ns at {} shard against {:.0}..{:.0}ns at {}",
        if read_separated {
            "separated"
        } else {
            "did NOT separate, so it has measured the machine"
        },
        first.read_low_ns,
        first.read_high_ns,
        first.shards,
        last.read_low_ns,
        last.read_high_ns,
        last.shards
    );

    if let Some(path) = json_output {
        let mut report = String::new();
        let _ = writeln!(report, "{{");
        let _ = writeln!(report, "  \"resident_entries\": {RESIDENT},");
        let _ = writeln!(report, "  \"key_space\": {KEY_SPACE},");
        let _ = writeln!(report, "  \"reads\": {reads},");
        let _ = writeln!(report, "  \"read_cost_separated\": {read_separated},");
        let _ = writeln!(report, "  \"shards\": [");
        for (position, row) in rows.iter().enumerate() {
            let comma = if position + 1 == rows.len() { "" } else { "," };
            let _ = writeln!(
                report,
                "    {{\"shards\": {}, \"entries_each\": {}, \
                 \"hit_percent_skewed\": {:.2}, \"hit_percent_even\": {:.2}, \
                 \"read_ns\": {:.1}, \"concurrent_ns\": {:.1}, \
                 \"stats_us\": {:.1}}}{comma}",
                row.shards,
                row.per_shard_entries,
                row.skewed_hit_percent,
                row.even_hit_percent,
                row.read_ns,
                row.concurrent_ns,
                row.stats_us
            );
        }
        let _ = writeln!(report, "  ]");
        let _ = writeln!(report, "}}");
        std::fs::write(&path, report).expect("write the report");
        println!("\nwrote {}", path.display());
    }
}
