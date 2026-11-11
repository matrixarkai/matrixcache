// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

type RecordKeys = AHashMap<Arc<CacheKey>, ()>;
type RecordGroups = AHashMap<Arc<str>, RecordKeys>;
type RecordShards = AHashMap<ShardId, RecordGroups>;

/// A key-only snapshot and the amount of tier-key work used to produce it.
#[derive(Debug, Default)]
pub struct CacheRecordKeySnapshot {
    pub keys: Vec<CacheKey>,
    /// True when every participating cache used its configured record index.
    pub indexed: bool,
    /// Candidate keys visited, including duplicates across tiers or cache shards.
    pub entries_examined: usize,
}

#[derive(Debug, Default)]
struct RecordKeyIndex {
    namespaces: AHashMap<String, RecordShards>,
}

impl RecordKeyIndex {
    fn insert(&mut self, key: &Arc<CacheKey>) {
        let Some(shards) = self.namespaces.get_mut(key.namespace.as_ref()) else {
            return;
        };
        shards
            .entry(key.shard_id)
            .or_default()
            .entry(Arc::clone(&key.record_key))
            .or_default()
            .insert(Arc::clone(key), ());
    }

    fn remove(&mut self, key: &CacheKey) {
        let Some(shards) = self.namespaces.get_mut(key.namespace.as_ref()) else {
            return;
        };
        let Some(records) = shards.get_mut(&key.shard_id) else {
            return;
        };
        if let Some(keys) = records.get_mut(key.record_key.as_ref()) {
            keys.remove(key);
            if keys.is_empty() {
                records.remove(key.record_key.as_ref());
            } else if keys.capacity() > keys.len().saturating_mul(4).max(16) {
                keys.shrink_to_fit();
            }
        }
        if records.is_empty() {
            shards.remove(&key.shard_id);
        } else if records.capacity() > records.len().saturating_mul(4).max(16) {
            records.shrink_to_fit();
        }
        if shards.is_empty() {
            *shards = RecordShards::default();
        } else if shards.capacity() > shards.len().saturating_mul(4).max(16) {
            shards.shrink_to_fit();
        }
    }

    fn clear_entries(&mut self) {
        for shards in self.namespaces.values_mut() {
            *shards = RecordShards::default();
        }
    }

    fn keys(&self) -> impl Iterator<Item = &Arc<CacheKey>> {
        self.namespaces
            .values()
            .flat_map(|shards| shards.values())
            .flat_map(|records| records.values())
            .flat_map(|keys| keys.keys())
    }

    fn record_names(&self) -> impl Iterator<Item = &Arc<str>> {
        self.namespaces
            .values()
            .flat_map(|shards| shards.values())
            .flat_map(|records| records.keys())
    }

    fn table_bytes(&self) -> usize {
        map_table_bytes(&self.namespaces)
            + self
                .namespaces
                .values()
                .map(|shards| {
                    map_table_bytes(shards)
                        + shards
                            .values()
                            .map(|records| {
                                map_table_bytes(records)
                                    + records
                                        .values()
                                        .map(|keys| map_table_bytes(keys))
                                        .sum::<usize>()
                            })
                            .sum::<usize>()
                })
                .sum::<usize>()
    }
}

impl CacheInner {
    fn forget_record_key_if_absent(&mut self, key: &CacheKey) {
        if !self
            .record_key_index
            .namespaces
            .contains_key(key.namespace.as_ref())
        {
            return;
        }
        if !self.memory.contains_key(key)
            && !self.pmem.contains_key(key)
            && !self.disk_index.contains_key(key)
        {
            self.record_key_index.remove(key);
        }
    }

    fn insert_disk_index(&mut self, key: Arc<CacheKey>, bytes: u64) -> Option<u64> {
        self.record_key_index.insert(&key);
        self.disk_index.insert(key, bytes)
    }

    fn remove_disk_index(&mut self, key: &CacheKey) -> Option<u64> {
        let bytes = self.disk_index.remove(key);
        self.forget_record_key_if_absent(key);
        bytes
    }

    fn rebuild_record_key_index(&mut self) {
        if self.record_key_index.namespaces.is_empty() {
            return;
        }
        self.record_key_index.clear_entries();
        for key in self
            .memory
            .keys()
            .chain(self.pmem.keys())
            .chain(self.disk_index.keys())
        {
            self.record_key_index.insert(key);
        }
    }
}

impl MultiLayerCache {
    /// Enable record-scoped lookup for one namespace, backfilling existing tier keys.
    /// Registration is retained across clears and recovery. Other namespaces incur
    /// no per-entry index allocation. Repeated registration does no work.
    pub fn enable_record_key_index(&self, namespace: &str) {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        if inner.record_key_index.namespaces.contains_key(namespace) {
            return;
        }
        inner
            .record_key_index
            .namespaces
            .insert(namespace.to_owned(), RecordShards::default());
        let CacheInner {
            memory,
            pmem,
            disk_index,
            record_key_index,
            ..
        } = &mut *inner;
        for key in memory.keys().chain(pmem.keys()).chain(disk_index.keys()) {
            if key.namespace == namespace {
                record_key_index.insert(key);
            }
        }
    }

    /// Snapshot selectors for the requested records without accessing values or files.
    /// Unregistered namespaces retain the full tier-key scan behavior. Duplicate
    /// record requests and tier copies produce one sorted key in the result.
    pub fn keys_for_records(
        &self,
        shard_id: ShardId,
        namespace: &str,
        records: &[&str],
    ) -> CacheRecordKeySnapshot {
        let inner = self.inner.read().expect("cache lock poisoned");
        let indexed = inner.record_key_index.namespaces.get(namespace);
        let mut result = CacheRecordKeySnapshot {
            indexed: indexed.is_some(),
            ..Default::default()
        };
        if records.is_empty() {
            return result;
        }
        let requested = records.iter().copied().collect::<BTreeSet<_>>();
        let mut selected = BTreeSet::new();
        if let Some(shards) = indexed {
            if let Some(groups) = shards.get(&shard_id) {
                for record in requested {
                    if let Some(keys) = groups.get(record) {
                        for key in keys.keys() {
                            result.entries_examined += 1;
                            selected.insert(key.as_ref());
                        }
                    }
                }
            }
        } else {
            for key in inner
                .memory
                .keys()
                .chain(inner.pmem.keys())
                .chain(inner.disk_index.keys())
            {
                result.entries_examined += 1;
                if key.shard_id == shard_id
                    && key.namespace == namespace
                    && requested.contains(key.record_key.as_ref())
                {
                    selected.insert(key.as_ref());
                }
            }
        }
        result.keys = selected.into_iter().cloned().collect();
        result
    }
}

impl ShardedMultiLayerCache {
    pub fn enable_record_key_index(&self, namespace: &str) {
        for shard in self.shards.iter() {
            shard.enable_record_key_index(namespace);
        }
    }

    /// Cache shards are visited sequentially, without a cache-wide atomic snapshot.
    pub fn keys_for_records(
        &self,
        shard_id: ShardId,
        namespace: &str,
        records: &[&str],
    ) -> CacheRecordKeySnapshot {
        let mut result = CacheRecordKeySnapshot {
            indexed: true,
            ..Default::default()
        };
        let mut keys = BTreeSet::new();
        for shard in self.shards.iter() {
            let snapshot = shard.keys_for_records(shard_id, namespace, records);
            result.indexed &= snapshot.indexed;
            result.entries_examined += snapshot.entries_examined;
            keys.extend(snapshot.keys);
        }
        result.keys = keys.into_iter().collect();
        result
    }
}
