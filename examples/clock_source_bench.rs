// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Which clock should the read path stamp entries from?
//!
//! The refresh check needs a monotonic-enough "now" on every hit, and how that
//! is obtained turns out to matter more than the check itself.
//!
//! CacheLib takes `std::time(nullptr)` — second granularity — and its own
//! comment says why: *"::time is the fastest for getting the second
//! granularity system clock through the vdso. This is faster than
//! std::chrono::system_clock::now"*. Elsewhere it caches the result in a
//! `thread_local uint32_t staleTime` so that repeated calls inside the same
//! second do not go to the clock at all.
//!
//! Rust's `Instant::now()` is `clock_gettime(CLOCK_MONOTONIC)` and
//! `SystemTime::now()` is `CLOCK_REALTIME` — the one `time()` reads. On a
//! platform where the vDSO serves those they are tens of nanoseconds; where it
//! does not, they are syscalls and cost an order of magnitude more. This box is
//! the second kind, which is exactly why the question needs measuring rather
//! than assuming.
//!
//! Four candidates:
//!
//! * **Instant::now** — monotonic, what the code used first.
//! * **SystemTime::now** — the clock CacheLib actually reads.
//! * **thread-local, amortised** — CacheLib's `staleTime` idea: serve repeats
//!   from a thread-local that touches nothing shared. Amortised by call count
//!   rather than by second, because deciding staleness *by second* costs a
//!   clock call and that is the thing being avoided. Its staleness is then
//!   bounded by throughput rather than by time, which is a real drawback.
//! * **republished atomic** — a background thread stores the time; readers do
//!   a relaxed load. What the branch currently does.
//!
//! ```text
//! cargo run --release --no-default-features --example clock_source_bench
//! ```

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const OPS: usize = 2_000_000;
const REPEATS: usize = 5;

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
    samples[samples.len() / 2]
}

thread_local! {
    /// Calls remaining before this thread reads the clock again, and the
    /// value it last saw. CacheLib's `staleTime` idea -- serve repeats from a
    /// thread-local that touches nothing shared -- amortised by count rather
    /// than by second, because deciding staleness by second needs a clock call
    /// and that is the cost being avoided.
    static CACHED: Cell<(u64, u64)> = const { Cell::new((0, 0)) };
}

/// How many calls are served from the thread-local before the clock is read
/// again. CacheLib does not need this because `time()` reaches its vDSO; where
/// it does not, amortising is the only way the idea pays.
const AMORTISE: u64 = 256;

fn thread_local_millis(base: Instant) -> u64 {
    CACHED.with(|cached| {
        let (countdown, millis) = cached.get();
        if countdown > 0 {
            cached.set((countdown - 1, millis));
            millis
        } else {
            let fresh = base.elapsed().as_millis() as u64;
            cached.set((AMORTISE, fresh));
            fresh
        }
    })
}

fn bench<F: Fn() -> u64 + Send + Sync + Copy + 'static>(threads: usize, op: F) -> f64 {
    let workers = (0..threads)
        .map(|_| {
            std::thread::spawn(move || {
                let started = Instant::now();
                let mut sink = 0_u64;
                for _ in 0..OPS {
                    sink = sink.wrapping_add(op());
                }
                std::hint::black_box(sink);
                started.elapsed().as_nanos() as f64 / OPS as f64
            })
        })
        .collect::<Vec<_>>();
    workers
        .into_iter()
        .map(|w| w.join().expect("worker"))
        .sum::<f64>()
        / threads as f64
}

fn main() {
    // The republished clock the branch currently uses.
    static PUBLISHED: AtomicU64 = AtomicU64::new(0);
    let stop = Arc::new(AtomicBool::new(false));
    let ticker = {
        let stop = Arc::clone(&stop);
        let base = Instant::now();
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                PUBLISHED.store(base.elapsed().as_millis() as u64, Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(10));
            }
        })
    };

    println!("{OPS} ops/thread, median of {REPEATS}, ns per op as seen by one thread\n");
    println!(
        "{:<10}{:>15}{:>17}{:>19}{:>20}",
        "threads", "Instant::now", "SystemTime::now", "thread-local x256", "republished atomic"
    );

    for threads in [1_usize, 2, 4, 8] {
        let base = Instant::now();
        let instant = median(
            (0..REPEATS)
                .map(|_| bench(threads, || Instant::now().elapsed().as_millis() as u64))
                .collect(),
        );
        let system = median(
            (0..REPEATS)
                .map(|_| {
                    bench(threads, || {
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0)
                    })
                })
                .collect(),
        );
        let local = median(
            (0..REPEATS)
                .map(|_| bench(threads, move || thread_local_millis(base)))
                .collect(),
        );
        let published = median(
            (0..REPEATS)
                .map(|_| bench(threads, || PUBLISHED.load(Ordering::Relaxed)))
                .collect(),
        );
        println!("{threads:<10}{instant:>15.2}{system:>17.2}{local:>19.2}{published:>20.2}");
    }

    stop.store(true, Ordering::Relaxed);
    ticker.join().expect("ticker");

    println!(
        "\nThe read path calls this once per hit, so the eight-thread column is the\n\
         one that decides. A source that costs more as threads are added is a\n\
         shared line; one that stays flat is not."
    );
}
