// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Capacity-pressure exercise. PMEM is file-backed, not physical persistent memory.
//! `--promoted-working-set` supplies increasing caller hotness to exercise SSD
//! replacement. It does not measure automatic popularity adaptation.
//! `--fifo-ssd` selects insertion-order SSD replacement without popularity gating.
//! `--rounds N` repeats the working set 1..=1024 times (default: 4).

use matrixcache::{CacheAdmissionRequest, CacheBlockKind, CacheKey, CacheOptions, MultiLayerCache};
use std::time::Instant;

fn payload(index: usize, round: usize) -> Vec<u8> {
    let mut state = (index as u64 + 1) ^ ((round as u64 + 1) << 32);
    (0..1024)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

fn main() {
    let mut promoted_working_set = false;
    let mut fifo_ssd = false;
    let mut rounds = 4usize;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--promoted-working-set" => promoted_working_set = true,
            "--fifo-ssd" => fifo_ssd = true,
            "--rounds" => {
                rounds = args
                    .next()
                    .expect("--rounds requires a value")
                    .parse()
                    .expect("--rounds requires an integer");
                assert!((1..=1024).contains(&rounds), "--rounds must be 1..=1024");
            }
            _ => panic!("unknown argument: {arg}"),
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let options = || {
        let mut options = CacheOptions::new(64 * 1024, 128 * 1024, 1024 * 1024)
            .with_auto_recover_on_start(true)
            .with_pmem_paths([dir.path().join("pmem")])
            .with_ssd_paths([dir.path().join("ssd")]);
        if fifo_ssd {
            options.cache_ssd_replacement_policy = "fifo".to_string();
        }
        options
    };
    let cache = MultiLayerCache::with_options(options());
    cache.start().unwrap();
    let keys: Vec<_> = (0..8192)
        .map(|i| CacheKey::string(0, &format!("entry-{i:05}")))
        .collect();
    let started = Instant::now();
    let mut hits = 0;
    let mut misses = 0;
    for round in 0..rounds {
        for (index, key) in keys.iter().enumerate() {
            let value = payload(index, round);
            if promoted_working_set {
                cache
                    .put_with_admission(
                        key.clone(),
                        value,
                        CacheAdmissionRequest {
                            block_kind: CacheBlockKind::Object,
                            shard_id: 0,
                            routing_slot: None,
                            block_bytes: 1024,
                            hotness: 100 + ((round * keys.len() + index) / 128 * 4) as u32,
                            pinned: false,
                        },
                    )
                    .unwrap();
            } else {
                cache.put(key.clone(), value).unwrap();
            }
            if index >= 512 {
                let candidate = index - 512;
                match cache.get(&keys[candidate]).unwrap() {
                    Some(value) => {
                        assert!(
                            value == payload(candidate, round),
                            "incorrect value: index={candidate} round={round} bytes={}",
                            value.len()
                        );
                        hits += 1;
                    }
                    None => misses += 1,
                }
            }
        }
        let stats = cache.stats();
        assert!(stats.memory_bytes <= 64 * 1024);
        assert!(stats.pmem_bytes <= 128 * 1024);
        assert!(stats.disk_bytes <= 1024 * 1024);
        if (round + 1) % 16 == 0 {
            eprintln!(
                "completed_rounds={} elapsed_s={:.1}",
                round + 1,
                started.elapsed().as_secs_f64()
            );
        }
    }
    let elapsed = started.elapsed().as_secs_f64();
    let stats = cache.stats();
    assert!(stats.memory_evictions > 0 && stats.pmem_evictions > 0);
    assert!(stats.ssd_evictions > 0 || stats.ssd_admission_rejected > 0);
    if promoted_working_set || fifo_ssd {
        assert!(
            stats.ssd_evictions > 0,
            "selected workload never exercised SSD eviction"
        );
    }
    assert!(cache.stop());
    drop(cache);
    let recovered = MultiLayerCache::with_options(options());
    recovered.start().unwrap();
    let mut retained = 0;
    for (index, key) in keys.iter().enumerate() {
        if let Some(value) = recovered.get(key).unwrap() {
            assert!(
                value == payload(index, rounds - 1),
                "incorrect recovered value: index={index}"
            );
            retained += 1;
        }
    }
    assert!(retained > 0);
    let puts = rounds * keys.len();
    println!("fifo_ssd={fifo_ssd} promoted_working_set={promoted_working_set} rounds={rounds} puts={puts} reads={} hits={hits} misses={misses} elapsed_s={elapsed:.3} workload_ops_per_s={:.1} memory_evictions={} pmem_evictions={} ssd_evictions={} ssd_rejections={} restart_retained={retained}",
        hits + misses, (puts + hits + misses) as f64 / elapsed,
        stats.memory_evictions, stats.pmem_evictions, stats.ssd_evictions, stats.ssd_admission_rejected);
    assert!(recovered.stop());
}
