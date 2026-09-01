// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

/// Stable shard identifier used to scope cache keys.
pub type ShardId = u64;
pub type ChunkId = u64;

/// Everything this crate can fail with.
///
/// Most variants read directly from their `Display` text. Three are worth
/// knowing before you match on them:
///
/// * `ReplaceMismatch` means a replace found the entry had already changed
///   since it was read, not that the entry was missing.
/// * `CorruptBlock` covers a stored record that does not describe itself
///   consistently -- a failed CRC-32C, or a header whose offsets do not match
///   the payload that follows it.
/// * `UnsupportedTier` is what an operation answers when handed
///   [`CacheTier::Reject`], which names an admission decision rather than a
///   place data can live.
#[derive(Debug, Error)]
pub enum CacheError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("cache is stopped")]
    Stopped,
    #[error("cache is already started")]
    AlreadyStarted,
    #[error("cache buffer not found")]
    NotFound,
    #[error("cache buffer was replaced before update")]
    ReplaceMismatch,
    #[error("cache tier does not support this operation: {0:?}")]
    UnsupportedTier(CacheTier),
    #[error("cache instance does not support this operation: {0:?}")]
    UnsupportedInstance(CacheInstanceKind),
    #[error("corrupt cache block: {0}")]
    CorruptBlock(String),
    #[error("unsupported cache block codec {0}")]
    UnsupportedCodec(u8),
    #[error("cache map capacity exceeded")]
    CapacityExceeded,
    #[error("invalid cache configuration: {0}")]
    InvalidConfig(String),
    #[error("rocksdb error: {0}")]
    RocksDb(String),
}

#[cfg(any(test, not(feature = "rocksdb-ssd")))]
const CACHE_MANIFEST_NAME: &str = "cache_manifest.jsonl";
pub const MT_HASH_SEED: u64 = 0x2017_0730;
pub const MT_MURMUR_HASH2_DEFAULT_SEED: u32 = 97;
pub const RDMA_DEFAULT_DRAM: u8 = 0;
pub const RDMA_MAX_BLOCK_SIZE: usize = 1usize << 32;
pub const RDMA_BUCKET_SIZE: usize = 512;
pub const RDMA_ENTRY_SIZE: usize = 32;
pub const RDMA_BUCKET_CAP: usize = 15;
pub const RDMA_OP_FAIL: i32 = -1;
pub const RDMA_OP_SUCCESS: i32 = 0;
pub const RDMA_NOT_FOUND: i32 = 1;
pub const RDMA_COLLISION: i32 = 2;
pub const RDMA_OUT_OF_MEM: i32 = 3;
pub const RDMA_BUCKET_LOCKED: i32 = 4;
pub const RDMA_VAL_CORRUPT: i32 = 5;
pub const RDMA_DATA_HEADER: usize = 16;
pub const RDMA_KEY_LEN: usize = 8;
pub const RDMA_VAL_LEN: usize = 8;
pub const RDMA_CRC_LEN: usize = 8;
pub const RDMA_FAIL_ALLOC: i32 = -1;
pub const RDMA_CRC_MISMATCH: i32 = -2;

#[allow(non_upper_case_globals)]
pub const DefaultDRAM: u8 = RDMA_DEFAULT_DRAM;
#[allow(non_upper_case_globals)]
pub const MAX_BLOCK_SIZE: usize = RDMA_MAX_BLOCK_SIZE;
#[allow(non_upper_case_globals)]
pub const BUCKET_SIZE: usize = RDMA_BUCKET_SIZE;
#[allow(non_upper_case_globals)]
pub const ENTRY_SIZE: usize = RDMA_ENTRY_SIZE;
#[allow(non_upper_case_globals)]
pub const BUCKET_CAP: usize = RDMA_BUCKET_CAP;
#[allow(non_upper_case_globals)]
pub const OP_FAIL: i32 = RDMA_OP_FAIL;
#[allow(non_upper_case_globals)]
pub const OP_SUCCESS: i32 = RDMA_OP_SUCCESS;
#[allow(non_upper_case_globals)]
pub const NOT_FOUND: i32 = RDMA_NOT_FOUND;
#[allow(non_upper_case_globals)]
pub const COLLISON: i32 = RDMA_COLLISION;
#[allow(non_upper_case_globals)]
pub const OUT_OF_MEM: i32 = RDMA_OUT_OF_MEM;
#[allow(non_upper_case_globals)]
pub const BUCKET_LOCKED: i32 = RDMA_BUCKET_LOCKED;
#[allow(non_upper_case_globals)]
pub const VAL_CORRUPT: i32 = RDMA_VAL_CORRUPT;
#[allow(non_upper_case_globals)]
pub const DATA_HEADER: usize = RDMA_DATA_HEADER;
#[allow(non_upper_case_globals)]
pub const KEY_LEN: usize = RDMA_KEY_LEN;
#[allow(non_upper_case_globals)]
pub const VAL_LEN: usize = RDMA_VAL_LEN;
#[allow(non_upper_case_globals)]
pub const CRC_LEN: usize = RDMA_CRC_LEN;
#[allow(non_upper_case_globals)]
pub const FAIL_ALLOC: i32 = RDMA_FAIL_ALLOC;
#[allow(non_upper_case_globals)]
pub const CRC_MISMATCH: i32 = RDMA_CRC_MISMATCH;
/// Which medium a remote (RDMA) cache entry lives on.
///
/// More than a label: each variant selects the base of its own synthetic address
/// range (`0x0100_0000` for `Dram`, `0x0200_0000` for `Pmem`, and so on), so an
/// [`AllocatorAddress`] on the remote side carries its medium in the high bits
/// and can be recovered from an index entry with `from_code`.
///
/// `Invalid` is the unset or unrecognised case rather than a fourth medium, and
/// it has an address range of its own so a bad value stays distinguishable
/// instead of aliasing a real one.
#[repr(u8)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RdmaStorageEngineKind {
    #[default]
    #[serde(alias = "DRAM")]
    Dram = 0,
    #[serde(alias = "PMEM")]
    Pmem = 1,
    #[serde(alias = "SSD")]
    Ssd = 2,
    #[serde(alias = "INVALID")]
    Invalid = 3,
}

impl RdmaStorageEngineKind {
    pub const DRAM: Self = Self::Dram;
    pub const PMEM: Self = Self::Pmem;
    pub const SSD: Self = Self::Ssd;
    pub const INVALID: Self = Self::Invalid;
}

impl RdmaStorageEngineKind {
    pub fn from_code(code: u8) -> Self {
        match code {
            0 => Self::Dram,
            1 => Self::Pmem,
            2 => Self::Ssd,
            _ => Self::Invalid,
        }
    }

    pub fn as_code(self) -> u8 {
        self as u8
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdmaResponse {
    buffer: Vec<u8>,
    allocator: StdAllocator,
    ptr: Option<AllocatorAddress>,
    buf_size: usize,
}

impl Default for RdmaResponse {
    fn default() -> Self {
        Self {
            buffer: Vec::new(),
            allocator: StdAllocator::new(),
            ptr: None,
            buf_size: usize::MAX,
        }
    }
}

impl RdmaResponse {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(size: usize) -> Self {
        let mut response = Self::default();
        response.init(size);
        response
    }

    #[allow(non_snake_case)]
    pub fn New(size: usize) -> Self {
        Self::with_capacity(size)
    }

    pub fn init(&mut self, size: usize) {
        self.release_allocation();
        self.ptr = self.allocator.allocate(size);
        self.buf_size = size;
        self.buffer.clear();
        self.buffer
            .reserve(size.saturating_sub(self.buffer.capacity()));
    }

    #[allow(non_snake_case)]
    pub fn Init(&mut self, size: usize) {
        self.init(size);
    }

    pub fn fill(&mut self, data: &[u8]) {
        self.release_allocation();
        self.ptr = self.allocator.allocate(data.len());
        self.buf_size = data.len();
        self.buffer.clear();
        self.buffer.extend_from_slice(data);
    }

    #[allow(non_snake_case)]
    pub fn Fill(&mut self, data: &[u8]) {
        self.fill(data);
    }

    pub fn clear(&mut self) {
        self.release_allocation();
        self.buffer.clear();
        self.buf_size = usize::MAX;
    }

    #[allow(non_snake_case)]
    pub fn Clear(&mut self) {
        self.clear();
    }

    pub fn response_size(&self) -> usize {
        self.buf_size
    }

    #[allow(non_snake_case)]
    pub fn GetRespSize(&self) -> usize {
        self.response_size()
    }

    pub fn response(&self) -> &[u8] {
        &self.buffer
    }

    #[allow(non_snake_case)]
    pub fn GetResponse(&self) -> &[u8] {
        self.response()
    }

    pub fn allocation_addr(&self) -> Option<AllocatorAddress> {
        self.ptr
    }

    pub fn allocation_count(&self) -> usize {
        self.allocator.outstanding_allocations()
    }

    pub fn allocation_bytes(&self) -> usize {
        self.allocator.outstanding_bytes()
    }

    fn release_allocation(&mut self) {
        if let Some(ptr) = self.ptr.take() {
            self.allocator.free(ptr, self.buf_size);
        }
    }
}

pub fn rdma_data_crc(data: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    hasher.finish()
}

#[allow(non_snake_case)]
pub fn DataCRC(data: &[u8]) -> u64 {
    rdma_data_crc(data)
}

pub fn rdma_verify_crc(data: &[u8], crc: u64) -> bool {
    rdma_data_crc(data) == crc
}

#[allow(non_snake_case)]
pub fn VerifyCRC(data: &[u8], crc: u64) -> bool {
    rdma_verify_crc(data, crc)
}

pub fn rdma_verify_key(left: &[u8], right: &[u8]) -> bool {
    left == right
}

#[allow(non_snake_case)]
pub fn VerifyKey(left: &[u8], right: &[u8]) -> bool {
    rdma_verify_key(left, right)
}

pub fn rdma_is_equal(left: &[u8], right: &[u8]) -> bool {
    left == right
}

#[allow(non_snake_case)]
pub fn IsEqual(left: &[u8], right: &[u8]) -> bool {
    rdma_is_equal(left, right)
}

/// Allocation over remote (RDMA) memory.
///
/// The remote counterpart to [`CacheAllocatorApi`], reduced to what a remote
/// region needs: `allocate` and `free`, both in terms of an
/// [`AllocatorAddress`].
pub trait RdmaCacheAllocatorApi {
    fn allocate(&mut self, len: usize) -> Option<AllocatorAddress>;
    fn free(&mut self, addr: AllocatorAddress, len: usize);
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RdmaStdAllocator {
    outstanding: HashMap<AllocatorAddress, usize>,
}

impl RdmaStdAllocator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn allocate(&mut self, len: usize) -> Option<AllocatorAddress> {
        let addr = allocate_virtual_region(len, 1).ok()?;
        self.outstanding.insert(addr, len);
        Some(addr)
    }

    #[allow(non_snake_case)]
    pub fn Allocate(&mut self, len: usize) -> Option<AllocatorAddress> {
        self.allocate(len)
    }

    pub fn free(&mut self, addr: AllocatorAddress, _len: usize) {
        self.outstanding.remove(&addr);
        let _ = free_virtual_region(addr);
    }

    #[allow(non_snake_case)]
    pub fn Free(&mut self, addr: AllocatorAddress, len: usize) {
        self.free(addr, len);
    }

    pub fn outstanding_allocations(&self) -> usize {
        self.outstanding.len()
    }

    pub fn outstanding_bytes(&self) -> usize {
        self.outstanding.values().copied().sum()
    }
}

impl RdmaCacheAllocatorApi for RdmaStdAllocator {
    fn allocate(&mut self, len: usize) -> Option<AllocatorAddress> {
        RdmaStdAllocator::allocate(self, len)
    }

    fn free(&mut self, addr: AllocatorAddress, len: usize) {
        RdmaStdAllocator::free(self, addr, len);
    }
}

pub type RdmaCacheAllocator = RdmaStdAllocator;
pub type StdAllocator = RdmaStdAllocator;

fn rdma_stable_hash<T: Hash>(value: &T, salt: u64) -> u64 {
    let mut hasher = DefaultHasher::new();
    salt.hash(&mut hasher);
    value.hash(&mut hasher);
    hasher.finish()
}

pub fn rdma_hash_code_1b<T: Hash>(value: &T) -> u8 {
    (rdma_stable_hash(value, 0x1b) & 0xff) as u8
}

#[allow(non_snake_case)]
pub fn HashCode1B<T: Hash>(value: &T) -> u8 {
    rdma_hash_code_1b(value)
}

pub fn signature_96<T: Hash>(key: &T, bucket_pos: u64) -> [u8; 12] {
    let first = rdma_stable_hash(&(bucket_pos, key), 0x96);
    let second = rdma_stable_hash(&(bucket_pos, key), 0x9600);
    let mut signature = [0; 12];
    signature[..8].copy_from_slice(&first.to_le_bytes());
    signature[8..].copy_from_slice(&second.to_le_bytes()[..4]);
    signature
}

#[allow(non_snake_case)]
pub fn Signature_96<T: Hash>(key: &T, bucket_pos: u64) -> [u8; 12] {
    signature_96(key, bucket_pos)
}

pub fn signature_128<T: Hash>(key: &T, bucket_pos: u64) -> [u8; 16] {
    let first = rdma_stable_hash(&(bucket_pos, key), 0x128);
    let second = rdma_stable_hash(&(bucket_pos, key), 0x1280);
    let mut signature = [0; 16];
    signature[..8].copy_from_slice(&first.to_le_bytes());
    signature[8..].copy_from_slice(&second.to_le_bytes());
    signature
}

#[allow(non_snake_case)]
pub fn Signature_128<T: Hash>(key: &T, bucket_pos: u64) -> [u8; 16] {
    signature_128(key, bucket_pos)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RdmaIndexEntry {
    signature: [u8; 16],
    addr: u64,
    length: i32,
    version: i32,
}

impl Default for RdmaIndexEntry {
    fn default() -> Self {
        Self {
            signature: [0; 16],
            addr: 0,
            length: 0,
            version: -1,
        }
    }
}

impl RdmaIndexEntry {
    pub fn ptr(&self) -> AllocatorAddress {
        ((self.addr & 0xFFFF_FFFF_FFFF_0000) >> 16) as AllocatorAddress
    }

    #[allow(non_snake_case)]
    pub fn GetPtr(&self) -> AllocatorAddress {
        self.ptr()
    }

    pub fn crc(&self) -> u8 {
        ((self.addr & 0xFF00) >> 8) as u8
    }

    #[allow(non_snake_case)]
    pub fn GetCRC(&self) -> u8 {
        self.crc()
    }

    pub fn entry_type(&self) -> u8 {
        ((self.addr & 0xC0) >> 6) as u8
    }

    #[allow(non_snake_case)]
    pub fn GetType(&self) -> u8 {
        self.entry_type()
    }

    pub fn storage_engine_type(&self) -> RdmaStorageEngineKind {
        RdmaStorageEngineKind::from_code(self.entry_type())
    }

    pub fn overflow_flag(&self) -> i32 {
        ((self.addr & 0x20) >> 5) as i32
    }

    #[allow(non_snake_case)]
    pub fn GetOverflowFlag(&self) -> i32 {
        self.overflow_flag()
    }

    pub fn signature_128b(&self) -> [u8; 16] {
        self.signature
    }

    #[allow(non_snake_case)]
    pub fn GetSignature128b(&self) -> [u8; 16] {
        self.signature_128b()
    }

    pub fn signature_96b(&self) -> [u8; 12] {
        let mut signature = [0; 12];
        signature.copy_from_slice(&self.signature[..12]);
        signature
    }

    #[allow(non_snake_case)]
    pub fn GetSignature96b(&self) -> [u8; 12] {
        self.signature_96b()
    }

    pub fn set_signature_96(&mut self, signature: [u8; 12]) {
        self.signature = [0; 16];
        self.signature[..12].copy_from_slice(&signature);
    }

    pub fn set_signature_128(&mut self, signature: [u8; 16]) {
        self.signature = signature;
    }

    pub fn length(&self) -> i32 {
        self.length
    }

    #[allow(non_snake_case)]
    pub fn GetLength(&self) -> i32 {
        self.length()
    }

    pub fn set_data_length(&mut self, len: i32) {
        self.length = len;
    }

    #[allow(non_snake_case)]
    pub fn SetDataLength(&mut self, len: i32) {
        self.set_data_length(len);
    }

    pub fn version(&self) -> i32 {
        self.version
    }

    #[allow(non_snake_case)]
    pub fn GetVersion(&self) -> i32 {
        self.version()
    }

    pub fn set_version(&mut self) {
        self.version += 1;
    }

    #[allow(non_snake_case)]
    pub fn SetVersion(&mut self) {
        self.set_version();
    }

    pub fn addr(&self) -> u64 {
        self.addr
    }

    #[allow(non_snake_case)]
    pub fn GetAddr(&self) -> u64 {
        self.addr()
    }

    pub fn set_addr(&mut self, addr: u64) {
        self.addr = addr;
    }

    #[allow(non_snake_case)]
    pub fn SetAddr(&mut self, addr: u64) {
        self.set_addr(addr);
    }

    pub fn entry_crc(&self) -> u8 {
        let mut hasher = DefaultHasher::new();
        self.signature.hash(&mut hasher);
        self.length.hash(&mut hasher);
        self.version.hash(&mut hasher);
        (hasher.finish() & 0xff) as u8
    }

    pub fn set_packed_addr(
        &mut self,
        ptr: AllocatorAddress,
        storage_type: RdmaStorageEngineKind,
        block_size: usize,
    ) {
        let overflow = if block_size > RDMA_MAX_BLOCK_SIZE {
            1
        } else {
            0
        };
        let mut addr = ((ptr as u64) << 16)
            | ((storage_type.as_code() as u64) << 6)
            | ((overflow as u64) << 5);
        addr |= (self.entry_crc() as u64) << 8;
        self.addr = addr;
    }

    pub fn get_rkey(&self, _buffer: &mut [u8]) -> i32 {
        0
    }

    #[allow(non_snake_case)]
    pub fn GetRkey(&self, buffer: &mut [u8]) -> i32 {
        self.get_rkey(buffer)
    }
}

pub fn entry_crc(entry: &RdmaIndexEntry) -> u8 {
    entry.entry_crc()
}

#[allow(non_snake_case)]
pub fn EntryCRC(entry: &RdmaIndexEntry) -> u8 {
    entry_crc(entry)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RdmaBucketHeader {
    fingerprints: [u8; RDMA_BUCKET_CAP],
    bitmap: u16,
}

impl Default for RdmaBucketHeader {
    fn default() -> Self {
        Self {
            fingerprints: [0; RDMA_BUCKET_CAP],
            bitmap: 0,
        }
    }
}

impl RdmaBucketHeader {
    pub fn bitmap(&self) -> u16 {
        self.bitmap
    }

    #[allow(non_snake_case)]
    pub fn GetBitmap(&self) -> u16 {
        self.bitmap()
    }

    pub fn fingerprint(&self, pos: usize) -> Option<u8> {
        self.fingerprints.get(pos).copied()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RdmaBucket<K> {
    metadata: RdmaBucketHeader,
    entries: [RdmaIndexEntry; RDMA_BUCKET_CAP],
    keys: [Option<K>; RDMA_BUCKET_CAP],
}

impl<K: Clone> Default for RdmaBucket<K> {
    fn default() -> Self {
        Self {
            metadata: RdmaBucketHeader::default(),
            entries: std::array::from_fn(|_| RdmaIndexEntry::default()),
            keys: std::array::from_fn(|_| None),
        }
    }
}

impl<K> RdmaBucket<K>
where
    K: Clone + Eq + Hash,
{
    pub fn metadata(&self) -> &RdmaBucketHeader {
        &self.metadata
    }

    #[allow(non_snake_case)]
    pub fn GetMetadata(&self) -> &RdmaBucketHeader {
        self.metadata()
    }

    pub fn get_entry(&self, pos: usize) -> Option<&RdmaIndexEntry> {
        self.entries.get(pos)
    }

    #[allow(non_snake_case)]
    pub fn GetEntry(&self, pos: usize) -> Option<&RdmaIndexEntry> {
        self.get_entry(pos)
    }

    pub fn lock_bucket(&mut self) -> bool {
        if self.is_locked() {
            return false;
        }
        self.metadata.bitmap |= 1;
        true
    }

    #[allow(non_snake_case)]
    pub fn LockBucket(&mut self) -> bool {
        self.lock_bucket()
    }

    pub fn unlock_bucket(&mut self) {
        self.metadata.bitmap &= !1;
    }

    #[allow(non_snake_case)]
    pub fn UnlockBucket(&mut self) {
        self.unlock_bucket();
    }

    pub fn is_locked(&self) -> bool {
        self.metadata.bitmap & 1 == 1
    }

    #[allow(non_snake_case)]
    pub fn IsLocked(&self) -> bool {
        self.is_locked()
    }

    pub fn empty_entry(&self) -> i32 {
        for pos in 0..RDMA_BUCKET_CAP {
            if self.metadata.bitmap & (1 << (pos + 1)) == 0 {
                return pos as i32;
            }
        }
        -1
    }

    #[allow(non_snake_case)]
    pub fn GetEmptyEntry(&self) -> i32 {
        self.empty_entry()
    }

    pub fn occupy_entry(&mut self, pos: usize) {
        if pos < RDMA_BUCKET_CAP {
            self.metadata.bitmap |= 1 << (pos + 1);
        }
    }

    #[allow(non_snake_case)]
    pub fn OccupyEntry(&mut self, pos: usize) {
        self.occupy_entry(pos);
    }

    pub fn clear_entry(&mut self, pos: usize) {
        if pos < RDMA_BUCKET_CAP {
            self.metadata.bitmap &= !(1 << (pos + 1));
            self.entries[pos] = RdmaIndexEntry::default();
            self.keys[pos] = None;
            self.metadata.fingerprints[pos] = 0;
        }
    }

    #[allow(non_snake_case)]
    pub fn ClearEntry(&mut self, pos: usize) {
        self.clear_entry(pos);
    }

    pub fn evict_entry(&mut self) -> usize {
        let pos = self
            .keys
            .iter()
            .position(Option::is_some)
            .unwrap_or(0)
            .min(RDMA_BUCKET_CAP - 1);
        self.clear_entry(pos);
        pos
    }

    #[allow(non_snake_case)]
    pub fn EvictEntry(&mut self) -> usize {
        self.evict_entry()
    }

    pub fn occupied_entry_count(&self) -> u64 {
        ((self.metadata.bitmap >> 1) & 0x7fff).count_ones() as u64
    }

    #[allow(non_snake_case)]
    pub fn GetOccupiedEntryNum(&self) -> u64 {
        self.occupied_entry_count()
    }

    pub fn get_index(&self, key: &K, sig96: &[u8; 12], sig128: &[u8; 16]) -> i32 {
        let fp = rdma_hash_code_1b(key);
        for pos in 0..RDMA_BUCKET_CAP {
            if self.metadata.bitmap & (1 << (pos + 1)) == 0 {
                continue;
            }
            if self.metadata.fingerprints[pos] != fp {
                continue;
            }
            let entry = &self.entries[pos];
            let signature_match = if entry.storage_engine_type() == RdmaStorageEngineKind::Ssd {
                &entry.signature == sig128
            } else {
                &entry.signature_96b() == sig96
            };
            if signature_match && self.keys[pos].as_ref() == Some(key) {
                return pos as i32;
            }
        }
        -1
    }

    #[allow(non_snake_case)]
    pub fn GetIndex(&self, key: &K, sig96: &[u8; 12], sig128: &[u8; 16]) -> i32 {
        self.get_index(key, sig96, sig128)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdmaHashTableGet {
    pub addr: Option<AllocatorAddress>,
    pub len: usize,
    pub storage_type: RdmaStorageEngineKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdmaHashTablePut {
    pub status: i32,
    pub old_addr: Option<AllocatorAddress>,
    pub old_len: usize,
    pub old_type: RdmaStorageEngineKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdmaHashTableDel {
    pub status: i32,
    pub addr: Option<AllocatorAddress>,
    pub len: usize,
    pub storage_type: RdmaStorageEngineKind,
}

#[derive(Debug, Clone)]
pub struct RdmaHashTable<K> {
    buckets: Vec<RdmaBucket<K>>,
}

impl<K> RdmaHashTable<K>
where
    K: Clone + Eq + Hash,
{
    pub fn new(bucket_count: usize) -> Self {
        let bucket_count = bucket_count.max(1);
        Self {
            buckets: (0..bucket_count).map(|_| RdmaBucket::default()).collect(),
        }
    }

    fn bucket_pos(&self, key: &K) -> usize {
        (rdma_stable_hash(key, 0) as usize) % self.buckets.len()
    }

    pub fn get(&self, key: &K) -> RdmaHashTableGet {
        let bucket_pos = self.bucket_pos(key);
        let bucket = &self.buckets[bucket_pos];
        let sig96 = signature_96(key, bucket_pos as u64);
        let sig128 = signature_128(key, bucket_pos as u64);
        let pos = bucket.get_index(key, &sig96, &sig128);
        if pos < 0 {
            return RdmaHashTableGet {
                addr: None,
                len: 0,
                storage_type: RdmaStorageEngineKind::Invalid,
            };
        }
        let entry = &bucket.entries[pos as usize];
        RdmaHashTableGet {
            addr: Some(entry.ptr()),
            len: if entry.overflow_flag() == 1 {
                0
            } else {
                entry.length().max(0) as usize
            },
            storage_type: entry.storage_engine_type(),
        }
    }

    #[allow(non_snake_case)]
    pub fn Get(&self, key: &K) -> RdmaHashTableGet {
        self.get(key)
    }

    pub fn put(
        &mut self,
        key: K,
        addr: AllocatorAddress,
        kv_size: usize,
        storage_type: RdmaStorageEngineKind,
    ) -> RdmaHashTablePut {
        let bucket_pos = self.bucket_pos(&key);
        let bucket = &mut self.buckets[bucket_pos];
        if !bucket.lock_bucket() {
            return RdmaHashTablePut {
                status: RDMA_BUCKET_LOCKED,
                old_addr: None,
                old_len: 0,
                old_type: RdmaStorageEngineKind::Invalid,
            };
        }
        let sig96 = signature_96(&key, bucket_pos as u64);
        let sig128 = signature_128(&key, bucket_pos as u64);
        let existing = bucket.get_index(&key, &sig96, &sig128);
        let block_size = kv_size.saturating_add(RDMA_DATA_HEADER + RDMA_CRC_LEN);
        let mut old_addr = None;
        let mut old_len = 0;
        let mut old_type = RdmaStorageEngineKind::Invalid;
        let pos = if existing >= 0 {
            let pos = existing as usize;
            let old = &bucket.entries[pos];
            old_addr = Some(old.ptr());
            old_len = old.length().max(0) as usize;
            old_type = old.storage_engine_type();
            pos
        } else {
            let empty = bucket.empty_entry();
            let pos = if empty >= 0 {
                empty as usize
            } else {
                bucket.evict_entry()
            };
            bucket.occupy_entry(pos);
            bucket.keys[pos] = Some(key.clone());
            bucket.metadata.fingerprints[pos] = rdma_hash_code_1b(&key);
            pos
        };

        let entry = &mut bucket.entries[pos];
        if storage_type == RdmaStorageEngineKind::Ssd {
            entry.set_signature_128(sig128);
        } else {
            entry.set_signature_96(sig96);
        }
        entry.set_data_length(block_size.min(i32::MAX as usize) as i32);
        entry.set_version();
        entry.set_packed_addr(addr, storage_type, block_size);
        bucket.unlock_bucket();
        RdmaHashTablePut {
            status: RDMA_OP_SUCCESS,
            old_addr,
            old_len,
            old_type,
        }
    }

    #[allow(non_snake_case)]
    pub fn Put(
        &mut self,
        key: K,
        addr: AllocatorAddress,
        kv_size: usize,
        storage_type: RdmaStorageEngineKind,
    ) -> RdmaHashTablePut {
        self.put(key, addr, kv_size, storage_type)
    }

    pub fn del(&mut self, key: &K) -> RdmaHashTableDel {
        let bucket_pos = self.bucket_pos(key);
        let bucket = &mut self.buckets[bucket_pos];
        if !bucket.lock_bucket() {
            return RdmaHashTableDel {
                status: RDMA_BUCKET_LOCKED,
                addr: None,
                len: 0,
                storage_type: RdmaStorageEngineKind::Invalid,
            };
        }
        let sig96 = signature_96(key, bucket_pos as u64);
        let sig128 = signature_128(key, bucket_pos as u64);
        let pos = bucket.get_index(key, &sig96, &sig128);
        if pos < 0 {
            bucket.unlock_bucket();
            return RdmaHashTableDel {
                status: RDMA_NOT_FOUND,
                addr: None,
                len: 0,
                storage_type: RdmaStorageEngineKind::Invalid,
            };
        }
        let entry = bucket.entries[pos as usize].clone();
        bucket.clear_entry(pos as usize);
        bucket.unlock_bucket();
        RdmaHashTableDel {
            status: RDMA_OP_SUCCESS,
            addr: Some(entry.ptr()),
            len: entry.length().max(0) as usize,
            storage_type: entry.storage_engine_type(),
        }
    }

    #[allow(non_snake_case)]
    pub fn Del(&mut self, key: &K) -> RdmaHashTableDel {
        self.del(key)
    }

    pub fn get_bucket(&self, index: usize) -> Option<&RdmaBucket<K>> {
        self.buckets.get(index)
    }

    #[allow(non_snake_case)]
    pub fn GetBucket(&self, index: usize) -> Option<&RdmaBucket<K>> {
        self.get_bucket(index)
    }

    pub fn size(&self) -> usize {
        self.buckets.len() * RDMA_BUCKET_SIZE
    }

    #[allow(non_snake_case)]
    pub fn GetSize(&self) -> usize {
        self.size()
    }

    pub fn num_entries(&self) -> u64 {
        self.buckets
            .iter()
            .map(RdmaBucket::occupied_entry_count)
            .sum()
    }

    #[allow(non_snake_case)]
    pub fn GetNumEntries(&self) -> u64 {
        self.num_entries()
    }

    pub fn all_buckets_unlocked(&self) -> bool {
        self.buckets.iter().all(|bucket| !bucket.is_locked())
    }

    #[allow(non_snake_case)]
    pub fn AllBucketsUnlocked(&self) -> bool {
        self.all_buckets_unlocked()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdmaStoredBlock {
    key: Vec<u8>,
    value: Vec<u8>,
    crc: u64,
}

impl RdmaStoredBlock {
    fn encoded_without_crc(key: &[u8], value: &[u8]) -> Vec<u8> {
        let mut block = Vec::with_capacity(RDMA_DATA_HEADER + key.len() + value.len());
        block.extend_from_slice(&(key.len() as u64).to_le_bytes());
        block.extend_from_slice(&(value.len() as u64).to_le_bytes());
        block.extend_from_slice(key);
        block.extend_from_slice(value);
        block
    }

    pub fn new(key: &[u8], value: &[u8]) -> Self {
        let crc = rdma_data_crc(&Self::encoded_without_crc(key, value));
        Self {
            key: key.to_vec(),
            value: value.to_vec(),
            crc,
        }
    }

    pub fn encoded_len(&self) -> usize {
        RDMA_DATA_HEADER + self.key.len() + self.value.len() + RDMA_CRC_LEN
    }

    #[allow(non_snake_case)]
    pub fn EncodedLen(&self) -> usize {
        self.encoded_len()
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut block = Self::encoded_without_crc(&self.key, &self.value);
        block.extend_from_slice(&self.crc.to_le_bytes());
        block
    }

    #[allow(non_snake_case)]
    pub fn Encode(&self) -> Vec<u8> {
        self.encode()
    }

    pub fn value(&self) -> &[u8] {
        &self.value
    }

    pub fn key(&self) -> &[u8] {
        &self.key
    }

    pub fn stored_crc(&self) -> u64 {
        self.crc
    }
}

#[derive(Debug, Clone)]
pub struct RdmaStorageEngine {
    storage_type: RdmaStorageEngineKind,
    capacity: usize,
    used: usize,
    next_addr: AllocatorAddress,
    blocks: HashMap<AllocatorAddress, RdmaStoredBlock>,
}

impl RdmaStorageEngine {
    pub fn new(storage_type: RdmaStorageEngineKind, capacity: usize) -> Self {
        let base = match storage_type {
            RdmaStorageEngineKind::Dram => 0x0100_0000,
            RdmaStorageEngineKind::Pmem => 0x0200_0000,
            RdmaStorageEngineKind::Ssd => 0x0300_0000,
            RdmaStorageEngineKind::Invalid => 0x0400_0000,
        };
        Self {
            storage_type,
            capacity,
            used: 0,
            next_addr: base,
            blocks: HashMap::new(),
        }
    }

    pub fn storage_type(&self) -> RdmaStorageEngineKind {
        self.storage_type
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn used(&self) -> usize {
        self.used
    }

    pub fn put(&mut self, key: &[u8], value: &[u8]) -> Option<AllocatorAddress> {
        let block = RdmaStoredBlock::new(key, value);
        let len = block.encoded_len();
        if self.used.saturating_add(len) > self.capacity {
            return None;
        }
        let addr = self.next_addr;
        self.next_addr = self.next_addr.saturating_add(round_up(len, 8).max(8));
        self.used += len;
        self.blocks.insert(addr, block);
        Some(addr)
    }

    #[allow(non_snake_case)]
    pub fn Put(&mut self, key: &[u8], value: &[u8]) -> Option<AllocatorAddress> {
        self.put(key, value)
    }

    pub fn get(
        &self,
        key: &[u8],
        mut size: usize,
        response: &mut RdmaResponse,
        addr: AllocatorAddress,
    ) -> i32 {
        let Some(block) = self.blocks.get(&addr) else {
            return RDMA_NOT_FOUND;
        };
        if size == 0 {
            size = block.encoded_len();
        }
        response.fill(block.value());
        if block.key() != key {
            return RDMA_NOT_FOUND;
        }
        let expected_without_crc = block.encoded_len().saturating_sub(RDMA_CRC_LEN);
        if size != block.encoded_len()
            || rdma_data_crc(&RdmaStoredBlock::encoded_without_crc(
                block.key(),
                block.value(),
            )) != block.stored_crc()
            || size.saturating_sub(RDMA_CRC_LEN) != expected_without_crc
        {
            return RDMA_CRC_MISMATCH;
        }
        RDMA_OP_SUCCESS
    }

    #[allow(non_snake_case)]
    pub fn Get(
        &self,
        key: &[u8],
        size: usize,
        response: &mut RdmaResponse,
        addr: AllocatorAddress,
    ) -> i32 {
        self.get(key, size, response, addr)
    }

    pub fn del(&mut self, addr: AllocatorAddress, _len: usize) -> i32 {
        if let Some(block) = self.blocks.remove(&addr) {
            self.used = self.used.saturating_sub(block.encoded_len());
        }
        RDMA_OP_SUCCESS
    }

    #[allow(non_snake_case)]
    pub fn Del(&mut self, addr: AllocatorAddress, len: usize) -> i32 {
        self.del(addr, len)
    }

    pub fn stats(&self) -> (usize, usize, usize) {
        (self.capacity, self.used, self.blocks.len())
    }

    #[allow(non_snake_case)]
    pub fn Stats(&self) -> (usize, usize, usize) {
        self.stats()
    }
}

#[derive(Debug, Clone)]
pub struct RdmaStorageEngineDram {
    inner: RdmaStorageEngine,
}

impl RdmaStorageEngineDram {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: RdmaStorageEngine::new(RdmaStorageEngineKind::Dram, capacity),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self::new(capacity)
    }

    pub fn put(&mut self, key: &[u8], value: &[u8]) -> Option<AllocatorAddress> {
        self.inner.put(key, value)
    }

    #[allow(non_snake_case)]
    pub fn Put(&mut self, key: &[u8], value: &[u8]) -> Option<AllocatorAddress> {
        self.put(key, value)
    }

    pub fn get(
        &self,
        key: &[u8],
        size: usize,
        response: &mut RdmaResponse,
        addr: AllocatorAddress,
    ) -> i32 {
        self.inner.get(key, size, response, addr)
    }

    #[allow(non_snake_case)]
    pub fn Get(
        &self,
        key: &[u8],
        size: usize,
        response: &mut RdmaResponse,
        addr: AllocatorAddress,
    ) -> i32 {
        self.get(key, size, response, addr)
    }

    pub fn del(&mut self, addr: AllocatorAddress, len: usize) -> i32 {
        self.inner.del(addr, len)
    }

    #[allow(non_snake_case)]
    pub fn Del(&mut self, addr: AllocatorAddress, len: usize) -> i32 {
        self.del(addr, len)
    }

    pub fn stats(&self) -> (usize, usize, usize) {
        self.inner.stats()
    }

    #[allow(non_snake_case)]
    pub fn Stats(&self) -> (usize, usize, usize) {
        self.stats()
    }
}

#[derive(Debug, Clone)]
pub struct RdmaStorageEnginePmem {
    inner: RdmaStorageEngine,
}

impl RdmaStorageEnginePmem {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: RdmaStorageEngine::new(RdmaStorageEngineKind::Pmem, capacity),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self::new(capacity)
    }

    pub fn put(&mut self, key: &[u8], value: &[u8]) -> Option<AllocatorAddress> {
        let ptr = self.inner.put(key, value);
        if let Some(addr) = ptr {
            PMemPersist(
                addr,
                self.inner
                    .blocks
                    .get(&addr)
                    .map_or(0, RdmaStoredBlock::encoded_len),
            );
        }
        ptr
    }

    #[allow(non_snake_case)]
    pub fn Put(&mut self, key: &[u8], value: &[u8]) -> Option<AllocatorAddress> {
        self.put(key, value)
    }

    pub fn get(
        &self,
        key: &[u8],
        size: usize,
        response: &mut RdmaResponse,
        addr: AllocatorAddress,
    ) -> i32 {
        self.inner.get(key, size, response, addr)
    }

    #[allow(non_snake_case)]
    pub fn Get(
        &self,
        key: &[u8],
        size: usize,
        response: &mut RdmaResponse,
        addr: AllocatorAddress,
    ) -> i32 {
        self.get(key, size, response, addr)
    }

    pub fn del(&mut self, addr: AllocatorAddress, len: usize) -> i32 {
        self.inner.del(addr, len)
    }

    #[allow(non_snake_case)]
    pub fn Del(&mut self, addr: AllocatorAddress, len: usize) -> i32 {
        self.del(addr, len)
    }

    pub fn stats(&self) -> (usize, usize, usize) {
        self.inner.stats()
    }

    #[allow(non_snake_case)]
    pub fn Stats(&self) -> (usize, usize, usize) {
        self.stats()
    }
}

#[derive(Debug, Clone)]
pub struct RdmaStorageEngineSsd {
    inner: RdmaStorageEngine,
}

impl RdmaStorageEngineSsd {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: RdmaStorageEngine::new(RdmaStorageEngineKind::Ssd, capacity),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self::new(capacity)
    }

    pub fn put(&mut self, key: &[u8], value: &[u8]) -> Option<AllocatorAddress> {
        self.inner.put(key, value)
    }

    #[allow(non_snake_case)]
    pub fn Put(&mut self, key: &[u8], value: &[u8]) -> Option<AllocatorAddress> {
        self.put(key, value)
    }

    pub fn get(
        &self,
        key: &[u8],
        size: usize,
        response: &mut RdmaResponse,
        addr: AllocatorAddress,
    ) -> i32 {
        self.inner.get(key, size, response, addr)
    }

    #[allow(non_snake_case)]
    pub fn Get(
        &self,
        key: &[u8],
        size: usize,
        response: &mut RdmaResponse,
        addr: AllocatorAddress,
    ) -> i32 {
        self.get(key, size, response, addr)
    }

    pub fn del(&mut self, addr: AllocatorAddress, len: usize) -> i32 {
        self.inner.del(addr, len)
    }

    #[allow(non_snake_case)]
    pub fn Del(&mut self, addr: AllocatorAddress, len: usize) -> i32 {
        self.del(addr, len)
    }

    pub fn stats(&self) -> (usize, usize, usize) {
        self.inner.stats()
    }

    #[allow(non_snake_case)]
    pub fn Stats(&self) -> (usize, usize, usize) {
        self.stats()
    }
}

/// The replacement policy a remote (RDMA) cache reports.
///
/// Narrower than [`ReplacementPolicyKind`], which it converts into:
/// `Fifo` and `Lru` map across directly, and `Other` -- anything this crate does
/// not model -- arrives as `ReplacementPolicyKind::MaxCode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[derive(Default)]
pub enum RdmaReplacementPolicyKind {
    #[default]
    #[serde(alias = "FIFO")]
    Fifo = 0,
    #[serde(alias = "LRU")]
    Lru = 1,
    #[serde(alias = "OTHER")]
    Other = 2,
}

impl RdmaReplacementPolicyKind {
    pub const FIFO: Self = Self::Fifo;
    pub const LRU: Self = Self::Lru;
    pub const OTHER: Self = Self::Other;
}


impl RdmaReplacementPolicyKind {
    pub fn as_replacement_policy_type(self) -> ReplacementPolicyKind {
        match self {
            Self::Fifo => ReplacementPolicyKind::Fifo,
            Self::Lru => ReplacementPolicyKind::Lru,
            Self::Other => ReplacementPolicyKind::MaxCode,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RdmaCache {
    cache_index: RdmaHashTable<Vec<u8>>,
    dram_engine: Option<RdmaStorageEngineDram>,
    pmem_engine: Option<RdmaStorageEnginePmem>,
    ssd_engine: Option<RdmaStorageEngineSsd>,
    replacement_policy: RdmaReplacementPolicyKind,
}

impl RdmaCache {
    pub fn new(
        dram_capacity: usize,
        pmem_capacity: usize,
        ssd_capacity: usize,
        replacement_policy: RdmaReplacementPolicyKind,
    ) -> Self {
        Self {
            cache_index: RdmaHashTable::new(1024),
            dram_engine: (dram_capacity > 0).then(|| RdmaStorageEngineDram::new(dram_capacity)),
            pmem_engine: (pmem_capacity > 0).then(|| RdmaStorageEnginePmem::new(pmem_capacity)),
            ssd_engine: (ssd_capacity > 0).then(|| RdmaStorageEngineSsd::new(ssd_capacity)),
            replacement_policy,
        }
    }

    pub fn with_dram_capacity(dram_capacity: usize) -> Self {
        Self::new(dram_capacity, 0, 0, RdmaReplacementPolicyKind::Fifo)
    }

    pub fn lookup(&self, key: &[u8], response: &mut RdmaResponse) -> i32 {
        let key_vec = key.to_vec();
        let index = self.cache_index.get(&key_vec);
        let Some(addr) = index.addr else {
            return RDMA_NOT_FOUND;
        };
        match index.storage_type {
            RdmaStorageEngineKind::Dram => {
                self.dram_engine.as_ref().map_or(RDMA_NOT_FOUND, |engine| {
                    engine.get(key, index.len, response, addr)
                })
            }
            RdmaStorageEngineKind::Pmem => {
                self.pmem_engine.as_ref().map_or(RDMA_NOT_FOUND, |engine| {
                    engine.get(key, index.len, response, addr)
                })
            }
            RdmaStorageEngineKind::Ssd => {
                self.ssd_engine.as_ref().map_or(RDMA_NOT_FOUND, |engine| {
                    engine.get(key, index.len, response, addr)
                })
            }
            RdmaStorageEngineKind::Invalid => RDMA_NOT_FOUND,
        }
    }

    #[allow(non_snake_case)]
    pub fn Lookup(&self, key: &[u8], response: &mut RdmaResponse) -> i32 {
        self.lookup(key, response)
    }

    pub fn insert(&mut self, key: &[u8], value: &[u8]) -> i32 {
        self.insert_to_storage(RdmaStorageEngineKind::Dram, key, value)
    }

    #[allow(non_snake_case)]
    pub fn Insert(&mut self, key: &[u8], value: &[u8]) -> i32 {
        self.insert(key, value)
    }

    pub fn insert_to_storage(
        &mut self,
        storage_type: RdmaStorageEngineKind,
        key: &[u8],
        value: &[u8],
    ) -> i32 {
        let key_vec = key.to_vec();
        let Some(addr) = self.put_storage_block(storage_type, key, value) else {
            return RDMA_FAIL_ALLOC;
        };
        let put = self
            .cache_index
            .put(key_vec, addr, key.len() + value.len(), storage_type);
        if put.status != RDMA_OP_SUCCESS {
            self.delete_storage_block(storage_type, addr, key.len() + value.len());
            return put.status;
        }
        if let Some(old_addr) = put.old_addr {
            if put.old_len > 0 {
                let delete_status = self.delete_storage_block(put.old_type, old_addr, put.old_len);
                if delete_status != RDMA_OP_SUCCESS {
                    return RDMA_OP_FAIL;
                }
            }
        }
        RDMA_OP_SUCCESS
    }

    #[allow(non_snake_case)]
    pub fn InsertToStorage(
        &mut self,
        storage_type: RdmaStorageEngineKind,
        key: &[u8],
        value: &[u8],
    ) -> i32 {
        self.insert_to_storage(storage_type, key, value)
    }

    pub fn remove(&mut self, key: &[u8]) -> i32 {
        let key_vec = key.to_vec();
        let del = self.cache_index.del(&key_vec);
        if del.status != RDMA_OP_SUCCESS {
            return del.status;
        }
        let Some(addr) = del.addr else {
            return RDMA_NOT_FOUND;
        };
        self.delete_storage_block(del.storage_type, addr, del.len)
    }

    #[allow(non_snake_case)]
    pub fn Remove(&mut self, key: &[u8]) -> i32 {
        self.remove(key)
    }

    pub fn get_capacity(&self, storage_type: RdmaStorageEngineKind) -> usize {
        match storage_type {
            RdmaStorageEngineKind::Dram => self
                .dram_engine
                .as_ref()
                .map_or(0, |engine| engine.stats().0),
            RdmaStorageEngineKind::Pmem => self
                .pmem_engine
                .as_ref()
                .map_or(0, |engine| engine.stats().0),
            RdmaStorageEngineKind::Ssd => self
                .ssd_engine
                .as_ref()
                .map_or(0, |engine| engine.stats().0),
            RdmaStorageEngineKind::Invalid => 0,
        }
    }

    #[allow(non_snake_case)]
    pub fn GetCapacity(&self, storage_type: RdmaStorageEngineKind) -> usize {
        self.get_capacity(storage_type)
    }

    pub fn init_storage_engine(&mut self, storage_type: RdmaStorageEngineKind, capacity: usize) {
        match storage_type {
            RdmaStorageEngineKind::Dram => {
                self.dram_engine = Some(RdmaStorageEngineDram::new(capacity));
            }
            RdmaStorageEngineKind::Pmem => {
                self.pmem_engine = Some(RdmaStorageEnginePmem::new(capacity));
            }
            RdmaStorageEngineKind::Ssd => {
                self.ssd_engine = Some(RdmaStorageEngineSsd::new(capacity));
            }
            RdmaStorageEngineKind::Invalid => {}
        }
    }

    #[allow(non_snake_case)]
    pub fn InitStorageEngine(&mut self, storage_type: RdmaStorageEngineKind, capacity: usize) {
        self.init_storage_engine(storage_type, capacity);
    }

    pub fn replacement_policy_type(&self) -> RdmaReplacementPolicyKind {
        self.replacement_policy
    }

    #[allow(non_snake_case)]
    pub fn GetReplacementPolicyType(&self) -> RdmaReplacementPolicyKind {
        self.replacement_policy_type()
    }

    pub fn set_replacement_policy(&mut self, policy: RdmaReplacementPolicyKind) {
        self.replacement_policy = policy;
    }

    #[allow(non_snake_case)]
    pub fn SetReplacementPolicy(&mut self, policy: RdmaReplacementPolicyKind) {
        self.set_replacement_policy(policy);
    }

    pub fn num_index_entries(&self) -> u64 {
        self.cache_index.num_entries()
    }

    pub fn storage_stats(
        &self,
        storage_type: RdmaStorageEngineKind,
    ) -> Option<(usize, usize, usize)> {
        match storage_type {
            RdmaStorageEngineKind::Dram => {
                self.dram_engine.as_ref().map(RdmaStorageEngineDram::stats)
            }
            RdmaStorageEngineKind::Pmem => {
                self.pmem_engine.as_ref().map(RdmaStorageEnginePmem::stats)
            }
            RdmaStorageEngineKind::Ssd => self.ssd_engine.as_ref().map(RdmaStorageEngineSsd::stats),
            RdmaStorageEngineKind::Invalid => None,
        }
    }

    fn put_storage_block(
        &mut self,
        storage_type: RdmaStorageEngineKind,
        key: &[u8],
        value: &[u8],
    ) -> Option<AllocatorAddress> {
        match storage_type {
            RdmaStorageEngineKind::Dram => self.dram_engine.as_mut()?.put(key, value),
            RdmaStorageEngineKind::Pmem => self.pmem_engine.as_mut()?.put(key, value),
            RdmaStorageEngineKind::Ssd => self.ssd_engine.as_mut()?.put(key, value),
            RdmaStorageEngineKind::Invalid => None,
        }
    }

    fn delete_storage_block(
        &mut self,
        storage_type: RdmaStorageEngineKind,
        addr: AllocatorAddress,
        len: usize,
    ) -> i32 {
        match storage_type {
            RdmaStorageEngineKind::Dram => self
                .dram_engine
                .as_mut()
                .map_or(RDMA_NOT_FOUND, |engine| engine.del(addr, len)),
            RdmaStorageEngineKind::Pmem => self
                .pmem_engine
                .as_mut()
                .map_or(RDMA_NOT_FOUND, |engine| engine.del(addr, len)),
            RdmaStorageEngineKind::Ssd => self
                .ssd_engine
                .as_mut()
                .map_or(RDMA_NOT_FOUND, |engine| engine.del(addr, len)),
            RdmaStorageEngineKind::Invalid => RDMA_NOT_FOUND,
        }
    }
}

pub fn hash_uint64(block_id: u64) -> u64 {
    const M: u64 = 0xc6a4_a793_5bd1_e995;
    const R: u32 = 47;
    let mut k = block_id;
    k = k.wrapping_mul(M);
    k ^= k >> R;
    k = k.wrapping_mul(M);

    let mut h = MT_HASH_SEED ^ ((std::mem::size_of::<u64>() as u64).wrapping_mul(M));
    h ^= k;
    h = h.wrapping_mul(M);
    h ^= h >> R;
    h = h.wrapping_mul(M);
    h ^= h >> R;
    h
}

pub fn mur_mur_hash2_with_seed(key: &[u8], seed: u32) -> u32 {
    const M: u32 = 0x5bd1_e995;
    const R: u32 = 24;

    let mut h = seed ^ key.len() as u32;
    let mut chunks = key.chunks_exact(4);
    for chunk in &mut chunks {
        let mut k = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        k = k.wrapping_mul(M);
        k ^= k >> R;
        k = k.wrapping_mul(M);

        h = h.wrapping_mul(M);
        h ^= k;
    }

    let tail = chunks.remainder();
    match tail.len() {
        3 => {
            h ^= ((tail[2] as i8 as i32) << 16) as u32;
            h ^= ((tail[1] as i8 as i32) << 8) as u32;
            h ^= tail[0] as i8 as i32 as u32;
            h = h.wrapping_mul(M);
        }
        2 => {
            h ^= ((tail[1] as i8 as i32) << 8) as u32;
            h ^= tail[0] as i8 as i32 as u32;
            h = h.wrapping_mul(M);
        }
        1 => {
            h ^= tail[0] as i8 as i32 as u32;
            h = h.wrapping_mul(M);
        }
        _ => {}
    }

    h ^= h >> 13;
    h = h.wrapping_mul(M);
    h ^= h >> 15;
    h
}

pub fn mur_mur_hash2(key: &[u8]) -> u32 {
    mur_mur_hash2_with_seed(key, MT_MURMUR_HASH2_DEFAULT_SEED)
}

static FAST_RAND16_SEED: AtomicU32 = AtomicU32::new(1988);
static FAST_RAND64_SEED: AtomicU64 = AtomicU64::new(0x9e37_79b9_7f4a_7c15);

pub fn fast_rand16() -> i32 {
    let next = FAST_RAND16_SEED
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |seed| {
            Some(seed.wrapping_mul(214013).wrapping_add(2531011))
        })
        .map(|old| old.wrapping_mul(214013).wrapping_add(2531011))
        .unwrap_or_else(|seed| seed);
    ((next >> 16) & 0x7fff) as i32
}

pub fn fast_rand64() -> u64 {
    FAST_RAND64_SEED
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |seed| {
            Some(
                seed.wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407),
            )
        })
        .map(|old| {
            old.wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407)
        })
        .unwrap_or_else(|seed| seed)
}

pub fn xxh32_with_seed(input: &[u8], seed: u32) -> u32 {
    const PRIME1: u32 = 2_654_435_761;
    const PRIME2: u32 = 2_246_822_519;
    const PRIME3: u32 = 3_266_489_917;
    const PRIME4: u32 = 668_265_263;
    const PRIME5: u32 = 374_761_393;

    fn round(acc: u32, lane: u32) -> u32 {
        acc.wrapping_add(lane.wrapping_mul(PRIME2))
            .rotate_left(13)
            .wrapping_mul(PRIME1)
    }

    let mut offset = 0usize;
    let mut hash = if input.len() >= 16 {
        let mut v1 = seed.wrapping_add(PRIME1).wrapping_add(PRIME2);
        let mut v2 = seed.wrapping_add(PRIME2);
        let mut v3 = seed;
        let mut v4 = seed.wrapping_sub(PRIME1);
        while offset + 16 <= input.len() {
            v1 = round(
                v1,
                u32::from_le_bytes(input[offset..offset + 4].try_into().expect("xxh lane")),
            );
            v2 = round(
                v2,
                u32::from_le_bytes(input[offset + 4..offset + 8].try_into().expect("xxh lane")),
            );
            v3 = round(
                v3,
                u32::from_le_bytes(input[offset + 8..offset + 12].try_into().expect("xxh lane")),
            );
            v4 = round(
                v4,
                u32::from_le_bytes(
                    input[offset + 12..offset + 16]
                        .try_into()
                        .expect("xxh lane"),
                ),
            );
            offset += 16;
        }
        v1.rotate_left(1)
            .wrapping_add(v2.rotate_left(7))
            .wrapping_add(v3.rotate_left(12))
            .wrapping_add(v4.rotate_left(18))
    } else {
        seed.wrapping_add(PRIME5)
    };

    hash = hash.wrapping_add(input.len() as u32);
    while offset + 4 <= input.len() {
        let lane = u32::from_le_bytes(input[offset..offset + 4].try_into().expect("xxh lane"));
        hash = hash
            .wrapping_add(lane.wrapping_mul(PRIME3))
            .rotate_left(17)
            .wrapping_mul(PRIME4);
        offset += 4;
    }
    while offset < input.len() {
        hash = hash
            .wrapping_add((input[offset] as u32).wrapping_mul(PRIME5))
            .rotate_left(11)
            .wrapping_mul(PRIME1);
        offset += 1;
    }

    hash ^= hash >> 15;
    hash = hash.wrapping_mul(PRIME2);
    hash ^= hash >> 13;
    hash = hash.wrapping_mul(PRIME3);
    hash ^ (hash >> 16)
}

pub fn get_hashed_key(id: u64, len: usize) -> Vec<u8> {
    let hash = xxh32_with_seed(&id.to_le_bytes(), 0);
    hash.to_ne_bytes()[..len.min(std::mem::size_of::<u32>())].to_vec()
}

pub fn get_rand_str(len: usize) -> String {
    (0..len)
        .map(|_| (b'a' + (fast_rand64() % 26) as u8) as char)
        .collect()
}

#[derive(Debug, Clone)]
pub struct RandomStringGenerator {
    buf: Vec<u8>,
    next_offset_seed: u32,
}

impl Default for RandomStringGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl RandomStringGenerator {
    pub const DEFAULT_SIZE: usize = 10 << 20;
    pub const OFFSET_WINDOW: usize = 2 << 16;

    pub fn new() -> Self {
        Self::with_size(Self::DEFAULT_SIZE)
    }

    pub fn with_size(size: usize) -> Self {
        let size = size.max(Self::OFFSET_WINDOW + 1);
        let mut seed = 1988u32;
        let mut buf = Vec::with_capacity(size);
        for _ in 0..size {
            seed = seed.wrapping_mul(214013).wrapping_add(2531011);
            buf.push(b'a' + (((seed >> 16) & 0x7fff) % 26) as u8);
        }
        Self {
            buf,
            next_offset_seed: 1988,
        }
    }

    pub fn rand_value_bytes(&mut self, size: usize) -> Vec<u8> {
        let size = if self.buf.len() >= size.saturating_add(Self::OFFSET_WINDOW) {
            size
        } else {
            self.buf.len().saturating_sub(Self::OFFSET_WINDOW)
        };
        self.next_offset_seed = self
            .next_offset_seed
            .wrapping_mul(214013)
            .wrapping_add(2531011);
        let offset = ((self.next_offset_seed >> 16) & 0x7fff) as usize;
        self.buf[offset..offset + size].to_vec()
    }

    pub fn rand_value(&mut self, size: usize) -> String {
        String::from_utf8(self.rand_value_bytes(size)).expect("random buffer is lowercase ascii")
    }

    #[allow(non_snake_case)]
    pub fn RandValueBytes(&mut self, size: usize) -> Vec<u8> {
        self.rand_value_bytes(size)
    }

    #[allow(non_snake_case)]
    pub fn RandValue(&mut self, size: usize) -> String {
        self.rand_value(size)
    }
}

#[allow(non_snake_case)]
pub fn HashUInt64(block_id: u64) -> u64 {
    hash_uint64(block_id)
}

#[allow(non_snake_case)]
pub fn MurMurHash2(key: &[u8]) -> u32 {
    mur_mur_hash2(key)
}

#[allow(non_snake_case)]
pub fn MurMurHash2WithSeed(key: &[u8], seed: u32) -> u32 {
    mur_mur_hash2_with_seed(key, seed)
}

#[allow(non_snake_case)]
pub fn FastRand16() -> i32 {
    fast_rand16()
}

#[allow(non_snake_case)]
pub fn FastRand64() -> u64 {
    fast_rand64()
}

#[allow(non_snake_case)]
pub fn XXH32WithSeed(input: &[u8], seed: u32) -> u32 {
    xxh32_with_seed(input, seed)
}

#[allow(non_snake_case)]
pub fn GetHashedKey(id: u64, len: usize) -> Vec<u8> {
    get_hashed_key(id, len)
}

#[allow(non_snake_case)]
pub fn GetRandStr(len: usize) -> String {
    get_rand_str(len)
}

pub fn round_up(value: usize, align: usize) -> usize {
    if align == 0 {
        return value;
    }
    value.saturating_add(align - 1) / align * align
}

#[allow(non_snake_case)]
pub fn ROUND_UP(value: usize, align: usize) -> usize {
    round_up(value, align)
}
