// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

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
