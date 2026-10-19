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
//! metadata, the recency orders and the hash tables. That makes it ruinous for
//! small entries and rounding error for large ones, and flat in the value
//! size, which the self-check below asserts.
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

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};

const ENTRIES: usize = 20_000;
const KEY_CHARS: usize = 32;
/// Capacity as a multiple of what the fill needs, so nothing evicts.
const HEADROOM: usize = 4;

/// A key whose `record_key` is `KEY_CHARS` long, so the per-entry key cost is
/// a stated quantity rather than whatever the example happened to produce.
fn key(index: usize) -> CacheKey {
    let digits = format!("{index:08}");
    let mut record = String::with_capacity(KEY_CHARS);
    record.push_str(&"k".repeat(KEY_CHARS - digits.len()));
    record.push_str(&digits);
    assert_eq!(record.len(), KEY_CHARS);
    CacheKey::string(0, &record)
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

    let per_entry_terms = footprint.key_bytes
        + footprint.metadata_bytes
        + footprint.order_bytes
        + footprint.table_bytes;

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
    println!("  the per-entry overhead -- keys, metadata, orders, tables");
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
    println!("  {:>6}  {:>14}", "value", "total/payload");
    for row in &rows {
        println!("  {:>6}  {:>13.2}x", row.value_bytes, row.amplification);
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
