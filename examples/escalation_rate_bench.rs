// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! How often does a read have to take the cache exclusively?
//!
//! Escalation is what limits read scaling: a hit served entirely under the
//! shared lock costs nothing to its neighbours, and one that has to move its
//! entry in the recency order serialises against every other write. So the
//! share of hits that escalate is the number that predicts whether reads scale,
//! and it is now exported as `access_order_refreshes`.
//!
//! Two things drive it, and they pull in opposite directions:
//!
//! * **the refresh window.** An entry re-read inside the window keeps its place
//!   and stays on the shared path. A window shorter than the workload's reuse
//!   time escalates everything.
//! * **churn.** The first read after admission always escalates, because a new
//!   entry may have been placed part-way down the order and has to be lifted
//!   out. A workload that admits constantly therefore escalates constantly,
//!   however long the window is.
//!
//! That second one is a real cost, introduced deliberately, and this measures
//! it rather than assuming it is small: the churn column is the price of the
//! first-read rule.
//!
//! ```text
//! cargo run --release --no-default-features --example escalation_rate_bench
//! ```

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use std::time::Duration;

const VALUE_BYTES: usize = 64;
const RESIDENT: usize = 4_096;
const READS: usize = 200_000;

/// The share of hits that escalated, and the hit rate, for one workload.
fn measure(window: Duration, key_space: usize) -> (f64, f64) {
    let cache = MultiLayerCache::try_with_options(CacheOptions::new(RESIDENT * VALUE_BYTES, 0, 0))
        .expect("cache");
    cache.set_lru_refresh_time(window);

    let mut state = 0x2545_F491_4F6C_DD1D_u64;
    for _ in 0..READS {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let index = ((state >> 33) as usize) % key_space;
        let key = CacheKey::string(0, &format!("e-{index:07}"));
        if cache.get(&key).expect("get").is_none() {
            cache.put(key, vec![b'v'; VALUE_BYTES]).expect("put");
        }
    }

    let stats = cache.stats();
    let hits = stats.memory_hits;
    let looked_up = hits + stats.misses;
    let escalated = if hits == 0 {
        0.0
    } else {
        stats.access_order_refreshes as f64 / hits as f64 * 100.0
    };
    let hit_rate = if looked_up == 0 {
        0.0
    } else {
        hits as f64 / looked_up as f64 * 100.0
    };
    (escalated, hit_rate)
}

fn main() {
    println!("room for {RESIDENT} entries, {READS} uniform reads, refilled on miss\n");
    println!(
        "{:<24}{:>16}{:>14}{:>16}{:>14}",
        "refresh window", "settled: esc%", "hit rate", "churning: esc%", "hit rate"
    );

    for (label, window) in [
        ("0 (always moves)", Duration::ZERO),
        ("1ms", Duration::from_millis(1)),
        ("500ms (default)", Duration::from_millis(500)),
        ("60s", Duration::from_secs(60)),
    ] {
        // Settled: the key space fits, so almost everything is a re-read of
        // something already admitted and the window decides.
        let (settled_esc, settled_hit) = measure(window, RESIDENT / 2);
        // Churning: the key space is ten times capacity, so most reads miss and
        // admit, and the read after each admission escalates whatever the
        // window says.
        let (churn_esc, churn_hit) = measure(window, RESIDENT * 10);
        println!(
            "{label:<24}{settled_esc:>15.1}%{settled_hit:>13.1}%\
             {churn_esc:>15.1}%{churn_hit:>13.1}%"
        );
    }

    println!(
        "\nThe settled column is what the refresh window buys: a window longer than\n\
         the reuse time keeps reads on the shared path. The churning column is what\n\
         it cannot buy, because the first read after an admission always escalates --\n\
         that is the floor, and it is set by how often the workload admits."
    );
}
