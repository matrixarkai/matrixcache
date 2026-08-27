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

    pub fn build_simple_lru_cache(capacity: usize) -> SimpleLRUCache {
        SimpleLRUCache::new(capacity)
    }

    pub fn build_zero_copy_simple_lru_cache(capacity: usize) -> ZeroCopySimpleLRUCache {
        ZeroCopySimpleLRUCache::new(capacity)
    }

    pub fn build_concurrent_simple_lru_cache(capacity: usize) -> ConcurrentSimpleLruCache {
        ConcurrentSimpleLruCache::new(capacity)
    }

    pub fn build_memcached_wrapper(capacity: usize) -> MemcachedWrapper {
        MemcachedWrapper::new(capacity)
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
    pub fn BuildSimpleLRUCache(capacity: usize) -> SimpleLRUCache {
        Self::build_simple_lru_cache(capacity)
    }

    #[allow(non_snake_case)]
    pub fn BuildZeroCopySimpleLRUCache(capacity: usize) -> ZeroCopySimpleLRUCache {
        Self::build_zero_copy_simple_lru_cache(capacity)
    }

    #[allow(non_snake_case)]
    pub fn BuildConcurrentSimpleLRUCache(capacity: usize) -> ConcurrentSimpleLruCache {
        Self::build_concurrent_simple_lru_cache(capacity)
    }

    #[allow(non_snake_case)]
    pub fn BuildMemcachedWrapper(capacity: usize) -> MemcachedWrapper {
        Self::build_memcached_wrapper(capacity)
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
fn write_cache_block_atomic(path: &Path, block: &[u8]) -> Result<(), CacheError> {
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
        temp.sync_all()?;
    }
    fs::rename(&temp_path, path)?;
    sync_parent_dir(path)?;
    Ok(())
}

#[cfg(not(feature = "rocksdb-ssd"))]
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

    fn read_ssd_blocks(&self, keys: &[CacheKey]) -> Result<Vec<Option<Vec<u8>>>, CacheError> {
        let store_keys = keys.iter().map(Self::ssd_store_key).collect::<Vec<_>>();
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
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            write_cache_block_atomic(&path, block)
        }
    }

    fn write_ssd_blocks(&mut self, entries: &[(CacheKey, Vec<u8>)]) -> Result<(), CacheError> {
        if entries.is_empty() {
            return Ok(());
        }
        self.ssd_store.put_batch(
            entries
                .iter()
                .map(|(key, block)| (Self::ssd_store_key(key), block.clone()))
                .collect(),
        )?;
        #[cfg(feature = "rocksdb-ssd")]
        {
            Ok(())
        }
        #[cfg(not(feature = "rocksdb-ssd"))]
        {
            for (key, block) in entries {
                let path = self.disk_path(key);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                write_cache_block_atomic(&path, block)?;
            }
            Ok(())
        }
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
            fs::create_dir_all(&self.disk_dir)?;
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.manifest_path())?;
            writeln!(file, "{}", op.encode_line())?;
            Ok(())
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
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        use std::io::Write as _;
        writeln!(file, "{}", line)?;
        Ok(())
    }

    fn persist_pmem_put(&self, key: &CacheKey, value: &[u8]) -> Result<(), CacheError> {
        let Some(path) = self.pmem_block_path_for_key(key) else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, value)?;
        fs::rename(&tmp, &path)?;
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
        self.refresh_usage_stats();
        self.refresh_pin_stats();
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
                    hotness: 0,
                    hits: 0,
                    last_access_epoch: 0,
                    admission_reason: CacheAdmissionReason::MemoryOnly,
                });
            }
            self.disk_index = recovered_index;
            self.disk_order = recovered_order.into_iter().collect();
            self.ssd_bytes = recovered_bytes;
            self.stats.disk_bytes = recovered_bytes;
            Ok(report)
        }
        #[cfg(not(feature = "rocksdb-ssd"))]
        {
            let manifest_path = self.manifest_path();
            if !manifest_path.exists() {
                self.disk_index.clear();
                self.ssd_bytes = 0;
                self.stats.disk_bytes = 0;
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
                    hotness: 0,
                    hits: 0,
                    last_access_epoch: 0,
                    admission_reason: CacheAdmissionReason::MemoryOnly,
                });
            }

            self.disk_index = recovered_index;
            self.disk_order = recovered_order.into_iter().collect();
            self.ssd_bytes = recovered_bytes;
            self.stats.disk_bytes = recovered_bytes;
            Ok(report)
        }
    }

    fn default_request(&self, key: &CacheKey, block_bytes: usize) -> CacheAdmissionRequest {
        let existing = self.metadata.get(key).copied();
        CacheAdmissionRequest {
            block_kind: existing
                .map(|meta| meta.block_kind)
                .unwrap_or_else(|| infer_block_kind(key)),
            shard_id: key.shard_id,
            routing_slot: existing
                .and_then(|meta| meta.routing_slot)
                .or_else(|| extract_routing_slot(key)),
            block_bytes,
            hotness: existing.map(|meta| meta.hotness).unwrap_or_default(),
            pinned: self.pinned.contains_key(key),
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

    fn record_metadata(
        &mut self,
        key: &CacheKey,
        block_kind: CacheBlockKind,
        routing_slot: Option<u32>,
        block_bytes: usize,
        requested_hotness: u32,
        admission_reason: CacheAdmissionReason,
    ) {
        self.access_epoch = self.access_epoch.saturating_add(1);
        let current = self.metadata.get(key).copied();
        self.metadata.insert(
            key.clone(),
            CacheEntryMeta {
                block_kind,
                routing_slot,
                hotness: current.map(|meta| meta.hotness).unwrap_or_else(|| {
                    initial_hotness(block_kind, block_bytes).max(requested_hotness)
                }),
                hits: current.map(|meta| meta.hits).unwrap_or_default(),
                last_access_epoch: self.access_epoch,
                admission_reason,
            },
        );
    }

    fn record_hit_metadata(&mut self, key: &CacheKey, block_bytes: usize) {
        self.access_epoch = self.access_epoch.saturating_add(1);
        let epoch = self.access_epoch;
        let threshold = self.tiering_policy.memory_hotness_threshold;

        // The hit path runs for every read, so it must not pay for the miss
        // path. Looking the entry up first keeps the key clone, the block-kind
        // inference and the routing-slot extraction on the branch that
        // actually needs them; going through `entry()` did all of that on
        // every hit and then discarded it.
        let crossed_threshold = if let Some(entry) = self.metadata.get_mut(key) {
            entry.hits = entry.hits.saturating_add(1);
            let before = entry.hotness;
            entry.hotness = entry.hotness.saturating_add(1);
            entry.last_access_epoch = epoch;
            before < threshold && entry.hotness >= threshold
        } else {
            let block_kind = infer_block_kind(key);
            let before = initial_hotness(block_kind, block_bytes);
            let hotness = before.saturating_add(1);
            self.metadata.insert(
                key.clone(),
                CacheEntryMeta {
                    block_kind,
                    routing_slot: extract_routing_slot(key),
                    hotness,
                    hits: 1,
                    last_access_epoch: epoch,
                    admission_reason: CacheAdmissionReason::MemoryOnly,
                },
            );
            before < threshold && hotness >= threshold
        };

        if crossed_threshold {
            self.stats.hotness_promotions = self.stats.hotness_promotions.saturating_add(1);
        }
    }

    fn record_hit(&mut self, key: &CacheKey, block_bytes: usize) {
        self.record_hit_metadata(key, block_bytes);
        // Move the entry to the back of each tier's access order. Victim
        // selection reads the front of that order, so without this an entry
        // written early stays at the front however often it is read, and is
        // offered up for eviction on every pass.
        self.memory_order.touch_access(key);
        self.pmem_order.touch_access(key);
        self.disk_order.touch_access(key);
    }

    fn put_memory(&mut self, key: CacheKey, value: Vec<u8>) -> bool {
        let eviction_started = Instant::now();
        if self.memory_capacity_bytes == 0 || value.len() > self.memory_capacity_bytes {
            self.stats.memory_admission_rejected += 1;
            self.stats.eviction_oversize += 1;
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
        self.stats.memory_bytes = self.memory_bytes as u64;
        self.refresh_pin_stats();
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
        self.stats.pmem_bytes = self.pmem_bytes as u64;
        self.refresh_pin_stats();
        true
    }

    fn put_ssd_bypass_if_absent(&mut self, key: CacheKey, value: Vec<u8>) -> bool {
        if self.disk_index.contains_key(&key) || self.ssd_capacity_bytes == 0 {
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
        self.disk_index.insert(key.clone(), block_len as u64);
        self.disk_order.push_back_if_absent(key.clone());
        self.ssd_bytes = self.ssd_bytes.saturating_add(block_len as u64);
        self.stats.disk_fills = self.stats.disk_fills.saturating_add(1);
        self.stats.ssd_admission_accepted = self.stats.ssd_admission_accepted.saturating_add(1);
        self.stats.disk_bytes = self.ssd_bytes;
        let _ = self.append_disk_manifest_put(&key, block_len as u64);
        true
    }

    fn demote_memory_victim(&mut self, key: &CacheKey, value: &[u8]) -> bool {
        if !self.eviction_handler_enabled {
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
        if !self.eviction_handler_enabled || self.ssd_capacity_bytes == 0 {
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
            self.record_eviction(CacheTier::Memory, victim.clone(), old_value.to_vec());
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
        self.stats.memory_bytes = self.memory_bytes as u64;
        self.refresh_pin_stats();
    }

    fn evict_pmem_to_capacity_since(&mut self, eviction_started: Instant) {
        while self.pmem_bytes > self.pmem_capacity_bytes {
            let before = self.pmem_bytes;
            let Some((victim, _reason, pinned_skips)) = self.select_pmem_eviction_victim() else {
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
            self.record_eviction(CacheTier::Pmem, victim.clone(), old_value.to_vec());
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
        self.stats.pmem_bytes = self.pmem_bytes as u64;
        self.refresh_pin_stats();
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
            self.record_eviction(CacheTier::Ssd, victim.clone(), evicted_value.clone());
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
            self.stats.disk_bytes = self.ssd_bytes;
            return;
        }
        let _ = self.delete_ssd_blocks(&victim_keys);
        for key in &victim_keys {
            let _ = self.append_disk_manifest_delete(key);
        }
        self.stats.disk_bytes = self.ssd_bytes;
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
            if self.pinned.contains_key(key) {
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
        let mut groups: HashMap<EvictionGroupKey<'a>, SlotEvictionGroup> = HashMap::new();
        for key in keys {
            if self.pinned.contains_key(key) {
                pinned_skips = pinned_skips.saturating_add(1);
                continue;
            }
            let score = self.eviction_score(key);
            let group_key = self.eviction_group_key(key);
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
                    .then_with(|| left.victim.cmp(&right.victim))
            })
            .map(|group| {
                (
                    group.victim,
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
        let meta = self.metadata.get(key).copied().unwrap_or(CacheEntryMeta {
            block_kind: infer_block_kind(key),
            routing_slot: extract_routing_slot(key),
            hotness: 0,
            hits: 0,
            last_access_epoch: 0,
            admission_reason: CacheAdmissionReason::MemoryOnly,
        });
        EvictionScore {
            hotness: meta.hotness,
            hits: meta.hits,
            last_access_epoch: meta.last_access_epoch,
        }
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
            last_access_epoch: self.access_epoch.saturating_add(1),
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
        self.metadata
            .get(key)
            .and_then(|meta| meta.routing_slot)
            .or_else(|| extract_routing_slot(key))
            .map(EvictionGroupKey::Slot)
            .unwrap_or(EvictionGroupKey::Object(&key.namespace, &key.record_key))
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
        }
    }

    fn record_eviction(&mut self, tier: CacheTier, key: CacheKey, value: Vec<u8>) {
        // The metric counts evictions and needs no value, so it is recorded
        // even while the eviction handler is disabled.
        if self.eviction_metric_callback.is_some() {
            self.pending_eviction_metric_tiers.push_back(tier);
        }
        if self.eviction_handler_enabled && self.eviction_callback.is_some() {
            self.pending_eviction_records
                .push_back(CacheEvictionRecord { tier, key, value });
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
            let key_pinned = self.pinned.contains_key(key);
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
            if key_pinned && removed_pinned_bytes > 0 {
                self.pinned_removed_bytes
                    .insert(key.clone(), removed_pinned_bytes);
            } else {
                self.pinned.remove(key);
                self.pinned_removed_bytes.remove(key);
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
        self.stats.memory_bytes = self.memory_bytes as u64;
        self.stats.pmem_bytes = self.pmem_bytes as u64;
        self.stats.disk_bytes = self.ssd_bytes;
        self.refresh_pin_stats();
    }

    fn clear_all_locked(&mut self, reset_stats: bool) -> Result<(), CacheError> {
        self.memory.clear();
        self.pmem.clear();
        self.clear_pmem_persistence()?;
        self.disk_index.clear();
        self.disk_order.clear();
        self.memory_order.clear();
        self.pmem_order.clear();
        self.pinned.clear();
        self.pinned_handle_bytes.clear();
        self.pinned_removed_bytes.clear();
        self.metadata.clear();
        self.async_writeback_queue.clear();
        self.async_writeback_positions.clear();
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
            self.access_epoch = 0;
        } else {
            self.stats.invalidations = self.stats.invalidations.saturating_add(1);
            self.refresh_usage_stats();
            self.refresh_pin_stats();
            self.refresh_async_writeback_pressure_stats();
        }
        Ok(())
    }

    fn refresh_usage_stats(&mut self) {
        self.stats.memory_bytes = self.memory_bytes as u64;
        self.stats.pmem_bytes = self.pmem_bytes as u64;
        self.stats.disk_bytes = self.ssd_bytes;
    }

    fn increment_pin(&mut self, key: &CacheKey) {
        let bytes = self
            .memory
            .get(key)
            .map(|value| value.len())
            .or_else(|| self.pmem.get(key).map(|value| value.len()))
            .or_else(|| self.pinned_removed_bytes.get(key).copied());
        self.increment_pin_with_optional_size(key, bytes);
    }

    fn increment_pin_with_size(&mut self, key: &CacheKey, bytes: usize) {
        self.increment_pin_with_optional_size(key, Some(bytes));
    }

    fn increment_pin_with_optional_size(&mut self, key: &CacheKey, bytes: Option<usize>) {
        let count = self.pinned.entry(key.clone()).or_insert(0);
        *count = count.saturating_add(1);
        if let Some(bytes) = bytes {
            self.pinned_handle_bytes
                .entry(key.clone())
                .and_modify(|existing| *existing = (*existing).max(bytes))
                .or_insert(bytes);
        }
        self.stats.pin_operations = self.stats.pin_operations.saturating_add(1);
        self.refresh_pin_stats();
    }

    fn decrement_pin(&mut self, key: &CacheKey) {
        if let Some(count) = self.pinned.get_mut(key) {
            if *count > 1 {
                *count -= 1;
            } else {
                self.pinned.remove(key);
                self.pinned_handle_bytes.remove(key);
                self.pinned_removed_bytes.remove(key);
            }
            self.stats.unpin_operations = self.stats.unpin_operations.saturating_add(1);
        }
        self.refresh_pin_stats();
    }

    fn pinned_removed_bytes_total(&self) -> usize {
        self.pinned_removed_bytes
            .values()
            .copied()
            .fold(0usize, usize::saturating_add)
    }

    fn pinned_memory_bytes(&self) -> u64 {
        let live_pinned_bytes = self
            .pinned
            .keys()
            .map(|key| {
                let memory_bytes = self.memory.get(key).map(|value| value.len()).unwrap_or(0);
                let pmem_bytes = self.pmem.get(key).map(|value| value.len()).unwrap_or(0);
                let handle_bytes = self.pinned_handle_bytes.get(key).copied().unwrap_or(0);
                let removed_bytes = self.pinned_removed_bytes.get(key).copied().unwrap_or(0);
                memory_bytes.max(pmem_bytes).max(handle_bytes).max(removed_bytes) as u64
            })
            .sum::<u64>();
        live_pinned_bytes
    }

    fn refresh_pin_stats(&mut self) {
        self.stats.pinned_entries = self.pinned.len() as u64;
        self.stats.pinned_bytes = self.pinned_memory_bytes();
    }

    fn record_get_latency(&mut self, started: Instant) {
        let micros = elapsed_micros(started);
        self.record_get_latency_micros(micros);
    }

    fn record_get_latency_micros(&mut self, micros: u64) {
        self.stats.get_latency_samples = self.stats.get_latency_samples.saturating_add(1);
        self.stats.get_latency_total_micros =
            self.stats.get_latency_total_micros.saturating_add(micros);
        self.stats.get_latency_max_micros = self.stats.get_latency_max_micros.max(micros);
        let mut ignored_samples = 0;
        observe_latency_bucket(
            micros,
            &mut ignored_samples,
            &mut self.stats.get_latency_le_10us,
            &mut self.stats.get_latency_le_100us,
            &mut self.stats.get_latency_le_1ms,
            &mut self.stats.get_latency_le_10ms,
            &mut self.stats.get_latency_gt_10ms,
        );
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

    fn record_read_through_latency(&mut self, started: Instant) {
        let micros = elapsed_micros(started);
        self.record_read_through_latency_micros(micros);
    }

    fn record_read_through_latency_micros(&mut self, micros: u64) {
        observe_latency_bucket(
            micros,
            &mut self.stats.read_through_latency_samples,
            &mut self.stats.read_through_latency_le_10us,
            &mut self.stats.read_through_latency_le_100us,
            &mut self.stats.read_through_latency_le_1ms,
            &mut self.stats.read_through_latency_le_10ms,
            &mut self.stats.read_through_latency_gt_10ms,
        );
    }

    fn record_refill_latency(&mut self, started: Instant) {
        let micros = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
        observe_latency_bucket(
            micros,
            &mut self.stats.refill_latency_samples,
            &mut self.stats.refill_latency_le_10us,
            &mut self.stats.refill_latency_le_100us,
            &mut self.stats.refill_latency_le_1ms,
            &mut self.stats.refill_latency_le_10ms,
            &mut self.stats.refill_latency_gt_10ms,
        );
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

    fn rebuild_async_writeback_positions(&mut self) {
        self.async_writeback_positions.clear();
        self.async_writeback_positions.extend(
            self.async_writeback_queue
                .iter()
                .enumerate()
                .map(|(index, job)| (job.key.clone(), index)),
        );
    }
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
