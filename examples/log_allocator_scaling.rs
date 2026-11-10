// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

use matrixcache::{CacheAllocatorApi, SimpleLogBasedMemoryAllocator};
use std::time::Instant;

fn main() {
    println!("live_regions,ns_per_allocate_free");
    for live in [128, 1024, 8192, 32768] {
        let mut allocator = SimpleLogBasedMemoryAllocator::with_capacity((live + 1) * 8);
        for _ in 0..live {
            std::hint::black_box(allocator.allocate(8).unwrap());
        }
        for _ in 0..512 {
            let address = allocator.allocate(8).unwrap();
            allocator.free(address, 8).unwrap();
        }
        let started = Instant::now();
        for _ in 0..4096 {
            let address = allocator.allocate(8).unwrap();
            std::hint::black_box(address);
            allocator.free(address, 8).unwrap();
        }
        println!("{live},{:.1}", started.elapsed().as_nanos() as f64 / 4096.0);
        assert_eq!(allocator.live_region_count(), live);
    }
}
