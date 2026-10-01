// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! What does it cost to decide where a key lives, as the cluster grows?
//!
//! A cluster lookup happens before the cache lookup it is for, on every
//! operation, so its cost is added to every hit and every miss. A memory-tier
//! hit is roughly 226ns on this hardware, which is the number to hold this
//! against: routing that costs a fraction of that is bookkeeping, and routing
//! that approaches it is a second cache lookup nobody asked for.
//!
//! Three things are worth watching, and they grow differently.
//!
//! **Placing one key.** A hash and a binary search over the ring. The search is
//! logarithmic in ring points, which is the easy part; the ring is 16 bytes per
//! point, so at four thousand nodes it is ten megabytes and every probe is a
//! cache miss. That is the part that does not look logarithmic.
//!
//! **Placing a key and its copies.** `owners` walks the ring from the key's own
//! point, taking each node whose failure domain it has not used. It stops as
//! soon as there is nothing better to find, so the walk is short when there are
//! domains to spread across -- and the second table is where to look if that
//! stops being true.
//!
//! **Building the ring.** `add_nodes` rebuilds once; adding nodes one at a time
//! rebuilds once per node, each time over a larger ring. The third table is the
//! size of that difference, which is the reason the bulk form exists.
//!
//! **The knob.** Ring points per node is what the first table's cost is really
//! made of, and the default of 160 is chosen for a cluster of tens rather than
//! thousands. Balance depends on the ring's *total* points, so a large cluster
//! already has plenty and can afford fewer each. The last table is that trade:
//! what lowering it buys in lookup cost, and what it costs in how evenly the
//! keys land. `CacheClusterTopology::with_points_per_weight` is how to take it.
//!
//! ```text
//! cargo run --release --no-default-features --example cluster_routing_cost
//! cargo run --release --no-default-features --example cluster_routing_cost -- 1024
//! cargo run --release --no-default-features --example cluster_routing_cost -- 512 \
//!     --json-output /tmp/matrixcache-routing.json --require-passed --max-owner-ns 2000
//! ```

use matrixcache::{CacheClusterTopology, CacheKey, CacheNodeState};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Instant;

/// Node counts the tables step through, filtered by the requested ceiling.
const NODE_STEPS: [usize; 5] = [1, 8, 64, 512, 4_096];
/// Keys placed per measurement. Enough that the timer is not the subject.
const KEYS: usize = 20_000;
/// Medians are taken over this many passes.
const REPEATS: usize = 5;
/// Copies asked for in the second table.
const COPIES: usize = 3;
/// Nodes per failure domain in the zoned arm, so domains stay plentiful.
const NODES_PER_ZONE: usize = 8;
/// Node count for the ring-build table. Adding one at a time is quadratic in
/// this, so it stays small enough to run and large enough to show the shape.
const BUILD_NODES: usize = 256;

fn keys(count: usize) -> Vec<CacheKey> {
    (0..count)
        .map(|index| CacheKey::string((index % 16) as u64, &format!("record-{index:07}")))
        .collect()
}

fn unzoned(nodes: usize) -> CacheClusterTopology {
    let names: Vec<String> = (0..nodes).map(|i| format!("cache-{i:05}")).collect();
    let mut cluster = CacheClusterTopology::new();
    cluster
        .add_nodes(names.iter().map(|name| (name.as_str(), 1)))
        .expect("distinct names");
    cluster
}

fn zoned(nodes: usize) -> CacheClusterTopology {
    let members: Vec<(String, String)> = (0..nodes)
        .map(|i| {
            (
                format!("cache-{i:05}"),
                format!("zone-{:03}", i / NODES_PER_ZONE),
            )
        })
        .collect();
    let mut cluster = CacheClusterTopology::new();
    cluster
        .add_nodes_in_zones(
            members
                .iter()
                .map(|(name, zone)| (name.as_str(), 1, zone.as_str())),
        )
        .expect("distinct names and real zones");
    cluster
}

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[samples.len() / 2]
}

/// Median, lowest and highest of a set of passes.
///
/// The spread is reported rather than hidden. The same configuration measured
/// twice within one run of this bench differed by a quarter, because what is in
/// the processor's caches when a pass begins depends on what ran before it --
/// and a ten megabyte ring is large enough for that to be most of the answer.
/// A single number here would be a number that moves.
fn median_low_high(mut samples: Vec<f64>) -> (f64, f64, f64) {
    samples.sort_by(f64::total_cmp);
    (
        samples[samples.len() / 2],
        samples[0],
        samples[samples.len() - 1],
    )
}

/// Nanoseconds per `owner` call, median of [`REPEATS`] passes.
fn owner_ns(cluster: &CacheClusterTopology, keys: &[CacheKey]) -> (f64, f64, f64) {
    let mut samples = Vec::with_capacity(REPEATS);
    for _ in 0..REPEATS {
        let started = Instant::now();
        let mut sink = 0_usize;
        for key in keys {
            sink += cluster.owner(key).map_or(0, str::len);
        }
        let elapsed = started.elapsed();
        assert!(sink > 0, "every key should have had an owner");
        samples.push(elapsed.as_secs_f64() * 1e9 / keys.len() as f64);
    }
    median_low_high(samples)
}

/// Nanoseconds per `owners` call for [`COPIES`] copies.
fn owners_ns(cluster: &CacheClusterTopology, keys: &[CacheKey], copies: usize) -> f64 {
    let mut samples = Vec::with_capacity(REPEATS);
    for _ in 0..REPEATS {
        let started = Instant::now();
        let mut sink = 0_usize;
        for key in keys {
            sink += cluster.owners(key, copies).len();
        }
        let elapsed = started.elapsed();
        assert!(sink > 0, "every key should have had at least one owner");
        samples.push(elapsed.as_secs_f64() * 1e9 / keys.len() as f64);
    }
    median(samples)
}

/// Keys per node the balance measurement uses.
///
/// Balance has to be measured with enough keys per node that the ring is what
/// the answer is about. The peak of N Poisson draws of mean m is roughly
/// `1 + 3.5/sqrt(m)` times the mean, so at five keys per node the busiest node
/// is three times the mean *from the draw alone* -- which is what the first
/// version of this measured, and it said nothing about the ring. At 250 the
/// draw contributes about 1.22x and the ring is visible underneath it.
const BALANCE_KEYS_PER_NODE: usize = 250;
/// Keys are built in batches so a million of them are not held at once.
const BALANCE_BATCH: usize = 50_000;

/// How much more than its share the busiest node holds.
///
/// The number the ring points are there to keep down: with few of them the arc
/// between two neighbours is a wide random variable, and the busiest node
/// carries the widest one.
fn peak_over_mean(cluster: &CacheClusterTopology) -> (f64, f64) {
    let nodes = cluster.live_node_count();
    let total = nodes.saturating_mul(BALANCE_KEYS_PER_NODE);
    let mut load: HashMap<&str, u32> = HashMap::with_capacity(nodes);
    let mut placed = 0_usize;
    while placed < total {
        let upto = (placed + BALANCE_BATCH).min(total);
        for index in placed..upto {
            let key = CacheKey::string((index % 16) as u64, &format!("balance-{index:08}"));
            *load.entry(cluster.owner(&key).expect("owned")).or_insert(0) += 1;
        }
        placed = upto;
    }
    let mean = total as f64 / nodes as f64;
    let peak = f64::from(*load.values().max().expect("a live cluster"));
    (peak / mean, mean)
}

struct Placement {
    nodes: usize,
    ring_points: usize,
    index_slots: usize,
    owner_ns: f64,
    owner_low_ns: f64,
    owner_high_ns: f64,
    copies_ns: f64,
    zoned_copies_ns: f64,
    domains: usize,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut max_nodes = *NODE_STEPS.last().expect("a last step");
    let mut json_output: Option<PathBuf> = None;
    let mut require_passed = false;
    let mut max_owner_ns: Option<f64> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json-output" => {
                index += 1;
                json_output = Some(PathBuf::from(
                    args.get(index).expect("--json-output needs a path"),
                ));
            }
            "--require-passed" => require_passed = true,
            "--max-owner-ns" => {
                index += 1;
                max_owner_ns = Some(
                    args.get(index)
                        .expect("--max-owner-ns needs a number")
                        .parse()
                        .expect("--max-owner-ns takes a number"),
                );
            }
            other => {
                max_nodes = other.parse().expect("the node ceiling takes a number");
            }
        }
        index += 1;
    }

    let steps: Vec<usize> = NODE_STEPS
        .into_iter()
        .filter(|nodes| *nodes <= max_nodes)
        .collect();
    assert!(
        !steps.is_empty(),
        "no node count at or below {max_nodes} to measure"
    );
    let sample = keys(KEYS);

    println!("placing {KEYS} keys, median of {REPEATS}; a memory-tier hit is about 226ns\n");

    let mut rows = Vec::new();
    for nodes in steps {
        let plain = unzoned(nodes);
        let zoned_cluster = zoned(nodes);
        let (owner_median, owner_low, owner_high) = owner_ns(&plain, &sample);
        rows.push(Placement {
            nodes,
            ring_points: plain.ring_point_count(),
            index_slots: plain.ring_index_slots(),
            owner_ns: owner_median,
            owner_low_ns: owner_low,
            owner_high_ns: owner_high,
            copies_ns: owners_ns(&plain, &sample, COPIES),
            zoned_copies_ns: owners_ns(&zoned_cluster, &sample, COPIES),
            domains: zoned_cluster.failure_domain_count(),
        });
    }

    println!(
        "{:>7}  {:>12}  {:>10}  {:>11}  {:>18}  {:>13}  {:>12}",
        "nodes",
        "ring points",
        "index KiB",
        "owner ns",
        "owner spread",
        "3 copies ns",
        "zoned 3 ns"
    );
    for row in &rows {
        println!(
            "{:>7}  {:>12}  {:>10.1}  {:>11.1}  {:>8.1}..{:<8.1}  {:>13.1}  {:>12.1}",
            row.nodes,
            row.ring_points,
            (row.index_slots + 1) as f64 * 4.0 / 1024.0,
            row.owner_ns,
            row.owner_low_ns,
            row.owner_high_ns,
            row.copies_ns,
            row.zoned_copies_ns
        );
    }

    // What the copy walk does when there is nowhere left to spread to. Every
    // node in one domain, so every copy after the first is one the walk could
    // not separate and had to place anyway.
    let crowded = {
        let names: Vec<String> = (0..512).map(|i| format!("cache-{i:05}")).collect();
        let mut cluster = CacheClusterTopology::new();
        cluster
            .add_nodes_in_zones(names.iter().map(|name| (name.as_str(), 1, "one-rack")))
            .expect("distinct names");
        cluster
    };
    assert_eq!(crowded.failure_domain_count(), 1);
    println!("\n512 nodes in one failure domain, so no copy can be separated:");
    println!("{:>7}  {:>13}", "copies", "ns per key");
    let mut crowded_ns = Vec::new();
    for copies in [1_usize, 3, 16, 64] {
        let ns = owners_ns(&crowded, &sample, copies);
        println!("{copies:>7}  {ns:>13.1}");
        crowded_ns.push((copies, ns));
    }

    // The knob. At the largest size measured above, what fewer points per node
    // buys and what it costs.
    let knob_nodes = rows.last().expect("a row").nodes;
    println!("\n{knob_nodes} nodes, points per node against lookup cost and balance:");
    println!(
        "{:>7}  {:>12}  {:>11}  {:>18}  {:>13}",
        "points", "ring points", "owner ns", "owner spread", "peak/mean"
    );
    let mut knob = Vec::new();
    let mut balance_mean = 0.0_f64;
    let mut knob_low = f64::MAX;
    let mut knob_high = 0.0_f64;
    for points in [160_u32, 40, 10] {
        let names: Vec<String> = (0..knob_nodes).map(|i| format!("cache-{i:05}")).collect();
        let mut cluster = CacheClusterTopology::with_points_per_weight(points);
        cluster
            .add_nodes(names.iter().map(|name| (name.as_str(), 1)))
            .expect("distinct names");
        let (ns, low, high) = owner_ns(&cluster, &sample);
        let (balance, mean) = peak_over_mean(&cluster);
        println!(
            "{points:>7}  {:>12}  {ns:>11.1}  {low:>8.1}..{high:<8.1}  {balance:>13.2}",
            cluster.ring_point_count()
        );
        balance_mean = mean;
        knob_low = knob_low.min(low);
        knob_high = knob_high.max(high);
        knob.push((points, cluster.ring_point_count(), ns, balance));
    }

    println!(
        "  balance measured at {balance_mean:.0} keys per node, where the draw \
alone contributes about {:.2}x",
        1.0 + 3.5 / balance_mean.sqrt()
    );
    // Read the right column. Balance is deterministic -- the same keys, the same
    // hash, no clock -- so a difference in it is real. The cost column is a
    // timing on a machine that may be doing other things, and when the three
    // rows' spreads overlap it has not separated them, whatever the medians say.
    let knob_medians_separated = {
        let mut medians: Vec<f64> = knob.iter().map(|(_, _, ns, _)| *ns).collect();
        medians.sort_by(f64::total_cmp);
        medians
            .windows(2)
            .all(|pair| pair[1] - pair[0] > knob_high - knob_low)
    };
    if knob_medians_separated {
        println!("  the cost column separated the three; both columns are readable");
    } else {
        println!(
            "  the cost column did NOT separate the three -- their spreads overlap \
({knob_low:.0}..{knob_high:.0}ns), so read the balance column and re-run this on \
an idle machine for the cost"
        );
    }

    // Why the bulk form exists: one rebuild against one per node.
    let names: Vec<String> = (0..BUILD_NODES).map(|i| format!("cache-{i:05}")).collect();
    let bulk_started = Instant::now();
    let mut bulk = CacheClusterTopology::new();
    bulk.add_nodes(names.iter().map(|name| (name.as_str(), 1)))
        .expect("distinct names");
    let bulk_ms = bulk_started.elapsed().as_secs_f64() * 1e3;

    let one_started = Instant::now();
    let mut one_at_a_time = CacheClusterTopology::new();
    for name in &names {
        one_at_a_time.add_node(name, 1).expect("a distinct name");
    }
    let one_ms = one_started.elapsed().as_secs_f64() * 1e3;
    assert_eq!(bulk.ring_point_count(), one_at_a_time.ring_point_count());

    println!("\nbuilding a {BUILD_NODES}-node ring:");
    println!("  add_nodes, one rebuild      {bulk_ms:>8.2} ms");
    println!(
        "  add_node, one per node      {one_ms:>8.2} ms  ({:.1}x)",
        one_ms / bulk_ms
    );

    // Marking a node down rebuilds the ring too, which is what a failure costs
    // before any data moves.
    let mut failing = unzoned(rows.last().expect("a row").nodes.max(1));
    let down_started = Instant::now();
    assert!(failing.set_node_state("cache-00000", CacheNodeState::Down));
    let down_ms = down_started.elapsed().as_secs_f64() * 1e3;
    println!("  one node marked down        {down_ms:>8.2} ms");

    let worst_owner_ns = rows.iter().map(|row| row.owner_ns).fold(0.0_f64, f64::max);
    let passed = max_owner_ns.is_none_or(|limit| worst_owner_ns <= limit);

    if let Some(path) = json_output {
        let mut report = String::new();
        let _ = writeln!(report, "{{");
        let _ = writeln!(report, "  \"keys\": {KEYS},");
        let _ = writeln!(report, "  \"repeats\": {REPEATS},");
        let _ = writeln!(report, "  \"copies\": {COPIES},");
        let _ = writeln!(report, "  \"placement\": [");
        for (position, row) in rows.iter().enumerate() {
            let comma = if position + 1 == rows.len() { "" } else { "," };
            let _ = writeln!(
                report,
                "    {{\"nodes\": {}, \"ring_points\": {}, \"domains\": {}, \
                 \"index_slots\": {}, \
                 \"owner_ns\": {:.1}, \"owner_low_ns\": {:.1}, \"owner_high_ns\": {:.1}, \
                 \"copies_ns\": {:.1}, \"zoned_copies_ns\": {:.1}}}{comma}",
                row.nodes,
                row.ring_points,
                row.domains,
                row.index_slots,
                row.owner_ns,
                row.owner_low_ns,
                row.owner_high_ns,
                row.copies_ns,
                row.zoned_copies_ns
            );
        }
        let _ = writeln!(report, "  ],");
        let _ = writeln!(report, "  \"one_domain\": [");
        for (position, (copies, ns)) in crowded_ns.iter().enumerate() {
            let comma = if position + 1 == crowded_ns.len() {
                ""
            } else {
                ","
            };
            let _ = writeln!(
                report,
                "    {{\"copies\": {copies}, \"ns\": {ns:.1}}}{comma}"
            );
        }
        let _ = writeln!(report, "  ],");
        let _ = writeln!(report, "  \"balance_keys_per_node\": {balance_mean:.0},");
        let _ = writeln!(
            report,
            "  \"points_per_node_cost_separated\": {knob_medians_separated},"
        );
        let _ = writeln!(report, "  \"points_per_node\": [");
        for (position, (points, ring_points, ns, balance)) in knob.iter().enumerate() {
            let comma = if position + 1 == knob.len() { "" } else { "," };
            let _ = writeln!(
                report,
                "    {{\"points\": {points}, \"ring_points\": {ring_points}, \
                 \"owner_ns\": {ns:.1}, \"peak_over_mean\": {balance:.3}}}{comma}"
            );
        }
        let _ = writeln!(report, "  ],");
        let _ = writeln!(report, "  \"build_nodes\": {BUILD_NODES},");
        let _ = writeln!(report, "  \"build_bulk_ms\": {bulk_ms:.3},");
        let _ = writeln!(report, "  \"build_one_at_a_time_ms\": {one_ms:.3},");
        let _ = writeln!(report, "  \"mark_down_ms\": {down_ms:.3},");
        let _ = writeln!(report, "  \"worst_owner_ns\": {worst_owner_ns:.1},");
        if let Some(limit) = max_owner_ns {
            let _ = writeln!(report, "  \"max_owner_ns\": {limit:.1},");
        }
        let _ = writeln!(report, "  \"passed\": {passed}");
        let _ = writeln!(report, "}}");
        std::fs::write(&path, report).expect("write the report");
        println!("\nwrote {}", path.display());
    }

    if let Some(limit) = max_owner_ns {
        println!(
            "\nworst owner cost {worst_owner_ns:.1}ns against a ceiling of {limit:.1}ns: {}",
            if passed { "within" } else { "OVER" }
        );
    }
    if require_passed && !passed {
        std::process::exit(1);
    }
}
