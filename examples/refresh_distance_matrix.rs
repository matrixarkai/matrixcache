// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Does raising `lru_refresh_distance` cost hit rate on workloads other than
//! the one it was first measured on?
//!
//! `refresh_distance_hit_rate` answers that for a single shape — one skew, one
//! capacity ratio — and put the cost of a distance of 512 at 0.03 points. That
//! is a thin basis for changing a default, because the setting trades
//! *ordering accuracy* for throughput and how much ordering accuracy is worth
//! depends entirely on the access pattern:
//!
//! * Under heavy skew the hot set is small and stays resident whatever the
//!   order says, so a stale order costs little.
//! * Under a flat pattern there is no ordering to get wrong — every key is
//!   equally valuable and any victim is as good as any other.
//! * The case that should hurt is **mild** skew with a **tight** cache, where
//!   the eviction decision is both consequential and close.
//!
//! So this sweeps skew against capacity and reports hit rate at each distance.
//!
//! The workload is a fixed deterministic sequence, identical at every point in
//! the matrix, so the differences between cells are exact rather than sampled.
//! A cell that differs by 0.03 points differs by 0.03 points; there is no
//! confidence interval to argue about.
//!
//! Single-threaded on purpose: it measures policy, not throughput, so it can
//! run while something else has the machine.
//!
//! ```text
//! cargo run --release --no-default-features --example refresh_distance_matrix
//! ```

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use std::time::Duration;

const VALUE_BYTES: usize = 64;
const KEY_SPACE: usize = 16_384;
const ACCESSES: usize = 200_000;
/// Windows bracketing this workload's reuse time.
const WINDOWS: [Duration; 4] = [
    Duration::ZERO,
    Duration::from_millis(50),
    Duration::from_millis(500),
    Duration::from_secs(60),
];

/// A deterministic draw whose concentration is set by `exponent`.
///
/// 1.0 is uniform over the key space; 3.0 puts most draws in the low keys.
fn draw(state: &mut u64, exponent: f64) -> usize {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1);
    let unit = ((*state >> 11) as f64) / ((1u64 << 53) as f64);
    let skewed = unit.powf(exponent);
    ((skewed * KEY_SPACE as f64) as usize).min(KEY_SPACE - 1)
}

/// Hit rate for one (skew, capacity, refresh window) point.
fn hit_rate(exponent: f64, resident: usize, window: Duration) -> f64 {
    let cache = MultiLayerCache::try_with_options(CacheOptions::new(resident * VALUE_BYTES, 0, 0))
        .expect("cache");
    cache.set_lru_refresh_time(window);

    let mut state = 0x2545_F491_4F6C_DD1D;
    for _ in 0..ACCESSES {
        let index = draw(&mut state, exponent);
        let key = CacheKey::string(0, &format!("m-{index:06}"));
        if cache.get(&key).expect("get").is_none() {
            cache.put(key, vec![b'v'; VALUE_BYTES]).expect("put");
        }
    }
    let stats = cache.stats();
    let total = stats.memory_hits + stats.misses;
    if total == 0 {
        0.0
    } else {
        stats.memory_hits as f64 / total as f64 * 100.0
    }
}

fn main() {
    println!(
        "{KEY_SPACE} keys, {ACCESSES} accesses, {VALUE_BYTES}-byte values.\n\
         Hit rate %, and the change in points against distance 0.\n"
    );
    print!("{:<8}{:>10}", "skew", "resident");
    for distance in WINDOWS {
        print!("{:>18}", format!("w={distance:?}"));
    }
    println!();

    let mut worst = 0.0_f64;
    let mut worst_cell = String::new();
    // 1.0 uniform, 2.0 moderate, 3.0 heavy. Capacity as a fraction of the key
    // space: tight, then roomy.
    for exponent in [1.0_f64, 2.0, 3.0] {
        for divisor in [8_usize, 4, 2] {
            let resident = KEY_SPACE / divisor;
            print!("{exponent:<8.1}{resident:>10}");
            let mut baseline = 0.0;
            for (position, distance) in WINDOWS.iter().enumerate() {
                let rate = hit_rate(exponent, resident, *distance);
                if position == 0 {
                    baseline = rate;
                    print!("{:>18}", format!("{rate:.2}"));
                } else {
                    let delta = rate - baseline;
                    if *distance <= Duration::from_millis(500) && delta < worst {
                        worst = delta;
                        worst_cell =
                            format!("skew {exponent:.1}, {resident} resident, w={distance:?}");
                    }
                    print!("{:>18}", format!("{rate:.2} ({delta:+.2})"));
                }
            }
            println!();
            // Flushing per row means a long run is readable while it is still
            // going, rather than arriving all at once at the end.
            use std::io::Write;
            std::io::stdout().flush().ok();
        }
    }

    println!(
        "\nworst cell at a window of 500ms or less: {:+.2} points ({})",
        worst,
        if worst_cell.is_empty() {
            "none below baseline".to_string()
        } else {
            worst_cell
        }
    );
    println!(
        "A default above zero is free only if that worst cell is close to zero.\n\
         The one to watch is mild skew with a tight cache, where the eviction\n\
         decision is both consequential and close."
    );
}
