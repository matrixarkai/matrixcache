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

fn encode_cache_block(value: &[u8], options: CacheBlockOptions) -> Result<Vec<u8>, CacheError> {
    let (codec, payload) = match options.compression {
        CacheCompression::None if value.len() >= options.min_compress_bytes => {
            (CACHE_CODEC_RAW, value.to_vec())
        }
        CacheCompression::None => (CACHE_CODEC_RAW, value.to_vec()),
        CacheCompression::Zstd { level } if value.len() >= options.min_compress_bytes => {
            let compressed = zstd::stream::encode_all(value, level)?;
            if CACHE_HEADER_LEN + compressed.len() < value.len() {
                (CACHE_CODEC_ZSTD, compressed)
            } else {
                (CACHE_CODEC_RAW, value.to_vec())
            }
        }
        CacheCompression::Zstd { .. } => (CACHE_CODEC_RAW, value.to_vec()),
    };
    let mut block = Vec::with_capacity(CACHE_HEADER_LEN + payload.len());
    block.extend_from_slice(CACHE_BLOCK_MAGIC);
    block.push(CACHE_BLOCK_VERSION);
    block.push(codec);
    block.extend_from_slice(&(value.len() as u64).to_le_bytes());
    block.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    block.extend_from_slice(&payload);
    Ok(block)
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
        CACHE_CODEC_ZSTD => zstd::stream::decode_all(payload)?,
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
            .join(&key.namespace)
            .join(key.disk_name())
    }

    fn ssd_store_key(key: &CacheKey) -> String {
        CacheManifestRecord::from_entry(key, 0).encode_line()
    }

    fn ssd_block_exists(&self, key: &CacheKey) -> bool {
        self.ssd_store.peek(&Self::ssd_store_key(key))
    }

    fn read_ssd_block(&self, key: &CacheKey) -> Result<Option<Vec<u8>>, CacheError> {
        match self.ssd_store.get(&Self::ssd_store_key(key)) {
            Ok(buffer) => Ok(Some(buffer.to_vec())),
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

    fn write_ssd_block(&mut self, key: &CacheKey, block: &[u8]) -> Result<(), CacheError> {
        self.charge_ssd_write(block.len() as u64);
        self.ssd_store
            .put(&Self::ssd_store_key(key), block.to_vec())
            .map(|_| ())?;
        #[cfg(feature = "rocksdb-ssd")]
        {
            Ok(())
        }
        #[cfg(not(feature = "rocksdb-ssd"))]
        {
            let path = self.disk_path(key);
            let parent = path.parent().unwrap_or(&self.disk_dir).to_path_buf();
            let durable = self.ssd_block_durability;
            creating_the_directory_if_missing(&parent, || {
                write_cache_block_atomic(&path, block, durable)
            })
        }
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
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let admits = self
            .ssd_write_budget
            .admits(hasher.finish(), Instant::now());
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
        let _ = fs::remove_file(self.disk_path(key));
        match self.ssd_store.delete(&Self::ssd_store_key(key)) {
            Ok(()) | Err(CacheError::NotFound) => Ok(()),
            Err(err) => Err(err),
        }
    }
    fn delete_ssd_blocks(&mut self, keys: &[CacheKey]) -> Result<(), CacheError> {
        if keys.is_empty() {
            return Ok(());
        }
        for key in keys {
            let _ = fs::remove_file(self.disk_path(key));
        }
        let store_keys = keys.iter().map(Self::ssd_store_key).collect::<Vec<_>>();
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
                    let record = CacheManifestRecord::from_entry(key, *block_len);
                    writeln!(file, "{}", record.encode_line())?;
                }
                file.sync_all()?;
            }
            fs::rename(temp_path, manifest_path)?;
            Ok(())
        }
    }

    fn append_disk_manifest_op(&self, op: CacheManifestOp) -> Result<(), CacheError> {
        #[cfg(feature = "rocksdb-ssd")]
        {
            let _ = op;
            Ok(())
        }
        #[cfg(not(feature = "rocksdb-ssd"))]
        {
            let line = op.encode_line();
            creating_the_directory_if_missing(&self.disk_dir, || {
                let mut file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(self.manifest_path())
                    .map_err(CacheError::Io)?;
                writeln!(file, "{line}").map_err(CacheError::Io)
            })
        }
    }

    fn append_disk_manifest_put(&self, key: &CacheKey, block_len: u64) -> Result<(), CacheError> {
        self.append_disk_manifest_op(CacheManifestOp::Put(CacheManifestRecord::from_entry(
            key, block_len,
        )))
    }

    fn append_disk_manifest_delete(&self, key: &CacheKey) -> Result<(), CacheError> {
        self.append_disk_manifest_op(CacheManifestOp::Delete(key.clone()))
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

    fn pmem_manifest_key(key: &CacheKey, block_len: u64) -> String {
        CacheManifestRecord::from_entry(key, block_len).encode_line()
    }

    fn pmem_block_path_for_key(&self, key: &CacheKey) -> Option<PathBuf> {
        let mut hasher = DefaultHasher::new();
        Self::pmem_manifest_key(key, 0).hash(&mut hasher);
        self.pmem_block_dir()
            .map(|dir| dir.join(format!("{:016x}.bin", hasher.finish())))
    }

    fn encode_pmem_delete_line(key: &CacheKey) -> String {
        format!(
            "pmd1	{}	{}	{}	{}",
            key.shard_id,
            encode_manifest_field(&key.record_key),
            encode_manifest_field(&key.namespace),
            encode_manifest_field(&key.selector)
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
            record_key,
            namespace,
            selector,
        })
    }

    fn append_pmem_manifest_line(&self, line: String) -> Result<(), CacheError> {
        let Some(path) = self.pmem_manifest_path() else {
            return Ok(());
        };
        let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
        creating_the_directory_if_missing(&parent, || {
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(CacheError::Io)?;
            use std::io::Write as _;
            writeln!(file, "{line}").map_err(CacheError::Io)
        })
    }

    fn persist_pmem_put(&self, key: &CacheKey, value: &[u8]) -> Result<(), CacheError> {
        let Some(path) = self.pmem_block_path_for_key(key) else {
            return Ok(());
        };
        let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let tmp = path.with_extension("tmp");
        let durable = self.pmem_block_durability;
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
        self.append_pmem_manifest_line(CacheManifestRecord::from_entry(key, value.len() as u64).encode_line())
    }

    fn persist_pmem_delete(&self, key: &CacheKey) -> Result<(), CacheError> {
        if let Some(path) = self.pmem_block_path_for_key(key) {
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(CacheError::Io(err)),
            }
        }
        self.append_pmem_manifest_line(Self::encode_pmem_delete_line(key))
    }

    fn clear_pmem_persistence(&self) -> Result<(), CacheError> {
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
            if self.put_pmem_with_persistence(key, value, false) {
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
                if recovered_index.insert(key.clone(), block_len).is_some() {
                    report.skipped_files = report.skipped_files.saturating_add(1);
                    continue;
                }
                recovered_order.push_back(key.clone());
                recovered_bytes = recovered_bytes.saturating_add(block_len);
                report.recovered_files = report.recovered_files.saturating_add(1);
                report.recovered_bytes = report.recovered_bytes.saturating_add(block_len);
                let block_kind = infer_block_kind(&key);
                let routing_slot = extract_routing_slot(&key);
                self.metadata.entry(key).or_insert(CacheEntryMeta {
                    block_kind,
                    routing_slot,
                    hotness: AtomicU32::new(0),
                    hits: AtomicU64::new(0),
                    last_access_epoch: AtomicU64::new(0),
                    admission_reason: CacheAdmissionReason::MemoryOnly,
                    expires_at_millis: AtomicU64::new(0),
                });
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
                if recovered_index.insert(key.clone(), block_len).is_some() {
                    report.skipped_files = report.skipped_files.saturating_add(1);
                    continue;
                }
                recovered_order.push_back(key.clone());
                recovered_bytes = recovered_bytes.saturating_add(block_len);
                report.recovered_files = report.recovered_files.saturating_add(1);
                report.recovered_bytes = report.recovered_bytes.saturating_add(block_len);
                let block_kind = infer_block_kind(&key);
                let routing_slot = extract_routing_slot(&key);
                self.metadata.entry(key).or_insert(CacheEntryMeta {
                    block_kind,
                    routing_slot,
                    hotness: AtomicU32::new(0),
                    hits: AtomicU64::new(0),
                    last_access_epoch: AtomicU64::new(0),
                    admission_reason: CacheAdmissionReason::MemoryOnly,
                    expires_at_millis: AtomicU64::new(0),
                });
            }

            self.disk_index = recovered_index;
            self.disk_order = recovered_order.into_iter().collect();
            self.ssd_bytes = recovered_bytes;
            Ok(report)
        }
    }

    fn default_request(&self, key: &CacheKey, block_bytes: usize) -> CacheAdmissionRequest {
        // A reference is enough: this reads four fields and drops the rest.
        let existing = self.metadata.get(key);
        CacheAdmissionRequest {
            block_kind: existing
                .map(|meta| meta.block_kind)
                .unwrap_or_else(|| infer_block_kind(key)),
            shard_id: key.shard_id,
            routing_slot: existing
                .and_then(|meta| meta.routing_slot)
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
        if let Some(value) = self.memory.remove(key) {
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
            if notifying {
                departing = departing.or(Some((CacheTier::Pmem, value.to_vec())));
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
        self.metadata.remove(key);
        self.stats.expired_removals = self.stats.expired_removals.saturating_add(1);
        if let Some((tier, value)) = departing {
            self.record_eviction(tier, key.clone(), value, CacheRemovalCause::Expired);
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

    /// Whether the entry under `key` has passed its time to live.
    fn entry_expired(&self, key: &CacheKey, now_millis: u64) -> bool {
        self.metadata
            .get(key)
            .is_some_and(|meta| meta.is_expired(now_millis))
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
        if let Some(meta) = self.metadata.get_mut(key) {
            meta.block_kind = block_kind;
            meta.routing_slot = routing_slot;
            meta.last_access_epoch.store(epoch, Ordering::Relaxed);
            meta.admission_reason = admission_reason;
            // Rewriting an entry restarts its life, which is what a caller
            // means by putting the key again.
            meta.set_ttl(self.default_ttl_millis, epoch);
            return;
        }
        self.metadata.insert(
            key.clone(),
            CacheEntryMeta {
                block_kind,
                routing_slot,
                hotness: AtomicU32::new(
                    initial_hotness(block_kind, block_bytes).max(requested_hotness),
                ),
                hits: AtomicU64::new(0),
                last_access_epoch: AtomicU64::new(epoch),
                admission_reason,
                expires_at_millis: AtomicU64::new(0),
            },
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
        let crossed_threshold = if let Some(entry) = self.metadata.get(key) {
            entry.hits.fetch_add(1, Ordering::Relaxed);
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
            self.metadata.insert(
                key.clone(),
                CacheEntryMeta {
                    block_kind,
                    routing_slot: extract_routing_slot(key),
                    hotness: AtomicU32::new(hotness),
                    hits: AtomicU64::new(1),
                    last_access_epoch: AtomicU64::new(epoch),
                    admission_reason: CacheAdmissionReason::MemoryOnly,
                    expires_at_millis: AtomicU64::new(0),
                },
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
        let Some(entry) = self.metadata.get(key) else {
            return HitOutcome::NeedsMetadata;
        };
        let epoch = CoarseClock::now_millis();
        self.reconfigure_refresh_window(epoch);
        let threshold = self.tiering_policy.memory_hotness_threshold;
        let hits_before = entry.hits.fetch_add(1, Ordering::Relaxed);
        // A sampled hit signal for the admission sketch, and only when the
        // filter that consumes it is on -- a bool already in this cache line,
        // against ~1.6ns of sketch work nothing would read.
        //
        // Recording every hit measured ~26ns against a ~226ns read; one in
        // sixteen is about 1.6ns. The comparison it feeds is ordinal, so the
        // lost resolution costs nothing: a key hit a thousand times still
        // records far more than one hit twice.
        if self.admission_filter_enabled && hits_before % HIT_SAMPLE_INTERVAL == 0 {
            self.access_frequency.record(key);
        }
        let before = entry.hotness.fetch_add(1, Ordering::Relaxed);
        // Read, do not swap: this stamp records when the entry was last MOVED
        // in the access order, not when it was last read. Swapping here made
        // the window measure the gap between consecutive reads, so an entry
        // read continuously always looked freshly seen and was never promoted
        // -- the exact opposite of what the window is for. The stamp is
        // advanced below, only when the move actually happens.
        let seen_at = entry.last_access_epoch.load(Ordering::Relaxed);
        if before < threshold && before.saturating_add(1) >= threshold {
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
        if let Some(meta) = self.metadata.get(key) {
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
            .and_then(|key| self.metadata.get(key))
            .map(|meta| now.saturating_sub(meta.last_access_epoch.load(Ordering::Relaxed)))
            .unwrap_or(0);

        let scaled = (oldest_age as f64 * self.lru_refresh_ratio) as u64;
        let effective = scaled
            .max(self.lru_refresh_floor_millis)
            .min(LRU_REFRESH_CAP.as_millis() as u64);
        self.lru_refresh_effective_millis
            .store(effective, Ordering::Relaxed);
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
        let Some(victim) = self.memory_order.iter_access().next() else {
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
    fn drop_stale_tier_copy(&mut self, key: &CacheKey, tier: CacheTier) {
        match tier {
            CacheTier::Memory => {
                if let Some(stale) = self.memory.remove(key) {
                    self.memory_bytes = self.memory_bytes.saturating_sub(stale.len());
                    self.stats.memory_bytes = self.memory_bytes as u64;
                    self.stats.stale_tier_copies_dropped =
                        self.stats.stale_tier_copies_dropped.saturating_add(1);
                }
                self.memory_order.remove(key);
            }
            CacheTier::Pmem => {
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
    }

    fn put_memory(&mut self, key: CacheKey, value: Vec<u8>) -> bool {
        let eviction_started = Instant::now();
        if self.memory_capacity_bytes == 0 || value.len() > self.memory_capacity_bytes {
            self.stats.memory_admission_rejected += 1;
            self.stats.eviction_oversize += 1;
            return false;
        }
        // Recorded whether or not it is admitted: a key rejected once has to be
        // able to earn its way in, or the filter is a permanent ban rather than
        // a comparison.
        self.access_frequency.record(&key);
        if !self.candidate_beats_the_victim(&key, value.len()) {
            self.stats.memory_admission_rejected += 1;
            return false;
        }
        self.stats.memory_admission_accepted += 1;
        self.stats.memory_fills += 1;
        let value = Arc::<[u8]>::from(value);
        if let Some(old) = self.memory.insert(key.clone(), Arc::clone(&value)) {
            self.memory_bytes = self.memory_bytes.saturating_sub(old.len());
        } else {
            self.memory_order.push_back_if_absent(key);
        }
        self.memory_bytes += value.len();
        self.evict_memory_to_capacity_since(eviction_started);
        true
    }

    fn put_pmem(&mut self, key: CacheKey, value: Vec<u8>) -> bool {
        self.put_pmem_with_persistence(key, value, true)
    }

    fn put_pmem_with_persistence(
        &mut self,
        key: CacheKey,
        value: Vec<u8>,
        persist: bool,
    ) -> bool {
        let eviction_started = Instant::now();
        if self.pmem_capacity_bytes == 0 || value.len() > self.pmem_capacity_bytes {
            self.stats.pmem_admission_rejected =
                self.stats.pmem_admission_rejected.saturating_add(1);
            return false;
        }
        if persist && self.persist_pmem_put(&key, &value).is_err() {
            self.stats.pmem_admission_rejected =
                self.stats.pmem_admission_rejected.saturating_add(1);
            return false;
        }
        self.stats.pmem_admission_accepted = self.stats.pmem_admission_accepted.saturating_add(1);
        self.stats.pmem_fills = self.stats.pmem_fills.saturating_add(1);
        let value = Arc::<[u8]>::from(value);
        if let Some(old) = self.pmem.insert(key.clone(), Arc::clone(&value)) {
            self.pmem_bytes = self.pmem_bytes.saturating_sub(old.len());
        } else {
            self.pmem_order.push_back_if_absent(key);
        }
        self.pmem_bytes = self.pmem_bytes.saturating_add(value.len());
        self.evict_pmem_to_capacity_since(eviction_started);
        true
    }

    fn put_ssd_bypass_if_absent(&mut self, key: CacheKey, value: Vec<u8>) -> bool {
        if self.disk_index.contains_key(&key) || self.ssd_capacity_bytes == 0 {
            return false;
        }
        if !self.ssd_write_budget_admits(&key) {
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
        let Ok(block) = encode_cache_block(&value, self.block_options) else {
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
        if self.write_ssd_block(&key, &block).is_err() {
            self.stats.ssd_admission_rejected = self.stats.ssd_admission_rejected.saturating_add(1);
            return false;
        }
        // Record the entry before claiming it. Recovery reads only the manifest,
        // so a block it never records is unreachable after a restart: never
        // served, never counted, and never reclaimed by any eviction path. Undo
        // the write and reject, exactly as a failed block write is rejected.
        if self.append_disk_manifest_put(&key, block_len as u64).is_err() {
            let _ = self.delete_ssd_blocks(std::slice::from_ref(&key));
            self.stats.ssd_admission_rejected = self.stats.ssd_admission_rejected.saturating_add(1);
            return false;
        }
        self.disk_index.insert(key.clone(), block_len as u64);
        self.disk_order.push_back_if_absent(key.clone());
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

    fn demote_memory_victim(&mut self, key: &CacheKey, value: &[u8]) -> bool {
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
                return true;
            }
            return self.put_pmem(key.clone(), value.to_vec());
        }
        if self.ssd_capacity_bytes > 0 {
            return self.put_ssd_bypass_if_absent(key.clone(), value.to_vec());
        }
        false
    }

    fn demote_pmem_victim(&mut self, key: &CacheKey, value: &[u8]) -> bool {
        if !self.eviction_handler_enabled
            || self.ssd_capacity_bytes == 0
            || !self.demotion_is_worthwhile(key)
        {
            return false;
        }
        self.put_ssd_bypass_if_absent(key.clone(), value.to_vec())
    }

    fn refill_from_ssd(&mut self, key: CacheKey, value: Vec<u8>) -> bool {
        if matches!(
            self.tiering_policy.data_placement,
            CacheDataPlacement::SideBySide
        ) && self.pmem_capacity_bytes > 0
            && value.len() > self.tiering_policy.data_placement_threshold_bytes
        {
            return self.put_pmem(key, value);
        }
        self.put_memory(key, value)
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
            let Some(old_value) = self.memory.remove(&victim) else {
                // A key the order still listed but the tier no longer holds.
                // It is gone from the order now, so the next round makes
                // progress rather than picking it again.
                continue;
            };
            self.memory_bytes = self.memory_bytes.saturating_sub(old_value.len());
            let demoted = self.demote_memory_victim(&victim, &old_value);
            self.record_eviction(
                CacheTier::Memory,
                victim.clone(),
                old_value.to_vec(),
                removal_cause(reason),
            );
            self.stats.memory_evictions = self.stats.memory_evictions.saturating_add(1);
            self.stats.eviction_capacity = self.stats.eviction_capacity.saturating_add(1);
            self.stats.memory_slot_evictions = self.stats.memory_slot_evictions.saturating_add(1);
            self.record_memory_eviction_reason(reason);
            if !demoted
                && !self.pmem.contains_key(&victim)
                && !self.disk_index.contains_key(&victim)
            {
                self.metadata.remove(&victim);
            }
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
            let demoted = self.demote_pmem_victim(&victim, &old_value);
            let _ = self.persist_pmem_delete(&victim);
            self.record_eviction(
                CacheTier::Pmem,
                victim.clone(),
                old_value.to_vec(),
                removal_cause(reason),
            );
            self.stats.pmem_evictions = self.stats.pmem_evictions.saturating_add(1);
            self.stats.pmem_eviction_capacity = self.stats.pmem_eviction_capacity.saturating_add(1);
            if !demoted
                && !self.memory.contains_key(&victim)
                && !self.disk_index.contains_key(&victim)
            {
                self.metadata.remove(&victim);
            }
            self.record_eviction_latency(eviction_started);
            if self.pmem_bytes == before {
                break;
            }
        }
    }

    fn evict_ssd_for(&mut self, incoming_bytes: u64) {
        self.evict_ssd_for_batch(incoming_bytes);
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
            self.record_eviction(
                CacheTier::Ssd,
                victim.clone(),
                evicted_value.clone(),
                removal_cause(reason),
            );
            self.ssd_bytes = self.ssd_bytes.saturating_sub(removed_bytes);
            self.stats.ssd_evictions = self.stats.ssd_evictions.saturating_add(1);
            self.stats.ssd_eviction_capacity = self.stats.ssd_eviction_capacity.saturating_add(1);
            self.stats.ssd_slot_evictions = self.stats.ssd_slot_evictions.saturating_add(1);
            self.record_ssd_eviction_reason(reason);
            if !self.memory.contains_key(&victim) {
                self.metadata.remove(&victim);
            }
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
            CacheReplacementPolicy::Slru | CacheReplacementPolicy::WeightedHotnessLru => {
                let picked = self.select_windowed_eviction_victim(&self.memory_order);
                self.record_sampled_groups(picked.groups_weighed);
                picked.victim
            }
        }
    }

    fn select_pmem_eviction_victim(&mut self) -> Option<(CacheKey, EvictionReason, u64)> {
        match self.pmem_replacement_policy {
            CacheReplacementPolicy::Fifo => {
                self.select_fifo_eviction_victim(&self.pmem_order)
            }
            CacheReplacementPolicy::Slru | CacheReplacementPolicy::WeightedHotnessLru => {
                let picked = self.select_windowed_eviction_victim(&self.pmem_order);
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
            CacheReplacementPolicy::Slru | CacheReplacementPolicy::WeightedHotnessLru => {
                let picked = self.select_windowed_eviction_victim(&self.disk_order);
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
    fn select_windowed_eviction_victim(&self, order: &CacheKeyOrder) -> PickedEvictionVictim {
        // An entry past its time to live could not have been served again, so
        // dropping it costs no future hit. Take one the moment the window turns
        // one up, in preference to weighing live entries against each other and
        // throwing out something a caller still wants.
        //
        // Scanned over the same bounded window as the scoring below, so this
        // adds a comparison per candidate rather than a pass over the tier.
        let now_millis = CoarseClock::now_millis();
        for candidate in order.iter_access().take(EVICTION_CANDIDATE_WINDOW) {
            if self.is_pinned(candidate) {
                continue;
            }
            if self.entry_expired(candidate, now_millis) {
                return PickedEvictionVictim {
                    victim: Some((candidate.clone(), EvictionReason::Expired, 0)),
                    groups_weighed: 0,
                };
            }
        }
        let windowed =
            self.select_eviction_victim(order.iter_access().take(EVICTION_CANDIDATE_WINDOW));
        if windowed.victim.is_some() || order.len() <= EVICTION_CANDIDATE_WINDOW {
            return windowed;
        }
        let full = self.select_eviction_victim(order.iter_access());
        PickedEvictionVictim {
            victim: full.victim,
            groups_weighed: windowed
                .groups_weighed
                .saturating_add(full.groups_weighed),
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
    fn select_eviction_victim<'a, I>(&self, keys: I) -> PickedEvictionVictim
    where
        I: IntoIterator<Item = &'a CacheKey>,
    {
        let mut pinned_skips = 0u64;
        let mut groups: HashMap<EvictionGroupKey<'a>, SlotEvictionGroup<'a>> = HashMap::new();
        for key in keys {
            if self.is_pinned(key) {
                pinned_skips = pinned_skips.saturating_add(1);
                continue;
            }
            // One lookup, both answers. Scoring and grouping ask the same
            // map about the same key, and hashing a CacheKey hashes three
            // Strings -- doing it twice per candidate, up to 512 candidates
            // per eviction, is the bulk of choosing a victim.
            let meta = self.metadata.get(key);
            let score = eviction_score_of(meta);
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
        }
    }

    fn eviction_score(&self, key: &CacheKey) -> EvictionScore {
        eviction_score_of(self.metadata.get(key))
    }

    fn incoming_ssd_block_is_colder_than_existing_groups(
        &self,
        key: &CacheKey,
        request: &CacheAdmissionRequest,
        block_bytes: usize,
    ) -> bool {
        let incoming_score = EvictionScore {
            hotness: initial_hotness(request.block_kind, block_bytes).max(request.hotness),
            hits: 0,
            last_access_epoch: CoarseClock::now_millis(),
        };
        let incoming_group = request
            .routing_slot
            .or_else(|| extract_routing_slot(key))
            .map(EvictionGroupKey::Slot)
            .unwrap_or(EvictionGroupKey::Object(&key.namespace, &key.record_key));
        self.disk_order
            .iter()
            .take(EVICTION_CANDIDATE_WINDOW)
            .filter(|candidate| self.eviction_group_key(candidate) != incoming_group)
            .map(|candidate| self.eviction_score(candidate))
            .min()
            .map(|coldest_existing| incoming_score < coldest_existing)
            .unwrap_or(false)
    }

    fn eviction_group_key<'a>(&self, key: &'a CacheKey) -> EvictionGroupKey<'a> {
        eviction_group_key_of(self.metadata.get(key), key)
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

    fn record_eviction(
        &mut self,
        tier: CacheTier,
        key: CacheKey,
        value: Vec<u8>,
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
                    key,
                    value,
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
            .and_then(|block| decode_cache_block(&block).ok())
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
            .then(|| keys.iter().cloned().collect::<HashSet<_>>());
        let disk_delete_set = disk_delete_keys.and_then(|delete_keys| {
            (delete_keys.len() > SET_MEMBERSHIP_THRESHOLD)
                .then(|| delete_keys.iter().cloned().collect::<HashSet<_>>())
        });
        let mut disk_keys = Vec::new();
        for key in keys {
            let key_pinned = self.is_pinned(key);
            let mut removed_pinned_bytes = 0usize;
            if let Some(value) = self.memory.remove(key) {
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
                let should_delete = match (disk_delete_keys, disk_delete_set.as_ref()) {
                    (None, _) => true,
                    (Some(_), Some(delete_keys)) => delete_keys.contains(key),
                    (Some(delete_keys), None) => delete_keys.contains(key),
                };
                if disk_bytes > 0 || should_delete {
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
            self.metadata.remove(key);
            {
                let mut pins = self.pins_for(key);
                if key_pinned && removed_pinned_bytes > 0 {
                    // Still held, so remember what the handle is worth now the
                    // tier no longer has it.
                    pins.entries
                        .entry(key.clone())
                        .or_default()
                        .removed_bytes = Some(removed_pinned_bytes);
                } else {
                    pins.entries.remove(key);
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
        self.metadata.clear();
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
    fn pins_for(&self, key: &CacheKey) -> std::sync::MutexGuard<'_, CachePinState> {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let stripe = (hasher.finish() as usize) % self.pins.len();
        self.pins[stripe].lock().expect("pin lock poisoned")
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
            .memory
            .get(key)
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

    /// Pin, and count the handle, under a single acquisition.
    ///
    /// The two used to be separate locks a line apart on the read path. They
    /// describe the same event, so there is no reason to take the lock twice
    /// for it.
    fn increment_pin_for_handle(&self, key: &CacheKey, bytes: usize) {
        let mut pins = self.pins_for(key);
        // One lookup for the whole handle: the count and its size are the same
        // entry, so they cannot be found separately or updated apart.
        let entry = pins.entries.entry(key.clone()).or_default();
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
        let entry = pins.entries.entry(key.clone()).or_default();
        entry.handles = entry.handles.saturating_add(1);
        if let Some(bytes) = bytes {
            entry.handle_bytes = entry.handle_bytes.max(bytes);
        }
        pins.pin_operations = pins.pin_operations.saturating_add(1);
    }

    fn decrement_pin(&self, key: &CacheKey) {
        let mut pins = self.pins_for(key);
        let Some(entry) = pins.entries.get_mut(key) else {
            return;
        };
        if entry.handles > 1 {
            entry.handles -= 1;
        } else {
            // The last handle takes everything known about the key with it, in
            // one removal.
            pins.entries.remove(key);
        }
        pins.unpin_operations = pins.unpin_operations.saturating_add(1);
    }

    fn pinned_removed_bytes_total(&self) -> usize {
        self.all_pins()
            .iter()
            .flat_map(|pins| pins.entries.values().filter_map(|entry| entry.removed_bytes))
            .fold(0usize, usize::saturating_add)
    }

    fn pinned_memory_bytes(&self) -> u64 {
        self.all_pins()
            .iter()
            .flat_map(|pins| {
                pins.entries.iter().map(|(key, entry)| {
                    let memory_bytes =
                        self.memory.get(key).map(|value| value.len()).unwrap_or(0);
                    let pmem_bytes = self.pmem.get(key).map(|value| value.len()).unwrap_or(0);
                    let removed_bytes = entry.removed_bytes.unwrap_or(0);
                    memory_bytes
                        .max(pmem_bytes)
                        .max(entry.handle_bytes)
                        .max(removed_bytes) as u64
                })
            })
            .sum::<u64>()
    }

    fn record_get_latency(&self, started: Instant) {
        let micros = elapsed_micros(started);
        self.record_get_latency_micros(micros);
    }

    fn record_get_latency_micros(&self, micros: u64) {
        self.read_counters.get_latency.observe_with_total(micros);
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
        self.read_counters.read_through_latency.observe(micros);
    }

    fn record_refill_latency(&self, started: Instant) {
        let micros = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
        self.read_counters.refill_latency.observe(micros);
    }

    fn record_writeback_latency(&mut self, started: Instant) {
        let micros = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
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
fn eviction_score_of(meta: Option<&CacheEntryMeta>) -> EvictionScore {
    match meta {
        Some(meta) => EvictionScore {
            hotness: meta.hotness.load(Ordering::Relaxed),
            hits: meta.hits.load(Ordering::Relaxed),
            last_access_epoch: meta.last_access_epoch.load(Ordering::Relaxed),
        },
        None => EvictionScore {
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
    meta.and_then(|meta| meta.routing_slot)
        .or_else(|| extract_routing_slot(key))
        .map(EvictionGroupKey::Slot)
        .unwrap_or(EvictionGroupKey::Object(&key.namespace, &key.record_key))
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
    match key.namespace.as_str() {
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
const EVICTION_CANDIDATE_WINDOW: usize = 512;

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

fn extract_routing_slot(key: &CacheKey) -> Option<u32> {
    let suffix = key.selector.strip_prefix("slot-")?;
    let (slot, _) = suffix.split_once(':')?;
    slot.parse::<u32>().ok()
}

fn encode_manifest_field(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn decode_manifest_field(value: &str) -> Option<String> {
    if !value.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for chunk in value.as_bytes().chunks_exact(2) {
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
