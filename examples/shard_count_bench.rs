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
/// Writes per measurement, per arm.
const WRITES: usize = 50_000;
/// Distinct keys each writer owns.
///
/// Four writers times this is 8,192 keys, half the resident set, so the cache is
/// not evicting while the write arms are timed -- what they measure is the lock
/// and not the replacement policy. It still fits at every shard count in
/// [`SHARD_STEPS`]: at 1,024 shards the cache holds sixteen entries a shard and
/// this puts eight there.
const WRITE_KEYS_PER_THREAD: usize = 2_048;
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

/// A key for the write arms: no two writers ever name the same one.
///
/// `step * THREADS + thread_index` is a bijection, so the arm measures four
/// writers contending for a lock rather than four writers contending for an
/// entry. [`skewed_key`] would not do: it pushes three quarters of its traffic
/// into an eighth of the space, so the writers would collide on the hot keys and
/// land on one shard however many shards there were -- which would answer "do
/// shards help writes" with "no" for a reason that is about the skew.
fn write_key(thread_index: usize, step: usize) -> CacheKey {
    let index = (step % WRITE_KEYS_PER_THREAD) * THREADS + thread_index;
    CacheKey::string((index % 16) as u64, &format!("w-{index:08}"))
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
    single_write_ns: f64,
    single_write_low_ns: f64,
    single_write_high_ns: f64,
    concurrent_write_ns: f64,
    concurrent_write_low_ns: f64,
    concurrent_write_high_ns: f64,
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

/// Put every key the write arms use, so none of them is a first admission.
fn warm_the_write_keys(cache: &Arc<ShardedMultiLayerCache>) {
    let value = vec![b'w'; VALUE_BYTES];
    for thread_index in 0..THREADS {
        for step in 0..WRITE_KEYS_PER_THREAD {
            let _ = cache.put(write_key(thread_index, step), value.clone());
        }
    }
}

/// Nanoseconds per write with one writer, as the control for the arm below.
///
/// Without it a slow four-writer number cannot be read: writes may simply cost
/// more at a thousand shards the way reads do, and only the ratio of the two
/// arms separates "sharding is expensive" from "writers are queueing".
fn single_write_ns(cache: &Arc<ShardedMultiLayerCache>, writes: usize) -> f64 {
    let per_thread = writes / THREADS;
    let value = vec![b'w'; VALUE_BYTES];
    let started = Instant::now();
    let mut stored = 0_usize;
    // Every key the four-writer arm covers, in one thread. The first version of
    // this control wrote only writer 0's keys -- a quarter of the distinct keys
    // for the same number of writes -- so the two arms differed in how much the
    // cache had to evict as well as in how many threads were running, and a
    // ratio between them was not about threads. It read 38,834ns against
    // 2,566ns at neighbouring shard counts, which is what sent me looking.
    for thread_index in 0..THREADS {
        for step in 0..per_thread {
            if cache
                .put(write_key(thread_index, step), value.clone())
                .is_ok()
            {
                stored += 1;
            }
        }
    }
    let elapsed = started.elapsed();
    assert!(stored > 0, "nothing was stored, so this timed nothing");
    elapsed.as_secs_f64() * 1e9 / (per_thread * THREADS) as f64
}

/// Nanoseconds per write with [`THREADS`] writers going at once.
///
/// The read arm found that readers do not need shards: a memory hit is served
/// under a shared lock, so four readers already go at once at one shard. A write
/// takes the exclusive lock, so this is the arm where sharding has something to
/// do -- and the pair is what says whether "concurrency does not need shards" is
/// a fact about concurrency or a fact about reading.
fn concurrent_write_ns(cache: &Arc<ShardedMultiLayerCache>, writes: usize) -> f64 {
    let per_thread = writes / THREADS;
    let started = Instant::now();
    let mut handles = Vec::with_capacity(THREADS);
    for thread_index in 0..THREADS {
        let cache = Arc::clone(cache);
        handles.push(thread::spawn(move || {
            let value = vec![b'w'; VALUE_BYTES];
            let mut stored = 0_usize;
            for step in 0..per_thread {
                if cache
                    .put(write_key(thread_index, step), value.clone())
                    .is_ok()
                {
                    stored += 1;
                }
            }
            stored
        }));
    }
    let stored: usize = handles
        .into_iter()
        .map(|handle| handle.join().expect("writer thread"))
        .sum();
    let elapsed = started.elapsed();
    assert!(stored > 0, "nothing was stored, so this timed nothing");
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

        // Last, because these arms put new keys in the cache: run them earlier
        // and the read arm above would be reading a different cache.
        // Admit the write set once, untimed. Without this the first timed pass is
        // the one that puts all 8,192 keys in for the first time and the other
        // two overwrite, which showed up as a one-writer spread of
        // 1,926..9,879ns at sixteen shards -- one cold pass and two warm ones,
        // and a median that reported neither.
        warm_the_write_keys(&shared);

        let mut single_write_samples = Vec::with_capacity(PASSES);
        let mut concurrent_write_samples = Vec::with_capacity(PASSES);
        for _ in 0..PASSES {
            single_write_samples.push(single_write_ns(&shared, WRITES));
            concurrent_write_samples.push(concurrent_write_ns(&shared, WRITES));
        }
        let (single_write, single_write_low, single_write_high) = median(single_write_samples);
        let (concurrent_write, concurrent_write_low, concurrent_write_high) =
            median(concurrent_write_samples);

        rows.push(Row {
            shards,
            per_shard_entries: RESIDENT / shards.max(1),
            skewed_hit_percent: hit_percent(shards, KEY_SPACE, true),
            even_hit_percent: hit_percent(shards, KEY_SPACE, false),
            read_ns,
            read_low_ns: read_low,
            read_high_ns: read_high,
            concurrent_ns,
            single_write_ns: single_write,
            single_write_low_ns: single_write_low,
            single_write_high_ns: single_write_high,
            concurrent_write_ns: concurrent_write,
            concurrent_write_low_ns: concurrent_write_low,
            concurrent_write_high_ns: concurrent_write_high,
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

    // A table of its own rather than two more columns, because the question it
    // answers is its own question: a write takes the exclusive lock, so this is
    // where dividing the cache into shards has something to do.
    println!();
    println!(
        "{:>7}  {:>10}  {:>16}  {:>11}  {:>16}  {:>7}",
        "shards", "1 writer", "spread", "4 writers", "spread", "gain"
    );
    for row in &rows {
        println!(
            "{:>7}  {:>10.1}  {:>7.0}..{:<7.0}  {:>11.1}  {:>7.0}..{:<7.0}  {:>6.2}x",
            row.shards,
            row.single_write_ns,
            row.single_write_low_ns,
            row.single_write_high_ns,
            row.concurrent_write_ns,
            row.concurrent_write_low_ns,
            row.concurrent_write_high_ns,
            row.single_write_ns / row.concurrent_write_ns
        );
    }
    println!(
        "ns per write; gain is one writer's cost over four writers' -- 1.00x means \
the writers never queued, 0.25x means they took turns. Spreads because the \
one-writer column is not stable and a bare median would hide that."
    );

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
    let gain_of = |row: &Row| row.single_write_ns / row.concurrent_write_ns;
    // The cheapest four-writer cost, which is what a caller choosing a shard
    // count would pick. Choosing by the one-over-four ratio instead picked 256
    // shards, where four writers cost MORE per operation than at one shard and
    // the ratio was large only because that row's own one-writer control was
    // slow -- a best row that is worse than the baseline it is compared to.
    let best_write = rows
        .iter()
        .min_by(|left, right| {
            left.concurrent_write_ns
                .total_cmp(&right.concurrent_write_ns)
        })
        .expect("a row");
    println!(
        "{THREADS} writers: {:.0}ns per write at {} shard, {:.0}ns at {} -- {:.2}x, {}",
        first.concurrent_write_ns,
        first.shards,
        best_write.concurrent_write_ns,
        best_write.shards,
        first.concurrent_write_ns / best_write.concurrent_write_ns,
        if best_write.shards > first.shards {
            "so writers DO need the shards, where readers did not"
        } else {
            "so writers do not need them either"
        }
    );
    println!(
        "   at {} shard four writers cost {:.2}x what one writer costs, so they are \
taking turns on the one lock",
        first.shards,
        1.0 / gain_of(first)
    );

    let write_separated = best_write.concurrent_write_high_ns < first.concurrent_write_low_ns;
    println!(
        "   four writers cost {:.0}..{:.0}ns at {} shard against {:.0}..{:.0}ns at {} -- {}",
        first.concurrent_write_low_ns,
        first.concurrent_write_high_ns,
        first.shards,
        best_write.concurrent_write_low_ns,
        best_write.concurrent_write_high_ns,
        best_write.shards,
        if write_separated {
            "separated, so the queueing is the finding"
        } else {
            "did NOT separate, so this has measured the machine and not the lock"
        }
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
