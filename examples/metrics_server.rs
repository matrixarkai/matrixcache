// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Serves the cache's statistics for Grafana to scrape.
//!
//! Exposes `/metrics` in Prometheus text format, covering every field of
//! `CacheStats` — including tier residency, sharded batch fan-out, and latency
//! histograms — plus `/healthz` and a plain-text index at `/`.
//!
//! No HTTP dependency: this is `std::net::TcpListener` and a handful of lines
//! of response writing, because a metrics endpoint that drags in an async
//! runtime is a much bigger decision than a metrics endpoint. If a real server
//! is already in the process, call
//! [`matrixcache::prometheus_text`] from its own handler instead and ignore
//! this file — that function is the actual deliverable.
//!
//! To watch it do something, it drives a small skewed workload in the
//! background so the numbers move.
//!
//! ```text
//! cargo run --release --no-default-features --example metrics_server
//! curl -s localhost:9184/metrics | head -40
//! ```
//!
//! Then point Prometheus at it:
//!
//! ```yaml
//! scrape_configs:
//!   - job_name: matrixcache
//!     static_configs:
//!       - targets: ["localhost:9184"]
//! ```

use matrixcache::{CacheKey, CacheOptions, PrometheusScrapeBuffer, ShardedMultiLayerCache};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const PORT: u16 = 9184;
const VALUE_BYTES: usize = 256;
const KEY_SPACE: usize = 16_384;
const RESIDENT: usize = KEY_SPACE / 4;
const SHARDS: usize = 4;
const SMALL_BATCH: usize = 32;
const LARGE_BATCH: usize = 320;

/// Deterministic skew, so the dashboard shows a realistic hit rate rather than
/// the 100% a uniform loop over a resident set would produce.
fn skewed(state: &mut u64) -> usize {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1);
    let unit = ((*state >> 11) as f64) / ((1u64 << 53) as f64);
    ((unit * unit * unit * KEY_SPACE as f64) as usize).min(KEY_SPACE - 1)
}

fn batch_keys(prefix: &str, worker: usize, start: usize, count: usize) -> Vec<CacheKey> {
    (0..count)
        .map(|offset| {
            let logical = start.wrapping_add(offset);
            CacheKey::string(logical as u64, &format!("{prefix}-{worker}-{logical:06}"))
        })
        .collect()
}

fn respond(mut stream: TcpStream, status: &str, content_type: &str, body: &str) {
    // A scraper that hangs up mid-response is normal and not worth logging.
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(body.as_bytes());
    let _ = stream.flush();
}

fn main() {
    let cache = Arc::new(
        ShardedMultiLayerCache::try_with_options(
            CacheOptions::new(RESIDENT * VALUE_BYTES, 0, 0),
            SHARDS,
        )
        .expect("cache"),
    );
    cache.start().expect("start cache");

    // Something for the dashboard to show.
    let stop = Arc::new(AtomicBool::new(false));
    for worker in 0..3 {
        let cache = Arc::clone(&cache);
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut state = 0x2545_F491_4F6C_DD1D ^ ((worker as u64) << 32);
            while !stop.load(Ordering::Relaxed) {
                for _ in 0..2_000 {
                    let key = CacheKey::string(0, &format!("m-{:06}", skewed(&mut state)));
                    if cache.get(&key).expect("get").is_none() {
                        cache.put(key, vec![b'v'; VALUE_BYTES]).expect("put");
                    }
                }
                let base = skewed(&mut state);
                for keys in [
                    batch_keys("small", worker, base, SMALL_BATCH),
                    batch_keys("large", worker, base, LARGE_BATCH),
                ] {
                    let values = cache.get_batch(&keys).expect("batch get");
                    let missing = keys
                        .into_iter()
                        .zip(values)
                        .filter_map(|(key, value)| value.is_none().then_some(key))
                        .collect::<Vec<_>>();
                    for key in missing {
                        cache.put(key, vec![b'b'; VALUE_BYTES]).expect("batch fill");
                    }
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        });
    }

    let listener = TcpListener::bind(("127.0.0.1", PORT)).unwrap_or_else(|err| {
        panic!("cannot bind 127.0.0.1:{PORT}: {err}. Is another exporter running?")
    });
    println!("matrixcache metrics on http://127.0.0.1:{PORT}/metrics");
    println!("three workers are driving skewed single-key and sharded batch workloads");

    let mut scrape_buffer = PrometheusScrapeBuffer::new();
    let mut line = String::with_capacity(256);

    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        line.clear();
        let peeked = stream.try_clone().expect("clone stream");
        if BufReader::new(peeked).read_line(&mut line).is_err() {
            continue;
        }
        // "GET /metrics HTTP/1.1" -- the middle field is all that matters.
        match line.split_whitespace().nth(1).unwrap_or("/") {
            "/metrics" => {
                let body = scrape_buffer.render(&cache.stats(), &[("cache", "example")]);
                respond(stream, "200 OK", "text/plain; version=0.0.4", body);
            }
            "/healthz" => respond(stream, "200 OK", "text/plain", "ok\n"),
            "/" => respond(
                stream,
                "200 OK",
                "text/plain",
                "matrixcache exporter\n\n  /metrics   Prometheus exposition\n  \
                 /healthz   liveness\n",
            ),
            _ => respond(stream, "404 Not Found", "text/plain", "not found\n"),
        }
    }
}
