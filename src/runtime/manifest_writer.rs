// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

const PMEM_MANIFEST_COMPACT_MIN: u64 = 4096;

#[cfg(test)]
mod pmem_manifest_compaction_tests {
    use super::*;

    // rust-internal: compaction drops missing files without relying on resident state.
    #[test]
    fn pmem_compaction_prunes_missing_blocks_and_reopens_for_new_writes() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_options(
            CacheOptions::new(0, 1024, 0).with_pmem_paths([dir.path().join("pmem")]),
        );
        let mut inner = cache.inner.write().unwrap();
        let keep = CacheKey::string(0, "nonresident");
        inner.persist_pmem_put(&keep, b"keep").unwrap();
        for i in 0..512 {
            let key = CacheKey::string(0, &format!("missing-{i}"));
            inner.persist_pmem_put(&key, b"old").unwrap();
            fs::remove_file(inner.pmem_block_path_for_key(&key).unwrap()).unwrap();
        }
        assert!(inner.pmem.is_empty());
        let manifest = inner.pmem_manifest_path().unwrap();
        assert_eq!(fs::read_to_string(&manifest).unwrap().lines().count(), 513);
        inner.compact_pmem_manifest().unwrap();
        assert_eq!(fs::read_to_string(&manifest).unwrap().lines().count(), 1);
        let restored = CacheKey::string(0, "missing-0");
        inner.persist_pmem_put(&restored, b"new").unwrap();
        inner.reset_pmem_manifest_writer();
        let report = inner.recover_pmem_index_locked().unwrap();
        assert_eq!(report.scanned_files, 2);
        assert_eq!(report.recovered_files, 2);
        assert_eq!(inner.pmem.get(&keep).unwrap().as_ref(), b"keep");
        assert_eq!(inner.pmem.get(&restored).unwrap().as_ref(), b"new");
    }

    // rust-internal: metadata errors must not replace the original manifest.
    #[test]
    fn pmem_compaction_preserves_manifest_on_block_metadata_error() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_options(
            CacheOptions::new(0, 1024, 0).with_pmem_paths([dir.path().join("pmem")]),
        );
        let inner = cache.inner.write().unwrap();
        let key = CacheKey::string(0, "keep");
        inner.persist_pmem_put(&key, b"keep").unwrap();
        let manifest = inner.pmem_manifest_path().unwrap();
        let original = fs::read(&manifest).unwrap();
        let block_dir = inner.pmem_block_path_for_key(&key).unwrap().parent().unwrap().to_path_buf();
        let saved = block_dir.with_extension("saved");
        fs::rename(&block_dir, &saved).unwrap();
        fs::write(&block_dir, b"not a directory").unwrap();
        assert!(inner.compact_pmem_manifest().is_err());
        assert_eq!(fs::read(&manifest).unwrap(), original);
    }

    // rust-internal: absent files need no tombstone; recovery must skip old records.
    #[test]
    fn absent_pmem_deletes_do_not_grow_manifest_or_restore_missing_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_options(
            CacheOptions::new(0, 1024, 0).with_pmem_paths([dir.path().join("pmem")]),
        );
        let keep = CacheKey::string(0, "keep");
        let missing = CacheKey::string(0, "missing");
        let deleted = CacheKey::string(0, "deleted");
        let mut inner = cache.inner.write().unwrap();
        inner.persist_pmem_put(&keep, b"keep").unwrap();
        inner.persist_pmem_put(&missing, b"old").unwrap();
        inner.persist_pmem_put(&deleted, b"delete").unwrap();
        inner.persist_pmem_delete(&deleted).unwrap();
        fs::remove_file(inner.pmem_block_path_for_key(&missing).unwrap()).unwrap();
        let manifest = inner.pmem_manifest_path().unwrap();
        let before = fs::read(&manifest).unwrap();
        for index in 0..2048 {
            inner.persist_pmem_delete(&CacheKey::string(0, &format!("absent-{index}"))).unwrap();
        }
        inner.persist_pmem_delete(&missing).unwrap();
        inner.persist_pmem_delete(&deleted).unwrap();
        assert_eq!(fs::read(&manifest).unwrap(), before);
        inner.reset_pmem_manifest_writer();
        assert_eq!(inner.recover_pmem_index_locked().unwrap().recovered_files, 1);
        assert!(inner.pmem.contains_key(&keep));
        assert!(!inner.pmem.contains_key(&missing));
        assert!(!inner.pmem.contains_key(&deleted));
        inner.persist_pmem_put(&missing, b"new").unwrap();
        inner.reset_pmem_manifest_writer();
        assert_eq!(inner.recover_pmem_index_locked().unwrap().recovered_files, 2);
        assert_eq!(inner.pmem.get(&missing).unwrap().as_ref(), b"new");
    }

    // rust-internal: compact the durable log, including nonresident records.
    #[test]
    fn pmem_history_compacts_and_recovers_latest_live_records() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_options(
            CacheOptions::new(0, 1024, 0).with_pmem_paths([dir.path().join("pmem")]),
        );
        let dormant = CacheKey::string(0, "dormant");
        let updated = CacheKey::string(0, "updated");
        let deleted = CacheKey::string(0, "deleted");
        let mut inner = cache.inner.write().unwrap();
        inner.persist_pmem_put(&dormant, b"keep").unwrap();
        inner.persist_pmem_put(&deleted, b"remove").unwrap();
        inner.persist_pmem_delete(&deleted).unwrap();
        for index in 0..PMEM_MANIFEST_COMPACT_MIN {
            inner
                .persist_pmem_put(&updated, index.to_string().as_bytes())
                .unwrap();
        }
        assert!(inner.pmem.is_empty());
        let text = fs::read_to_string(inner.pmem_manifest_path().unwrap()).unwrap();
        assert!(text.lines().count() < 10, "history was not compacted");
        let report = inner.recover_pmem_index_locked().unwrap();
        assert_eq!(report.recovered_files, 2);
        assert_eq!(inner.pmem.get(&dormant).unwrap().as_ref(), b"keep");
        assert_eq!(inner.pmem.get(&updated).unwrap().as_ref(), b"4095");
        assert!(!inner.pmem.contains_key(&deleted));
    }

    // rust-internal: failed compaction must retain the successful append.
    #[test]
    fn pmem_history_preserves_log_on_compaction_failure_and_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_options(
            CacheOptions::new(0, 1024, 0).with_pmem_paths([dir.path().join("pmem")]),
        );
        let key = CacheKey::string(0, "key");
        let mut inner = cache.inner.write().unwrap();
        inner.persist_pmem_put(&key, b"old").unwrap();
        let path = inner.pmem_manifest_path().unwrap();
        let temporary = path.with_extension("compact.tmp");
        fs::create_dir(&temporary).unwrap();
        inner
            .pmem_manifest_compact_after
            .store(1, Ordering::Relaxed);
        inner.persist_pmem_put(&key, b"new-value").unwrap();
        assert_eq!(
            inner.recover_pmem_index_locked().unwrap().recovered_files,
            1
        );
        assert_eq!(inner.pmem.get(&key).unwrap().as_ref(), b"new-value");
        fs::remove_dir(&temporary).unwrap();
        fs::write(&path, b"invalid-record\n").unwrap();
        assert!(inner.compact_pmem_manifest().is_err());
        assert_eq!(fs::read(&path).unwrap(), b"invalid-record\n");
    }
}

#[cfg(unix)]
#[derive(Debug)]
struct PmemManifestWriter {
    path: PathBuf,
    identity: (u64, u64),
    file: File,
}

#[cfg(unix)]
impl PmemManifestWriter {
    fn identity(metadata: &fs::Metadata) -> (u64, u64) {
        use std::os::unix::fs::MetadataExt;
        (metadata.dev(), metadata.ino())
    }

    fn append(writer: &mut Option<Self>, path: &Path, bytes: &[u8]) -> Result<(), CacheError> {
        let matches = writer.as_ref().is_some_and(|current| {
            current.path == path
                && fs::metadata(path)
                    .is_ok_and(|metadata| Self::identity(&metadata) == current.identity)
        });
        if !matches {
            *writer = None;
            let file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;
            *writer = Some(Self {
                path: path.to_path_buf(),
                identity: Self::identity(&file.metadata()?),
                file,
            });
        }
        let result = std::io::Write::write_all(
            &mut writer.as_mut().expect("manifest writer opened").file,
            bytes,
        );
        if result.is_err() {
            *writer = None;
        }
        result.map_err(CacheError::Io)
    }
}

impl CacheInner {
    fn reset_pmem_manifest_history(&self) {
        self.pmem_manifest_appends.store(0, Ordering::Relaxed);
        self.pmem_manifest_compact_after
            .store(PMEM_MANIFEST_COMPACT_MIN, Ordering::Relaxed);
    }

    fn compact_pmem_manifest(&self) -> Result<(), CacheError> {
        use std::io::{BufRead, Write};
        let Some(path) = self.pmem_manifest_path() else {
            return Ok(());
        };
        // The resident map can be incomplete when automatic recovery is off.
        // Derive the durable live set from the log, including this append.
        let mut live = HashMap::<CacheKey, u64>::new();
        for line in std::io::BufReader::new(File::open(&path)?).lines() {
            let line = line?;
            if let Some(record) = CacheManifestRecord::decode_line(&line) {
                live.insert(record.key(), record.block_len);
            } else if let Some(key) = Self::decode_pmem_delete_line(&line) {
                live.remove(&key);
            } else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "invalid PMEM manifest record",
                )
                .into());
            }
        }
        let temporary = path.with_extension("compact.tmp");
        let mut retained = 0u64;
        {
            let mut output = std::io::BufWriter::new(File::create(&temporary)?);
            for (key, length) in &live {
                if let Some(block_path) = self.pmem_block_path_for_key(key) {
                    match fs::metadata(block_path) {
                        Ok(_) => {}
                        // Recovery already skips these entries; do not carry
                        // them into every future manifest rewrite and restart.
                        Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                        Err(err) => return Err(err.into()),
                    }
                }
                writeln!(output, "{}", manifest_line_for_key(key, *length))?;
                retained = retained.saturating_add(1);
            }
            output.flush()?;
            output.get_ref().sync_all()?;
        }
        self.reset_pmem_manifest_writer();
        fs::rename(&temporary, &path)?;
        sync_parent_dir(&path)?;
        self.pmem_manifest_compact_after.store(
            PMEM_MANIFEST_COMPACT_MIN.max(retained.saturating_mul(3)),
            Ordering::Relaxed,
        );
        Ok(())
    }

    fn reset_pmem_manifest_writer(&self) {
        #[cfg(unix)]
        {
            self.pmem_manifest_writer
                .lock()
                .expect("manifest writer poisoned")
                .take();
        }
    }
}

#[cfg(all(test, unix))]
mod manifest_writer_tests {
    use super::*;

    // rust-internal: cached descriptor identity and reopening behavior.
    #[test]
    fn pmem_manifest_writer_follows_replacement_and_deletion() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest");
        let old_path = dir.path().join("old-manifest");
        let mut writer = None;
        PmemManifestWriter::append(&mut writer, &path, b"first\n").unwrap();
        PmemManifestWriter::append(&mut writer, &path, b"second\n").unwrap();
        fs::rename(&path, &old_path).unwrap();
        fs::write(&path, b"replacement\n").unwrap();
        PmemManifestWriter::append(&mut writer, &path, b"third\n").unwrap();
        assert_eq!(fs::read(&old_path).unwrap(), b"first\nsecond\n");
        assert_eq!(fs::read(&path).unwrap(), b"replacement\nthird\n");
        fs::remove_file(&path).unwrap();
        PmemManifestWriter::append(&mut writer, &path, b"fourth\n").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"fourth\n");
    }

    // rust-internal: invalid targets cannot reuse a previously valid descriptor.
    #[test]
    fn pmem_manifest_writer_discards_handle_on_open_failure() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest");
        let mut writer = None;
        PmemManifestWriter::append(&mut writer, &path, b"first\n").unwrap();
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(PmemManifestWriter::append(&mut writer, &path, b"bad\n").is_err());
        assert!(writer.is_none());
        fs::remove_dir(&path).unwrap();
        PmemManifestWriter::append(&mut writer, &path, b"retry\n").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"retry\n");
    }

    // rust-internal: lifecycle methods close the cached descriptor.
    #[test]
    fn pmem_manifest_writer_resets_with_cache_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_options(
            CacheOptions::new(64, 64, 0).with_pmem_paths([dir.path().join("pmem")]),
        );
        let key = CacheKey::string(0, "key");
        let initial_fixed_bytes = cache.footprint().fixed_bytes;
        cache
            .inner
            .read()
            .unwrap()
            .persist_pmem_put(&key, b"value")
            .unwrap();
        assert!(cache
            .inner
            .read()
            .unwrap()
            .pmem_manifest_writer
            .lock()
            .unwrap()
            .is_some());
        let path_bytes = cache
            .inner
            .read()
            .unwrap()
            .pmem_manifest_writer
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .path
            .capacity();
        assert_eq!(
            cache.footprint().fixed_bytes,
            initial_fixed_bytes + path_bytes
        );
        cache.stop();
        assert_eq!(cache.footprint().fixed_bytes, initial_fixed_bytes);
        assert!(cache
            .inner
            .read()
            .unwrap()
            .pmem_manifest_writer
            .lock()
            .unwrap()
            .is_none());
        cache.start().unwrap();
        cache
            .inner
            .read()
            .unwrap()
            .persist_pmem_put(&key, b"next")
            .unwrap();
        cache
            .inner
            .read()
            .unwrap()
            .clear_pmem_persistence()
            .unwrap();
        assert!(cache
            .inner
            .read()
            .unwrap()
            .pmem_manifest_writer
            .lock()
            .unwrap()
            .is_none());
        cache
            .inner
            .read()
            .unwrap()
            .persist_pmem_put(&key, b"last")
            .unwrap();
        cache.set_pmem_paths(vec![dir.path().join("other")]);
        assert!(cache
            .inner
            .read()
            .unwrap()
            .pmem_manifest_writer
            .lock()
            .unwrap()
            .is_none());
    }
}
