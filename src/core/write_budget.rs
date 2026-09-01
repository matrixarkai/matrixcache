// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI
//
// Keeping the SSD tier inside a write budget.
//
// Flash wears out by being written to. A cache that admits everything the
// workload offers will write as fast as the workload runs, which is fine for a
// benchmark and expensive for a drive that has to last three years. The usual
// answer is to decide a sustainable write rate up front and admit only as much
// as fits inside it.
//
// Rejecting whatever happens not to fit would bias the cache towards small
// entries: once the budget is tight, a stream of small blocks would sail
// through while nothing large was ever admitted again. So the decision is made
// per key rather than per byte -- a fixed share of keys is admitted, chosen by
// hashing the key, and the share is what moves in response to the observed
// rate. Every size is then equally likely to get in, and the same key decides
// the same way while the share holds still.

/// Resolution of the admitted share. Ten thousand steps is finer than the rate
/// can be measured and keeps the arithmetic in integers.
const WRITE_BUDGET_SCALE: u64 = 10_000;

/// How long the observed rate is averaged over before the share is adjusted.
///
/// Short enough to react within a scrape interval, long enough that a burst of
/// a few large blocks does not slam the share shut.
const WRITE_BUDGET_WINDOW: Duration = Duration::from_secs(1);

/// The most the share may move in one step, as a multiplier numerator over
/// `WRITE_BUDGET_SCALE`. Halving or doubling per window converges in a few
/// seconds without oscillating, which a proportional correction does when the
/// workload is bursty.
const WRITE_BUDGET_MAX_STEP_UP: u64 = 2;

/// Admission control for SSD writes, targeting a sustainable byte rate.
///
/// Constructed [`unlimited`](Self::unlimited) by default, in which case it
/// admits everything and costs one comparison. Give it a target and it starts
/// measuring.
///
/// The clock is passed in rather than read, so the behaviour can be tested
/// across minutes of simulated time without waiting for any of them.
#[derive(Debug, Clone)]
pub struct SsdWriteBudget {
    /// Bytes per second the tier is allowed to absorb. Zero means no limit.
    target_bytes_per_sec: u64,
    /// Share of keys currently admitted, out of [`WRITE_BUDGET_SCALE`].
    admitted_share: u64,
    bytes_this_window: u64,
    window_started: Option<Instant>,
    /// Bytes per second measured over the last window that closed.
    ///
    /// Kept because the share alone cannot distinguish a budget that is
    /// working from one that cannot work: both look like a share pinned near
    /// the floor, and only the rate says which.
    observed_bytes_per_sec: u64,
}

impl Default for SsdWriteBudget {
    fn default() -> Self {
        Self::unlimited()
    }
}

impl SsdWriteBudget {
    /// A budget that admits everything, which is what a cache does unless it
    /// is told a drive it has to look after.
    pub fn unlimited() -> Self {
        Self {
            target_bytes_per_sec: 0,
            admitted_share: WRITE_BUDGET_SCALE,
            bytes_this_window: 0,
            window_started: None,
            observed_bytes_per_sec: 0,
        }
    }

    /// A budget targeting `bytes_per_sec`. Zero is the same as unlimited.
    pub fn with_target(bytes_per_sec: u64) -> Self {
        Self {
            target_bytes_per_sec: bytes_per_sec,
            admitted_share: WRITE_BUDGET_SCALE,
            bytes_this_window: 0,
            window_started: None,
            observed_bytes_per_sec: 0,
        }
    }

    /// Bytes per second the last closed window actually saw.
    ///
    /// Zero until a window has closed. Compared against the target, this is
    /// what tells a budget that is holding from one that is being ignored.
    pub fn observed_bytes_per_sec(&self) -> u64 {
        self.observed_bytes_per_sec
    }

    /// Bytes per second this budget is aiming at. Zero when unlimited.
    pub fn target_bytes_per_sec(&self) -> u64 {
        self.target_bytes_per_sec
    }

    pub fn set_target_bytes_per_sec(&mut self, bytes_per_sec: u64) {
        self.target_bytes_per_sec = bytes_per_sec;
        if bytes_per_sec == 0 {
            self.admitted_share = WRITE_BUDGET_SCALE;
        }
    }

    /// The share of keys currently admitted, out of 10000. Exported so an
    /// operator can see the budget working rather than infer it from a drop in
    /// admissions.
    pub fn admitted_share(&self) -> u64 {
        self.admitted_share
    }

    /// Whether a write of `block_bytes` under `key_hash` may go ahead.
    ///
    /// Rolls the measurement window first, so a caller that stops writing for a
    /// while sees the share recover rather than staying where the last burst
    /// left it.
    pub fn admits(&mut self, key_hash: u64, now: Instant) -> bool {
        if self.target_bytes_per_sec == 0 {
            return true;
        }
        self.roll_window(now);
        if self.admitted_share >= WRITE_BUDGET_SCALE {
            return true;
        }
        // The top bits are the well-mixed ones for most hashers, and using them
        // keeps this independent of the low bits a hash map uses for bucketing.
        (key_hash >> 32) % WRITE_BUDGET_SCALE < self.admitted_share
    }

    /// Record bytes actually written, so the next window can judge the rate.
    ///
    /// Called for every physical write, including ones this budget did not gate
    /// -- reclaim and recovery wear the drive exactly as admissions do, and a
    /// budget that ignored them would aim at the wrong number.
    pub fn record_written(&mut self, bytes: u64, now: Instant) {
        if self.target_bytes_per_sec == 0 {
            return;
        }
        if self.window_started.is_none() {
            self.window_started = Some(now);
        }
        self.bytes_this_window = self.bytes_this_window.saturating_add(bytes);
    }

    fn roll_window(&mut self, now: Instant) {
        let Some(started) = self.window_started else {
            self.window_started = Some(now);
            return;
        };
        let elapsed = now.saturating_duration_since(started);
        if elapsed < WRITE_BUDGET_WINDOW {
            return;
        }

        // Bytes the target would have allowed over the window that just ended.
        let allowed = self
            .target_bytes_per_sec
            .saturating_mul(elapsed.as_millis().min(u128::from(u64::MAX)) as u64)
            / 1_000;

        self.admitted_share = if self.bytes_this_window <= allowed {
            // Under target: open up, but by a bounded step, so a quiet window
            // does not immediately undo a share that took several windows to
            // find.
            self.admitted_share
                .saturating_mul(WRITE_BUDGET_MAX_STEP_UP)
                .clamp(1, WRITE_BUDGET_SCALE)
        } else if self.bytes_this_window == 0 {
            WRITE_BUDGET_SCALE
        } else {
            // Over target: scale the share by how far over we went. Never to
            // zero -- a budget that admits nothing can never learn that the
            // pressure has passed.
            (self.admitted_share.saturating_mul(allowed) / self.bytes_this_window).max(1)
        };

        // Recorded before the counter is reset, and from the window that just
        // closed rather than the one starting, so it describes something that
        // actually happened.
        let elapsed_millis = elapsed.as_millis().min(u128::from(u64::MAX)) as u64;
        self.observed_bytes_per_sec = if elapsed_millis == 0 {
            0
        } else {
            self.bytes_this_window.saturating_mul(1_000) / elapsed_millis
        };
        self.bytes_this_window = 0;
        self.window_started = Some(now);
    }
}
