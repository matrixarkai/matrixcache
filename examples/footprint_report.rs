// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Where a cache's bytes go, beyond the bytes it was asked to hold.
//!
//! The capacity bound counts payload: `memory_capacity_bytes` bounds the bytes
//! of the values handed to `put`. Everything the cache allocates in order to
//! find those values again sits outside that bound. This prints it, term by
//! term.
//!
//! Almost all of the overhead is charged **per entry** -- the keys, the
//! recency orders and the hash tables. That makes it ruinous for small entries
//! and rounding error for large ones, and flat in the value size, which the
//! self-check below asserts.
//!
//! The `metadata` column is printed for decomposition and **not added**: a
//! `HashMap` stores its values inline in the table it allocates, and the
//! metadata map's value is a `CacheEntryMeta` rather than a handle to one, so
//! those bytes are already in `tables`. They were added as well until today,
//! which inflated every per-entry figure this report published by the size of
//! that struct.
//!
//! The one term that is not per entry is the admission sketch, which is sized
//! from `memory_capacity_bytes` when the admission filter is switched on. With
//! the filter off -- the default -- it is a few dozen bytes whatever the
//! capacity. The last section measures both, because the difference between
//! them is large: a sketch sized for a gibibyte of capacity is half a
//! gibibyte.
//!
//! ```text
//! cargo run --release --no-default-features --example footprint_report
//! cargo run --release --no-default-features --example footprint_report -- --json-output /tmp/matrixcache-footprint.json --require-passed
//! ```

use matrixcache::{
    CacheBlockOptions, CacheDataPlacement, CacheKey, CacheOptions, CacheTieringPolicy,
    MultiLayerCache,
};

const ENTRIES: usize = 20_000;
const KEY_CHARS: usize = 32;
/// Capacity as a multiple of what the fill needs, so nothing evicts.
const HEADROOM: usize = 4;

/// A key whose `record_key` is `KEY_CHARS` long, so the per-entry key cost is
/// a stated quantity rather than whatever the example happened to produce.
fn record(index: usize) -> String {
    let digits = format!("{index:08}");
    let mut record = String::with_capacity(KEY_CHARS);
    record.push_str(&"k".repeat(KEY_CHARS - digits.len()));
    record.push_str(&digits);
    assert_eq!(record.len(), KEY_CHARS);
    record
}

fn key(index: usize) -> CacheKey {
    CacheKey::string(0, &record(index))
}

struct Row {
    /// The sixteen bytes in front of each payload: a strong count and a
    /// weak count, resident for as long as the entry is, and counted by
    /// nothing until today.
    handles: usize,
    value_bytes: usize,
    capacity: usize,
    payload: usize,
    keys: usize,
    metadata: usize,
    orders: usize,
    tables: usize,
    sketch: usize,
    other: usize,
    total: usize,
    /// Overhead charged per entry: everything but the sketch.
    per_entry: f64,
    amplification: f64,
}

fn measure(value_bytes: usize) -> Row {
    let capacity = ENTRIES * value_bytes * HEADROOM;
    let dir = std::env::temp_dir().join(format!("matrixcache-footprint-{value_bytes}"));
    let _ = std::fs::remove_dir_all(&dir);
    let cache =
        MultiLayerCache::with_options(CacheOptions::new(capacity, 0, 0).with_ssd_paths(vec![dir]));
    let value = vec![0u8; value_bytes];
    for index in 0..ENTRIES {
        cache
            .put(key(index), value.clone())
            .expect("a cache with room must accept a put");
    }

    let footprint = cache.footprint();
    assert_eq!(
        footprint.memory_entries, ENTRIES,
        "nothing may evict during the fill, or the denominator is wrong"
    );

    let per_entry_terms = footprint.key_bytes
        + footprint.order_bytes
        + footprint.table_bytes
        // The sixteen bytes in front of each payload. Per entry, and
        // overhead rather than payload, so it belongs in this sum: a
        // figure built from the keys, the orders and the tables alone was
        // missing the one term paid for every value in the cache.
        + footprint.payload_header_bytes;

    Row {
        handles: footprint.payload_header_bytes,
        value_bytes,
        capacity,
        payload: footprint.payload_bytes,
        keys: footprint.key_bytes,
        metadata: footprint.metadata_bytes,
        orders: footprint.order_bytes,
        tables: footprint.table_bytes,
        sketch: footprint.sketch_bytes,
        other: footprint.writeback_bytes
            + footprint.pin_bytes
            + footprint.fixed_bytes
            + footprint.eviction_queue_bytes,
        total: footprint.total_bytes,
        per_entry: per_entry_terms as f64 / ENTRIES as f64,
        amplification: footprint.amplification(),
    }
}

/// What a cache with all three tiers configured holds, per entry it tracks.
///
/// The memory-only table above is the common case, but it leaves out the two
/// tables a tiered cache adds -- the PMEM map and the disk index -- and it
/// divides by a denominator that happens to equal the entry count because
/// every entry is memory-resident. In a tiered cache most entries are not, so
/// the figure to read is overhead per *tracked* entry.
struct TieredRow {
    value_bytes: usize,
    memory_entries: usize,
    tracked_entries: usize,
    payload: usize,
    keys: usize,
    metadata: usize,
    orders: usize,
    tables: usize,
    /// Which map holds `tables`, so that a reader can see which one to go
    /// after. The aggregate says how much and not where.
    tables_by_map: matrixcache::CacheTableBytes,
    /// What the SSD tier's store holds in memory, which this arm reported as
    /// nothing until today -- the footprint excluded it on the grounds that
    /// the tier's bytes live on a drive, which is true of the RocksDB build
    /// and false of this one, where the store is a map of blocks with a
    /// snapshot and a journal behind it.
    ssd_store: usize,
    per_tracked_entry: f64,
    /// The same figure with the store counted. Reported beside rather than
    /// inside, because how much the store holds depends on how many entries
    /// the admission policy sent to the tier -- so it moves with the value
    /// size, and folding it in would break the flatness the per-entry terms
    /// genuinely have.
    with_store_per_tracked_entry: f64,
}

fn measure_tiered(value_bytes: usize) -> TieredRow {
    const TIERED_ENTRIES: usize = 2_000;
    let dir = std::env::temp_dir().join(format!("matrixcache-footprint-tiered-{value_bytes}"));
    let _ = std::fs::remove_dir_all(&dir);
    let cache = MultiLayerCache::with_tiering_policy(
        dir,
        CacheTieringPolicy {
            // Deliberately far smaller than the fill, so entries demote to
            // PMEM and the tier tables are populated. A memory tier with room
            // for everything measures the table above a second time.
            memory_capacity_bytes: 16 * value_bytes,
            pmem_capacity_bytes: 1 << 24,
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
    let value = vec![0u8; value_bytes];
    for index in 0..TIERED_ENTRIES {
        cache
            .put(key(index), value.clone())
            .expect("a cache with room must accept a put");
    }

    let footprint = cache.footprint();
    let per_entry_terms = footprint.key_bytes
        + footprint.order_bytes
        + footprint.table_bytes
        // The sixteen bytes in front of each payload. Per entry, and
        // overhead rather than payload, so it belongs in this sum: a
        // figure built from the keys, the orders and the tables alone was
        // missing the one term paid for every value in the cache.
        + footprint.payload_header_bytes;

    TieredRow {
        value_bytes,
        memory_entries: footprint.memory_entries,
        tracked_entries: footprint.tracked_entries,
        payload: footprint.payload_bytes,
        keys: footprint.key_bytes,
        metadata: footprint.metadata_bytes,
        orders: footprint.order_bytes,
        tables: footprint.table_bytes,
        tables_by_map: footprint.table_bytes_by_map,
        ssd_store: footprint.ssd_store_bytes,
        per_tracked_entry: per_entry_terms as f64 / footprint.tracked_entries.max(1) as f64,
        with_store_per_tracked_entry: (per_entry_terms + footprint.ssd_store_bytes) as f64
            / footprint.tracked_entries.max(1) as f64,
    }
}

/// The sketch with the admission filter off and then on, per capacity.
///
/// Two caches rather than one toggled, so the "off" figure is what a cache
/// that never enables the filter actually holds rather than what one holds
/// before being switched.
fn measure_sketch() -> Vec<(usize, usize, usize)> {
    [1usize << 20, 1 << 24, 1 << 28, 1 << 30]
        .into_iter()
        .map(|capacity| {
            let off_dir = std::env::temp_dir().join(format!("matrixcache-sketch-off-{capacity}"));
            let on_dir = std::env::temp_dir().join(format!("matrixcache-sketch-on-{capacity}"));
            let _ = std::fs::remove_dir_all(&off_dir);
            let _ = std::fs::remove_dir_all(&on_dir);

            let off = MultiLayerCache::with_options(
                CacheOptions::new(capacity, 0, 0).with_ssd_paths(vec![off_dir]),
            );
            let on = MultiLayerCache::with_options(
                CacheOptions::new(capacity, 0, 0).with_ssd_paths(vec![on_dir]),
            );
            on.set_admission_filter_enabled(true);

            (
                capacity,
                off.footprint().sketch_bytes,
                on.footprint().sketch_bytes,
            )
        })
        .collect()
}

/// The headline figure is a **sawtooth** in the entry count, and this arm is
/// what says so.
///
/// Every per-entry figure here divides hash tables by the entries in them,
/// and a `HashMap` holds a power of two of buckets: it doubles when the
/// entries pass seven eighths of what it has. So the per-entry cost falls as
/// a table fills and jumps back the moment it grows, and a figure quoted at
/// one entry count is a figure quoted at one phase of that cycle.
///
/// The same key shape as the headline arm, so the 20,000-entry row here is
/// the headline figure measured a second way -- which the self-checks compare.
struct PhaseRow {
    entries: usize,
    /// The buckets a map of this many entries holds, derived the way the cache
    /// derives them.
    buckets: usize,
    /// Entries over buckets as a percentage: where in the growth cycle this
    /// count sits.
    load: f64,
    per_entry: f64,
}

const PHASE_VALUE_BYTES: usize = 64;

fn measure_phase(entries: usize) -> PhaseRow {
    let capacity = entries * PHASE_VALUE_BYTES * HEADROOM;
    let cache = MultiLayerCache::with_options(CacheOptions::new(capacity, 0, 0));
    let value = vec![0u8; PHASE_VALUE_BYTES];
    for index in 0..entries {
        cache
            .put(key(index), value.clone())
            .expect("a cache with room must accept a put");
    }
    let footprint = cache.footprint();
    assert_eq!(
        footprint.memory_entries, entries,
        "nothing may evict during the fill, or the denominator is wrong"
    );
    let per_entry_terms = footprint.key_bytes
        + footprint.order_bytes
        + footprint.table_bytes
        + footprint.payload_header_bytes;
    PhaseRow {
        entries,
        buckets: estimated_buckets_for(entries),
        load: entries as f64 / estimated_buckets_for(entries) as f64 * 100.0,
        per_entry: per_entry_terms as f64 / entries as f64,
    }
}

/// The buckets a `HashMap` holds for this many entries: the next power of two
/// above eight sevenths of them, which is the rule the cache's own table
/// accounting uses.
fn estimated_buckets_for(entries: usize) -> usize {
    (entries * 8 / 7).next_power_of_two()
}

/// What the *shape* of a key costs, which none of the tables above show.
///
/// Every row above uses `CacheKey::string`, whose namespace and selector are
/// both literals the key borrows. That is the cheapest shape the cache has.
/// The other constructors derive their record key or compute their selector,
/// and a computed selector is a second allocation that stays resident for as
/// long as the entry does -- held once per key allocation, which is once per
/// entry.
///
/// So the headline figure above is a floor across shapes rather than a typical
/// cost, which the self-checks below assert rather than leave implied.
struct ShapeRow {
    shape: &'static str,
    record_chars: usize,
    owned_selector: usize,
    /// Capacity the selector holds beyond its length. A selector sized by
    /// `format!` rather than reserved keeps this, for the life of the entry.
    selector_slack: usize,
    keys: usize,
    per_entry: f64,
    amplification: f64,
}

const SHAPE_VALUE_BYTES: usize = 64;

fn selector_bytes(key: &CacheKey) -> (usize, usize) {
    match &key.selector {
        std::borrow::Cow::Borrowed(_) => (0, 0),
        std::borrow::Cow::Owned(owned) => (owned.len(), owned.capacity() - owned.len()),
    }
}

fn measure_shape(shape: &'static str, build: &dyn Fn(usize) -> CacheKey) -> ShapeRow {
    let capacity = ENTRIES * SHAPE_VALUE_BYTES * HEADROOM;
    let cache = MultiLayerCache::with_options(CacheOptions::new(capacity, 0, 0));
    let value = vec![0u8; SHAPE_VALUE_BYTES];
    for index in 0..ENTRIES {
        cache
            .put(build(index), value.clone())
            .expect("a cache with room must accept a put");
    }

    let footprint = cache.footprint();
    assert_eq!(
        footprint.memory_entries, ENTRIES,
        "nothing may evict during the fill, or the denominator is wrong"
    );
    assert_eq!(
        footprint.payload_bytes,
        ENTRIES * SHAPE_VALUE_BYTES,
        "every entry of this shape has to be a distinct key, or the fill is smaller than it looks"
    );

    let sample = build(0);
    let (owned_selector, selector_slack) = selector_bytes(&sample);
    let per_entry_terms = footprint.key_bytes
        + footprint.order_bytes
        + footprint.table_bytes
        // The sixteen bytes in front of each payload. Per entry, and
        // overhead rather than payload, so it belongs in this sum: a
        // figure built from the keys, the orders and the tables alone was
        // missing the one term paid for every value in the cache.
        + footprint.payload_header_bytes;

    ShapeRow {
        shape,
        record_chars: sample.record_key.len(),
        owned_selector,
        selector_slack,
        keys: footprint.key_bytes,
        per_entry: per_entry_terms as f64 / ENTRIES as f64,
        amplification: footprint.amplification(),
    }
}

/// What a cache that shrank still holds, against one that never grew.
///
/// A `HashMap` never shrinks on its own, so the tables stay sized for the high
/// water mark however few entries are left. Measured before the shrink was
/// added: a cache taken from 20,000 entries down to 1,000 held 3,762 bytes an
/// entry where a cache that never grew held 320.
///
/// The recency order's arena is compacted rather than left at its high water
/// mark -- a node's index is its identity, so that means rebuilding the arena
/// in eviction order and rewriting every name in both lists, the index and the
/// cold bits. What is left is the rounding: the arena is rebuilt at the live
/// count and grows by doubling after that.
struct ShrunkenRow {
    label: &'static str,
    entries: usize,
    keys: usize,
    orders: usize,
    tables: usize,
    per_entry: f64,
}

const SHRUNKEN_FILLED: usize = 20_000;
const SHRUNKEN_KEPT: usize = 1_000;

fn measure_shrunken() -> Vec<ShrunkenRow> {
    let value = vec![0u8; 64];
    let build = || MultiLayerCache::with_options(CacheOptions::new(1 << 30, 0, 0));
    let row = |label: &'static str, cache: &MultiLayerCache| {
        let footprint = cache.footprint();
        let entries = footprint.tracked_entries.max(1);
        ShrunkenRow {
            label,
            entries: footprint.tracked_entries,
            keys: footprint.key_bytes,
            orders: footprint.order_bytes,
            tables: footprint.table_bytes,
            per_entry: (footprint.key_bytes
                + footprint.order_bytes
                + footprint.table_bytes
                + footprint.payload_header_bytes) as f64
                / entries as f64,
        }
    };

    let shrunk = build();
    for index in 0..SHRUNKEN_FILLED {
        shrunk
            .put(key(index), value.clone())
            .expect("a cache with room must accept a put");
    }
    let mut rows = vec![row("after the fill", &shrunk)];
    for index in SHRUNKEN_KEPT..SHRUNKEN_FILLED {
        shrunk
            .invalidate(&key(index))
            .expect("a started cache must accept an invalidate");
    }
    rows.push(row("after shrinking", &shrunk));

    let never_grew = build();
    for index in 0..SHRUNKEN_KEPT {
        never_grew
            .put(key(index), value.clone())
            .expect("a cache with room must accept a put");
    }
    rows.push(row("never grew", &never_grew));
    rows
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let json_output = args
        .windows(2)
        .find(|pair| pair[0] == "--json-output")
        .map(|pair| pair[1].clone());
    let require_passed = args.iter().any(|arg| arg == "--require-passed");

    println!("{ENTRIES} entries, {KEY_CHARS}-character keys, memory tier only.");
    println!("Capacity is {HEADROOM}x what the fill needs, so nothing evicts.");
    println!();
    println!(
        "  {:>6}  {:>10}  {:>9}  {:>9}  {:>8}  {:>8}  {:>9}  {:>11}  {:>7}",
        "value", "payload", "handles", "keys", "metadata", "orders", "tables", "sketch", "other"
    );

    let rows: Vec<Row> = [64usize, 256, 1024, 4096]
        .into_iter()
        .map(measure)
        .collect();
    for row in &rows {
        println!(
            "  {:>6}  {:>10}  {:>9}  {:>9}  {:>8}  {:>8}  {:>9}  {:>11}  {:>7}",
            row.value_bytes,
            row.payload,
            row.handles,
            row.keys,
            row.metadata,
            row.orders,
            row.tables,
            row.sketch,
            row.other
        );
    }

    println!();
    println!(
        "  the per-entry overhead -- keys, orders, tables, and the handle in front of each \
         payload"
    );
    println!("  (the metadata column above is inside tables, not beside it)");
    println!();
    println!(
        "  {:>6}  {:>16}  {:>22}",
        "value", "B/entry", "as a share of the value"
    );
    for row in &rows {
        println!(
            "  {:>6}  {:>16.1}  {:>21.1}%",
            row.value_bytes,
            row.per_entry,
            row.per_entry / row.value_bytes as f64 * 100.0
        );
    }

    println!();
    println!("  the admission sketch, which is the one term not charged per entry");
    println!();
    println!(
        "  {:>14}  {:>14}  {:>14}  {:>10}",
        "capacity", "filter off", "filter on", "on/capacity"
    );
    let sketches = measure_sketch();
    for (capacity, off, on) in &sketches {
        println!(
            "  {:>14}  {:>14}  {:>14}  {:>9.3}x",
            capacity,
            off,
            on,
            *on as f64 / *capacity as f64
        );
    }

    println!();
    println!("  all three tiers configured, 2000 entries, most of them demoted");
    println!();
    println!(
        "  {:>6}  {:>8}  {:>8}  {:>10}  {:>9}  {:>8}  {:>8}  {:>9}  {:>10}  {:>14}",
        "value",
        "in mem",
        "tracked",
        "payload",
        "keys",
        "metadata",
        "orders",
        "tables",
        "ssd store",
        "B/tracked entry"
    );
    let tiered: Vec<TieredRow> = [64usize, 256, 1024, 4096]
        .into_iter()
        .map(measure_tiered)
        .collect();
    for row in &tiered {
        println!(
            "  {:>6}  {:>8}  {:>8}  {:>10}  {:>9}  {:>8}  {:>8}  {:>9}  {:>10}  {:>14.1}",
            row.value_bytes,
            row.memory_entries,
            row.tracked_entries,
            row.payload,
            row.keys,
            row.metadata,
            row.orders,
            row.tables,
            row.ssd_store,
            row.per_tracked_entry
        );
    }

    println!();
    println!(
        "  and with the SSD store's resident bytes counted, which the arm above omitted until \
         today:"
    );
    println!("  {:>6}  {:>16}", "value", "B/tracked entry");
    for row in &tiered {
        println!(
            "  {:>6}  {:>16.1}",
            row.value_bytes, row.with_store_per_tracked_entry
        );
    }
    println!();
    println!("  which map holds the table bytes, per tracked entry");
    println!();
    println!(
        "  {:>6}  {:>8}  {:>8}  {:>10}  {:>9}  {:>9}  {:>8}",
        "value", "memory", "pmem", "disk index", "metadata", "expiries", "total"
    );
    for row in &tiered {
        let per_entry = |bytes: usize| bytes as f64 / row.tracked_entries.max(1) as f64;
        let maps = row.tables_by_map;
        println!(
            "  {:>6}  {:>8.1}  {:>8.1}  {:>10.1}  {:>9.1}  {:>9.1}  {:>8.1}",
            row.value_bytes,
            per_entry(maps.memory),
            per_entry(maps.pmem),
            per_entry(maps.disk_index),
            per_entry(maps.metadata),
            per_entry(maps.expiries),
            per_entry(maps.total())
        );
    }
    println!();
    println!("  The metadata column holds the entries this tier is not holding: a memory-resident");
    println!("  entry keeps its metadata inline in the memory tier's own bucket, which is why the");
    println!("  memory-only arm above reports no metadata table at all and this arm, where almost");
    println!("  every entry has been demoted, reports the largest table in the cache.");
    println!();
    // Named, because this table reads off the memory-only rows measured near
    // the top of this arm, not off the tiered rows printed just above it. It
    // used to sit directly under the tiered table with nothing saying which
    // numbers it was about.
    println!("  amplification of the memory-only cache, resident over payload");
    println!();
    println!("  {:>6}  {:>14}", "value", "total/payload");
    for row in &rows {
        println!("  {:>6}  {:>13.2}x", row.value_bytes, row.amplification);
    }
    println!();

    println!(
        "  what a cache that shrank still holds ({SHRUNKEN_FILLED} entries down to \
         {SHRUNKEN_KEPT}, 64-byte values)"
    );
    println!();
    println!(
        "  {:>16}  {:>8}  {:>9}  {:>9}  {:>9}  {:>9}",
        "cache", "entries", "keys", "orders", "tables", "B/entry"
    );
    let shrunken = measure_shrunken();
    for row in &shrunken {
        println!(
            "  {:>16}  {:>8}  {:>9}  {:>9}  {:>9}  {:>9.1}",
            row.label, row.entries, row.keys, row.orders, row.tables, row.per_entry
        );
    }
    println!();

    println!(
        "  the same figure across the growth cycle of the tables it divides, \
         {PHASE_VALUE_BYTES}-byte values"
    );
    println!();
    println!(
        "  {:>8}  {:>9}  {:>7}  {:>9}",
        "entries", "buckets", "load", "B/entry"
    );
    let phases: Vec<PhaseRow> = [
        14_000usize,
        16_000,
        ENTRIES,
        24_000,
        28_000,
        28_700,
        30_000,
        32_000,
    ]
    .into_iter()
    .map(measure_phase)
    .collect();
    for row in &phases {
        println!(
            "  {:>8}  {:>9}  {:>6.1}%  {:>9.1}",
            row.entries, row.buckets, row.load, row.per_entry
        );
    }
    println!();
    println!(
        "  A table holds a power of two of buckets and doubles at seven eighths full, so this"
    );
    println!(
        "  figure falls as the tables fill and jumps back when they grow. The headline above is"
    );
    println!(
        "  measured at {ENTRIES} entries, which is one phase of that cycle rather than the best"
    );
    println!("  or the worst of it.");
    println!();
    println!("  what the key shape costs, {ENTRIES} entries of {SHAPE_VALUE_BYTES}-byte values");
    println!();
    println!(
        "  {:>9}  {:>7}  {:>10}  {:>6}  {:>9}  {:>8}  {:>7}",
        "shape", "record", "owned sel", "slack", "keys", "B/entry", "ampl."
    );
    let shapes: Vec<ShapeRow> = vec![
        measure_shape("string", &|index| CacheKey::string(0, &record(index))),
        measure_shape("set", &|index| CacheKey::set_members(0, &record(index))),
        measure_shape("hash", &|index| {
            CacheKey::hash(0, &record(index), "field-00000123")
        }),
        measure_shape("page", &|index| CacheKey::page(0, index as u64, 4096, 512)),
        measure_shape("feature", &|index| {
            CacheKey::feature_query(
                0,
                &record(index),
                1_700_000_000_000,
                1_700_000_600_000,
                Some(5_000),
            )
        }),
    ];
    for row in &shapes {
        println!(
            "  {:>9}  {:>7}  {:>10}  {:>6}  {:>9}  {:>8.1}  {:>6.2}x",
            row.shape,
            row.record_chars,
            row.owned_selector,
            row.selector_slack,
            row.keys,
            row.per_entry,
            row.amplification
        );
    }
    println!();

    // ---- Self-checks ------------------------------------------------------

    let mut failures: Vec<String> = Vec::new();

    // The ratchet.
    //
    // Every check below this block is relational -- flat in the value size,
    // non-zero, one shape no cheaper than another -- and not one of them
    // would notice the figures going back up. A change that gave away
    // everything this work has won would have passed every gate in this file.
    //
    // So the figures are pinned, each a little above what is measured today,
    // with the ladder that got there. They are ceilings and not predictions:
    // raising one is a deliberate act, and the number that replaced it goes
    // in the ladder beside it.
    //
    //   memory-only, per entry      377.5 -> 311.3 -> 298.2 -> 283.5 -> 299.5
    //   its amplification at 64 B    6.90x -> 5.87x -> 5.66x -> 5.43x -> 5.68x
    //   tiered, per tracked entry   426.4 -> 409.6 -> 411.0 -> 427.0
    //   shrunken to 1,000 of 20,000  3,762.5 -> 1,223.8 -> 363.6 -> 347.2 -> 328.9 -> 344.9
    //   a cache that never grew      330.8 -> 314.4 -> 296.1 -> 312.1
    //   the dearest key shape        343.3 -> 330.2 -> 315.5 -> 331.5
    //
    // The last step on every line is **upward by sixteen bytes an entry, and
    // it is not a regression**: it is the handle in front of each payload --
    // a strong count and a weak count on the `Arc<[u8]>` the tier holds --
    // which was resident all along and counted by nothing. The cache did not
    // get worse; these figures got honest. Every ceiling below was raised for
    // that reason and no other, which is what this comment is for.
    //
    // Deterministic, all of it: these are struct sizes and table sizes at a
    // fixed entry count, and three runs of the same binary gave the same
    // figures to the tenth of a byte. Nothing here is a timing.
    const MEMORY_ONLY_CEILING: f64 = 306.0;
    const MEMORY_ONLY_AMPLIFICATION_CEILING: f64 = 5.8;
    const TIERED_CEILING: f64 = 435.0;
    const SHRUNKEN_CEILING: f64 = 352.0;
    const NEVER_GREW_CEILING: f64 = 320.0;
    const DEAREST_SHAPE_CEILING: f64 = 340.0;

    for row in &rows {
        if row.per_entry > MEMORY_ONLY_CEILING {
            failures.push(format!(
                "the memory-only overhead is {:.1} bytes an entry at {} byte values, above the \
                 {MEMORY_ONLY_CEILING:.0} this has been held to -- raise the ceiling on purpose \
                 or do not give the bytes back",
                row.per_entry, row.value_bytes
            ));
        }
        if row.value_bytes == 64 && row.amplification > MEMORY_ONLY_AMPLIFICATION_CEILING {
            failures.push(format!(
                "the memory-only amplification at 64-byte values is {:.2}x, above the \
                 {MEMORY_ONLY_AMPLIFICATION_CEILING:.2}x this has been held to",
                row.amplification
            ));
        }
    }
    for row in &tiered {
        if row.per_tracked_entry > TIERED_CEILING {
            failures.push(format!(
                "the tiered overhead is {:.1} bytes a tracked entry at {} byte values, above \
                 the {TIERED_CEILING:.0} this has been held to",
                row.per_tracked_entry, row.value_bytes
            ));
        }
    }

    // The per-entry terms must not move with the value size. If they do,
    // something in them is charged per byte and this table is describing it
    // wrongly.
    let per_entry: Vec<f64> = rows.iter().map(|row| row.per_entry).collect();
    let smallest = per_entry.iter().cloned().fold(f64::MAX, f64::min);
    let largest = per_entry.iter().cloned().fold(f64::MIN, f64::max);
    if smallest <= 0.0 {
        failures.push("per-entry overhead came out as zero or less, which cannot be".to_string());
    } else if largest / smallest > 1.01 {
        failures.push(format!(
            "the per-entry terms range from {smallest:.1} to {largest:.1} bytes across value \
             sizes, so one of them is charged per byte rather than per entry and the heading \
             above is wrong"
        ));
    }

    // The tiered arm is worthless unless entries actually left the memory
    // tier: a cache that held everything in memory would report the table
    // above again under a different heading. This is the check an earlier
    // version of this arm did not have, and it reported a difference of zero
    // because nothing had demoted.
    for row in &tiered {
        if row.tracked_entries == 0 {
            failures.push(format!(
                "the tiered arm at {} byte values tracked no entries at all",
                row.value_bytes
            ));
        } else if row.memory_entries * 4 > row.tracked_entries {
            failures.push(format!(
                "the tiered arm at {} byte values left {} of {} entries in the memory tier, so \
                 it is not measuring a tiered cache",
                row.value_bytes, row.memory_entries, row.tracked_entries
            ));
        }
    }

    // The SSD store's resident bytes are the term this arm used to omit. With
    // RocksDB it is zero, because those bytes are allocated by the native
    // library and cannot be seen from here, so the check is one-sided: it may
    // be zero, but if it is not it has to account for what the store cannot
    // avoid holding -- one key per entry it was given, each of which is that
    // entry's manifest line.
    //
    // NOT measured against the payload. That was the first version of this
    // check and it failed: the values in this arm are all zeros, so the blocks
    // compress to almost nothing and the store holds keys and table rather
    // than bytes. A payload-relative bound says nothing about a compressed
    // tier -- the same trap that made an earlier disk measurement meaningless.
    const STORE_KEY_FLOOR_BYTES: usize = 32;
    for row in &tiered {
        let demoted = row.tracked_entries.saturating_sub(row.memory_entries);
        if row.ssd_store > 0 && row.ssd_store < demoted * STORE_KEY_FLOOR_BYTES {
            failures.push(format!(
                "the SSD store reports {} bytes resident for the {} entries demoted out \
                 of memory, which is less than the key it has to hold for each of them",
                row.ssd_store, demoted
            ));
        }
    }

    // And the per-tracked-entry overhead must be flat in the value size here
    // too, for the same reason it must be flat above.
    let tiered_per_entry: Vec<f64> = tiered.iter().map(|row| row.per_tracked_entry).collect();
    let tiered_smallest = tiered_per_entry.iter().cloned().fold(f64::MAX, f64::min);
    let tiered_largest = tiered_per_entry.iter().cloned().fold(f64::MIN, f64::max);
    if tiered_smallest > 0.0 && tiered_largest / tiered_smallest > 1.01 {
        failures.push(format!(
            "the tiered per-entry terms range from {tiered_smallest:.1} to {tiered_largest:.1} \
             bytes across value sizes, so one of them is charged per byte"
        ));
    }

    // With the filter off the sketch must not grow with capacity at all. That
    // is the claim the table above makes, and it is the one worth guarding:
    // the sketch was once the largest thing many caches held.
    let off_sizes: Vec<usize> = sketches.iter().map(|(_, off, _)| *off).collect();
    let off_largest = off_sizes.iter().copied().max().unwrap_or(0);
    if off_largest > 1024 {
        failures.push(format!(
            "a cache with the admission filter off holds up to {off_largest} bytes of sketch, \
             which is not the few dozen the table claims"
        ));
    }

    // And with the filter on it must be sized from capacity, or the filter has
    // been quietly left without an estimator.
    for (capacity, _, on) in &sketches {
        let expected = (capacity / 2).next_power_of_two();
        if *on != expected {
            failures.push(format!(
                "with the filter on, a cache of {capacity} bytes holds {on} bytes of sketch where \
                 {expected} was expected; the filter needs its estimator sized from capacity"
            ));
        }
    }

    // A cache that shrank must give its table space back: a `HashMap` never
    // shrinks on its own, and the tables are the largest per-entry term.
    match (
        shrunken.iter().find(|row| row.label == "after shrinking"),
        shrunken.iter().find(|row| row.label == "never grew"),
        shrunken.iter().find(|row| row.label == "after the fill"),
    ) {
        (Some(after), Some(small), Some(filled)) => {
            if after.tables > small.tables {
                failures.push(format!(
                    "a cache shrunk to {} entries holds {} bytes of table where one that never \
                     grew holds {}; the tables are staying sized for the high water mark",
                    after.entries, after.tables, small.tables
                ));
            }
            if after.tables * 4 > filled.tables {
                failures.push(format!(
                    "the tables barely came down: {} against {} when full",
                    after.tables, filled.tables
                ));
            }
            // The orders come down too, now that the arena is compacted. Not
            // all the way: the rebuilt arena grows by doubling, so a thousand
            // live nodes sit in one sized for two thousand.
            if after.orders > small.orders * 2 {
                failures.push(format!(
                    "a cache shrunk to {} entries holds {} bytes of recency order where one \
                     that never grew holds {}; the arena is not being compacted",
                    after.entries, after.orders, small.orders
                ));
            }
        }
        _ => failures.push("the shrunken arm lost one of its three rows".to_string()),
    }
    for row in &shrunken {
        let ceiling = match row.label {
            "after shrinking" => SHRUNKEN_CEILING,
            "never grew" => NEVER_GREW_CEILING,
            // The filled row is the memory-only arm measured again, and it is
            // ratcheted there.
            _ => continue,
        };
        if row.per_entry > ceiling {
            failures.push(format!(
                "the \"{}\" row holds {:.1} bytes an entry, above the {ceiling:.0} it has been \
                 held to",
                row.label, row.per_entry
            ));
        }
    }

    // A selector the cache keeps is held for the life of the entry and once
    // per key allocation, so slack in it is resident waste. `format!` sizes a
    // string from nothing and grows it: the feature query's selector came out
    // of it holding fifty-two bytes for thirty-two characters.
    for row in &shapes {
        if row.selector_slack > 0 {
            failures.push(format!(
                "the {} shape keeps {} bytes of slack in its selector, which is resident for \
                 the life of the entry; reserve the allocation instead of letting it grow",
                row.shape, row.selector_slack
            ));
        }
    }

    // A positive control. If no shape here computes a selector, the check
    // above passes by measuring nothing at all.
    if !shapes.iter().any(|row| row.owned_selector > 0) {
        failures.push(
            "no shape in this arm computes a selector, so the slack check above is vacuous"
                .to_string(),
        );
    }

    // The headline table uses the cheapest shape, which makes its figure a
    // floor rather than a typical cost. Worth stating as a check, because the
    // number gets quoted on its own.
    // The sweep across the growth cycle, which is what keeps the headline
    // figure honest rather than lucky.
    //
    // Measured: 257.3 bytes an entry at 85.4% load and 330.1 at 43.8%, so the
    // published figure swings by 1.28x with nothing changing but the entry
    // count -- 28,000 entries reports 257.3 and 28,700 reports 330.1, a 28%
    // jump for 2.5% more entries. A per-entry figure over a hash table is a
    // sawtooth, and these checks pin the envelope rather than one tooth.
    const PHASE_WORST_CEILING: f64 = 340.0;
    match (
        phases
            .iter()
            .map(|row| row.per_entry)
            .fold(f64::INFINITY, f64::min),
        phases
            .iter()
            .map(|row| row.per_entry)
            .fold(f64::NEG_INFINITY, f64::max),
        phases.iter().find(|row| row.entries == ENTRIES),
        rows.iter().find(|row| row.value_bytes == PHASE_VALUE_BYTES),
    ) {
        (best, worst, Some(headline_phase), Some(headline_row)) => {
            // The same quantity measured twice, in two arms, with the same
            // key shape and the same value size. A difference means one of
            // the two fixtures has drifted from what it claims to measure.
            if (headline_phase.per_entry - headline_row.per_entry).abs() > 0.05 {
                failures.push(format!(
                    "the sweep reports {:.1} bytes an entry at {ENTRIES} entries and the \
                     headline arm reports {:.1}, so the two arms are not measuring the same \
                     cache",
                    headline_phase.per_entry, headline_row.per_entry
                ));
            }
            // The ceiling that does not depend on where the fixture sits in
            // the cycle: whatever phase a deployment lands in, this is the
            // figure it is held to.
            if worst > PHASE_WORST_CEILING {
                failures.push(format!(
                    "the worst phase in the sweep holds {worst:.1} bytes an entry, above the \
                     {PHASE_WORST_CEILING:.0} the envelope has been held to"
                ));
            }
            // And the headline has to be an ordinary phase, not the flattering
            // end of the cycle. If a change moved the fixture to the fullest
            // table it could find, this is what would say so.
            if headline_row.per_entry <= best + 0.05 {
                failures.push(format!(
                    "the headline entry count reports {:.1} bytes an entry, which is the best \
                     phase in the sweep -- the fixture is sitting at the fullest table rather \
                     than a representative one",
                    headline_row.per_entry
                ));
            }
            if headline_row.per_entry >= worst - 0.05 {
                failures.push(format!(
                    "the headline entry count reports {:.1} bytes an entry, which is the worst \
                     phase in the sweep",
                    headline_row.per_entry
                ));
            }
            // The phenomenon itself. If the tables stopped growing by powers
            // of two this would hold flat, and the prose beside the table
            // would be describing something that no longer happens.
            if worst / best < 1.15 {
                failures.push(format!(
                    "the sweep reports {best:.1} to {worst:.1} bytes an entry, a swing of \
                     {:.2}x -- the tables are no longer growing the way the text beside this \
                     table says they do",
                    worst / best
                ));
            }
        }
        (_, _, headline_phase, headline_row) => failures.push(format!(
            "the sweep is missing a row at {ENTRIES} entries ({}) or the headline arm is \
             missing its {PHASE_VALUE_BYTES}-byte row ({})",
            headline_phase.is_some(),
            headline_row.is_some()
        )),
    }

    // The dearest shape is ratcheted too, since the headline figure is the
    // cheapest one and a regression could hide in the others.
    for row in &shapes {
        if row.per_entry > DEAREST_SHAPE_CEILING {
            failures.push(format!(
                "the {} shape costs {:.1} bytes an entry, above the \
                 {DEAREST_SHAPE_CEILING:.0} the dearest shape has been held to",
                row.shape, row.per_entry
            ));
        }
    }

    if let Some(string_row) = shapes.iter().find(|row| row.shape == "string") {
        for row in &shapes {
            if row.per_entry < string_row.per_entry - 0.05 {
                failures.push(format!(
                    "the {} shape costs {:.1} bytes an entry against the string shape's {:.1}, \
                     so the headline figure is not the floor it is presented as",
                    row.shape, row.per_entry, string_row.per_entry
                ));
            }
        }
    } else {
        failures.push("the shape arm lost the string shape it compares against".to_string());
    }

    if failures.is_empty() {
        println!("self-checks: passed");
        println!();
        println!(
            "With the admission filter off, which is the default, the sketch holds {} bytes",
            off_largest
        );
        println!("whatever the capacity. Switched on, it is sized from capacity: half a");
        println!("gibibyte for a gibibyte, which is why it waits to be asked for.");
    } else {
        println!("self-checks: FAILED");
        for failure in &failures {
            println!("  - {failure}");
        }
    }

    if let Some(path) = json_output {
        let mut json = String::from("{\n");
        json.push_str(&format!("  \"entries\": {ENTRIES},\n"));
        json.push_str(&format!("  \"key_chars\": {KEY_CHARS},\n"));
        json.push_str("  \"rows\": [\n");
        for (index, row) in rows.iter().enumerate() {
            json.push_str(&format!(
                "    {{\"value_bytes\": {}, \"capacity\": {}, \"payload\": {}, \"handles\": {}, \"keys\": {}, \
                 \"metadata\": {}, \"orders\": {}, \"tables\": {}, \"sketch\": {}, \
                 \"other\": {}, \"total\": {}, \"per_entry_overhead\": {:.1}, \
                 \"amplification\": {:.3}}}{}\n",
                row.value_bytes,
                row.capacity,
                row.payload,
                row.handles,
                row.keys,
                row.metadata,
                row.orders,
                row.tables,
                row.sketch,
                row.other,
                row.total,
                row.per_entry,
                row.amplification,
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
