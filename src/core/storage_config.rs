// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheRecoverReport {
    pub scanned_files: u64,
    pub recovered_files: u64,
    pub recovered_bytes: u64,
    pub skipped_files: u64,
}

/// Where a value was placed, or that it was not placed at all.
///
/// `Reject` is not a tier. It is the admission policy declining the value, and
/// operations that need a real destination answer it with
/// [`CacheError::UnsupportedTier`]. For where a *read* was served from, which
/// has no such case, see [`CacheReadTier`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheTier {
    Memory,
    #[serde(alias = "kPMEM")]
    Pmem,
    #[serde(alias = "kSSD")]
    Ssd,
    Reject,
}
/// What a single cache instance is backed by.
///
/// `Unified` is not a fourth medium alongside the others -- it spans them. An
/// instance created as `Unified` is therefore not confined to one tier, which
/// matters when reasoning about anything that assumes a single backing store.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CacheInstanceKind {
    #[serde(alias = "kDRAM")]
    Dram = 0,
    #[serde(alias = "kPMEM")]
    Pmem = 1,
    #[serde(alias = "kSSD")]
    Ssd = 2,
    #[serde(alias = "kUnified")]
    Unified = 3,
}

#[allow(non_upper_case_globals)]
impl CacheInstanceKind {
    pub const kDRAM: Self = Self::Dram;
    pub const kPMEM: Self = Self::Pmem;
    pub const kSSD: Self = Self::Ssd;
    pub const kUnified: Self = Self::Unified;
}

impl CacheInstanceKind {
    fn as_tier(self) -> Option<CacheTier> {
        match self {
            CacheInstanceKind::Dram => Some(CacheTier::Memory),
            CacheInstanceKind::Pmem => Some(CacheTier::Pmem),
            CacheInstanceKind::Ssd => Some(CacheTier::Ssd),
            CacheInstanceKind::Unified => None,
        }
    }
}
/// Which storage engine backs a tier.
///
/// `Ssd` is RocksDB, the default (see [`SsdEngineKind`]). `Simple` is the
/// file-backed store used when the `rocksdb-ssd` feature is turned off; it is
/// meant for lightweight local diagnostics rather than production. `MultiSsd`
/// spreads records over several device paths, choosing one by hashing the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StorageEngineKind {
    #[serde(alias = "kDRAM")]
    Dram,
    #[serde(alias = "kPMEM")]
    Pmem,
    #[serde(alias = "kSSD")]
    Ssd,
    #[serde(alias = "kSimple")]
    Simple,
    #[serde(alias = "kMultiSSD")]
    MultiSsd,
}

#[allow(non_upper_case_globals)]
impl StorageEngineKind {
    pub const kDRAM: Self = Self::Dram;
    pub const kPMEM: Self = Self::Pmem;
    pub const kSSD: Self = Self::Ssd;
    pub const kSimple: Self = Self::Simple;
    pub const kMultiSSD: Self = Self::MultiSsd;
}
/// Which key-value engine backs the SSD tier.
///
/// RocksDB is the only one. Kept as an enum so a configuration file can name it
/// and so a second engine could be added without changing the shape of the
/// options, but nothing in this crate branches on the value today.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SsdEngineKind {
    /// Supported Rust Ssd engine.
    #[serde(alias = "kRocksDB")]
    RocksDb = 0,
}

#[allow(non_upper_case_globals)]
impl SsdEngineKind {
    pub const kRocksDB: Self = Self::RocksDb;
}
/// Which staging buffer a record is written into.
///
/// `BufferManager` keeps one buffer per variant and flushes them independently,
/// so the split determines what gets written together: user data, metadata,
/// records rewritten by collection, and codec output are each batched apart from
/// the others.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WriteBufferKind {
    #[serde(alias = "kUserDataBuf")]
    UserDataBuf = 0,
    #[serde(alias = "kMetaDataBuf")]
    MetaDataBuf = 1,
    #[serde(alias = "kGCBuf")]
    GcBuf = 2,
    #[serde(alias = "kCodecDataBuf")]
    CodecDataBuf = 3,
}

#[allow(non_upper_case_globals)]
impl WriteBufferKind {
    pub const kUserDataBuf: Self = Self::UserDataBuf;
    pub const kMetaDataBuf: Self = Self::MetaDataBuf;
    pub const kGCBuf: Self = Self::GcBuf;
    pub const kCodecDataBuf: Self = Self::CodecDataBuf;
}
/// Whether a stored record is payload or part of the metadata log.
///
/// Declared vocabulary: the crate serialises and compares it, but nothing here
/// branches on it. It exists so a configuration or an external tool can name the
/// distinction.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DataKind {
    #[serde(alias = "DATA")]
    Data = 1,
    #[serde(alias = "META_LOG")]
    MetaLog = 2,
}

impl DataKind {
    pub const DATA: Self = Self::Data;
    pub const META_LOG: Self = Self::MetaLog;
}
/// Whether collection is permitted to lose data to make progress.
///
/// Declared vocabulary: nothing in this crate reads it. The collector that
/// exists sweeps tombstones and relocates what is still referenced, which is the
/// `Lossless` behaviour, but it does so unconditionally rather than by
/// consulting this.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GcMode {
    #[serde(alias = "LOSSY")]
    Lossy = 1,
    #[serde(alias = "LOSSLESS")]
    Lossless = 10,
}

impl GcMode {
    pub const LOSSY: Self = Self::Lossy;
    pub const LOSSLESS: Self = Self::Lossless;
}
/// The state bits carried by a stored record.
///
/// `SoftDel` marks a record removed but not yet reclaimed, so a read must treat
/// it as absent while collection still sees it.
///
/// `MaxCode` is not a state. Its value `0xf` is the mask for the state field
/// packed into a colored pointer, so it is used as `state as u64 &
/// RecordState::MaxCode as u64` rather than compared against.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RecordState {
    #[serde(alias = "kSoftDel")]
    SoftDel = 0x0,
    #[serde(alias = "kNormal")]
    Normal = 0x1,
    #[serde(alias = "kPinned")]
    Pinned = 0x2,
    #[serde(alias = "kMaxCode")]
    MaxCode = 0xf,
}

#[allow(non_upper_case_globals)]
impl RecordState {
    pub const kSoftDel: Self = Self::SoftDel;
    pub const kNormal: Self = Self::Normal;
    pub const kPinned: Self = Self::Pinned;
    pub const kMaxCode: Self = Self::MaxCode;
}

/// What the SSD index holds for a key.
///
/// Either a colored pointer -- an address on the SSD tier with the record's
/// [`RecordState`] packed into its low bits -- or the value itself, still
/// resident in memory and not yet written down. `state` answers `None` for the
/// pointer form because the state lives inside the packed word.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SsdIndexValue {
    SsdColoredPtr(u64),
    Memory {
        value: Vec<u8>,
        state: RecordState,
    },
}

impl SsdIndexValue {
    /// The record's state.
    ///
    /// A record on the device carries its state in the low bits of its packed
    /// pointer, so this reports one for both shapes. Reporting `None` for device
    /// records made every state operation below silently skip them.
    pub fn state(&self) -> Option<RecordState> {
        match self {
            Self::SsdColoredPtr(ptr) => Some(decode_colored_ptr_record_state(*ptr)),
            Self::Memory { state, .. } => Some(*state),
        }
    }

    /// The same record in a different state.
    ///
    /// The state field is cleared before the new value is written: setting it by
    /// OR alone cannot move a record back to `SoftDel`, whose encoding is zero.
    pub fn with_state(self, state: RecordState) -> Self {
        match self {
            Self::SsdColoredPtr(ptr) => Self::SsdColoredPtr(
                mask_colored_ptr_record_state(ptr & !SSD_RECORD_STATE_FLAGS, state),
            ),
            Self::Memory { value, .. } => Self::Memory { value, state },
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct SsdIndex {
    entries: Arc<RwLock<HashMap<String, SsdIndexValue>>>,
}


impl SsdIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update_index(&self, key: &str, value: SsdIndexValue) -> bool {
        let mut entries = self.entries.write().expect("ssd index lock poisoned");
        let Some(current) = entries.get(key).cloned() else {
            return false;
        };
        // An update repoints a record; it does not restate it. The state comes
        // from the entry already here, whichever shape the new value lands in --
        // otherwise moving a record to the device would silently adopt whatever
        // state the fresh pointer happened to carry.
        let value = match current.state() {
            Some(state) => value.with_state(state),
            None => value,
        };
        entries.insert(key.to_string(), value);
        true
    }

    pub fn put(&self, key: impl Into<String>, value: SsdIndexValue) {
        self.entries
            .write()
            .expect("ssd index lock poisoned")
            .insert(key.into(), value);
    }

    pub fn get(&self, key: &str) -> Option<SsdIndexValue> {
        let mut entries = self.entries.write().expect("ssd index lock poisoned");
        let value = entries.get(key)?.clone();
        if value.state() == Some(RecordState::SoftDel) {
            let promoted = value.with_state(RecordState::Normal);
            entries.insert(key.to_string(), promoted.clone());
            return Some(promoted);
        }
        Some(value)
    }

    pub fn unpin(&self, key: &str) {
        let mut entries = self.entries.write().expect("ssd index lock poisoned");
        let Some(value) = entries.get(key).cloned() else {
            return;
        };
        if value.state() == Some(RecordState::Pinned) {
            entries.insert(key.to_string(), value.with_state(RecordState::Normal));
        }
    }

    pub fn pin(&self, key: &str) -> bool {
        let mut entries = self.entries.write().expect("ssd index lock poisoned");
        let Some(value) = entries.get(key).cloned() else {
            return false;
        };
        if value.state() == Some(RecordState::Pinned) {
            return false;
        }
        entries.insert(key.to_string(), value.with_state(RecordState::Pinned));
        true
    }

    pub fn soft_delete(&self, key: &str) {
        let mut entries = self.entries.write().expect("ssd index lock poisoned");
        let Some(value) = entries.get(key).cloned() else {
            return;
        };
        if value.state() != Some(RecordState::Pinned) {
            entries.insert(key.to_string(), value.with_state(RecordState::SoftDel));
        }
    }

    pub fn delete_if<F>(&self, key: &str, pred: F) -> bool
    where
        F: FnOnce(RecordState) -> bool,
    {
        let mut entries = self.entries.write().expect("ssd index lock poisoned");
        let state = entries.get(key).and_then(SsdIndexValue::state);
        if state.is_some_and(pred) {
            entries.remove(key);
            true
        } else {
            false
        }
    }

    pub fn scan_index_for_recover<F>(&self, mut func: F)
    where
        F: FnMut(&str, &SsdIndexValue),
    {
        let entries = self.entries.read().expect("ssd index lock poisoned");
        for (key, value) in entries.iter() {
            func(key, value);
        }
    }

    #[allow(non_snake_case)]
    pub fn UpdateIndex(&self, key: &str, value: SsdIndexValue) -> bool {
        self.update_index(key, value)
    }

    #[allow(non_snake_case)]
    pub fn Put(&self, key: impl Into<String>, value: SsdIndexValue) {
        self.put(key, value);
    }

    #[allow(non_snake_case)]
    pub fn Get(&self, key: &str) -> Option<SsdIndexValue> {
        self.get(key)
    }

    #[allow(non_snake_case)]
    pub fn UnPin(&self, key: &str) {
        self.unpin(key);
    }

    #[allow(non_snake_case)]
    pub fn Pin(&self, key: &str) -> bool {
        self.pin(key)
    }

    #[allow(non_snake_case)]
    pub fn SoftDelete(&self, key: &str) {
        self.soft_delete(key);
    }

    #[allow(non_snake_case)]
    pub fn DeleteIf<F>(&self, key: &str, pred: F) -> bool
    where
        F: FnOnce(RecordState) -> bool,
    {
        self.delete_if(key, pred)
    }

    #[allow(non_snake_case)]
    pub fn ScanIndexForRecover<F>(&self, func: F)
    where
        F: FnMut(&str, &SsdIndexValue),
    {
        self.scan_index_for_recover(func);
    }
}

#[derive(Debug, Clone)]
pub struct IndexUpdater {
    index: Arc<SsdIndex>,
}

impl IndexUpdater {
    pub fn new(index: Arc<SsdIndex>) -> Self {
        Self { index }
    }

    pub fn delete_if<F>(&self, key: &str, pred: F) -> bool
    where
        F: FnOnce(RecordState) -> bool,
    {
        self.index.delete_if(key, pred)
    }

    pub fn get(&self, key: &str) -> Option<SsdIndexValue> {
        self.index.get(key)
    }

    pub fn update_index(&self, key: &str, value: SsdIndexValue) -> bool {
        self.index.update_index(key, value)
    }

    #[allow(non_snake_case)]
    pub fn DeleteIf<F>(&self, key: &str, pred: F) -> bool
    where
        F: FnOnce(RecordState) -> bool,
    {
        self.delete_if(key, pred)
    }

    #[allow(non_snake_case)]
    pub fn Get(&self, key: &str) -> Option<SsdIndexValue> {
        self.get(key)
    }

    #[allow(non_snake_case)]
    pub fn UpdateIndex(&self, key: &str, value: SsdIndexValue) -> bool {
        self.update_index(key, value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WriteBufferRecord {
    pub key: String,
    pub value: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteBuffer {
    buf_type: WriteBufferKind,
    capacity: u32,
    records: Vec<WriteBufferRecord>,
}

impl WriteBuffer {
    pub fn new(buf_type: WriteBufferKind, capacity: u32) -> Self {
        Self {
            buf_type,
            capacity,
            records: Vec::new(),
        }
    }

    pub fn push_back(&mut self, key: impl Into<String>, value: impl Into<Vec<u8>>) {
        self.records.push(WriteBufferRecord {
            key: key.into(),
            value: value.into(),
        });
    }

    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    pub fn size(&self) -> u32 {
        self.key_size().saturating_add(self.value_size())
    }

    pub fn count(&self) -> u32 {
        self.records.len().min(u32::MAX as usize) as u32
    }

    pub fn buf_type(&self) -> WriteBufferKind {
        self.buf_type
    }

    pub fn key_size(&self) -> u32 {
        self.records
            .iter()
            .map(|record| record.key.len() as u32)
            .sum()
    }

    pub fn value_size(&self) -> u32 {
        self.records
            .iter()
            .map(|record| record.value.len() as u32)
            .sum()
    }

    pub fn records(&self) -> &[WriteBufferRecord] {
        &self.records
    }

    pub fn steal_buf_q(&mut self) -> Vec<WriteBufferRecord> {
        std::mem::take(&mut self.records)
    }

    #[allow(non_snake_case)]
    pub fn PushBack(&mut self, key: impl Into<String>, value: impl Into<Vec<u8>>) {
        self.push_back(key, value);
    }

    #[allow(non_snake_case)]
    pub fn Capacity(&self) -> u32 {
        self.capacity()
    }

    #[allow(non_snake_case)]
    pub fn Size(&self) -> u32 {
        self.size()
    }

    #[allow(non_snake_case)]
    pub fn Count(&self) -> u32 {
        self.count()
    }

    #[allow(non_snake_case)]
    pub fn BufType(&self) -> WriteBufferKind {
        self.buf_type()
    }

    #[allow(non_snake_case)]
    pub fn KeySize(&self) -> u32 {
        self.key_size()
    }

    #[allow(non_snake_case)]
    pub fn ValueSize(&self) -> u32 {
        self.value_size()
    }

    #[allow(non_snake_case)]
    pub fn StealBufQ(&mut self) -> Vec<WriteBufferRecord> {
        self.steal_buf_q()
    }
}

impl Default for WriteBuffer {
    fn default() -> Self {
        Self::new(WriteBufferKind::UserDataBuf, 10_485_760)
    }
}

pub fn put_fixed_uint8(buf: &mut Vec<u8>, num: u8) -> usize {
    buf.push(num);
    buf.len()
}

pub fn put_fixed_uint32(buf: &mut Vec<u8>, num: u32) -> usize {
    buf.extend_from_slice(&num.to_le_bytes());
    buf.len()
}

pub fn put_fixed_uint64(buf: &mut Vec<u8>, num: u64) -> usize {
    buf.extend_from_slice(&num.to_le_bytes());
    buf.len()
}

pub fn put_fixed_hash64(buf: &mut Vec<u8>, num: u64) -> usize {
    put_fixed_uint64(buf, num)
}

pub fn put_fixed_hash128(buf: &mut Vec<u8>, hash: Xxh128) -> usize {
    put_fixed_uint64(buf, hash.first);
    put_fixed_uint64(buf, hash.second)
}

pub fn get_fixed_uint8(buf: &[u8], offset: usize) -> Option<(u8, usize)> {
    let value = *buf.get(offset)?;
    Some((value, offset + 1))
}

pub fn get_fixed_uint32(buf: &[u8], offset: usize) -> Option<(u32, usize)> {
    let end = offset.checked_add(4)?;
    Some((
        u32::from_le_bytes(buf.get(offset..end)?.try_into().ok()?),
        end,
    ))
}

pub fn get_fixed_uint64(buf: &[u8], offset: usize) -> Option<(u64, usize)> {
    let end = offset.checked_add(8)?;
    Some((
        u64::from_le_bytes(buf.get(offset..end)?.try_into().ok()?),
        end,
    ))
}

pub fn get_fixed_hash64(buf: &[u8], offset: usize) -> Option<(u64, usize)> {
    get_fixed_uint64(buf, offset)
}

pub fn get_fixed_hash128(buf: &[u8], offset: usize) -> Option<(Xxh128, usize)> {
    let (first, offset) = get_fixed_uint64(buf, offset)?;
    let (second, offset) = get_fixed_uint64(buf, offset)?;
    Some((Xxh128 { first, second }, offset))
}

pub fn aligned_to(size: u32, align_size: usize) -> u32 {
    if align_size == 0 {
        return size;
    }
    let align_size = align_size.min(u32::MAX as usize) as u32;
    let rem = size % align_size;
    if rem == 0 {
        size
    } else {
        size.saturating_add(align_size - rem)
    }
}

pub fn copy_bytes_to(dst: &mut Vec<u8>, src: &[u8]) -> usize {
    dst.extend_from_slice(src);
    dst.len()
}

pub fn copy_bytes_from(src: &[u8], offset: usize, len: usize) -> Option<(Vec<u8>, usize)> {
    let end = offset.checked_add(len)?;
    Some((src.get(offset..end)?.to_vec(), end))
}

pub const SSD_MEMORY_ADDR_FLAGS: u64 = 0x0000_ffff_ffff_ffff;
pub const SSD_RECORD_STATE_FLAGS: u64 = 0x0000_0000_0000_000f;
pub const SSD_RECORD_ALIGN_SIZE: i32 = 4096;

pub fn decode_colored_ptr(colored_ptr: u64) -> (u32, u64) {
    let size = ((colored_ptr >> 7) & 0xfff) as u32;
    let lba = colored_ptr >> 19;
    (size, lba)
}

pub fn mask_colored_ptr_memory_address(old_colored_ptr: u64, address: u64) -> u64 {
    old_colored_ptr | (address & SSD_MEMORY_ADDR_FLAGS)
}

pub fn mask_colored_ptr_lba(old_colored_ptr: u64, lba: u64) -> u64 {
    old_colored_ptr | ((lba << 19) & 0xffff_ffff_fff8_0000)
}

pub fn mask_colored_ptr_size(old_colored_ptr: u64, size: u32) -> u64 {
    old_colored_ptr | (((size as u64) & 0xfff) << 7)
}

pub fn mask_colored_ptr_record_state(old_colored_ptr: u64, state: RecordState) -> u64 {
    old_colored_ptr | ((state as u64) & (RecordState::MaxCode as u64))
}

/// Reads a record state back out of a packed pointer.
///
/// The field is four bits wide and three of its sixteen values are named; an
/// unnamed one reads as `Normal`, because a record that exists is more usefully
/// treated as live than as corrupt.
pub fn decode_colored_ptr_record_state(colored_ptr: u64) -> RecordState {
    match colored_ptr & SSD_RECORD_STATE_FLAGS {
        0x0 => RecordState::SoftDel,
        0x2 => RecordState::Pinned,
        0xf => RecordState::MaxCode,
        _ => RecordState::Normal,
    }
}

#[allow(non_snake_case)]
pub fn PutFixedUint8(buf: &mut Vec<u8>, num: u8) -> usize {
    put_fixed_uint8(buf, num)
}

#[allow(non_snake_case)]
pub fn PutFixedUint32(buf: &mut Vec<u8>, num: u32) -> usize {
    put_fixed_uint32(buf, num)
}

#[allow(non_snake_case)]
pub fn PutFixedUint64(buf: &mut Vec<u8>, num: u64) -> usize {
    put_fixed_uint64(buf, num)
}

#[allow(non_snake_case)]
pub fn PutFixedHash64(buf: &mut Vec<u8>, num: u64) -> usize {
    put_fixed_hash64(buf, num)
}

#[allow(non_snake_case)]
pub fn PutFixedHash128(buf: &mut Vec<u8>, hash: Xxh128) -> usize {
    put_fixed_hash128(buf, hash)
}

#[allow(non_snake_case)]
pub fn GetFixedUint8(buf: &[u8], offset: usize) -> Option<(u8, usize)> {
    get_fixed_uint8(buf, offset)
}

#[allow(non_snake_case)]
pub fn GetFixedUint32(buf: &[u8], offset: usize) -> Option<(u32, usize)> {
    get_fixed_uint32(buf, offset)
}

#[allow(non_snake_case)]
pub fn GetFixedUint64(buf: &[u8], offset: usize) -> Option<(u64, usize)> {
    get_fixed_uint64(buf, offset)
}

#[allow(non_snake_case)]
pub fn GetFixedHash64(buf: &[u8], offset: usize) -> Option<(u64, usize)> {
    get_fixed_hash64(buf, offset)
}

#[allow(non_snake_case)]
pub fn GetFixedHash128(buf: &[u8], offset: usize) -> Option<(Xxh128, usize)> {
    get_fixed_hash128(buf, offset)
}

#[allow(non_snake_case)]
pub fn AlignedTo(size: u32, align_size: usize) -> u32 {
    aligned_to(size, align_size)
}

#[allow(non_snake_case)]
pub fn CopyBytesTo(dst: &mut Vec<u8>, src: &[u8]) -> usize {
    copy_bytes_to(dst, src)
}

#[allow(non_snake_case)]
pub fn CopyBytesFrom(src: &[u8], offset: usize, len: usize) -> Option<(Vec<u8>, usize)> {
    copy_bytes_from(src, offset, len)
}

#[allow(non_snake_case)]
pub fn DecodeColoredPtr(colored_ptr: u64) -> (u32, u64) {
    decode_colored_ptr(colored_ptr)
}

#[allow(non_snake_case)]
pub fn MaskColoredPtrMemoryAddress(old_colored_ptr: u64, address: u64) -> u64 {
    mask_colored_ptr_memory_address(old_colored_ptr, address)
}

#[allow(non_snake_case)]
pub fn MaskColoredPtrLBA(old_colored_ptr: u64, lba: u64) -> u64 {
    mask_colored_ptr_lba(old_colored_ptr, lba)
}

#[allow(non_snake_case)]
pub fn MaskColoredPtrSize(old_colored_ptr: u64, size: u32) -> u64 {
    mask_colored_ptr_size(old_colored_ptr, size)
}

#[allow(non_snake_case)]
pub fn MaskColoredPtrRecordState(old_colored_ptr: u64, state: RecordState) -> u64 {
    mask_colored_ptr_record_state(old_colored_ptr, state)
}

fn hash_bytes_with_seed(value: &[u8], seed: u64) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    seed.hash(&mut hasher);
    value.hash(&mut hasher);
    hasher.finish()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Xxh128 {
    pub first: u64,
    pub second: u64,
}

#[derive(Debug, Clone)]
pub struct BufferEncoder {
    align_size: usize,
    xxh_seed: u64,
}

impl BufferEncoder {
    pub const DATA_FIXED_PART_SIZE: u32 = 20;
    pub const OPLOG_FIXED_PART_SIZE: u32 = 28;
    pub const OPLOG_HEADER_SIZE: u32 = 8;

    pub fn new(_buf_size: usize) -> Self {
        Self {
            align_size: SSD_RECORD_ALIGN_SIZE as usize,
            xxh_seed: 0,
        }
    }

    pub fn align_size(&self) -> usize {
        self.align_size
    }

    pub fn xxh_seed(&self) -> u64 {
        self.xxh_seed
    }

    pub fn calculate_encoded_oplog_size(&self, buffer: &WriteBuffer) -> u32 {
        buffer
            .key_size()
            .saturating_add(Self::OPLOG_FIXED_PART_SIZE.saturating_mul(buffer.count()))
    }

    pub fn calculate_encoded_data_size(&self, buffer: &WriteBuffer) -> u32 {
        buffer
            .value_size()
            .saturating_add(Self::DATA_FIXED_PART_SIZE.saturating_mul(buffer.count()))
    }

    pub fn serialize_data(&self, value: &[u8]) -> Vec<u8> {
        let mut encoded = Vec::with_capacity(Self::DATA_FIXED_PART_SIZE as usize + value.len());
        encoded.resize(16, 0);
        put_fixed_uint32(&mut encoded, value.len() as u32);
        copy_bytes_to(&mut encoded, value);
        let hash = self.hash128(&encoded[16..]);
        encoded[0..8].copy_from_slice(&hash.first.to_le_bytes());
        encoded[8..16].copy_from_slice(&hash.second.to_le_bytes());
        encoded
    }

    pub fn deserialize_data(&self, src: &[u8]) -> (Vec<u8>, bool) {
        if src.len() < Self::DATA_FIXED_PART_SIZE as usize {
            return (Vec::new(), true);
        }
        let Some((expected, offset)) = get_fixed_hash128(src, 0) else {
            return (Vec::new(), true);
        };
        let Some((length, offset)) = get_fixed_uint32(src, offset) else {
            return (Vec::new(), true);
        };
        let length = length as usize;
        let Some(end) = (Self::DATA_FIXED_PART_SIZE as usize).checked_add(length) else {
            return (Vec::new(), true);
        };
        if src.len() < end {
            return (Vec::new(), true);
        }
        let Some((value, _)) = copy_bytes_from(src, offset, length) else {
            return (Vec::new(), true);
        };
        let actual = self.hash128(&src[16..end]);
        (value, actual != expected)
    }

    pub fn serialize_oplog<F>(
        &self,
        records: &[WriteBufferRecord],
        mut update_entry_callback: F,
        mut batch_begin_offset: u64,
        oplog_size: u32,
    ) -> Vec<u8>
    where
        F: FnMut(&str, SsdIndexValue) -> bool,
    {
        let mut encoded = Vec::with_capacity(oplog_size as usize);
        for record in records {
            let key_len = record.key.len() as u32;
            let value_len = record.value.len() as u32;
            let record_size = Self::DATA_FIXED_PART_SIZE.saturating_add(value_len);
            let record_units = aligned_to(record_size, self.align_size) / self.align_size as u32;
            let mut colored_ptr = 0;
            colored_ptr = mask_colored_ptr_record_state(colored_ptr, RecordState::SoftDel);
            colored_ptr = mask_colored_ptr_lba(colored_ptr, batch_begin_offset);
            colored_ptr = mask_colored_ptr_size(colored_ptr, record_units);
            update_entry_callback(&record.key, SsdIndexValue::SsdColoredPtr(colored_ptr));

            let entry_start = encoded.len();
            encoded.resize(entry_start + 16, 0);
            put_fixed_uint32(&mut encoded, key_len);
            copy_bytes_to(&mut encoded, record.key.as_bytes());
            put_fixed_uint64(&mut encoded, colored_ptr);
            let hash = self.hash128(&encoded[entry_start + 16..]);
            encoded[entry_start..entry_start + 8].copy_from_slice(&hash.first.to_le_bytes());
            encoded[entry_start + 8..entry_start + 16].copy_from_slice(&hash.second.to_le_bytes());
            batch_begin_offset = batch_begin_offset
                .saturating_add(Self::DATA_FIXED_PART_SIZE as u64 + value_len as u64);
        }
        encoded
    }

    pub fn deserialize_oplog(&self, src: &[u8]) -> (String, u64, usize, bool) {
        if src.len() < Self::OPLOG_FIXED_PART_SIZE as usize {
            return (String::new(), 0, 0, true);
        }
        let Some((expected, offset)) = get_fixed_hash128(src, 0) else {
            return (String::new(), 0, 0, true);
        };
        let Some((key_len, offset)) = get_fixed_uint32(src, offset) else {
            return (String::new(), 0, 0, true);
        };
        let Some((key_bytes, offset)) = copy_bytes_from(src, offset, key_len as usize) else {
            return (String::new(), 0, 0, true);
        };
        let Some((offset_value, next_offset)) = get_fixed_uint64(src, offset) else {
            return (String::new(), 0, 0, true);
        };
        let actual = self.hash128(&src[16..next_offset]);
        let key = String::from_utf8_lossy(&key_bytes).into_owned();
        (key, offset_value, next_offset, actual != expected)
    }

    fn hash128(&self, value: &[u8]) -> Xxh128 {
        let first = hash_bytes_with_seed(value, self.xxh_seed);
        let second = 0;
        Xxh128 { first, second }
    }

    #[allow(non_snake_case)]
    pub fn CalculateEncodedOpLogSize(&self, buffer: &WriteBuffer) -> u32 {
        self.calculate_encoded_oplog_size(buffer)
    }

    #[allow(non_snake_case)]
    pub fn CalculateEncodedDataSize(&self, buffer: &WriteBuffer) -> u32 {
        self.calculate_encoded_data_size(buffer)
    }

    #[allow(non_snake_case)]
    pub fn SerializeData(&self, value: &[u8]) -> Vec<u8> {
        self.serialize_data(value)
    }

    #[allow(non_snake_case)]
    pub fn DeserializeData(&self, src: &[u8]) -> (Vec<u8>, bool) {
        self.deserialize_data(src)
    }

    #[allow(non_snake_case)]
    pub fn SerializeOplog<F>(
        &self,
        records: &[WriteBufferRecord],
        update_entry_callback: F,
        batch_begin_offset: u64,
        oplog_size: u32,
    ) -> Vec<u8>
    where
        F: FnMut(&str, SsdIndexValue) -> bool,
    {
        self.serialize_oplog(records, update_entry_callback, batch_begin_offset, oplog_size)
    }

    #[allow(non_snake_case)]
    pub fn DeserializeOplog(&self, src: &[u8]) -> (String, u64, usize, bool) {
        self.deserialize_oplog(src)
    }

    #[allow(non_snake_case)]
    pub fn GetXXHSeed(&self) -> u64 {
        self.xxh_seed()
    }
}

#[derive(Debug, Clone)]
pub struct BufferManager {
    write_enabled: bool,
    buffers: HashMap<WriteBufferKind, Vec<WriteBufferRecord>>,
    flushed: Vec<WriteBufferRecord>,
    capacity_per_buf: u32,
    flush_threshold: f64,
    flush_size: usize,
}

impl Default for BufferManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BufferManager {
    pub fn new() -> Self {
        Self {
            write_enabled: false,
            buffers: HashMap::new(),
            flushed: Vec::new(),
            capacity_per_buf: 1 << 21,
            flush_threshold: 0.8,
            flush_size: 10,
        }
    }

    pub fn with_config(capacity_per_buf: u32, flush_threshold: f64, flush_size: usize) -> Self {
        Self {
            capacity_per_buf,
            flush_threshold,
            flush_size,
            ..Self::new()
        }
    }

    pub fn put(
        &mut self,
        value: (impl Into<String>, impl Into<Vec<u8>>),
        buffer_type: WriteBufferKind,
    ) -> Result<(), CacheError> {
        if !self.write_enabled {
            return Err(CacheError::Stopped);
        }
        self.buffers
            .entry(buffer_type)
            .or_default()
            .push(WriteBufferRecord {
                key: value.0.into(),
                value: value.1.into(),
            });
        Ok(())
    }

    pub fn start(&mut self) {
        self.write_enabled = true;
    }

    pub fn stop(&mut self) {
        self.write_enabled = false;
    }

    pub fn write_enabled(&self) -> bool {
        self.write_enabled
    }

    pub fn set_write_enabled(&mut self, status: bool) {
        self.write_enabled = status;
    }

    pub fn flush_buffers(&mut self) -> usize {
        let mut flushed = 0;
        for records in self.buffers.values_mut() {
            flushed += records.len();
            self.flushed.append(records);
        }
        flushed
    }

    pub fn codec_buffers(&self) -> usize {
        self.buffers.values().map(Vec::len).sum()
    }

    pub fn flush_buffer(&mut self, mut buffer: WriteBuffer) -> usize {
        let records = buffer.steal_buf_q();
        let count = records.len();
        self.flushed.extend(records);
        count
    }

    pub fn codec_buffer(&self, buffer: &WriteBuffer, encoder: &BufferEncoder) -> (u32, u32) {
        (
            encoder.calculate_encoded_data_size(buffer),
            encoder.calculate_encoded_oplog_size(buffer),
        )
    }

    pub fn buffered_count(&self, buffer_type: WriteBufferKind) -> usize {
        self.buffers.get(&buffer_type).map_or(0, Vec::len)
    }

    pub fn flushed_records(&self) -> &[WriteBufferRecord] {
        &self.flushed
    }

    pub fn capacity_per_buf(&self) -> u32 {
        self.capacity_per_buf
    }

    pub fn flush_threshold(&self) -> f64 {
        self.flush_threshold
    }

    pub fn flush_size(&self) -> usize {
        self.flush_size
    }

    #[allow(non_snake_case)]
    pub fn Put(
        &mut self,
        value: (impl Into<String>, impl Into<Vec<u8>>),
        buffer_type: WriteBufferKind,
    ) -> Result<(), CacheError> {
        self.put(value, buffer_type)
    }

    #[allow(non_snake_case)]
    pub fn Start(&mut self) {
        self.start();
    }

    #[allow(non_snake_case)]
    pub fn Stop(&mut self) {
        self.stop();
    }

    #[allow(non_snake_case)]
    pub fn WriteEnabled(&self) -> bool {
        self.write_enabled()
    }

    #[allow(non_snake_case)]
    pub fn SetWriteEnabled(&mut self, status: bool) {
        self.set_write_enabled(status);
    }

    #[allow(non_snake_case)]
    pub fn FlushBuffers(&mut self) -> usize {
        self.flush_buffers()
    }

    #[allow(non_snake_case)]
    pub fn CodecBuffers(&self) -> usize {
        self.codec_buffers()
    }

    #[allow(non_snake_case)]
    pub fn FlushBuffer(&mut self, buffer: WriteBuffer) -> usize {
        self.flush_buffer(buffer)
    }

    #[allow(non_snake_case)]
    pub fn CodecBuffer(&self, buffer: &WriteBuffer, encoder: &BufferEncoder) -> (u32, u32) {
        self.codec_buffer(buffer, encoder)
    }
}

impl SsdEngineKind {
    pub fn from_config_name(value: &str) -> Self {
        if value.eq_ignore_ascii_case("rocksdb")
            || value.eq_ignore_ascii_case("rocks_db")
            || value.eq_ignore_ascii_case("kRocksDB")
            || value.eq_ignore_ascii_case("kSSDRocksDBStorageEngine")
        {
            Self::RocksDb
        } else {
            Self::RocksDb
        }
    }

    pub fn as_config_name(self) -> &'static str {
        match self {
            Self::RocksDb => "RocksDB",
        }
    }

    #[allow(non_snake_case)]
    pub fn FromConfigName(value: &str) -> Self {
        Self::from_config_name(value)
    }

    #[allow(non_snake_case)]
    pub fn AsConfigName(self) -> &'static str {
        self.as_config_name()
    }
}

impl StorageEngineKind {
    pub fn from_config_name(value: &str) -> Self {
        if value.eq_ignore_ascii_case("pmem")
            || value.eq_ignore_ascii_case("persistent_memory")
            || value.eq_ignore_ascii_case("persistent-memory")
            || value.eq_ignore_ascii_case("kPMEMStorageEngine")
            || value.eq_ignore_ascii_case("kPMEM")
        {
            Self::Pmem
        } else if value.eq_ignore_ascii_case("ssd")
            || value.eq_ignore_ascii_case("rocksdb")
            || value.eq_ignore_ascii_case("rocks_db")
            || value.eq_ignore_ascii_case("kRocksDB")
            || value.eq_ignore_ascii_case("kSSDRocksDBStorageEngine")
            || value.eq_ignore_ascii_case("kSSD")
        {
            Self::Ssd
        } else if value.eq_ignore_ascii_case("simple")
            || value.eq_ignore_ascii_case("simple_storage")
            || value.eq_ignore_ascii_case("kSimpleStorageEngine")
        {
            Self::Simple
        } else if value.eq_ignore_ascii_case("multi_ssd")
            || value.eq_ignore_ascii_case("multi-ssd")
            || value.eq_ignore_ascii_case("kMultiSSDStorageEngine")
        {
            Self::MultiSsd
        } else if value.eq_ignore_ascii_case("dram")
            || value.eq_ignore_ascii_case("kDRAM")
            || value.eq_ignore_ascii_case("kDRAMStorageEngine")
        {
            Self::Dram
        } else {
            Self::Dram
        }
    }

    pub fn from_config_code(value: u8) -> Self {
        match value {
            1 => Self::Pmem,
            2 => Self::Ssd,
            3 => Self::Simple,
            4 => Self::MultiSsd,
            _ => Self::Dram,
        }
    }

    pub fn config_code(self) -> u8 {
        match self {
            Self::Dram => 0,
            Self::Pmem => 1,
            Self::Ssd => 2,
            Self::Simple => 3,
            Self::MultiSsd => 4,
        }
    }

    pub fn is_ssd_like(self) -> bool {
        matches!(self, Self::Ssd | Self::MultiSsd)
    }

    pub fn canonical_instance_type(self) -> CacheInstanceKind {
        match self {
            Self::Dram | Self::Simple => CacheInstanceKind::Dram,
            Self::Pmem => CacheInstanceKind::Pmem,
            Self::Ssd | Self::MultiSsd => CacheInstanceKind::Ssd,
        }
    }

    pub fn as_config_enum_name(self) -> &'static str {
        match self {
            Self::Dram => "kDRAMStorageEngine",
            Self::Pmem => "kPMEMStorageEngine",
            Self::Ssd => "kSSDRocksDBStorageEngine",
            Self::Simple => "kSimpleStorageEngine",
            Self::MultiSsd => "kMultiSSDStorageEngine",
        }
    }

    pub fn as_config_name(self) -> &'static str {
        match self {
            Self::Dram => "DRAM",
            Self::Pmem => "PMEM",
            Self::Ssd => "SSD",
            Self::Simple => "Simple",
            Self::MultiSsd => "MultiSSD",
        }
    }

    fn as_instance_type(self) -> CacheInstanceKind {
        self.canonical_instance_type()
    }

    #[allow(non_snake_case)]
    pub fn FromConfigCode(value: u8) -> Self {
        Self::from_config_code(value)
    }

    #[allow(non_snake_case)]
    pub fn ConfigCode(self) -> u8 {
        self.config_code()
    }

    #[allow(non_snake_case)]
    pub fn IsSsdLike(self) -> bool {
        self.is_ssd_like()
    }

    #[allow(non_snake_case)]
    pub fn AsConfigEnumName(self) -> &'static str {
        self.as_config_enum_name()
    }

    #[allow(non_snake_case)]
    pub fn AsConfigName(self) -> &'static str {
        self.as_config_name()
    }
}
/// The replacement policy named by a configuration value.
///
/// Wider than [`CacheReplacementPolicy`], which is what the crate implements:
/// `Lru` and the `MaxCode` sentinel both map onto `WeightedHotnessLru`. Accept
/// this at a configuration boundary and convert inward; match on
/// [`CacheReplacementPolicy`] when deciding behaviour.
///
/// `MaxCode` marks the end of the numeric range rather than naming a policy --
/// a C-style sentinel kept so serialized codes stay stable.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReplacementPolicyKind {
    #[serde(alias = "kFIFO")]
    Fifo = 0,
    #[serde(alias = "kLRU")]
    Lru = 1,
    #[serde(alias = "kSLRU")]
    Slru = 2,
    #[serde(alias = "kWeightedHotnessLru")]
    WeightedHotnessLru = 3,
    #[serde(alias = "kMaxCode")]
    MaxCode = 4,
}

#[allow(non_upper_case_globals)]
impl ReplacementPolicyKind {
    pub const kFIFO: Self = Self::Fifo;
    pub const kLRU: Self = Self::Lru;
    pub const kSLRU: Self = Self::Slru;
    pub const kWeightedHotnessLru: Self = Self::WeightedHotnessLru;
    pub const kMaxCode: Self = Self::MaxCode;
}

impl ReplacementPolicyKind {
    pub fn from_config_name(value: &str) -> Self {
        if value.eq_ignore_ascii_case("fifo") || value.eq_ignore_ascii_case("kFIFO") {
            Self::Fifo
        } else if value.eq_ignore_ascii_case("slru") || value.eq_ignore_ascii_case("kSLRU") {
            Self::Slru
        } else if value.eq_ignore_ascii_case("lru") || value.eq_ignore_ascii_case("kLRU") {
            Self::Lru
        } else if value.eq_ignore_ascii_case("kMaxCode") {
            Self::MaxCode
        } else {
            Self::WeightedHotnessLru
        }
    }

    pub fn as_config_name(self) -> &'static str {
        match self {
            Self::Fifo => "FIFO",
            Self::Slru => "SLRU",
            Self::Lru => "LRU",
            Self::WeightedHotnessLru => "WeightedHotnessLru",
            Self::MaxCode => "MaxCode",
        }
    }

    fn as_cache_policy(self) -> CacheReplacementPolicy {
        match self {
            ReplacementPolicyKind::Fifo => CacheReplacementPolicy::Fifo,
            ReplacementPolicyKind::Slru => CacheReplacementPolicy::Slru,
            ReplacementPolicyKind::Lru => CacheReplacementPolicy::WeightedHotnessLru,
            ReplacementPolicyKind::WeightedHotnessLru => {
                CacheReplacementPolicy::WeightedHotnessLru
            }
            ReplacementPolicyKind::MaxCode => CacheReplacementPolicy::WeightedHotnessLru,
        }
    }
}

/// How a value is divided between the DRAM and persistent-memory tiers.
///
/// `SideBySide` treats them as two independent pools, each holding whatever it
/// admits; `Tiered` treats DRAM as the front of a single hierarchy that falls
/// through to PMEM. This changes what `size` and `capacity` mean: side-by-side
/// adds the two tiers together, tiered takes the larger of them.
///
/// `Tiered` is the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheDataPlacement {
    #[serde(alias = "kSideBySide")]
    SideBySide,
    #[serde(alias = "kTiered")]
    Tiered,
}

impl CacheDataPlacement {
    pub fn try_from_config_name(value: &str) -> Result<Self, CacheError> {
        if value.eq_ignore_ascii_case("sidebyside")
            || value.eq_ignore_ascii_case("side_by_side")
            || value.eq_ignore_ascii_case("side-by-side")
            || value.eq_ignore_ascii_case("kSideBySide")
            || value == "SideBySide"
        {
            Ok(Self::SideBySide)
        } else if value.eq_ignore_ascii_case("tiered")
            || value.eq_ignore_ascii_case("kTiered")
            || value == "Tiered"
        {
            Ok(Self::Tiered)
        } else {
            Err(CacheError::InvalidConfig(format!(
                "Invalid DRAM PMEM data placement type: {value}"
            )))
        }
    }

    pub fn from_config_name(value: &str) -> Self {
        if value.eq_ignore_ascii_case("sidebyside")
            || value.eq_ignore_ascii_case("side_by_side")
            || value.eq_ignore_ascii_case("side-by-side")
        {
            Self::SideBySide
        } else {
            Self::Tiered
        }
    }

    pub fn as_config_name(self) -> &'static str {
        match self {
            Self::SideBySide => "SideBySide",
            Self::Tiered => "Tiered",
        }
    }
}
/// The configuration-facing form of [`CacheDataPlacement`].
///
/// The same choice between side-by-side and tiered, plus a `MaxCode` sentinel
/// marking the end of the numeric range. Converts both ways with `From`;
/// `MaxCode` collapses to `Tiered` on the way in, since it names no placement.
///
/// Accept this where a configuration value arrives and convert to
/// [`CacheDataPlacement`] before deciding anything.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DramPmemDataPlacement {
    #[serde(alias = "kSideBySide")]
    SideBySide = 0,
    #[serde(alias = "kTiered")]
    Tiered = 1,
    #[serde(alias = "kMaxCode")]
    MaxCode = 2,
}

#[allow(non_upper_case_globals)]
impl DramPmemDataPlacement {
    pub const kSideBySide: Self = Self::SideBySide;
    pub const kTiered: Self = Self::Tiered;
    pub const kMaxCode: Self = Self::MaxCode;
}

impl DramPmemDataPlacement {
    pub fn try_from_config_name(value: &str) -> Result<Self, CacheError> {
        Ok(match CacheDataPlacement::try_from_config_name(value)? {
            CacheDataPlacement::SideBySide => Self::SideBySide,
            CacheDataPlacement::Tiered => Self::Tiered,
        })
    }

    pub fn from_config_name(value: &str) -> Self {
        if value.eq_ignore_ascii_case("sidebyside")
            || value.eq_ignore_ascii_case("side_by_side")
            || value.eq_ignore_ascii_case("side-by-side")
            || value.eq_ignore_ascii_case("kSideBySide")
        {
            Self::SideBySide
        } else if value.eq_ignore_ascii_case("kMaxCode") {
            Self::MaxCode
        } else {
            Self::Tiered
        }
    }

    pub fn as_config_name(self) -> &'static str {
        match self {
            Self::SideBySide => "SideBySide",
            Self::Tiered => "Tiered",
            Self::MaxCode => "MaxCode",
        }
    }

    pub fn as_cache_data_placement(self) -> CacheDataPlacement {
        match self {
            Self::SideBySide => CacheDataPlacement::SideBySide,
            Self::Tiered | Self::MaxCode => CacheDataPlacement::Tiered,
        }
    }

    pub fn from_cache_data_placement(placement: CacheDataPlacement) -> Self {
        match placement {
            CacheDataPlacement::SideBySide => Self::SideBySide,
            CacheDataPlacement::Tiered => Self::Tiered,
        }
    }

    #[allow(non_snake_case)]
    pub fn FromConfigName(value: &str) -> Self {
        Self::from_config_name(value)
    }

    #[allow(non_snake_case)]
    pub fn AsConfigName(self) -> &'static str {
        self.as_config_name()
    }

    #[allow(non_snake_case)]
    pub fn AsCacheDataPlacement(self) -> CacheDataPlacement {
        self.as_cache_data_placement()
    }
}

impl From<DramPmemDataPlacement> for CacheDataPlacement {
    fn from(value: DramPmemDataPlacement) -> Self {
        value.as_cache_data_placement()
    }
}

impl From<CacheDataPlacement> for DramPmemDataPlacement {
    fn from(value: CacheDataPlacement) -> Self {
        Self::from_cache_data_placement(value)
    }
}

fn default_cache_data_placement() -> CacheDataPlacement {
    CacheDataPlacement::Tiered
}

/// Why the admission policy placed a value where it did.
///
/// Returned alongside the chosen [`CacheTier`] so a caller can tell a hot-path
/// admission from a fallback. `Oversize` is the only one that accompanies
/// `CacheTier::Reject`: the value did not fit anywhere. `MemoryOnly` is the
/// unconditional fallthrough at the end of the chain -- nothing above it
/// matched, so the value goes to memory alone, with neither the persistent nor
/// the SSD tier admitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheAdmissionReason {
    HotPage,
    HotObject,
    WarmSlot,
    PersistentMemory,
    LargeColdBlock,
    Oversize,
    MemoryOnly,
}

/// Which operation an access record describes.
///
/// Only these three are recorded. Note that `RemoveAll` does not produce
/// records -- clearing a cache is not reported as a `Delete` per entry.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheAccessRecordKind {
    #[serde(alias = "kPut")]
    Put = 1,
    #[serde(alias = "kGet")]
    Get = 2,
    #[serde(alias = "kDelete")]
    Delete = 3,
}

#[allow(non_upper_case_globals)]
impl CacheAccessRecordKind {
    pub const kPut: Self = Self::Put;
    pub const kGet: Self = Self::Get;
    pub const kDelete: Self = Self::Delete;
    pub const kMaxCode: u8 = 4;

    pub fn config_code(self) -> u8 {
        self as u8
    }

    pub fn from_config_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Put),
            2 => Some(Self::Get),
            3 => Some(Self::Delete),
            _ => None,
        }
    }

    pub fn as_config_name(self) -> &'static str {
        match self {
            Self::Put => "kPut",
            Self::Get => "kGet",
            Self::Delete => "kDelete",
        }
    }

    #[allow(non_snake_case)]
    pub fn ConfigCode(self) -> u8 {
        self.config_code()
    }

    #[allow(non_snake_case)]
    pub fn FromConfigCode(code: u8) -> Option<Self> {
        Self::from_config_code(code)
    }

    #[allow(non_snake_case)]
    pub fn AsConfigName(self) -> &'static str {
        self.as_config_name()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheAccessRecord {
    pub record_type: CacheAccessRecordKind,
    pub key: CacheKey,
}

pub type AccessRecordKind = CacheAccessRecordKind;

#[derive(Clone)]
struct CacheAccessRecordCallback {
    callback: Arc<dyn Fn(CacheAccessRecord) + Send + Sync + 'static>,
}

impl std::fmt::Debug for CacheAccessRecordCallback {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("CacheAccessRecordCallback")
    }
}

impl CacheAccessRecordCallback {
    fn new<F>(callback: F) -> Self
    where
        F: Fn(CacheAccessRecord) + Send + Sync + 'static,
    {
        Self {
            callback: Arc::new(callback),
        }
    }

    fn call(&self, record: CacheAccessRecord) {
        (self.callback)(record);
    }
}

/// Why an entry left the cache, as told to an eviction handler.
///
/// A handler cannot work this out from the record alone, and the two cases
/// want opposite treatment: a value evicted to make room is the entry's
/// current contents and is worth writing somewhere slower, while an expired
/// one is stale and must not be. A handler that only releases a resource
/// wants both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheRemovalCause {
    /// Taken to make room. The value was still within its life.
    Evicted,
    /// Dropped for having passed its time to live. The value is stale.
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheEvictionRecord {
    pub tier: CacheTier,
    pub key: CacheKey,
    pub value: Vec<u8>,
    /// Why the entry left. See [`CacheRemovalCause`].
    pub cause: CacheRemovalCause,
}

#[derive(Clone)]
struct CacheEvictionCallback {
    callback: Arc<dyn Fn(CacheEvictionRecord) + Send + Sync + 'static>,
}

impl std::fmt::Debug for CacheEvictionCallback {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("CacheEvictionCallback")
    }
}

impl CacheEvictionCallback {
    fn new<F>(callback: F) -> Self
    where
        F: Fn(CacheEvictionRecord) + Send + Sync + 'static,
    {
        Self {
            callback: Arc::new(callback),
        }
    }

    fn call(&self, record: CacheEvictionRecord) {
        (self.callback)(record);
    }
}

/// Receives the number of entries evicted from one tier in a single batch.
///
/// Eviction metrics are independent of the eviction handler: they are reported
/// even while the handler is disabled. Counting entries rather than bytes is
/// what makes that affordable, since a count needs nothing materialised.
#[derive(Clone)]
struct CacheEvictionMetricCallback {
    callback: Arc<dyn Fn(CacheTier, usize) + Send + Sync + 'static>,
}

impl std::fmt::Debug for CacheEvictionMetricCallback {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("CacheEvictionMetricCallback")
    }
}

impl CacheEvictionMetricCallback {
    fn new<F>(callback: F) -> Self
    where
        F: Fn(CacheTier, usize) + Send + Sync + 'static,
    {
        Self {
            callback: Arc::new(callback),
        }
    }

    fn call(&self, tier: CacheTier, count: usize) {
        (self.callback)(tier, count);
    }
}

/// What kind of block a cached record holds.
///
/// Inferred from the key when an entry is first admitted, and used by the
/// hotness scoring, so it influences which entries survive eviction rather than
/// merely labelling them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheBlockKind {
    Page,
    Object,
    Index,
    Oplog,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheAdmissionRequest {
    pub block_kind: CacheBlockKind,
    pub shard_id: ShardId,
    #[serde(default)]
    pub routing_slot: Option<u32>,
    pub block_bytes: usize,
    #[serde(default)]
    pub hotness: u32,
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheTieringPolicy {
    pub memory_capacity_bytes: usize,
    #[serde(default)]
    pub pmem_capacity_bytes: usize,
    pub ssd_capacity_bytes: usize,
    #[serde(default = "default_cache_data_placement")]
    pub data_placement: CacheDataPlacement,
    #[serde(default)]
    pub data_placement_threshold_bytes: usize,
    pub memory_hotness_threshold: u32,
    #[serde(default)]
    pub pmem_admit_hotness_threshold: u32,
    pub ssd_admit_hotness_threshold: u32,
    pub max_memory_block_bytes: usize,
    #[serde(default)]
    pub max_pmem_block_bytes: usize,
    pub max_ssd_block_bytes: usize,
    #[serde(default = "default_ssd_write_through")]
    pub ssd_write_through: bool,
}

impl Default for CacheTieringPolicy {
    fn default() -> Self {
        Self {
            memory_capacity_bytes: 64 * 1024 * 1024,
            pmem_capacity_bytes: 512 * 1024 * 1024,
            ssd_capacity_bytes: 16 * 1024 * 1024 * 1024,
            data_placement: CacheDataPlacement::Tiered,
            data_placement_threshold_bytes: 1024 * 1024,
            memory_hotness_threshold: 8,
            pmem_admit_hotness_threshold: 4,
            ssd_admit_hotness_threshold: 2,
            max_memory_block_bytes: 1024 * 1024,
            max_pmem_block_bytes: 4 * 1024 * 1024,
            max_ssd_block_bytes: 16 * 1024 * 1024,
            ssd_write_through: true,
        }
    }
}

fn default_ssd_write_through() -> bool {
    true
}

fn default_ssd_block_durability() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheAdmissionDecision {
    pub tier: CacheTier,
    pub reason: CacheAdmissionReason,
    pub admit_memory: bool,
    #[serde(default)]
    pub admit_pmem: bool,
    pub admit_ssd: bool,
}

impl CacheTieringPolicy {
    pub fn decide(&self, request: &CacheAdmissionRequest) -> CacheAdmissionDecision {
        let ssd_enabled = self.ssd_capacity_bytes > 0;
        if ssd_enabled
            && (request.block_bytes > self.max_ssd_block_bytes
                || request.block_bytes > self.ssd_capacity_bytes)
        {
            return CacheAdmissionDecision {
                tier: CacheTier::Reject,
                reason: CacheAdmissionReason::Oversize,
                admit_memory: false,
                admit_pmem: false,
                admit_ssd: false,
            };
        }
        if matches!(self.data_placement, CacheDataPlacement::SideBySide) {
            if self.pmem_capacity_bytes > 0
                && request.block_bytes > self.data_placement_threshold_bytes
                && request.block_bytes <= self.max_pmem_block_bytes
                && request.block_bytes <= self.pmem_capacity_bytes
            {
                return CacheAdmissionDecision {
                    tier: CacheTier::Pmem,
                    reason: CacheAdmissionReason::PersistentMemory,
                    admit_memory: false,
                    admit_pmem: true,
                    admit_ssd: ssd_enabled,
                };
            }
            if request.block_bytes <= self.max_memory_block_bytes
                && request.block_bytes <= self.memory_capacity_bytes
            {
                return CacheAdmissionDecision {
                    tier: CacheTier::Memory,
                    reason: if matches!(request.block_kind, CacheBlockKind::Page) {
                        CacheAdmissionReason::HotPage
                    } else {
                        CacheAdmissionReason::HotObject
                    },
                    admit_memory: true,
                    admit_pmem: false,
                    admit_ssd: ssd_enabled,
                };
            }
        }
        if request.pinned
            || (request.hotness >= self.memory_hotness_threshold
                && request.block_bytes <= self.max_memory_block_bytes
                && request.block_bytes <= self.memory_capacity_bytes)
        {
            return CacheAdmissionDecision {
                tier: CacheTier::Memory,
                reason: if matches!(request.block_kind, CacheBlockKind::Page) {
                    CacheAdmissionReason::HotPage
                } else {
                    CacheAdmissionReason::HotObject
                },
                admit_memory: true,
                admit_pmem: true,
                admit_ssd: ssd_enabled,
            };
        }
        if self.pmem_capacity_bytes > 0
            && request.hotness >= self.pmem_admit_hotness_threshold
            && request.block_bytes <= self.max_pmem_block_bytes
            && request.block_bytes <= self.pmem_capacity_bytes
        {
            return CacheAdmissionDecision {
                tier: CacheTier::Pmem,
                reason: CacheAdmissionReason::PersistentMemory,
                admit_memory: false,
                admit_pmem: true,
                admit_ssd: ssd_enabled,
            };
        }
        if ssd_enabled
            && request.routing_slot.is_some()
            && request.hotness >= self.ssd_admit_hotness_threshold
            && request.block_bytes <= self.max_ssd_block_bytes
        {
            return CacheAdmissionDecision {
                tier: CacheTier::Ssd,
                reason: CacheAdmissionReason::WarmSlot,
                admit_memory: false,
                admit_pmem: false,
                admit_ssd: true,
            };
        }
        if ssd_enabled
            && (request.block_bytes > self.max_memory_block_bytes
                || request.hotness >= self.ssd_admit_hotness_threshold)
        {
            return CacheAdmissionDecision {
                tier: CacheTier::Ssd,
                reason: CacheAdmissionReason::LargeColdBlock,
                admit_memory: false,
                admit_pmem: false,
                admit_ssd: true,
            };
        }
        CacheAdmissionDecision {
            tier: CacheTier::Memory,
            reason: CacheAdmissionReason::MemoryOnly,
            admit_memory: true,
            admit_pmem: false,
            admit_ssd: false,
        }
    }
}

