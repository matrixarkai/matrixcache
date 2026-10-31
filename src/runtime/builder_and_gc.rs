// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

impl MatrixCacheBuilder {
    pub fn build_cache(options: CacheOptions) -> MultiLayerCache {
        MultiLayerCache::with_options(options)
    }

    pub fn build_sharded_cache(
        options: CacheOptions,
        shard_count: usize,
    ) -> ShardedMultiLayerCache {
        ShardedMultiLayerCache::with_options(options, shard_count)
    }

    pub fn build_zero_copy_cache(options: CacheOptions) -> MultiLayerCache {
        MultiLayerCache::with_options(options)
    }

    pub fn build_cache_api(options: CacheOptions) -> Box<dyn CacheApi> {
        Box::new(Self::build_cache(options))
    }

    pub fn build_sharded_cache_api(options: CacheOptions, shard_count: usize) -> Box<dyn CacheApi> {
        Box::new(Self::build_sharded_cache(options, shard_count))
    }

    pub fn build_zero_copy_cache_api(options: CacheOptions) -> Box<dyn ZeroCopyCacheApi> {
        Box::new(Self::build_zero_copy_cache(options))
    }

    pub fn build_sharded_zero_copy_cache_api(
        options: CacheOptions,
        shard_count: usize,
    ) -> Box<dyn ZeroCopyCacheApi> {
        Box::new(Self::build_sharded_cache(options, shard_count))
    }

    pub fn build_simple_lru_cache(capacity: usize) -> SimpleLruCache {
        SimpleLruCache::new(capacity)
    }

    pub fn build_zero_copy_simple_lru_cache(capacity: usize) -> ZeroCopySimpleLruCache {
        ZeroCopySimpleLruCache::new(capacity)
    }

    pub fn build_concurrent_simple_lru_cache(capacity: usize) -> ConcurrentSimpleLruCache {
        ConcurrentSimpleLruCache::new(capacity)
    }

    pub fn build_in_process_memcached_cache(capacity: usize) -> InProcessMemcachedCache {
        InProcessMemcachedCache::new(capacity)
    }

    pub fn build_memcached_wrapper(capacity: usize) -> InProcessMemcachedCache {
        Self::build_in_process_memcached_cache(capacity)
    }

    pub fn build_flexible_cache(
        capacity: usize,
        policy: impl AsRef<str>,
        engine: impl AsRef<str>,
        pmem_paths: impl IntoIterator<Item = PathBuf>,
        ssd_paths: impl IntoIterator<Item = PathBuf>,
    ) -> FlexibleCache {
        FlexibleCache::new(capacity, policy, engine, pmem_paths, ssd_paths)
    }

    pub fn build_flexible_cache_from_path_strings(
        capacity: usize,
        policy: impl AsRef<str>,
        engine: impl AsRef<str>,
        pmem_paths: impl IntoIterator<Item = String>,
        ssd_paths: impl IntoIterator<Item = String>,
    ) -> FlexibleCache {
        FlexibleCache::from_path_strings(capacity, policy, engine, pmem_paths, ssd_paths)
    }

    pub fn build_multi_tier_string_cache(options: CacheOptions) -> MultiTierStringCache {
        MultiTierStringCache::new(options)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn build_multi_tier_cache(
        dram_capacity: usize,
        pmem_capacity: usize,
        ssd_capacity: usize,
        policy: impl AsRef<str>,
        pmem_paths: impl IntoIterator<Item = PathBuf>,
        ssd_paths: impl IntoIterator<Item = PathBuf>,
        dram_pmem_data_placement_type: impl AsRef<str>,
        enable_eviction: bool,
        side_by_side_dram_pmem_placement_threshold: usize,
        ssd_storage_engine: impl AsRef<str>,
    ) -> MultiTierCache {
        MultiTierCache::new(
            dram_capacity,
            pmem_capacity,
            ssd_capacity,
            policy,
            pmem_paths,
            ssd_paths,
            dram_pmem_data_placement_type,
            enable_eviction,
            side_by_side_dram_pmem_placement_threshold,
            ssd_storage_engine,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn build_multi_tier_cache_from_path_strings(
        dram_capacity: usize,
        pmem_capacity: usize,
        ssd_capacity: usize,
        policy: impl AsRef<str>,
        pmem_paths: impl IntoIterator<Item = String>,
        ssd_paths: impl IntoIterator<Item = String>,
        dram_pmem_data_placement_type: impl AsRef<str>,
        enable_eviction: bool,
        side_by_side_dram_pmem_placement_threshold: usize,
        ssd_storage_engine: impl AsRef<str>,
    ) -> MultiTierCache {
        MultiTierCache::from_path_strings(
            dram_capacity,
            pmem_capacity,
            ssd_capacity,
            policy,
            pmem_paths,
            ssd_paths,
            dram_pmem_data_placement_type,
            enable_eviction,
            side_by_side_dram_pmem_placement_threshold,
            ssd_storage_engine,
        )
    }

    #[allow(non_snake_case)]
    pub fn BuildCache(options: CacheOptions) -> MultiLayerCache {
        Self::build_cache(options)
    }

    #[allow(non_snake_case)]
    pub fn BuildShardedCache(options: CacheOptions, shard_count: usize) -> ShardedMultiLayerCache {
        Self::build_sharded_cache(options, shard_count)
    }

    #[allow(non_snake_case)]
    pub fn BuildZeroCopyCache(options: CacheOptions) -> MultiLayerCache {
        Self::build_zero_copy_cache(options)
    }

    #[allow(non_snake_case)]
    pub fn BuildCacheApi(options: CacheOptions) -> Box<dyn CacheApi> {
        Self::build_cache_api(options)
    }

    #[allow(non_snake_case)]
    pub fn BuildShardedCacheApi(options: CacheOptions, shard_count: usize) -> Box<dyn CacheApi> {
        Self::build_sharded_cache_api(options, shard_count)
    }

    #[allow(non_snake_case)]
    pub fn BuildZeroCopyCacheApi(options: CacheOptions) -> Box<dyn ZeroCopyCacheApi> {
        Self::build_zero_copy_cache_api(options)
    }

    #[allow(non_snake_case)]
    pub fn BuildShardedZeroCopyCacheApi(
        options: CacheOptions,
        shard_count: usize,
    ) -> Box<dyn ZeroCopyCacheApi> {
        Self::build_sharded_zero_copy_cache_api(options, shard_count)
    }

    #[allow(non_snake_case)]
    pub fn BuildSimpleLRUCache(capacity: usize) -> SimpleLruCache {
        Self::build_simple_lru_cache(capacity)
    }

    #[allow(non_snake_case)]
    pub fn BuildZeroCopySimpleLRUCache(capacity: usize) -> ZeroCopySimpleLruCache {
        Self::build_zero_copy_simple_lru_cache(capacity)
    }

    #[allow(non_snake_case)]
    pub fn BuildConcurrentSimpleLRUCache(capacity: usize) -> ConcurrentSimpleLruCache {
        Self::build_concurrent_simple_lru_cache(capacity)
    }

    #[allow(non_snake_case)]
    pub fn BuildInProcessMemcachedCache(capacity: usize) -> InProcessMemcachedCache {
        Self::build_in_process_memcached_cache(capacity)
    }

    #[allow(non_snake_case)]
    pub fn BuildMemcachedWrapper(capacity: usize) -> InProcessMemcachedCache {
        Self::build_in_process_memcached_cache(capacity)
    }

    #[allow(non_snake_case)]
    pub fn BuildFlexibleCache(
        capacity: usize,
        policy: impl AsRef<str>,
        engine: impl AsRef<str>,
        pmem_paths: impl IntoIterator<Item = PathBuf>,
        ssd_paths: impl IntoIterator<Item = PathBuf>,
    ) -> FlexibleCache {
        Self::build_flexible_cache(capacity, policy, engine, pmem_paths, ssd_paths)
    }

    #[allow(non_snake_case)]
    pub fn BuildFlexibleCacheFromPathStrings(
        capacity: usize,
        policy: impl AsRef<str>,
        engine: impl AsRef<str>,
        pmem_paths: impl IntoIterator<Item = String>,
        ssd_paths: impl IntoIterator<Item = String>,
    ) -> FlexibleCache {
        Self::build_flexible_cache_from_path_strings(
            capacity, policy, engine, pmem_paths, ssd_paths,
        )
    }

    #[allow(non_snake_case)]
    pub fn BuildMultiTierStringCache(options: CacheOptions) -> MultiTierStringCache {
        Self::build_multi_tier_string_cache(options)
    }

    #[allow(non_snake_case, clippy::too_many_arguments)]
    pub fn BuildMultiTierCache(
        dram_capacity: usize,
        pmem_capacity: usize,
        ssd_capacity: usize,
        policy: impl AsRef<str>,
        pmem_paths: impl IntoIterator<Item = PathBuf>,
        ssd_paths: impl IntoIterator<Item = PathBuf>,
        dram_pmem_data_placement_type: impl AsRef<str>,
        enable_eviction: bool,
        side_by_side_dram_pmem_placement_threshold: usize,
        ssd_storage_engine: impl AsRef<str>,
    ) -> MultiTierCache {
        Self::build_multi_tier_cache(
            dram_capacity,
            pmem_capacity,
            ssd_capacity,
            policy,
            pmem_paths,
            ssd_paths,
            dram_pmem_data_placement_type,
            enable_eviction,
            side_by_side_dram_pmem_placement_threshold,
            ssd_storage_engine,
        )
    }

    #[allow(non_snake_case, clippy::too_many_arguments)]
    pub fn BuildMultiTierCacheFromPathStrings(
        dram_capacity: usize,
        pmem_capacity: usize,
        ssd_capacity: usize,
        policy: impl AsRef<str>,
        pmem_paths: impl IntoIterator<Item = String>,
        ssd_paths: impl IntoIterator<Item = String>,
        dram_pmem_data_placement_type: impl AsRef<str>,
        enable_eviction: bool,
        side_by_side_dram_pmem_placement_threshold: usize,
        ssd_storage_engine: impl AsRef<str>,
    ) -> MultiTierCache {
        Self::build_multi_tier_cache_from_path_strings(
            dram_capacity,
            pmem_capacity,
            ssd_capacity,
            policy,
            pmem_paths,
            ssd_paths,
            dram_pmem_data_placement_type,
            enable_eviction,
            side_by_side_dram_pmem_placement_threshold,
            ssd_storage_engine,
        )
    }
}

const CACHE_BLOCK_MAGIC: &[u8; 8] = b"TSBCACHE";
const CACHE_BLOCK_VERSION: u8 = 1;
const CACHE_CODEC_RAW: u8 = 0;
const CACHE_CODEC_ZSTD: u8 = 1;
const CACHE_HEADER_LEN: usize = 8 + 1 + 1 + 8 + 8;
/// The level a codec is built with when the first thing a thread does is
/// decode. Decompression takes no level, so this only decides what an encode
/// on the same thread would start from -- and `compress` re-parameterises when
/// the level it is handed differs.
const CACHE_FALLBACK_COMPRESSION_LEVEL: i32 = 0;

/// Frame a value as a block, with one copy of the payload.
///
/// Every arm of this used to `value.to_vec()` the payload into an owned buffer
/// and then copy that buffer into the block -- two payload-sized allocations
/// and two copies per block written, where the block itself is the only buffer
/// anyone keeps. Three of the four arms store the value raw, so three of the
/// four were paying for a copy that existed only to be copied again.
///
/// The compressed payload still needs a buffer of its own, because whether it
/// is used at all is decided by how big it turns out to be. The raw payload
/// does not: it is already in the caller's slice.
/// The compressor and the decompressor a thread keeps between calls.
///
/// Every call used to build its own. The context itself is zstd's, allocated
/// through C and so invisible to a Rust counting allocator, but the buffers
/// the convenience functions wrap it in are not: `zstd::stream::encode_all`
/// allocates a 32 KiB output buffer per call, and `zstd::stream::decode_all`
/// a 128 KiB input buffer sized from `DCtx::in_size()`. Measured on their own,
/// those two are 32,786 and 132,099 bytes an operation; the context behind
/// them is more allocation again that this instrument cannot see.
///
/// Held per thread rather than per cache, because the codec carries no cache
/// state -- a level and a scratch buffer -- and a shard's writes can arrive on
/// any thread. The price is that a thread which has compressed once keeps
/// zstd's context until it exits: bounded resident state per working thread,
/// against unbounded churn per call.
struct CacheBlockCodec {
    /// Reset per call with `reinit`, which keeps the workspace and starts a
    /// new frame.
    encoder: zstd::stream::raw::Encoder<'static>,
    /// The level the encoder is currently parameterised for, so a run of
    /// writes at one level does not re-set it on every block.
    level: i32,
    /// Where a compressed payload is built. Reused, so a payload that turns
    /// out not to be worth keeping costs no allocation at all.
    scratch: Vec<u8>,
    decoder: zstd::bulk::Decompressor<'static>,
}

impl CacheBlockCodec {
    fn new(level: i32) -> Result<Self, CacheError> {
        Ok(Self {
            encoder: zstd::stream::raw::Encoder::new(level)?,
            level,
            scratch: Vec::new(),
            decoder: zstd::bulk::Decompressor::new()?,
        })
    }

    /// Compress `value` into the scratch buffer and borrow the result.
    ///
    /// The bytes are the ones `zstd::stream::encode_all` wrote, frame for
    /// frame -- `the_compressed_payload_is_what_the_streaming_encoder_wrote`
    /// sweeps fixtures and levels and compares them. Two things make that
    /// true and are easy to lose:
    ///
    /// * the level is a parameter of the context, so a reused context has to
    ///   be re-parameterised when the level changes;
    /// * an empty value is not run through the compressor at all. `encode_all`
    ///   copies zero bytes into the encoder and so never calls it, and the
    ///   frame it then ends is a single-segment one. Handing the compressor an
    ///   empty input instead makes it declare a window, and the nine bytes
    ///   come out different.
    fn compress(&mut self, value: &[u8], level: i32) -> Result<&[u8], CacheError> {
        use zstd::stream::raw::Operation as _;

        self.encoder.reinit()?;
        if level != self.level {
            self.encoder
                .set_parameter(zstd::zstd_safe::CParameter::CompressionLevel(level))?;
            self.level = level;
        }
        let needed = zstd::zstd_safe::compress_bound(value.len());
        self.scratch.clear();
        // `reserve` is relative to the LENGTH, which is zero here, so this asks
        // for the whole bound. Asking for the difference from what the buffer
        // already holds reads as the same thing and is not: on a cleared buffer
        // `reserve(needed - capacity)` guarantees only `needed - capacity` of
        // room, so a buffer already holding more than half of what the next
        // block needs is left exactly as short as it was. An ascending sequence
        // of value sizes is enough to reach it -- 256 bytes then 1,024 left a
        // 771-byte buffer for a 1,092-byte bound -- and an output buffer that
        // small makes the compressor refuse the block.
        self.scratch.reserve(needed);
        let mut out = zstd::zstd_safe::OutBuffer::around(&mut self.scratch);
        if !value.is_empty() {
            let mut input = zstd::zstd_safe::InBuffer::around(value);
            self.encoder.run(&mut input, &mut out)?;
            if input.pos() != value.len() {
                return Err(CacheError::CorruptBlock(format!(
                    "the compressor took {} of {} bytes in one pass",
                    input.pos(),
                    value.len()
                )));
            }
        }
        // `compress_bound` is zstd's own worst case for a single frame and the
        // buffer above holds it, so one pass ends it. A non-zero hint would
        // mean output still pending with nowhere to put it, which is why this
        // reports rather than loops -- and which is why the sizing above is
        // tested rather than argued: the first version of it was short, and
        // this is the arm that said so.
        if self.encoder.finish(&mut out, false)? != 0 {
            return Err(CacheError::CorruptBlock(
                "the compressor had more to write than its own bound allows".to_string(),
            ));
        }
        let written = out.as_slice().len();
        Ok(&self.scratch[..written])
    }

    /// Decompress `payload`, which the block header says holds `original_len`
    /// bytes.
    ///
    /// Having the length means the output is allocated once at exactly the
    /// size it needs. `zstd::stream::decode_all` cannot do that: the frames
    /// this codec writes record no content size, so it reads through a 128 KiB
    /// buffer and grows the output as it goes.
    ///
    /// `try_reserve_exact` rather than `reserve`, because `original_len` is
    /// eight bytes off a device: a corrupt header asking for more than fits is
    /// a rejected block, not an abort.
    fn decompress(&mut self, payload: &[u8], original_len: usize) -> Result<Vec<u8>, CacheError> {
        let mut decoded = Vec::new();
        if decoded.try_reserve_exact(original_len).is_err() {
            return Err(CacheError::CorruptBlock(format!(
                "a block claiming {original_len} bytes will not fit in memory"
            )));
        }
        match self.decoder.decompress_to_buffer(payload, &mut decoded) {
            Ok(_) => Ok(decoded),
            // The sized decode refuses a payload that expands past what the
            // header claims, and refuses it with zstd's words about a
            // destination buffer. The unsized one reaches the length check
            // below this call, which names the mismatch -- and that is what
            // every caller and every test has always been told. So a failure
            // here falls back to it rather than changing the reason a corrupt
            // block is rejected. It runs only for blocks already being
            // refused, never on a read that succeeds.
            Err(_) => Ok(zstd::stream::decode_all(payload)?),
        }
    }
}

thread_local! {
    static CACHE_BLOCK_CODEC: RefCell<Option<CacheBlockCodec>> = const { RefCell::new(None) };
}

/// Run `use_codec` against this thread's codec, building it on first use.
///
/// Lazily, so a process that never compresses never pays for a context.
fn with_cache_block_codec<T>(
    level: i32,
    use_codec: impl FnOnce(&mut CacheBlockCodec) -> Result<T, CacheError>,
) -> Result<T, CacheError> {
    CACHE_BLOCK_CODEC.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = Some(CacheBlockCodec::new(level)?);
        }
        use_codec(slot.as_mut().expect("the codec was just built"))
    })
}

/// Frame a payload that is already in the form it will be stored in.
///
/// Split out of `encode_cache_block` so the compressed arm can frame straight
/// from the codec's scratch buffer while it is still borrowed. The bytes are
/// the ones that arm always wrote.
fn frame_cache_block(original_len: usize, codec: u8, payload: &[u8]) -> Vec<u8> {
    let mut block = Vec::with_capacity(CACHE_HEADER_LEN + payload.len());
    block.extend_from_slice(CACHE_BLOCK_MAGIC);
    block.push(CACHE_BLOCK_VERSION);
    block.push(codec);
    block.extend_from_slice(&(original_len as u64).to_le_bytes());
    block.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    block.extend_from_slice(payload);
    block
}

fn encode_cache_block(value: &[u8], options: CacheBlockOptions) -> Result<Vec<u8>, CacheError> {
    match options.compression {
        CacheCompression::Zstd { level } if value.len() >= options.min_compress_bytes => {
            with_cache_block_codec(level, |codec| {
                let compressed = codec.compress(value, level)?;
                // Kept only if framing it is smaller than storing the value as
                // it is, which is the same comparison as before. The payload
                // that loses that comparison was never allocated: it is in the
                // codec's scratch, which the next block reuses.
                if CACHE_HEADER_LEN + compressed.len() < value.len() {
                    Ok(frame_cache_block(value.len(), CACHE_CODEC_ZSTD, compressed))
                } else {
                    Ok(frame_cache_block(value.len(), CACHE_CODEC_RAW, value))
                }
            })
        }
        CacheCompression::Zstd { .. } | CacheCompression::None => {
            Ok(frame_cache_block(value.len(), CACHE_CODEC_RAW, value))
        }
    }
}

#[cfg(not(feature = "rocksdb-ssd"))]
/// Write a block so a reader never sees half of one.
///
/// `durable` decides whether the write also survives the machine losing power:
/// with it, the block is flushed and the directory entry flushed after the
/// rename, which is two `fsync` calls and most of what a block write costs.
/// Without it, the rename still makes the block appear whole or not at all --
/// a reader can never see a torn block either way -- but a crash can lose
/// blocks the cache believed it had written.
///
/// Losing one is a miss, not a loss, unless something recovers this tier and
/// expects it to be complete. That is why the choice belongs to the caller and
/// why it defaults to durable.
fn write_cache_block_atomic(path: &Path, block: &[u8], durable: bool) -> Result<(), CacheError> {
    let temp_path = path.with_extension(format!(
        "cache_block.tmp.{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    {
        let mut temp = File::create(&temp_path)?;
        temp.write_all(block)?;
        temp.flush()?;
        if durable {
            temp.sync_all()?;
        }
    }
    fs::rename(&temp_path, path)?;
    if durable {
        sync_parent_dir(path)?;
    }
    Ok(())
}

/// Flush the directory entry, so a renamed file is still there after a crash.
///
/// Not gated to one backend: the persistent tier writes files in every build,
/// so it needs this in every build.
fn sync_parent_dir(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if let Ok(dir) = File::open(parent) {
            dir.sync_all()?;
        }
    }
    Ok(())
}

/// Decoded block bytes, in the form the read that asked for them needs.
///
/// A read that refills hands the bytes to **two** holders: the tier it refills
/// and the caller. Decoding into the handle the tier wants costs one
/// allocation for the tier and one copy for the caller.
///
/// A read that cannot refill -- an SSD-only instance, which never puts
/// anything back into memory -- has one holder, and a handle would have to be
/// copied again to hand back an owned value. That read decodes into the value
/// it is going to return.
///
/// Both of those used to be the same thing: decode into a `Vec`, clone the
/// `Vec` for the tier, and let the tier build an `Arc` from the clone. Three
/// payload-sized allocations to serve two holders, and the extra one grew with
/// the payload.
enum CacheDecodedBlock {
    /// Shared with the tier this read refills.
    Shared(Arc<[u8]>),
    /// Owned by the read, because nothing else wants these bytes.
    Owned(Vec<u8>),
}

impl CacheDecodedBlock {
    fn decode(block: &[u8], refilling: bool) -> Result<Self, CacheError> {
        if refilling {
            Ok(Self::Shared(decode_cache_block_shared(block)?))
        } else {
            Ok(Self::Owned(decode_cache_block(block)?))
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::Shared(bytes) => bytes.len(),
            Self::Owned(bytes) => bytes.len(),
        }
    }

    /// A handle to refill a tier with, or `None` from the form that has none.
    ///
    /// A read that asked for the owning form is a read that said it would not
    /// refill, so there is nothing to hand over.
    fn to_refill(&self) -> Option<Arc<[u8]>> {
        match self {
            Self::Shared(bytes) => Some(Arc::clone(bytes)),
            Self::Owned(_) => None,
        }
    }

    /// One owned copy, for a read that needs several.
    fn to_vec(&self) -> Vec<u8> {
        match self {
            Self::Shared(bytes) => bytes.to_vec(),
            Self::Owned(bytes) => bytes.clone(),
        }
    }

    /// The owned copy, for a read that needs one.
    ///
    /// The owning form moves rather than copies here, which is why the two
    /// forms are worth telling apart at all.
    fn into_vec(self) -> Vec<u8> {
        match self {
            Self::Shared(bytes) => bytes.to_vec(),
            Self::Owned(bytes) => bytes,
        }
    }
}

/// Decode a block into a handle the tiers and the caller share.
///
/// Seven read paths wrote `Arc::<[u8]>::from(decode_cache_block(..)?)`, which
/// for an uncompressed block is two payload-sized allocations and two copies:
/// one into the `Vec` the decode owns, one into the `Arc` that `Vec` is thrown
/// away for. An uncompressed payload is already a slice of the block, so the
/// handle can be built straight from it.
///
/// A compressed payload still decompresses into a `Vec` first -- building the
/// handle in place without one would need the unsafe this crate forbids.
fn decode_cache_block_shared(block: &[u8]) -> Result<Arc<[u8]>, CacheError> {
    if let Some(payload) = uncompressed_block_payload(block)? {
        return Ok(Arc::<[u8]>::from(payload));
    }
    Ok(Arc::<[u8]>::from(decode_cache_block(block)?))
}

fn cache_block_matches_value(block: &[u8], value: &[u8]) -> Result<bool, CacheError> {
    if let Some(payload) = uncompressed_block_payload(block)? {
        return Ok(payload == value);
    }
    Ok(decode_cache_block(block)? == value)
}

/// The payload of a block that needs no decompressing, or `None` when it does.
///
/// The header checks are the ones [`decode_cache_block`] makes, in the same
/// order, so a block either passes both or fails both.
fn uncompressed_block_payload(block: &[u8]) -> Result<Option<&[u8]>, CacheError> {
    if !block.starts_with(CACHE_BLOCK_MAGIC) {
        return Ok(Some(block));
    }
    if block.len() < CACHE_HEADER_LEN {
        return Err(CacheError::CorruptBlock("short header".to_string()));
    }
    let version = block[8];
    if version != CACHE_BLOCK_VERSION {
        return Err(CacheError::CorruptBlock(format!(
            "unsupported version {version}"
        )));
    }
    if block[9] != CACHE_CODEC_RAW {
        return Ok(None);
    }
    let original_len = u64::from_le_bytes(
        block[10..18]
            .try_into()
            .expect("cache block original length slice"),
    ) as usize;
    let payload_len = u64::from_le_bytes(
        block[18..26]
            .try_into()
            .expect("cache block payload length slice"),
    ) as usize;
    if block.len() != CACHE_HEADER_LEN + payload_len {
        return Err(CacheError::CorruptBlock(
            "payload length mismatch".to_string(),
        ));
    }
    let payload = &block[CACHE_HEADER_LEN..];
    if payload.len() != original_len {
        return Err(CacheError::CorruptBlock(
            "original length mismatch".to_string(),
        ));
    }
    Ok(Some(payload))
}

fn decode_cache_block_owned(mut block: Vec<u8>) -> Result<Vec<u8>, CacheError> {
    if let Some(payload) = uncompressed_block_payload(&block)? {
        let len = payload.len();
        let start = block.len() - len;
        if start != 0 {
            block.copy_within(start.., 0);
            block.truncate(len);
        }
        return Ok(block);
    }
    decode_cache_block(&block)
}

fn decode_cache_block(block: &[u8]) -> Result<Vec<u8>, CacheError> {
    if !block.starts_with(CACHE_BLOCK_MAGIC) {
        return Ok(block.to_vec());
    }
    if block.len() < CACHE_HEADER_LEN {
        return Err(CacheError::CorruptBlock("short header".to_string()));
    }
    let version = block[8];
    if version != CACHE_BLOCK_VERSION {
        return Err(CacheError::CorruptBlock(format!(
            "unsupported version {version}"
        )));
    }
    let codec = block[9];
    let original_len = u64::from_le_bytes(
        block[10..18]
            .try_into()
            .expect("cache block original length slice"),
    ) as usize;
    let payload_len = u64::from_le_bytes(
        block[18..26]
            .try_into()
            .expect("cache block payload length slice"),
    ) as usize;
    if block.len() != CACHE_HEADER_LEN + payload_len {
        return Err(CacheError::CorruptBlock(
            "payload length mismatch".to_string(),
        ));
    }
    let payload = &block[CACHE_HEADER_LEN..];
    let decoded = match codec {
        CACHE_CODEC_RAW => payload.to_vec(),
        // The level matters only to the compressor; a codec built for this
        // read decompresses the same whatever it was parameterised with.
        CACHE_CODEC_ZSTD => with_cache_block_codec(CACHE_FALLBACK_COMPRESSION_LEVEL, |codec| {
            codec.decompress(payload, original_len)
        })?,
        other => return Err(CacheError::UnsupportedCodec(other)),
    };
    if decoded.len() != original_len {
        return Err(CacheError::CorruptBlock(
            "original length mismatch".to_string(),
        ));
    }
    Ok(decoded)
}

fn is_encoded_compressed_block(block: &[u8]) -> bool {
    block.starts_with(CACHE_BLOCK_MAGIC)
        && block.len() >= CACHE_HEADER_LEN
        && block[9] == CACHE_CODEC_ZSTD
}
/// Run a write, creating its directory only if that is what was missing.
///
/// `create_dir_all` on a directory that already exists is not free: it stats
/// and then tries to make each component, and the kernel refuses each one.
/// Called before every write, on a path that runs once per entry, that is most
/// of the syscalls the write makes -- 2000 puts issued 9874 failing `mkdir`
/// calls and 9876 `statx` calls, about ten wasted syscalls each.
///
/// The directory almost always exists, so the write is tried first and the
/// directory is created only when it comes back missing. `NotFound` from a
/// create-or-append is the missing directory: the file itself is created by
/// the operation, so it cannot be the thing that was absent.
fn creating_the_directory_if_missing<T>(
    directory: &Path,
    mut write: impl FnMut() -> Result<T, CacheError>,
) -> Result<T, CacheError> {
    match write() {
        Err(CacheError::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(directory)?;
            write()
        }
        outcome => outcome,
    }
}

/// What an eviction reason means to a handler outside the cache.
///
/// The reasons are an implementation detail and change as the policy does;
/// what a handler needs is the one distinction that changes what it should do
/// with the value, which is whether the value is still the entry's contents.
fn removal_cause(reason: EvictionReason) -> CacheRemovalCause {
    match reason {
        EvictionReason::Expired => CacheRemovalCause::Expired,
        EvictionReason::Cold | EvictionReason::LowHit | EvictionReason::Stale => {
            CacheRemovalCause::Evicted
        }
    }
}


impl CacheInner {
    fn disk_path(&self, key: &CacheKey) -> PathBuf {
        self.disk_dir
            .join(format!("shard-{}", key.shard_id))
            .join(&*key.namespace)
            .join(key.disk_name())
    }

    /// The SSD tier's key for an entry: the entry's `v1` manifest line.
    ///
    /// This runs on every invalidation **and on every SSD read**, including the
    /// tier lookup for a key the cache does not hold, so it is built directly
    /// rather than through a record that exists only to be encoded and dropped.
    /// The bytes are unchanged -- see [`manifest_line`] for why that matters
    /// more here than the allocations do.
    fn ssd_store_key(key: &CacheKey) -> String {
        manifest_line_for_key(key, 0)
    }

    fn ssd_block_exists(&self, key: &CacheKey) -> bool {
        self.ssd_store.peek(&Self::ssd_store_key(key))
    }

    fn read_ssd_block(&self, key: &CacheKey) -> Result<Option<Vec<u8>>, CacheError> {
        match self.ssd_store.get_owned(&Self::ssd_store_key(key)) {
            Ok(block) => Ok(Some(block)),
            Err(CacheError::NotFound) => match fs::read(self.disk_path(key)) {
                Ok(block) => Ok(Some(block)),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(err) => Err(CacheError::Io(err)),
            },
            Err(err) => Err(err),
        }
    }

    /// Reads a batch of blocks.
    ///
    /// Borrows the keys rather than taking them by value: the caller has them
    /// already and only their contents are read here, so cloning every one to
    /// build the argument copied three `String`s per candidate for nothing.
    fn read_ssd_blocks(&self, keys: &[&CacheKey]) -> Result<Vec<Option<Vec<u8>>>, CacheError> {
        let store_keys = keys
            .iter()
            .map(|key| Self::ssd_store_key(key))
            .collect::<Vec<_>>();
        let mut blocks = self.ssd_store.get_batch(&store_keys)?;
        for (block, key) in blocks.iter_mut().zip(keys.iter()) {
            if block.is_some() {
                continue;
            }
            match fs::read(self.disk_path(key)) {
                Ok(legacy_block) => *block = Some(legacy_block),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(CacheError::Io(err)),
            }
        }
        Ok(blocks)
    }

    /// Writes one encoded block, taking ownership so the store gets the bytes
    /// rather than a copy of them.
    ///
    /// It used to take a borrow and hand `block.to_vec()` to the store: one
    /// full copy of the payload per SSD write, on a path whose whole job is
    /// moving those bytes. [`Self::write_ssd_blocks`] below already took the
    /// batch by value for exactly this reason, and the order here is now the
    /// order it uses -- the file first, because it borrows, then the store,
    /// which moves.
    ///
    /// That order is the one behavioural change: on a failure the halves land
    /// the other way round from before. Neither order can lose a block the
    /// manifest points at -- that line is appended by the caller only after
    /// this returns -- so what a failure can leave behind is a file nothing
    /// points at, which recovery ignores because recovery reads the manifest.
    fn write_ssd_block(&mut self, key: &CacheKey, block: Vec<u8>) -> Result<(), CacheError> {
        self.note_ssd_probe_mutation(key);
        self.charge_ssd_write(block.len() as u64);
        #[cfg(not(feature = "rocksdb-ssd"))]
        {
            let path = self.disk_path(key);
            let parent = path.parent().unwrap_or(&self.disk_dir).to_path_buf();
            let durable = self.ssd_block_durability;
            creating_the_directory_if_missing(&parent, || {
                write_cache_block_atomic(&path, &block, durable)
            })?;
        }
        self.ssd_store
            .put(&Self::ssd_store_key(key), block)
            .map(|_| ())
    }

    /// Writes a batch of encoded blocks, taking ownership so the store gets the
    /// bytes rather than a copy of them.
    ///
    /// The files are written first because they are the other reader of the
    /// same bytes; the store then takes them by move. Cloning for the store
    /// meant copying every value in the batch a second time.
    fn write_ssd_blocks(&mut self, entries: Vec<(CacheKey, Vec<u8>)>) -> Result<(), CacheError> {
        if entries.is_empty() {
            return Ok(());
        }
        for (key, _) in &entries {
            self.note_ssd_probe_mutation(key);
        }
        let written: u64 = entries.iter().map(|(_, block)| block.len() as u64).sum();
        self.charge_ssd_write(written);
        #[cfg(not(feature = "rocksdb-ssd"))]
        for (key, block) in &entries {
            let path = self.disk_path(key);
            let parent = path.parent().unwrap_or(&self.disk_dir).to_path_buf();
            let durable = self.ssd_block_durability;
            creating_the_directory_if_missing(&parent, || {
                write_cache_block_atomic(&path, block, durable)
            })?;
        }
        self.ssd_store.put_batch(
            entries
                .into_iter()
                .map(|(key, block)| (Self::ssd_store_key(&key), block))
                .collect(),
        )?;
        Ok(())
    }

    /// Record bytes that reached the drive, whoever asked for them.
    ///
    /// Reclaim and recovery wear the flash exactly as an admission does, so a
    /// budget that only counted admissions would aim at the wrong number.
    fn charge_ssd_write(&mut self, bytes: u64) {
        self.stats.ssd_bytes_written = self.stats.ssd_bytes_written.saturating_add(bytes);
        self.ssd_write_budget.record_written(bytes, Instant::now());
        self.refresh_ssd_write_budget_stats();
    }

    /// Copy every published number out of the write budget.
    ///
    /// One function rather than a line at each call site: the budget's share,
    /// measured rate and target all move together, and three sites that each
    /// copy a different subset is how a statistic goes stale in only one of
    /// them.
    fn refresh_ssd_write_budget_stats(&mut self) {
        self.stats.ssd_write_budget_share = self.ssd_write_budget.admitted_share();
        self.stats.ssd_write_budget_observed_bytes_per_sec =
            self.ssd_write_budget.observed_bytes_per_sec();
        self.stats.ssd_write_budget_target_bytes_per_sec =
            self.ssd_write_budget.target_bytes_per_sec();
    }

    /// Whether the write budget will let this key be admitted to the SSD tier.
    ///
    /// Only new admissions are asked. A reclaim or recovery rewrite is work the
    /// cache has already committed to, and refusing it would lose data rather
    /// than save a write.
    fn ssd_write_budget_admits(&mut self, key: &CacheKey) -> bool {
        let admits = self
            .ssd_write_budget
            .admits_with_hash(|| {
                let mut hasher = DefaultHasher::new();
                key.hash(&mut hasher);
                hasher.finish()
            }, Instant::now());
        self.refresh_ssd_write_budget_stats();
        if !admits {
            self.stats.ssd_write_budget_rejections =
                self.stats.ssd_write_budget_rejections.saturating_add(1);
            self.stats.ssd_admission_rejected =
                self.stats.ssd_admission_rejected.saturating_add(1);
        }
        admits
    }

    fn delete_ssd_block(&mut self, key: &CacheKey) -> Result<(), CacheError> {
        self.note_ssd_probe_mutation(key);
        let _ = fs::remove_file(self.disk_path(key));
        match self.ssd_store.delete(&Self::ssd_store_key(key)) {
            Ok(()) | Err(CacheError::NotFound) => Ok(()),
            Err(err) => Err(err),
        }
    }
    fn delete_ssd_blocks<K>(&mut self, keys: &[K]) -> Result<(), CacheError>
    where
        K: std::borrow::Borrow<CacheKey>,
    {
        if keys.is_empty() {
            return Ok(());
        }
        for key in keys {
            self.note_ssd_probe_mutation(key.borrow());
            let _ = fs::remove_file(self.disk_path(key.borrow()));
        }
        // A store with no capacity holds nothing and accepts nothing: its own
        // `delete_batch` returns `Ok(())` without looking at the keys. Encoding
        // a store key per entry to hand it that answer is a manifest line built
        // per key for a tier that is switched off.
        if !self.ssd_store.holds_blocks() {
            return Ok(());
        }
        let store_keys = keys
            .iter()
            .map(|key| Self::ssd_store_key(key.borrow()))
            .collect::<Vec<_>>();
        self.ssd_store.delete_batch(&store_keys).map(|_| ())
    }

    #[cfg(not(feature = "rocksdb-ssd"))]
    fn manifest_path(&self) -> PathBuf {
        self.disk_dir.join(CACHE_MANIFEST_NAME)
    }

    fn rewrite_disk_manifest(&self) -> Result<(), CacheError> {
        #[cfg(feature = "rocksdb-ssd")]
        {
            Ok(())
        }
        #[cfg(not(feature = "rocksdb-ssd"))]
        {
            fs::create_dir_all(&self.disk_dir)?;
            let manifest_path = self.manifest_path();
            let temp_path = self
                .disk_dir
                .join(format!("{CACHE_MANIFEST_NAME}.tmp.{}", std::process::id()));
            {
                let mut file = File::create(&temp_path)?;
                for (key, block_len) in &self.disk_index {
                    writeln!(file, "{}", manifest_line_for_key(key, *block_len))?;
                }
                file.sync_all()?;
            }
            fs::rename(temp_path, manifest_path)?;
            // Whoever called this has just made the file one line per live
            // entry, so the length counter describes it again.
            self.manifest_appends
                .store(self.disk_index.len() as u64, Ordering::Relaxed);
            Ok(())
        }
    }

    #[cfg(not(feature = "rocksdb-ssd"))]
    fn append_disk_manifest_op(&self, op: CacheManifestOp) -> Result<(), CacheError> {
        let mut line = op.encode_line();
        line.push('\n');
        creating_the_directory_if_missing(&self.disk_dir, || {
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.manifest_path())
                .map_err(CacheError::Io)?;
            std::io::Write::write_all(&mut file, line.as_bytes()).map_err(CacheError::Io)
        })?;
        self.compact_disk_manifest_if_long()
    }

    /// Write the manifest whole once it has grown past a multiple of what it
    /// describes.
    ///
    /// The file is append-only, so rewriting the same key appends the same
    /// line again: two thousand writes of one key left two thousand identical
    /// lines, 142,000 bytes, for one live entry -- and recovery reads the
    /// file, so its cost grew with the write history rather than with the live
    /// set. Nothing bounded it: the whole-manifest rewrite existed but only
    /// three bulk invalidations called it.
    ///
    /// Four times the live set, above a floor, so a cache whose disk index is
    /// genuinely large is not rewritten for every write, and a rewrite cannot
    /// repeat until the file has grown fourfold again -- the rewrite leaves
    /// exactly one line per live entry, which is where the counter is reset
    /// to. Three live-set-sized runs of writes between rewrites, so the work
    /// is a third of a line per write amortised.
    #[cfg(not(feature = "rocksdb-ssd"))]
    fn compact_disk_manifest_if_long(&self) -> Result<(), CacheError> {
        const MANIFEST_COMPACT_FLOOR: u64 = 64;
        let appended = self.manifest_appends.fetch_add(1, Ordering::Relaxed) + 1;
        let live = self.disk_index.len() as u64;
        if appended <= MANIFEST_COMPACT_FLOOR.max(live.saturating_mul(4)) {
            return Ok(());
        }
        self.rewrite_disk_manifest()?;
        // What the rewrite just wrote: one line per live entry.
        self.manifest_appends.store(live, Ordering::Relaxed);
        Ok(())
    }

    /// Record a block's arrival in the disk manifest.
    ///
    /// With RocksDB there is no manifest: the store keeps its own index and
    /// nothing replays these lines. The operation is therefore not built either.
    /// It used to be -- constructed, passed in, and dropped unread -- which is a
    /// clone of the key and its record per call.
    #[cfg(feature = "rocksdb-ssd")]
    fn append_disk_manifest_put(&self, _key: &CacheKey, _block_len: u64) -> Result<(), CacheError> {
        Ok(())
    }

    #[cfg(not(feature = "rocksdb-ssd"))]
    fn append_disk_manifest_put(&self, key: &CacheKey, block_len: u64) -> Result<(), CacheError> {
        self.append_disk_manifest_op(CacheManifestOp::Put(CacheManifestRecord::from_entry(
            key, block_len,
        )))
    }

    /// Record a block's removal in the disk manifest. See
    /// [`Self::append_disk_manifest_put`] for why this builds nothing with
    /// RocksDB.
    #[cfg(feature = "rocksdb-ssd")]
    fn append_disk_manifest_delete(&self, _key: &CacheKey) -> Result<(), CacheError> {
        Ok(())
    }

    #[cfg(not(feature = "rocksdb-ssd"))]
    fn append_disk_manifest_delete(&self, key: &CacheKey) -> Result<(), CacheError> {
        self.append_disk_manifest_op(CacheManifestOp::Delete(key.clone()))
    }

    /// Whether this cache has a persistent-memory tier at all.
    ///
    /// Cheap on purpose: it answers without cloning a path, so the persistence
    /// paths below can ask it before they encode anything.
    fn has_pmem_tier(&self) -> bool {
        !self.pmem_paths.is_empty()
    }

    fn pmem_root_dir(&self) -> Option<PathBuf> {
        self.pmem_paths.first().cloned()
    }

    fn pmem_block_dir(&self) -> Option<PathBuf> {
        self.pmem_root_dir().map(|path| path.join("pmem-cache-blocks"))
    }

    fn pmem_manifest_path(&self) -> Option<PathBuf> {
        self.pmem_root_dir()
            .map(|path| path.join("pmem-cache-manifest.log"))
    }

    #[cfg(test)]
    fn pmem_manifest_key(key: &CacheKey, block_len: u64) -> String {
        manifest_line_for_key(key, block_len)
    }

    /// Where a key's block would live in the persistent-memory tier, if there is
    /// one.
    ///
    /// The directory is looked up **first**. Naming the block means encoding the
    /// key as a manifest line and hashing it, and a cache with no persistent
    /// tier used to pay for that on every call before discovering there was
    /// nowhere to put the answer. Same result either way -- `None` when the tier
    /// is absent, the same path when it is there.
    fn pmem_block_path_for_key(&self, key: &CacheKey) -> Option<PathBuf> {
        let dir = self.pmem_block_dir()?;
        Some(dir.join(format!("{:016x}.bin", pmem_key_hash(key))))
    }

    fn encode_pmem_delete_line(key: &CacheKey) -> String {
        manifest_line(
            "pmd1",
            key.shard_id,
            &key.record_key,
            &key.namespace,
            &key.selector,
            None,
        )
    }

    fn decode_pmem_delete_line(line: &str) -> Option<CacheKey> {
        let mut fields = line.split('\t');
        if fields.next()? != "pmd1" {
            return None;
        }
        let shard_id = fields.next()?.parse::<ShardId>().ok()?;
        let record_key = decode_manifest_field(fields.next()?)?;
        let namespace = decode_manifest_field(fields.next()?)?;
        let selector = decode_manifest_field(fields.next()?)?;
        if fields.next().is_some() {
            return None;
        }
        Some(CacheKey {
            shard_id,
            record_key: std::sync::Arc::from(record_key),
            // Decoded from a manifest line, so both of these are owned rather
            // than one of the literals.
            namespace: std::borrow::Cow::Owned(namespace),
            selector: std::borrow::Cow::Owned(selector),
        })
    }

    fn append_pmem_manifest_line(&self, mut line: String) -> Result<(), CacheError> {
        let Some(path) = self.pmem_manifest_path() else {
            return Ok(());
        };
        let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
        line.push('\n');
        creating_the_directory_if_missing(&parent, || {
            #[cfg(unix)]
            {
                let mut writer = self
                    .pmem_manifest_writer
                    .lock()
                    .expect("manifest writer poisoned");
                PmemManifestWriter::append(&mut writer, &path, line.as_bytes())
            }
            #[cfg(not(unix))]
            {
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .map_err(CacheError::Io)?;
                std::io::Write::write_all(&mut file, line.as_bytes()).map_err(CacheError::Io)
            }
        })?;
        if self.pmem_manifest_appends.fetch_add(1, Ordering::Relaxed) + 1
            >= self.pmem_manifest_compact_after.load(Ordering::Relaxed)
        {
            // The append succeeded; maintenance failure must not undo admission.
            let _ = self.compact_pmem_manifest();
            self.pmem_manifest_appends.store(0, Ordering::Relaxed);
        }
        Ok(())
    }

    fn open_matching_pmem_block(path: &Path, value: &[u8]) -> Result<Option<File>, CacheError> {
        let mut file = match File::open(path) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(CacheError::Io(err)),
        };
        if file.metadata()?.len() != value.len() as u64 {
            return Ok(None);
        }
        let mut buffer = [0u8; 8192];
        for chunk in value.chunks(buffer.len()) {
            std::io::Read::read_exact(&mut file, &mut buffer[..chunk.len()])?;
            if &buffer[..chunk.len()] != chunk {
                return Ok(None);
            }
        }
        Ok(Some(file))
    }

    fn persist_pmem_put(&self, key: &CacheKey, value: &[u8]) -> Result<(), CacheError> {
        let Some(path) = self.pmem_block_path_for_key(key) else {
            return Ok(());
        };
        let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let tmp = path.with_extension("tmp");
        let durable = self.pmem_block_durability;
        if self.pmem.get(key).is_some_and(|old| old.as_ref() == value) {
            if let Some(file) = Self::open_matching_pmem_block(&path, value)? {
                // Keep the durability operations and manifest append: the file
                // may have been written under a different policy or recovered.
                if durable {
                    file.sync_all()?;
                    sync_parent_dir(&path)?;
                }
                return self.append_pmem_manifest_line(manifest_line_for_key(key, value.len() as u64));
            }
        }
        creating_the_directory_if_missing(&parent, || {
            if durable {
                let mut file = File::create(&tmp).map_err(CacheError::Io)?;
                // Fully qualified: the  import is gated to one backend
                // and this path is in both.
                std::io::Write::write_all(&mut file, value).map_err(CacheError::Io)?;
                file.sync_all().map_err(CacheError::Io)?;
            } else {
                fs::write(&tmp, value).map_err(CacheError::Io)?;
            }
            fs::rename(&tmp, &path).map_err(CacheError::Io)
        })?;
        if durable {
            sync_parent_dir(&path).map_err(CacheError::Io)?;
        }
        self.append_pmem_manifest_line(manifest_line_for_key(key, value.len() as u64))
    }

    /// Drop a key from the persistent-memory tier's files.
    ///
    /// Returns before encoding anything when there is no such tier. Both halves
    /// below are already no-ops in that case -- there is no block path to remove
    /// and no manifest to append to -- but they were reached through a manifest
    /// line and a delete line built for a tier that does not exist. An
    /// invalidation on a cache with no persistent-memory tier is the common
    /// case, not a corner of it.
    fn persist_pmem_delete(&self, key: &CacheKey) -> Result<(), CacheError> {
        if !self.has_pmem_tier() {
            return Ok(());
        }
        if let Some(path) = self.pmem_block_path_for_key(key) {
            match fs::remove_file(path) {
                Ok(()) => {}
                // Recovery skips missing block files. An absent file needs no
                // additional manifest record, including on repeated deletes.
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(err) => return Err(CacheError::Io(err)),
            }
        }
        self.append_pmem_manifest_line(Self::encode_pmem_delete_line(key))
    }

    fn clear_pmem_persistence(&self) -> Result<(), CacheError> {
        self.reset_pmem_manifest_writer();
        self.reset_pmem_manifest_history();
        let Some(root) = self.pmem_root_dir() else {
            return Ok(());
        };
        match fs::remove_dir_all(&root) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(CacheError::Io(err)),
        }
        fs::create_dir_all(root)?;
        Ok(())
    }

    fn recover_persistent_tiers_locked(&mut self) -> Result<CacheRecoverReport, CacheError> {
        let pmem = self.recover_pmem_index_locked()?;
        let ssd = self.recover_disk_index_locked()?;
        // A restart into less room than the data on the device is an ordinary
        // event: the device shrank, or the configuration did. Recovery indexes
        // what is there, and the bound has to hold once it has -- so the tiers
        // come down to their capacities here rather than at the next write,
        // which on a cache nobody writes to again never comes.
        //
        // Measured before this: 256 incompressible kibibyte blocks written
        // with a mebibyte of room, re-opened with 64 kibibytes, recovered all
        // 256 and held 268,800 bytes -- four times the configured capacity,
        // reported as resident, and nothing on the way to removing it.
        self.evict_pmem_to_capacity();
        self.evict_ssd_to_capacity();
        Ok(CacheRecoverReport {
            scanned_files: pmem.scanned_files.saturating_add(ssd.scanned_files),
            recovered_files: pmem.recovered_files.saturating_add(ssd.recovered_files),
            recovered_bytes: pmem.recovered_bytes.saturating_add(ssd.recovered_bytes),
            skipped_files: pmem.skipped_files.saturating_add(ssd.skipped_files),
        })
    }

    fn recover_pmem_index_locked(&mut self) -> Result<CacheRecoverReport, CacheError> {
        let mut report = CacheRecoverReport::default();
        let Some(manifest_path) = self.pmem_manifest_path() else {
            return Ok(report);
        };
        let file = match File::open(&manifest_path) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(report),
            Err(err) => return Err(CacheError::Io(err)),
        };
        let mut live = HashMap::<CacheKey, u64>::new();
        let reader = std::io::BufReader::new(file);
        for line in std::io::BufRead::lines(reader) {
            let line = line?;
            if let Some(record) = CacheManifestRecord::decode_line(&line) {
                live.insert(record.key(), record.block_len);
            } else if let Some(key) = Self::decode_pmem_delete_line(&line) {
                live.remove(&key);
            } else {
                report.skipped_files = report.skipped_files.saturating_add(1);
            }
        }
        self.pmem.clear();
        self.pmem_order.clear();
        self.pmem_bytes = 0;
        for (key, expected_len) in live {
            report.scanned_files = report.scanned_files.saturating_add(1);
            let Some(path) = self.pmem_block_path_for_key(&key) else {
                report.skipped_files = report.skipped_files.saturating_add(1);
                continue;
            };
            let value = match fs::read(path) {
                Ok(value) => value,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    report.skipped_files = report.skipped_files.saturating_add(1);
                    continue;
                }
                Err(err) => return Err(CacheError::Io(err)),
            };
            if value.len() as u64 != expected_len {
                report.skipped_files = report.skipped_files.saturating_add(1);
                continue;
            }
            if self.put_pmem_with_persistence(&key, value, false) {
                report.recovered_files = report.recovered_files.saturating_add(1);
                report.recovered_bytes = report.recovered_bytes.saturating_add(expected_len);
            } else {
                report.skipped_files = report.skipped_files.saturating_add(1);
            }
        }
        Ok(report)
    }

    fn recover_disk_index_locked(&mut self) -> Result<CacheRecoverReport, CacheError> {
        #[cfg(feature = "rocksdb-ssd")]
        {
            let mut report = CacheRecoverReport::default();
            let mut recovered_index = HashMap::new();
            let mut recovered_order = VecDeque::new();
            let mut recovered_bytes = 0u64;
            let mut recovered = Vec::new();
            self.ssd_store
                .recover_view_data(&mut |store_key: &str, view: StringViewBuffer| {
                    recovered.push((store_key.to_string(), view.size()));
                })?;
            for (store_key, block_len) in recovered {
                report.scanned_files = report.scanned_files.saturating_add(1);
                let Some(record) = CacheManifestRecord::decode_line(&store_key) else {
                    report.skipped_files = report.skipped_files.saturating_add(1);
                    continue;
                };
                let key = record.key();
                let block_len = block_len as u64;
                if recovered_index
                    .insert(self.shared_key(&key), block_len)
                    .is_some()
                {
                    report.skipped_files = report.skipped_files.saturating_add(1);
                    continue;
                }
                recovered_order.push_back(key.clone());
                recovered_bytes = recovered_bytes.saturating_add(block_len);
                report.recovered_files = report.recovered_files.saturating_add(1);
                report.recovered_bytes = report.recovered_bytes.saturating_add(block_len);
                let block_kind = infer_block_kind(&key);
                let routing_slot = extract_routing_slot(&key);
                let shared = self.shared_key(&key);
                self.set_metadata_if_absent(shared, CacheEntryMeta::new(
                    block_kind,
                    routing_slot,
                    0,
                    0,
                    0,
                    CacheAdmissionReason::MemoryOnly,
                ));
            }
            self.disk_index = recovered_index;
            self.disk_order = recovered_order.into_iter().collect();
            self.ssd_bytes = recovered_bytes;
            Ok(report)
        }
        #[cfg(not(feature = "rocksdb-ssd"))]
        {
            let manifest_path = self.manifest_path();
            if !manifest_path.exists() {
                self.disk_index.clear();
                self.ssd_bytes = 0;
                return Ok(CacheRecoverReport::default());
            }

            let file = File::open(manifest_path)?;
            let reader = BufReader::new(file);
            let mut report = CacheRecoverReport::default();
            let mut recovered_index = HashMap::new();
            let mut recovered_order = VecDeque::new();
            let mut recovered_bytes = 0u64;

            for line in reader.lines() {
                let line = line?;
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                report.scanned_files = report.scanned_files.saturating_add(1);
                let Some(op) = CacheManifestOp::decode_line(line) else {
                    report.skipped_files = report.skipped_files.saturating_add(1);
                    continue;
                };
                let record = match op {
                    CacheManifestOp::Put(record) => record,
                    CacheManifestOp::Delete(key) => {
                        recovered_bytes = recovered_bytes
                            .saturating_sub(recovered_index.remove(&key).unwrap_or(0));
                        recovered_order.retain(|candidate| candidate != &key);
                        report.skipped_files = report.skipped_files.saturating_add(1);
                        continue;
                    }
                };
                let key = record.key();
                let path = self.disk_path(&key);
                let Ok(metadata) = path.metadata() else {
                    report.skipped_files = report.skipped_files.saturating_add(1);
                    continue;
                };
                if !metadata.is_file() {
                    report.skipped_files = report.skipped_files.saturating_add(1);
                    continue;
                }
                let block_len = metadata.len().max(record.block_len);
                if recovered_index
                    .insert(self.shared_key(&key), block_len)
                    .is_some()
                {
                    report.skipped_files = report.skipped_files.saturating_add(1);
                    continue;
                }
                recovered_order.push_back(key.clone());
                recovered_bytes = recovered_bytes.saturating_add(block_len);
                report.recovered_files = report.recovered_files.saturating_add(1);
                report.recovered_bytes = report.recovered_bytes.saturating_add(block_len);
                let block_kind = infer_block_kind(&key);
                let routing_slot = extract_routing_slot(&key);
                let shared = self.shared_key(&key);
                self.set_metadata_if_absent(shared, CacheEntryMeta::new(
                    block_kind,
                    routing_slot,
                    0,
                    0,
                    0,
                    CacheAdmissionReason::MemoryOnly,
                ));
            }

            self.disk_index = recovered_index;
            self.disk_order = recovered_order.into_iter().collect();
            self.ssd_bytes = recovered_bytes;
            Ok(report)
        }
    }

    fn default_request(&self, key: &CacheKey, block_bytes: usize) -> CacheAdmissionRequest {
        // A reference is enough: this reads four fields and drops the rest.
        let existing = self.metadata_of(key);
        CacheAdmissionRequest {
            block_kind: existing
                .map(|meta| meta.block_kind)
                .unwrap_or_else(|| infer_block_kind(key)),
            shard_id: key.shard_id,
            routing_slot: existing
                .and_then(|meta| meta.routing_slot())
                .or_else(|| extract_routing_slot(key)),
            block_bytes,
            hotness: existing
                .map(|meta| meta.hotness.load(Ordering::Relaxed))
                .unwrap_or_default(),
            pinned: self.is_pinned(key),
        }
    }

    fn default_insert_request(
        &self,
        key: &CacheKey,
        value_bytes: usize,
        logical_size: usize,
    ) -> CacheAdmissionRequest {
        let block_bytes = if matches!(
            self.tiering_policy.data_placement,
            CacheDataPlacement::Tiered
        ) {
            value_bytes
        } else {
            logical_size
        };
        self.default_request(key, block_bytes)
    }

    /// Drop an entry that has passed its time to live, from every tier.
    ///
    /// Separate from invalidation so the counters can tell the two apart: an
    /// entry that expired was not thrown out to make room, and an operator
    /// looking at a fallen hit rate wants to know which happened.
    fn remove_expired_entry(&mut self, key: &CacheKey) {
        // The handler is told about an expired entry evicted under pressure,
        // because eviction notifies whatever its reason. It was not told about
        // the same entry reclaimed here, so whether a handler heard about an
        // expiry came down to whether the cache happened to be full at the
        // time -- and a handler that releases a resource as an entry leaves
        // would leak every entry that expired quietly.
        //
        // Reported from the highest tier that held it, once, so a handler sees
        // one departure rather than one per copy.
        let notifying = self.eviction_handler_enabled && self.eviction_callback.is_some();
        let mut departing: Option<(CacheTier, Vec<u8>)> = None;
        if let Some(value) = self.take_resident(key) {
            self.memory_bytes = self.memory_bytes.saturating_sub(value.len());
            self.stats.memory_bytes = self.memory_bytes as u64;
            if notifying {
                departing = Some((CacheTier::Memory, value.to_vec()));
            }
        }
        self.memory_order.remove(key);
        if let Some(value) = self.pmem.remove(key) {
            self.pmem_bytes = self.pmem_bytes.saturating_sub(value.len());
            self.stats.pmem_bytes = self.pmem_bytes as u64;
            if notifying && departing.is_none() {
                departing = Some((CacheTier::Pmem, value.to_vec()));
            }
        }
        self.pmem_order.remove(key);
        // The persistent-memory tier is written through to disk, so dropping
        // it from the map above leaves the file behind for recovery to restore
        // — the same return from the dead as the SSD copy below, one tier over.
        // The eviction and invalidation paths both delete it; this one did not.
        if self.persist_pmem_delete(key).is_err() {
            self.stats.expired_delete_failures =
                self.stats.expired_delete_failures.saturating_add(1);
        }
        // The SSD copy too, and this is the one that matters most. Expiry is
        // recorded on the metadata being dropped two lines below, so a copy
        // left on a lower tier has nothing left to say it is too old: the next
        // read falls through to it, serves it, and the entry is back for good
        // because nothing will ever find it expired again.
        //
        // The manifest delete is what stops a restart bringing it back as
        // well: recovery reads the manifest, not the metadata.
        if notifying && departing.is_none() {
            // Reads nothing unless a handler is registered.
            let value = self.read_ssd_value_for_eviction(key);
            if !value.is_empty() {
                departing = Some((CacheTier::Ssd, value));
            }
        }
        if let Some(block_len) = self.disk_index.remove(key) {
            self.ssd_bytes = self.ssd_bytes.saturating_sub(block_len);
            self.stats.disk_bytes = self.ssd_bytes;
        }
        self.disk_order.remove(key);
        if self.delete_ssd_block(key).is_err()
            || self.append_disk_manifest_delete(key).is_err()
        {
            self.stats.expired_delete_failures =
                self.stats.expired_delete_failures.saturating_add(1);
        }
        self.forget_metadata_of(key);
        self.forget_expiry(key);
        self.stats.expired_removals = self.stats.expired_removals.saturating_add(1);
        if let Some((tier, value)) = departing {
            self.record_eviction_with_value(tier, key, || value, CacheRemovalCause::Expired);
        }
    }

    /// Reclaim a few expired entries, without waiting for memory pressure.
    ///
    /// Expiry is otherwise noticed by a read that wanted the entry, or by
    /// eviction preferring an expired victim. Neither happens to an entry
    /// nobody reads in a cache under no pressure, so a workload that writes
    /// with a life and moves on would hold that memory until something else
    /// forced a reclaim.
    ///
    /// Bounded to a handful of the coldest entries per write, which is where
    /// the expired ones collect: this is a fixed cost per write, not a sweep,
    /// and it does not lengthen as the cache grows. A caller that wants the
    /// whole resident set swept can still ask for one.
    fn sweep_some_expired(&mut self) {
        if !self.ttl_in_use {
            return;
        }
        let now_millis = CoarseClock::now_millis();
        // Every tier, not just memory. An entry can be resident only on the
        // persistent or SSD tier -- written straight there, or demoted -- and
        // one of those with a life on it, never read again, would hold its
        // space until something else forced a reclaim. That is the same thing
        // this sweep exists to prevent one tier up.
        //
        // Collected first: the walks borrow the orders that removal edits.
        let mut expired: Vec<CacheKey> = Vec::new();
        for order in [&self.memory_order, &self.pmem_order, &self.disk_order] {
            expired.extend(
                order
                    .iter_access()
                    .take(EXPIRY_SWEEP_PER_WRITE)
                    .filter(|key| !self.is_pinned(key))
                    .filter(|key| self.entry_expired(key, now_millis))
                    .cloned(),
            );
        }
        // A key resident on two tiers is collected once per tier, and removal
        // counts every call, so the duplicates have to go before the count is
        // taken rather than after.
        expired.sort_unstable();
        expired.dedup();
        for key in &expired {
            self.remove_expired_entry(key);
        }
    }

    /// Drops an entry's metadata and any expiry held for it, but only if no
    /// tier still holds the key.
    ///
    /// The metadata map is keyed independently of the tiers and outlives any
    /// one of them, so every removal has to ask all three. An entry that loses
    /// its metadata while a tier can still serve it is not merely cheaper to
    /// keep: [`Self::entry_expired`] answers `false` for an entry with no
    /// expiry recorded, so a stripped entry stops expiring; and
    /// `eviction_score_of` scores a key with no metadata as zero hotness, zero
    /// hits and epoch zero, which is the coldest thing in the cache. Removing
    /// it early therefore makes an entry both immortal against its time to live
    /// and first out under pressure.
    ///
    /// The rule lives here rather than at the four sites that need it because
    /// one of them had two thirds of it.
    fn forget_metadata_unless_resident(&mut self, key: &CacheKey) {
        if !self.memory.contains_key(key)
            && !self.pmem.contains_key(key)
            && !self.disk_index.contains_key(key)
        {
            self.forget_metadata_of(key);
            self.forget_expiry(key);
        }
    }

    /// Whether the entry under `key` has passed its time to live.
    ///
    /// Costs nothing at all when no time to live has ever been set, which is
    /// the default: the flag is checked before the map is touched, so the read
    /// path does not hash a key to learn that nothing expires.
    /// Give back the table space a cache that shrank no longer needs.
    ///
    /// A `HashMap` never shrinks on its own, and a recency order's index is
    /// one. Measured: a cache filled to twenty thousand entries and then
    /// resized down to about a thousand held **3,358 bytes an entry** where a
    /// cache that never grew held 320 -- the tables stay sized for the high
    /// water mark, and the live set does not matter to them.
    ///
    /// Only when the live set is a quarter of the buckets, and only above a
    /// floor, so that a small cache never rehashes and a shrink cannot repeat
    /// without the table having grown fourfold again. Measured on a map held
    /// at a fixed ten thousand entries across thirty thousand writes: the
    /// predicate asks for **no** shrink at all, so a cache sitting at its
    /// capacity pays nothing for this.
    /// Give a pin stripe's table back when the pins it held are gone.
    ///
    /// The pin maps are deliberately not in [`Self::shrink_tables_if_sparse`]:
    /// reaching them means locking **every** stripe, and `all_pins` is not
    /// reentrant, so a shrink from the removal paths would take locks they do
    /// not need. Each stripe shrinks instead on the release that empties it,
    /// under the lock that release already holds.
    ///
    /// Same threshold as the tier maps, and against the bucket count for the
    /// same reason -- see `estimated_buckets`.
    fn shrink_pin_stripe_if_sparse(pins: &mut CachePinState) {
        const SHRINK_FLOOR: usize = 64;
        let buckets = estimated_buckets(pins.entries.capacity());
        if buckets > SHRINK_FLOOR && pins.entries.len().saturating_mul(4) <= buckets {
            pins.entries.shrink_to_fit();
        }
    }

    fn shrink_tables_if_sparse(&mut self) {
        const SHRINK_FLOOR: usize = 64;

        // Against the **bucket count**, not against `capacity()`: see
        // `estimated_buckets`. Measured on a plain `HashMap` taken from
        // 20,000 entries down to 1,000 -- the capacity-based predicate fired
        // once and left the table four times too large, this one halves it
        // four times and lands exactly where a map that never grew would be.
        fn sparse<K, V>(map: &HashMap<K, V>) -> bool {
            let buckets = estimated_buckets(map.capacity());
            buckets > SHRINK_FLOOR && map.len().saturating_mul(4) <= buckets
        }

        if sparse(&self.memory) {
            self.memory.shrink_to_fit();
        }
        if sparse(&self.pmem) {
            self.pmem.shrink_to_fit();
        }
        if sparse(&self.disk_index) {
            self.disk_index.shrink_to_fit();
        }
        if sparse(&self.metadata) {
            self.metadata.shrink_to_fit();
        }
        if sparse(&self.expiries) {
            self.expiries.shrink_to_fit();
        }
        // Bounded by `max_async_writeback_queue` rather than by the entry
        // count, so this one is tens of kilobytes rather than megabytes -- but
        // it is the same table keeping the same high water mark, and a reader
        // of this list would otherwise wonder why it is absent.
        if sparse(&self.async_writeback_positions) {
            self.async_writeback_positions.shrink_to_fit();
        }
        for order in [
            &mut self.memory_order,
            &mut self.pmem_order,
            &mut self.disk_order,
        ] {
            order.shrink_index_if_sparse(SHRINK_FLOOR);
            // The arena is the larger half: a node is 24 bytes and the free
            // list holds a position for every one ever released.
            order.compact_if_sparse(SHRINK_FLOOR);
        }
    }

    fn entry_expired(&self, key: &CacheKey, now_millis: u64) -> bool {
        self.ttl_in_use
            && self
                .expiries
                .get(key)
                .is_some_and(|expires_at| now_millis >= *expires_at)
    }

    /// Stamp an expiry `ttl_millis` from now, or clear it when zero.
    ///
    /// An absent key is what no expiry looks like, so clearing removes rather
    /// than storing a zero.
    fn set_entry_expiry(&mut self, key: &CacheKey, ttl_millis: u64, now_millis: u64) {
        if ttl_millis == 0 {
            self.forget_expiry(key);
            return;
        }
        // Saturating, so a caller asking for a century does not wrap into an
        // entry that is already expired.
        let expires_at = now_millis.saturating_add(ttl_millis).max(1);
        let shared = self.shared_key(key);
        self.expiries.insert(shared, expires_at);
    }

    /// Drop any expiry held for `key`.
    fn forget_expiry(&mut self, key: &CacheKey) {
        if self.ttl_in_use {
            self.expiries.remove(key);
        }
    }

    fn record_metadata(
        &mut self,
        key: &CacheKey,
        block_kind: CacheBlockKind,
        routing_slot: Option<u32>,
        block_bytes: usize,
        requested_hotness: u32,
        admission_reason: CacheAdmissionReason,
    ) {
        let epoch = CoarseClock::now_millis();
        // Re-putting a key already known -- an overwrite, a refill from a lower
        // tier, a write-through -- is the common case, and it needs neither the
        // second lookup nor the key clone. Hotness and hits carry across, which
        // is what rebuilding the entry did; the rest is replaced either way.
        let mut rewriting = false;
        if let Some(meta) = self.metadata_of_mut(key) {
            meta.block_kind = block_kind;
            meta.set_routing_slot(routing_slot);
            meta.last_access_epoch.store(epoch, Ordering::Relaxed);
            meta.admission_reason = admission_reason;
            rewriting = true;
        }
        if rewriting {
            // Rewriting an entry restarts its life, which is what a caller
            // means by putting the key again. Stamped outside the borrow
            // above, because the expiry is no longer a field of the struct
            // that borrow holds.
            let default_ttl_millis = self.default_ttl_millis;
            self.set_entry_expiry(key, default_ttl_millis, epoch);
            return;
        }
        let shared = self.shared_key(key);
        self.set_metadata(
            shared,
            CacheEntryMeta::new(
                block_kind,
                routing_slot,
                initial_hotness(block_kind, block_bytes).max(requested_hotness),
                0,
                epoch,
                admission_reason,
            ),
        );
    }

    /// Returns the epoch this entry was last read at, or `None` if this read is
    /// the first sighting of it.
    fn record_hit_metadata(&mut self, key: &CacheKey, block_bytes: usize) -> Option<u64> {
        let epoch = CoarseClock::now_millis();
        let threshold = self.tiering_policy.memory_hotness_threshold;

        // The hit path runs for every read, so it must not pay for the miss
        // path. Looking the entry up first keeps the key clone, the block-kind
        // inference and the routing-slot extraction on the branch that
        // actually needs them; going through `entry()` did all of that on
        // every hit and then discarded it.
        let mut seen_at = None;
        let crossed_threshold = if let Some(entry) = self.metadata_of(key) {
            let hits_before = entry.record_hits(1);
            if let Some(sketch) = &self.ssd_access_frequency {
                let samples = sampled_hit_count(hits_before, 1);
                if samples != 0 && self.disk_index.contains_key(key) {
                    sketch.record_many(key, samples);
                }
            }
            let before = entry.hotness.fetch_add(1, Ordering::Relaxed);
            // Read, do not swap. This stamp records when the entry was
            // last MOVED in the access order; swapping here would make the
            // refresh window measure the gap between consecutive reads, so an
            // entry read constantly would look permanently fresh and never be
            // promoted again. The shared-lock path above avoids the same trap,
            // and this one has to as well or the tier a read lands on decides
            // whether the entry keeps its place.
            seen_at = Some(entry.last_access_epoch.load(Ordering::Relaxed));
            before < threshold && before.saturating_add(1) >= threshold
        } else {
            let block_kind = infer_block_kind(key);
            let before = initial_hotness(block_kind, block_bytes);
            let hotness = before.saturating_add(1);
            let shared = self.shared_key(key);
            self.set_metadata(
                shared,
                CacheEntryMeta::new(
                    block_kind,
                    extract_routing_slot(key),
                    hotness,
                    1,
                    epoch,
                    CacheAdmissionReason::MemoryOnly,
                ),
            );
            before < threshold && hotness >= threshold
        };

        if crossed_threshold {
            self.read_counters
                .hotness_promotions
                .fetch_add(1, Ordering::Relaxed);
        }
        seen_at
    }

    /// Accounts for a hit without needing the cache exclusively.
    ///
    /// Every counter this touches is atomic, so it runs under a shared borrow.
    /// The two things it cannot do -- move the entry in the tier access
    /// orders, and insert metadata for a key that has none -- are reported
    /// back rather than done, so the caller can escalate for them alone.
    ///
    /// A concurrent pair of hits on the same entry can both observe the
    /// hotness threshold being crossed and count two promotions where the
    /// exclusive path counted one. That is a reporting counter, nothing
    /// branches on it, and the alternative is the lock this exists to avoid.
    fn record_hit_shared(&self, key: &CacheKey) -> HitOutcome {
        let epoch = CoarseClock::now_millis();
        self.reconfigure_refresh_window(epoch);
        self.record_hit_shared_occurrences_at(key, 1, epoch)
    }

    fn record_hit_shared_at(&self, key: &CacheKey, epoch: u64) -> HitOutcome {
        self.record_hit_shared_occurrences_at(key, 1, epoch)
    }

    fn record_hit_shared_occurrences_at(
        &self,
        key: &CacheKey,
        occurrences: usize,
        epoch: u64,
    ) -> HitOutcome {
        let Some(entry) = self.metadata_of(key) else {
            return HitOutcome::NeedsMetadata;
        };
        let threshold = self.tiering_policy.memory_hotness_threshold;
        let occurrences = occurrences.max(1) as u64;
        let hits_before = entry.record_hits(occurrences);
        if let Some(sketch) = &self.ssd_access_frequency {
            let samples = sampled_hit_count(hits_before, occurrences);
            if samples != 0 && self.disk_index.contains_key(key) {
                sketch.record_many(key, samples);
            }
        }
        // A sampled hit signal for the admission sketch, and only when the
        // filter that consumes it is on -- a bool already in this cache line,
        // against ~1.6ns of sketch work nothing would read.
        //
        // Recording every hit measured ~26ns against a ~226ns read; one in
        // sixteen is about 1.6ns. The comparison it feeds is ordinal, so the
        // lost resolution costs nothing: a key hit a thousand times still
        // records far more than one hit twice.
        if self.admission_filter_enabled {
            let sample_hits = sampled_hit_count(hits_before, occurrences);
            self.access_frequency.record_many(key, sample_hits);
        }
        let hotness_delta = occurrences.min(u32::MAX as u64) as u32;
        let before = entry.hotness.fetch_add(hotness_delta, Ordering::Relaxed);
        // Read, do not swap: this stamp records when the entry was last MOVED
        // in the access order, not when it was last read. Swapping here made
        // the window measure the gap between consecutive reads, so an entry
        // read continuously always looked freshly seen and was never promoted
        // -- the exact opposite of what the window is for. The stamp is
        // advanced below, only when the move actually happens.
        let seen_at = entry.last_access_epoch.load(Ordering::Relaxed);
        if before < threshold && before.saturating_add(hotness_delta) >= threshold {
            self.read_counters
                .hotness_promotions
                .fetch_add(1, Ordering::Relaxed);
        }
        // The first read since admission always moves the entry, whatever the
        // refresh window says. This is CacheLib's `|| !isAccessed(node)`, and it
        // exists because of the insertion point: a new entry lands part-way
        // down the order, and whether it ever leaves there cannot be left to a
        // window measured in hundreds of milliseconds when reuse at any real
        // throughput is measured in microseconds. Without this, a genuinely
        // useful entry sits where it was inserted more or less indefinitely.
        //
        // It is also the read most worth acting on: the first one is the
        // strongest evidence an entry is not the one-hit-wonder the insertion
        // point is there to filter out.
        if hits_before == 0 || self.access_order_needs_refresh(Some(seen_at)) {
            entry.last_access_epoch.store(epoch, Ordering::Relaxed);
            self.read_counters
                .access_order_refreshes
                .fetch_add(1, Ordering::Relaxed);
            HitOutcome::NeedsAccessOrderRefresh
        } else {
            HitOutcome::Accounted
        }
    }

    /// Moves the entry to the back of each tier's access order.
    ///
    /// Victim selection reads the front of that order, so without this an
    /// entry written early stays at the front however often it is read.
    fn refresh_access_order(&mut self, key: &CacheKey) {
        self.memory_order.touch_access(key);
        self.pmem_order.touch_access(key);
        self.disk_order.touch_access(key);
    }

    fn record_hit(&mut self, key: &CacheKey, block_bytes: usize) {
        let seen_at = self.record_hit_metadata(key, block_bytes);
        if !self.access_order_needs_refresh(seen_at) {
            return;
        }
        // Advance the stamp with the move it describes. The extra lookup is on
        // the branch that promotes, which the refresh window exists to make
        // rare; the alternative is stamping on every read, which is the bug.
        if let Some(meta) = self.metadata_of(key) {
            meta.last_access_epoch
                .store(CoarseClock::now_millis(), Ordering::Relaxed);
        }
        // Counted here as well as on the shared path. This counter is how much
        // of the read path is known to be escalating, and it was only ever
        // incremented by one of the two paths that promote -- so every
        // promotion from a zero-copy read, a persistent-memory hit or an SSD
        // hit was invisible to it.
        self.read_counters
            .access_order_refreshes
            .fetch_add(1, Ordering::Relaxed);
        self.refresh_access_order(key);
    }

    /// Whether a hit should move the entry in the tier access orders.
    ///
    /// `seen_at` is the epoch the entry was last read at, or `None` if this
    /// read is the first sighting -- which always moves it, since it has no
    /// position yet.
    ///
    /// `access_epoch - seen_at` is how many other accesses happened in
    /// between, and therefore how far from the newest end the entry can have
    /// drifted. Within the refresh distance it is still near the back, so the
    /// move is skipped.
    fn access_order_needs_refresh(&self, seen_at: Option<u64>) -> bool {
        let window = self.lru_refresh_effective_millis.load(Ordering::Relaxed);
        if window == 0 {
            return true;
        }
        match seen_at {
            None => true,
            Some(seen_at) => CoarseClock::now_millis().saturating_sub(seen_at) > window,
        }
    }

    /// Rescales the refresh window to the age of the oldest resident entry.
    ///
    /// This is CacheLib's `reconfigureLocked`, which computes
    /// `min(max(default, oldestElementAge * ratio), cap)` and stores it into a
    /// relaxed atomic. The idea is that the right window is a fraction of how
    /// long an entry survives rather than an absolute duration: a cache whose
    /// entries live ten minutes can skip promotions for a long time and still
    /// order its queue meaningfully, while one whose entries live two seconds
    /// cannot -- there, a long window makes every entry look recently read and
    /// the ordering stops carrying information.
    ///
    /// Runs under the shared lock: reading the oldest entry and storing the
    /// result both need only `&self`. Two readers can recompute at once and one
    /// will overwrite the other, which costs nothing -- they compute almost the
    /// same number from almost the same state.
    fn reconfigure_refresh_window(&self, now: u64) {
        // Disabled is the default, so this is one float compare on the hit path
        // for anyone who has not turned it on.
        if self.lru_refresh_ratio <= 0.0 {
            return;
        }
        if now < self.next_reconfigure_millis.load(Ordering::Relaxed) {
            return;
        }
        self.next_reconfigure_millis.store(
            now.saturating_add(LRU_RECONFIGURE_INTERVAL.as_millis() as u64),
            Ordering::Relaxed,
        );

        // The front of the access order is the least recently used entry --
        // the one eviction would take next, and so the one whose age says how
        // long an entry survives here.
        let oldest_age = self
            .memory_order
            .iter_access()
            .next()
            .and_then(|key| self.metadata_of(key))
            .map(|meta| now.saturating_sub(meta.last_access_epoch.load(Ordering::Relaxed)))
            .unwrap_or(0);

        let scaled = (oldest_age as f64 * self.lru_refresh_ratio) as u64;
        let effective = scaled
            .max(self.lru_refresh_floor_millis)
            .min(LRU_REFRESH_CAP.as_millis() as u64);
        self.lru_refresh_effective_millis
            .store(effective, Ordering::Relaxed);
    }

    /// Size the admission sketch for the entries the cache turned out to
    /// hold.
    ///
    /// The sketch is sized when the filter is switched on, from the memory
    /// capacity divided by what an entry is **expected** to cost -- and that
    /// expectation is a guess whenever the filter is switched on at startup,
    /// which [`MultiLayerCache::set_expected_entry_bytes`] says is the usual
    /// case. Measured: a one-mebibyte cache of four-kibibyte values, told
    /// nothing, sizes the sketch for 16,384 entries and then holds 256. That
    /// is 524,288 bytes of counters for keys that cannot exist, against 8,192
    /// for the keys that can -- and against about 72,000 bytes of every other
    /// piece of bookkeeping those 256 entries carry between them. The sketch
    /// was seven times the rest of the cache.
    ///
    /// So the first time the tier is full -- the first moment the entry count
    /// is a fact rather than a guess -- the sketch is rebuilt for that count.
    /// It corrects in both directions: a cache told to expect large entries
    /// and handed small ones gets a **bigger** sketch, which is the accuracy
    /// the sizing is there for.
    ///
    /// Re-seeded from the hit history the resident entries carry, on the same
    /// sampled scale the sketch records on, because nothing has been evicted
    /// yet at this moment: every key the sketch could know anything about is
    /// still here to be asked. Bounded by
    /// `SKETCH_RESEED_RECORDS_PER_ENTRY`.
    fn size_the_sketch_for_what_is_resident(&mut self) {
        if !self.admission_filter_enabled || self.sketch_sized_from_what_is_resident {
            return;
        }
        // Before the tier is full the entry count is still a guess, and a
        // guess is what this is correcting.
        if self.memory.is_empty() || self.memory_bytes < self.memory_capacity_bytes {
            return;
        }
        self.sketch_sized_from_what_is_resident = true;
        let entries = self.memory.len();
        if FrequencySketch::bytes_for(entries) == self.access_frequency.footprint_bytes() {
            return;
        }
        let sized = FrequencySketch::with_capacity(entries);
        for (key, resident) in &self.memory {
            let hits = resident.meta.as_ref().map_or(0, CacheEntryMeta::hits);
            let records = sampled_hit_count(0, hits).min(SKETCH_RESEED_RECORDS_PER_ENTRY);
            // Every resident key gets at least one, so a key the cache is
            // holding outranks a key nothing has ever seen.
            sized.record_many(key.as_ref(), records.max(1));
        }
        self.access_frequency = sized;
    }

    /// Whether a newcomer is worth what admitting it would cost.
    ///
    /// The replacement policy has already decided which resident entry is
    /// coldest; this asks only whether the candidate has been wanted more often
    /// than that one. Both sides come from the same sketch, so they are on the
    /// same scale and the same decay applies to each.
    ///
    /// Returns true when the filter is off, when there is room, or when there
    /// is no victim to compare against -- an empty cache admits, as it must.
    fn candidate_beats_the_victim(&self, key: &CacheKey, value_len: usize) -> bool {
        if !self.admission_filter_enabled {
            return true;
        }
        // Nothing has to be evicted, so nothing has to be compared.
        if self.memory_bytes + value_len <= self.memory_capacity_bytes {
            return true;
        }
        // The entry weighed against has to be one that could actually be
        // displaced. Eviction skips pinned entries, so a pinned entry is not a
        // trade on offer -- and weighing against one is not a conservative
        // mistake but the worst available one. A pin is a handle held without
        // being read through, so a pinned entry drifts to the least recently
        // accessed end *and* stays cold, which is both the place this was
        // looking and a score everything beats. One of them at the head turned
        // the filter off outright: 32 of 32 first sightings admitted over a
        // resident set read eighty times.
        //
        // Searched over the window eviction uses, with the same fallback to the
        // whole order when the window turns up nothing, so the two cannot
        // disagree about what is on offer.
        let mut on_offer = self
            .memory_order
            .iter_access()
            .take(EVICTION_CANDIDATE_WINDOW)
            .find(|candidate| !self.is_pinned(candidate));
        if on_offer.is_none() && self.memory_order.len() > EVICTION_CANDIDATE_WINDOW {
            on_offer = self
                .memory_order
                .iter_access()
                .find(|candidate| !self.is_pinned(candidate));
        }
        // Nothing here can be displaced at all, so there is no trade to judge.
        let Some(victim) = on_offer else {
            return true;
        };
        // Both sides from the same estimator, so they share a scale and a
        // decay. Ties go to the newcomer, as CacheLib's `newcomerWinsOnTie`
        // defaults to: a cache that will not replace like with like cannot
        // follow a working set that moves.
        self.access_frequency.estimate(key) >= self.access_frequency.estimate(victim)
    }

    /// Take out a tier's copy of `key` because a newer write went elsewhere.
    ///
    /// This is not an eviction and not an invalidation: the entry is not being
    /// discarded to make room, and the key is not going away. It is being
    /// dropped from one tier because a fresher copy of it now lives in another,
    /// and a read that found this one would be served the older value.
    ///
    /// Deliberately leaves the entry's metadata alone. The key is still cached,
    /// just at a different level, and its hotness and hit history are what
    /// routed the write in the first place.
    fn drop_stale_tier_copy(&mut self, key: &CacheKey, tier: CacheTier) -> Result<(), CacheError> {
        match tier {
            CacheTier::Memory => {
                // Removing nothing must not invalidate batch replacement credit.
                if self.resident_value(key).is_some() {
                    if let Some(stale) = self.take_resident(key) {
                        self.memory_bytes = self.memory_bytes.saturating_sub(stale.len());
                        self.stats.memory_bytes = self.memory_bytes as u64;
                        self.stats.stale_tier_copies_dropped =
                            self.stats.stale_tier_copies_dropped.saturating_add(1);
                    }
                }
                self.memory_order.remove(key);
            }
            CacheTier::Pmem => {
                if self.pmem.contains_key(key) {
                    // Recovery must not restore the older copy ahead of SSD.
                    self.persist_pmem_delete(key)?;
                    self.note_ssd_probe_mutation(key);
                }
                if let Some(stale) = self.pmem.remove(key) {
                    self.pmem_bytes = self.pmem_bytes.saturating_sub(stale.len());
                    self.stats.pmem_bytes = self.pmem_bytes as u64;
                    self.stats.stale_tier_copies_dropped =
                        self.stats.stale_tier_copies_dropped.saturating_add(1);
                }
                self.pmem_order.remove(key);
            }
            CacheTier::Ssd | CacheTier::Reject => {}
        }
        Ok(())
    }

    fn put_memory(&mut self, key: &CacheKey, value: Vec<u8>) -> bool {
        self.put_memory_shared(key, Arc::<[u8]>::from(value))
    }

    fn put_memory_shared(&mut self, key: &CacheKey, value: Arc<[u8]>) -> bool {
        let eviction_started = Instant::now();
        let value_len = value.len();
        if self.memory_capacity_bytes == 0 || value_len > self.memory_capacity_bytes {
            self.stats.memory_admission_rejected += 1;
            self.stats.eviction_oversize += 1;
            return false;
        }
        // The sketch was sized from a guess about what an entry costs. This is
        // the path that reads it, so this is where the guess gets corrected.
        self.size_the_sketch_for_what_is_resident();
        // Recorded whether or not it is admitted: a key rejected once has to be
        // able to earn its way in, or the filter is a permanent ban rather than
        // a comparison.
        self.access_frequency.record(key);
        if !self.candidate_beats_the_victim(key, value_len) {
            self.stats.memory_admission_rejected += 1;
            return false;
        }
        self.stats.memory_admission_accepted += 1;
        self.stats.memory_fills += 1;
        // One handle for the entry map and the recency order both, taken from
        // whatever already holds this key -- the metadata map, if the write
        // path reached it first.
        let shared = self.shared_key(key);
        if let Some(old) = self.make_resident(Arc::clone(&shared), Arc::clone(&value)) {
            self.memory_bytes = self.memory_bytes.saturating_sub(old.len());
            // An overwrite is an access. The access order alone does not settle
            // it -- the eviction score weighs hotness before recency, so a
            // rewritten key with the lowest hotness is given up whatever
            // position it holds -- so raise both.
            self.memory_order.touch_access(key);
            if let Some(meta) = self.metadata_of(key) {
                let threshold = self.tiering_policy.memory_hotness_threshold;
                let before = meta.hotness.fetch_add(1, Ordering::Relaxed);
                // Counted the same way a read counts it. Raising hotness
                // without this would let an entry cross the threshold by being
                // written and never appear in `hotness_promotions`, which is a
                // counter quietly ceasing to mean what it says.
                if before < threshold && before.saturating_add(1) >= threshold {
                    self.read_counters
                        .hotness_promotions
                        .fetch_add(1, Ordering::Relaxed);
                }
            }
        } else {
            self.memory_order.push_back_if_absent_shared(shared);
        }
        self.memory_bytes += value_len;
        self.evict_memory_to_capacity_since(eviction_started);
        true
    }

    fn put_pmem(&mut self, key: &CacheKey, value: Vec<u8>) -> bool {
        self.put_pmem_shared_with_persistence(key, Arc::<[u8]>::from(value), true)
    }

    fn put_pmem_with_persistence(
        &mut self,
        key: &CacheKey,
        value: Vec<u8>,
        persist: bool,
    ) -> bool {
        self.put_pmem_shared_with_persistence(key, Arc::<[u8]>::from(value), persist)
    }

    fn put_pmem_shared(&mut self, key: &CacheKey, value: Arc<[u8]>) -> bool {
        self.put_pmem_shared_with_persistence(key, value, true)
    }

    fn put_pmem_shared_with_persistence(
        &mut self,
        key: &CacheKey,
        value: Arc<[u8]>,
        persist: bool,
    ) -> bool {
        self.note_ssd_probe_mutation(key);
        let eviction_started = Instant::now();
        if self.pmem_capacity_bytes == 0 || value.len() > self.pmem_capacity_bytes {
            self.stats.pmem_admission_rejected =
                self.stats.pmem_admission_rejected.saturating_add(1);
            return false;
        }
        if persist && self.persist_pmem_put(key, &value).is_err() {
            self.stats.pmem_admission_rejected =
                self.stats.pmem_admission_rejected.saturating_add(1);
            return false;
        }
        self.stats.pmem_admission_accepted = self.stats.pmem_admission_accepted.saturating_add(1);
        self.stats.pmem_fills = self.stats.pmem_fills.saturating_add(1);
        let shared = self.shared_key(key);
        if let Some(old) = self.pmem.insert(Arc::clone(&shared), Arc::clone(&value)) {
            self.pmem_bytes = self.pmem_bytes.saturating_sub(old.len());
        } else {
            self.pmem_order.push_back_if_absent_shared(shared);
        }
        self.pmem_bytes = self.pmem_bytes.saturating_add(value.len());
        self.evict_pmem_to_capacity_since(eviction_started);
        true
    }

    fn put_ssd_bypass_if_absent(&mut self, key: &CacheKey, value: &[u8]) -> bool {
        if self.disk_index.contains_key(key) || self.ssd_capacity_bytes == 0 {
            return false;
        }
        if !self.ssd_write_budget_admits(key) {
            return false;
        }
        if value.len() > self.tiering_policy.max_ssd_block_bytes
            || value.len() > self.ssd_capacity_bytes
        {
            self.stats.ssd_admission_rejected = self.stats.ssd_admission_rejected.saturating_add(1);
            self.stats.ssd_oversize_rejections =
                self.stats.ssd_oversize_rejections.saturating_add(1);
            return false;
        }
        let Ok(block) = encode_cache_block(value, self.block_options) else {
            self.stats.ssd_admission_rejected = self.stats.ssd_admission_rejected.saturating_add(1);
            return false;
        };
        let block_len = block.len();
        if block_len > self.tiering_policy.max_ssd_block_bytes
            || block_len > self.ssd_capacity_bytes
        {
            self.stats.ssd_admission_rejected = self.stats.ssd_admission_rejected.saturating_add(1);
            self.stats.ssd_oversize_rejections =
                self.stats.ssd_oversize_rejections.saturating_add(1);
            return false;
        }
        self.evict_ssd_for(block_len as u64);
        if self.ssd_bytes.saturating_add(block_len as u64) > self.ssd_capacity_bytes as u64 {
            self.stats.ssd_admission_rejected = self.stats.ssd_admission_rejected.saturating_add(1);
            return false;
        }
        if self.write_ssd_block(key, block).is_err() {
            self.stats.ssd_admission_rejected = self.stats.ssd_admission_rejected.saturating_add(1);
            return false;
        }
        // Record the entry before claiming it. Recovery reads only the manifest,
        // so a block it never records is unreachable after a restart: never
        // served, never counted, and never reclaimed by any eviction path. Undo
        // the write and reject, exactly as a failed block write is rejected.
        if self.append_disk_manifest_put(key, block_len as u64).is_err() {
            let _ = self.delete_ssd_blocks(std::slice::from_ref(&key));
            self.stats.ssd_admission_rejected = self.stats.ssd_admission_rejected.saturating_add(1);
            return false;
        }
        let shared = self.shared_key(key);
        self.disk_index
            .insert(Arc::clone(&shared), block_len as u64);
        self.disk_order.push_back_if_absent_shared(shared);
        self.ssd_bytes = self.ssd_bytes.saturating_add(block_len as u64);
        self.stats.disk_fills = self.stats.disk_fills.saturating_add(1);
        self.stats.ssd_admission_accepted = self.stats.ssd_admission_accepted.saturating_add(1);
        true
    }

    /// Whether an entry leaving a tier is worth writing to the one below.
    ///
    /// An expired entry is not. Its bytes can never be served -- the metadata
    /// that says it is too old outlives the demotion -- so writing it down
    /// spends a flash write, a slice of the SSD write budget and a slot in the
    /// lower tier on a value no read will ever be given.
    ///
    /// Checked here rather than at the eviction reason, because an entry can
    /// be chosen for being cold and be expired as well; the reason says why it
    /// was picked, not what it is worth keeping.
    fn demotion_is_worthwhile(&mut self, key: &CacheKey) -> bool {
        if !self.entry_expired(key, CoarseClock::now_millis()) {
            return true;
        }
        self.stats.expired_demotions_skipped =
            self.stats.expired_demotions_skipped.saturating_add(1);
        false
    }

    fn demote_memory_victim(&mut self, key: &CacheKey, value: &Arc<[u8]>) -> bool {
        if !self.eviction_handler_enabled || !self.demotion_is_worthwhile(key) {
            return false;
        }
        if matches!(
            self.tiering_policy.data_placement,
            CacheDataPlacement::Tiered
        ) && self.pmem_capacity_bytes > 0
            && !self.pmem_paths.is_empty()
        {
            if self.pmem.contains_key(key) {
                // Already down there, so the demotion is done and there is
                // nothing to write. Counted because the alternative is a
                // `pmem_fills` that silently trails `memory_evictions`.
                self.stats.demotions_already_resident =
                    self.stats.demotions_already_resident.saturating_add(1);
                return true;
            }
            // The same allocation the memory tier was holding, handed down
            // rather than copied: it is leaving one map for another, and both
            // of them hold the bytes behind a handle.
            return self.put_pmem_shared(key, Arc::clone(value));
        }
        if self.ssd_capacity_bytes > 0 {
            return self.put_ssd_bypass_if_absent(key, value);
        }
        false
    }

    fn demote_pmem_victim(&mut self, key: &CacheKey, value: &Arc<[u8]>) -> bool {
        if !self.eviction_handler_enabled
            || self.ssd_capacity_bytes == 0
            || !self.demotion_is_worthwhile(key)
        {
            return false;
        }
        self.put_ssd_bypass_if_absent(key, value)
    }

    fn promote_pmem_probe_if_current(&mut self, key: &CacheKey, value: &Arc<[u8]>) {
        // A read can return its captured value, but must not restore it after
        // another operation replaced, invalidated, or expired the entry.
        if !self.started
            || self.ssd_instance_only
            || self.memory.contains_key(key)
            || self.entry_expired(key, CoarseClock::now_millis())
            || !self.pmem.get(key).is_some_and(|current| Arc::ptr_eq(current, value))
        {
            return;
        }
        self.record_hit(key, value.len());
        if !self.put_memory_shared(key, Arc::clone(value)) {
            self.stats.refill_failures = self.stats.refill_failures.saturating_add(1);
        }
    }

    fn bump_ssd_probe_epoch(&mut self) {
        self.ssd_probe_epoch = self.ssd_probe_epoch.saturating_add(1);
        self.ssd_probe_global_epoch = self.ssd_probe_epoch;
    }

    fn note_ssd_probe_mutation(&mut self, key: &CacheKey) {
        if self.ssd_capacity_bytes == 0 {
            // No SSD admission needs key history. A global fence also rejects
            // any read still pending from before the tier was disabled.
            self.bump_ssd_probe_epoch();
            return;
        }
        self.ssd_probe_epoch = self.ssd_probe_epoch.saturating_add(1);
        let mut hash = DefaultHasher::new();
        key.hash(&mut hash);
        let index = self.ssd_probe_epoch as usize % self.ssd_probe_mutations.len();
        self.ssd_probe_mutations[index] = hash.finish();
    }

    fn ssd_probe_is_current_for(&self, key: &CacheKey, epoch: u64) -> bool {
        if epoch == u64::MAX || self.ssd_probe_epoch == u64::MAX
            || epoch < self.ssd_probe_global_epoch || epoch > self.ssd_probe_epoch
        {
            return false;
        }
        let changed = self.ssd_probe_epoch - epoch;
        if changed == 0 {
            return true;
        }
        if changed > self.ssd_probe_mutations.len() as u64 {
            return false;
        }
        let mut hash = DefaultHasher::new();
        key.hash(&mut hash);
        let hash = hash.finish();
        // A hash collision can only skip promotion, never admit stale bytes.
        (epoch + 1..=self.ssd_probe_epoch).all(|sequence| {
            self.ssd_probe_mutations[sequence as usize % self.ssd_probe_mutations.len()] != hash
        })
    }

    #[cfg(test)]
    fn ssd_probe_is_current(&self, epoch: u64) -> bool {
        epoch != u64::MAX && epoch == self.ssd_probe_epoch
    }

    fn ssd_batch_probe_epoch(&self, keys: &[&CacheKey]) -> u64 {
        // A resident may have arrived since the initial miss scan. Its bytes
        // can be newer than SSD even if it is evicted before this batch refills.
        if keys.iter().any(|key| self.memory.contains_key(*key) || self.pmem.contains_key(*key)) {
            u64::MAX
        } else {
            self.ssd_probe_epoch
        }
    }

    // Skipping a raced promotion is not a refill failure. The caller may still
    // return the value observed by its read, but must not install it again.
    fn refill_ssd_probe(&mut self, key: &CacheKey, value: Arc<[u8]>, current: bool) -> bool {
        if !current
            || !self.started
            || self.ssd_instance_only
            || self.memory.contains_key(key)
            || self.pmem.contains_key(key)
            || self.entry_expired(key, CoarseClock::now_millis())
        {
            return true;
        }
        // Installing the bytes just read into unused memory does not change
        // logical data. Do not cancel other pending reads for that placement.
        // Retain invalidation when placement can evict or touch another tier.
        let memory_target = !matches!(
            self.tiering_policy.data_placement,
            CacheDataPlacement::SideBySide
        ) || self.pmem_capacity_bytes == 0
            || value.len() <= self.tiering_policy.data_placement_threshold_bytes;
        let spare_memory = self.memory_bytes.checked_add(value.len())
            .is_some_and(|bytes| bytes <= self.memory_capacity_bytes);
        let epoch = self.ssd_probe_epoch;
        let journal_index = epoch.saturating_add(1) as usize % self.ssd_probe_mutations.len();
        let previous_mutation = self.ssd_probe_mutations[journal_index];
        let refilled = self.refill_from_ssd_shared(key, value);
        // The cache lock stays exclusive here. Suppress only the single
        // resident insertion increment; any extra mutation keeps the guard.
        if refilled && memory_target && spare_memory
            && epoch.checked_add(1) == Some(self.ssd_probe_epoch)
            && self.ssd_probe_global_epoch <= epoch
        {
            self.ssd_probe_epoch = epoch;
            self.ssd_probe_mutations[journal_index] = previous_mutation;
        }
        refilled
    }

    fn refill_from_ssd_shared(&mut self, key: &CacheKey, value: Arc<[u8]>) -> bool {
        if matches!(
            self.tiering_policy.data_placement,
            CacheDataPlacement::SideBySide
        ) && self.pmem_capacity_bytes > 0
            && value.len() > self.tiering_policy.data_placement_threshold_bytes
        {
            return self.put_pmem_shared(key, value);
        }
        self.put_memory_shared(key, value)
    }

    fn evict_memory_to_capacity(&mut self) {
        self.evict_memory_to_capacity_since(Instant::now());
    }

    fn evict_pmem_to_capacity(&mut self) {
        self.evict_pmem_to_capacity_since(Instant::now());
    }

    fn evict_memory_to_capacity_since(&mut self, eviction_started: Instant) {
        while self.memory_bytes > self.memory_capacity_bytes {
            let before = self.memory_bytes;
            let Some((victim, reason, pinned_skips)) = self.select_memory_eviction_victim() else {
                self.stats.eviction_pinned_skips =
                    self.stats.eviction_pinned_skips.saturating_add(1);
                break;
            };
            self.stats.eviction_pinned_skips = self
                .stats
                .eviction_pinned_skips
                .saturating_add(pinned_skips);
            // Drop the victim from the order as it is taken. Selection reads
            // that order, so leaving it until after the loop lets the next
            // round pick the same key, whose bytes are already gone.
            self.memory_order.remove(&victim);
            let Some(old_value) = self.take_resident(&victim) else {
                // A key the order still listed but the tier no longer holds.
                // It is gone from the order now, so the next round makes
                // progress rather than picking it again.
                continue;
            };
            self.memory_bytes = self.memory_bytes.saturating_sub(old_value.len());
            // Answer not consulted: whatever tier this lands in is one
            // the metadata rule below asks about.
            let _ = self.demote_memory_victim(&victim, &old_value);
            self.record_eviction(
                CacheTier::Memory,
                &victim,
                &old_value,
                removal_cause(reason),
            );
            self.stats.memory_evictions = self.stats.memory_evictions.saturating_add(1);
            self.stats.eviction_capacity = self.stats.eviction_capacity.saturating_add(1);
            self.stats.memory_slot_evictions = self.stats.memory_slot_evictions.saturating_add(1);
            self.record_memory_eviction_reason(reason);
            // `demoted` is not tested any more: a demoted victim is in the
            // PMEM tier, which the helper asks about.
            self.forget_metadata_unless_resident(&victim);
            self.record_eviction_latency(eviction_started);
            if self.memory_bytes == before {
                break;
            }
        }
    }

    fn evict_pmem_to_capacity_since(&mut self, eviction_started: Instant) {
        while self.pmem_bytes > self.pmem_capacity_bytes {
            let before = self.pmem_bytes;
            let Some((victim, reason, pinned_skips)) = self.select_pmem_eviction_victim() else {
                self.stats.pmem_eviction_pinned_skips =
                    self.stats.pmem_eviction_pinned_skips.saturating_add(1);
                break;
            };
            self.stats.pmem_eviction_pinned_skips = self
                .stats
                .pmem_eviction_pinned_skips
                .saturating_add(pinned_skips);
            // Drop the victim from the order as it is taken. Selection reads
            // that order, so leaving it until after the loop lets the next
            // round pick the same key, whose bytes are already gone.
            self.pmem_order.remove(&victim);
            let Some(old_value) = self.pmem.remove(&victim) else {
                // A key the order still listed but the tier no longer holds.
                // It is gone from the order now, so the next round makes
                // progress rather than picking it again.
                continue;
            };
            self.pmem_bytes = self.pmem_bytes.saturating_sub(old_value.len());
            // Answer not consulted: whatever tier this lands in is one
            // the metadata rule below asks about.
            let _ = self.demote_pmem_victim(&victim, &old_value);
            let _ = self.persist_pmem_delete(&victim);
            self.record_eviction(CacheTier::Pmem, &victim, &old_value, removal_cause(reason));
            self.stats.pmem_evictions = self.stats.pmem_evictions.saturating_add(1);
            self.stats.pmem_eviction_capacity = self.stats.pmem_eviction_capacity.saturating_add(1);
            self.forget_metadata_unless_resident(&victim);
            self.record_eviction_latency(eviction_started);
            if self.pmem_bytes == before {
                break;
            }
        }
    }

    fn evict_ssd_for(&mut self, incoming_bytes: u64) {
        self.evict_ssd_for_batch(incoming_bytes);
    }

    fn projected_ssd_install_bytes(
        &self,
        pending: &[StagedSsdBatchWrite],
        pending_bytes: u64,
        key: &CacheKey,
        block_bytes: u64,
        credit: &mut Option<(u64, u64)>,
    ) -> u64 {
        let upper = self.ssd_bytes.saturating_add(pending_bytes).saturating_add(block_bytes);
        if upper <= self.ssd_capacity_bytes as u64 {
            return upper;
        }
        // Batch-local replacement credit is reusable only while storage has not
        // changed. Demotion and eviction invalidate the existing mutation epoch.
        let pending_replaced = match *credit {
            Some((epoch, bytes)) if epoch != u64::MAX && epoch == self.ssd_probe_epoch => bytes,
            _ => {
                let bytes = pending.iter().fold(0u64, |total, entry| {
                    total.saturating_add(self.disk_index.get(&entry.key).copied().unwrap_or_default())
                });
                *credit = Some((self.ssd_probe_epoch, bytes));
                bytes
            }
        };
        let replaced = pending_replaced.saturating_add(
            self.disk_index.get(key).copied().unwrap_or_default());
        self.ssd_bytes.saturating_sub(replaced)
            .saturating_add(pending_bytes).saturating_add(block_bytes)
    }

    fn make_room_for_ssd_install(
        &mut self,
        pending: &[StagedSsdBatchWrite],
        pending_bytes: u64,
        key: &CacheKey,
        block_bytes: u64,
        credit: &mut Option<(u64, u64)>,
    ) -> bool {
        loop {
            let projected = self.projected_ssd_install_bytes(pending, pending_bytes, key, block_bytes, credit);
            let capacity = self.ssd_capacity_bytes as u64;
            if projected <= capacity {
                return true;
            }
            let before = self.ssd_bytes;
            let incoming = capacity.saturating_sub(before).saturating_add(projected - capacity);
            self.evict_ssd_for(incoming);
            if self.ssd_bytes >= before {
                return false;
            }
            // Eviction may remove an old copy whose replacement credit we used.
        }
    }

    fn evict_ssd_to_capacity(&mut self) {
        self.evict_ssd_for_batch(0);
    }

    fn evict_ssd_for_batch(&mut self, incoming_bytes: u64) {
        let mut victim_keys = Vec::new();
        while self.ssd_bytes.saturating_add(incoming_bytes) > self.ssd_capacity_bytes as u64 {
            let before = self.ssd_bytes;
            let Some((victim, reason, pinned_skips)) = self.select_ssd_eviction_victim() else {
                break;
            };
            self.stats.ssd_eviction_pinned_skips = self
                .stats
                .ssd_eviction_pinned_skips
                .saturating_add(pinned_skips);
            let evicted_value = self.read_ssd_value_for_eviction(&victim);
            let removed_bytes = self.disk_index.remove(&victim).unwrap_or_default();
            // Drop the victim from the order as it is taken. Selection reads
            // that order, so leaving it until after the loop lets the next
            // round pick the same key, whose bytes are already gone.
            self.disk_order.remove(&victim);
            self.record_eviction_with_value(
                CacheTier::Ssd,
                &victim,
                || evicted_value,
                removal_cause(reason),
            );
            self.ssd_bytes = self.ssd_bytes.saturating_sub(removed_bytes);
            self.stats.ssd_evictions = self.stats.ssd_evictions.saturating_add(1);
            self.stats.ssd_eviction_capacity = self.stats.ssd_eviction_capacity.saturating_add(1);
            self.stats.ssd_slot_evictions = self.stats.ssd_slot_evictions.saturating_add(1);
            self.record_ssd_eviction_reason(reason);
            // This asked about the memory tier alone, so evicting a key from
            // SSD threw away the metadata of an entry the PMEM tier was still
            // serving.
            self.forget_metadata_unless_resident(&victim);
            victim_keys.push(victim);
            if self.ssd_bytes == before {
                // Freed nothing this round; the memory and pmem loops bail out
                // here too rather than spin.
                break;
            }
        }
        if victim_keys.is_empty() {
            return;
        }
        let _ = self.delete_ssd_blocks(&victim_keys);
        for key in &victim_keys {
            let _ = self.append_disk_manifest_delete(key);
        }
    }

    fn select_memory_eviction_victim(&mut self) -> Option<(CacheKey, EvictionReason, u64)> {
        match self.memory_replacement_policy {
            CacheReplacementPolicy::Fifo => {
                self.select_fifo_eviction_victim(&self.memory_order)
            }
            CacheReplacementPolicy::Slru => {
                let candidates = self.memory_cold_candidate_count();
                let picked =
                    self.select_windowed_eviction_victim(&self.memory_order, true, candidates, false);
                self.record_sampled_groups(picked.groups_weighed);
                self.apply_cold_tail_promotions(&picked.promote);
                picked.victim
            }
            CacheReplacementPolicy::WeightedHotnessLru => {
                let candidates = self.memory_cold_candidate_count();
                let picked =
                    self.select_windowed_eviction_victim(&self.memory_order, false, candidates, false);
                self.record_sampled_groups(picked.groups_weighed);
                self.apply_cold_tail_promotions(&picked.promote);
                picked.victim
            }
        }
    }

    fn select_pmem_eviction_victim(&mut self) -> Option<(CacheKey, EvictionReason, u64)> {
        match self.pmem_replacement_policy {
            CacheReplacementPolicy::Fifo => {
                self.select_fifo_eviction_victim(&self.pmem_order)
            }
            CacheReplacementPolicy::Slru => {
                let picked = self.select_windowed_eviction_victim(
                    &self.pmem_order,
                    true,
                    EVICTION_CANDIDATE_WINDOW,
                    false,
                );
                self.record_sampled_groups(picked.groups_weighed);
                picked.victim
            }
            CacheReplacementPolicy::WeightedHotnessLru => {
                let picked = self.select_windowed_eviction_victim(
                    &self.pmem_order,
                    false,
                    EVICTION_CANDIDATE_WINDOW,
                    false,
                );
                self.record_sampled_groups(picked.groups_weighed);
                picked.victim
            }
        }
    }

    fn select_ssd_eviction_victim(&mut self) -> Option<(CacheKey, EvictionReason, u64)> {
        match self.ssd_replacement_policy {
            CacheReplacementPolicy::Fifo => {
                self.select_fifo_eviction_victim(&self.disk_order)
            }
            CacheReplacementPolicy::Slru => {
                let picked = self.select_windowed_eviction_victim(
                    &self.disk_order,
                    true,
                    EVICTION_CANDIDATE_WINDOW,
                    self.ssd_access_frequency.is_some(),
                );
                self.record_sampled_groups(picked.groups_weighed);
                picked.victim
            }
            CacheReplacementPolicy::WeightedHotnessLru => {
                let picked = self.select_windowed_eviction_victim(
                    &self.disk_order,
                    false,
                    EVICTION_CANDIDATE_WINDOW,
                    self.ssd_access_frequency.is_some(),
                );
                self.record_sampled_groups(picked.groups_weighed);
                picked.victim
            }
        }
    }

    fn select_fifo_eviction_victim(
        &self,
        keys: &CacheKeyOrder,
    ) -> Option<(CacheKey, EvictionReason, u64)> {
        let mut pinned_skips = 0u64;
        for key in keys.iter() {
            if self.is_pinned(key) {
                pinned_skips = pinned_skips.saturating_add(1);
                continue;
            }
            return Some((key.clone(), EvictionReason::Stale, pinned_skips));
        }
        None
    }

    /// Weigh a bounded window of `order`, least recently accessed first.
    ///
    /// Falls back to the whole tier when the window turns up nothing
    /// evictable, so a run of pinned entries at the front of the order cannot
    /// stall eviction while unpinned entries sit further back. A tier holding
    /// no more than the window weighs everything either way, so its victim is
    /// exactly the one it would have picked before the window existed.
    fn select_windowed_eviction_victim(
        &self,
        order: &CacheKeyOrder,
        segmented: bool,
        candidates: usize,
        frequency: bool,
    ) -> PickedEvictionVictim {
        // An entry past its time to live could not have been served again, so
        // dropping it costs no future hit. Take one the moment the window turns
        // one up, in preference to weighing live entries against each other and
        // throwing out something a caller still wants.
        //
        // Scanned over the same bounded window as the scoring below, so this
        // adds a comparison per candidate rather than a pass over the tier.
        let now_millis = CoarseClock::now_millis();
        for candidate in order.iter_access().take(candidates) {
            if self.is_pinned(candidate) {
                continue;
            }
            if self.entry_expired(candidate, now_millis) {
                return PickedEvictionVictim {
                    victim: Some((candidate.clone(), EvictionReason::Expired, 0)),
                    groups_weighed: 0,
                    promote: Vec::new(),
                };
            }
        }
        // The rule a three-list policy applies at the tail of its cold list: an
        // entry that has been read twice is moved back rather than given up, and
        // only an entry that has not is deleted. The classes here are derived
        // rather than stored -- cold is the byte share this scan already walks,
        // and read-twice is the counter the score already keeps -- so there is no
        // per-class bookkeeping to drift.
        //
        // Bounded by the scan, and gives up the coldest anyway when every
        // candidate has earned a second chance: a full cache must be able to
        // release something, and a rule that can refuse to is a stall.
        if !frequency && self.cold_eviction_share_pct < 100 {
            let mut promote = Vec::new();
            let mut first_unpinned = None;
            for key in order.iter_access().take(candidates) {
                if self.is_pinned(key) {
                    continue;
                }
                if first_unpinned.is_none() {
                    first_unpinned = Some(key.clone());
                }
                let twice_read = self
                    .metadata_of(key)
                    .is_some_and(|meta| meta.hits() >= SLRU_PROTECTED_HITS);
                if twice_read {
                    promote.push(key.clone());
                    continue;
                }
                return PickedEvictionVictim {
                    victim: Some((key.clone(), EvictionReason::Stale, 0)),
                    groups_weighed: 0,
                    promote,
                };
            }
            if let Some(victim) = first_unpinned {
                // Everything in the share had been read twice. Keep the moves --
                // they are still the right answer for the rest -- but take the
                // coldest, which is what the list would have done once its warm
                // share filled.
                promote.retain(|key| key != &victim);
                return PickedEvictionVictim {
                    victim: Some((victim, EvictionReason::Stale, 0)),
                    groups_weighed: 0,
                    promote,
                };
            }
        }
        let windowed =
            self.select_eviction_victim(order.iter_access().take(candidates), segmented, candidates, frequency);
        if windowed.victim.is_some() || order.len() <= candidates {
            return windowed;
        }
        let full = self.select_eviction_victim(order.iter_access(), segmented, order.len(), frequency);
        PickedEvictionVictim {
            victim: full.victim,
            groups_weighed: windowed
                .groups_weighed
                .saturating_add(full.groups_weighed),
            promote: Vec::new(),
        }
    }

    fn record_sampled_groups(&mut self, groups: usize) {
        self.stats.eviction_sampled_groups = self
            .stats
            .eviction_sampled_groups
            .saturating_add(groups as u64);
    }

    /// Weigh `keys` and return the coldest group's coldest member.
    ///
    /// Borrows the keys rather than taking them by value: the caller passes the
    /// tier's own map keys straight in, so a selection no longer starts by
    /// cloning every key in the tier.
    fn select_eviction_victim<'a, I>(
        &self,
        keys: I,
        segmented: bool,
        candidates: usize,
        frequency: bool,
    ) -> PickedEvictionVictim
    where
        I: IntoIterator<Item = &'a CacheKey>,
    {
        let mut pinned_skips = 0u64;
        // Sized for the window up front. The candidate walk is a linked list,
        // so the iterator cannot say how many are coming and the table grew
        // from empty on every eviction: measured at 11.48 allocations and
        // 44,140 bytes for one evicting write, most of it this table doubling
        // and copying itself four or five times on the way up.
        let mut groups: HashMap<EvictionGroupKey<'a>, SlotEvictionGroup<'a>> =
            HashMap::with_capacity(candidates.min(EVICTION_CANDIDATE_WINDOW));
        for key in keys {
            if self.is_pinned(key) {
                pinned_skips = pinned_skips.saturating_add(1);
                continue;
            }
            // One lookup, both answers. Scoring and grouping ask the same
            // map about the same key, and hashing a CacheKey hashes three
            // Strings -- doing it twice per candidate, up to 128 candidates
            // per eviction, is the bulk of choosing a victim.
            let meta = self.metadata_of(key);
            let mut score = eviction_score_of(meta, segmented);
            if frequency {
                let recent = self.ssd_access_frequency.as_ref().expect("SSD frequency mode enabled").estimate(key);
                score.hotness = recent as u32;
                score.hits = 0;
                score.segment = u8::from(segmented && recent as u64 >= SLRU_PROTECTED_HITS);
            }
            let group_key = eviction_group_key_of(meta, key);
            groups
                .entry(group_key)
                .and_modify(|group| group.observe(key, score))
                .or_insert_with(|| SlotEvictionGroup::new(key, score));
        }
        let group_count = groups.len();
        let victim = groups
            .into_values()
            .min_by(|left, right| {
                left.group_score
                    .cmp(&right.group_score)
                    .then_with(|| left.victim_score.cmp(&right.victim_score))
                    .then_with(|| left.victim.cmp(right.victim))
            })
            .map(|group| {
                // The one clone this selection makes.
                (
                    group.victim.clone(),
                    eviction_reason_for(group.victim_score),
                    pinned_skips,
                )
            });
        PickedEvictionVictim {
            victim,
            groups_weighed: group_count,
            promote: Vec::new(),
        }
    }

    fn incoming_ssd_block_is_colder_than_existing_groups(
        &self,
        key: &CacheKey,
        request: &CacheAdmissionRequest,
        block_bytes: usize,
    ) -> bool {
        // FIFO replaces by insertion order, not popularity. The capacity
        // executor still checks pins and refuses an entry when none can leave.
        if self.ssd_replacement_policy == CacheReplacementPolicy::Fifo {
            return false;
        }
        let frequency = self.ssd_access_frequency.as_ref().map(|sketch| {
            sketch.record(key);
            sketch.estimate(key)
        });
        let incoming_score = EvictionScore {
            segment: 0,
            hotness: initial_hotness(request.block_kind, block_bytes).max(request.hotness),
            hits: 0,
            last_access_epoch: CoarseClock::now_millis(),
        };
        let incoming_group = request
            .routing_slot
            .or_else(|| extract_routing_slot(key))
            .map(EvictionGroupKey::Slot)
            .unwrap_or(EvictionGroupKey::Object(key));
        let mut weighed = 0;
        for candidate in self.disk_order.iter() {
            if self.is_pinned(candidate) {
                continue;
            }
            let meta = self.metadata_of(candidate);
            if eviction_group_key_of(meta, candidate) == incoming_group {
                continue;
            }
            // Admission compares popularity, not resident protection segments.
            // One eligible colder entry suffices; no minimum needs materializing.
            let can_replace = match frequency {
                Some(incoming) => self.ssd_access_frequency.as_ref().expect("SSD frequency mode enabled").estimate(candidate) <= incoming,
                None => eviction_score_of(meta, false) <= incoming_score,
            };
            if can_replace {
                return false;
            }
            weighed += 1;
            if weighed == EVICTION_CANDIDATE_WINDOW {
                break;
            }
        }
        // Count eligible entries, so a pinned prefix cannot hide the actual
        // victims. An entirely pinned tier is rejected by the capacity check.
        weighed != 0
    }

    fn record_expired_eviction(&mut self) {
        self.stats.eviction_expired = self.stats.eviction_expired.saturating_add(1);
    }

    fn record_memory_eviction_reason(&mut self, reason: EvictionReason) {
        match reason {
            EvictionReason::Cold => {
                self.stats.eviction_cold = self.stats.eviction_cold.saturating_add(1)
            }
            EvictionReason::LowHit => {
                self.stats.eviction_low_hit = self.stats.eviction_low_hit.saturating_add(1)
            }
            EvictionReason::Stale => {
                self.stats.eviction_stale = self.stats.eviction_stale.saturating_add(1)
            }
            // Counted on its own rather than folded into a coldness reason: an
            // expired entry was not judged cold, it simply could not be served.
            EvictionReason::Expired => self.record_expired_eviction(),
        }
    }

    fn record_ssd_eviction_reason(&mut self, reason: EvictionReason) {
        match reason {
            EvictionReason::Cold => {
                self.stats.ssd_eviction_cold = self.stats.ssd_eviction_cold.saturating_add(1)
            }
            EvictionReason::LowHit => {
                self.stats.ssd_eviction_low_hit = self.stats.ssd_eviction_low_hit.saturating_add(1)
            }
            EvictionReason::Stale => {
                self.stats.ssd_eviction_stale = self.stats.ssd_eviction_stale.saturating_add(1)
            }
            EvictionReason::Expired => self.record_expired_eviction(),
        }
    }

    /// Queues an eviction for whatever is listening, and copies the entry's
    /// bytes **only if something is**.
    ///
    /// Borrowed rather than owned on purpose. A record carries an owned key
    /// and an owned value because a handler may read it long after the entry
    /// is gone, but the callers used to build both of those before getting
    /// here -- so an eviction copied the whole payload and cloned the key
    /// whether or not any handler existed to be given them, and the handler
    /// is off by default. The copies now happen on the one branch that needs
    /// them.
    fn record_eviction(
        &mut self,
        tier: CacheTier,
        key: &CacheKey,
        value: &[u8],
        cause: CacheRemovalCause,
    ) {
        self.record_eviction_with_value(tier, key, || value.to_vec(), cause);
    }

    fn record_eviction_with_value(
        &mut self,
        tier: CacheTier,
        key: &CacheKey,
        value: impl FnOnce() -> Vec<u8>,
        cause: CacheRemovalCause,
    ) {
        // The metric counts evictions and needs no value, so it is recorded
        // even while the eviction handler is disabled.
        if self.eviction_metric_callback.is_some() {
            self.pending_eviction_metric_tiers.push_back(tier);
        }
        if self.eviction_handler_enabled && self.eviction_callback.is_some() {
            self.pending_eviction_records
                .push_back(CacheEvictionRecord {
                    tier,
                    key: key.clone(),
                    value: value(),
                    cause,
                });
        }
    }

    fn read_ssd_value_for_eviction(&self, key: &CacheKey) -> Vec<u8> {
        if !self.eviction_handler_enabled || self.eviction_callback.is_none() {
            return Vec::new();
        }
        self.read_ssd_block(key)
            .ok()
            .flatten()
            .and_then(|block| decode_cache_block_owned(block).ok())
            .unwrap_or_default()
    }

    fn invalidate_key_locked(&mut self, key: &CacheKey, remove_disk: bool) {
        let disk_key = if remove_disk { Some(key.clone()) } else { None };
        self.invalidate_keys_locked(
            std::slice::from_ref(key),
            remove_disk,
            disk_key.as_ref().map(std::slice::from_ref),
        );
    }

    fn invalidate_keys_locked(
        &mut self,
        keys: &[CacheKey],
        remove_disk: bool,
        disk_delete_keys: Option<&[CacheKey]>,
    ) {
        if keys.is_empty() {
            return;
        }
        const SET_MEMBERSHIP_THRESHOLD: usize = 8;
        let key_set = (keys.len() > SET_MEMBERSHIP_THRESHOLD)
            .then(|| keys.iter().collect::<HashSet<_>>());
        let mut disk_keys = Vec::new();
        for key in keys {
            let key_pinned = self.is_pinned(key);
            let mut removed_pinned_bytes = 0usize;
            if let Some(value) = self.take_resident(key) {
                removed_pinned_bytes = removed_pinned_bytes.max(value.len());
                self.memory_bytes = self.memory_bytes.saturating_sub(value.len());
            }
            self.memory_order.remove(key);
            if remove_disk {
                let disk_bytes = self.disk_index.remove(key).unwrap_or_default();
                removed_pinned_bytes = removed_pinned_bytes.max(
                    disk_bytes
                        .min(usize::MAX as u64)
                        .try_into()
                        .unwrap_or(usize::MAX),
                );
                if disk_delete_keys.is_none() {
                    disk_keys.push(key.clone());
                }
                self.ssd_bytes = self.ssd_bytes.saturating_sub(disk_bytes);
                let _ = self.append_disk_manifest_delete(key);
            }
            if let Some(value) = self.pmem.remove(key) {
                removed_pinned_bytes = removed_pinned_bytes.max(value.len());
                self.pmem_bytes = self.pmem_bytes.saturating_sub(value.len());
            }
            if remove_disk {
                let _ = self.persist_pmem_delete(key);
            }
            self.pmem_order.remove(key);
            // Unconditional until now, which was only safe because every live
            // caller passes `remove_disk`. With it false the disk index keeps
            // the key and the entry stays servable.
            self.forget_metadata_unless_resident(key);
            {
                let mut pins = self.pins_for(key);
                if key_pinned && removed_pinned_bytes > 0 {
                    // Still held, so remember what the handle is worth now the
                    // tier no longer has it.
                    self.pinned_removals_seen.store(true, Ordering::Relaxed);
                    pins.entries
                        .entry(self.shared_key(key))
                        .or_default()
                        .removed_bytes = Some(removed_pinned_bytes);
                } else {
                    pins.entries.remove(key);
                    Self::shrink_pin_stripe_if_sparse(&mut pins);
                }
            }
            self.stats.invalidations += 1;
        }
        if remove_disk {
            match key_set.as_ref() {
                Some(key_set) => {
                    self.disk_order
                        .retain(|candidate| !key_set.contains(candidate));
                }
                None => {
                    self.disk_order
                        .retain(|candidate| !keys.contains(candidate));
                }
            }
            let delete_keys = disk_delete_keys.unwrap_or(&disk_keys);
            let _ = self.delete_ssd_blocks(delete_keys);
        }
    }

    fn clear_all_locked(&mut self, reset_stats: bool) -> Result<(), CacheError> {
        self.bump_ssd_probe_epoch();
        self.memory.clear();
        self.pmem.clear();
        self.clear_pmem_persistence()?;
        self.disk_index.clear();
        self.disk_order.clear();
        self.memory_order.clear();
        self.pmem_order.clear();
        for mut pins in self.all_pins() {
            pins.entries.clear();
            if reset_stats {
                // The pin counters live here now. Resetting the statistics has
                // to reach them where they are, or they survive a reset that
                // is documented to clear everything.
                pins.pin_operations = 0;
                pins.unpin_operations = 0;
                pins.zero_copy_handle_hits = 0;
            }
        }
        self.clear_metadata();
        self.expiries.clear();
        self.async_writeback_queue.clear();
        self.async_writeback_positions.clear();
        self.async_writeback_head = 0;
        self.async_writeback_queue_bytes = 0;
        self.pending_eviction_records.clear();
        self.pending_eviction_metric_tiers.clear();
        self.memory_bytes = 0;
        self.pmem_bytes = 0;
        self.ssd_bytes = 0;
        self.ssd_store.reset()?;
        match fs::remove_dir_all(&self.disk_dir) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) if cfg!(feature = "rocksdb-ssd") => {}
            Err(err) => return Err(CacheError::Io(err)),
        }
        fs::create_dir_all(&self.disk_dir)?;
        if reset_stats {
            self.stats = CacheStats::default();
            self.read_counters.reset();
        } else {
            self.stats.invalidations = self.stats.invalidations.saturating_add(1);
            self.refresh_async_writeback_pressure_stats();
        }
        Ok(())
    }

    /// The pin state, locked.
    ///
    /// Every caller reaches it this way, so the lock order is cache-then-pins
    /// by construction: it cannot be reached without a borrow of the cache.
    /// The stripe a key's pin state lives in.
    ///
    /// A key is always in the same stripe, so a count and the sizes beside it
    /// are never split across two locks.
    fn pin_stripe_index(&self, key: &CacheKey) -> usize {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        (hasher.finish() as usize) % self.pins.len()
    }

    fn pins_for(&self, key: &CacheKey) -> std::sync::MutexGuard<'_, CachePinState> {
        self.pins[self.pin_stripe_index(key)]
            .lock()
            .expect("pin lock poisoned")
    }

    /// Every stripe, in a fixed order.
    ///
    /// Whole-cache operations -- clearing, counting, summing -- need all of
    /// them. Always in index order, so two of these can never deadlock against
    /// each other.
    fn all_pins(&self) -> Vec<std::sync::MutexGuard<'_, CachePinState>> {
        self.pins
            .iter()
            .map(|stripe| stripe.lock().expect("pin lock poisoned"))
            .collect()
    }

    fn is_pinned(&self, key: &CacheKey) -> bool {
        self.pins_for(key).entries.contains_key(key)
    }

    fn increment_pin(&self, key: &CacheKey) {
        let bytes = self
            .resident_value(key)
            .map(|value| value.len())
            .or_else(|| self.pmem.get(key).map(|value| value.len()))
            .or_else(|| {
                self.pins_for(key)
                    .entries
                    .get(key)
                    .and_then(|entry| entry.removed_bytes)
            });
        self.increment_pin_with_optional_size(key, bytes);
    }

    fn increment_pin_with_size(&self, key: &CacheKey, bytes: usize) {
        self.increment_pin_with_optional_size(key, Some(bytes));
    }

    fn increment_pin_with_size_counts(&self, counts: &[(&CacheKey, usize, usize)]) {
        if counts.is_empty() {
            return;
        }
        let mut updates = Vec::<(usize, usize)>::with_capacity(counts.len());
        for (index, (key, _, count)) in counts.iter().enumerate() {
            if *count == 0 {
                continue;
            }
            updates.push((self.pin_stripe_index(key), index));
        }

        if updates.is_empty() {
            return;
        }
        updates.sort_unstable_by_key(|(stripe, _)| *stripe);
        let mut cursor = 0;
        while cursor < updates.len() {
            let stripe = updates[cursor].0;
            let mut pins = self.pins[stripe].lock().expect("pin lock poisoned");
            while cursor < updates.len() && updates[cursor].0 == stripe {
                let index = updates[cursor].1;
                let (key, bytes, count) = &counts[index];
                let entry = pins.entries.entry(self.shared_key(key)).or_default();
                entry.handles = entry.handles.saturating_add(*count as u64);
                entry.handle_bytes = entry.handle_bytes.max(*bytes);
                pins.pin_operations = pins.pin_operations.saturating_add(*count as u64);
                cursor += 1;
            }
        }
    }

    fn increment_pin_handle_counts(&self, counts: &[(&CacheKey, usize, usize, usize)]) {
        if counts.is_empty() {
            return;
        }
        let mut updates = Vec::<(usize, usize)>::with_capacity(counts.len());
        for (index, (key, _, handles, _)) in counts.iter().enumerate() {
            if *handles == 0 {
                continue;
            }
            updates.push((self.pin_stripe_index(key), index));
        }

        if updates.is_empty() {
            return;
        }
        updates.sort_unstable_by_key(|(stripe, _)| *stripe);
        let mut cursor = 0;
        while cursor < updates.len() {
            let stripe = updates[cursor].0;
            let mut pins = self.pins[stripe].lock().expect("pin lock poisoned");
            while cursor < updates.len() && updates[cursor].0 == stripe {
                let index = updates[cursor].1;
                let (key, bytes, handles, handle_hits) = &counts[index];
                let entry = pins.entries.entry(self.shared_key(key)).or_default();
                entry.handles = entry.handles.saturating_add(*handles as u64);
                entry.handle_bytes = entry.handle_bytes.max(*bytes);
                pins.pin_operations = pins.pin_operations.saturating_add(*handles as u64);
                pins.zero_copy_handle_hits =
                    pins.zero_copy_handle_hits.saturating_add(*handle_hits as u64);
                cursor += 1;
            }
        }
    }

    /// Pin, and count the handle, under a single acquisition.
    ///
    /// The two used to be separate locks a line apart on the read path. They
    /// describe the same event, so there is no reason to take the lock twice
    /// for it.
    fn increment_pin_for_handle(&self, key: &CacheKey, bytes: usize) {
        let mut pins = self.pins_for(key);
        // One lookup for the whole handle: the count and its size are the same
        // entry, so they cannot be found separately or updated apart.
        let entry = pins.entries.entry(self.shared_key(key)).or_default();
        entry.handles = entry.handles.saturating_add(1);
        entry.handle_bytes = entry.handle_bytes.max(bytes);
        pins.pin_operations = pins.pin_operations.saturating_add(1);
        pins.zero_copy_handle_hits = pins.zero_copy_handle_hits.saturating_add(1);
    }

    /// Takes the cache **shared** and this lock briefly, which is the whole
    /// point: a reader taking a handle no longer serialises against every
    /// other reader.
    fn increment_pin_with_optional_size(&self, key: &CacheKey, bytes: Option<usize>) {
        // One guard for the count and the size together: they describe the
        // same handle and must not be seen apart.
        let mut pins = self.pins_for(key);
        let entry = pins.entries.entry(self.shared_key(key)).or_default();
        entry.handles = entry.handles.saturating_add(1);
        if let Some(bytes) = bytes {
            entry.handle_bytes = entry.handle_bytes.max(bytes);
        }
        pins.pin_operations = pins.pin_operations.saturating_add(1);
    }

    /// Whether a resident tier is holding more than its capacity allows.
    ///
    /// Only a pin can leave a tier here. Every write path evicts to capacity
    /// before it returns, an oversize value is refused rather than admitted,
    /// and eviction **skips** a pinned entry rather than taking it -- so a
    /// tier above its bound is a tier holding something it could not
    /// displace. Shrinking the capacity under a pin is the way to get there:
    /// the eviction that follows takes what it can and leaves the rest.
    ///
    /// Cheap on purpose. This is read on every release, under a lock the
    /// release already holds, and it is three integer comparisons.
    fn over_a_capacity(&self) -> bool {
        self.memory_bytes > self.memory_capacity_bytes
            || self.pmem_bytes > self.pmem_capacity_bytes
            || self.ssd_bytes > self.ssd_capacity_bytes as u64
    }

    fn decrement_pin(&self, key: &CacheKey) {
        self.decrement_pin_count(key, 1);
    }

    fn decrement_pin_count(&self, key: &CacheKey, count: usize) {
        if count == 0 {
            return;
        }
        let mut pins = self.pins_for(key);
        let Some(entry) = pins.entries.get_mut(key) else {
            return;
        };
        let count = count as u64;
        if entry.handles > count {
            entry.handles -= count;
        } else {
            // The last handle takes everything known about the key with it, in
            // one removal.
            pins.entries.remove(key);
            Self::shrink_pin_stripe_if_sparse(&mut pins);
        }
        pins.unpin_operations = pins.unpin_operations.saturating_add(count);
    }

    fn decrement_pin_counts(&self, counts: &[(CacheKey, usize)]) {
        if counts.is_empty() {
            return;
        }
        let mut updates = Vec::<(usize, usize)>::with_capacity(counts.len());
        for (index, (key, count)) in counts.iter().enumerate() {
            if *count == 0 {
                continue;
            }
            updates.push((self.pin_stripe_index(key), index));
        }

        if updates.is_empty() {
            return;
        }
        updates.sort_unstable_by_key(|(stripe, _)| *stripe);
        let mut cursor = 0;
        while cursor < updates.len() {
            let stripe = updates[cursor].0;
            let mut pins = self.pins[stripe].lock().expect("pin lock poisoned");
            while cursor < updates.len() && updates[cursor].0 == stripe {
                let index = updates[cursor].1;
                let (key, count) = &counts[index];
                let Some(entry) = pins.entries.get_mut(key) else {
                    cursor += 1;
                    continue;
                };
                let count = *count as u64;
                if entry.handles > count {
                    entry.handles -= count;
                } else {
                    pins.entries.remove(key);
                    Self::shrink_pin_stripe_if_sparse(&mut pins);
                }
                pins.unpin_operations = pins.unpin_operations.saturating_add(count);
                cursor += 1;
            }
        }
    }

    fn pinned_removed_bytes_total(&self) -> usize {
        // Nothing has ever been removed from a tier while pinned, so every
        // entry's `removed_bytes` is `None` and the sum is zero. Answered
        // without locking a single stripe, which is what `size()` was doing
        // on every call -- and on a sharded cache, once per shard.
        if !self.pinned_removals_seen.load(Ordering::Relaxed) {
            return 0;
        }
        self.all_pins()
            .iter()
            .flat_map(|pins| pins.entries.values().filter_map(|entry| entry.removed_bytes))
            .fold(0usize, usize::saturating_add)
    }


    fn record_get_latency(&self, started: Instant) {
        let micros = elapsed_micros(started);
        self.record_get_latency_micros(micros);
    }

    fn record_get_latency_micros(&self, micros: u64) {
        self.read_counters.get_latency.observe_with_total(micros);
    }

    fn record_get_latency_micros_many(&self, micros: u64, samples: usize) {
        self.read_counters
            .get_latency
            .observe_many_with_total(micros, samples);
    }

    fn record_put_latency(&mut self, started: Instant) {
        let micros = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
        self.stats.put_latency_samples = self.stats.put_latency_samples.saturating_add(1);
        self.stats.put_latency_total_micros =
            self.stats.put_latency_total_micros.saturating_add(micros);
        self.stats.put_latency_max_micros = self.stats.put_latency_max_micros.max(micros);
        let mut ignored_samples = 0;
        observe_latency_bucket(
            micros,
            &mut ignored_samples,
            &mut self.stats.put_latency_le_10us,
            &mut self.stats.put_latency_le_100us,
            &mut self.stats.put_latency_le_1ms,
            &mut self.stats.put_latency_le_10ms,
            &mut self.stats.put_latency_gt_10ms,
        );
    }

    fn record_read_through_latency(&self, started: Instant) {
        let micros = elapsed_micros(started);
        self.record_read_through_latency_micros(micros);
    }

    fn record_read_through_latency_micros(&self, micros: u64) {
        self.read_counters
            .read_through_latency
            .observe_with_total(micros);
    }

    fn record_read_through_latency_micros_many(&self, micros: u64, samples: usize) {
        self.read_counters
            .read_through_latency
            .observe_many_with_total(micros, samples);
    }

    fn record_refill_latency(&self, started: Instant) {
        let micros = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
        self.read_counters.refill_latency.observe_with_total(micros);
    }

    fn record_writeback_latency(&mut self, started: Instant) {
        let micros = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
        self.stats.writeback_latency_total_micros = self
            .stats
            .writeback_latency_total_micros
            .saturating_add(micros);
        self.stats.writeback_latency_max_micros =
            self.stats.writeback_latency_max_micros.max(micros);
        observe_latency_bucket(
            micros,
            &mut self.stats.writeback_latency_samples,
            &mut self.stats.writeback_latency_le_10us,
            &mut self.stats.writeback_latency_le_100us,
            &mut self.stats.writeback_latency_le_1ms,
            &mut self.stats.writeback_latency_le_10ms,
            &mut self.stats.writeback_latency_gt_10ms,
        );
    }

    fn record_eviction_latency(&mut self, started: Instant) {
        let micros = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
        self.stats.eviction_latency_total_micros = self
            .stats
            .eviction_latency_total_micros
            .saturating_add(micros);
        self.stats.eviction_latency_max_micros =
            self.stats.eviction_latency_max_micros.max(micros);
        observe_latency_bucket(
            micros,
            &mut self.stats.eviction_latency_samples,
            &mut self.stats.eviction_latency_le_10us,
            &mut self.stats.eviction_latency_le_100us,
            &mut self.stats.eviction_latency_le_1ms,
            &mut self.stats.eviction_latency_le_10ms,
            &mut self.stats.eviction_latency_gt_10ms,
        );
    }

    fn record_compaction_latency_micros(&mut self, micros: u64) {
        self.stats.compaction_latency_total_micros = self
            .stats
            .compaction_latency_total_micros
            .saturating_add(micros);
        self.stats.compaction_latency_max_micros =
            self.stats.compaction_latency_max_micros.max(micros);
        observe_latency_bucket(
            micros,
            &mut self.stats.compaction_latency_samples,
            &mut self.stats.compaction_latency_le_10us,
            &mut self.stats.compaction_latency_le_100us,
            &mut self.stats.compaction_latency_le_1ms,
            &mut self.stats.compaction_latency_le_10ms,
            &mut self.stats.compaction_latency_gt_10ms,
        );
    }
}

impl CacheInner {
    fn refresh_async_writeback_pressure_stats(&mut self) {
        let depth = self.async_writeback_queue.len() as u64;
        let bytes = self.async_writeback_queue_bytes;
        self.stats.async_writeback_queue_depth = depth;
        self.stats.async_writeback_queue_bytes = bytes;
        self.stats.async_writeback_max_queue_depth =
            self.stats.async_writeback_max_queue_depth.max(depth);
        self.stats.async_writeback_max_queue_bytes =
            self.stats.async_writeback_max_queue_bytes.max(bytes);
    }

}

/// The score an entry is weighed by, from metadata already looked up.
///
/// An entry with no metadata scores as never read, which is what the previous
/// fallback amounted to: it built a whole `CacheEntryMeta`, inferring a block
/// kind and extracting a routing slot, and then read three zero fields off it.
fn eviction_score_of(meta: Option<&CacheEntryMeta>, segmented: bool) -> EvictionScore {
    match meta {
        Some(meta) => {
            let hits = meta.hits();
            EvictionScore {
                segment: u8::from(segmented && hits >= SLRU_PROTECTED_HITS),
                hotness: meta.hotness.load(Ordering::Relaxed),
                hits,
                last_access_epoch: meta.last_access_epoch.load(Ordering::Relaxed),
            }
        }
        None => EvictionScore {
            segment: 0,
            hotness: 0,
            hits: 0,
            last_access_epoch: 0,
        },
    }
}

/// The group an entry is weighed within, from metadata already looked up.
fn eviction_group_key_of<'a>(
    meta: Option<&CacheEntryMeta>,
    key: &'a CacheKey,
) -> EvictionGroupKey<'a> {
    meta.and_then(|meta| meta.routing_slot())
        .or_else(|| extract_routing_slot(key))
        .map(EvictionGroupKey::Slot)
        .unwrap_or(EvictionGroupKey::Object(key))
}

fn observe_latency_bucket(
    micros: u64,
    samples: &mut u64,
    le_10us: &mut u64,
    le_100us: &mut u64,
    le_1ms: &mut u64,
    le_10ms: &mut u64,
    gt_10ms: &mut u64,
) {
    *samples = samples.saturating_add(1);
    if micros <= 10 {
        *le_10us = le_10us.saturating_add(1);
    } else if micros <= 100 {
        *le_100us = le_100us.saturating_add(1);
    } else if micros <= 1_000 {
        *le_1ms = le_1ms.saturating_add(1);
    } else if micros <= 10_000 {
        *le_10ms = le_10ms.saturating_add(1);
    } else {
        *gt_10ms = gt_10ms.saturating_add(1);
    }
}

fn infer_block_kind(key: &CacheKey) -> CacheBlockKind {
    match &*key.namespace {
        "page" => CacheBlockKind::Page,
        "index" => CacheBlockKind::Index,
        "oplog" => CacheBlockKind::Oplog,
        "string" | "hash" | "set" | "feature" => CacheBlockKind::Object,
        _ => CacheBlockKind::Other,
    }
}

/// How many candidates victim selection weighs before it settles.
///
/// Selection used to weigh every resident entry, so a cache sitting at
/// capacity paid a cost proportional to how much it held on every single
/// write. Weighing a bounded window of the oldest-resident entries instead
/// keeps that cost flat as the cache grows. The window is wider than the
/// working set of a small cache, so those keep weighing everything and choose
/// exactly what they chose before.
const EVICTION_CANDIDATE_WINDOW: usize = 128;

/// Share of the access order, coldest first, an eviction may choose from when a
/// caller has not asked for one.
///
/// A hundred is every resident entry, which is what this cache has always done,
/// so the default changes nothing. See
/// [`MultiLayerCache::set_cold_eviction_share_pct`] for what asking for less
/// buys and what it costs.
pub const COLD_EVICTION_SHARE_PCT_DEFAULT: u32 = 100;

/// The share a segmented policy would leave evictable: what is left after a hot
/// and a warm share, `100 - SLRU_DEFAULT_HOT_LRU_PCT - SLRU_DEFAULT_WARM_LRU_PCT`.
/// Written as the remainder so the three cannot drift apart.
pub const COLD_EVICTION_SHARE_PCT_SEGMENTED: u32 =
    100 - SLRU_DEFAULT_HOT_LRU_PCT - SLRU_DEFAULT_WARM_LRU_PCT;

/// How many of the coldest entries a write looks at for expiry.
///
/// Small on purpose. The expired entries collect at the cold end, so a few per
/// write keeps up with any expiry rate a write rate can produce, while the cost
/// stays a fixed handful of comparisons rather than anything that grows with
/// the cache.
const EXPIRY_SWEEP_PER_WRITE: usize = 8;

fn eviction_reason_for(score: EvictionScore) -> EvictionReason {
    if score.hotness == 0 {
        EvictionReason::Cold
    } else if score.hits == 0 {
        EvictionReason::LowHit
    } else {
        EvictionReason::Stale
    }
}

impl CacheInner {
    /// How many of the coldest memory entries make up the evictable share.
    ///
    /// The share is a share of BYTES, which is the unit a budget is kept in and
    /// the unit the policy this follows bounds its lists by. It was a share of
    /// the entry count until a workload with varied sizes was driven through it:
    /// at a share of 40 the count version cost sixteen points of hit rate where
    /// the byte version costs four, because sixty per cent of the entries is not
    /// sixty per cent of the bytes once entries differ in size, so the
    /// reservation was the wrong size and protected the wrong set.
    ///
    /// Prices each candidate from `memory`, which is the authority on what an
    /// entry costs. Nothing is accounted twice and nothing can drift: the
    /// alternative was a byte count per node in the access order, maintained at
    /// five mutation sites, and a drifting one of those is an eviction bound that
    /// is quietly wrong.
    ///
    /// Walks no further than the window it replaces, so a large tier does not
    /// scan more than it used to, and returns at least one candidate because a
    /// cache with a resident entry has to be able to give one up.
    /// Move entries the scan passed over back to the hot end.
    ///
    /// The scan runs while the cache is held shared and cannot move anything, so
    /// it reports what it passed over and this applies it. An entry read twice is
    /// moved rather than given up, which is what a three-list policy does at the
    /// tail of its cold list; moving it is what stops the next eviction weighing
    /// it again and makes the second chance a chance rather than a reprieve of
    /// one scan.
    fn apply_cold_tail_promotions(&mut self, keys: &[CacheKey]) {
        for key in keys {
            self.memory_order.touch_access(key);
        }
    }

    fn memory_cold_candidate_count(&self) -> usize {
        let share = self.cold_eviction_share_pct;
        if share >= 100 {
            return EVICTION_CANDIDATE_WINDOW;
        }
        let budget = self
            .memory_bytes
            .saturating_mul(share as usize)
            .saturating_div(100);
        let mut bytes = 0usize;
        let mut count = 0usize;
        for key in self.memory_order.iter_access() {
            if count >= EVICTION_CANDIDATE_WINDOW {
                break;
            }
            if bytes >= budget && count >= 1 {
                break;
            }
            bytes =
                bytes.saturating_add(self.resident_value(key).map_or(0, |value| value.len()));
            count += 1;
        }
        count.max(1)
    }
}

fn initial_hotness(block_kind: CacheBlockKind, block_bytes: usize) -> u32 {
    match block_kind {
        CacheBlockKind::Page => 2,
        CacheBlockKind::Index => 3,
        CacheBlockKind::Oplog => 1,
        CacheBlockKind::Object if block_bytes <= 4096 => 2,
        CacheBlockKind::Object => 1,
        CacheBlockKind::Other => 0,
    }
}

/// Most records a single resident entry contributes when the sketch is
/// re-sized.
///
/// The re-seed exists to restore each resident entry's rank above a key
/// nothing has seen, which is the comparison admission makes. Restoring the
/// full distribution would mean replaying every entry's sampled history, which
/// on a cache of a million entries is tens of millions of counter updates
/// under the cache lock -- so it is bounded here instead, and the ordering
/// among the hot keys re-establishes itself from the hits that follow.
const SKETCH_RESEED_RECORDS_PER_ENTRY: u64 = 8;

fn sampled_hit_count(hits_before: u64, occurrences: u64) -> u64 {
    if occurrences == 0 {
        return 0;
    }
    let last_hit = hits_before.saturating_add(occurrences.saturating_sub(1));
    let before_window = hits_before.saturating_sub(1);
    last_hit / HIT_SAMPLE_INTERVAL - before_window / HIT_SAMPLE_INTERVAL
}

fn extract_routing_slot(key: &CacheKey) -> Option<u32> {
    let suffix = key.selector.strip_prefix("slot-")?;
    let (slot, _) = suffix.split_once(':')?;
    slot.parse::<u32>().ok()
}

/// Hex-encode a manifest field into a line that is already being built.
///
/// The bytes are exactly what the field encoding has always produced. What
/// changes is where they land: into the caller's line rather than into a
/// `String` of their own that is then copied into it and dropped.
fn push_manifest_field(line: &mut String, value: &str) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in value.as_bytes() {
        line.push(HEX[(byte >> 4) as usize] as char);
        line.push(HEX[(byte & 0x0f) as usize] as char);
    }
}

/// Decimal digits, pushed rather than formatted.
///
/// `write!` would do the same thing, but it reaches for `std::fmt::Write`,
/// and this file's `Write` import is gated to one backend while every manifest
/// line is built under both.
fn push_decimal(line: &mut String, value: u64) {
    // u64::MAX is 20 digits.
    let mut digits = [0u8; 20];
    let mut index = digits.len();
    let mut rest = value;
    loop {
        index -= 1;
        digits[index] = b'0' + (rest % 10) as u8;
        rest /= 10;
        if rest == 0 {
            break;
        }
    }
    for &digit in &digits[index..] {
        line.push(digit as char);
    }
}

/// How many characters `value` occupies in decimal.
fn decimal_width(value: u64) -> usize {
    if value == 0 {
        1
    } else {
        value.ilog10() as usize + 1
    }
}

/// A manifest line, in one allocation.
///
/// Every manifest line in this crate is a tag, a shard id and three hex-encoded
/// key fields, with a block length on the two shapes that carry one. Building
/// one with `format!` over a per-field encoder allocated three field `String`s
/// that were copied into the line and dropped, and then let `format!` size the
/// line by its literal pieces alone -- which under-counts the hex fields by an
/// order of magnitude, so the line was allocated and then grown twice more.
/// Nine allocations for a string this can size up front, measured flat from
/// four record-key characters to a hundred and sixty.
///
/// **The bytes are unchanged.** These lines are RocksDB keys and manifest
/// records already written to disk; a changed byte would not fail a test, it
/// would make an entry that is present unfindable. The capacity below is the
/// length of the result plus one byte, so appending a journal newline does not
/// grow and copy the entire encoded key. Store keys keep the same bytes.
fn manifest_line(
    tag: &str,
    shard_id: ShardId,
    record_key: &str,
    namespace: &str,
    selector: &str,
    block_len: Option<u64>,
) -> String {
    let mut line = String::with_capacity(
        tag.len()
            + 1
            + decimal_width(shard_id)
            + 1
            + record_key.len() * 2
            + 1
            + namespace.len() * 2
            + 1
            + selector.len() * 2
            + block_len.map_or(0, |len| 1 + decimal_width(len))
            + 1,
    );
    line.push_str(tag);
    line.push('\t');
    push_decimal(&mut line, shard_id);
    line.push('\t');
    push_manifest_field(&mut line, record_key);
    line.push('\t');
    push_manifest_field(&mut line, namespace);
    line.push('\t');
    push_manifest_field(&mut line, selector);
    if let Some(len) = block_len {
        line.push('\t');
        push_decimal(&mut line, len);
    }
    line
}

/// Hash the existing manifest-key representation without allocating its text.
fn pmem_key_hash(key: &CacheKey) -> u64 {
    let mut hasher = DefaultHasher::new();
    hasher.write(b"v1\t");
    let mut digits = [0u8; 20];
    let mut start = digits.len();
    let mut shard = key.shard_id;
    loop {
        start -= 1;
        digits[start] = b'0' + (shard % 10) as u8;
        shard /= 10;
        if shard == 0 { break; }
    }
    hasher.write(&digits[start..]);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = [0u8; 128];
    for field in [key.record_key.as_ref(), key.namespace.as_ref(), key.selector.as_ref()] {
        hasher.write(b"\t");
        for chunk in field.as_bytes().chunks(encoded.len() / 2) {
            for (i, byte) in chunk.iter().enumerate() {
                encoded[2 * i] = HEX[(byte >> 4) as usize];
                encoded[2 * i + 1] = HEX[(byte & 15) as usize];
            }
            hasher.write(&encoded[..chunk.len() * 2]);
        }
    }
    hasher.write(b"\t0");
    // Match Hash for str, including its prefix-free terminator. Existing block
    // filenames must remain addressable without a migration.
    hasher.write_u8(0xff);
    hasher.finish()
}

/// A `v1` manifest line built directly from borrowed key fields.
fn manifest_line_for_key(key: &CacheKey, block_len: u64) -> String {
    manifest_line(
        "v1",
        key.shard_id,
        &key.record_key,
        &key.namespace,
        &key.selector,
        Some(block_len),
    )
}

fn decode_manifest_field(value: &str) -> Option<String> {
    if !value.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for chunk in value.as_bytes().as_chunks::<2>().0 {
        let high = decode_hex_nibble(chunk[0])?;
        let low = decode_hex_nibble(chunk[1])?;
        bytes.push((high << 4) | low);
    }
    String::from_utf8(bytes).ok()
}

fn decode_hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[allow(dead_code)]
fn dir_size(path: &Path) -> Result<u64, std::io::Error> {
    if !path.exists() {
        return Ok(0);
    }
    let mut total = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                stack.push(entry.path());
            } else if metadata.is_file() {
                total += metadata.len();
            }
        }
    }
    Ok(total)
}

fn unique_temp_path(kind: &str) -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!(
        "temporalstore-rust-{kind}-{}-{nanos}-{counter}",
        std::process::id()
    ))
}

/// Microseconds since , saturating rather than wrapping.
fn elapsed_micros(started: Instant) -> u64 {
    started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64
}
