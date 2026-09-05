#!/usr/bin/env python3
"""Generate a Prometheus exporter covering every field of CacheStats.

Written rather than hand-authored because there are 111 fields and a
hand-written exporter drifts the moment one is added: a metric that silently
stops being exported looks exactly like a metric whose value is zero.

Two things it does beyond dumping numbers:

  * the seven `*_latency_le_*` families become real Prometheus histograms --
    cumulative `_bucket{le=...}` series plus `_sum` and `_count` -- so Grafana
    can compute quantiles with `histogram_quantile`. Exported as flat counters
    they would be seven groups of unrelated numbers that no dashboard could
    turn into a latency figure.
  * every metric is classified `counter` or `gauge`, because `rate()` on a
    gauge is meaningless and Grafana will happily let you do it.
  * latency histograms also get direct p50/p95 gauges, so lightweight logs and
    dashboards can show percentile movement without repeating the bucket
    expression in every panel.
  * derived resident-efficiency gauges expose hit rate, resident bytes, and
    cumulative operation density without asking every dashboard to repeat the
    same denominator logic.
"""
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
STATS = ROOT / "src" / "core" / "concurrency_stats.rs"
OUTPUT = ROOT / "src" / "core" / "metrics.rs"


def cache_stats_fields(path):
    """The public field names of `CacheStats`, in declaration order.

    Read from the struct rather than from a list kept alongside it, because a
    list kept alongside it is a list that goes stale.
    """
    import re

    text = path.read_text()
    start = text.index("pub struct CacheStats {")
    end = text.index("\n}", start)
    body = text[start:end]
    return re.findall(r"^\s*pub ([a-z0-9_]+):", body, re.M)


FIELDS = cache_stats_fields(STATS)

# The seven latency families, and whether the struct also carries a running
# total for them (only get and put do).
LATENCY_FAMILIES = [
    ("get_latency", "get_latency_total_micros", "get_latency_max_micros"),
    ("put_latency", "put_latency_total_micros", "put_latency_max_micros"),
    ("read_through_latency", "read_through_latency_total_micros", "read_through_latency_max_micros"),
    ("refill_latency", "refill_latency_total_micros", "refill_latency_max_micros"),
    ("writeback_latency", "writeback_latency_total_micros", "writeback_latency_max_micros"),
    ("eviction_latency", "eviction_latency_total_micros", "eviction_latency_max_micros"),
    ("compaction_latency", "compaction_latency_total_micros", "compaction_latency_max_micros"),
    ("sharded_batch_latency", "sharded_batch_latency_total_micros", "sharded_batch_latency_max_micros"),
]
BUCKETS = [("le_10us", "1e-05"), ("le_100us", "0.0001"), ("le_1ms", "0.001"),
           ("le_10ms", "0.01"), ("gt_10ms", "+Inf")]

consumed = set()
for family, total, mx in LATENCY_FAMILIES:
    consumed.add(family + "_samples")
    for suffix, _ in BUCKETS:
        consumed.add("%s_%s" % (family, suffix))
    if total:
        consumed.add(total)
    if mx:
        consumed.add(mx)

# Duplicate spellings of the same quantity, kept in the struct for
# compatibility. Exporting both would double-count in any dashboard sum.
ALIASES = {
    "get_latency_count", "get_latency_total_us", "get_latency_max_us",
    "put_latency_count", "put_latency_total_us", "put_latency_max_us",
}

GAUGE_SUFFIXES = ("_bytes", "_entries", "_depth", "_max_queue_depth",
                  "_max_queue_bytes", "_max_micros", "_share",
                  "_bytes_per_sec")


def kind(field):
    if field.endswith(GAUGE_SUFFIXES):
        return "gauge"
    return "counter"


HELP = {
    "ssd_bytes_written": "Bytes written to the SSD tier, including reclaim and recovery",
    "ssd_write_budget_rejections": "Admissions refused to stay inside the SSD write budget",
    "ssd_write_budget_share": "Share of keys the SSD write budget admits, out of 10000",
    "expired_demotions_skipped":
        "Demotions declined because the entry had already expired",
    "ssd_write_budget_observed_bytes_per_sec":
        "Bytes per second the SSD write budget measured over its last window",
    "ssd_write_budget_target_bytes_per_sec":
        "Bytes per second the SSD write budget is aiming at, zero when uncapped",
    "memory_hits": "Reads served from the memory tier",
    "pmem_hits": "Reads served from the persistent-memory tier",
    "disk_hits": "Reads served from the SSD tier",
    "misses": "Reads that found nothing in any tier",
    "puts": "Values written into the cache",
    "memory_bytes": "Bytes resident in the memory tier",
    "pmem_bytes": "Bytes resident in the persistent-memory tier",
    "disk_bytes": "Bytes resident on SSD",
    "pinned_entries": "Entries currently pinned against eviction",
    "pinned_bytes": "Bytes held by pinned entries",
    "hotness_promotions": "Entries that crossed the hotness threshold",
    "refill_failures": "Promotions into a faster tier that did not fit",
    "async_writeback_queue_depth": "Write-back jobs waiting",
    "async_writeback_queue_bytes": "Bytes waiting in the write-back queue",
    "compression_bytes_saved": "Bytes not written because a value compressed",
}

scalars = [f for f in FIELDS if f not in consumed and f not in ALIASES]

out = []
out.append("// SPDX-License-Identifier: Apache-2.0")
out.append("// Copyright 2026 MatrixArkAI")
out.append("//")
out.append("// Prometheus exposition for the whole of `CacheStats`.")
out.append("//")
out.append("// This file is generated, and it is generated rather than written because")
out.append("// `CacheStats` has over a hundred fields: a hand-maintained exporter drifts the")
out.append("// first time one is added, and a metric that has silently stopped being")
out.append("// exported looks exactly like a metric whose value is zero.")
out.append("//")
out.append("// Regenerate with `tools/gen_metrics.py` after changing `CacheStats`.")
out.append("")
out.append("use std::fmt::Write as _;")
out.append("")
out.append("pub const PROMETHEUS_TEXT_CAPACITY_BYTES: usize = 32 * 1024;")
out.append("")
out.append("/// Renders a snapshot in Prometheus text exposition format (version 0.0.4).")
out.append("///")
out.append("/// `labels` are appended to every series, so several caches in one process")
out.append("/// can be told apart -- pass something like `&[(\"cache\", \"sessions\")]`.")
out.append("/// Label values are escaped; names are assumed well-formed.")
out.append("///")
out.append("/// The latency families are exported as real histograms rather than as")
out.append("/// loose counters, so `histogram_quantile` works on them. Their buckets are")
out.append("/// cumulative, as the format requires.")
out.append("pub fn prometheus_text(stats: &CacheStats, labels: &[(&str, &str)]) -> String {")
out.append("    let mut out = String::with_capacity(PROMETHEUS_TEXT_CAPACITY_BYTES);")
out.append("    prometheus_text_into(stats, labels, &mut out);")
out.append("    out")
out.append("}")
out.append("")
out.append("/// Renders a snapshot into a caller-owned Prometheus scrape buffer.")
out.append("///")
out.append("/// Reusing the same buffer avoids allocating a new 32 KiB scrape body for")
out.append("/// every pull while keeping the one-shot `prometheus_text` API available for")
out.append("/// tests and small tools.")
out.append("pub fn prometheus_text_into(stats: &CacheStats, labels: &[(&str, &str)], out: &mut String) {")
out.append("    out.clear();")
out.append("    if out.capacity() < PROMETHEUS_TEXT_CAPACITY_BYTES {")
out.append("        out.reserve(PROMETHEUS_TEXT_CAPACITY_BYTES - out.capacity());")
out.append("    }")
out.append("    let mut out = out;")
out.append("    let tags = render_labels(labels);")
out.append("")

for field in scalars:
    metric = "matrixcache_%s" % field
    help_text = HELP.get(field) or field.replace("_", " ").capitalize()
    out.append('    metric(&mut out, "%s", "%s", "%s", &tags, stats.%s);'
               % (metric, help_text, kind(field), field))

out.append("")
out.append("    let total_hits = stats")
out.append("        .memory_hits")
out.append("        .saturating_add(stats.pmem_hits)")
out.append("        .saturating_add(stats.disk_hits);")
out.append("    let total_requests = total_hits.saturating_add(stats.misses);")
out.append("    let total_ops = total_requests.saturating_add(stats.puts);")
out.append("    let resident_bytes = stats")
out.append("        .memory_bytes")
out.append("        .saturating_add(stats.pmem_bytes)")
out.append("        .saturating_add(stats.disk_bytes);")
out.append('    metric(&mut out, "matrixcache_total_hits", "Reads served from any cache tier", "counter", &tags, total_hits);')
out.append('    metric(&mut out, "matrixcache_total_requests", "Cache read requests across hits and misses", "counter", &tags, total_requests);')
out.append('    metric(&mut out, "matrixcache_total_operations", "Cache reads plus writes", "counter", &tags, total_ops);')
out.append('    metric(&mut out, "matrixcache_resident_bytes", "Bytes resident across memory, PMEM and SSD tiers", "gauge", &tags, resident_bytes);')
out.append('    metric_f64(&mut out, "matrixcache_hit_rate_ratio", "Read hit ratio across all cache tiers", "gauge", &tags, ratio(total_hits, total_requests));')
out.append('    metric_f64(&mut out, "matrixcache_memory_hit_share_ratio", "Share of hits served by memory", "gauge", &tags, ratio(stats.memory_hits, total_hits));')
out.append('    metric_f64(&mut out, "matrixcache_total_ops_per_resident_mib", "Cumulative cache operations per resident MiB", "gauge", &tags, per_resident_mib(total_ops, resident_bytes));')
out.append('    metric_f64(&mut out, "matrixcache_evictions_per_resident_mib", "Cumulative tier evictions per resident MiB", "gauge", &tags, per_resident_mib(stats.memory_evictions.saturating_add(stats.pmem_evictions).saturating_add(stats.ssd_evictions), resident_bytes));')
out.append('    metric_f64(&mut out, "matrixcache_writeback_queue_bytes_per_resident_mib", "Async writeback queue bytes per resident MiB", "gauge", &tags, per_resident_mib(stats.async_writeback_queue_bytes, resident_bytes));')
out.append("")
for family, total, mx in LATENCY_FAMILIES:
    metric = "matrixcache_%s_seconds" % family
    pretty = family.replace("_latency", "").replace("_", " ")
    out.append('    // %s latency' % pretty)
    out.append('    let _ = writeln!(out, "# HELP %s %s latency");' % (metric, pretty))
    out.append('    let _ = writeln!(out, "# TYPE %s histogram");' % metric)
    out.append("    {")
    out.append("        let mut cumulative = 0_u64;")
    for suffix, le in BUCKETS:
        out.append("        cumulative = cumulative.saturating_add(stats.%s_%s);" % (family, suffix))
        out.append('        bucket(&mut out, "%s", &tags, "%s", cumulative);' % (metric, le))
    if total:
        out.append('        let _ = writeln!(')
        out.append('            out,')
        out.append('            "%s_sum{tags} {:.6}",' % metric)
        out.append('            stats.%s as f64 / 1_000_000.0' % total)
        out.append("        );")
    else:
        out.append("        // No running total is kept for this family, so no _sum is")
        out.append("        // exported. Emitting a zero would read as \"all samples were")
        out.append("        // instantaneous\" rather than \"not measured\".")
    out.append('        let _ = writeln!(out, "%s_count{tags} {cumulative}");' % metric)
    out.append("    }")
    if total:
        out.append('    metric_f64(&mut out, "matrixcache_%s_avg_seconds", "%s average latency", "gauge", &tags, average_seconds(stats.%s, stats.%s_samples));'
                   % (family, pretty, total, family))
    if mx:
        out.append('    metric_f64(&mut out, "matrixcache_%s_max_seconds", "%s peak latency", "gauge", &tags, stats.%s as f64 / 1_000_000.0);'
                   % (family, pretty, mx))
    max_expr = "stats.%s" % mx if mx else "0"
    out.append('    metric_f64(&mut out, "matrixcache_%s_p50_seconds", "%s p50 latency", "gauge", &tags, percentile_seconds(stats.%s_samples, stats.%s_le_10us, stats.%s_le_100us, stats.%s_le_1ms, stats.%s_le_10ms, stats.%s_gt_10ms, %s, 50));'
               % (family, pretty, family, family, family, family, family, family, max_expr))
    out.append('    metric_f64(&mut out, "matrixcache_%s_p95_seconds", "%s p95 latency", "gauge", &tags, percentile_seconds(stats.%s_samples, stats.%s_le_10us, stats.%s_le_100us, stats.%s_le_1ms, stats.%s_le_10ms, stats.%s_gt_10ms, %s, 95));'
               % (family, pretty, family, family, family, family, family, family, max_expr))
    out.append('    metric_f64(&mut out, "matrixcache_%s_p99_seconds", "%s p99 latency", "gauge", &tags, percentile_seconds(stats.%s_samples, stats.%s_le_10us, stats.%s_le_100us, stats.%s_le_1ms, stats.%s_le_10ms, stats.%s_gt_10ms, %s, 99));'
               % (family, pretty, family, family, family, family, family, family, max_expr))
    out.append("")

out.append("}")
out.append("")
out.append("fn render_labels(labels: &[(&str, &str)]) -> String {")
out.append("    if labels.is_empty() {")
out.append('        return String::new();')
out.append("    }")
out.append("    let mut rendered = String::from(\"{\");")
out.append("    for (index, (name, value)) in labels.iter().enumerate() {")
out.append("        if index > 0 {")
out.append("            rendered.push(',');")
out.append("        }")
out.append('        let _ = write!(rendered, "{}=\\"{}\\"", name, escape(value));')
out.append("    }")
out.append("    rendered.push('}');")
out.append("    rendered")
out.append("}")
out.append("")
out.append("/// Backslash, double quote and newline are the three characters the text")
out.append("/// format reserves inside a label value.")
out.append("fn escape(value: &str) -> String {")
out.append("    value")
out.append('        .replace(\'\\\\\', "\\\\\\\\")')
out.append('        .replace(\'"\', "\\\\\\"")')
out.append('        .replace(\'\\n\', "\\\\n")')
out.append("}")
out.append("")
out.append("fn metric(out: &mut String, name: &str, help: &str, kind: &str, tags: &str, value: u64) {")
out.append('    let _ = writeln!(out, "# HELP {name} {help}");')
out.append('    let _ = writeln!(out, "# TYPE {name} {kind}");')
out.append('    let _ = writeln!(out, "{name}{tags} {value}");')
out.append("}")
out.append("")
out.append("fn metric_f64(out: &mut String, name: &str, help: &str, kind: &str, tags: &str, value: f64) {")
out.append('    let _ = writeln!(out, "# HELP {name} {help}");')
out.append('    let _ = writeln!(out, "# TYPE {name} {kind}");')
out.append('    let _ = writeln!(out, "{name}{tags} {value:.6}");')
out.append("}")
out.append("")
out.append("fn average_seconds(total_micros: u64, samples: u64) -> f64 {")
out.append("    if samples == 0 {")
out.append("        0.0")
out.append("    } else {")
out.append("        total_micros as f64 / samples as f64 / 1_000_000.0")
out.append("    }")
out.append("}")
out.append("")
out.append("fn ratio(numerator: u64, denominator: u64) -> f64 {")
out.append("    if denominator == 0 {")
out.append("        0.0")
out.append("    } else {")
out.append("        numerator as f64 / denominator as f64")
out.append("    }")
out.append("}")
out.append("")
out.append("fn per_resident_mib(value: u64, resident_bytes: u64) -> f64 {")
out.append("    if resident_bytes == 0 {")
out.append("        0.0")
out.append("    } else {")
out.append("        value as f64 / (resident_bytes as f64 / (1024.0 * 1024.0))")
out.append("    }")
out.append("}")
out.append("")
out.append("fn percentile_seconds(")
out.append("    samples: u64,")
out.append("    le_10us: u64,")
out.append("    le_100us: u64,")
out.append("    le_1ms: u64,")
out.append("    le_10ms: u64,")
out.append("    gt_10ms: u64,")
out.append("    max_micros: u64,")
out.append("    percentile: u64,")
out.append(") -> f64 {")
out.append("    if samples == 0 {")
out.append("        return 0.0;")
out.append("    }")
out.append("    let rank = samples.saturating_mul(percentile).saturating_add(99) / 100;")
out.append("    let mut cumulative = le_10us;")
out.append("    if rank <= cumulative {")
out.append("        return 0.000010;")
out.append("    }")
out.append("    cumulative = cumulative.saturating_add(le_100us);")
out.append("    if rank <= cumulative {")
out.append("        return 0.000100;")
out.append("    }")
out.append("    cumulative = cumulative.saturating_add(le_1ms);")
out.append("    if rank <= cumulative {")
out.append("        return 0.001000;")
out.append("    }")
out.append("    cumulative = cumulative.saturating_add(le_10ms);")
out.append("    if rank <= cumulative {")
out.append("        return 0.010000;")
out.append("    }")
out.append("    if gt_10ms > 0 {")
out.append("        return max_micros.max(10_001) as f64 / 1_000_000.0;")
out.append("    }")
out.append("    max_micros as f64 / 1_000_000.0")
out.append("}")
out.append("")
out.append("fn bucket(out: &mut String, name: &str, tags: &str, le: &str, value: u64) {")
out.append("    let separator = if tags.is_empty() { \"\" } else { \",\" };")
out.append("    let inner = tags.trim_start_matches('{').trim_end_matches('}');")
out.append('    let _ = writeln!(out, "{name}_bucket{{{inner}{separator}le=\\"{le}\\"}} {value}");')
out.append("}")

OUTPUT.write_text("\n".join(out) + "\n")
print("generated metrics.rs: %d scalar metrics + %d histograms"
      % (len(scalars), len(LATENCY_FAMILIES)))
print("skipped %d duplicate-spelling fields: %s" % (len(ALIASES), ", ".join(sorted(ALIASES))))
