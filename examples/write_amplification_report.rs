// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Bytes written per byte of payload, counted rather than argued.
//!
//! Three quantities, and the whole point of this report is that they are three
//! rather than one. Each is per fill, and each names what it counts:
//!
//! * **payload** -- the bytes of the values handed to `put`. This is the
//!   denominator of every ratio below. It is exact by construction: entries
//!   times the value size, and the report asserts that it is.
//! * **written** -- the bytes this process handed to `write`, read from
//!   `/proc/self/io`. Logical write amplification: what the cache asked the
//!   kernel to store, not what a drive eventually erased. Nothing else in the
//!   process writes during the window the delta is taken over.
//! * **charged** -- what the cache itself publishes as `ssd_bytes_written`,
//!   which is also what the SSD write budget throttles against.
//!
//! The ratios are printed from the terms rather than stored: a ratio that has
//! been through a sum is a different quantity from the ratio of the things
//! summed, and the JSON carries the terms so whoever reads it can divide them
//! their own way.
//!
//! The on-disk residue is decomposed by **what wrote the file**, and every term
//! is asserted non-zero. A decomposition with a residue in it can be wrong in
//! each of its named terms and still add up to the total, because the residue
//! absorbs the error.
//!
//! The difference between written and resident is the third thing worth having:
//! bytes the fill put on the device and then superseded, which is rewriting
//! rather than storing.
//!
//! ```text
//! cargo run --release --no-default-features --example write_amplification_report
//! cargo run --release --no-default-features --example write_amplification_report -- --json-output /tmp/matrixcache-write-amplification.json --require-passed
//! ```

use std::collections::BTreeMap;
use std::path::Path;

use matrixcache::{
    CacheBlockOptions, CacheDataPlacement, CacheKey, CacheTieringPolicy, MultiLayerCache,
};

const ENTRIES: usize = 2_000;
const KEY_CHARS: usize = 32;
/// Entries the memory tier may hold, so the fill is pushed down to the SSD
/// tier rather than measured against a cache that never wrote anything.
const RESIDENT_ENTRIES: usize = 16;
/// The value sizes swept. The ratio has to fall across these: the per-entry
/// costs -- a manifest line, a store key -- are fixed, so a bigger value
/// amortises them. A ratio that did not move with the value size would be a
/// ratio whose denominator is not the payload.
const VALUE_SIZES: [usize; 4] = [64, 256, 1024, 4096];

/// Bytes this process has handed to `write`, from `/proc/self/io`.
///
/// `None` where that file cannot be read, which is a reason to say so rather
/// than to publish a zero: a missing counter would make every amplification
/// below read as 0.000x, which is the most flattering possible answer.
fn written_bytes() -> Option<u64> {
    let io = std::fs::read_to_string("/proc/self/io").ok()?;
    io.lines()
        .find_map(|line| line.strip_prefix("wchar:"))
        .and_then(|rest| rest.trim().parse().ok())
}

/// What wrote a file, from its name.
///
/// Taken from the name the writer chose rather than from where the file sits,
/// because the tiers share directories and a label read off a directory would
/// attribute by location instead of by writer.
fn family_of(name: &str) -> &'static str {
    if name.contains(".cache_block.tmp.") {
        // A block is written to a temporary name and renamed, so one left
        // behind is a write that did not complete.
        "incomplete blocks"
    } else if name.ends_with(".cache_block") {
        "ssd block files"
    } else if name.contains("journal") {
        "store journal"
    } else if name.contains("compat_store") {
        "store snapshot"
    } else if name.contains("manifest") || name.starts_with("MANIFEST") {
        // The cache's own manifest and the store's share a family deliberately:
        // both are the index of what the tier holds, written beside the blocks.
        "tier index"
    } else if name.ends_with(".log") {
        // Named by generation -- `000004.log` -- so the name cannot be a
        // family or the table's columns move between runs.
        "store write-ahead log"
    } else if name.ends_with(".sst") {
        "store sorted tables"
    } else if name.ends_with(".bin") {
        "persistent tier blocks"
    } else {
        // `CURRENT`, `IDENTITY`, `LOCK`, `LOG`, `OPTIONS-000007`: a handful of
        // files written once, none of them per entry.
        "store bookkeeping"
    }
}

/// Files under `root`, grouped by what wrote them: count, bytes, largest.
fn by_family(root: &Path) -> BTreeMap<&'static str, (u64, u64, u64)> {
    let mut out: BTreeMap<&'static str, (u64, u64, u64)> = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                stack.push(entry.path());
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let slot = out.entry(family_of(&name)).or_insert((0, 0, 0));
            slot.0 += 1;
            slot.1 += meta.len();
            slot.2 = slot.2.max(meta.len());
        }
    }
    out
}

fn key(index: usize) -> CacheKey {
    let digits = format!("{index:08}");
    let mut record = String::with_capacity(KEY_CHARS);
    record.push_str(&"k".repeat(KEY_CHARS - digits.len()));
    record.push_str(&digits);
    assert_eq!(record.len(), KEY_CHARS);
    CacheKey::string(0, &record)
}

/// A value that will not compress, so the block carries the payload.
///
/// A compressible fixture frames to forty-odd bytes whatever the value size,
/// and the report would then publish an amplification below one while
/// measuring nothing about the write path.
fn incompressible(bytes: usize) -> Vec<u8> {
    let mut fill = 0x2545_f491_4f6c_dd1du64;
    (0..bytes)
        .map(|_| {
            fill ^= fill << 13;
            fill ^= fill >> 7;
            fill ^= fill << 17;
            (fill & 0xff) as u8
        })
        .collect()
}

struct Row {
    value_bytes: usize,
    payload: u64,
    written: u64,
    charged: u64,
    /// Bytes the **fill** left on disk: what is there afterwards, less what was
    /// already there when the window opened. A store that writes its own
    /// bookkeeping while it is being opened would otherwise be charged to the
    /// fill at one end and not at the other, and the superseded term below --
    /// which is a difference of the two -- would come out negative and
    /// saturate to nothing.
    resident: u64,
    families: BTreeMap<&'static str, (u64, u64, u64)>,
    resident_at_start: u64,
    memory_entries: usize,
    tracked_entries: usize,
}

impl Row {
    fn amplification(&self) -> f64 {
        self.written as f64 / self.payload as f64
    }
    fn on_disk(&self) -> f64 {
        self.resident as f64 / self.payload as f64
    }
    fn charged_share(&self) -> f64 {
        self.charged as f64 / self.written as f64
    }
    fn superseded(&self) -> u64 {
        self.written.saturating_sub(self.resident)
    }
    fn family(&self, name: &str) -> (u64, u64, u64) {
        self.families.get(name).copied().unwrap_or((0, 0, 0))
    }
    /// Bytes under `name` that the fill wrote, for the one family that can
    /// already hold bytes before the window opens.
    fn family_bytes_from_the_fill(&self, name: &str) -> u64 {
        let (_, bytes, _) = self.family(name);
        if name == "store bookkeeping" {
            bytes.saturating_sub(self.resident_at_start)
        } else {
            bytes
        }
    }
}

fn measure(value_bytes: usize) -> Option<Row> {
    let root = std::env::temp_dir().join(format!(
        "matrixcache-write-amplification-{value_bytes}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let cache = MultiLayerCache::with_tiering_policy(
        root.clone(),
        CacheTieringPolicy {
            memory_capacity_bytes: RESIDENT_ENTRIES * value_bytes,
            pmem_capacity_bytes: 0,
            ssd_capacity_bytes: ENTRIES * value_bytes * 8,
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
    let value = incompressible(value_bytes);

    // Nothing may be printed between these two reads, and nothing else in the
    // process writes, so the delta is the cache's own writes. The store may
    // already have written its bookkeeping while it was being opened, so what
    // is on disk at this moment is measured too and subtracted at the end.
    let resident_at_start: u64 = by_family(&root).values().map(|(_, bytes, _)| bytes).sum();
    let charged_before = cache.stats().ssd_bytes_written;
    let before = written_bytes()?;
    for index in 0..ENTRIES {
        cache
            .put(key(index), value.clone())
            .expect("a cache with room must accept a put");
    }
    let written = written_bytes()? - before;
    let charged = cache.stats().ssd_bytes_written - charged_before;

    let families = by_family(&root);
    let resident_after: u64 = families.values().map(|(_, bytes, _)| bytes).sum();
    let footprint = cache.footprint();
    let row = Row {
        value_bytes,
        payload: (ENTRIES * value_bytes) as u64,
        written,
        charged,
        resident: resident_after.saturating_sub(resident_at_start),
        families,
        resident_at_start,
        memory_entries: footprint.memory_entries,
        tracked_entries: footprint.tracked_entries,
    };
    let _ = std::fs::remove_dir_all(&root);
    Some(row)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let json_output = args
        .windows(2)
        .find(|pair| pair[0] == "--json-output")
        .map(|pair| pair[1].clone());
    let require_passed = args.iter().any(|arg| arg == "--require-passed");

    let mut failures: Vec<String> = Vec::new();
    let mut rows = Vec::new();
    for value_bytes in VALUE_SIZES {
        match measure(value_bytes) {
            Some(row) => rows.push(row),
            None => {
                failures.push(
                    "/proc/self/io could not be read, so there is no write counter to report \
                     -- this is a fixture problem, not a measurement"
                        .to_string(),
                );
                break;
            }
        }
    }

    println!(
        "{ENTRIES} entries, {KEY_CHARS}-character keys, incompressible values, write-through to \
         the SSD tier."
    );
    println!("The denominator of every ratio is the payload: the bytes handed to `put`.");
    println!();
    println!(
        "{:>7} {:>12} {:>14} {:>12} {:>12} {:>11} {:>9}",
        "value", "payload", "written", "resident", "superseded", "written/B", "on disk/B"
    );
    for row in &rows {
        println!(
            "{:>7} {:>12} {:>14} {:>12} {:>12} {:>10.3}x {:>8.3}x",
            row.value_bytes,
            row.payload,
            row.written,
            row.resident,
            row.superseded(),
            row.amplification(),
            row.on_disk()
        );
    }

    println!();
    println!(
        "  Superseded is written less resident, so it is bytes the fill put on the device and
           then replaced: journal compaction, or an index written whole again. It reads as zero
           where a backend preallocates a file, because the file is then longer than what has
           been written through it and the difference is not a quantity this can see."
    );
    println!();
    println!("  what is on disk afterwards, by what wrote it (bytes an entry)");
    println!();
    let mut names: Vec<&str> = Vec::new();
    for row in &rows {
        for name in row.families.keys() {
            if !names.contains(name) {
                names.push(name);
            }
        }
    }
    print!("{:>7}", "value");
    for name in &names {
        print!(" {:>24}", name);
    }
    println!();
    for row in &rows {
        print!("{:>7}", row.value_bytes);
        for name in &names {
            print!(
                " {:>24.1}",
                row.family_bytes_from_the_fill(name) as f64 / ENTRIES as f64
            );
        }
        println!();
    }

    println!();
    println!("  what the cache publishes, against what it wrote");
    println!();
    println!(
        "{:>7} {:>14} {:>14} {:>12}",
        "value", "written", "charged", "published"
    );
    for row in &rows {
        println!(
            "{:>7} {:>14} {:>14} {:>11.3}x",
            row.value_bytes,
            row.written,
            row.charged,
            row.charged_share()
        );
    }
    println!();
    println!(
        "  `ssd_bytes_written` is charged once per block handed to the tier, and the tier writes\n  \
         that block to a file AND to the store behind it, with a manifest line beside them. So\n  \
         the figure the SSD write budget throttles against is a fraction of what the path puts\n  \
         on the device, and the last column is that fraction."
    );

    // ---------------------------------------------------------------- checks
    // Fixture checks first, so a fill that never reached the tier fails as a
    // fixture problem rather than being published as a low amplification.
    if rows.len() != VALUE_SIZES.len() {
        failures.push(format!(
            "{} of {} value sizes were measured",
            rows.len(),
            VALUE_SIZES.len()
        ));
    }
    for row in &rows {
        if row.payload != (ENTRIES * row.value_bytes) as u64 {
            failures.push(format!(
                "the {}-byte row's denominator is {} where the payload handed to put was {}",
                row.value_bytes,
                row.payload,
                ENTRIES * row.value_bytes
            ));
        }
        if row.tracked_entries != ENTRIES {
            failures.push(format!(
                "the {}-byte row tracked {} of {ENTRIES} entries, so some of the fill was \
                 dropped and the denominator does not describe it",
                row.value_bytes, row.tracked_entries
            ));
        }
        if row.memory_entries * 4 > ENTRIES {
            failures.push(format!(
                "the {}-byte row left {} of {ENTRIES} entries in memory, so the fill is not \
                 reaching the tier this measures",
                row.value_bytes, row.memory_entries
            ));
        }
        if row.written == 0 {
            failures.push(format!(
                "the {}-byte row wrote nothing at all",
                row.value_bytes
            ));
        }
        // Each named term non-zero. A decomposition can be wrong in every term
        // and still sum to the total, so the terms are checked rather than the
        // sum. Which terms exist is a property of the backend, so the list is
        // the backend's -- a term checked on the arm that cannot produce it
        // would be a failure about the build rather than about the writes.
        for name in TERMS_THAT_MUST_CARRY_BYTES {
            let (files, bytes, _) = row.family(name);
            if bytes == 0 {
                failures.push(format!(
                    "the {}-byte row has no bytes under \"{name}\", so that term of the \
                     decomposition is not being measured",
                    row.value_bytes
                ));
            }
            if name == "ssd block files" && files * 2 < ENTRIES as u64 {
                failures.push(format!(
                    "the {}-byte row left {files} block files for {ENTRIES} entries",
                    row.value_bytes
                ));
            }
        }
        // The payload has to be carried whole by something, or this is a report
        // about compression rather than about writing. The compatibility store
        // writes one file per block, so the largest of those is the value plus
        // a header; the other backend packs many values into one file, so the
        // same claim is made of the total it wrote.
        if row.resident < row.payload {
            failures.push(format!(
                "the {}-byte row left {} bytes on disk for {} of payload, so the values are \
                 not being stored whole and the fixture has started compressing",
                row.value_bytes, row.resident, row.payload
            ));
        }
        if row.amplification() <= 1.0 {
            failures.push(format!(
                "the {}-byte row wrote {:.3} bytes per payload byte, which is at or below one \
                 -- the fill is not reaching the device",
                row.value_bytes,
                row.amplification()
            ));
        }
    }

    // The ratio must respond to its denominator: a term charged per entry --
    // an index line, a store key -- is the same size whatever the value is, so
    // a bigger value amortises it. A figure that did not move with the value
    // size would be one whose denominator is not the payload.
    //
    // Stated across the ends of the sweep rather than step by step, because
    // step by step is **not** true of every backend: one of the two flushes to
    // a sorted table once the fill outgrows its memtable, which writes the
    // values a second time and puts the ratio back up at the largest value.
    // Asserting monotonicity would be asserting that fixture rather than
    // anything about the cache.
    if let (Some(first), Some(last)) = (rows.first(), rows.last()) {
        if last.amplification() >= first.amplification() {
            failures.push(format!(
                "amplification is {:.3}x at {} bytes and {:.3}x at {} bytes; the per-entry \
                 terms should amortise across that range, so a figure that did not come down \
                 is one its denominator is not moving",
                first.amplification(),
                first.value_bytes,
                last.amplification(),
                last.value_bytes
            ));
        }
    }
    // And step by step on the backend where every term is per entry.
    #[cfg(not(feature = "rocksdb-ssd"))]
    for pair in rows.windows(2) {
        if pair[1].amplification() >= pair[0].amplification() {
            failures.push(format!(
                "amplification did not fall from the {}-byte value ({:.3}x) to the {}-byte one \
                 ({:.3}x), and on this backend every term it writes is charged per entry",
                pair[0].value_bytes,
                pair[0].amplification(),
                pair[1].value_bytes,
                pair[1].amplification()
            ));
        }
    }

    // The ratchet. Replaced on purpose, with the number that replaced it.
    for row in &rows {
        let ceiling = match row.value_bytes {
            64 => WRITE_AMPLIFICATION_64,
            256 => WRITE_AMPLIFICATION_256,
            1024 => WRITE_AMPLIFICATION_1024,
            _ => WRITE_AMPLIFICATION_4096,
        };
        if row.amplification() > ceiling {
            failures.push(format!(
                "the {}-byte row writes {:.3} bytes per payload byte, above the {ceiling:.2} it \
                 has been held to",
                row.value_bytes,
                row.amplification()
            ));
        }
    }

    if failures.is_empty() {
        println!();
        println!("self-checks: passed");
    } else {
        println!();
        println!("self-checks: FAILED");
        for failure in &failures {
            println!("  - {failure}");
        }
    }

    if let Some(path) = json_output {
        let mut json = String::from("{\n  \"passed\": ");
        json.push_str(if failures.is_empty() { "true" } else { "false" });
        json.push_str(",\n  \"entries\": ");
        json.push_str(&ENTRIES.to_string());
        json.push_str(",\n  \"key_chars\": ");
        json.push_str(&KEY_CHARS.to_string());
        json.push_str(",\n  \"rows\": [\n");
        for (index, row) in rows.iter().enumerate() {
            json.push_str("    {\"value_bytes\": ");
            json.push_str(&row.value_bytes.to_string());
            json.push_str(", \"payload_bytes\": ");
            json.push_str(&row.payload.to_string());
            json.push_str(", \"written_bytes\": ");
            json.push_str(&row.written.to_string());
            json.push_str(", \"charged_bytes\": ");
            json.push_str(&row.charged.to_string());
            json.push_str(", \"resident_bytes\": ");
            json.push_str(&row.resident.to_string());
            json.push_str(", \"resident_at_start_bytes\": ");
            json.push_str(&row.resident_at_start.to_string());
            json.push_str(", \"families\": {");
            for (position, (name, (files, bytes, largest))) in row.families.iter().enumerate() {
                if position > 0 {
                    json.push_str(", ");
                }
                json.push('"');
                json.push_str(name);
                json.push_str("\": {\"files\": ");
                json.push_str(&files.to_string());
                json.push_str(", \"bytes\": ");
                json.push_str(&bytes.to_string());
                json.push_str(", \"largest_bytes\": ");
                json.push_str(&largest.to_string());
                json.push('}');
            }
            json.push('}');
            json.push('}');
            if index + 1 < rows.len() {
                json.push(',');
            }
            json.push('\n');
        }
        json.push_str("  ]\n}\n");
        std::fs::write(&path, json).expect("the report must be writable");
        println!("wrote {path}");
    }

    if require_passed && !failures.is_empty() {
        std::process::exit(1);
    }
}

/// The terms whose bytes must be non-zero, which is a property of the backend
/// rather than of the write path.
///
/// The compatibility store writes a block file, appends to its journal and
/// writes the tier index beside them. The other backend writes through a
/// write-ahead log and has no block files and no index of ours at all, so
/// asking it for those would be a failure about the build.
#[cfg(not(feature = "rocksdb-ssd"))]
const TERMS_THAT_MUST_CARRY_BYTES: [&str; 3] = ["ssd block files", "store journal", "tier index"];
#[cfg(feature = "rocksdb-ssd")]
const TERMS_THAT_MUST_CARRY_BYTES: [&str; 1] = ["store write-ahead log"];

// The ceilings, per value size, measured on this tree and identical across
// three runs of the same binary. They are not a target: they are where the
// write path is, so that a change which writes more fails here rather than
// being noticed much later. Raising one is a deliberate act, and the number
// that replaced it goes beside it.
//
//            64      256     1024     4096
//   measured   9.648    4.946    4.293    3.147   compatibility store
//   measured   3.235    1.563    1.141    2.026   the other backend
#[cfg(not(feature = "rocksdb-ssd"))]
const WRITE_AMPLIFICATION_64: f64 = 10.0;
#[cfg(not(feature = "rocksdb-ssd"))]
const WRITE_AMPLIFICATION_256: f64 = 5.15;
#[cfg(not(feature = "rocksdb-ssd"))]
const WRITE_AMPLIFICATION_1024: f64 = 4.45;
#[cfg(not(feature = "rocksdb-ssd"))]
const WRITE_AMPLIFICATION_4096: f64 = 3.27;
// The other backend's ceilings are deliberately loose, and the reason is in
// the figures: its first three rows repeat to the byte across runs, and its
// 4,096-byte row came out 2.026x, 1.675x, 1.675x and 1.291x on four runs of the
// same binary. The fill outgrows that store's memtable there and the flush it
// provokes lands inside or outside the measured window depending on when the
// background thread gets the processor. A ceiling tight enough to catch a small
// regression on that row would be a ceiling that fails on a loaded machine, so
// these four are catastrophe checks and not claims to catch anything smaller.
// The tight gate is the one above, on a backend that is deterministic to the
// byte, and that is the arm the workflow runs.
#[cfg(feature = "rocksdb-ssd")]
const WRITE_AMPLIFICATION_64: f64 = 4.0;
#[cfg(feature = "rocksdb-ssd")]
const WRITE_AMPLIFICATION_256: f64 = 2.2;
#[cfg(feature = "rocksdb-ssd")]
const WRITE_AMPLIFICATION_1024: f64 = 1.6;
#[cfg(feature = "rocksdb-ssd")]
const WRITE_AMPLIFICATION_4096: f64 = 4.0;
