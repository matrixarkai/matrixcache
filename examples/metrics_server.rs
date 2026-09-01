// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Serves the cache's statistics for Grafana to scrape.
//!
//! Exposes `/metrics` in Prometheus text format, covering every field of
//! `CacheStats` — 59 scalars and seven latency histograms — plus `/healthz`
//! and a plain-text index at `/`.
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

use matrixcache::{prometheus_text, CacheKey, CacheOptions, MultiLayerCache};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const PORT: u16 = 9184;
const VALUE_BYTES: usize = 256;
const KEY_SPACE: usize = 16_384;
const RESIDENT: usize = KEY_SPACE / 4;

/// Deterministic skew, so the dashboard shows a realistic hit rate rather than
/// the 100% a uniform loop over a resident set would produce.
fn skewed(state: &mut u64) -> usize {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1);
    let unit = ((*state >> 11) as f64) / ((1u64 << 53) as f64);
    ((unit * unit * unit * KEY_SPACE as f64) as usize).min(KEY_SPACE - 1)
}

fn respond(mut stream: TcpStream, status: &str, content_type: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    );
    // A scraper that hangs up mid-response is normal and not worth logging.
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn main() {
    let cache = Arc::new(
        MultiLayerCache::try_with_options(CacheOptions::new(RESIDENT * VALUE_BYTES, 0, 0))
            .expect("cache"),
    );

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
                std::thread::sleep(Duration::from_millis(5));
            }
        });
    }

    let listener = TcpListener::bind(("127.0.0.1", PORT)).unwrap_or_else(|err| {
        panic!("cannot bind 127.0.0.1:{PORT}: {err}. Is another exporter running?")
    });
    println!("matrixcache metrics on http://127.0.0.1:{PORT}/metrics");
    println!("three workers are driving a skewed workload so the series move");

    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let cache = Arc::clone(&cache);
        std::thread::spawn(move || {
            let mut line = String::new();
            let peeked = stream.try_clone().expect("clone stream");
            if BufReader::new(peeked).read_line(&mut line).is_err() {
                return;
            }
            // "GET /metrics HTTP/1.1" -- the middle field is all that matters.
            let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
            match path.as_str() {
                "/metrics" => {
                    let body = prometheus_text(&cache.stats(), &[("cache", "example")]);
                    respond(stream, "200 OK", "text/plain; version=0.0.4", &body);
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
        });
    }
}
