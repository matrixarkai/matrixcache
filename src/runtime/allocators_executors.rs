// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllocatorStats {
    pub num_allocated_bytes: usize,
    pub num_freed_bytes: usize,
    pub num_occupied_bytes: usize,
}

impl AllocatorStats {
    pub fn new(num_allocated_bytes: usize, num_freed_bytes: usize) -> Self {
        let num_occupied_bytes = num_allocated_bytes.saturating_sub(num_freed_bytes);
        Self {
            num_allocated_bytes,
            num_freed_bytes,
            num_occupied_bytes,
        }
    }

    #[allow(non_snake_case)]
    pub fn NumAllocatedBytes(&self) -> usize {
        self.num_allocated_bytes
    }

    #[allow(non_snake_case)]
    pub fn NumFreedBytes(&self) -> usize {
        self.num_freed_bytes
    }

    #[allow(non_snake_case)]
    pub fn NumOccupiedBytes(&self) -> usize {
        self.num_occupied_bytes
    }
}
/// Which allocator implementation backs a tier.
///
/// Not chosen directly -- it follows from the [`StorageEngineKind`]:
/// `Dram` and `Simple` use `PoolBasedAllocator`, while `Pmem`, `Ssd` and
/// `MultiSsd` use `LogBasedAllocator`, which is the one that reclaims by
/// collection rather than by freeing in place.
///
/// `MaxCode` marks the end of the numeric range rather than naming an
/// allocator.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AllocatorKind {
    #[serde(alias = "kLogBasedAllocator")]
    LogBasedAllocator = 0,
    #[serde(alias = "kPoolBasedAllocator")]
    PoolBasedAllocator = 1,
    #[serde(alias = "kJeAllocator")]
    JeAllocator = 2,
    #[serde(alias = "kMaxCode")]
    MaxCode = 3,
}

#[allow(non_upper_case_globals)]
impl AllocatorKind {
    pub const kLogBasedAllocator: Self = Self::LogBasedAllocator;
    pub const kPoolBasedAllocator: Self = Self::PoolBasedAllocator;
    pub const kJeAllocator: Self = Self::JeAllocator;
    pub const kMaxCode: Self = Self::MaxCode;
}

impl AllocatorKind {
    pub fn from_config_name(value: &str) -> Self {
        if value.eq_ignore_ascii_case("log")
            || value.eq_ignore_ascii_case("log_based")
            || value.eq_ignore_ascii_case("logbased")
            || value.eq_ignore_ascii_case("kLogBasedAllocator")
        {
            Self::LogBasedAllocator
        } else if value.eq_ignore_ascii_case("pool")
            || value.eq_ignore_ascii_case("pool_based")
            || value.eq_ignore_ascii_case("poolbased")
            || value.eq_ignore_ascii_case("kPoolBasedAllocator")
        {
            Self::PoolBasedAllocator
        } else if value.eq_ignore_ascii_case("je")
            || value.eq_ignore_ascii_case("jemalloc")
            || value.eq_ignore_ascii_case("kJeAllocator")
        {
            Self::JeAllocator
        } else {
            Self::MaxCode
        }
    }

    pub fn as_config_name(self) -> &'static str {
        match self {
            Self::LogBasedAllocator => "LogBasedAllocator",
            Self::PoolBasedAllocator => "PoolBasedAllocator",
            Self::JeAllocator => "JeAllocator",
            Self::MaxCode => "MaxCode",
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkMeta {
    pub id: ChunkId,
    pub num_allocated_bytes: usize,
    pub num_freed_bytes: usize,
    #[serde(rename = "ref_cnt")]
    pub ref_count: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolChunkMeta {
    pub id: ChunkId,
    #[serde(rename = "num_alloc_objects")]
    pub num_allocated_objects: usize,
}
/// How eagerly an allocator should flush to persistent memory.
///
/// Declared vocabulary: nothing in this crate reads it. Present so the
/// persistent-memory options can express the choice that the reference design
/// makes, but no code path here honours it.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FlushPolicy {
    #[serde(alias = "kNoFlush")]
    NoFlush = 0,
    #[serde(alias = "kInstantFlush")]
    InstantFlush = 1,
    #[serde(alias = "kMiniBatchFlush")]
    MiniBatchFlush = 2,
}

#[allow(non_upper_case_globals)]
impl FlushPolicy {
    pub const kNoFlush: Self = Self::NoFlush;
    pub const kInstantFlush: Self = Self::InstantFlush;
    pub const kMiniBatchFlush: Self = Self::MiniBatchFlush;
}

impl FlushPolicy {
    pub fn from_config_name(value: &str) -> Self {
        if value.eq_ignore_ascii_case("no_flush")
            || value.eq_ignore_ascii_case("noflush")
            || value.eq_ignore_ascii_case("kNoFlush")
        {
            Self::NoFlush
        } else if value.eq_ignore_ascii_case("instant_flush")
            || value.eq_ignore_ascii_case("instant")
            || value.eq_ignore_ascii_case("kInstantFlush")
        {
            Self::InstantFlush
        } else {
            Self::MiniBatchFlush
        }
    }

    pub fn as_config_name(self) -> &'static str {
        match self {
            Self::NoFlush => "NoFlush",
            Self::InstantFlush => "InstantFlush",
            Self::MiniBatchFlush => "MiniBatchFlush",
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkRecoverStats {
    pub valid_bytes: usize,
    pub freed_bytes: usize,
    pub corrupted_bytes: usize,
}

impl ChunkRecoverStats {
    pub fn total_bytes(&self) -> usize {
        self.valid_bytes
            .saturating_add(self.freed_bytes)
            .saturating_add(self.corrupted_bytes)
    }

    #[allow(non_snake_case)]
    pub fn TotalBytes(&self) -> usize {
        self.total_bytes()
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PmemRecoverStats {
    pub total_bytes: usize,
    pub valid_bytes: usize,
    pub freed_bytes: usize,
    pub corrupted_bytes: usize,
}

impl PmemRecoverStats {
    pub fn add_chunk_stats(&mut self, stats: ChunkRecoverStats) {
        self.total_bytes = self.total_bytes.saturating_add(stats.total_bytes());
        self.valid_bytes = self.valid_bytes.saturating_add(stats.valid_bytes);
        self.freed_bytes = self.freed_bytes.saturating_add(stats.freed_bytes);
        self.corrupted_bytes = self.corrupted_bytes.saturating_add(stats.corrupted_bytes);
    }

    #[allow(non_snake_case)]
    pub fn AddChunkStats(&mut self, stats: ChunkRecoverStats) {
        self.add_chunk_stats(stats)
    }
}

pub const POOL_ALLOCATOR_HEADER_LEN: usize = std::mem::size_of::<u32>();
pub const POOL_ALLOCATOR_TOMBSTONE_MASK: u32 = 1_u32 << 31;

/// An address in the allocator's modelled address space.
///
/// This is not a machine pointer and cannot be dereferenced -- the crate sets
/// `unsafe_code = "forbid"`. Regions live in a process-wide registry keyed by
/// this value, and addresses are minted from `1 << 32` upward so they cannot be
/// confused with a small offset. Arithmetic on it (alignment, header offsets) is
/// arithmetic on the model, not on memory.
pub type AllocatorAddress = usize;

static VIRTUAL_MEMORY_REGISTRY: OnceLock<Mutex<HashMap<AllocatorAddress, Vec<u8>>>> = OnceLock::new();
static NEXT_VIRTUAL_MEMORY_ADDRESS: AtomicU64 = AtomicU64::new(1 << 32);
static TLS_RESOURCE_POOL: OnceLock<Mutex<Vec<bool>>> = OnceLock::new();

fn virtual_memory_registry() -> &'static Mutex<HashMap<AllocatorAddress, Vec<u8>>> {
    VIRTUAL_MEMORY_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn align_address(value: usize, align: usize) -> usize {
    if align <= 1 {
        value.max(1)
    } else {
        round_up(value.max(1), align)
    }
}

fn allocate_virtual_region(len: usize, align: usize) -> Result<AllocatorAddress, CacheError> {
    if len == 0 {
        return Err(CacheError::CapacityExceeded);
    }
    let stride = len.saturating_add(align.max(1)).max(1) as u64;
    let raw = NEXT_VIRTUAL_MEMORY_ADDRESS.fetch_add(stride, Ordering::Relaxed) as usize;
    let ptr = align_address(raw, align.max(1));
    virtual_memory_registry()
        .lock()
        .expect("virtual memory registry lock poisoned")
        .insert(ptr, vec![0; len]);
    Ok(ptr)
}

fn free_virtual_region(ptr: AllocatorAddress) -> Result<Vec<u8>, CacheError> {
    virtual_memory_registry()
        .lock()
        .expect("virtual memory registry lock poisoned")
        .remove(&ptr)
        .ok_or(CacheError::NotFound)
}

pub fn parse_allocator_type(allocator_type: &str) -> AllocatorKind {
    AllocatorKind::from_config_name(allocator_type)
}

pub fn dram_allocate_object(
    object_len: usize,
    alignment: usize,
) -> Result<AllocatorAddress, CacheError> {
    allocate_virtual_region(object_len, alignment)
}

pub fn dram_allocate_object_v2(
    address: AllocatorAddress,
    object_len: usize,
) -> Result<AllocatorAddress, CacheError> {
    if object_len == 0 || address == 0 {
        return Err(CacheError::CapacityExceeded);
    }
    virtual_memory_registry()
        .lock()
        .expect("virtual memory registry lock poisoned")
        .insert(address, vec![0; object_len]);
    Ok(address)
}

pub fn dram_free_object(addr: AllocatorAddress, _len: usize) -> Result<(), CacheError> {
    free_virtual_region(addr).map(|_| ())
}

pub fn pmem_allocate_object(
    filename: impl AsRef<Path>,
    object_len: usize,
    alignment: usize,
) -> Result<AllocatorAddress, CacheError> {
    let filename = filename.as_ref();
    if let Some(parent) = filename.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = File::create(filename)?;
    file.set_len(object_len as u64)?;
    allocate_virtual_region(object_len, alignment)
}

pub fn pmem_allocate_object_v2(
    address: AllocatorAddress,
    filename: impl AsRef<Path>,
    object_len: usize,
) -> Result<AllocatorAddress, CacheError> {
    let filename = filename.as_ref();
    if let Some(parent) = filename.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = File::create(filename)?;
    file.set_len(object_len as u64)?;
    dram_allocate_object_v2(address, object_len)
}

pub fn pmem_free_object(addr: AllocatorAddress, len: usize) -> Result<(), CacheError> {
    dram_free_object(addr, len)
}

pub fn pre_allocate(len: usize, align: usize) -> Result<AllocatorAddress, CacheError> {
    allocate_virtual_region(round_up(len, 2 * 1024 * 1024), align.max(1))
}

pub fn post_free(addr: AllocatorAddress, len: usize) -> Result<(), CacheError> {
    dram_free_object(addr, len)
}

pub fn pmem_flush(_addr: AllocatorAddress, _len: usize) {}

pub fn pmem_drain() {}

pub fn pmem_persist(addr: AllocatorAddress, len: usize) {
    pmem_flush(addr, len);
    pmem_drain();
}

struct ThreadLocalResourceId {
    id: usize,
}

impl ThreadLocalResourceId {
    fn new() -> Self {
        let pool = TLS_RESOURCE_POOL.get_or_init(|| Mutex::new(Vec::new()));
        let mut pool = pool.lock().expect("tls resource pool lock poisoned");
        if let Some((id, free)) = pool.iter_mut().enumerate().find(|(_, free)| **free) {
            *free = false;
            Self { id }
        } else {
            let id = pool.len();
            pool.push(false);
            Self { id }
        }
    }
}

impl Drop for ThreadLocalResourceId {
    fn drop(&mut self) {
        if let Some(pool) = TLS_RESOURCE_POOL.get() {
            if let Ok(mut pool) = pool.lock() {
                if let Some(slot) = pool.get_mut(self.id) {
                    *slot = true;
                }
            }
        }
    }
}

thread_local! {
    static THREAD_LOCAL_RESOURCE_ID: ThreadLocalResourceId = ThreadLocalResourceId::new();
}

pub fn thread_local_resource_id() -> i32 {
    THREAD_LOCAL_RESOURCE_ID.with(|resource| resource.id.min(i32::MAX as usize) as i32)
}

pub fn get_pmem_file_name(
    data_path: impl AsRef<Path>,
    expected_len: i64,
    invalid_fname: Option<&mut Vec<String>>,
) -> Result<Vec<String>, CacheError> {
    let data_path = data_path.as_ref();
    let mut valid = Vec::new();
    let mut invalid_fname = invalid_fname;
    for entry in fs::read_dir(data_path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if !metadata.is_file() {
            continue;
        }
        let filename = entry.file_name().to_string_lossy().to_string();
        if expected_len < 0 || metadata.len() == expected_len as u64 {
            valid.push(filename);
        } else if let Some(invalid) = invalid_fname.as_deref_mut() {
            invalid.push(filename);
        }
    }
    valid.sort();
    if let Some(invalid) = invalid_fname {
        invalid.sort();
    }
    Ok(valid)
}

pub fn pmem_map_file(
    addr: AllocatorAddress,
    filename: impl AsRef<Path>,
    object_len: usize,
) -> Result<AllocatorAddress, CacheError> {
    let mut bytes = fs::read(filename)?;
    bytes.resize(object_len, 0);
    let ptr = if addr == 0 {
        allocate_virtual_region(object_len, 1)?
    } else {
        addr
    };
    virtual_memory_registry()
        .lock()
        .expect("virtual memory registry lock poisoned")
        .insert(ptr, bytes);
    Ok(ptr)
}

pub fn delete_pmem_file(path: impl AsRef<Path>) -> bool {
    fs::remove_file(path).is_ok()
}

#[allow(non_snake_case)]
pub fn ParseAllocatorType(allocator_type: &str) -> AllocatorKind {
    parse_allocator_type(allocator_type)
}

#[allow(non_snake_case)]
pub fn DramAllocateObject(object_len: usize, alignment: usize) -> Result<AllocatorAddress, CacheError> {
    dram_allocate_object(object_len, alignment)
}

#[allow(non_snake_case)]
pub fn DramAllocateObjectV2(
    address: AllocatorAddress,
    object_len: usize,
) -> Result<AllocatorAddress, CacheError> {
    dram_allocate_object_v2(address, object_len)
}

#[allow(non_snake_case)]
pub fn DramAllocateObject_v2(
    address: AllocatorAddress,
    object_len: usize,
) -> Result<AllocatorAddress, CacheError> {
    dram_allocate_object_v2(address, object_len)
}

#[allow(non_snake_case)]
pub fn DramFreeObject(addr: AllocatorAddress, len: usize) -> Result<(), CacheError> {
    dram_free_object(addr, len)
}

#[allow(non_snake_case)]
pub fn PMemAllocateObject(
    filename: impl AsRef<Path>,
    object_len: usize,
    alignment: usize,
) -> Result<AllocatorAddress, CacheError> {
    pmem_allocate_object(filename, object_len, alignment)
}

#[allow(non_snake_case)]
pub fn PMemAllocateObjectV2(
    address: AllocatorAddress,
    filename: impl AsRef<Path>,
    object_len: usize,
) -> Result<AllocatorAddress, CacheError> {
    pmem_allocate_object_v2(address, filename, object_len)
}

#[allow(non_snake_case)]
pub fn PMemAllocateObject_v2(
    address: AllocatorAddress,
    filename: impl AsRef<Path>,
    object_len: usize,
) -> Result<AllocatorAddress, CacheError> {
    pmem_allocate_object_v2(address, filename, object_len)
}

#[allow(non_snake_case)]
pub fn PMemFreeObject(addr: AllocatorAddress, len: usize) -> Result<(), CacheError> {
    pmem_free_object(addr, len)
}

#[allow(non_snake_case)]
pub fn PreAllocate(len: usize, align: usize) -> Result<AllocatorAddress, CacheError> {
    pre_allocate(len, align)
}

#[allow(non_snake_case)]
pub fn PostFree(addr: AllocatorAddress, len: usize) -> Result<(), CacheError> {
    post_free(addr, len)
}

#[allow(non_snake_case)]
pub fn PMemFlush(addr: AllocatorAddress, len: usize) {
    pmem_flush(addr, len);
}

#[allow(non_snake_case)]
pub fn PMemDrain() {
    pmem_drain();
}

#[allow(non_snake_case)]
pub fn PMemPersist(addr: AllocatorAddress, len: usize) {
    pmem_persist(addr, len);
}

#[allow(non_snake_case)]
pub fn GetThreadLocalResourceID() -> i32 {
    thread_local_resource_id()
}

#[allow(non_snake_case)]
pub fn GetPmemFileName(
    data_path: impl AsRef<Path>,
    expected_len: i64,
    invalid_fname: Option<&mut Vec<String>>,
) -> Result<Vec<String>, CacheError> {
    get_pmem_file_name(data_path, expected_len, invalid_fname)
}

#[allow(non_snake_case)]
pub fn PMemMapFile(
    addr: AllocatorAddress,
    filename: impl AsRef<Path>,
    object_len: usize,
) -> Result<AllocatorAddress, CacheError> {
    pmem_map_file(addr, filename, object_len)
}

#[allow(non_snake_case)]
pub fn DeletePmemFile(path: impl AsRef<Path>) -> bool {
    delete_pmem_file(path)
}

/// Receives each record scanned while a persistent-memory allocator recovers.
///
/// `on_scan_record` sees the raw regions in the order they are walked, including
/// ones that turn out to be free or tombstoned -- it is a scan of the medium,
/// not a list of live entries.
pub trait PmemAllocatorRecoverListener {
    fn on_scan_record(
        &mut self,
        ptr: AllocatorAddress,
        len: usize,
        crc32: u32,
    ) -> Result<(), CacheError>;

    #[allow(non_snake_case)]
    fn OnScanRecord(
        &mut self,
        ptr: AllocatorAddress,
        len: usize,
        crc32: u32,
    ) -> Result<(), CacheError> {
        self.on_scan_record(ptr, len, crc32)
    }
}

/// Observes collection inside a log-based allocator.
///
/// `on_gc_copy` fires for each record collection moves. Distinct from
/// [`GcCopyCallback`], which exists so an *index* can be repaired; this one is
/// for watching the allocator itself.
pub trait LogBasedAllocatorGcEventListener {
    fn on_gc_copy(
        &mut self,
        old_ptr: AllocatorAddress,
        new_ptr: AllocatorAddress,
    ) -> Result<(), CacheError>;
}

#[derive(Debug, Default, Clone)]
pub struct LogBasedAllocatorGcEventListenerMock {
    key2ptr_map: HashMap<String, AllocatorAddress>,
    ptr2key_map: HashMap<AllocatorAddress, String>,
    allocator: Option<SimpleLogBasedMemoryAllocator>,
}

impl LogBasedAllocatorGcEventListenerMock {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_allocator(allocator: SimpleLogBasedMemoryAllocator) -> Self {
        Self {
            allocator: Some(allocator),
            ..Self::default()
        }
    }

    pub fn set_allocator(&mut self, allocator: SimpleLogBasedMemoryAllocator) {
        self.allocator = Some(allocator);
    }

    pub fn allocator(&self) -> Option<&SimpleLogBasedMemoryAllocator> {
        self.allocator.as_ref()
    }

    pub fn allocator_mut(&mut self) -> Option<&mut SimpleLogBasedMemoryAllocator> {
        self.allocator.as_mut()
    }

    pub fn get_internal_map(&self, key: &str) -> Option<AllocatorAddress> {
        self.key2ptr_map.get(key).copied()
    }

    pub fn set_internal_map_and_return_old_ptr(
        &mut self,
        key: impl Into<String>,
        new_ptr: AllocatorAddress,
    ) -> Option<AllocatorAddress> {
        let key = key.into();
        let old_ptr = self.key2ptr_map.insert(key.clone(), new_ptr);
        if let Some(old_ptr) = old_ptr {
            self.ptr2key_map.remove(&old_ptr);
        }
        self.ptr2key_map.insert(new_ptr, key);
        old_ptr
    }

    pub fn del_internal_map_and_return_old_ptr(&mut self, key: &str) -> Option<AllocatorAddress> {
        let old_ptr = self.key2ptr_map.remove(key)?;
        self.ptr2key_map.remove(&old_ptr);
        Some(old_ptr)
    }

    #[allow(non_snake_case)]
    pub fn GetInternalMap(&self, key: &str) -> Option<AllocatorAddress> {
        self.get_internal_map(key)
    }

    #[allow(non_snake_case)]
    pub fn SetInternalMapAndReturnOldPtr(
        &mut self,
        key: impl Into<String>,
        new_ptr: AllocatorAddress,
    ) -> Option<AllocatorAddress> {
        self.set_internal_map_and_return_old_ptr(key, new_ptr)
    }

    #[allow(non_snake_case)]
    pub fn DelInternalMapAndReturnOldPtr(&mut self, key: &str) -> Option<AllocatorAddress> {
        self.del_internal_map_and_return_old_ptr(key)
    }

    #[allow(non_snake_case)]
    pub fn OnGCCopy(
        &mut self,
        old_ptr: AllocatorAddress,
        new_ptr: AllocatorAddress,
    ) -> Result<(), CacheError> {
        self.on_gc_copy(old_ptr, new_ptr)
    }
}

impl LogBasedAllocatorGcEventListener for LogBasedAllocatorGcEventListenerMock {
    fn on_gc_copy(
        &mut self,
        old_ptr: AllocatorAddress,
        new_ptr: AllocatorAddress,
    ) -> Result<(), CacheError> {
        let key = self
            .ptr2key_map
            .remove(&old_ptr)
            .ok_or(CacheError::NotFound)?;
        if self.key2ptr_map.get(&key).copied() != Some(old_ptr) {
            return Err(CacheError::ReplaceMismatch);
        }
        self.key2ptr_map.insert(key.clone(), new_ptr);
        self.ptr2key_map.insert(new_ptr, key);
        if let Some(allocator) = self.allocator.as_mut() {
            allocator.free(old_ptr, 0)?;
        }
        Ok(())
    }
}

/// The allocator interface the memory tiers are built on.
///
/// Note that an "allocation" here is a region in a process-wide registry, not
/// mapped memory: the crate sets `unsafe_code = "forbid"`, so an
/// `AllocatorAddress` is a key rather than something to dereference.
///
/// `seal` and `seal_with_crc` close a region against further writes, the latter
/// recording a checksum that the read path verifies.
///
/// Implemented by [`SimpleLogBasedMemoryAllocator`],
/// [`PoolBasedMemoryAllocatorBase`] and [`JeAllocator`].
pub trait CacheAllocatorApi {
    fn allocate(&mut self, len: usize) -> Result<AllocatorAddress, CacheError>;
    fn contains(&self, ptr: AllocatorAddress) -> bool;
    fn free(&mut self, ptr: AllocatorAddress, len: usize) -> Result<(), CacheError>;
    fn seal(&mut self, ptr: AllocatorAddress) -> Result<(), CacheError>;
    fn seal_with_crc(
        &mut self,
        ptr: AllocatorAddress,
        len: usize,
        crc32: u32,
    ) -> Result<(), CacheError>;
    fn stats(&self) -> Result<AllocatorStats, CacheError>;
    fn capacity(&self) -> Result<usize, CacheError>;
}

/// A [`CacheAllocatorApi`] whose regions can be read and written directly.
///
/// `write_region` and `read_region` move whole records in and out of an
/// allocated region, which is what the memory-backed storage engine needs on top
/// of allocation.
pub trait MemStorageAllocatorApi: CacheAllocatorApi {
    fn write_region(&mut self, ptr: AllocatorAddress, data: &[u8]) -> Result<(), CacheError>;
    fn read_region(&self, ptr: AllocatorAddress) -> Result<&[u8], CacheError>;
}

/// A [`CacheAllocatorApi`] that reclaims space by collection rather than by
/// freeing in place.
///
/// `gc` sweeps tombstoned records and compacts what remains. Only
/// [`SimpleLogBasedMemoryAllocator`] implements it -- the pool-based allocator
/// frees directly and has nothing to collect.
pub trait LogBasedMemoryAllocatorApi: CacheAllocatorApi {
    fn iterate_recyclable_chunk_meta<F>(&self, func: F) -> Result<(), CacheError>
    where
        F: FnMut(&ChunkMeta) -> bool;
    fn retrieve_chunk_meta<F>(&self, chunk_id: ChunkId, func: F) -> Result<(), CacheError>
    where
        F: FnOnce(&ChunkMeta);
    fn gc(&mut self, chunk_ids: &[ChunkId]) -> Result<(), CacheError>;
}

impl MemStorageAllocatorApi for SimpleLogBasedMemoryAllocator {
    fn write_region(&mut self, ptr: AllocatorAddress, data: &[u8]) -> Result<(), CacheError> {
        self.write(ptr, data)
    }

    fn read_region(&self, ptr: AllocatorAddress) -> Result<&[u8], CacheError> {
        self.read(ptr)
    }
}

#[derive(Debug, Clone)]
struct SimpleAllocationRegion {
    data: Vec<u8>,
    sealed: bool,
    crc32: Option<u32>,
    len_for_crc: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct SimpleLogBasedMemoryAllocator {
    regions: HashMap<AllocatorAddress, Option<SimpleAllocationRegion>>,
    /// Size each freed region held, kept after the region itself is dropped so
    /// a recyclable chunk can still report what reclaiming it would return.
    freed_lens: HashMap<AllocatorAddress, usize>,
    next_ptr: AllocatorAddress,
    capacity: usize,
    num_allocated_bytes: usize,
    num_freed_bytes: usize,
    gc_runs: u64,
}

impl SimpleLogBasedMemoryAllocator {
    pub fn new() -> Self {
        Self::with_capacity(0)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_and_base(capacity, 1)
    }

    pub fn with_capacity_and_base(capacity: usize, base_ptr: AllocatorAddress) -> Self {
        Self {
            regions: HashMap::new(),
            freed_lens: HashMap::new(),
            next_ptr: base_ptr.max(1),
            capacity,
            num_allocated_bytes: 0,
            num_freed_bytes: 0,
            gc_runs: 0,
        }
    }

    pub fn write(&mut self, ptr: AllocatorAddress, data: &[u8]) -> Result<(), CacheError> {
        let region = self
            .regions
            .get_mut(&ptr)
            .and_then(Option::as_mut)
            .ok_or(CacheError::NotFound)?;
        if data.len() > region.data.len() {
            return Err(CacheError::CapacityExceeded);
        }
        region.data[..data.len()].copy_from_slice(data);
        Ok(())
    }

    pub fn read(&self, ptr: AllocatorAddress) -> Result<&[u8], CacheError> {
        self.regions
            .get(&ptr)
            .and_then(Option::as_ref)
            .map(|region| region.data.as_slice())
            .ok_or(CacheError::NotFound)
    }

    pub fn sealed(&self, ptr: AllocatorAddress) -> bool {
        self.regions
            .get(&ptr)
            .and_then(Option::as_ref)
            .is_some_and(|region| region.sealed)
    }

    pub fn crc32(&self, ptr: AllocatorAddress) -> Option<u32> {
        self.regions
            .get(&ptr)
            .and_then(Option::as_ref)
            .and_then(|region| region.crc32)
    }

    pub fn gc_runs(&self) -> u64 {
        self.gc_runs
    }

    pub fn live_region_count(&self) -> usize {
        self.regions
            .values()
            .filter(|region| region.is_some())
            .count()
    }

    #[allow(non_snake_case)]
    pub fn Allocate(&mut self, len: usize) -> Result<AllocatorAddress, CacheError> {
        self.allocate(len)
    }

    #[allow(non_snake_case)]
    pub fn Contains(&self, ptr: AllocatorAddress) -> bool {
        self.contains(ptr)
    }

    #[allow(non_snake_case)]
    pub fn Free(&mut self, ptr: AllocatorAddress, len: usize) -> Result<(), CacheError> {
        self.free(ptr, len)
    }

    #[allow(non_snake_case)]
    pub fn Seal(&mut self, ptr: AllocatorAddress) -> Result<(), CacheError> {
        self.seal(ptr)
    }

    #[allow(non_snake_case)]
    pub fn SealWithCRC(
        &mut self,
        ptr: AllocatorAddress,
        len: usize,
        crc32: u32,
    ) -> Result<(), CacheError> {
        self.seal_with_crc(ptr, len, crc32)
    }

    #[allow(non_snake_case)]
    pub fn GetStats(&self) -> Result<AllocatorStats, CacheError> {
        self.stats()
    }

    #[allow(non_snake_case)]
    pub fn Capacity(&self) -> Result<usize, CacheError> {
        self.capacity()
    }

    #[allow(non_snake_case)]
    pub fn TEST_GetAllocMetrics(&self) -> Result<AllocatorStats, CacheError> {
        self.stats()
    }

    pub fn test_global_free_list_size(&self) -> usize {
        self.regions
            .values()
            .filter(|region| region.is_none())
            .count()
    }

    #[allow(non_snake_case)]
    pub fn TEST_GetGobalFreeListSize(&self) -> usize {
        self.test_global_free_list_size()
    }

    #[allow(non_snake_case)]
    pub fn IterateRecyclableChunkMeta<F>(&self, func: F) -> Result<(), CacheError>
    where
        F: FnMut(&ChunkMeta) -> bool,
    {
        self.iterate_recyclable_chunk_meta(func)
    }

    #[allow(non_snake_case)]
    pub fn RetrieveChunkMeta<F>(&self, chunk_id: ChunkId, func: F) -> Result<(), CacheError>
    where
        F: FnOnce(&ChunkMeta),
    {
        self.retrieve_chunk_meta(chunk_id, func)
    }

    #[allow(non_snake_case)]
    pub fn GC(&mut self, chunk_ids: &[ChunkId]) -> Result<(), CacheError> {
        self.gc(chunk_ids)
    }
}

impl Default for SimpleLogBasedMemoryAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl CacheAllocatorApi for SimpleLogBasedMemoryAllocator {
    fn allocate(&mut self, len: usize) -> Result<AllocatorAddress, CacheError> {
        let live_bytes = self
            .regions
            .values()
            .filter_map(Option::as_ref)
            .map(|region| region.data.len())
            .sum::<usize>();
        if self.capacity != 0 && live_bytes.saturating_add(len) > self.capacity {
            return Err(CacheError::CapacityExceeded);
        }
        let ptr = self.next_ptr;
        self.next_ptr = self.next_ptr.saturating_add(1).max(1);
        self.regions.insert(
            ptr,
            Some(SimpleAllocationRegion {
                data: vec![0; len],
                sealed: false,
                crc32: None,
                len_for_crc: None,
            }),
        );
        self.num_allocated_bytes = self.num_allocated_bytes.saturating_add(len);
        Ok(ptr)
    }

    fn contains(&self, ptr: AllocatorAddress) -> bool {
        self.regions.get(&ptr).is_some_and(Option::is_some)
    }

    fn free(&mut self, ptr: AllocatorAddress, len: usize) -> Result<(), CacheError> {
        let Some(region) = self.regions.get_mut(&ptr) else {
            return Err(CacheError::NotFound);
        };
        let Some(region) = region.take() else {
            return Err(CacheError::NotFound);
        };
        self.num_freed_bytes = self
            .num_freed_bytes
            .saturating_add(len.min(region.data.len()));
        self.freed_lens.insert(ptr, region.data.len());
        Ok(())
    }

    fn seal(&mut self, ptr: AllocatorAddress) -> Result<(), CacheError> {
        let region = self
            .regions
            .get_mut(&ptr)
            .and_then(Option::as_mut)
            .ok_or(CacheError::NotFound)?;
        region.sealed = true;
        Ok(())
    }

    fn seal_with_crc(
        &mut self,
        ptr: AllocatorAddress,
        len: usize,
        crc32: u32,
    ) -> Result<(), CacheError> {
        let region = self
            .regions
            .get_mut(&ptr)
            .and_then(Option::as_mut)
            .ok_or(CacheError::NotFound)?;
        if len > region.data.len() {
            return Err(CacheError::CapacityExceeded);
        }
        region.sealed = true;
        region.crc32 = Some(crc32);
        region.len_for_crc = Some(len);
        Ok(())
    }

    fn stats(&self) -> Result<AllocatorStats, CacheError> {
        let live_bytes = self
            .regions
            .values()
            .filter_map(Option::as_ref)
            .map(|region| region.data.len())
            .sum::<usize>();
        Ok(AllocatorStats {
            num_allocated_bytes: self.num_allocated_bytes,
            num_freed_bytes: self.num_freed_bytes,
            num_occupied_bytes: live_bytes,
        })
    }

    fn capacity(&self) -> Result<usize, CacheError> {
        Ok(self.capacity)
    }
}

impl LogBasedMemoryAllocatorApi for SimpleLogBasedMemoryAllocator {
    fn iterate_recyclable_chunk_meta<F>(&self, mut func: F) -> Result<(), CacheError>
    where
        F: FnMut(&ChunkMeta) -> bool,
    {
        for (ptr, region) in &self.regions {
            if region.is_none() {
                // A freed region is wholly reclaimable: it was allocated at this
                // size, all of it is now free, and nothing references it.
                let len = self.freed_lens.get(ptr).copied().unwrap_or(0);
                let meta = ChunkMeta {
                    id: *ptr as ChunkId,
                    num_allocated_bytes: len,
                    num_freed_bytes: len,
                    ref_count: 0,
                };
                if !func(&meta) {
                    break;
                }
            }
        }
        Ok(())
    }

    fn retrieve_chunk_meta<F>(&self, chunk_id: ChunkId, func: F) -> Result<(), CacheError>
    where
        F: FnOnce(&ChunkMeta),
    {
        let ptr = chunk_id as AllocatorAddress;
        let region = self.regions.get(&ptr).ok_or(CacheError::NotFound)?;
        let (allocated, freed, ref_count) = match region {
            Some(region) => (region.data.len(), 0, 1),
            None => {
                let len = self.freed_lens.get(&ptr).copied().unwrap_or(0);
                (len, len, 0)
            }
        };
        let meta = ChunkMeta {
            id: chunk_id,
            num_allocated_bytes: allocated,
            num_freed_bytes: freed,
            ref_count,
        };
        func(&meta);
        Ok(())
    }

    fn gc(&mut self, chunk_ids: &[ChunkId]) -> Result<(), CacheError> {
        self.gc_runs = self.gc_runs.saturating_add(1);
        if chunk_ids.is_empty() {
            // Collecting nothing was asked for; the run still counts, matching
            // the reference, which takes an explicit list and does not treat an
            // empty one as "everything".
            return Ok(());
        }
        for id in chunk_ids {
            let ptr = *id as AllocatorAddress;
            // Only a freed slot is reclaimable. A live region keyed by the same
            // id is left alone rather than dropped from under its holder.
            if self.regions.get(&ptr).is_some_and(Option::is_none) {
                self.regions.remove(&ptr);
                self.freed_lens.remove(&ptr);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct JeAllocationRegion {
    data: Vec<u8>,
    sealed: bool,
    crc32: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct JeAllocator {
    regions: HashMap<AllocatorAddress, JeAllocationRegion>,
    next_ptr: AllocatorAddress,
    capacity: usize,
    num_allocated_bytes: usize,
    num_freed_bytes: usize,
    freed_objects: usize,
}

impl JeAllocator {
    pub fn new(capacity_bytes: usize) -> Self {
        Self::with_capacity(capacity_bytes)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            regions: HashMap::new(),
            next_ptr: 1 << 40,
            capacity,
            num_allocated_bytes: 0,
            num_freed_bytes: 0,
            freed_objects: 0,
        }
    }

    pub fn write(&mut self, ptr: AllocatorAddress, data: &[u8]) -> Result<(), CacheError> {
        let region = self.regions.get_mut(&ptr).ok_or(CacheError::NotFound)?;
        if data.len() > region.data.len() {
            return Err(CacheError::CapacityExceeded);
        }
        region.data[..data.len()].copy_from_slice(data);
        Ok(())
    }

    pub fn read(&self, ptr: AllocatorAddress) -> Result<&[u8], CacheError> {
        self.regions
            .get(&ptr)
            .map(|region| region.data.as_slice())
            .ok_or(CacheError::NotFound)
    }

    pub fn sealed(&self, ptr: AllocatorAddress) -> bool {
        self.regions.get(&ptr).is_some_and(|region| region.sealed)
    }

    pub fn crc32(&self, ptr: AllocatorAddress) -> Option<u32> {
        self.regions.get(&ptr).and_then(|region| region.crc32)
    }

    #[allow(non_snake_case)]
    pub fn Allocate(&mut self, len: usize) -> Result<AllocatorAddress, CacheError> {
        self.allocate(len)
    }

    #[allow(non_snake_case)]
    pub fn Contains(&self, ptr: AllocatorAddress) -> bool {
        self.contains(ptr)
    }

    #[allow(non_snake_case)]
    pub fn Free(&mut self, ptr: AllocatorAddress, len: usize) -> Result<(), CacheError> {
        self.free(ptr, len)
    }

    #[allow(non_snake_case)]
    pub fn Seal(&mut self, ptr: AllocatorAddress) -> Result<(), CacheError> {
        self.seal(ptr)
    }

    #[allow(non_snake_case)]
    pub fn SealWithCRC(
        &mut self,
        ptr: AllocatorAddress,
        len: usize,
        crc32: u32,
    ) -> Result<(), CacheError> {
        self.seal_with_crc(ptr, len, crc32)
    }

    #[allow(non_snake_case)]
    pub fn GetStats(&self) -> Result<AllocatorStats, CacheError> {
        self.stats()
    }

    #[allow(non_snake_case)]
    pub fn Capacity(&self) -> Result<usize, CacheError> {
        self.capacity()
    }

    #[allow(non_snake_case)]
    pub fn TEST_GetAllocMetrics(&self) -> Result<AllocatorStats, CacheError> {
        self.stats()
    }

    pub fn test_global_free_list_size(&self) -> usize {
        self.freed_objects
    }

    #[allow(non_snake_case)]
    pub fn TEST_GetGobalFreeListSize(&self) -> usize {
        self.test_global_free_list_size()
    }
}

impl CacheAllocatorApi for JeAllocator {
    fn allocate(&mut self, len: usize) -> Result<AllocatorAddress, CacheError> {
        let occupied = self
            .regions
            .values()
            .map(|region| region.data.len())
            .sum::<usize>();
        if self.capacity != 0 && occupied.saturating_add(len) > self.capacity {
            return Err(CacheError::CapacityExceeded);
        }
        let ptr = self.next_ptr;
        self.next_ptr = self.next_ptr.saturating_add(1).max(1 << 40);
        self.regions.insert(
            ptr,
            JeAllocationRegion {
                data: vec![0; len],
                sealed: false,
                crc32: None,
            },
        );
        self.num_allocated_bytes = self.num_allocated_bytes.saturating_add(len);
        Ok(ptr)
    }

    fn contains(&self, ptr: AllocatorAddress) -> bool {
        self.regions.contains_key(&ptr)
    }

    fn free(&mut self, ptr: AllocatorAddress, len: usize) -> Result<(), CacheError> {
        let region = self.regions.remove(&ptr).ok_or(CacheError::NotFound)?;
        self.num_freed_bytes = self
            .num_freed_bytes
            .saturating_add(len.min(region.data.len()));
        self.freed_objects = self.freed_objects.saturating_add(1);
        Ok(())
    }

    fn seal(&mut self, ptr: AllocatorAddress) -> Result<(), CacheError> {
        let region = self.regions.get_mut(&ptr).ok_or(CacheError::NotFound)?;
        region.sealed = true;
        Ok(())
    }

    fn seal_with_crc(
        &mut self,
        ptr: AllocatorAddress,
        len: usize,
        crc32: u32,
    ) -> Result<(), CacheError> {
        let region = self.regions.get_mut(&ptr).ok_or(CacheError::NotFound)?;
        if len > region.data.len() {
            return Err(CacheError::CapacityExceeded);
        }
        region.sealed = true;
        region.crc32 = Some(crc32);
        Ok(())
    }

    fn stats(&self) -> Result<AllocatorStats, CacheError> {
        let occupied = self
            .regions
            .values()
            .map(|region| region.data.len())
            .sum::<usize>();
        Ok(AllocatorStats {
            num_allocated_bytes: self.num_allocated_bytes,
            num_freed_bytes: self.num_freed_bytes,
            num_occupied_bytes: occupied,
        })
    }

    fn capacity(&self) -> Result<usize, CacheError> {
        Ok(self.capacity)
    }
}

impl MemStorageAllocatorApi for JeAllocator {
    fn write_region(&mut self, ptr: AllocatorAddress, data: &[u8]) -> Result<(), CacheError> {
        self.write(ptr, data)
    }

    fn read_region(&self, ptr: AllocatorAddress) -> Result<&[u8], CacheError> {
        self.read(ptr)
    }
}

#[derive(Debug, Clone)]
struct PoolObject {
    data: Vec<u8>,
    requested_len: usize,
    sealed: bool,
    crc32: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct PoolBasedMemoryAllocatorBase {
    live: HashMap<AllocatorAddress, PoolObject>,
    free_list: Vec<AllocatorAddress>,
    next_ptr: AllocatorAddress,
    capacity_bytes: usize,
    max_thread_count: usize,
    object_len: usize,
    chunk_size: usize,
    num_occupied_bytes: usize,
    num_allocated_objects: usize,
    num_freed_objects: usize,
}

impl PoolBasedMemoryAllocatorBase {
    pub const DEFAULT_CHUNK_SIZE: usize = 4 * 1024 * 1024;
    pub const DEFAULT_OBJECT_LEN: usize = 4 * 1024;
    pub const DEFAULT_MAX_THREAD_NUM: usize = 100;

    pub fn new(capacity_bytes: usize, max_thread_count: usize, object_len: usize) -> Self {
        let object_len = object_len.max(1).min(capacity_bytes.max(1));
        let chunk_size = Self::DEFAULT_CHUNK_SIZE
            .min(capacity_bytes.max(object_len))
            .max(object_len);
        Self {
            live: HashMap::new(),
            free_list: Vec::new(),
            next_ptr: 1 << 44,
            capacity_bytes,
            max_thread_count,
            object_len,
            chunk_size,
            num_occupied_bytes: 0,
            num_allocated_objects: 0,
            num_freed_objects: 0,
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self::new(
            capacity,
            Self::DEFAULT_MAX_THREAD_NUM,
            Self::DEFAULT_OBJECT_LEN,
        )
    }

    pub fn with_capacity_and_object_len(capacity: usize, object_len: usize) -> Self {
        Self::new(capacity, Self::DEFAULT_MAX_THREAD_NUM, object_len)
    }

    pub fn pmem(
        _path: impl AsRef<Path>,
        _flush_policy: FlushPolicy,
        capacity_bytes: usize,
        max_thread_count: usize,
        object_len: usize,
    ) -> Self {
        Self::new(capacity_bytes, max_thread_count, object_len)
    }

    pub fn max_thread_count(&self) -> usize {
        self.max_thread_count
    }

    pub fn object_len(&self) -> usize {
        self.object_len
    }

    pub fn chunk_size(&self) -> usize {
        self.chunk_size
    }

    pub fn allocated_chunk_count(&self) -> usize {
        if self.num_occupied_bytes == 0 {
            0
        } else {
            self.num_occupied_bytes.div_ceil(self.chunk_size)
        }
    }

    fn ensure_chunk_capacity_for_one_object(&mut self) -> Result<(), CacheError> {
        let live_and_cached_objects = self.live.len().saturating_add(self.free_list.len());
        if live_and_cached_objects < self.num_occupied_bytes / self.object_len {
            return Ok(());
        }
        let next_occupied = self.num_occupied_bytes.saturating_add(self.chunk_size);
        if self.capacity_bytes != 0 && next_occupied > self.capacity_bytes {
            return Err(CacheError::CapacityExceeded);
        }
        self.num_occupied_bytes = next_occupied;
        Ok(())
    }

    pub fn write(&mut self, ptr: AllocatorAddress, data: &[u8]) -> Result<(), CacheError> {
        let object = self.live.get_mut(&ptr).ok_or(CacheError::NotFound)?;
        if data.len() > object.data.len() {
            return Err(CacheError::CapacityExceeded);
        }
        object.data[..data.len()].copy_from_slice(data);
        object.requested_len = data.len();
        Ok(())
    }

    pub fn read(&self, ptr: AllocatorAddress) -> Result<&[u8], CacheError> {
        let object = self.live.get(&ptr).ok_or(CacheError::NotFound)?;
        Ok(&object.data[..object.requested_len.min(object.data.len())])
    }

    pub fn sealed(&self, ptr: AllocatorAddress) -> bool {
        self.live.get(&ptr).is_some_and(|object| object.sealed)
    }

    pub fn crc32(&self, ptr: AllocatorAddress) -> Option<u32> {
        self.live.get(&ptr).and_then(|object| object.crc32)
    }

    #[allow(non_snake_case)]
    pub fn Allocate(&mut self, len: usize) -> Result<AllocatorAddress, CacheError> {
        self.allocate(len)
    }

    #[allow(non_snake_case)]
    pub fn Contains(&self, ptr: AllocatorAddress) -> bool {
        self.contains(ptr)
    }

    #[allow(non_snake_case)]
    pub fn Free(&mut self, ptr: AllocatorAddress, len: usize) -> Result<(), CacheError> {
        self.free(ptr, len)
    }

    #[allow(non_snake_case)]
    pub fn Seal(&mut self, ptr: AllocatorAddress) -> Result<(), CacheError> {
        self.seal(ptr)
    }

    #[allow(non_snake_case)]
    pub fn SealWithCRC(
        &mut self,
        ptr: AllocatorAddress,
        len: usize,
        crc32: u32,
    ) -> Result<(), CacheError> {
        self.seal_with_crc(ptr, len, crc32)
    }

    #[allow(non_snake_case)]
    pub fn GetStats(&self) -> Result<AllocatorStats, CacheError> {
        self.stats()
    }

    #[allow(non_snake_case)]
    pub fn Capacity(&self) -> Result<usize, CacheError> {
        self.capacity()
    }

    #[allow(non_snake_case)]
    pub fn TEST_GetAllocMetrics(&self) -> Result<AllocatorStats, CacheError> {
        self.stats()
    }

    pub fn test_global_free_list_size(&self) -> usize {
        self.free_list.len()
    }

    #[allow(non_snake_case)]
    pub fn TEST_GetGobalFreeListSize(&self) -> usize {
        self.test_global_free_list_size()
    }
}

impl CacheAllocatorApi for PoolBasedMemoryAllocatorBase {
    fn allocate(&mut self, len: usize) -> Result<AllocatorAddress, CacheError> {
        if len == 0 || len >= self.object_len {
            return Err(CacheError::CapacityExceeded);
        }
        let ptr = if let Some(ptr) = self.free_list.pop() {
            ptr
        } else {
            self.ensure_chunk_capacity_for_one_object()?;
            let ptr = self.next_ptr;
            self.next_ptr = self.next_ptr.saturating_add(1).max(1 << 44);
            ptr
        };
        self.live.insert(
            ptr,
            PoolObject {
                data: vec![0; self.object_len],
                requested_len: len,
                sealed: false,
                crc32: None,
            },
        );
        self.num_allocated_objects = self.num_allocated_objects.saturating_add(1);
        Ok(ptr)
    }

    fn contains(&self, ptr: AllocatorAddress) -> bool {
        self.live.contains_key(&ptr)
    }

    fn free(&mut self, ptr: AllocatorAddress, _len: usize) -> Result<(), CacheError> {
        let _object = self.live.remove(&ptr).ok_or(CacheError::NotFound)?;
        self.free_list.push(ptr);
        self.num_freed_objects = self.num_freed_objects.saturating_add(1);
        Ok(())
    }

    fn seal(&mut self, ptr: AllocatorAddress) -> Result<(), CacheError> {
        let object = self.live.get_mut(&ptr).ok_or(CacheError::NotFound)?;
        object.sealed = true;
        Ok(())
    }

    fn seal_with_crc(
        &mut self,
        ptr: AllocatorAddress,
        len: usize,
        crc32: u32,
    ) -> Result<(), CacheError> {
        let object = self.live.get_mut(&ptr).ok_or(CacheError::NotFound)?;
        if len > object.data.len() {
            return Err(CacheError::CapacityExceeded);
        }
        object.requested_len = len;
        object.sealed = true;
        object.crc32 = Some(crc32);
        Ok(())
    }

    fn stats(&self) -> Result<AllocatorStats, CacheError> {
        Ok(AllocatorStats {
            num_allocated_bytes: self.num_allocated_objects.saturating_mul(self.object_len),
            num_freed_bytes: self.free_list.len().saturating_mul(self.object_len),
            num_occupied_bytes: self.num_occupied_bytes,
        })
    }

    fn capacity(&self) -> Result<usize, CacheError> {
        Ok(self.capacity_bytes)
    }
}

impl MemStorageAllocatorApi for PoolBasedMemoryAllocatorBase {
    fn write_region(&mut self, ptr: AllocatorAddress, data: &[u8]) -> Result<(), CacheError> {
        self.write(ptr, data)
    }

    fn read_region(&self, ptr: AllocatorAddress) -> Result<&[u8], CacheError> {
        self.read(ptr)
    }
}

pub type CacheAllocator = SimpleLogBasedMemoryAllocator;
pub type LogBasedMemoryAllocator = SimpleLogBasedMemoryAllocator;
pub type LogBasedMemoryAllocatorDram = SimpleLogBasedMemoryAllocator;
pub type LogBasedMemoryAllocatorPmem = SimpleLogBasedMemoryAllocator;
pub type PoolBasedMemoryAllocator = PoolBasedMemoryAllocatorBase;
pub type PoolBasedMemoryAllocatorDram = PoolBasedMemoryAllocatorBase;
pub type PoolBasedMemoryAllocatorPmem = PoolBasedMemoryAllocatorBase;

#[derive(Debug)]
pub struct CacheExecutorHandle {
    name: String,
    thread_count: usize,
    numa_id: Option<usize>,
    submitted_tasks: Mutex<u64>,
}

impl CacheExecutorHandle {
    pub fn new(name: impl Into<String>, thread_count: usize, numa_id: Option<usize>) -> Self {
        Self {
            name: name.into(),
            thread_count: thread_count.max(1),
            numa_id,
            submitted_tasks: Mutex::new(0),
        }
    }

    pub fn add<F>(&self, func: F)
    where
        F: FnOnce() + Send + 'static,
    {
        *self
            .submitted_tasks
            .lock()
            .expect("executor submitted task counter lock poisoned") += 1;
        func();
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn thread_count(&self) -> usize {
        self.thread_count
    }

    pub fn numa_id(&self) -> Option<usize> {
        self.numa_id
    }

    pub fn submitted_tasks(&self) -> u64 {
        *self
            .submitted_tasks
            .lock()
            .expect("executor submitted task counter lock poisoned")
    }

    #[allow(non_snake_case)]
    pub fn Add<F>(&self, func: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.add(func);
    }
}

pub type SharedCacheExecutor = Arc<CacheExecutorHandle>;

#[derive(Debug, Clone, Copy)]
pub struct CacheExecutorConfig {
    pub common_executor_num_threads: usize,
    pub num_gc_workers: usize,
    pub used_num_numa_nodes: usize,
    pub num_pmem_cache_per_numa_writer_threads: usize,
}

impl Default for CacheExecutorConfig {
    fn default() -> Self {
        Self {
            common_executor_num_threads: 1,
            num_gc_workers: 1,
            used_num_numa_nodes: 1,
            num_pmem_cache_per_numa_writer_threads: 1,
        }
    }
}

#[derive(Debug, Default)]
struct CacheExecutorState {
    config: CacheExecutorConfig,
    common_executor: Option<SharedCacheExecutor>,
    gc_executor: Option<SharedCacheExecutor>,
    pmem_executors: Vec<SharedCacheExecutor>,
}

fn cache_executor_state() -> &'static Mutex<CacheExecutorState> {
    static STATE: OnceLock<Mutex<CacheExecutorState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(CacheExecutorState::default()))
}

/// The process-wide thread pools a cache is meant to share.
///
/// **Standalone.** Nothing in [`MultiLayerCache`] asks for an executor: every
/// getter here is reached only from tests, and the cache does its work on the
/// calling thread or on its own writeback workers. It is complete and tested
/// on its own.
///
/// Documented rather than left to be discovered, because a public type in a
/// cache library reads as something the cache uses -- and because
/// [`Self::configure`] and [`Self::destroy_all_executors`] reset one global,
/// which is worth knowing before calling either from more than one place.
pub struct CacheExecutor;

impl CacheExecutor {
    pub fn configure(config: CacheExecutorConfig) {
        let mut state = cache_executor_state()
            .lock()
            .expect("cache executor state lock poisoned");
        state.config = config;
        state.common_executor = None;
        state.gc_executor = None;
        state.pmem_executors.clear();
    }

    pub fn common_executor() -> SharedCacheExecutor {
        let mut state = cache_executor_state()
            .lock()
            .expect("cache executor state lock poisoned");
        if let Some(executor) = state.common_executor.as_ref() {
            return Arc::clone(executor);
        }
        let executor = Arc::new(CacheExecutorHandle::new(
            "CacheCommonThreadPool",
            state.config.common_executor_num_threads,
            None,
        ));
        state.common_executor = Some(Arc::clone(&executor));
        executor
    }

    pub fn gc_executor() -> SharedCacheExecutor {
        let mut state = cache_executor_state()
            .lock()
            .expect("cache executor state lock poisoned");
        if let Some(executor) = state.gc_executor.as_ref() {
            return Arc::clone(executor);
        }
        let executor = Arc::new(CacheExecutorHandle::new(
            "CacheGCThreadPool",
            state.config.num_gc_workers,
            None,
        ));
        state.gc_executor = Some(Arc::clone(&executor));
        executor
    }

    pub fn pmem_executors() -> Vec<SharedCacheExecutor> {
        let mut state = cache_executor_state()
            .lock()
            .expect("cache executor state lock poisoned");
        if state.pmem_executors.is_empty() {
            let numa_count = state.config.used_num_numa_nodes.max(1);
            state.pmem_executors = (0..numa_count)
                .map(|numa_id| {
                    Arc::new(CacheExecutorHandle::new(
                        format!("PmemNuma{numa_id}"),
                        state.config.num_pmem_cache_per_numa_writer_threads,
                        Some(numa_id),
                    ))
                })
                .collect();
        }
        state.pmem_executors.iter().map(Arc::clone).collect()
    }

    pub fn destroy_all_executors() {
        let mut state = cache_executor_state()
            .lock()
            .expect("cache executor state lock poisoned");
        state.common_executor = None;
        state.gc_executor = None;
        state.pmem_executors.clear();
    }

    pub fn initialized_executor_count() -> usize {
        let state = cache_executor_state()
            .lock()
            .expect("cache executor state lock poisoned");
        usize::from(state.common_executor.is_some())
            + usize::from(state.gc_executor.is_some())
            + state.pmem_executors.len()
    }

    #[allow(non_snake_case)]
    /// See [`Self::configure`].
    pub fn Configure(config: CacheExecutorConfig) {
        Self::configure(config);
    }

    #[allow(non_snake_case)]
    pub fn GetCommonExecutor() -> SharedCacheExecutor {
        Self::common_executor()
    }

    #[allow(non_snake_case)]
    pub fn GetGCExecutor() -> SharedCacheExecutor {
        Self::gc_executor()
    }

    #[allow(non_snake_case)]
    pub fn GetPmemExecutors() -> Vec<SharedCacheExecutor> {
        Self::pmem_executors()
    }

    #[allow(non_snake_case)]
    pub fn DestroyAllExecutors() {
        Self::destroy_all_executors();
    }
}

/// Milliseconds between collection checks in [`StorageGcController::poll`].
pub const GC_DEFAULT_CHECK_INTERVAL_MS: u64 = 1_000;

#[derive(Debug, Clone)]
pub struct StorageGcController {
    allocator: SimpleLogBasedMemoryAllocator,
    force_gc: bool,
    pause_gc: bool,
    enable_gc: bool,
    free_mem_min: usize,
    fragmentation_ratio_max: u8,
    gc_check_interval_ms: u64,
    last_gc_check: Option<Instant>,
    complete_gc_chunks: i64,
    fly_gc_chunks: i64,
    complete_gc_tasks: i64,
}

impl StorageGcController {
    pub fn new(allocator: SimpleLogBasedMemoryAllocator, force_gc: bool) -> Self {
        Self::with_thresholds(allocator, force_gc, 0, 50)
    }

    pub fn with_thresholds(
        allocator: SimpleLogBasedMemoryAllocator,
        force_gc: bool,
        free_mem_min: usize,
        fragmentation_ratio_max: u8,
    ) -> Self {
        Self {
            allocator,
            force_gc,
            pause_gc: false,
            enable_gc: false,
            free_mem_min,
            fragmentation_ratio_max,
            gc_check_interval_ms: GC_DEFAULT_CHECK_INTERVAL_MS,
            last_gc_check: None,
            complete_gc_chunks: 0,
            fly_gc_chunks: 0,
            complete_gc_tasks: 0,
        }
    }

    pub fn start(&mut self) {
        self.enable_gc = true;
    }

    pub fn stop(&mut self) {
        self.enable_gc = false;
        self.wait_all_task_complete();
    }

    pub fn set_pause_gc(&mut self, pause: bool) {
        self.pause_gc = pause;
    }

    pub fn gc_check_interval_ms(&self) -> u64 {
        self.gc_check_interval_ms
    }

    pub fn set_gc_check_interval_ms(&mut self, interval_ms: u64) {
        self.gc_check_interval_ms = interval_ms;
    }

    /// Enable or disable collection without draining outstanding work, unlike
    /// [`StorageGcController::stop`].
    pub fn set_enable_gc(&mut self, enable: bool) {
        self.enable_gc = enable;
    }

    /// Check whether collection is due and, if so, run one round.
    ///
    /// This is the monitoring half of the controller: it decides *when* to
    /// look, while `pick_submit_chunks` decides what to reclaim once asked.
    /// Because the controller has no thread of its own, a caller drives this
    /// from its own loop and the interval keeps the fragmentation check off
    /// the hot path rather than running it on every call. Returns the number
    /// of chunks reclaimed.
    pub fn poll(&mut self) -> Result<usize, CacheError> {
        if !self.enable_gc || self.pause_gc {
            return Ok(0);
        }
        let now = Instant::now();
        let due = match self.last_gc_check {
            None => true,
            Some(last) => {
                now.duration_since(last) >= Duration::from_millis(self.gc_check_interval_ms)
            }
        };
        if !due {
            return Ok(0);
        }
        self.last_gc_check = Some(now);
        self.pick_submit_chunks()
    }

    // Completion barrier: `fly_gc_chunks` is maintained by the GC executor's
    // submit/complete accounting, not mutated in this spin body.
    #[allow(clippy::while_immutable_condition)]
    pub fn wait_all_task_complete(&self) {
        while self.fly_gc_chunks > 0 {
            std::thread::yield_now();
        }
    }

    pub fn need_gc(&self) -> bool {
        if self.pause_gc {
            return false;
        }
        if self.force_gc {
            return true;
        }

        let Ok(stats) = self.allocator.GetStats() else {
            return false;
        };
        let Ok(capacity) = self.allocator.Capacity() else {
            return false;
        };
        if capacity > 0 && capacity.saturating_sub(stats.NumOccupiedBytes()) < self.free_mem_min {
            return true;
        }
        let allocated_live = stats
            .NumAllocatedBytes()
            .saturating_sub(stats.NumFreedBytes());
        if stats.NumOccupiedBytes() == 0 || allocated_live >= stats.NumOccupiedBytes() {
            return false;
        }
        let fragmented = stats.NumOccupiedBytes().saturating_sub(allocated_live);
        fragmented.saturating_mul(100)
            > stats
                .NumOccupiedBytes()
                .saturating_mul(self.fragmentation_ratio_max as usize)
    }

    pub fn pick_submit_chunks(&mut self) -> Result<usize, CacheError> {
        if !self.enable_gc || !self.need_gc() {
            return Ok(0);
        }
        let mut chunks = Vec::new();
        self.allocator.IterateRecyclableChunkMeta(|meta| {
            chunks.push(meta.id);
            true
        })?;
        self.gc_job(chunks)
    }

    pub fn gc_job(&mut self, chunks: Vec<ChunkId>) -> Result<usize, CacheError> {
        if chunks.is_empty() {
            return Ok(0);
        }
        let chunk_count = chunks.len();
        self.fly_gc_chunks = self.fly_gc_chunks.saturating_add(chunk_count as i64);
        let gc_result = self.allocator.GC(&chunks);
        if gc_result.is_ok() {
            self.complete_gc_chunks = self.complete_gc_chunks.saturating_add(chunk_count as i64);
            self.complete_gc_tasks = self.complete_gc_tasks.saturating_add(1);
        }
        self.fly_gc_chunks = self.fly_gc_chunks.saturating_sub(chunk_count as i64);
        gc_result.map(|_| chunk_count)
    }

    pub fn allocator(&self) -> &SimpleLogBasedMemoryAllocator {
        &self.allocator
    }

    pub fn allocator_mut(&mut self) -> &mut SimpleLogBasedMemoryAllocator {
        &mut self.allocator
    }

    pub fn enable_gc(&self) -> bool {
        self.enable_gc
    }

    pub fn pause_gc(&self) -> bool {
        self.pause_gc
    }

    pub fn fly_gc_chunks(&self) -> i64 {
        self.fly_gc_chunks
    }

    pub fn complete_gc_chunks(&self) -> i64 {
        self.complete_gc_chunks
    }

    pub fn complete_gc_tasks(&self) -> i64 {
        self.complete_gc_tasks
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
    pub fn SetPauseGC(&mut self, pause: bool) {
        self.set_pause_gc(pause);
    }

    #[allow(non_snake_case)]
    pub fn WaitAllTaskComplete(&self) {
        self.wait_all_task_complete();
    }

    #[allow(non_snake_case)]
    pub fn NeedGc(&self) -> bool {
        self.need_gc()
    }

    #[allow(non_snake_case)]
    pub fn PickSubmitChunks(&mut self) -> Result<usize, CacheError> {
        self.pick_submit_chunks()
    }

    #[allow(non_snake_case)]
    pub fn TEST_GetNumGcCompleteChunks(&self) -> i64 {
        self.complete_gc_chunks()
    }

    #[allow(non_snake_case)]
    pub fn TEST_GetNumGcCompleteTasks(&self) -> i64 {
        self.complete_gc_tasks()
    }
}

