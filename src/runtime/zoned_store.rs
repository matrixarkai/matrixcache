// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

// Zoned storage: an append-only device model where writes advance a per-zone
// write pointer and space is reclaimed a whole zone at a time.
//
// A zone accepts writes only at its write pointer and cannot be overwritten in
// place; the only way to reuse its space is to reset the entire zone. Reclaim is
// therefore a group-level decision driven by how much of a group is garbage,
// which is what [`ZoneManager::find_gc_group`] reports and
// [`ZoneManager::reset_group`] acts on.
//
// The device is a regular file. That is one of the two device paths the model
// describes -- a real zoned namespace is the other -- and it is the one that
// needs no special hardware, so it is what this crate implements.
//
// Only *large* zone mode is implemented: one group holds exactly one zone, and
// that zone is split into a data region followed by a metadata region:
//
// ```text
// |<-------------------------- one zone ------------------------->|
// | header | data ... data | meta log | padding            | footer|
// ```
//
// The header records a sequence number and a magic value; the footer records
// where the group's metadata log starts and how long it is, so a restart can
// find the metadata without scanning the zone.
//
// The append kind and the reclaim mode are the crate's existing `DataKind` and
// `GcMode`; only the zone, the device and the manager are new here.

use std::io::{Read as _, Seek as _};

/// Page size assumed for a simulated device, in bytes.
///
/// Every offset and length handed to [`ZoneDevice`] is a multiple of this.
pub const ZONE_PAGE_SIZE: u64 = 4096;

/// Magic value stamped into each zone header.
pub const ZONE_HEADER_MAGIC: u64 = 20220209;

/// How a device's zones are grouped.
///
/// The discriminants match the values the reference model uses on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ZoneMode {
    /// One group spans several zones.
    Small = 1,
    /// One group is exactly one zone, split into a data and a metadata region.
    Large = 10,
}

/// A zone's position in its write lifecycle.
///
/// `Empty -> Open -> Full -> Empty`, where the last step is a reset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ZoneStatus {
    /// Never written since the last reset.
    Empty = 1,
    /// Open for appends.
    Open = 2,
    /// No space left; only a reset can reuse it.
    Full = 3,
    /// Closed to appends but still holding data.
    Closed = 4,
    /// Unusable.
    Offline = 10,
}

/// One zone: a region that is written forward and reset as a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Zone {
    /// Offset of the zone's first byte within the device.
    pub start: u64,
    /// Distance to the next zone's start.
    pub size: u64,
    /// Writable bytes, which can be less than [`Zone::size`].
    pub capacity: u64,
    /// Device offset the next append will land at.
    pub write_pointer: u64,
    /// Bytes written that have not been trimmed.
    pub valid_bytes: u64,
    /// Where the zone sits in its lifecycle.
    pub status: ZoneStatus,
}

impl Zone {
    /// Creates an empty zone starting at `start`.
    pub fn new(start: u64, size: u64, capacity: u64) -> Self {
        Self {
            start,
            size,
            capacity,
            write_pointer: start,
            valid_bytes: 0,
            status: ZoneStatus::Empty,
        }
    }

    /// Bytes still appendable before the zone is full.
    pub fn avail_bytes(&self) -> u64 {
        self.capacity
            .saturating_sub(self.write_pointer.saturating_sub(self.start))
    }

    /// Where the zone sits in its lifecycle.
    pub fn status(&self) -> ZoneStatus {
        self.status
    }

    /// Returns the zone to its post-reset state.
    pub fn reset(&mut self) {
        self.write_pointer = self.start;
        self.valid_bytes = 0;
        self.status = ZoneStatus::Empty;
    }
}

/// The geometry a [`ZoneDevice`] was opened with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZoneDeviceInfo {
    /// Total bytes the device spans.
    pub device_size: u64,
    /// Total bytes usable for zones.
    pub device_capacity: u64,
    /// Alignment every read and write obeys.
    pub page_size: u64,
    /// Distance between consecutive zone starts.
    pub zone_size: u64,
    /// Writable bytes per zone.
    pub zone_capacity: u64,
    /// How many zones the device holds.
    pub zones_in_device: u64,
    /// Bytes per group.
    pub group_size: u64,
    /// How many groups the device holds.
    pub groups_in_device: u64,
    /// How many zones make up one group.
    pub zones_in_group: u64,
}

/// A file standing in for a zoned block device.
///
/// The file is created at full size on open, so every offset within
/// [`ZoneDeviceInfo::device_capacity`] is addressable straight away.
#[derive(Debug)]
pub struct ZoneDevice {
    file: File,
    path: PathBuf,
    info: ZoneDeviceInfo,
    mode: ZoneMode,
}

impl ZoneDevice {
    /// Opens `path` as a zoned device of `capacity` bytes.
    ///
    /// `zone_capacity` must be a multiple of [`ZONE_PAGE_SIZE`] and `capacity` a
    /// multiple of `zone_capacity`; anything else cannot be addressed by whole
    /// pages and is rejected rather than silently rounded.
    pub fn open(path: impl AsRef<Path>, capacity: u64, zone_capacity: u64) -> Result<Self, CacheError> {
        if zone_capacity == 0 || !zone_capacity.is_multiple_of(ZONE_PAGE_SIZE) {
            return Err(CacheError::InvalidConfig(format!(
                "zone capacity {zone_capacity} must be a non-zero multiple of {ZONE_PAGE_SIZE}"
            )));
        }
        if capacity == 0 || !capacity.is_multiple_of(zone_capacity) {
            return Err(CacheError::InvalidConfig(format!(
                "device capacity {capacity} must be a non-zero multiple of zone capacity {zone_capacity}"
            )));
        }
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)?;
        file.set_len(capacity)?;

        let zones_in_device = capacity / zone_capacity;
        let info = ZoneDeviceInfo {
            device_size: capacity,
            device_capacity: capacity,
            page_size: ZONE_PAGE_SIZE,
            zone_size: zone_capacity,
            zone_capacity,
            zones_in_device,
            // Large mode: one zone per group.
            group_size: zone_capacity,
            groups_in_device: zones_in_device,
            zones_in_group: 1,
        };
        Ok(Self {
            file,
            path,
            info,
            mode: ZoneMode::Large,
        })
    }

    /// The geometry this device was opened with.
    pub fn info(&self) -> ZoneDeviceInfo {
        self.info
    }

    /// How this device's zones are grouped.
    pub fn zone_mode(&self) -> ZoneMode {
        self.mode
    }

    /// The backing file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Builds the device's zones, all empty.
    pub fn init_zones(&self) -> Vec<Zone> {
        (0..self.info.zones_in_device)
            .map(|index| {
                Zone::new(
                    index * self.info.zone_size,
                    self.info.zone_size,
                    self.info.zone_capacity,
                )
            })
            .collect()
    }

    /// Marks the zone at `offset` open for appends.
    pub fn open_zone(&self, _offset: u64) -> Result<(), CacheError> {
        Ok(())
    }

    /// Marks the zone at `offset` closed to further appends.
    pub fn close_zone(&self, _offset: u64) -> Result<(), CacheError> {
        Ok(())
    }

    /// Discards the zone at `offset`, returning its space.
    ///
    /// A real device drops the contents; the file simulation zeroes them, so a
    /// later read cannot mistake a stale header for a live one.
    pub fn reset_zone(&mut self, offset: u64) -> Result<(), CacheError> {
        let zeros = vec![0u8; self.info.zone_capacity as usize];
        self.write(&zeros, offset)
    }

    /// Reads `len` bytes from `offset`.
    pub fn read(&mut self, offset: u64, len: usize) -> Result<Vec<u8>, CacheError> {
        if offset.saturating_add(len as u64) > self.info.device_capacity {
            return Err(CacheError::InvalidConfig(format!(
                "read of {len} bytes at {offset} runs past the device"
            )));
        }
        let mut buf = vec![0u8; len];
        self.file.seek(std::io::SeekFrom::Start(offset))?;
        self.file.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// Writes `buf` at `offset`.
    pub fn write(&mut self, buf: &[u8], offset: u64) -> Result<(), CacheError> {
        if offset.saturating_add(buf.len() as u64) > self.info.device_capacity {
            return Err(CacheError::InvalidConfig(format!(
                "write of {} bytes at {offset} runs past the device",
                buf.len()
            )));
        }
        self.file.seek(std::io::SeekFrom::Start(offset))?;
        std::io::Write::write_all(&mut self.file, buf)?;
        Ok(())
    }

    /// Flushes buffered writes to the file.
    pub fn close(&mut self) -> Result<(), CacheError> {
        std::io::Write::flush(&mut self.file)?;
        Ok(())
    }
}

/// One group of zones, tracked as a reclaim unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZoneGroup {
    group_id: u16,
    zone_index: usize,
    zone_size: u64,
    meta_offset: u64,
    meta_size: u64,
    garbage_bytes: u64,
}

impl ZoneGroup {
    fn new(group_id: u16, zone_index: usize, zone_size: u64) -> Self {
        Self {
            group_id,
            zone_index,
            zone_size,
            meta_offset: 0,
            meta_size: 0,
            garbage_bytes: 0,
        }
    }

    /// This group's identifier.
    pub fn group_id(&self) -> u16 {
        self.group_id
    }

    /// Fraction of the group that is garbage, used to rank reclaim candidates.
    pub fn garbage_rate(&self) -> f64 {
        if self.zone_size == 0 {
            return 0.0;
        }
        self.garbage_bytes as f64 / self.zone_size as f64
    }

    /// Bytes in this group that have been trimmed.
    pub fn garbage_bytes(&self) -> u64 {
        self.garbage_bytes
    }

    /// Overrides the garbage total, which recovery does after replaying metadata.
    pub fn set_garbage_bytes(&mut self, bytes: u64) {
        self.garbage_bytes = bytes;
    }

    /// Where this group's metadata log begins.
    pub fn meta_offset(&self) -> u64 {
        self.meta_offset
    }

    /// How long this group's metadata log is.
    pub fn meta_size(&self) -> u64 {
        self.meta_size
    }
}

/// Tracks zone metadata and serves reads, writes and reclaim decisions.
///
/// **Standalone.** A [`MultiLayerCache`] cannot be pointed at zoned storage:
/// [`SsdEngineKind`] offers one engine, and nothing in the cache's write path
/// reaches this. It is complete and tested on its own, and connecting it would
/// mean an engine kind that selects it and a tier that writes through it.
/// Documented here rather than left to be discovered, because a public type in
/// a cache library reads as something the cache uses.
///
/// Two ways this differs from the model it follows, both deliberate:
///
/// * [`ZoneManager::finish_group`] returns [`CacheError::CapacityExceeded`] when no
///   free group is left, rather than blocking until a collector frees one. A
///   library with no collector thread of its own would simply never wake.
/// * Only [`ZoneMode::Large`] is implemented.
#[derive(Debug)]
pub struct ZoneManager {
    device: ZoneDevice,
    zones: Vec<Zone>,
    groups: Vec<ZoneGroup>,
    free_list: VecDeque<u16>,
    gc_list: Vec<u16>,
    recovery_list: VecDeque<u16>,
    current_group: u16,
    meta_zone: Zone,
    header_size: u64,
    footer_size: u64,
    sequence: u64,
    ensured: bool,
    meta_appendable: bool,
}

impl ZoneManager {
    /// Opens `device` and takes the first free group.
    ///
    /// With `reuse_existing` set, zones whose header and footer still describe a
    /// readable metadata log are queued for [`ZoneManager::recover`]; otherwise
    /// every zone is reset and whatever it held is lost.
    pub fn new(device: ZoneDevice, reuse_existing: bool) -> Result<Self, CacheError> {
        let info = device.info();
        let zones = device.init_zones();
        let header_size = info.page_size;
        let footer_size = info.page_size;

        let mut manager = Self {
            device,
            zones,
            groups: Vec::new(),
            free_list: VecDeque::new(),
            gc_list: Vec::new(),
            recovery_list: VecDeque::new(),
            current_group: 0,
            meta_zone: Zone::new(0, 0, 0),
            header_size,
            footer_size,
            sequence: 0,
            ensured: false,
            meta_appendable: true,
        };

        let recovered = reuse_existing && manager.pick_recoverable_groups()?;
        if !recovered {
            manager.groups.clear();
            manager.free_list.clear();
            manager.recovery_list.clear();
            for index in 0..info.groups_in_device {
                let group_id = index as u16;
                manager
                    .groups
                    .push(ZoneGroup::new(group_id, index as usize, info.zone_size));
                manager.zones[index as usize].reset();
                let start = manager.zones[index as usize].start;
                manager.device.reset_zone(start)?;
                manager.free_list.push_back(group_id);
            }
        }

        if manager.free_list.is_empty() {
            return Err(CacheError::CapacityExceeded);
        }
        manager.open_next_group()?;
        Ok(manager)
    }

    /// The geometry of the underlying device.
    pub fn device_info(&self) -> ZoneDeviceInfo {
        self.device.info()
    }

    /// How this device's zones are grouped.
    pub fn zone_mode(&self) -> ZoneMode {
        self.device.zone_mode()
    }

    /// Distance between consecutive zone starts.
    pub fn zone_size(&self) -> u64 {
        self.device.info().zone_size
    }

    /// Writable bytes per zone.
    pub fn zone_capacity(&self) -> u64 {
        self.device.info().zone_capacity
    }

    /// Bytes per group.
    pub fn group_size(&self) -> u64 {
        self.device.info().group_size
    }

    /// Total addressable bytes.
    pub fn capacity(&self) -> u64 {
        self.device.info().device_capacity
    }

    /// Bytes held by groups that are full and awaiting reclaim.
    pub fn used_space(&self) -> u64 {
        let info = self.device.info();
        self.gc_list.len() as u64 * info.zones_in_group * info.zone_capacity
    }

    /// Trimmed bytes across all groups awaiting reclaim.
    pub fn garbage_bytes(&self) -> u64 {
        self.gc_list
            .iter()
            .map(|group_id| self.groups[*group_id as usize].garbage_bytes())
            .sum()
    }

    /// The group currently taking appends.
    pub fn current_group_id(&self) -> u16 {
        self.current_group
    }

    /// Groups that are full and awaiting reclaim.
    pub fn gc_group_ids(&self) -> &[u16] {
        &self.gc_list
    }

    /// How many groups are free to be opened.
    pub fn free_group_count(&self) -> usize {
        self.free_list.len()
    }

    /// Reports whether the current group can take `data_size` plus `meta_size`.
    ///
    /// A caller appends only after this returns `true`, because the footer must
    /// still fit once both are written.
    pub fn ensure_available_space(&mut self, data_size: u64, meta_size: u64) -> bool {
        self.ensured = true;
        let page_size = self.device.info().page_size;
        if !data_size.is_multiple_of(page_size) || !meta_size.is_multiple_of(page_size) {
            return false;
        }
        let needed = data_size.saturating_add(meta_size);
        let zone = self.zones[self.current_zone_index()];
        needed <= zone.avail_bytes().saturating_sub(self.footer_size)
    }

    /// Appends `buf` and returns the device offset it landed at.
    ///
    /// [`ZoneManager::ensure_available_space`] must have returned `true` first.
    /// Appending a [`DataKind::MetaLog`] closes out the group's data region:
    /// the rest of the zone is padded and the footer written, so exactly one
    /// metadata log is accepted per group.
    pub fn append(&mut self, buf: &[u8], kind: DataKind) -> Result<u64, CacheError> {
        let info = self.device.info();
        let size = buf.len() as u64;
        if size == 0 || !size.is_multiple_of(info.page_size) || size > info.zone_size {
            return Err(CacheError::InvalidConfig(format!(
                "append of {size} bytes is not a whole number of {} byte pages within a zone",
                info.page_size
            )));
        }
        if !self.ensured {
            return Err(CacheError::InvalidConfig(
                "append without a preceding ensure_available_space".to_string(),
            ));
        }

        let zone_index = self.current_zone_index();
        match kind {
            DataKind::Data => {
                if self.zones[zone_index].avail_bytes() < size {
                    return Err(CacheError::CapacityExceeded);
                }
                let offset = self.zones[zone_index].write_pointer;
                self.device.write(buf, offset)?;
                self.zones[zone_index].write_pointer += size;
                self.zones[zone_index].valid_bytes += size;
                self.zones[zone_index].status = ZoneStatus::Open;

                // The metadata log always follows the data written so far, so it
                // moves every time the data region grows.
                self.meta_zone.start = self.zones[zone_index].write_pointer;
                self.meta_zone.write_pointer = self.meta_zone.start;
                Ok(offset)
            }
            DataKind::MetaLog => {
                if !self.meta_appendable {
                    return Err(CacheError::InvalidConfig(
                        "a group accepts only one metadata log".to_string(),
                    ));
                }
                if self.meta_zone.avail_bytes() < size {
                    return Err(CacheError::CapacityExceeded);
                }
                let offset = self.meta_zone.start;
                self.device.write(buf, offset)?;

                let group_index = self.current_group as usize;
                self.groups[group_index].meta_offset = offset;
                self.groups[group_index].meta_size = size;
                self.meta_zone.valid_bytes += size;
                self.zones[zone_index].write_pointer += size;

                self.seal_current_zone()?;
                self.meta_appendable = false;
                Ok(offset)
            }
        }
    }

    /// Reads `len` bytes from `offset`.
    pub fn read(&mut self, offset: u64, len: usize) -> Result<Vec<u8>, CacheError> {
        self.device.read(offset, len)
    }

    /// Marks `size` bytes at `offset` as garbage.
    ///
    /// Zones are never rewritten in place, so this only moves bytes from the
    /// valid column to the garbage column; the space returns when the group is
    /// reset.
    pub fn trim_bytes(&mut self, offset: u64, size: u64) -> Result<(), CacheError> {
        let info = self.device.info();
        let group_id = offset / info.group_size;
        if group_id >= info.groups_in_device {
            return Err(CacheError::InvalidConfig(format!(
                "offset {offset} is outside the device"
            )));
        }
        let group_index = group_id as usize;
        let zone_index = self.groups[group_index].zone_index;
        self.zones[zone_index].valid_bytes = self.zones[zone_index].valid_bytes.saturating_sub(size);
        self.groups[group_index].garbage_bytes =
            self.groups[group_index].garbage_bytes.saturating_add(size);
        Ok(())
    }

    /// Closes the current group and opens a free one.
    ///
    /// Returns [`CacheError::CapacityExceeded`] when no free group is left, rather
    /// than waiting for a collector to release one.
    pub fn finish_group(&mut self) -> Result<(), CacheError> {
        self.ensured = false;
        self.meta_appendable = true;

        let zone_index = self.current_zone_index();
        let start = self.zones[zone_index].start;
        self.device.close_zone(start)?;
        self.zones[zone_index].status = ZoneStatus::Full;
        if !self.gc_list.contains(&self.current_group) {
            self.gc_list.push(self.current_group);
        }
        self.open_next_group()
    }

    /// Returns the group with the most garbage, or `None` while free space lasts.
    ///
    /// Reclaim is deliberately withheld until space is short: resetting a group
    /// early throws away data that is still readable and buys nothing.
    pub fn find_gc_group(&mut self) -> Option<(u16, GcMode)> {
        let info = self.device.info();
        let used = self.used_space();
        let capacity = self.capacity();
        let throttle = 10 * info.zone_capacity;
        let free_floor = throttle.min((capacity / 10).max(1 << 30));
        if capacity.saturating_sub(used) > free_floor {
            return None;
        }
        let groups = &self.groups;
        self.gc_list.sort_by(|left, right| {
            let left_rate = groups[*left as usize].garbage_rate();
            let right_rate = groups[*right as usize].garbage_rate();
            right_rate
                .partial_cmp(&left_rate)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        self.gc_list.first().map(|id| (*id, GcMode::Lossy))
    }

    /// Resets `group_id` and returns it to the free list.
    ///
    /// The group must be awaiting reclaim; resetting the group currently taking
    /// appends would discard writes a caller still believes landed.
    pub fn reset_group(&mut self, group_id: u16) -> Result<(), CacheError> {
        let Some(position) = self.gc_list.iter().position(|id| *id == group_id) else {
            return Err(CacheError::NotFound);
        };
        self.gc_list.remove(position);

        let group_index = group_id as usize;
        let zone_index = self.groups[group_index].zone_index;
        let start = self.zones[zone_index].start;
        self.device.reset_zone(start)?;
        self.zones[zone_index].reset();
        self.groups[group_index].garbage_bytes = 0;
        self.groups[group_index].meta_offset = 0;
        self.groups[group_index].meta_size = 0;
        self.free_list.push_back(group_id);
        Ok(())
    }

    /// Reads back the metadata log a group recorded.
    pub fn load_meta_data(&mut self, group_id: u16) -> Result<Vec<u8>, CacheError> {
        let group = *self
            .groups
            .get(group_id as usize)
            .ok_or(CacheError::NotFound)?;
        if group.meta_size == 0 {
            return Err(CacheError::NotFound);
        }
        self.device.read(group.meta_offset, group.meta_size as usize)
    }

    /// Replays the metadata of every group queued by a reopen.
    ///
    /// `replay` is handed each group's metadata log and returns how many bytes
    /// of it are still live; the rest of the zone counts as garbage, which is
    /// what ranks the group for reclaim.
    pub fn recover<F>(&mut self, mut replay: F) -> Result<usize, CacheError>
    where
        F: FnMut(u16, &[u8]) -> u64,
    {
        let mut recovered = 0;
        while let Some(group_id) = self.recovery_list.pop_front() {
            let group = self.groups[group_id as usize];
            let zone_size = self.device.info().zone_size;
            match self.device.read(group.meta_offset, group.meta_size as usize) {
                Ok(buf) => {
                    let valid_bytes = replay(group_id, &buf);
                    self.groups[group_id as usize]
                        .set_garbage_bytes(zone_size.saturating_sub(valid_bytes));
                    recovered += 1;
                }
                Err(_) => {
                    // Unreadable metadata means nothing in the group can be
                    // located again, so all of it is garbage.
                    self.groups[group_id as usize].set_garbage_bytes(zone_size);
                }
            }
            if !self.gc_list.contains(&group_id) {
                self.gc_list.push(group_id);
            }
        }
        Ok(recovered)
    }

    /// Renders a human-readable property.
    ///
    /// Understood names are `"device"`, `"group"` and `"garbage"`; anything else
    /// returns `None`.
    pub fn property(&self, name: &str) -> Option<String> {
        let info = self.device.info();
        match name {
            "device" => Some(format!(
                "Device Size (B): {}\nGroup Size (B): {}\nZone Size (B): {}\nZone Capacity (B): {}\nGroups in Device: {}\nZones in Device: {}\n",
                info.device_capacity,
                info.group_size,
                info.zone_size,
                info.zone_capacity,
                info.groups_in_device,
                info.zones_in_device,
            )),
            "group" => Some(format!(
                "Used Groups: {}\nFree Groups: {}\n",
                self.gc_list.len(),
                self.free_list.len()
            )),
            "garbage" => {
                let mut out = String::from("GID     Garbage Ratio\n");
                for group_id in &self.gc_list {
                    let group = self.groups[*group_id as usize];
                    out.push_str(&format!(
                        "{:7} {:7}\n",
                        group.group_id(),
                        group.garbage_rate()
                    ));
                }
                Some(out)
            }
            _ => None,
        }
    }

    /// Flushes the device.
    pub fn close(&mut self) -> Result<(), CacheError> {
        self.device.close()
    }

    fn current_zone_index(&self) -> usize {
        self.groups[self.current_group as usize].zone_index
    }

    fn open_next_group(&mut self) -> Result<(), CacheError> {
        let group_id = self.free_list.pop_front().ok_or(CacheError::CapacityExceeded)?;
        self.current_group = group_id;

        let zone_index = self.groups[group_id as usize].zone_index;
        let start = self.zones[zone_index].start;
        self.device.open_zone(start)?;
        self.zones[zone_index].reset();
        self.zones[zone_index].status = ZoneStatus::Open;

        self.sequence += 1;
        self.write_header(zone_index)?;

        // In large mode the metadata region is carved out of the same zone, so
        // it starts wherever the data written so far ends.
        let write_pointer = self.zones[zone_index].write_pointer;
        let capacity = self.zones[zone_index].capacity;
        self.meta_zone = Zone::new(write_pointer, capacity, capacity);
        self.meta_appendable = true;
        Ok(())
    }

    fn write_header(&mut self, zone_index: usize) -> Result<(), CacheError> {
        let mut header = vec![0u8; self.header_size as usize];
        header[..8].copy_from_slice(&self.sequence.to_le_bytes());
        header[8..16].copy_from_slice(&ZONE_HEADER_MAGIC.to_le_bytes());
        let offset = self.zones[zone_index].write_pointer;
        self.device.write(&header, offset)?;
        self.zones[zone_index].write_pointer += self.header_size;
        self.zones[zone_index].valid_bytes += self.header_size;
        Ok(())
    }

    // Pads the zone out to its footer and stamps where the metadata log lives,
    // so a reopen can find it without scanning.
    fn seal_current_zone(&mut self) -> Result<(), CacheError> {
        let zone_index = self.current_zone_index();
        let group = self.groups[self.current_group as usize];
        let padding = self.zones[zone_index]
            .avail_bytes()
            .saturating_sub(self.footer_size);
        if padding > 0 {
            let chunk = (1u64 << 20).min(padding) as usize;
            let zeros = vec![0u8; chunk];
            let mut left = padding;
            while left > 0 {
                let take = (chunk as u64).min(left) as usize;
                let offset = self.zones[zone_index].write_pointer;
                self.device.write(&zeros[..take], offset)?;
                self.zones[zone_index].write_pointer += take as u64;
                left -= take as u64;
            }
        }

        let mut footer = vec![0u8; self.footer_size as usize];
        footer[..8].copy_from_slice(&group.meta_offset.to_le_bytes());
        footer[8..16].copy_from_slice(&group.meta_size.to_le_bytes());
        let offset = self.zones[zone_index].write_pointer;
        self.device.write(&footer, offset)?;
        self.zones[zone_index].write_pointer += self.footer_size;
        self.zones[zone_index].valid_bytes += self.footer_size;
        self.zones[zone_index].status = ZoneStatus::Full;
        Ok(())
    }

    // Queues every zone whose header and footer still describe a readable
    // metadata log. Returns false when nothing is recoverable, which tells the
    // caller to start from a clean device instead.
    fn pick_recoverable_groups(&mut self) -> Result<bool, CacheError> {
        let info = self.device.info();
        self.groups.clear();
        self.free_list.clear();
        self.recovery_list.clear();

        for index in 0..info.groups_in_device {
            let group_id = index as u16;
            let mut group = ZoneGroup::new(group_id, index as usize, info.zone_size);
            let zone_start = self.zones[index as usize].start;

            let header = self.device.read(zone_start, self.header_size as usize)?;
            let sequence = u64::from_le_bytes(header[..8].try_into().unwrap_or([0; 8]));
            let magic = u64::from_le_bytes(header[8..16].try_into().unwrap_or([0; 8]));
            if sequence == 0 || magic != ZONE_HEADER_MAGIC {
                self.groups.push(group);
                self.free_list.push_back(group_id);
                continue;
            }

            let footer_offset = zone_start + info.zone_capacity - self.footer_size;
            let footer = self.device.read(footer_offset, self.footer_size as usize)?;
            let meta_offset = u64::from_le_bytes(footer[..8].try_into().unwrap_or([0; 8]));
            let meta_size = u64::from_le_bytes(footer[8..16].try_into().unwrap_or([0; 8]));
            let zone_end = zone_start + info.zone_capacity;
            let fits = meta_offset > zone_start
                && meta_offset < zone_end
                && meta_size > 0
                && meta_size <= zone_end - meta_offset - info.page_size;
            if fits {
                group.meta_offset = meta_offset;
                group.meta_size = meta_size;
                self.groups.push(group);
                self.recovery_list.push_back(group_id);
            } else {
                self.groups.push(group);
                self.free_list.push_back(group_id);
            }
        }

        if self.recovery_list.is_empty() {
            return Ok(false);
        }
        // Always keep one group free to write into, even if that costs the
        // oldest recoverable group.
        if self.free_list.is_empty() {
            if let Some(group_id) = self.recovery_list.pop_front() {
                let zone_index = self.groups[group_id as usize].zone_index;
                let start = self.zones[zone_index].start;
                self.device.reset_zone(start)?;
                self.zones[zone_index].reset();
                self.free_list.push_back(group_id);
            }
        }
        Ok(true)
    }
}
