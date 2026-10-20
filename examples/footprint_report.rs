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

    let per_entry_terms = footprint.key_bytes + footprint.order_bytes + footprint.table_bytes;

    Row {
        value_bytes,
        capacity,
        payload: footprint.payload_bytes,
        keys: footprint.key_bytes,
        metadata: footprint.metadata_bytes,
        orders: footprint.order_bytes,
        tables: footprint.table_bytes,
        sketch: footprint.sketch_bytes,
        other: footprint.writeback_bytes + footprint.pin_bytes + footprint.fixed_bytes,
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
    per_tracked_entry: f64,
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
    let per_entry_terms = footprint.key_bytes + footprint.order_bytes + footprint.table_bytes;

    TieredRow {
        value_bytes,
        memory_entries: footprint.memory_entries,
        tracked_entries: footprint.tracked_entries,
        payload: footprint.payload_bytes,
        keys: footprint.key_bytes,
        metadata: footprint.metadata_bytes,
        orders: footprint.order_bytes,
        tables: footprint.table_bytes,
        per_tracked_entry: per_entry_terms as f64 / footprint.tracked_entries.max(1) as f64,
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
    let per_entry_terms = footprint.key_bytes + footprint.order_bytes + footprint.table_bytes;

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
        "  {:>6}  {:>10}  {:>9}  {:>8}  {:>8}  {:>9}  {:>11}  {:>7}",
        "value", "payload", "keys", "metadata", "orders", "tables", "sketch", "other"
    );

    let rows: Vec<Row> = [64usize, 256, 1024, 4096]
        .into_iter()
        .map(measure)
        .collect();
    for row in &rows {
        println!(
            "  {:>6}  {:>10}  {:>9}  {:>8}  {:>8}  {:>9}  {:>11}  {:>7}",
            row.value_bytes,
            row.payload,
            row.keys,
            row.metadata,
            row.orders,
            row.tables,
            row.sketch,
            row.other
        );
    }

    println!();
    println!("  the per-entry overhead -- keys, orders, tables");
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
        "  {:>6}  {:>8}  {:>8}  {:>10}  {:>9}  {:>8}  {:>8}  {:>9}  {:>14}",
        "value",
        "in mem",
        "tracked",
        "payload",
        "keys",
        "metadata",
        "orders",
        "tables",
        "B/tracked entry"
    );
    let tiered: Vec<TieredRow> = [64usize, 256, 1024, 4096]
        .into_iter()
        .map(measure_tiered)
        .collect();
    for row in &tiered {
        println!(
            "  {:>6}  {:>8}  {:>8}  {:>10}  {:>9}  {:>8}  {:>8}  {:>9}  {:>14.1}",
            row.value_bytes,
            row.memory_entries,
            row.tracked_entries,
            row.payload,
            row.keys,
            row.metadata,
            row.orders,
            row.tables,
            row.per_tracked_entry
        );
    }

    println!();
    println!("  {:>6}  {:>14}", "value", "total/payload");
    for row in &rows {
        println!("  {:>6}  {:>13.2}x", row.value_bytes, row.amplification);
    }
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
                "    {{\"value_bytes\": {}, \"capacity\": {}, \"payload\": {}, \"keys\": {}, \
                 \"metadata\": {}, \"orders\": {}, \"tables\": {}, \"sketch\": {}, \
                 \"other\": {}, \"total\": {}, \"per_entry_overhead\": {:.1}, \
                 \"amplification\": {:.3}}}{}\n",
                row.value_bytes,
                row.capacity,
                row.payload,
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
