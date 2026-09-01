// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI
//
// Turning a statistics snapshot into things an operator can act on.
//
// `CacheStats` has over a hundred counters. That is the right amount to export
// and the wrong amount to read: the numbers that say a cache is unwell are
// ratios between counters, not any single counter, and knowing which ratios
// matter is knowledge that otherwise lives only in whoever tuned it last.
//
// Every finding here names the two numbers it compared and the threshold it
// applied, so a reader can disagree with the judgement rather than having to
// trust it.

/// A share at or below this is the budget refusing as much as it is able.
///
/// The budget never clamps to zero -- one that admits nothing could never
/// learn that the pressure had passed -- so "as tight as it goes" is a small
/// share, not none. Given a little room above the clamp so a budget one step
/// off the floor still reads as pinned.
const WRITE_BUDGET_FLOOR_SHARE: u64 = WRITE_BUDGET_SCALE / 1_000;

/// How much a finding matters.
///
/// Ordered, so a report can be sorted worst-first and a caller can filter with
/// a comparison rather than a match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CacheHealthSeverity {
    /// Worth knowing, but nothing is wrong.
    Info,
    /// The cache is working, but it is doing more work than it needs to.
    Warning,
    /// The cache is failing to do something it was asked to do.
    Critical,
}

impl CacheHealthSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }
}

/// One thing worth telling an operator about.
///
/// `observed` and `threshold` are the two numbers behind the judgement, in the
/// units named by `id`. They are carried so a dashboard can plot how close a
/// cache is to a threshold rather than only whether it has crossed one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheHealthFinding {
    /// Stable identifier, safe to alert on. Never reworded.
    pub id: String,
    pub severity: CacheHealthSeverity,
    /// Which part of the cache the finding is about.
    pub component: String,
    /// What is wrong, and what to do about it.
    pub message: String,
    pub observed: u64,
    pub threshold: u64,
}

/// What a snapshot says about a cache's health.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheHealthReport {
    /// True when nothing critical was found. Warnings do not clear it.
    pub healthy: bool,
    pub critical_count: usize,
    pub warning_count: usize,
    /// Worst first, then by id, so the order is stable between snapshots.
    pub findings: Vec<CacheHealthFinding>,
}

impl CacheHealthReport {
    /// The worst finding, or `None` when the cache is clean.
    pub fn worst(&self) -> Option<&CacheHealthFinding> {
        self.findings.first()
    }

    /// Findings at or above `severity`.
    pub fn at_least(
        &self,
        severity: CacheHealthSeverity,
    ) -> impl Iterator<Item = &CacheHealthFinding> + '_ {
        self.findings
            .iter()
            .filter(move |finding| finding.severity >= severity)
    }
}

// --- thresholds -----------------------------------------------------------
//
// Each is the point past which the cheaper explanation stops being plausible.

/// Below this many requests the ratios are noise, so only absolute faults are
/// reported. A cache that has served a handful of reads is not unhealthy, it is
/// new.
const HEALTH_MIN_REQUESTS: u64 = 1_000;

/// A hit rate under this, on a cache that is evicting, means the working set
/// does not fit and the tier is mostly paying costs without returning value.
const HEALTH_MIN_HIT_PERCENT: u64 = 50;

/// Reads that have to take the cache exclusively to reorder their entry.
/// Serving reads under a shared lock is the whole point of the refresh
/// distance; past this share of hits, that has stopped working.
const HEALTH_MAX_ESCALATION_PERCENT: u64 = 25;

/// Reads finding an entry already expired, as a share of all reads. Past this,
/// the time to live is expiring entries faster than the workload comes back
/// for them, so the cache is paying to store and then discard them.
const HEALTH_MAX_EXPIRED_READ_PERCENT: u64 = 20;

/// Admission turning away more than this share of writes means the values do
/// not fit the tier they are aimed at, not that the tier is merely full.
const HEALTH_MAX_ADMISSION_REJECT_PERCENT: u64 = 10;

fn percent(part: u64, whole: u64) -> u64 {
    if whole == 0 {
        return 0;
    }
    part.saturating_mul(100) / whole
}

fn finding(
    id: &'static str,
    severity: CacheHealthSeverity,
    component: &'static str,
    message: String,
    observed: u64,
    threshold: u64,
) -> CacheHealthFinding {
    CacheHealthFinding {
        id: id.to_string(),
        severity,
        component: component.to_string(),
        message,
        observed,
        threshold,
    }
}

/// Judge a statistics snapshot.
///
/// Pure: it reads the snapshot and nothing else, so it is safe to call on a
/// stats sample taken anywhere, including one restored from a metrics scrape.
pub fn cache_health_report(stats: &CacheStats) -> CacheHealthReport {
    let mut findings = Vec::new();

    let hits = stats
        .memory_hits
        .saturating_add(stats.pmem_hits)
        .saturating_add(stats.disk_hits);
    let requests = hits.saturating_add(stats.misses);
    let seasoned = requests >= HEALTH_MIN_REQUESTS;

    // --- faults, reported however little traffic there has been ------------

    // A refill failure means a read found the value on a lower tier and then
    // failed to put it back in memory, so the next read pays the same cost
    // again. That is the read-through path not doing its job.
    if stats.refill_failures > 0 {
        findings.push(finding(
            "refill_failures",
            CacheHealthSeverity::Critical,
            "refill",
            format!(
                "{} read-through refills failed, so those reads will pay the lower tier again",
                stats.refill_failures
            ),
            stats.refill_failures,
            0,
        ));
    }

    // A reclaim whose durable delete failed has not reclaimed anything that
    // survives a restart: the copy on the device is what recovery reads, and it
    // returns without the metadata that carried its life -- as an entry that
    // will never expire again. The entry looks gone until the process does.
    if stats.expired_delete_failures > 0 {
        findings.push(finding(
            "expiry_could_not_delete_durable_copy",
            CacheHealthSeverity::Critical,
            "expiry",
            format!(
                "{} expired entries could not have their stored copy deleted; they will \
                 come back when the cache restarts",
                stats.expired_delete_failures
            ),
            stats.expired_delete_failures,
            0,
        ));
    }

    // Eviction stepping over more pinned entries than it manages to reclaim
    // means reclaim is being starved by callers holding handles.
    if stats.eviction_pinned_skips > stats.memory_evictions && stats.eviction_pinned_skips > 0 {
        findings.push(finding(
            "pinned_entries_block_eviction",
            CacheHealthSeverity::Critical,
            "pinning",
            format!(
                "eviction stepped over {} pinned entries while reclaiming {}; \
                 pinned handles are outliving the memory pressure",
                stats.eviction_pinned_skips, stats.memory_evictions
            ),
            stats.eviction_pinned_skips,
            stats.memory_evictions,
        ));
    }

    // Values bigger than the tier are never admitted, however much room there
    // is. This is a sizing mistake, not pressure.
    if stats.eviction_oversize > 0 {
        findings.push(finding(
            "values_larger_than_tier",
            CacheHealthSeverity::Warning,
            "admission",
            format!(
                "{} writes were larger than the memory tier itself and could never be admitted",
                stats.eviction_oversize
            ),
            stats.eviction_oversize,
            0,
        ));
    }

    if stats.ssd_oversize_rejections > 0 {
        findings.push(finding(
            "values_larger_than_ssd_block",
            CacheHealthSeverity::Warning,
            "ssd_admission",
            format!(
                "{} writes exceeded the largest block the SSD tier will store",
                stats.ssd_oversize_rejections
            ),
            stats.ssd_oversize_rejections,
            0,
        ));
    }

    // A budget that is turning writes away is doing its job, but it is also
    // costing hit rate, and an operator staring at a fallen hit rate should not
    // have to guess that the cause is a cap they set months ago.
    if stats.ssd_write_budget_rejections > 0 && stats.ssd_write_budget_share < WRITE_BUDGET_SCALE {
        let share_percent = percent(stats.ssd_write_budget_share, WRITE_BUDGET_SCALE);
        findings.push(finding(
            "ssd_write_budget_throttling",
            if share_percent < 50 {
                CacheHealthSeverity::Warning
            } else {
                CacheHealthSeverity::Info
            },
            "ssd_admission",
            format!(
                "the SSD write budget is admitting {share_percent}% of keys and has turned away \
                 {} writes; raise the target if the drive can take it, or accept the hit rate",
                stats.ssd_write_budget_rejections
            ),
            share_percent,
            100,
        ));
    }

    // The budget only refuses *admissions*. Reclaim and recovery writes are
    // counted against the target but never turned away -- they are work the
    // cache has already committed to. So if those alone exceed the target, the
    // budget can throttle admissions to nothing and the drive still sees more
    // than the operator asked for.
    //
    // That state is invisible from the share alone: a budget holding the line
    // and a budget being ignored both look like a share near the floor. Only
    // the measured rate separates them, which is why it is published.
    if stats.ssd_write_budget_target_bytes_per_sec > 0
        && stats.ssd_write_budget_observed_bytes_per_sec > stats.ssd_write_budget_target_bytes_per_sec
        && stats.ssd_write_budget_share <= WRITE_BUDGET_FLOOR_SHARE
    {
        let over_percent = percent(
            stats.ssd_write_budget_observed_bytes_per_sec,
            stats.ssd_write_budget_target_bytes_per_sec,
        );
        findings.push(finding(
            "ssd_write_budget_cannot_be_met",
            CacheHealthSeverity::Warning,
            "ssd_admission",
            format!(
                "the SSD write budget is admitting almost nothing and the drive is still seeing \
                 {over_percent}% of the {} bytes/s target; the excess is reclaim and recovery, \
                 which the budget counts but cannot refuse -- raise the target, or cut the \
                 rewriting",
                stats.ssd_write_budget_target_bytes_per_sec
            ),
            over_percent,
            100,
        ));
    }

    let backpressure = stats
        .writeback_backpressure_events
        .saturating_add(stats.async_writeback_backpressure_rejections);
    if backpressure > 0 {
        findings.push(finding(
            "writeback_backpressure",
            CacheHealthSeverity::Warning,
            "writeback",
            format!(
                "writeback applied backpressure {} times; the queue is not keeping up with writes",
                backpressure
            ),
            backpressure,
            0,
        ));
    }

    // --- ratios, which only mean something once there is traffic -----------

    if seasoned {
        let hit_percent = percent(hits, requests);
        if hit_percent < HEALTH_MIN_HIT_PERCENT && stats.memory_evictions > 0 {
            findings.push(finding(
                "hit_rate_below_floor",
                CacheHealthSeverity::Warning,
                "sizing",
                format!(
                    "{hit_percent}% of {requests} reads hit while the cache was evicting; \
                     the working set does not fit"
                ),
                hit_percent,
                HEALTH_MIN_HIT_PERCENT,
            ));
        }

        // Reads are served under a shared lock right up until one has to move
        // its entry in the access order. If that is happening on a quarter of
        // hits, readers are serialising and the refresh distance is too short
        // for this workload.
        if hits > 0 {
            let escalation_percent = percent(stats.access_order_refreshes, hits);
            if escalation_percent > HEALTH_MAX_ESCALATION_PERCENT {
                findings.push(finding(
                    "reads_escalate_to_exclusive",
                    CacheHealthSeverity::Warning,
                    "read_path",
                    format!(
                        "{escalation_percent}% of hits took the cache exclusively to reorder \
                         their entry; raise the access-order refresh distance, or shard the cache"
                    ),
                    escalation_percent,
                    HEALTH_MAX_ESCALATION_PERCENT,
                ));
            }
        }

        // An expired read is a miss the cache paid for twice: once to store the
        // entry and once to find it too old to serve. A few are the ordinary
        // cost of expiry; a fifth of all reads means the life is shorter than
        // the interval the workload comes back at, and lengthening it would
        // turn those into hits.
        if stats.expired_reads > 0 {
            let expired_percent = percent(stats.expired_reads, requests);
            if expired_percent > HEALTH_MAX_EXPIRED_READ_PERCENT {
                findings.push(finding(
                    "time_to_live_shorter_than_reuse",
                    CacheHealthSeverity::Warning,
                    "expiry",
                    format!(
                        "{expired_percent}% of {requests} reads found an entry that had \
                         already expired; the time to live is shorter than the interval \
                         callers come back at"
                    ),
                    expired_percent,
                    HEALTH_MAX_EXPIRED_READ_PERCENT,
                ));
            }
        }

        let offered = stats
            .memory_admission_accepted
            .saturating_add(stats.memory_admission_rejected);
        if offered > 0 {
            let reject_percent = percent(stats.memory_admission_rejected, offered);
            if reject_percent > HEALTH_MAX_ADMISSION_REJECT_PERCENT {
                findings.push(finding(
                    "memory_admission_rejecting",
                    CacheHealthSeverity::Warning,
                    "admission",
                    format!(
                        "the memory tier turned away {reject_percent}% of {offered} writes"
                    ),
                    reject_percent,
                    HEALTH_MAX_ADMISSION_REJECT_PERCENT,
                ));
            }
        }

        // Victim selection weighs a bounded window of candidates and only falls
        // back to the whole tier when that window holds nothing evictable. If
        // the average selection weighed more than the window, it is taking that
        // fallback routinely, which is the expensive path.
        if stats.memory_evictions > 0 {
            let weighed = stats.eviction_sampled_groups / stats.memory_evictions;
            if weighed > EVICTION_CANDIDATE_WINDOW as u64 {
                findings.push(finding(
                    "eviction_falls_back_to_full_scan",
                    CacheHealthSeverity::Warning,
                    "eviction",
                    format!(
                        "victim selection weighed {weighed} candidates per eviction against a \
                         window of {EVICTION_CANDIDATE_WINDOW}; the window is full of entries \
                         it cannot evict"
                    ),
                    weighed,
                    EVICTION_CANDIDATE_WINDOW as u64,
                ));
            }
        }
    }

    findings.sort_by(|left, right| {
        right
            .severity
            .cmp(&left.severity)
            .then_with(|| left.id.cmp(&right.id))
    });

    let critical_count = findings
        .iter()
        .filter(|f| f.severity == CacheHealthSeverity::Critical)
        .count();
    let warning_count = findings
        .iter()
        .filter(|f| f.severity == CacheHealthSeverity::Warning)
        .count();

    CacheHealthReport {
        healthy: critical_count == 0,
        critical_count,
        warning_count,
        findings,
    }
}


/// Renders a health report in Prometheus text exposition format.
///
/// `labels` are appended to every series, matching `prometheus_text`, so a
/// cache's statistics and its health line up under the same label set.
///
/// Three families are exported:
///
/// * `matrixcache_health_ok` -- 1 when nothing critical was found. This is the
///   series to alert on if you only alert on one.
/// * `matrixcache_health_findings` -- how many findings there are, by severity.
///   Always present, so "no findings" is a zero rather than a missing series.
/// * `matrixcache_health_finding` and `..._threshold` -- one pair per finding
///   currently reported, carrying the observed number and the threshold it was
///   judged against, so a dashboard can show how close a cache is rather than
///   only whether it has crossed.
///
/// The per-finding series exist only while their finding does. That is the
/// usual shape for this kind of metric, but it does mean an alert on one should
/// use a presence check rather than a comparison against an absent series.
pub fn cache_health_prometheus_text(report: &CacheHealthReport, labels: &[(&str, &str)]) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(1024);
    let tags = render_labels(labels);
    let inner = tags.trim_start_matches('{').trim_end_matches('}');
    let separator = if inner.is_empty() { "" } else { "," };

    metric(
        &mut out,
        "matrixcache_health_ok",
        "1 when the cache reports no critical finding",
        "gauge",
        &tags,
        u64::from(report.healthy),
    );

    let _ = writeln!(
        out,
        "# HELP matrixcache_health_findings Findings currently reported, by severity"
    );
    let _ = writeln!(out, "# TYPE matrixcache_health_findings gauge");
    for (severity, count) in [
        (CacheHealthSeverity::Critical, report.critical_count),
        (CacheHealthSeverity::Warning, report.warning_count),
    ] {
        let _ = writeln!(
            out,
            "matrixcache_health_findings{{{inner}{separator}severity=\"{}\"}} {}",
            severity.as_str(),
            count
        );
    }

    if report.findings.is_empty() {
        return out;
    }

    let _ = writeln!(
        out,
        "# HELP matrixcache_health_finding The number a reported finding observed"
    );
    let _ = writeln!(out, "# TYPE matrixcache_health_finding gauge");
    for finding in &report.findings {
        let _ = writeln!(
            out,
            "matrixcache_health_finding{{{inner}{separator}id=\"{}\",component=\"{}\",severity=\"{}\"}} {}",
            escape(&finding.id),
            escape(&finding.component),
            finding.severity.as_str(),
            finding.observed
        );
    }

    let _ = writeln!(
        out,
        "# HELP matrixcache_health_finding_threshold The threshold a reported finding was judged against"
    );
    let _ = writeln!(out, "# TYPE matrixcache_health_finding_threshold gauge");
    for finding in &report.findings {
        let _ = writeln!(
            out,
            "matrixcache_health_finding_threshold{{{inner}{separator}id=\"{}\"}} {}",
            escape(&finding.id),
            finding.threshold
        );
    }

    out
}
