// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI
//
// Prometheus exposition for the whole of `CacheStats`.
//
// This file is generated, and it is generated rather than written because
// `CacheStats` has over a hundred fields: a hand-maintained exporter drifts the
// first time one is added, and a metric that has silently stopped being
// exported looks exactly like a metric whose value is zero.
//
// Regenerate with `tools/gen_metrics.py` after changing `CacheStats`.

use std::fmt::Write as _;

/// Renders a snapshot in Prometheus text exposition format (version 0.0.4).
///
/// `labels` are appended to every series, so several caches in one process
/// can be told apart -- pass something like `&[("cache", "sessions")]`.
/// Label values are escaped; names are assumed well-formed.
///
/// The seven latency families are exported as real histograms rather than as
/// loose counters, so `histogram_quantile` works on them. Their buckets are
/// cumulative, as the format requires.
pub fn prometheus_text(stats: &CacheStats, labels: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(8 * 1024);
    let tags = render_labels(labels);

    metric(&mut out, "matrixcache_memory_hits", "Reads served from the memory tier", "counter", &tags, stats.memory_hits);
    metric(&mut out, "matrixcache_disk_hits", "Reads served from the SSD tier", "counter", &tags, stats.disk_hits);
    metric(&mut out, "matrixcache_misses", "Reads that found nothing in any tier", "counter", &tags, stats.misses);
    metric(&mut out, "matrixcache_puts", "Values written into the cache", "counter", &tags, stats.puts);
    metric(&mut out, "matrixcache_invalidations", "Invalidations", "counter", &tags, stats.invalidations);
    metric(&mut out, "matrixcache_memory_evictions", "Memory evictions", "counter", &tags, stats.memory_evictions);
    metric(&mut out, "matrixcache_pmem_hits", "Reads served from the persistent-memory tier", "counter", &tags, stats.pmem_hits);
    metric(&mut out, "matrixcache_pmem_fills", "Pmem fills", "counter", &tags, stats.pmem_fills);
    metric(&mut out, "matrixcache_pmem_evictions", "Pmem evictions", "counter", &tags, stats.pmem_evictions);
    metric(&mut out, "matrixcache_pmem_admission_accepted", "Pmem admission accepted", "counter", &tags, stats.pmem_admission_accepted);
    metric(&mut out, "matrixcache_pmem_admission_rejected", "Pmem admission rejected", "counter", &tags, stats.pmem_admission_rejected);
    metric(&mut out, "matrixcache_pmem_eviction_capacity", "Pmem eviction capacity", "counter", &tags, stats.pmem_eviction_capacity);
    metric(&mut out, "matrixcache_pmem_eviction_pinned_skips", "Pmem eviction pinned skips", "counter", &tags, stats.pmem_eviction_pinned_skips);
    metric(&mut out, "matrixcache_memory_admission_accepted", "Memory admission accepted", "counter", &tags, stats.memory_admission_accepted);
    metric(&mut out, "matrixcache_memory_admission_rejected", "Memory admission rejected", "counter", &tags, stats.memory_admission_rejected);
    metric(&mut out, "matrixcache_memory_fills", "Memory fills", "counter", &tags, stats.memory_fills);
    metric(&mut out, "matrixcache_disk_fills", "Disk fills", "counter", &tags, stats.disk_fills);
    metric(&mut out, "matrixcache_ssd_admission_accepted", "Ssd admission accepted", "counter", &tags, stats.ssd_admission_accepted);
    metric(&mut out, "matrixcache_ssd_admission_rejected", "Ssd admission rejected", "counter", &tags, stats.ssd_admission_rejected);
    metric(&mut out, "matrixcache_ssd_evictions", "Ssd evictions", "counter", &tags, stats.ssd_evictions);
    metric(&mut out, "matrixcache_ssd_eviction_capacity", "Ssd eviction capacity", "counter", &tags, stats.ssd_eviction_capacity);
    metric(&mut out, "matrixcache_ssd_eviction_pinned_skips", "Ssd eviction pinned skips", "counter", &tags, stats.ssd_eviction_pinned_skips);
    metric(&mut out, "matrixcache_ssd_oversize_rejections", "Ssd oversize rejections", "counter", &tags, stats.ssd_oversize_rejections);
    metric(&mut out, "matrixcache_ssd_bytes_written", "Bytes written to the SSD tier, including reclaim and recovery", "counter", &tags, stats.ssd_bytes_written);
    metric(&mut out, "matrixcache_ssd_write_budget_rejections", "Admissions refused to stay inside the SSD write budget", "counter", &tags, stats.ssd_write_budget_rejections);
    metric(&mut out, "matrixcache_ssd_write_budget_share", "Share of keys the SSD write budget admits, out of 10000", "gauge", &tags, stats.ssd_write_budget_share);
    metric(&mut out, "matrixcache_ssd_write_budget_observed_bytes_per_sec", "Bytes per second the SSD write budget measured over its last window", "gauge", &tags, stats.ssd_write_budget_observed_bytes_per_sec);
    metric(&mut out, "matrixcache_ssd_write_budget_target_bytes_per_sec", "Bytes per second the SSD write budget is aiming at, zero when uncapped", "gauge", &tags, stats.ssd_write_budget_target_bytes_per_sec);
    metric(&mut out, "matrixcache_stale_tier_copies_dropped", "Stale tier copies dropped", "counter", &tags, stats.stale_tier_copies_dropped);
    metric(&mut out, "matrixcache_expired_demotions_skipped", "Demotions declined because the entry had already expired", "counter", &tags, stats.expired_demotions_skipped);
    metric(&mut out, "matrixcache_expired_reads", "Expired reads", "counter", &tags, stats.expired_reads);
    metric(&mut out, "matrixcache_expired_removals", "Expired removals", "counter", &tags, stats.expired_removals);
    metric(&mut out, "matrixcache_eviction_expired", "Eviction expired", "counter", &tags, stats.eviction_expired);
    metric(&mut out, "matrixcache_expired_delete_failures", "Expired delete failures", "counter", &tags, stats.expired_delete_failures);
    metric(&mut out, "matrixcache_ssd_write_through_admissions", "Ssd write through admissions", "counter", &tags, stats.ssd_write_through_admissions);
    metric(&mut out, "matrixcache_hotness_promotions", "Entries that crossed the hotness threshold", "counter", &tags, stats.hotness_promotions);
    metric(&mut out, "matrixcache_access_order_refreshes", "Access order refreshes", "counter", &tags, stats.access_order_refreshes);
    metric(&mut out, "matrixcache_refill_failures", "Promotions into a faster tier that did not fit", "counter", &tags, stats.refill_failures);
    metric(&mut out, "matrixcache_eviction_capacity", "Eviction capacity", "counter", &tags, stats.eviction_capacity);
    metric(&mut out, "matrixcache_eviction_oversize", "Eviction oversize", "counter", &tags, stats.eviction_oversize);
    metric(&mut out, "matrixcache_eviction_cold", "Eviction cold", "counter", &tags, stats.eviction_cold);
    metric(&mut out, "matrixcache_eviction_low_hit", "Eviction low hit", "counter", &tags, stats.eviction_low_hit);
    metric(&mut out, "matrixcache_eviction_stale", "Eviction stale", "counter", &tags, stats.eviction_stale);
    metric(&mut out, "matrixcache_pinned_entries", "Entries currently pinned against eviction", "gauge", &tags, stats.pinned_entries);
    metric(&mut out, "matrixcache_pinned_bytes", "Bytes held by pinned entries", "gauge", &tags, stats.pinned_bytes);
    metric(&mut out, "matrixcache_pin_operations", "Pin operations", "counter", &tags, stats.pin_operations);
    metric(&mut out, "matrixcache_unpin_operations", "Unpin operations", "counter", &tags, stats.unpin_operations);
    metric(&mut out, "matrixcache_insert_pinned_operations", "Insert pinned operations", "counter", &tags, stats.insert_pinned_operations);
    metric(&mut out, "matrixcache_eviction_pinned_skips", "Eviction pinned skips", "counter", &tags, stats.eviction_pinned_skips);
    metric(&mut out, "matrixcache_zero_copy_handle_hits", "Zero copy handle hits", "counter", &tags, stats.zero_copy_handle_hits);
    metric(&mut out, "matrixcache_zero_copy_handle_misses", "Zero copy handle misses", "counter", &tags, stats.zero_copy_handle_misses);
    metric(&mut out, "matrixcache_async_writeback_enqueued", "Async writeback enqueued", "counter", &tags, stats.async_writeback_enqueued);
    metric(&mut out, "matrixcache_async_writeback_drained", "Async writeback drained", "counter", &tags, stats.async_writeback_drained);
    metric(&mut out, "matrixcache_async_writeback_backpressure_rejections", "Async writeback backpressure rejections", "counter", &tags, stats.async_writeback_backpressure_rejections);
    metric(&mut out, "matrixcache_writeback_backpressure_events", "Writeback backpressure events", "counter", &tags, stats.writeback_backpressure_events);
    metric(&mut out, "matrixcache_async_writeback_queue_depth", "Write-back jobs waiting", "gauge", &tags, stats.async_writeback_queue_depth);
    metric(&mut out, "matrixcache_async_writeback_queue_bytes", "Bytes waiting in the write-back queue", "gauge", &tags, stats.async_writeback_queue_bytes);
    metric(&mut out, "matrixcache_async_writeback_max_queue_depth", "Async writeback max queue depth", "gauge", &tags, stats.async_writeback_max_queue_depth);
    metric(&mut out, "matrixcache_async_writeback_max_queue_bytes", "Async writeback max queue bytes", "gauge", &tags, stats.async_writeback_max_queue_bytes);
    metric(&mut out, "matrixcache_sharded_batch_fanout_operations", "Sharded batch fanout operations", "counter", &tags, stats.sharded_batch_fanout_operations);
    metric(&mut out, "matrixcache_sharded_batch_local_operations", "Sharded batch local operations", "counter", &tags, stats.sharded_batch_local_operations);
    metric(&mut out, "matrixcache_sharded_batch_fanout_shards", "Sharded batch fanout shards", "counter", &tags, stats.sharded_batch_fanout_shards);
    metric(&mut out, "matrixcache_eviction_sampled_groups", "Eviction sampled groups", "counter", &tags, stats.eviction_sampled_groups);
    metric(&mut out, "matrixcache_memory_slot_evictions", "Memory slot evictions", "counter", &tags, stats.memory_slot_evictions);
    metric(&mut out, "matrixcache_ssd_slot_evictions", "Ssd slot evictions", "counter", &tags, stats.ssd_slot_evictions);
    metric(&mut out, "matrixcache_ssd_eviction_cold", "Ssd eviction cold", "counter", &tags, stats.ssd_eviction_cold);
    metric(&mut out, "matrixcache_ssd_eviction_low_hit", "Ssd eviction low hit", "counter", &tags, stats.ssd_eviction_low_hit);
    metric(&mut out, "matrixcache_ssd_eviction_stale", "Ssd eviction stale", "counter", &tags, stats.ssd_eviction_stale);
    metric(&mut out, "matrixcache_compressed_puts", "Compressed puts", "counter", &tags, stats.compressed_puts);
    metric(&mut out, "matrixcache_compressed_hits", "Compressed hits", "counter", &tags, stats.compressed_hits);
    metric(&mut out, "matrixcache_compression_bytes_saved", "Bytes not written because a value compressed", "counter", &tags, stats.compression_bytes_saved);
    metric(&mut out, "matrixcache_memory_bytes", "Bytes resident in the memory tier", "gauge", &tags, stats.memory_bytes);
    metric(&mut out, "matrixcache_pmem_bytes", "Bytes resident in the persistent-memory tier", "gauge", &tags, stats.pmem_bytes);
    metric(&mut out, "matrixcache_disk_bytes", "Bytes resident on SSD", "gauge", &tags, stats.disk_bytes);

    // get latency
    let _ = writeln!(out, "# HELP matrixcache_get_latency_seconds get latency");
    let _ = writeln!(out, "# TYPE matrixcache_get_latency_seconds histogram");
    {
        let mut cumulative = 0_u64;
        cumulative = cumulative.saturating_add(stats.get_latency_le_10us);
        bucket(&mut out, "matrixcache_get_latency_seconds", &tags, "1e-05", cumulative);
        cumulative = cumulative.saturating_add(stats.get_latency_le_100us);
        bucket(&mut out, "matrixcache_get_latency_seconds", &tags, "0.0001", cumulative);
        cumulative = cumulative.saturating_add(stats.get_latency_le_1ms);
        bucket(&mut out, "matrixcache_get_latency_seconds", &tags, "0.001", cumulative);
        cumulative = cumulative.saturating_add(stats.get_latency_le_10ms);
        bucket(&mut out, "matrixcache_get_latency_seconds", &tags, "0.01", cumulative);
        cumulative = cumulative.saturating_add(stats.get_latency_gt_10ms);
        bucket(&mut out, "matrixcache_get_latency_seconds", &tags, "+Inf", cumulative);
        let _ = writeln!(
            out,
            "matrixcache_get_latency_seconds_sum{tags} {:.6}",
            stats.get_latency_total_micros as f64 / 1_000_000.0
        );
        let _ = writeln!(out, "matrixcache_get_latency_seconds_count{tags} {cumulative}");
    }
    metric_f64(&mut out, "matrixcache_get_latency_avg_seconds", "get average latency", "gauge", &tags, average_seconds(stats.get_latency_total_micros, stats.get_latency_samples));
    metric(&mut out, "matrixcache_get_latency_max_seconds", "get peak latency", "gauge", &tags, stats.get_latency_max_micros);
    metric_f64(&mut out, "matrixcache_get_latency_p50_seconds", "get p50 latency", "gauge", &tags, percentile_seconds(stats.get_latency_samples, stats.get_latency_le_10us, stats.get_latency_le_100us, stats.get_latency_le_1ms, stats.get_latency_le_10ms, stats.get_latency_gt_10ms, stats.get_latency_max_micros, 50));
    metric_f64(&mut out, "matrixcache_get_latency_p95_seconds", "get p95 latency", "gauge", &tags, percentile_seconds(stats.get_latency_samples, stats.get_latency_le_10us, stats.get_latency_le_100us, stats.get_latency_le_1ms, stats.get_latency_le_10ms, stats.get_latency_gt_10ms, stats.get_latency_max_micros, 95));

    // put latency
    let _ = writeln!(out, "# HELP matrixcache_put_latency_seconds put latency");
    let _ = writeln!(out, "# TYPE matrixcache_put_latency_seconds histogram");
    {
        let mut cumulative = 0_u64;
        cumulative = cumulative.saturating_add(stats.put_latency_le_10us);
        bucket(&mut out, "matrixcache_put_latency_seconds", &tags, "1e-05", cumulative);
        cumulative = cumulative.saturating_add(stats.put_latency_le_100us);
        bucket(&mut out, "matrixcache_put_latency_seconds", &tags, "0.0001", cumulative);
        cumulative = cumulative.saturating_add(stats.put_latency_le_1ms);
        bucket(&mut out, "matrixcache_put_latency_seconds", &tags, "0.001", cumulative);
        cumulative = cumulative.saturating_add(stats.put_latency_le_10ms);
        bucket(&mut out, "matrixcache_put_latency_seconds", &tags, "0.01", cumulative);
        cumulative = cumulative.saturating_add(stats.put_latency_gt_10ms);
        bucket(&mut out, "matrixcache_put_latency_seconds", &tags, "+Inf", cumulative);
        let _ = writeln!(
            out,
            "matrixcache_put_latency_seconds_sum{tags} {:.6}",
            stats.put_latency_total_micros as f64 / 1_000_000.0
        );
        let _ = writeln!(out, "matrixcache_put_latency_seconds_count{tags} {cumulative}");
    }
    metric_f64(&mut out, "matrixcache_put_latency_avg_seconds", "put average latency", "gauge", &tags, average_seconds(stats.put_latency_total_micros, stats.put_latency_samples));
    metric(&mut out, "matrixcache_put_latency_max_seconds", "put peak latency", "gauge", &tags, stats.put_latency_max_micros);
    metric_f64(&mut out, "matrixcache_put_latency_p50_seconds", "put p50 latency", "gauge", &tags, percentile_seconds(stats.put_latency_samples, stats.put_latency_le_10us, stats.put_latency_le_100us, stats.put_latency_le_1ms, stats.put_latency_le_10ms, stats.put_latency_gt_10ms, stats.put_latency_max_micros, 50));
    metric_f64(&mut out, "matrixcache_put_latency_p95_seconds", "put p95 latency", "gauge", &tags, percentile_seconds(stats.put_latency_samples, stats.put_latency_le_10us, stats.put_latency_le_100us, stats.put_latency_le_1ms, stats.put_latency_le_10ms, stats.put_latency_gt_10ms, stats.put_latency_max_micros, 95));

    // read through latency
    let _ = writeln!(out, "# HELP matrixcache_read_through_latency_seconds read through latency");
    let _ = writeln!(out, "# TYPE matrixcache_read_through_latency_seconds histogram");
    {
        let mut cumulative = 0_u64;
        cumulative = cumulative.saturating_add(stats.read_through_latency_le_10us);
        bucket(&mut out, "matrixcache_read_through_latency_seconds", &tags, "1e-05", cumulative);
        cumulative = cumulative.saturating_add(stats.read_through_latency_le_100us);
        bucket(&mut out, "matrixcache_read_through_latency_seconds", &tags, "0.0001", cumulative);
        cumulative = cumulative.saturating_add(stats.read_through_latency_le_1ms);
        bucket(&mut out, "matrixcache_read_through_latency_seconds", &tags, "0.001", cumulative);
        cumulative = cumulative.saturating_add(stats.read_through_latency_le_10ms);
        bucket(&mut out, "matrixcache_read_through_latency_seconds", &tags, "0.01", cumulative);
        cumulative = cumulative.saturating_add(stats.read_through_latency_gt_10ms);
        bucket(&mut out, "matrixcache_read_through_latency_seconds", &tags, "+Inf", cumulative);
        let _ = writeln!(
            out,
            "matrixcache_read_through_latency_seconds_sum{tags} {:.6}",
            stats.read_through_latency_total_micros as f64 / 1_000_000.0
        );
        let _ = writeln!(out, "matrixcache_read_through_latency_seconds_count{tags} {cumulative}");
    }
    metric_f64(&mut out, "matrixcache_read_through_latency_avg_seconds", "read through average latency", "gauge", &tags, average_seconds(stats.read_through_latency_total_micros, stats.read_through_latency_samples));
    metric_f64(&mut out, "matrixcache_read_through_latency_p50_seconds", "read through p50 latency", "gauge", &tags, percentile_seconds(stats.read_through_latency_samples, stats.read_through_latency_le_10us, stats.read_through_latency_le_100us, stats.read_through_latency_le_1ms, stats.read_through_latency_le_10ms, stats.read_through_latency_gt_10ms, 0, 50));
    metric_f64(&mut out, "matrixcache_read_through_latency_p95_seconds", "read through p95 latency", "gauge", &tags, percentile_seconds(stats.read_through_latency_samples, stats.read_through_latency_le_10us, stats.read_through_latency_le_100us, stats.read_through_latency_le_1ms, stats.read_through_latency_le_10ms, stats.read_through_latency_gt_10ms, 0, 95));

    // refill latency
    let _ = writeln!(out, "# HELP matrixcache_refill_latency_seconds refill latency");
    let _ = writeln!(out, "# TYPE matrixcache_refill_latency_seconds histogram");
    {
        let mut cumulative = 0_u64;
        cumulative = cumulative.saturating_add(stats.refill_latency_le_10us);
        bucket(&mut out, "matrixcache_refill_latency_seconds", &tags, "1e-05", cumulative);
        cumulative = cumulative.saturating_add(stats.refill_latency_le_100us);
        bucket(&mut out, "matrixcache_refill_latency_seconds", &tags, "0.0001", cumulative);
        cumulative = cumulative.saturating_add(stats.refill_latency_le_1ms);
        bucket(&mut out, "matrixcache_refill_latency_seconds", &tags, "0.001", cumulative);
        cumulative = cumulative.saturating_add(stats.refill_latency_le_10ms);
        bucket(&mut out, "matrixcache_refill_latency_seconds", &tags, "0.01", cumulative);
        cumulative = cumulative.saturating_add(stats.refill_latency_gt_10ms);
        bucket(&mut out, "matrixcache_refill_latency_seconds", &tags, "+Inf", cumulative);
        let _ = writeln!(
            out,
            "matrixcache_refill_latency_seconds_sum{tags} {:.6}",
            stats.refill_latency_total_micros as f64 / 1_000_000.0
        );
        let _ = writeln!(out, "matrixcache_refill_latency_seconds_count{tags} {cumulative}");
    }
    metric_f64(&mut out, "matrixcache_refill_latency_avg_seconds", "refill average latency", "gauge", &tags, average_seconds(stats.refill_latency_total_micros, stats.refill_latency_samples));
    metric_f64(&mut out, "matrixcache_refill_latency_p50_seconds", "refill p50 latency", "gauge", &tags, percentile_seconds(stats.refill_latency_samples, stats.refill_latency_le_10us, stats.refill_latency_le_100us, stats.refill_latency_le_1ms, stats.refill_latency_le_10ms, stats.refill_latency_gt_10ms, 0, 50));
    metric_f64(&mut out, "matrixcache_refill_latency_p95_seconds", "refill p95 latency", "gauge", &tags, percentile_seconds(stats.refill_latency_samples, stats.refill_latency_le_10us, stats.refill_latency_le_100us, stats.refill_latency_le_1ms, stats.refill_latency_le_10ms, stats.refill_latency_gt_10ms, 0, 95));

    // writeback latency
    let _ = writeln!(out, "# HELP matrixcache_writeback_latency_seconds writeback latency");
    let _ = writeln!(out, "# TYPE matrixcache_writeback_latency_seconds histogram");
    {
        let mut cumulative = 0_u64;
        cumulative = cumulative.saturating_add(stats.writeback_latency_le_10us);
        bucket(&mut out, "matrixcache_writeback_latency_seconds", &tags, "1e-05", cumulative);
        cumulative = cumulative.saturating_add(stats.writeback_latency_le_100us);
        bucket(&mut out, "matrixcache_writeback_latency_seconds", &tags, "0.0001", cumulative);
        cumulative = cumulative.saturating_add(stats.writeback_latency_le_1ms);
        bucket(&mut out, "matrixcache_writeback_latency_seconds", &tags, "0.001", cumulative);
        cumulative = cumulative.saturating_add(stats.writeback_latency_le_10ms);
        bucket(&mut out, "matrixcache_writeback_latency_seconds", &tags, "0.01", cumulative);
        cumulative = cumulative.saturating_add(stats.writeback_latency_gt_10ms);
        bucket(&mut out, "matrixcache_writeback_latency_seconds", &tags, "+Inf", cumulative);
        let _ = writeln!(
            out,
            "matrixcache_writeback_latency_seconds_sum{tags} {:.6}",
            stats.writeback_latency_total_micros as f64 / 1_000_000.0
        );
        let _ = writeln!(out, "matrixcache_writeback_latency_seconds_count{tags} {cumulative}");
    }
    metric_f64(&mut out, "matrixcache_writeback_latency_avg_seconds", "writeback average latency", "gauge", &tags, average_seconds(stats.writeback_latency_total_micros, stats.writeback_latency_samples));
    metric_f64(&mut out, "matrixcache_writeback_latency_p50_seconds", "writeback p50 latency", "gauge", &tags, percentile_seconds(stats.writeback_latency_samples, stats.writeback_latency_le_10us, stats.writeback_latency_le_100us, stats.writeback_latency_le_1ms, stats.writeback_latency_le_10ms, stats.writeback_latency_gt_10ms, 0, 50));
    metric_f64(&mut out, "matrixcache_writeback_latency_p95_seconds", "writeback p95 latency", "gauge", &tags, percentile_seconds(stats.writeback_latency_samples, stats.writeback_latency_le_10us, stats.writeback_latency_le_100us, stats.writeback_latency_le_1ms, stats.writeback_latency_le_10ms, stats.writeback_latency_gt_10ms, 0, 95));

    // eviction latency
    let _ = writeln!(out, "# HELP matrixcache_eviction_latency_seconds eviction latency");
    let _ = writeln!(out, "# TYPE matrixcache_eviction_latency_seconds histogram");
    {
        let mut cumulative = 0_u64;
        cumulative = cumulative.saturating_add(stats.eviction_latency_le_10us);
        bucket(&mut out, "matrixcache_eviction_latency_seconds", &tags, "1e-05", cumulative);
        cumulative = cumulative.saturating_add(stats.eviction_latency_le_100us);
        bucket(&mut out, "matrixcache_eviction_latency_seconds", &tags, "0.0001", cumulative);
        cumulative = cumulative.saturating_add(stats.eviction_latency_le_1ms);
        bucket(&mut out, "matrixcache_eviction_latency_seconds", &tags, "0.001", cumulative);
        cumulative = cumulative.saturating_add(stats.eviction_latency_le_10ms);
        bucket(&mut out, "matrixcache_eviction_latency_seconds", &tags, "0.01", cumulative);
        cumulative = cumulative.saturating_add(stats.eviction_latency_gt_10ms);
        bucket(&mut out, "matrixcache_eviction_latency_seconds", &tags, "+Inf", cumulative);
        let _ = writeln!(
            out,
            "matrixcache_eviction_latency_seconds_sum{tags} {:.6}",
            stats.eviction_latency_total_micros as f64 / 1_000_000.0
        );
        let _ = writeln!(out, "matrixcache_eviction_latency_seconds_count{tags} {cumulative}");
    }
    metric_f64(&mut out, "matrixcache_eviction_latency_avg_seconds", "eviction average latency", "gauge", &tags, average_seconds(stats.eviction_latency_total_micros, stats.eviction_latency_samples));
    metric_f64(&mut out, "matrixcache_eviction_latency_p50_seconds", "eviction p50 latency", "gauge", &tags, percentile_seconds(stats.eviction_latency_samples, stats.eviction_latency_le_10us, stats.eviction_latency_le_100us, stats.eviction_latency_le_1ms, stats.eviction_latency_le_10ms, stats.eviction_latency_gt_10ms, 0, 50));
    metric_f64(&mut out, "matrixcache_eviction_latency_p95_seconds", "eviction p95 latency", "gauge", &tags, percentile_seconds(stats.eviction_latency_samples, stats.eviction_latency_le_10us, stats.eviction_latency_le_100us, stats.eviction_latency_le_1ms, stats.eviction_latency_le_10ms, stats.eviction_latency_gt_10ms, 0, 95));

    // compaction latency
    let _ = writeln!(out, "# HELP matrixcache_compaction_latency_seconds compaction latency");
    let _ = writeln!(out, "# TYPE matrixcache_compaction_latency_seconds histogram");
    {
        let mut cumulative = 0_u64;
        cumulative = cumulative.saturating_add(stats.compaction_latency_le_10us);
        bucket(&mut out, "matrixcache_compaction_latency_seconds", &tags, "1e-05", cumulative);
        cumulative = cumulative.saturating_add(stats.compaction_latency_le_100us);
        bucket(&mut out, "matrixcache_compaction_latency_seconds", &tags, "0.0001", cumulative);
        cumulative = cumulative.saturating_add(stats.compaction_latency_le_1ms);
        bucket(&mut out, "matrixcache_compaction_latency_seconds", &tags, "0.001", cumulative);
        cumulative = cumulative.saturating_add(stats.compaction_latency_le_10ms);
        bucket(&mut out, "matrixcache_compaction_latency_seconds", &tags, "0.01", cumulative);
        cumulative = cumulative.saturating_add(stats.compaction_latency_gt_10ms);
        bucket(&mut out, "matrixcache_compaction_latency_seconds", &tags, "+Inf", cumulative);
        let _ = writeln!(
            out,
            "matrixcache_compaction_latency_seconds_sum{tags} {:.6}",
            stats.compaction_latency_total_micros as f64 / 1_000_000.0
        );
        let _ = writeln!(out, "matrixcache_compaction_latency_seconds_count{tags} {cumulative}");
    }
    metric_f64(&mut out, "matrixcache_compaction_latency_avg_seconds", "compaction average latency", "gauge", &tags, average_seconds(stats.compaction_latency_total_micros, stats.compaction_latency_samples));
    metric_f64(&mut out, "matrixcache_compaction_latency_p50_seconds", "compaction p50 latency", "gauge", &tags, percentile_seconds(stats.compaction_latency_samples, stats.compaction_latency_le_10us, stats.compaction_latency_le_100us, stats.compaction_latency_le_1ms, stats.compaction_latency_le_10ms, stats.compaction_latency_gt_10ms, 0, 50));
    metric_f64(&mut out, "matrixcache_compaction_latency_p95_seconds", "compaction p95 latency", "gauge", &tags, percentile_seconds(stats.compaction_latency_samples, stats.compaction_latency_le_10us, stats.compaction_latency_le_100us, stats.compaction_latency_le_1ms, stats.compaction_latency_le_10ms, stats.compaction_latency_gt_10ms, 0, 95));

    out
}

fn render_labels(labels: &[(&str, &str)]) -> String {
    if labels.is_empty() {
        return String::new();
    }
    let mut rendered = String::from("{");
    for (index, (name, value)) in labels.iter().enumerate() {
        if index > 0 {
            rendered.push(',');
        }
        let _ = write!(rendered, "{}=\"{}\"", name, escape(value));
    }
    rendered.push('}');
    rendered
}

/// Backslash, double quote and newline are the three characters the text
/// format reserves inside a label value.
fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn metric(out: &mut String, name: &str, help: &str, kind: &str, tags: &str, value: u64) {
    let _ = writeln!(out, "# HELP {name} {help}");
    let _ = writeln!(out, "# TYPE {name} {kind}");
    let _ = writeln!(out, "{name}{tags} {value}");
}

fn metric_f64(out: &mut String, name: &str, help: &str, kind: &str, tags: &str, value: f64) {
    let _ = writeln!(out, "# HELP {name} {help}");
    let _ = writeln!(out, "# TYPE {name} {kind}");
    let _ = writeln!(out, "{name}{tags} {value:.6}");
}

fn average_seconds(total_micros: u64, samples: u64) -> f64 {
    if samples == 0 {
        0.0
    } else {
        total_micros as f64 / samples as f64 / 1_000_000.0
    }
}

fn percentile_seconds(
    samples: u64,
    le_10us: u64,
    le_100us: u64,
    le_1ms: u64,
    le_10ms: u64,
    gt_10ms: u64,
    max_micros: u64,
    percentile: u64,
) -> f64 {
    if samples == 0 {
        return 0.0;
    }
    let rank = samples.saturating_mul(percentile).saturating_add(99) / 100;
    let mut cumulative = le_10us;
    if rank <= cumulative {
        return 0.000010;
    }
    cumulative = cumulative.saturating_add(le_100us);
    if rank <= cumulative {
        return 0.000100;
    }
    cumulative = cumulative.saturating_add(le_1ms);
    if rank <= cumulative {
        return 0.001000;
    }
    cumulative = cumulative.saturating_add(le_10ms);
    if rank <= cumulative {
        return 0.010000;
    }
    if gt_10ms > 0 {
        return max_micros.max(10_001) as f64 / 1_000_000.0;
    }
    max_micros as f64 / 1_000_000.0
}

fn bucket(out: &mut String, name: &str, tags: &str, le: &str, value: u64) {
    let separator = if tags.is_empty() { "" } else { "," };
    let inner = tags.trim_start_matches('{').trim_end_matches('}');
    let _ = writeln!(out, "{name}_bucket{{{inner}{separator}le=\"{le}\"}} {value}");
}
