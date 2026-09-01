// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

// Reclaim for zoned storage.
//
// A zone cannot be rewritten in place, so space is returned only by resetting a
// whole group. Everything still worth keeping in that group has to be read out
// and written somewhere else first, and everything not worth keeping has to
// leave the index before the group is reset -- otherwise the index would point
// into a zone whose contents are gone.
//
// The order is therefore fixed: read the group's operation log, decide record by
// record, collect what survives, and only then reset.
//
// What survives is decided by the record's state:
//
// ```text
// SoftDel  ->  always dropped
// Normal   ->  dropped when the group is being reclaimed lossily
// Pinned   ->  always kept
// ```
//
// This runs on the calling thread. The model runs a background thread woken by a
// timer or by `Notify`; this crate has no background threads, so `collect` is
// called by whoever wants the space.

/// What one reclaim pass did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ZoneGcReport {
    /// The group reclaimed, if one was.
    pub group_id: Option<u16>,
    /// Operation log entries read.
    pub scanned: usize,
    /// Records removed from the index.
    pub dropped: usize,
    /// Records that must be written elsewhere before the group is reset.
    pub kept: usize,
    /// Entries whose checksum did not match, which are skipped.
    pub corrupt: usize,
    /// Bytes the reset returned.
    pub bytes_reclaimed: u64,
}

/// A record that outlived its group and must be written somewhere else.
pub type ZoneGcSurvivor = (String, Vec<u8>);

/// What one reclaim pass produced: what it did, and what still needs rewriting.
pub type ZoneGcOutcome = (ZoneGcReport, Vec<ZoneGcSurvivor>);

/// One decoded operation-log entry: the key, its value if the record could be
/// read, and how many bytes the entry occupied.
pub type ZoneGcEntry = (String, Option<Vec<u8>>, usize);

/// Reclaims whole zone groups, keeping the records that must survive.
///
/// **Standalone**, for the same reason as [`ZoneManager`]: nothing in the
/// cache runs it.
#[derive(Debug, Clone)]
pub struct ZoneGcWorker {
    encoder: BufferEncoder,
    enabled: bool,
    max_record_length: usize,
}

impl ZoneGcWorker {
    /// Creates a worker that will not read a record longer than
    /// `max_record_length`.
    pub fn new(max_record_length: usize) -> Self {
        Self {
            encoder: BufferEncoder::new(max_record_length),
            enabled: false,
            max_record_length,
        }
    }

    /// Allows reclaim to run.
    pub fn start(&mut self) {
        self.enabled = true;
    }

    /// Stops reclaim. A stopped worker collects nothing and resets nothing.
    pub fn stop(&mut self) {
        self.enabled = false;
    }

    /// Whether reclaim is allowed to run.
    pub fn gc_enabled(&self) -> bool {
        self.enabled
    }

    /// The longest record this worker will read back.
    pub fn max_record_length(&self) -> usize {
        self.max_record_length
    }

    /// The state an index entry is in.
    ///
    /// Both record shapes report one, device records included, so reclaim keeps
    /// no decoder of its own. A record with no state at all is treated as live.
    pub fn record_state(value: &SsdIndexValue) -> RecordState {
        value.state().unwrap_or(RecordState::Normal)
    }

    /// Whether a record in that state survives this kind of reclaim.
    pub fn survives(state: RecordState, mode: GcMode) -> bool {
        match state {
            // Already deleted: the only reason it still existed was that its
            // zone had not been reset yet, and now it is being reset.
            RecordState::SoftDel => false,
            // Pinned records are exactly the ones a holder is relying on.
            RecordState::Pinned => true,
            RecordState::Normal | RecordState::MaxCode => matches!(mode, GcMode::Lossless),
        }
    }

    /// Reads the key out of one operation log entry.
    ///
    /// Returns the key and how many bytes the entry used, or `None` if the entry
    /// does not check out.
    pub fn construct_single_key(&self, oplog: &[u8]) -> Option<(String, usize)> {
        let (key, _, used, corrupt) = self.encoder.deserialize_oplog(oplog);
        if corrupt || used == 0 {
            return None;
        }
        Some((key, used))
    }

    /// Reads one operation log entry and the record it points at.
    ///
    /// Returns the key, its value, and how many bytes the entry used. The value
    /// is `None` when the entry is intact but the record it points at is not --
    /// a torn record is dropped rather than copied forward.
    pub fn construct_single_record(
        &self,
        oplog: &[u8],
        zones: &mut ZoneManager,
    ) -> Result<Option<ZoneGcEntry>, CacheError> {
        let (key, pointer, used, corrupt) = self.encoder.deserialize_oplog(oplog);
        if corrupt || used == 0 {
            return Ok(None);
        }

        let (units, lba) = decode_colored_ptr(pointer);
        let span = units as usize * self.encoder.align_size();
        if span == 0 || span > self.max_record_length {
            return Ok(Some((key, None, used)));
        }
        let page_delta = lba % self.encoder.align_size() as u64;
        let Ok(buf) = zones.read(lba - page_delta, span) else {
            return Ok(Some((key, None, used)));
        };
        let start = page_delta as usize;
        if start >= buf.len() {
            return Ok(Some((key, None, used)));
        }
        let (value, corrupt) = self.encoder.deserialize_data(&buf[start..]);
        Ok(Some((key, (!corrupt).then_some(value), used)))
    }

    /// Walks a group's operation log and decides each record's fate.
    ///
    /// Records that do not survive are removed from `index`. Records that do are
    /// returned, so the caller can write them somewhere else *before* the group
    /// is reset — this does not reset anything itself.
    pub fn process_metadata(
        &self,
        oplogs: &[u8],
        mode: GcMode,
        index: &SsdIndex,
        zones: &mut ZoneManager,
    ) -> Result<ZoneGcOutcome, CacheError> {
        let mut report = ZoneGcReport::default();
        let mut survivors = Vec::new();
        let mut cursor = 0usize;

        while cursor < oplogs.len() {
            let remaining = &oplogs[cursor..];
            if remaining.len() < BufferEncoder::OPLOG_FIXED_PART_SIZE as usize {
                break;
            }
            let Some((key, value, used)) = self.construct_single_record(remaining, zones)? else {
                report.corrupt += 1;
                break;
            };
            report.scanned += 1;
            cursor += used;

            let Some(entry) = index.get(&key) else {
                // Already gone from the index; the log entry is just history.
                report.dropped += 1;
                continue;
            };
            let state = Self::record_state(&entry);
            match (Self::survives(state, mode), value) {
                (true, Some(value)) => {
                    report.kept += 1;
                    survivors.push((key, value));
                }
                (true, None) => {
                    // Worth keeping but unreadable: dropping it is the only
                    // honest option, since the zone is about to go.
                    report.corrupt += 1;
                    index.delete_if(&key, |_| true);
                    report.dropped += 1;
                }
                (false, _) => {
                    index.delete_if(&key, |_| true);
                    report.dropped += 1;
                }
            }
        }

        Ok((report, survivors))
    }

    /// Reclaims the group with the most garbage, if reclaim is warranted.
    ///
    /// Returns the records that must be rewritten before the caller relies on
    /// the freed space. The group is reset only after its log has been fully
    /// processed, so nothing is dropped that the index still points at.
    pub fn collect(
        &self,
        zones: &mut ZoneManager,
        index: &SsdIndex,
    ) -> Result<ZoneGcOutcome, CacheError> {
        let mut report = ZoneGcReport::default();
        if !self.enabled {
            return Ok((report, Vec::new()));
        }
        let Some((group_id, mode)) = zones.find_gc_group() else {
            return Ok((report, Vec::new()));
        };
        report.group_id = Some(group_id);

        let oplogs = match zones.load_meta_data(group_id) {
            Ok(oplogs) => oplogs,
            // A group with no log holds nothing anyone can find; resetting it is
            // still the right move.
            Err(CacheError::NotFound) => Vec::new(),
            Err(err) => return Err(err),
        };

        let body = if oplogs.len() > BufferEncoder::OPLOG_HEADER_SIZE as usize {
            let declared = u64::from_le_bytes(
                oplogs[..8].try_into().unwrap_or([0; 8]),
            ) as usize;
            let end = declared.min(oplogs.len());
            &oplogs[BufferEncoder::OPLOG_HEADER_SIZE as usize..end.max(8)]
        } else {
            &[][..]
        };

        let (walked, survivors) = self.process_metadata(body, mode, index, zones)?;
        report.scanned = walked.scanned;
        report.dropped = walked.dropped;
        report.kept = walked.kept;
        report.corrupt = walked.corrupt;

        zones.reset_group(group_id)?;
        report.bytes_reclaimed = zones.zone_capacity();
        Ok((report, survivors))
    }
}
