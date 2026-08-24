// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheRecoverReport {
    pub scanned_files: u64,
    pub recovered_files: u64,
    pub recovered_bytes: u64,
    pub skipped_files: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheTier {
    Memory,
    Pmem,
    Ssd,
    Reject,
}

#[allow(non_camel_case_types)]
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CacheInstanceType {
    kDRAM = 0,
    kPMEM = 1,
    kSSD = 2,
    kUnified = 3,
}

impl CacheInstanceType {
    fn as_tier(self) -> Option<CacheTier> {
        match self {
            CacheInstanceType::kDRAM => Some(CacheTier::Memory),
            CacheInstanceType::kPMEM => Some(CacheTier::Pmem),
            CacheInstanceType::kSSD => Some(CacheTier::Ssd),
            CacheInstanceType::kUnified => None,
        }
    }
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StorageEngineType {
    kDRAM,
    kPMEM,
    kSSD,
    kSimple,
    kMultiSSD,
}

#[allow(non_camel_case_types)]
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SSDEngineType {
    /// Supported Rust SSD engine.
    kRocksDB = 0,
}

#[allow(non_camel_case_types)]
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WriteBufferType {
    kUserDataBuf = 0,
    kMetaDataBuf = 1,
    kGCBuf = 2,
    kCodecDataBuf = 3,
}

#[allow(non_camel_case_types)]
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DataType {
    DATA = 1,
    META_LOG = 2,
}

#[allow(non_camel_case_types)]
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GCMode {
    LOSSY = 1,
    LOSSLESS = 10,
}

#[allow(non_camel_case_types)]
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RecordStateType {
    kSoftDel = 0x0,
    kNormal = 0x1,
    kPinned = 0x2,
    kMaxCode = 0xf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SsdIndexValue {
    SsdColoredPtr(u64),
    Memory {
        value: Vec<u8>,
        state: RecordStateType,
    },
}

impl SsdIndexValue {
    pub fn state(&self) -> Option<RecordStateType> {
        match self {
            Self::SsdColoredPtr(_) => None,
            Self::Memory { state, .. } => Some(*state),
        }
    }

    pub fn with_state(self, state: RecordStateType) -> Self {
        match self {
            Self::SsdColoredPtr(ptr) => Self::SsdColoredPtr(ptr),
            Self::Memory { value, .. } => Self::Memory { value, state },
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct SsdIndex {
    entries: Arc<RwLock<HashMap<String, SsdIndexValue>>>,
}

pub type Index = SsdIndex;

impl SsdIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update_index(&self, key: &str, value: SsdIndexValue) -> bool {
        let mut entries = self.entries.write().expect("ssd index lock poisoned");
        let Some(current) = entries.get(key).cloned() else {
            return false;
        };
        let value = match (current.state(), value) {
            (Some(state), SsdIndexValue::Memory { value, .. }) => {
                SsdIndexValue::Memory { value, state }
            }
            (_, value) => value,
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
        let value = entries.get_mut(key)?;
        if let SsdIndexValue::Memory { state, .. } = value {
            if *state == RecordStateType::kSoftDel {
                *state = RecordStateType::kNormal;
            }
        }
        Some(value.clone())
    }

    pub fn unpin(&self, key: &str) {
        let mut entries = self.entries.write().expect("ssd index lock poisoned");
        if let Some(SsdIndexValue::Memory { state, .. }) = entries.get_mut(key) {
            if *state == RecordStateType::kPinned {
                *state = RecordStateType::kNormal;
            }
        }
    }

    pub fn pin(&self, key: &str) -> bool {
        let mut entries = self.entries.write().expect("ssd index lock poisoned");
        let Some(SsdIndexValue::Memory { state, .. }) = entries.get_mut(key) else {
            return false;
        };
        if *state == RecordStateType::kPinned {
            return false;
        }
        *state = RecordStateType::kPinned;
        true
    }

    pub fn soft_delete(&self, key: &str) {
        let mut entries = self.entries.write().expect("ssd index lock poisoned");
        if let Some(SsdIndexValue::Memory { state, .. }) = entries.get_mut(key) {
            if *state != RecordStateType::kPinned {
                *state = RecordStateType::kSoftDel;
            }
        }
    }

    pub fn delete_if<F>(&self, key: &str, pred: F) -> bool
    where
        F: FnOnce(RecordStateType) -> bool,
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
        F: FnOnce(RecordStateType) -> bool,
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
        F: FnOnce(RecordStateType) -> bool,
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
        F: FnOnce(RecordStateType) -> bool,
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
    buf_type: WriteBufferType,
    capacity: u32,
    records: Vec<WriteBufferRecord>,
}

impl WriteBuffer {
    pub fn new(buf_type: WriteBufferType, capacity: u32) -> Self {
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

    pub fn buf_type(&self) -> WriteBufferType {
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
    pub fn BufType(&self) -> WriteBufferType {
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
        Self::new(WriteBufferType::kUserDataBuf, 10_485_760)
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

pub fn mask_colored_ptr_record_state(old_colored_ptr: u64, state: RecordStateType) -> u64 {
    old_colored_ptr | ((state as u64) & (RecordStateType::kMaxCode as u64))
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
pub fn MaskColoredPtrRecordState(old_colored_ptr: u64, state: RecordStateType) -> u64 {
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

    pub fn get_xxh_seed(&self) -> u64 {
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
        mut update_entry_cb: F,
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
            colored_ptr = mask_colored_ptr_record_state(colored_ptr, RecordStateType::kSoftDel);
            colored_ptr = mask_colored_ptr_lba(colored_ptr, batch_begin_offset);
            colored_ptr = mask_colored_ptr_size(colored_ptr, record_units);
            update_entry_cb(&record.key, SsdIndexValue::SsdColoredPtr(colored_ptr));

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
        update_entry_cb: F,
        batch_begin_offset: u64,
        oplog_size: u32,
    ) -> Vec<u8>
    where
        F: FnMut(&str, SsdIndexValue) -> bool,
    {
        self.serialize_oplog(records, update_entry_cb, batch_begin_offset, oplog_size)
    }

    #[allow(non_snake_case)]
    pub fn DeserializeOplog(&self, src: &[u8]) -> (String, u64, usize, bool) {
        self.deserialize_oplog(src)
    }

    #[allow(non_snake_case)]
    pub fn GetXXHSeed(&self) -> u64 {
        self.get_xxh_seed()
    }
}

#[derive(Debug, Clone)]
pub struct BufferManager {
    write_enabled: bool,
    buffers: HashMap<WriteBufferType, Vec<WriteBufferRecord>>,
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
        buffer_type: WriteBufferType,
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

    pub fn buffered_count(&self, buffer_type: WriteBufferType) -> usize {
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
        buffer_type: WriteBufferType,
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

impl SSDEngineType {
    pub fn from_reference_name(value: &str) -> Self {
        if value.eq_ignore_ascii_case("rocksdb")
            || value.eq_ignore_ascii_case("rocks_db")
            || value.eq_ignore_ascii_case("kRocksDB")
            || value.eq_ignore_ascii_case("kSSDRocksDBStorageEngine")
        {
            Self::kRocksDB
        } else {
            Self::kRocksDB
        }
    }

    pub fn as_reference_name(self) -> &'static str {
        match self {
            Self::kRocksDB => "RocksDB",
        }
    }

    #[allow(non_snake_case)]
    pub fn FromReferenceName(value: &str) -> Self {
        Self::from_reference_name(value)
    }

    #[allow(non_snake_case)]
    pub fn AsReferenceName(self) -> &'static str {
        self.as_reference_name()
    }
}

impl StorageEngineType {
    pub fn from_reference_name(value: &str) -> Self {
        if value.eq_ignore_ascii_case("pmem")
            || value.eq_ignore_ascii_case("persistent_memory")
            || value.eq_ignore_ascii_case("persistent-memory")
            || value.eq_ignore_ascii_case("kPMEMStorageEngine")
            || value.eq_ignore_ascii_case("kPMEM")
        {
            Self::kPMEM
        } else if value.eq_ignore_ascii_case("ssd")
            || value.eq_ignore_ascii_case("rocksdb")
            || value.eq_ignore_ascii_case("rocks_db")
            || value.eq_ignore_ascii_case("kRocksDB")
            || value.eq_ignore_ascii_case("kSSDRocksDBStorageEngine")
            || value.eq_ignore_ascii_case("kSSD")
        {
            Self::kSSD
        } else if value.eq_ignore_ascii_case("simple")
            || value.eq_ignore_ascii_case("simple_storage")
            || value.eq_ignore_ascii_case("kSimpleStorageEngine")
        {
            Self::kSimple
        } else if value.eq_ignore_ascii_case("multi_ssd")
            || value.eq_ignore_ascii_case("multi-ssd")
            || value.eq_ignore_ascii_case("kMultiSSDStorageEngine")
        {
            Self::kMultiSSD
        } else if value.eq_ignore_ascii_case("dram")
            || value.eq_ignore_ascii_case("kDRAM")
            || value.eq_ignore_ascii_case("kDRAMStorageEngine")
        {
            Self::kDRAM
        } else {
            Self::kDRAM
        }
    }

    pub fn from_reference_code(value: u8) -> Self {
        match value {
            1 => Self::kPMEM,
            2 => Self::kSSD,
            3 => Self::kSimple,
            4 => Self::kMultiSSD,
            _ => Self::kDRAM,
        }
    }

    pub fn reference_code(self) -> u8 {
        match self {
            Self::kDRAM => 0,
            Self::kPMEM => 1,
            Self::kSSD => 2,
            Self::kSimple => 3,
            Self::kMultiSSD => 4,
        }
    }

    pub fn is_ssd_like(self) -> bool {
        matches!(self, Self::kSSD | Self::kMultiSSD)
    }

    pub fn canonical_instance_type(self) -> CacheInstanceType {
        match self {
            Self::kDRAM | Self::kSimple => CacheInstanceType::kDRAM,
            Self::kPMEM => CacheInstanceType::kPMEM,
            Self::kSSD | Self::kMultiSSD => CacheInstanceType::kSSD,
        }
    }

    pub fn as_reference_enum_name(self) -> &'static str {
        match self {
            Self::kDRAM => "kDRAMStorageEngine",
            Self::kPMEM => "kPMEMStorageEngine",
            Self::kSSD => "kSSDRocksDBStorageEngine",
            Self::kSimple => "kSimpleStorageEngine",
            Self::kMultiSSD => "kMultiSSDStorageEngine",
        }
    }

    pub fn as_reference_name(self) -> &'static str {
        match self {
            Self::kDRAM => "DRAM",
            Self::kPMEM => "PMEM",
            Self::kSSD => "SSD",
            Self::kSimple => "Simple",
            Self::kMultiSSD => "MultiSSD",
        }
    }

    fn as_instance_type(self) -> CacheInstanceType {
        self.canonical_instance_type()
    }

    #[allow(non_snake_case)]
    pub fn FromReferenceCode(value: u8) -> Self {
        Self::from_reference_code(value)
    }

    #[allow(non_snake_case)]
    pub fn ReferenceCode(self) -> u8 {
        self.reference_code()
    }

    #[allow(non_snake_case)]
    pub fn IsSsdLike(self) -> bool {
        self.is_ssd_like()
    }

    #[allow(non_snake_case)]
    pub fn AsReferenceEnumName(self) -> &'static str {
        self.as_reference_enum_name()
    }

    #[allow(non_snake_case)]
    pub fn AsReferenceName(self) -> &'static str {
        self.as_reference_name()
    }
}

#[allow(non_camel_case_types)]
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReplacementPolicyType {
    kFIFO = 0,
    kLRU = 1,
    kSLRU = 2,
    kWeightedHotnessLru = 3,
    kMaxCode = 4,
}

impl ReplacementPolicyType {
    pub fn from_reference_name(value: &str) -> Self {
        if value.eq_ignore_ascii_case("fifo") || value.eq_ignore_ascii_case("kFIFO") {
            Self::kFIFO
        } else if value.eq_ignore_ascii_case("slru") || value.eq_ignore_ascii_case("kSLRU") {
            Self::kSLRU
        } else if value.eq_ignore_ascii_case("lru") || value.eq_ignore_ascii_case("kLRU") {
            Self::kLRU
        } else if value.eq_ignore_ascii_case("kMaxCode") {
            Self::kMaxCode
        } else {
            Self::kWeightedHotnessLru
        }
    }

    pub fn as_reference_name(self) -> &'static str {
        match self {
            Self::kFIFO => "FIFO",
            Self::kSLRU => "SLRU",
            Self::kLRU => "LRU",
            Self::kWeightedHotnessLru => "WeightedHotnessLru",
            Self::kMaxCode => "MaxCode",
        }
    }

    fn as_cache_policy(self) -> CacheReplacementPolicy {
        match self {
            ReplacementPolicyType::kFIFO => CacheReplacementPolicy::Fifo,
            ReplacementPolicyType::kSLRU => CacheReplacementPolicy::Slru,
            ReplacementPolicyType::kLRU => CacheReplacementPolicy::WeightedHotnessLru,
            ReplacementPolicyType::kWeightedHotnessLru => {
                CacheReplacementPolicy::WeightedHotnessLru
            }
            ReplacementPolicyType::kMaxCode => CacheReplacementPolicy::WeightedHotnessLru,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheDataPlacement {
    SideBySide,
    Tiered,
}

impl CacheDataPlacement {
    pub fn try_from_reference_name(value: &str) -> Result<Self, CacheError> {
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

    pub fn from_reference_name(value: &str) -> Self {
        if value.eq_ignore_ascii_case("sidebyside")
            || value.eq_ignore_ascii_case("side_by_side")
            || value.eq_ignore_ascii_case("side-by-side")
        {
            Self::SideBySide
        } else {
            Self::Tiered
        }
    }

    pub fn as_reference_name(self) -> &'static str {
        match self {
            Self::SideBySide => "SideBySide",
            Self::Tiered => "Tiered",
        }
    }
}

#[allow(non_camel_case_types)]
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DRAMPMEMDataPlacementType {
    kSideBySide = 0,
    kTiered = 1,
    kMaxCode = 2,
}

impl DRAMPMEMDataPlacementType {
    pub fn try_from_reference_name(value: &str) -> Result<Self, CacheError> {
        Ok(match CacheDataPlacement::try_from_reference_name(value)? {
            CacheDataPlacement::SideBySide => Self::kSideBySide,
            CacheDataPlacement::Tiered => Self::kTiered,
        })
    }

    pub fn from_reference_name(value: &str) -> Self {
        if value.eq_ignore_ascii_case("sidebyside")
            || value.eq_ignore_ascii_case("side_by_side")
            || value.eq_ignore_ascii_case("side-by-side")
            || value.eq_ignore_ascii_case("kSideBySide")
        {
            Self::kSideBySide
        } else if value.eq_ignore_ascii_case("kMaxCode") {
            Self::kMaxCode
        } else {
            Self::kTiered
        }
    }

    pub fn as_reference_name(self) -> &'static str {
        match self {
            Self::kSideBySide => "SideBySide",
            Self::kTiered => "Tiered",
            Self::kMaxCode => "MaxCode",
        }
    }

    pub fn as_cache_data_placement(self) -> CacheDataPlacement {
        match self {
            Self::kSideBySide => CacheDataPlacement::SideBySide,
            Self::kTiered | Self::kMaxCode => CacheDataPlacement::Tiered,
        }
    }

    pub fn from_cache_data_placement(placement: CacheDataPlacement) -> Self {
        match placement {
            CacheDataPlacement::SideBySide => Self::kSideBySide,
            CacheDataPlacement::Tiered => Self::kTiered,
        }
    }

    #[allow(non_snake_case)]
    pub fn FromReferenceName(value: &str) -> Self {
        Self::from_reference_name(value)
    }

    #[allow(non_snake_case)]
    pub fn AsReferenceName(self) -> &'static str {
        self.as_reference_name()
    }

    #[allow(non_snake_case)]
    pub fn AsCacheDataPlacement(self) -> CacheDataPlacement {
        self.as_cache_data_placement()
    }
}

impl From<DRAMPMEMDataPlacementType> for CacheDataPlacement {
    fn from(value: DRAMPMEMDataPlacementType) -> Self {
        value.as_cache_data_placement()
    }
}

impl From<CacheDataPlacement> for DRAMPMEMDataPlacementType {
    fn from(value: CacheDataPlacement) -> Self {
        Self::from_cache_data_placement(value)
    }
}

fn default_cache_data_placement() -> CacheDataPlacement {
    CacheDataPlacement::Tiered
}

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

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheAccessRecordType {
    Put = 1,
    Get = 2,
    Delete = 3,
}

#[allow(non_upper_case_globals)]
impl CacheAccessRecordType {
    pub const kPut: Self = Self::Put;
    pub const kGet: Self = Self::Get;
    pub const kDelete: Self = Self::Delete;
    pub const kMaxCode: u8 = 4;

    pub fn reference_code(self) -> u8 {
        self as u8
    }

    pub fn from_reference_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Put),
            2 => Some(Self::Get),
            3 => Some(Self::Delete),
            _ => None,
        }
    }

    pub fn as_reference_name(self) -> &'static str {
        match self {
            Self::Put => "kPut",
            Self::Get => "kGet",
            Self::Delete => "kDelete",
        }
    }

    #[allow(non_snake_case)]
    pub fn ReferenceCode(self) -> u8 {
        self.reference_code()
    }

    #[allow(non_snake_case)]
    pub fn FromReferenceCode(code: u8) -> Option<Self> {
        Self::from_reference_code(code)
    }

    #[allow(non_snake_case)]
    pub fn AsReferenceName(self) -> &'static str {
        self.as_reference_name()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheAccessRecord {
    pub record_type: CacheAccessRecordType,
    pub key: CacheKey,
}

pub type AccessRecordType = CacheAccessRecordType;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheEvictionRecord {
    pub tier: CacheTier,
    pub key: CacheKey,
    pub value: Vec<u8>,
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

