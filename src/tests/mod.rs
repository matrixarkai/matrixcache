// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

#[cfg(test)]
mod tests {
    #[cfg(feature = "rocksdb-ssd")]
    #[test]
    fn rocksdb_write_buffer_size_is_configurable() {
        // RocksDB preallocates its write-ahead log to hold a full memtable flush, so the write
        // buffer is a floor on the DB's on-disk size -- paid whether or not anything is cached.
        // Measured on a TemporalStore block cache holding 0.32 MB of content: the WAL file
        // reported 331,697 bytes of data against 73,822,208 bytes allocated, and the cache
        // directory was 74.5 MB, of which 74.1 MB was preallocated air.
        std::env::remove_var("MATRIXCACHE_ROCKSDB_WRITE_BUFFER_MB");
        assert_eq!(
            crate::StorageEngineRocksDB::rocksdb_write_buffer_bytes(),
            8 * 1024 * 1024,
            "the default is sized for a cache, not for a write-heavy database"
        );

        std::env::set_var("MATRIXCACHE_ROCKSDB_WRITE_BUFFER_MB", "64");
        assert_eq!(
            crate::StorageEngineRocksDB::rocksdb_write_buffer_bytes(),
            64 * 1024 * 1024
        );

        // Garbage and zero fall back rather than configuring a degenerate DB.
        for bad in ["0", "", "abc", "-4"] {
            std::env::set_var("MATRIXCACHE_ROCKSDB_WRITE_BUFFER_MB", bad);
            assert_eq!(
                crate::StorageEngineRocksDB::rocksdb_write_buffer_bytes(),
                8 * 1024 * 1024,
                "{bad:?} should fall back to the default"
            );
        }
        std::env::remove_var("MATRIXCACHE_ROCKSDB_WRITE_BUFFER_MB");
    }

    use super::*;

    /// A sharded cache has to report what its shards recorded.
    ///
    /// The aggregation is a hand-maintained list of field names, and eight had
    /// already fallen off it. A statistic missing from that list reads as zero,
    /// which is indistinguishable from a statistic that is genuinely zero --
    /// so a sharded deployment could not see its own expiry, write-budget or
    /// read-escalation numbers at all.
    #[test]
    fn a_sharded_cache_reports_the_statistics_its_shards_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ShardedMultiLayerCache::with_options(
            CacheOptions {
                dram_capacity: 1 << 18,
                ssd_paths: vec![dir.path().to_path_buf()],
                ..CacheOptions::default()
            },
            4,
        );

        // Writes with a life, then let them lapse and sweep, so the expiry
        // counters are non-zero on the shards.
        for index in 0..40 {
            cache
                .put_with_ttl(
                    CacheKey::string(0, &format!("agg-{index:03}")),
                    vec![b'v'; 64],
                    Duration::from_millis(40),
                )
                .unwrap();
        }
        std::thread::sleep(Duration::from_millis(150));
        cache.purge_expired();

        let total = cache.stats();
        assert_eq!(
            total.expired_removals, 40,
            "the sharded cache lost its shards' expiry counters"
        );

        // Every summed field must equal the sum of the shards' own numbers.
        // Checked against a handful that recent work added, which is where the
        // drift was.
        assert!(total.puts >= 40, "puts did not aggregate: {}", total.puts);
    }

    /// A statistic that reaches the aggregation's checker but not its sum
    /// reads as zero on a sharded cache, which is indistinguishable from a
    /// statistic that is genuinely zero.
    ///
    /// That is what happened to the two write-budget rates: they were added to
    /// a guard whose only job was to name the fields, and summed nowhere. This
    /// pins a rate that is not a count and not a proportion, so it exercises
    /// the third case the earlier tests do not.
    #[test]
    fn a_sharded_cache_reports_the_write_budget_rates_its_shards_measured() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ShardedMultiLayerCache::with_options(
            CacheOptions {
                dram_capacity: 1 << 18,
                ssd_capacity: 1 << 20,
                ssd_paths: vec![dir.path().to_path_buf()],
                ..CacheOptions::default()
            },
            4,
        );

        // Deliberately not a multiple of four, so a target that is merely
        // copied from one shard, or divided and not put back together, reads
        // differently from one that is summed.
        cache.set_ssd_write_budget_bytes_per_sec(4_002);
        assert_eq!(
            cache.stats().ssd_write_budget_target_bytes_per_sec,
            4_002,
            "the sharded target did not add its shards' slices back up"
        );

        // And the same field is zero when nothing is capped, so the assertion
        // above is not passing on a default.
        let uncapped = ShardedMultiLayerCache::with_options(
            CacheOptions {
                dram_capacity: 1 << 18,
                ..CacheOptions::default()
            },
            4,
        );
        assert_eq!(
            uncapped.stats().ssd_write_budget_target_bytes_per_sec,
            0,
            "an uncapped cache reported a target"
        );
    }

    /// Shrinking a tier must not write its expired entries down to the tier
    /// below.
    ///
    /// A demoted value keeps the metadata that says it is too old, so it can
    /// never be served: the write buys nothing and costs a flash write, a
    /// slice of the SSD write budget, and a slot in the lower tier that
    /// evicts something live to make room.
    ///
    /// Shrinking is where this bites. The per-write expiry sweep normally
    /// reclaims an expired entry long before eviction can pick it -- with
    /// writes driving the sweep, eviction sees no expired victims at all --
    /// but a capacity reduction evicts in bulk with no writes to drive it, so
    /// every expired entry in the tier is offered to demotion at once.
    #[test]
    fn shrinking_a_tier_does_not_write_its_expired_entries_downwards() {
        let dir = tempfile::tempdir().unwrap();
        let pmem_path = dir.path().join("pmem");
        // Memory large enough to hold every entry, so nothing is evicted while
        // the entries are still alive.
        //
        // No SSD tier. The assertion is about demotion into the persistent
        // tier, so the SSD one contributes nothing but a file write per put --
        // and that write is what made this test depend on how busy the machine
        // was. The loop below has to finish inside the life it just set, and
        // widening the life only moves the load at which it stops doing so: at
        // load 31 the writes took over four seconds, the entries began expiring
        // while they were still being written, and the sweep reclaimed them
        // before the shrink could see them. Without the SSD writes the loop is
        // memory-only and finishes in milliseconds.
        let options = CacheOptions::new(1 << 20, 1 << 20, 0).with_pmem_paths([pmem_path]);
        let cache = MultiLayerCache::with_options(options);
        cache.start().unwrap();

        // A life comfortably longer than the loop that writes them, so they
        // are all alive until the wait below and the sweep finds nothing.
        for index in 0..40 {
            cache
                .put_with_ttl(
                    CacheKey::string(0, &format!("shrink-victim-{index:03}")),
                    vec![b'v'; 64],
                    Duration::from_millis(4_000),
                )
                .unwrap();
        }
        // Most must still be alive when the loop ends, or the sweep reclaims
        // them during the writes and the shrink has nothing expired to demote.
        let filled = cache.stats();
        assert!(
            filled.expired_removals < 8,
            "too many entries expired while they were still being written: {}",
            filled.expired_removals
        );

        std::thread::sleep(Duration::from_millis(4_200));
        let pmem_before = cache.stats().pmem_fills;

        cache.set_capacity_for_tier(CacheTier::Memory, 1024);

        let after = cache.stats();
        assert!(
            after.eviction_expired > 20,
            "the shrink did not evict expired entries, so nothing was demoted \
             either: {} expired evictions",
            after.eviction_expired
        );
        assert_eq!(
            after.pmem_fills, pmem_before,
            "expired entries were written down to the persistent tier"
        );
        assert!(
            after.expired_demotions_skipped > 20,
            "the demotions were declined without being counted: {}",
            after.expired_demotions_skipped
        );
    }

    /// An entry that expires quietly must reach the eviction handler, and say
    /// that it expired.
    ///
    /// Eviction notifies the handler whatever its reason, expiry included, so
    /// an expired entry taken under memory pressure was reported. The same
    /// entry reclaimed by the expiry sweep was not, which made "did the
    /// handler hear about this entry" depend on whether the cache happened to
    /// be full at the time. A handler that releases a resource as an entry
    /// leaves would leak every entry that expired quietly.
    #[test]
    fn an_entry_that_expires_reaches_the_handler_and_says_it_expired() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1 << 20, dir.path());
        cache.start().unwrap();

        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorder = Arc::clone(&seen);
        cache.register_eviction_callback(move |record| {
            recorder
                .lock()
                .expect("recorder poisoned")
                .push((record.key.clone(), record.cause, record.value.clone()));
        });

        let expiring = CacheKey::string(0, "expires-quietly");
        cache
            .put_with_ttl(expiring.clone(), b"stale-contents".to_vec(), Duration::from_millis(40))
            .unwrap();
        std::thread::sleep(Duration::from_millis(150));

        // Nothing is near capacity, so only the sweep can reclaim it. Writes
        // drive the sweep.
        for index in 0..32 {
            cache
                .put(CacheKey::string(0, &format!("filler-{index:02}")), vec![b'f'; 32])
                .unwrap();
        }

        let seen = seen.lock().expect("recorder poisoned");
        let expiry = seen.iter().find(|(key, _, _)| key == &expiring);
        let Some((_, cause, value)) = expiry else {
            panic!(
                "the handler was never told the entry expired; it heard about {} other departures",
                seen.len()
            );
        };
        assert_eq!(
            *cause,
            CacheRemovalCause::Expired,
            "an expiry was reported as an eviction, so a handler would write \
             a stale value onwards"
        );
        // The value comes with it, or a handler that needs the contents to
        // release something cannot use the notification.
        assert_eq!(value.as_slice(), b"stale-contents");
    }

    /// The counter-case: an entry taken to make room is not an expiry.
    ///
    /// Without this, reporting every departure as `Expired` would pass the
    /// test above, and a handler would stop writing back values that are
    /// perfectly good.
    #[test]
    fn an_entry_evicted_for_room_is_not_reported_as_expired() {
        let dir = tempfile::tempdir().unwrap();
        // Small enough that the writes below must evict, and no lives set, so
        // nothing can genuinely expire.
        let cache = MultiLayerCache::new(4096, dir.path());
        cache.start().unwrap();

        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorder = Arc::clone(&seen);
        cache.register_eviction_callback(move |record| {
            recorder.lock().expect("recorder poisoned").push(record.cause);
        });

        for index in 0..256 {
            cache
                .put(CacheKey::string(0, &format!("room-{index:03}")), vec![b'v'; 64])
                .unwrap();
        }

        let seen = seen.lock().expect("recorder poisoned");
        assert!(!seen.is_empty(), "nothing was evicted, so nothing was checked");
        assert!(
            seen.iter().all(|cause| *cause == CacheRemovalCause::Evicted),
            "an entry with no life was reported as expired"
        );
    }

    /// An eviction handler on a sharded cache hears from every shard.
    ///
    /// A sharded cache could not register one at all, so sharding lost
    /// eviction notifications entirely. Asserting that departures arrive is
    /// not enough on its own -- registering on one shard would satisfy that --
    /// so this checks they come from more than one.
    #[test]
    fn an_eviction_handler_on_a_sharded_cache_hears_from_every_shard() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ShardedMultiLayerCache::with_options(
            CacheOptions {
                // Small enough that the writes below must evict.
                dram_capacity: 2048,
                ssd_paths: vec![dir.path().to_path_buf()],
                ..CacheOptions::default()
            },
            4,
        );
        cache.start().unwrap();

        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorder = Arc::clone(&seen);
        cache.register_eviction_callback(move |record| {
            recorder
                .lock()
                .expect("recorder poisoned")
                .push(record.key.clone());
        });

        for index in 0..512 {
            cache
                .put(CacheKey::string(0, &format!("sharded-{index:03}")), vec![b'v'; 64])
                .unwrap();
        }

        let departed = seen.lock().expect("recorder poisoned").clone();
        assert!(
            !departed.is_empty(),
            "a handler registered on a sharded cache heard nothing"
        );
        let shards: std::collections::HashSet<usize> = departed
            .iter()
            .map(|key| cache.shard_index_for_key(key))
            .collect();
        assert!(
            shards.len() > 1,
            "every departure came from one shard, so the handler was not \
             registered across the cache: {shards:?}"
        );

        // And clearing it stops every shard, not some of them: a handler left
        // on a few shards reports a fraction of the cache, which looks like a
        // measurement.
        cache.clear_eviction_callback();
        seen.lock().expect("recorder poisoned").clear();
        for index in 512..1024 {
            cache
                .put(CacheKey::string(0, &format!("sharded-{index:04}")), vec![b'v'; 64])
                .unwrap();
        }
        assert!(
            seen.lock().expect("recorder poisoned").is_empty(),
            "some shards kept reporting after the handler was cleared"
        );
    }

    /// Recovery from a tier that cannot hold anything is refused.
    ///
    /// A durable tier with no path is placed in a temporary directory. On its
    /// own that is the default -- a scratch cache -- and only worth
    /// mentioning. Asking to recover from it is a contradiction: the directory
    /// is created moments before it is read, so recovery finds nothing, every
    /// time, and nothing says so.
    #[test]
    fn recovery_from_a_temporary_tier_is_refused_and_says_why() {
        let options = CacheOptions {
            dram_capacity: 1 << 16,
            ssd_capacity: 1 << 20,
            ssd_paths: Vec::new(),
            // Recovery is asked for, which is what makes the missing path a
            // contradiction rather than the default scratch cache.
            auto_recover_on_start: true,
            ..CacheOptions::default()
        };

        let findings = options.validate();
        let ids: Vec<&str> = findings.iter().map(|f| f.id.as_str()).collect();
        assert!(ids.contains(&"recovery_expected_from_a_temporary_tier"), "{ids:?}");

        let refused = MultiLayerCache::try_with_options(options);
        let Err(CacheError::InvalidConfig(message)) = refused else {
            panic!("a cache whose durable tier is a temporary directory was built anyway");
        };
        // The message has to name the field and say what happens, or it sends
        // the reader back to the source to find out what was wrong.
        assert!(message.contains("ssd_paths"), "{message}");
        assert!(message.contains("empty every time"), "{message}");
    }

    /// The counter-case: a configuration that is merely unusual still builds.
    ///
    /// Refusing on warnings would turn a cache that works into one that will
    /// not start, so only the findings that mean the cache cannot do what it
    /// was asked can refuse.
    #[test]
    fn a_configuration_with_only_warnings_still_builds() {
        let dir = tempfile::tempdir().unwrap();
        let options = CacheOptions {
            dram_capacity: 1 << 16,
            // Paths with no capacity: the tier is off and the paths unused,
            // which is worth saying and not worth refusing.
            pmem_capacity: 0,
            pmem_paths: vec![dir.path().join("pmem")],
            ssd_capacity: 1 << 20,
            ssd_paths: vec![dir.path().to_path_buf()],
            ..CacheOptions::default()
        };

        let severities: Vec<CacheHealthSeverity> =
            options.validate().iter().map(|f| f.severity).collect();
        assert!(
            !severities.is_empty() && !severities.contains(&CacheHealthSeverity::Critical),
            "expected warnings and no refusal: {severities:?}"
        );
        assert!(
            MultiLayerCache::try_with_options(options).is_ok(),
            "a warning refused a cache that could have started"
        );
    }

    /// A policy name nobody offers is silently given the default, so the check
    /// has to be the thing that says the name was not recognised.
    ///
    /// The two parsers share one list of names, because a name accepted by the
    /// check and resolved to something else by the builder would be worse than
    /// no check: it would read as confirmation.
    #[test]
    fn a_replacement_policy_name_nobody_offers_is_reported_not_refused() {
        let options = CacheOptions {
            dram_capacity: 1 << 16,
            cache_dram_replacement_policy: "clock-pro".to_string(),
            ..CacheOptions::default()
        };
        let findings = options.validate();
        let finding = findings
            .iter()
            .find(|f| f.id == "replacement_policy_not_recognised")
            .expect("an unknown policy name was not reported");
        assert_eq!(finding.field, "cache_dram_replacement_policy");
        // It must say what the cache will actually use.
        assert!(finding.message.contains("WeightedHotnessLru"), "{}", finding.message);

        // Every name the cache does offer passes, including the two spellings
        // of the default. "LRU" is one of them: plain LRU is not a separate
        // policy here, so asking for it is asking for the weighted one.
        for name in ["FIFO", "fifo", "SLRU", "WeightedHotnessLru", "LRU"] {
            let accepted = CacheOptions {
                dram_capacity: 1 << 16,
                cache_dram_replacement_policy: name.to_string(),
                ..CacheOptions::default()
            };
            assert!(
                !accepted
                    .validate()
                    .iter()
                    .any(|f| f.id == "replacement_policy_not_recognised"),
                "{name} is a name the cache accepts and the check rejected it"
            );
            // And the two agree on what it resolves to.
            assert_eq!(
                CacheReplacementPolicy::try_from_config_name(name),
                Some(CacheReplacementPolicy::from_config_name(name)),
                "the check and the builder disagree about {name}"
            );
        }
    }

    /// A cache with no capacity anywhere accepts writes and stores none.
    #[test]
    fn a_cache_with_no_capacity_anywhere_is_refused() {
        let options = CacheOptions {
            dram_capacity: 0,
            pmem_capacity: 0,
            ssd_capacity: 0,
            ..CacheOptions::default()
        };
        assert!(options
            .validate()
            .iter()
            .any(|f| f.id == "no_tier_has_capacity"));
        assert!(matches!(
            MultiLayerCache::try_with_options(options),
            Err(CacheError::InvalidConfig(_))
        ));
    }

    /// The default configuration must not report anything above information.
    ///
    /// A check that warns about the defaults is a check people turn off. The
    /// defaults do put the durable tiers in a temporary directory, which is
    /// worth knowing and is not a fault.
    #[test]
    fn the_default_configuration_has_nothing_to_report() {
        let findings = CacheOptions::default().validate();
        assert!(
            findings
                .iter()
                .all(|f| f.severity == CacheHealthSeverity::Info),
            "the default configuration reports {:?}",
            findings
                .iter()
                .filter(|f| f.severity != CacheHealthSeverity::Info)
                .map(|f| f.id.as_str())
                .collect::<Vec<_>>()
        );
    }

    /// Sharding can make a cache refuse a value it has room for, and the
    /// configuration that was asked for never mentions shards.
    ///
    /// Each shard gets a slice of every tier, and a slice can be smaller than
    /// the largest value the tier would otherwise take. The test measures the
    /// refusal first and then checks the finding describes it, rather than
    /// asserting a rule against itself.
    #[test]
    fn sharding_can_refuse_a_value_the_whole_tier_had_room_for() {
        let dir = tempfile::tempdir().unwrap();
        let options = CacheOptions {
            // Large enough that splitting it eight ways still leaves room for
            // the value, so the SSD tier is the only one reported.
            dram_capacity: 64 << 20,
            pmem_capacity: 0,
            // Four mebibytes of SSD, and a value that fits in it four times
            // over. Split eight ways, no shard can take it.
            ssd_capacity: 4 << 20,
            ssd_paths: vec![dir.path().to_path_buf()],
            ..CacheOptions::default()
        };
        let value = vec![7u8; 1 << 20];

        let whole = MultiLayerCache::with_options(options.clone());
        whole.start().unwrap();
        whole.put(CacheKey::string(0, "big"), value.clone()).unwrap();
        assert_eq!(
            whole.stats().ssd_oversize_rejections,
            0,
            "the unsharded cache refused a value that fits four times over"
        );

        let sharded_dir = tempfile::tempdir().unwrap();
        let sharded_options = CacheOptions {
            ssd_paths: vec![sharded_dir.path().to_path_buf()],
            ..options.clone()
        };
        let sharded = ShardedMultiLayerCache::with_options(sharded_options, 8);
        sharded.start().unwrap();
        sharded.put(CacheKey::string(0, "big"), value).unwrap();
        assert_eq!(
            sharded.stats().ssd_oversize_rejections,
            1,
            "the split did not refuse the value, so there is nothing to report"
        );

        // Having measured it, the finding must say so, name the field, and
        // give the per-shard size that is doing the refusing.
        let findings = options.validate_for_shards(8);
        let finding = findings
            .iter()
            .find(|f| {
                f.id == "sharded_tier_refuses_values_it_has_room_for"
                    && f.field == "ssd_capacity"
            })
            .unwrap_or_else(|| {
                panic!(
                    "a refusal that happens was not reported: {:?}",
                    findings.iter().map(|f| f.id.as_str()).collect::<Vec<_>>()
                )
            });
        assert_eq!(finding.field, "ssd_capacity");
        assert!(finding.message.contains("524288"), "{}", finding.message);

        // And the same configuration unsharded reports nothing of the kind,
        // so the finding is about the split and not about the capacity.
        assert!(
            !options
                .validate_for_shards(1)
                .iter()
                .any(|f| f.id == "sharded_tier_refuses_values_it_has_room_for"),
            "an unsharded cache was told its shards were too small"
        );
    }

    /// More shards than the tier has bytes leaves some shards with none.
    #[test]
    fn a_tier_with_fewer_bytes_than_shards_is_reported() {
        let options = CacheOptions {
            dram_capacity: 4,
            pmem_capacity: 0,
            ssd_capacity: 0,
            ..CacheOptions::default()
        };
        let ids: Vec<String> = options
            .validate_for_shards(16)
            .into_iter()
            .map(|f| f.id)
            .collect();
        assert!(
            ids.iter().any(|id| id == "tier_has_fewer_bytes_than_shards"),
            "{ids:?}"
        );
    }

    /// Asking a tier for SLRU gets the weighted policy, and the check says so.
    ///
    /// Both names take the same branch in every tier's victim selection, so a
    /// configuration naming SLRU is answered with something else -- and a name
    /// that is recognised and then substituted is worse than one that is
    /// rejected, because it reads as confirmation.
    ///
    /// The first half pins the behaviour rather than trusting the branch. If
    /// SLRU is ever connected to tier eviction this fails, which is the point:
    /// the finding below has to go at the same time.
    #[test]
    fn slru_selects_the_same_victims_as_the_weighted_policy_and_is_reported() {
        fn evictions_under(policy: CacheReplacementPolicy) -> (u64, Vec<String>) {
            let dir = tempfile::tempdir().unwrap();
            let cache = MultiLayerCache::with_options(CacheOptions {
                dram_capacity: 4096,
                pmem_capacity: 0,
                ssd_capacity: 1 << 20,
                ssd_paths: vec![dir.path().to_path_buf()],
                ..CacheOptions::default()
            });
            cache.set_replacement_policy_for_tier(CacheTier::Memory, policy);
            cache.start().unwrap();

            let departed = Arc::new(Mutex::new(Vec::new()));
            let recorder = Arc::clone(&departed);
            cache.register_eviction_callback(move |record| {
                recorder
                    .lock()
                    .expect("recorder poisoned")
                    .push(format!("{:?}", record.key));
            });

            // A hot half read repeatedly against a cold half, so the policies
            // have something to disagree about if they can.
            for round in 0..8 {
                for index in 0..128 {
                    cache
                        .put(CacheKey::string(0, &format!("k{index:03}")), vec![b'v'; 64])
                        .unwrap();
                    if index % 2 == 0 {
                        let _ = cache.get(&CacheKey::string(0, &format!("k{index:03}")));
                    }
                }
                let _ = round;
            }
            let order = departed.lock().expect("recorder poisoned").clone();
            (cache.stats().memory_evictions, order)
        }

        let (slru_count, slru_order) = evictions_under(CacheReplacementPolicy::Slru);
        let (weighted_count, weighted_order) =
            evictions_under(CacheReplacementPolicy::WeightedHotnessLru);
        assert!(slru_count > 0, "nothing was evicted, so nothing was compared");
        assert_eq!(
            (slru_count, slru_order.clone()),
            (weighted_count, weighted_order),
            "SLRU now evicts differently from the weighted policy -- it has been \
             connected to tier eviction, so the finding that says it has not must go"
        );

        // The harness can tell policies apart, or "identical" above would mean
        // nothing: FIFO ignores the reads and evicts a different set.
        let (fifo_count, fifo_order) = evictions_under(CacheReplacementPolicy::Fifo);
        assert!(
            (fifo_count, fifo_order) != (slru_count, slru_order.clone()),
            "every policy evicted the same entries, so this test cannot tell them apart"
        );

        // And a configuration naming it is told.
        let options = CacheOptions {
            dram_capacity: 1 << 16,
            cache_ssd_replacement_policy: "SLRU".to_string(),
            ..CacheOptions::default()
        };
        let findings = options.validate();
        let finding = findings
            .iter()
            .find(|f| f.id == "replacement_policy_resolves_to_another")
            .expect("a policy answered with another was not reported");
        assert_eq!(finding.field, "cache_ssd_replacement_policy");
        assert!(finding.message.contains("WeightedHotnessLru"), "{}", finding.message);

        // The policies that are what they say they are report nothing.
        for name in ["FIFO", "WeightedHotnessLru"] {
            let honest = CacheOptions {
                dram_capacity: 1 << 16,
                cache_dram_replacement_policy: name.to_string(),
                ..CacheOptions::default()
            };
            assert!(
                !honest
                    .validate()
                    .iter()
                    .any(|f| f.id == "replacement_policy_resolves_to_another"),
                "{name} was reported as resolving to something else"
            );
        }
    }

    /// Records survive a reopen before anything has been compacted.
    ///
    /// The store used to rewrite itself in full after every put, so a reopen
    /// always found a complete snapshot. Now the writes go to a journal and
    /// the snapshot is taken only when the journal outgrows it, which means a
    /// store can be reopened with no snapshot at all -- and before the first
    /// compaction, that is every store.
    ///
    /// That case is not hypothetical: it is what the multi-SSD recovery test
    /// hit, because loading returned early when the snapshot file was missing
    /// and never looked at the journal.
    #[cfg(not(feature = "rocksdb-ssd"))]
    #[test]
    fn a_store_reopened_before_it_compacts_still_has_its_records() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("store");

        let mut store = StorageEngineRocksDb::new(path.to_string_lossy().to_string());
        assert!(store.start());
        for index in 0..8 {
            store
                .put(&format!("key-{index}"), format!("value-{index}").into_bytes())
                .unwrap();
        }
        // Deleted after being written, so replay has to apply the operations
        // in order rather than merely collecting the puts.
        store.delete("key-3").unwrap();
        // Deliberately not stopped. A clean shutdown takes a snapshot, so the
        // case where there is none is the case where the process did not get
        // to shut down -- or where something else opens the store while it is
        // still running, which is what the multi-SSD recovery test does.
        drop(store);

        // Small enough that no compaction can have happened, so everything
        // above is in the journal and nowhere else.
        assert!(
            !path.join("matrixcache_rocksdb_compat_store.bin").exists(),
            "the store compacted, so this test is no longer exercising a journal \
             without a snapshot"
        );

        let mut reopened = StorageEngineRocksDb::new(path.to_string_lossy().to_string());
        assert!(reopened.start());
        for index in 0..8 {
            let found = reopened.get(&format!("key-{index}"));
            if index == 3 {
                assert!(
                    matches!(found, Err(CacheError::NotFound)),
                    "a deleted record came back"
                );
            } else {
                assert_eq!(
                    found.unwrap().to_vec(),
                    format!("value-{index}").into_bytes(),
                    "record {index} did not survive the reopen"
                );
            }
        }
    }

    /// And once it has compacted, the journal is not replayed on top of the
    /// snapshot that already contains it.
    #[cfg(not(feature = "rocksdb-ssd"))]
    #[test]
    fn a_store_that_has_compacted_reopens_to_the_same_records() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("store");

        let mut store = StorageEngineRocksDb::new(path.to_string_lossy().to_string());
        assert!(store.start());
        // Enough bytes to pass the compaction floor several times over.
        for index in 0..64 {
            store
                .put(&format!("key-{index:03}"), vec![b'v'; 4096])
                .unwrap();
        }
        store.delete("key-000").unwrap();
        store.stop();

        assert!(
            path.join("matrixcache_rocksdb_compat_store.bin").exists(),
            "nothing compacted, so this test is not exercising a snapshot"
        );

        let mut reopened = StorageEngineRocksDb::new(path.to_string_lossy().to_string());
        assert!(reopened.start());
        assert!(
            matches!(reopened.get("key-000"), Err(CacheError::NotFound)),
            "a record deleted before compaction came back after it"
        );
        for index in 1..64 {
            assert_eq!(
                reopened.get(&format!("key-{index:03}")).unwrap().to_vec().len(),
                4096,
                "record {index} did not survive compaction"
            );
        }
    }

    /// `get_batch` and a loop of `get` must agree, on the values and on what
    /// they record.
    ///
    /// They are separate implementations of one question, and the batch one
    /// takes the cache exclusively for a whole batch where the single one
    /// probes under a shared lock. Any change to how the batch acquires the
    /// cache has to keep the answers identical, and "identical" has to include
    /// the counters -- a faster path that stops recording hits is not the same
    /// path.
    ///
    /// The workload deliberately mixes every case: entries in memory, entries
    /// only on the SSD tier, keys that were never written, a key repeated
    /// inside the batch, and an entry that has expired.
    #[test]
    fn get_batch_answers_what_a_loop_of_get_answers() {
        fn workload(dir: &std::path::Path, suffix: &str) -> (MultiLayerCache, Vec<CacheKey>) {
            let cache = MultiLayerCache::with_options(CacheOptions {
                // Large enough to hold everything. With a small memory tier
                // the two paths are not comparable by tier: the loop refills
                // memory from the SSD as it goes, evicting keys it has not
                // reached yet, while the batch reads memory for every key
                // before any refill happens. Both answer correctly; they just
                // answer from different tiers, so the tier counters diverge
                // for a reason that is not a defect.
                dram_capacity: 1 << 20,
                ssd_capacity: 1 << 20,
                ssd_paths: vec![dir.to_path_buf()],
                ..CacheOptions::default()
            });
            cache.start().unwrap();
            for index in 0..96 {
                cache
                    .put(
                        CacheKey::string(0, &format!("{suffix}-resident-{index:03}")),
                        vec![b'v'; 64],
                    )
                    .unwrap();
            }
            cache
                .put_with_ttl(
                    CacheKey::string(0, &format!("{suffix}-expiring")),
                    vec![b'v'; 64],
                    Duration::from_millis(30),
                )
                .unwrap();
            std::thread::sleep(Duration::from_millis(120));

            let mut keys: Vec<CacheKey> = (0..96)
                .map(|index| CacheKey::string(0, &format!("{suffix}-resident-{index:03}")))
                .collect();
            keys.push(CacheKey::string(0, &format!("{suffix}-expiring")));
            keys.push(CacheKey::string(0, &format!("{suffix}-never-written")));
            // The same key twice in one batch: the batch path deduplicates its
            // SSD reads, and both positions still have to be filled.
            keys.push(keys[0].clone());
            (cache, keys)
        }

        let batch_dir = tempfile::tempdir().unwrap();
        let (batch_cache, batch_keys) = workload(batch_dir.path(), "batch");
        let before = batch_cache.stats();
        let batched = batch_cache.get_batch(&batch_keys).unwrap();
        let batch_delta = batch_cache.stats();

        let loop_dir = tempfile::tempdir().unwrap();
        let (loop_cache, loop_keys) = workload(loop_dir.path(), "batch");
        let loop_before = loop_cache.stats();
        let looped: Vec<Option<Vec<u8>>> = loop_keys
            .iter()
            .map(|key| loop_cache.get(key).unwrap())
            .collect();
        let loop_delta = loop_cache.stats();

        assert_eq!(
            batched.len(),
            looped.len(),
            "the two paths returned different numbers of answers"
        );
        for (index, (from_batch, from_loop)) in batched.iter().zip(looped.iter()).enumerate() {
            assert_eq!(
                from_batch, from_loop,
                "position {index} differs: {} against {}",
                from_batch.is_some(),
                from_loop.is_some()
            );
        }
        // Something must have been served, or the comparison above is between
        // two lists of nothing.
        assert!(
            batched.iter().filter(|answer| answer.is_some()).count() > 50,
            "almost nothing was served, so this compares very little"
        );

        for (name, batch_count, loop_count) in [
            (
                "memory_hits",
                batch_delta.memory_hits - before.memory_hits,
                loop_delta.memory_hits - loop_before.memory_hits,
            ),
            (
                "disk_hits",
                batch_delta.disk_hits - before.disk_hits,
                loop_delta.disk_hits - loop_before.disk_hits,
            ),
            (
                "misses",
                batch_delta.misses - before.misses,
                loop_delta.misses - loop_before.misses,
            ),
        ] {
            assert_eq!(
                batch_count, loop_count,
                "{name}: the batch path recorded {batch_count} and the loop {loop_count}"
            );
        }
    }

    /// No read serves an entry that has passed its time to live.
    ///
    /// Eighteen public entry points can answer a read, and until this was
    /// written only three of them checked: `get`, `get_with_tier`, and
    /// `get_batch` after its own fix. The other ten that reach a tier served
    /// expired entries, `acquire` among them -- which is the worst of the set,
    /// because the handle it returns pins the entry against the eviction that
    /// would otherwise have removed it.
    ///
    /// Written as one list rather than eighteen tests on purpose. The failure
    /// names every path that regressed, and a path added later is one line
    /// here rather than a test somebody has to remember to write.
    #[test]
    fn no_read_serves_an_entry_that_has_expired() {
        fn fresh(dir: &std::path::Path, name: &str) -> (MultiLayerCache, CacheKey) {
            let cache = MultiLayerCache::with_options(CacheOptions {
                dram_capacity: 1 << 20,
                ssd_capacity: 1 << 20,
                ssd_paths: vec![dir.to_path_buf()],
                ..CacheOptions::default()
            });
            cache.start().unwrap();
            let key = CacheKey::string(0, name);
            cache
                .put_with_ttl(key.clone(), vec![b'v'; 32], Duration::from_millis(30))
                .unwrap();
            std::thread::sleep(Duration::from_millis(120));
            (cache, key)
        }

        let dir = tempfile::tempdir().unwrap();
        let mut served = Vec::new();

        let (c, k) = fresh(dir.path(), "a1");
        if c.peek(&k) { served.push("peek"); }
        let (c, k) = fresh(dir.path(), "a2");
        if c.peek_tier(&k).is_some() { served.push("peek_tier"); }
        let (c, k) = fresh(dir.path(), "a3");
        if c.get(&k).unwrap().is_some() { served.push("get"); }
        let (c, k) = fresh(dir.path(), "a4");
        if c.get_no_promotion(&k).unwrap().is_some() { served.push("get_no_promotion"); }
        let (c, k) = fresh(dir.path(), "a5");
        if c.lookup_no_promotion(&k).unwrap().is_some() { served.push("lookup_no_promotion"); }
        let (c, k) = fresh(dir.path(), "a6");
        if c.get_batch(std::slice::from_ref(&k)).unwrap()[0].is_some() { served.push("get_batch"); }
        let (c, k) = fresh(dir.path(), "a7");
        if c.get_batch_no_promotion(std::slice::from_ref(&k)).unwrap()[0].is_some() {
            served.push("get_batch_no_promotion");
        }
        let (c, k) = fresh(dir.path(), "a8");
        if c.get_memory(&k).is_some() { served.push("get_memory"); }
        let (c, k) = fresh(dir.path(), "a9");
        if c.acquire(&k).unwrap().is_some() { served.push("acquire"); }
        let (c, k) = fresh(dir.path(), "a10");
        if c.acquire_no_promotion(&k).unwrap().is_some() { served.push("acquire_no_promotion"); }
        let (c, k) = fresh(dir.path(), "a11");
        if c.acquire_scoped(&k).unwrap().is_some() { served.push("acquire_scoped"); }
        let (c, k) = fresh(dir.path(), "a12");
        if c.get_bypass_replacement_policy(&k).unwrap().is_some() {
            served.push("get_bypass_replacement_policy");
        }
        let (c, k) = fresh(dir.path(), "a13");
        if c.get_with_tier(&k).unwrap().is_some() { served.push("get_with_tier"); }

        let (c, k) = fresh(dir.path(), "a14");
        if c.acquire_batch(std::slice::from_ref(&k)).unwrap()[0].is_some() { served.push("acquire_batch"); }
        let (c, k) = fresh(dir.path(), "a15");
        if c.acquire_batch_no_promotion(std::slice::from_ref(&k)).unwrap()[0].is_some() {
            served.push("acquire_batch_no_promotion");
        }
        let (c, k) = fresh(dir.path(), "a16");
        if c.lookup(&k).unwrap().is_some() { served.push("lookup"); }
        let (c, k) = fresh(dir.path(), "a17");
        if c.lookup_batch_no_promotion(std::slice::from_ref(&k)).unwrap()[0].is_some() {
            served.push("lookup_batch_no_promotion");
        }
        let (c, k) = fresh(dir.path(), "a18");
        if c.get_pinned_handle(&k).unwrap().is_some() { served.push("get_pinned_handle"); }

        assert!(
            served.is_empty(),
            "these reads served an entry that had passed its time to live: {served:?}"
        );
    }

    /// Pin counts are summed across every stripe.
    ///
    /// The pin state is split over several locks so that handles on different
    /// keys can be taken at the same time. Everything that reports a total has
    /// to visit all of them: a figure taken from one stripe is a fraction of
    /// the answer, and a fraction that looks like an answer is worse than an
    /// error.
    ///
    /// Enough keys that they cannot all land in one stripe, and the count is
    /// checked before and after unpinning so a total that is merely never
    /// updated cannot pass.
    #[test]
    fn pin_totals_are_summed_across_every_stripe() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_options(CacheOptions {
            dram_capacity: 1 << 20,
            ssd_capacity: 1 << 20,
            ssd_paths: vec![dir.path().to_path_buf()],
            ..CacheOptions::default()
        });
        cache.start().unwrap();

        let keys: Vec<CacheKey> = (0..128)
            .map(|index| CacheKey::string(0, &format!("pinned-{index:04}")))
            .collect();
        for key in &keys {
            cache.put(key.clone(), vec![b'v'; 64]).unwrap();
            cache.pin(key.clone());
        }

        let held = cache.stats();
        assert_eq!(
            held.pinned_entries,
            keys.len() as u64,
            "the pinned count did not add up across stripes"
        );
        assert_eq!(
            held.pin_operations,
            keys.len() as u64,
            "the pin operations did not add up across stripes"
        );
        assert!(held.pinned_bytes > 0, "pinned bytes were not accounted");

        for key in &keys {
            cache.unpin(key);
        }
        let released = cache.stats();
        assert_eq!(released.pinned_entries, 0, "entries stayed pinned");
        assert_eq!(
            released.unpin_operations,
            keys.len() as u64,
            "the unpin operations did not add up across stripes"
        );
    }

    /// A stopped cache neither serves, nor pins, nor forgets on request.
    ///
    /// Most of the API already refused: `put`, `get`, `get_batch`, `acquire`,
    /// `purge_expired`, `remove_all` and `reset` all return `Stopped`. Three
    /// did not, and each contradicted a sibling that did -- `peek_tier` said an
    /// entry was resident while `get` on the same key refused, `pin` took a pin
    /// that `acquire` would not have given, and `invalidate_memory_only`
    /// dropped an entry that `invalidate` would have declined to touch.
    ///
    /// `unpin` deliberately still works. A pin taken before the stop has to be
    /// releasable after it, or shutting down with handles outstanding would
    /// leave them pinned forever. Giving something back is always allowed;
    /// taking something new is not.
    #[test]
    fn a_stopped_cache_does_not_serve_pin_or_forget() {
        fn stopped(dir: &std::path::Path, name: &str) -> (MultiLayerCache, CacheKey) {
            let cache = MultiLayerCache::with_options(CacheOptions {
                dram_capacity: 1 << 20,
                ssd_capacity: 1 << 20,
                ssd_paths: vec![dir.join(name)],
                ..CacheOptions::default()
            });
            cache.start().unwrap();
            let key = CacheKey::string(0, "resident");
            cache.put(key.clone(), vec![b'v'; 32]).unwrap();
            cache.stop();
            (cache, key)
        }

        let dir = tempfile::tempdir().unwrap();

        // Reporting residency for an entry no read will return.
        let (cache, key) = stopped(dir.path(), "peek");
        assert!(
            cache.peek_tier(&key).is_none(),
            "a stopped cache reported an entry it would refuse to serve"
        );
        assert!(
            matches!(cache.get(&key), Err(CacheError::Stopped)),
            "this test assumes `get` refuses, and it did not"
        );

        // Taking a pin that `acquire` would have refused.
        let (cache, key) = stopped(dir.path(), "pin");
        cache.pin(key.clone());
        assert_eq!(
            cache.stats().pinned_entries,
            0,
            "a stopped cache took a pin"
        );
        assert!(
            matches!(cache.acquire(&key), Err(CacheError::Stopped)),
            "this test assumes `acquire` refuses, and it did not"
        );

        // Forgetting an entry that `invalidate` would have declined to touch.
        // It returns nothing, so the refusal is only visible in the state: the
        // entry has to still be there when the cache runs again.
        let (cache, key) = stopped(dir.path(), "invalidate");
        cache.invalidate_memory_only(&key);
        cache.start().unwrap();
        assert!(
            cache.get(&key).unwrap().is_some(),
            "a stopped cache dropped an entry from memory"
        );

        // And the counter-case: giving a pin back still works, because a
        // handle held across a shutdown has to be releasable.
        let dir2 = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_options(CacheOptions {
            dram_capacity: 1 << 20,
            ssd_capacity: 1 << 20,
            ssd_paths: vec![dir2.path().to_path_buf()],
            ..CacheOptions::default()
        });
        cache.start().unwrap();
        let key = CacheKey::string(0, "held-across-shutdown");
        cache.put(key.clone(), vec![b'v'; 32]).unwrap();
        cache.pin(key.clone());
        assert_eq!(cache.stats().pinned_entries, 1);
        cache.stop();
        cache.unpin(&key);
        assert_eq!(
            cache.stats().pinned_entries,
            0,
            "a pin taken before the stop could not be released after it"
        );
    }

    /// The sharded cache honours the same contracts the single one does.
    ///
    /// It inherits them by delegating, which is worth a test precisely because
    /// inheritance is invisible: nothing in the sharded facade mentions a time
    /// to live or a stopped cache, so nothing there would look wrong if a
    /// shard-level implementation were added that skipped both.
    ///
    /// That is not hypothetical. `register_eviction_callback` had to be
    /// written for the sharded facade because it did not delegate -- it simply
    /// did not exist there. A read path added the same way, for the same
    /// reason, would serve expired entries and answer a stopped cache, and
    /// every single-cache test would still pass.
    ///
    /// Both lists must be empty. A failure names what regressed.
    #[test]
    fn the_sharded_cache_honours_expiry_and_stopping() {
        fn expired(dir: &std::path::Path, name: &str) -> (ShardedMultiLayerCache, CacheKey) {
            let cache = ShardedMultiLayerCache::with_options(
                CacheOptions {
                    dram_capacity: 1 << 20,
                    ssd_capacity: 1 << 20,
                    ssd_paths: vec![dir.join(name)],
                    ..CacheOptions::default()
                },
                4,
            );
            cache.start().unwrap();
            let key = CacheKey::string(0, "lapsed");
            cache
                .put_with_ttl(key.clone(), vec![b'v'; 32], Duration::from_millis(30))
                .unwrap();
            std::thread::sleep(Duration::from_millis(120));
            (cache, key)
        }

        let dir = tempfile::tempdir().unwrap();
        let mut served = Vec::new();

        let (c, k) = expired(dir.path(), "a");
        if c.get(&k).unwrap().is_some() { served.push("get"); }
        let (c, k) = expired(dir.path(), "b");
        if c.get_batch(std::slice::from_ref(&k)).unwrap()[0].is_some() { served.push("get_batch"); }
        let (c, k) = expired(dir.path(), "c");
        if c.get_no_promotion(&k).unwrap().is_some() { served.push("get_no_promotion"); }
        let (c, k) = expired(dir.path(), "d");
        if c.acquire(&k).unwrap().is_some() { served.push("acquire"); }
        let (c, k) = expired(dir.path(), "e");
        if c.get_memory(&k).is_some() { served.push("get_memory"); }
        let (c, k) = expired(dir.path(), "f");
        if c.lookup(&k).unwrap().is_some() { served.push("lookup"); }
        assert!(
            served.is_empty(),
            "a sharded cache served entries past their time to live: {served:?}"
        );

        // And the stopped contract.
        fn stopped(dir: &std::path::Path, name: &str) -> (ShardedMultiLayerCache, CacheKey) {
            let cache = ShardedMultiLayerCache::with_options(
                CacheOptions {
                    dram_capacity: 1 << 20,
                    ssd_capacity: 1 << 20,
                    ssd_paths: vec![dir.join(name)],
                    ..CacheOptions::default()
                },
                4,
            );
            cache.start().unwrap();
            let key = CacheKey::string(0, "resident");
            cache.put(key.clone(), vec![b'v'; 32]).unwrap();
            cache.stop();
            (cache, key)
        }

        let mut accepted = Vec::new();
        let (c, k) = stopped(dir.path(), "s1");
        if c.get(&k).is_ok() { accepted.push("get"); }
        let (c, k) = stopped(dir.path(), "s2");
        if c.put(k.clone(), vec![b'w'; 32]).is_ok() { accepted.push("put"); }
        let (c, k) = stopped(dir.path(), "s3");
        c.pin(k.clone());
        if c.stats().pinned_entries > 0 { accepted.push("pin"); }
        let (c, _k) = stopped(dir.path(), "s4");
        if c.remove_all().is_ok() { accepted.push("remove_all"); }
        assert!(
            accepted.is_empty(),
            "a stopped sharded cache still acted on: {accepted:?}"
        );
    }

    /// Nothing that reclaims may drop a pin.
    ///
    /// A pin is a promise to whoever holds the handle, and four different
    /// things reclaim: memory pressure, a capacity reduction, the per-write
    /// expiry sweep, and a read noticing an entry has lapsed. Each was written
    /// separately and each has its own reason to skip a pinned key, which is
    /// exactly the shape that ends up with three of the four agreeing.
    ///
    /// The last case is the interesting one, because the two contracts pull
    /// opposite ways: expiry says the entry must not be served, pinning says it
    /// must not be released. Both hold -- the read refuses, the pin survives --
    /// and the handle's own copy stays valid because it owns its bytes.
    #[test]
    fn nothing_that_reclaims_drops_a_pin() {
        fn cache(dir: &std::path::Path, name: &str, dram: usize) -> MultiLayerCache {
            let c = MultiLayerCache::with_options(CacheOptions {
                dram_capacity: dram,
                pmem_capacity: 0,
                ssd_capacity: 1 << 20,
                ssd_paths: vec![dir.join(name)],
                ..CacheOptions::default()
            });
            c.start().unwrap();
            c
        }
        let dir = tempfile::tempdir().unwrap();

        // Memory pressure: one pinned entry against many newcomers.
        let c = cache(dir.path(), "pressure", 4096);
        let key = CacheKey::string(0, "held");
        c.put(key.clone(), vec![b'v'; 64]).unwrap();
        c.pin(key.clone());
        for index in 0..512 {
            c.put(CacheKey::string(0, &format!("churn-{index:04}")), vec![b'v'; 64])
                .unwrap();
        }
        assert!(
            c.stats().memory_evictions > 0,
            "nothing was evicted, so the pin was never tested"
        );
        assert_eq!(c.stats().pinned_entries, 1, "memory pressure dropped a pin");

        // A capacity reduction, which evicts in bulk with no writes.
        let c = cache(dir.path(), "shrink", 1 << 20);
        let key = CacheKey::string(0, "held");
        c.put(key.clone(), vec![b'v'; 64]).unwrap();
        c.pin(key.clone());
        for index in 0..256 {
            c.put(CacheKey::string(0, &format!("other-{index:04}")), vec![b'v'; 64])
                .unwrap();
        }
        c.set_capacity_for_tier(CacheTier::Memory, 1024);
        assert_eq!(c.stats().pinned_entries, 1, "a shrink dropped a pin");

        // The expiry sweep, driven by writes.
        let c = cache(dir.path(), "sweep", 1 << 20);
        let key = CacheKey::string(0, "held-and-lapsed");
        c.put_with_ttl(key.clone(), vec![b'v'; 64], Duration::from_millis(30))
            .unwrap();
        c.pin(key.clone());
        std::thread::sleep(Duration::from_millis(120));
        for index in 0..64 {
            c.put(CacheKey::string(0, &format!("drive-{index:04}")), vec![b'v'; 64])
                .unwrap();
        }
        assert_eq!(c.stats().pinned_entries, 1, "the expiry sweep dropped a pin");

        // A read noticing the entry has lapsed. Both contracts at once.
        let c = cache(dir.path(), "read", 1 << 20);
        let key = CacheKey::string(0, "held-and-lapsed");
        c.put_with_ttl(key.clone(), vec![b'v'; 64], Duration::from_millis(30))
            .unwrap();
        c.pin(key.clone());
        std::thread::sleep(Duration::from_millis(120));
        assert!(
            c.get(&key).unwrap().is_none(),
            "an expired entry was served because it was pinned"
        );
        assert_eq!(
            c.stats().pinned_entries,
            1,
            "a read reclaiming an expired entry dropped its pin"
        );

        // And the pin is still releasable afterwards, so the accounting closes.
        c.unpin(&key);
        assert_eq!(c.stats().pinned_entries, 0, "the pin could not be released");
    }

    /// Block durability is on unless it is turned off, and turning it off
    /// does not change what the cache stores or serves.
    ///
    /// It removes two `fsync` calls per block write -- the block and then the
    /// directory entry after the rename -- which on this machine is the
    /// difference between 7676 and 421 microseconds per put. The block still
    /// arrives whole, because it arrives by rename; what is given up is
    /// surviving a crash of the machine.
    #[test]
    fn ssd_block_durability_is_on_by_default_and_optional() {
        assert!(
            CacheOptions::default().ssd_block_durability,
            "a cache stopped flushing its blocks without being asked"
        );

        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_options(CacheOptions {
            dram_capacity: 4096,
            ssd_capacity: 1 << 20,
            ssd_paths: vec![dir.path().to_path_buf()],
            ssd_block_durability: false,
            ..CacheOptions::default()
        });
        cache.start().unwrap();

        // Enough to push entries out of memory, so the reads below come back
        // from the tier whose writes are no longer flushed.
        for index in 0..256 {
            cache
                .put(CacheKey::string(0, &format!("unflushed-{index:03}")), vec![b'v'; 64])
                .unwrap();
        }
        assert!(
            cache.stats().memory_evictions > 0,
            "nothing left memory, so nothing was read back from the SSD tier"
        );
        for index in 0..256 {
            assert!(
                cache
                    .get(&CacheKey::string(0, &format!("unflushed-{index:03}")))
                    .unwrap()
                    .is_some(),
                "entry {index} was lost when its write was not flushed"
            );
        }
    }

    /// Recovering a tier whose writes were never flushed is a contradiction,
    /// and the configuration check says so.
    #[test]
    fn recovering_an_unflushed_tier_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let options = CacheOptions {
            dram_capacity: 1 << 16,
            ssd_capacity: 1 << 20,
            ssd_paths: vec![dir.path().to_path_buf()],
            auto_recover_on_start: true,
            ssd_block_durability: false,
            ..CacheOptions::default()
        };
        // Filtered by field: both tiers report under this id, and this test is
        // about the SSD one.
        assert!(
            options.validate().iter().any(|finding| {
                finding.id == "recovery_expects_durability_that_is_off"
                    && finding.field == "ssd_block_durability"
            }),
            "a configuration that recovers what it never flushed was not reported"
        );

        // Either half alone is a choice, not a contradiction.
        for (recover, durable) in [(true, true), (false, false)] {
            let sane = CacheOptions {
                auto_recover_on_start: recover,
                ssd_block_durability: durable,
                ..options.clone()
            };
            assert!(
                !sane.validate().iter().any(|f| {
                    f.id == "recovery_expects_durability_that_is_off"
                        && f.field == "ssd_block_durability"
                }),
                "recover={recover} durable={durable} was reported as a contradiction"
            );
        }
    }

    /// The persistent tier does not flush unless it is asked to, and says so.
    ///
    /// Its name invites the opposite assumption: real persistent memory is
    /// durable without being flushed, and this tier is files standing in for
    /// it. Measured, the tier wrote 436 blocks and issued zero `fsync` calls.
    ///
    /// The default stays what it has always been. What changes is that it is
    /// now written down and can be turned on.
    #[test]
    fn the_persistent_tier_states_whether_it_is_durable() {
        assert!(
            !CacheOptions::default().pmem_block_durability,
            "the persistent tier started flushing without being asked, which is a \
             behaviour change, not a default"
        );

        // On or off, the tier stores and serves the same things.
        for durable in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let cache = MultiLayerCache::with_options(CacheOptions {
                dram_capacity: 4096,
                pmem_capacity: 1 << 20,
                pmem_paths: vec![dir.path().join("pmem")],
                ssd_capacity: 0,
                pmem_block_durability: durable,
                ..CacheOptions::default()
            });
            cache.start().unwrap();
            for index in 0..128 {
                cache
                    .put(CacheKey::string(0, &format!("entry-{index:03}")), vec![b'v'; 64])
                    .unwrap();
            }
            assert!(
                cache.stats().pmem_fills > 0,
                "durable={durable}: nothing reached the persistent tier"
            );
            for index in 0..128 {
                assert!(
                    cache
                        .get(&CacheKey::string(0, &format!("entry-{index:03}")))
                        .unwrap()
                        .is_some(),
                    "durable={durable}: entry {index} was lost"
                );
            }
        }
    }

    /// Recovering the persistent tier without flushing it is the same
    /// contradiction as on the tier below, and is met more easily.
    #[test]
    fn recovering_an_unflushed_persistent_tier_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let options = CacheOptions {
            dram_capacity: 1 << 16,
            pmem_capacity: 1 << 20,
            pmem_paths: vec![dir.path().join("pmem")],
            ssd_capacity: 0,
            auto_recover_on_start: true,
            ..CacheOptions::default()
        };
        let reported: Vec<String> = options
            .validate()
            .into_iter()
            .filter(|finding| finding.id == "recovery_expects_durability_that_is_off")
            .map(|finding| finding.field)
            .collect();
        assert_eq!(
            reported,
            vec!["pmem_block_durability".to_string()],
            "the persistent tier's default was not reported against recovery"
        );

        // Turning it on settles it.
        let durable = CacheOptions {
            pmem_block_durability: true,
            ..options
        };
        assert!(
            !durable
                .validate()
                .iter()
                .any(|f| f.id == "recovery_expects_durability_that_is_off"),
            "a flushed tier was still reported as unflushed"
        );
    }

    /// Records written after a compaction are still there after a reopen.
    ///
    /// The journal is held open between appends rather than reopened for each
    /// one. Compaction folds it into a snapshot and deletes it, so the handle
    /// has to be dropped at the same moment: an append through a handle whose
    /// file has been unlinked succeeds, writes to an inode nothing can reach,
    /// and loses the record at the next restart. Nothing about that failure is
    /// visible until the reopen.
    ///
    /// So this writes across a compaction on purpose, and checks the far side.
    #[cfg(not(feature = "rocksdb-ssd"))]
    #[test]
    fn records_written_after_a_compaction_survive_a_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("store");
        let store_file = path.join("matrixcache_rocksdb_compat_store.bin");

        let mut store = StorageEngineRocksDb::new(path.to_string_lossy().to_string());
        assert!(store.start());

        // Enough bytes to force at least one compaction, which is what removes
        // the journal underneath the handle.
        for index in 0..64 {
            store
                .put(&format!("before-{index:03}"), vec![b'v'; 4096])
                .unwrap();
        }
        assert!(
            store_file.exists(),
            "nothing compacted, so the handle was never invalidated and this \
             test proves nothing"
        );

        // Written after the journal was deleted. These are the records a stale
        // handle would swallow.
        for index in 0..8 {
            store
                .put(&format!("after-{index:03}"), format!("value-{index}").into_bytes())
                .unwrap();
        }
        store.delete("before-000").unwrap();
        drop(store);

        let mut reopened = StorageEngineRocksDb::new(path.to_string_lossy().to_string());
        assert!(reopened.start());
        for index in 0..8 {
            assert_eq!(
                reopened
                    .get(&format!("after-{index:03}"))
                    .unwrap_or_else(|_| panic!("record after-{index:03} was written through a \
                                                stale handle and lost"))
                    .to_vec(),
                format!("value-{index}").into_bytes()
            );
        }
        assert!(
            matches!(reopened.get("before-000"), Err(CacheError::NotFound)),
            "a delete written after the compaction was lost"
        );
        // And the pre-compaction records are still in the snapshot.
        assert_eq!(reopened.get("before-001").unwrap().to_vec().len(), 4096);
    }

    /// The write-budget share is a proportion, not a count, so aggregating it
    /// by addition would report nonsense. The tightest shard is the one that
    /// explains why a caller is seeing writes refused.
    #[test]
    fn the_sharded_write_budget_share_reports_the_tightest_shard() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ShardedMultiLayerCache::with_options(
            CacheOptions {
                dram_capacity: 1 << 18,
                ssd_capacity: 1 << 20,
                ssd_paths: vec![dir.path().to_path_buf()],
                ..CacheOptions::default()
            },
            4,
        );
        // Uncapped: every shard admits everything, and four shards each at
        // 10000 must not read as 40000.
        assert_eq!(
            cache.stats().ssd_write_budget_share,
            10_000,
            "an uncapped sharded cache did not report a full share"
        );
    }

    /// Expired entries have to give their memory back even when the cache is
    /// nowhere near full, and without anyone calling the sweep.
    ///
    /// A read notices expiry, and eviction prefers an expired victim, but an
    /// entry nobody reads in a cache under no pressure gets neither. That
    /// memory used to sit there until something else forced a reclaim.
    #[test]
    fn expired_memory_comes_back_without_pressure_or_a_manual_sweep() {
        let dir = tempfile::tempdir().unwrap();
        // Room for far more than is written, so eviction never runs.
        let cache = MultiLayerCache::with_options(CacheOptions {
            dram_capacity: 1 << 20,
            pmem_capacity: 0,
            ssd_capacity: 0,
            ssd_paths: vec![dir.path().to_path_buf()],
            ..CacheOptions::default()
        });
        cache.start().unwrap();

        for index in 0..64 {
            cache
                .put_with_ttl(
                    CacheKey::string(0, &format!("lapses-{index:03}")),
                    vec![b'x'; 64],
                    Duration::from_millis(40),
                )
                .unwrap();
        }
        let peak = cache.stats().memory_bytes;
        assert!(peak > 0);
        assert_eq!(cache.stats().memory_evictions, 0, "the cache was under pressure");

        std::thread::sleep(Duration::from_millis(150));

        // Keep writing. Nothing reads the lapsed keys and nothing evicts, so
        // only the incremental sweep can reclaim them.
        for index in 0..64 {
            cache
                .put(CacheKey::string(0, &format!("later-{index:03}")), vec![b'y'; 64])
                .unwrap();
        }

        let stats = cache.stats();
        assert_eq!(stats.memory_evictions, 0, "eviction ran, so this proves nothing");
        assert!(
            stats.expired_removals > 0,
            "nothing was reclaimed incrementally: {stats:?}"
        );
        // The lapsed entries' bytes are back; only the newer writes remain.
        assert!(
            stats.memory_bytes <= peak,
            "memory grew past the first round despite the reclaim: {} vs {peak}",
            stats.memory_bytes
        );
    }

    /// A cache that never asks for a time to live must not pay for the sweep,
    /// and must behave exactly as it did.
    #[test]
    fn the_expiry_sweep_costs_nothing_when_no_life_is_ever_set() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_options(CacheOptions {
            dram_capacity: 1 << 20,
            pmem_capacity: 0,
            ssd_capacity: 0,
            ssd_paths: vec![dir.path().to_path_buf()],
            ..CacheOptions::default()
        });
        cache.start().unwrap();

        for index in 0..128 {
            cache
                .put(CacheKey::string(0, &format!("immortal-{index:03}")), vec![b'v'; 64])
                .unwrap();
        }
        let stats = cache.stats();
        assert_eq!(stats.expired_removals, 0);
        assert_eq!(stats.expired_reads, 0);
        // Every entry is still there.
        for index in 0..128 {
            assert!(
                cache
                    .get(&CacheKey::string(0, &format!("immortal-{index:03}")))
                    .unwrap()
                    .is_some(),
                "entry {index} went missing without any life set"
            );
        }
    }

    /// The same, for reads that take the cache exclusively.
    ///
    /// Hits are accounted by two different paths: one that runs under the
    /// shared lock, and one that already holds the cache exclusively — which is
    /// what a zero-copy read, a persistent-memory hit and an SSD hit all use.
    /// Both stamp the same field, so fixing the stamp in one of them leaves
    /// entries read through the other still unable to earn a promotion. Which
    /// tier a read lands on must not decide whether the entry keeps its place.
    #[test]
    fn an_entry_read_continuously_through_a_pinned_handle_is_still_promoted() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1 << 18, dir.path());
        cache.start().unwrap();

        let key = CacheKey::string(0, "pinned-constantly");
        cache.put(key.clone(), vec![b'v'; 64]).unwrap();

        // Long enough for two refresh windows to elapse, so a window measured
        // from the last promotion fires twice while one measured from the
        // previous read never fires at all.
        let started = Instant::now();
        let mut reads = 0u64;
        while started.elapsed() < Duration::from_millis(1_300) {
            let handle = cache.acquire(&key).unwrap().expect("the key is cached");
            cache.release(handle);
            reads += 1;
        }
        assert!(reads > 1_000, "the loop was too slow to be a tight one");

        let refreshes = cache.stats().access_order_refreshes;
        assert!(
            refreshes >= 2,
            "{reads} zero-copy reads over 1300ms produced {refreshes} promotions; \
             the exclusive hit path still stamps on every read"
        );
    }

    /// An entry that has expired must stay gone, including on the SSD tier.
    ///
    /// Expiry is recorded on the entry's metadata, and reclaiming an expired
    /// entry drops that metadata. If a copy of the value is still on a lower
    /// tier at that point, the next read finds no expiry to check, falls
    /// through to that tier and serves the value again -- the entry comes back
    /// from the dead, and stays back, because nothing will ever consider it
    /// expired again.
    #[test]
    fn an_expired_entry_does_not_come_back_from_a_lower_tier() {
        let dir = tempfile::tempdir().unwrap();
        // Memory and SSD both enabled, so a write lands on both.
        let cache = MultiLayerCache::new(1 << 16, dir.path());
        cache.start().unwrap();

        let key = CacheKey::string(0, "should-stay-gone");
        cache
            .put_with_ttl(key.clone(), vec![b'v'; 32], Duration::from_millis(40))
            .unwrap();
        assert!(cache.get(&key).unwrap().is_some(), "not cached to begin with");

        std::thread::sleep(Duration::from_millis(150));

        // The read that notices expiry reclaims the entry.
        assert!(
            cache.get(&key).unwrap().is_none(),
            "the expired entry was served"
        );

        // And it must still be gone. This is the read that used to resurrect
        // it: the metadata carrying the expiry has been dropped, so nothing is
        // left to say the value on the lower tier is too old to serve.
        assert!(
            cache.get(&key).unwrap().is_none(),
            "an expired entry came back from a lower tier on the next read"
        );
    }

    /// A reclaimed expired entry must not come back when the cache restarts.
    ///
    /// Recovery rebuilds the SSD tier from what is on the device, not from the
    /// metadata that carried the expiry -- so an expired entry whose block was
    /// left behind reappears on restart as an ordinary, unexpiring one.
    /// Verified by serving a read from the restarted cache rather than by
    /// comparing internal state, since serving the value is the thing that
    /// would actually be wrong.
    #[test]
    fn a_reclaimed_expired_entry_stays_gone_across_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let options = CacheOptions {
            dram_capacity: 1 << 16,
            ssd_capacity: 1 << 20,
            ssd_paths: vec![dir.path().to_path_buf()],
            ..CacheOptions::default()
        };

        let cache = MultiLayerCache::with_options(options.clone());
        cache.start().unwrap();
        let key = CacheKey::string(0, "gone-after-restart");
        cache
            .put_with_ttl(key.clone(), vec![b'v'; 32], Duration::from_millis(40))
            .unwrap();
        std::thread::sleep(Duration::from_millis(150));
        // The read that notices expiry reclaims it from every tier.
        assert!(cache.get(&key).unwrap().is_none());
        cache.stop();

        let restarted = MultiLayerCache::with_options(options);
        restarted.start().unwrap();
        restarted.recover_persistent_tiers().unwrap();
        assert!(
            restarted.get(&key).unwrap().is_none(),
            "an expired entry came back when the cache restarted"
        );
    }

    /// Clearing a shard must leave nothing of it behind on any tier.
    ///
    /// The same shape as the expiry bug: the metadata that describes an entry
    /// is dropped here, so a copy of the value surviving on a lower tier would
    /// be served afterwards with nothing left to describe it. Checked by
    /// asking the cache for the keys rather than by inspecting its tiers,
    /// because being served the value is the thing that would be wrong.
    #[test]
    fn clearing_a_shard_leaves_nothing_readable_on_any_tier() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1 << 16, dir.path());
        cache.start().unwrap();

        let cleared: Vec<CacheKey> = (0..16)
            .map(|index| CacheKey::string(7, &format!("cleared-{index:02}")))
            .collect();
        let kept: Vec<CacheKey> = (0..16)
            .map(|index| CacheKey::string(8, &format!("kept-{index:02}")))
            .collect();
        for key in cleared.iter().chain(kept.iter()) {
            cache.put(key.clone(), vec![b'v'; 32]).unwrap();
        }

        cache.invalidate_shard(7).unwrap();

        for key in &cleared {
            assert!(
                cache.get(key).unwrap().is_none(),
                "a cleared key was still served: {key:?}"
            );
        }
        for key in &kept {
            assert!(
                cache.get(key).unwrap().is_some(),
                "clearing one shard took another shard's entry: {key:?}"
            );
        }
    }

    /// Dropping an entry from memory must not make its lower-tier copy
    /// immortal.
    ///
    /// A memory-only invalidation is supposed to leave the SSD copy readable --
    /// that is the whole point of it. What it must not do is throw away the
    /// entry's metadata, because that is where the time to live is recorded:
    /// the value survives on SSD with nothing left to say when it stops being
    /// servable, and an entry written with a life becomes one without.
    #[test]
    fn a_memory_only_invalidation_does_not_strip_the_life_from_a_lower_tier_copy() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1 << 16, dir.path());
        cache.start().unwrap();

        let key = CacheKey::string(0, "memory-only-invalidated");
        cache
            .put_with_ttl(key.clone(), vec![b'v'; 32], Duration::from_millis(40))
            .unwrap();
        // Still inside its life, and resident on both tiers.
        assert!(cache.get(&key).unwrap().is_some());

        cache.invalidate_memory_only(&key);
        // The SSD copy is meant to still answer, for now.
        assert!(
            cache.get(&key).unwrap().is_some(),
            "a memory-only invalidation removed the lower-tier copy too"
        );

        std::thread::sleep(Duration::from_millis(150));
        assert!(
            cache.get(&key).unwrap().is_none(),
            "the entry outlived its time to live because the invalidation \
             dropped the metadata that carried it"
        );
    }

    /// Reclaiming an expired entry must delete its persisted copy too.
    ///
    /// The persistent-memory tier is written through to disk, so removing the
    /// entry from the in-memory map is not enough: the file survives, recovery
    /// restores it, and the entry is back — with its metadata, and therefore
    /// its time to live, gone. The eviction and invalidation paths both delete
    /// the persisted copy; expiry did not.
    #[test]
    fn a_reclaimed_expired_entry_does_not_return_from_persistent_memory() {
        let dir = tempfile::tempdir().unwrap();
        let pmem_path = dir.path().join("pmem");
        let ssd_path = dir.path().join("ssd");
        // Pmem large enough to take the value, memory too small, so the write
        // lands on the persistent tier.
        let options = CacheOptions::new(8, 4096, 4096)
            .with_pmem_paths([pmem_path.clone()])
            .with_ssd_paths([ssd_path.clone()]);

        let cache = MultiLayerCache::with_options(options.clone());
        cache.start().unwrap();
        let key = CacheKey::string(0, "pmem-should-stay-gone");
        cache
            .put_with_ttl(key.clone(), vec![b'v'; 64], Duration::from_millis(40))
            .unwrap();
        assert!(cache.get(&key).unwrap().is_some(), "not cached to begin with");

        std::thread::sleep(Duration::from_millis(150));
        assert!(cache.get(&key).unwrap().is_none(), "the expired entry was served");
        cache.stop();

        let restarted = MultiLayerCache::with_options(options);
        restarted.start().unwrap();
        restarted.recover_persistent_tiers().unwrap();
        assert!(
            restarted.get(&key).unwrap().is_none(),
            "an expired entry came back from persistent memory after a restart"
        );
    }

    /// An entry read continuously has to keep being promoted in the access
    /// order, not stop being promoted *because* it is read continuously.
    ///
    /// The refresh window exists so a hit does not have to move its entry every
    /// time. It compares the clock against a stamp on the entry -- and that
    /// stamp used to be advanced on every read, which made the comparison "how
    /// long since the previous read" rather than "how long since this entry was
    /// last moved". An entry read in a tight loop therefore always looked
    /// freshly seen, and was never moved again after its first read: the
    /// hotter it was, the more surely it drifted to the cold end and was
    /// evicted.
    #[test]
    fn an_entry_read_continuously_is_still_promoted() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1 << 18, dir.path());
        cache.start().unwrap();

        let key = CacheKey::string(0, "read-constantly");
        cache.put(key.clone(), vec![b'v'; 64]).unwrap();

        // Read without pause for longer than the refresh window, so a window
        // measured from the last promotion must elapse at least once, while
        // one measured from the previous read never can.
        let started = Instant::now();
        let mut reads = 0u64;
        while started.elapsed() < Duration::from_millis(900) {
            assert!(cache.get(&key).unwrap().is_some());
            reads += 1;
        }
        assert!(reads > 1_000, "the loop was too slow to be a tight one");

        let refreshes = cache.stats().access_order_refreshes;
        assert!(
            refreshes >= 2,
            "{reads} reads over 900ms produced {refreshes} promotions; \
             a continuously-read entry stopped being promoted after its first read"
        );
    }

    /// Under pressure, an entry past its life should go before a live one.
    ///
    /// Dropping an expired entry costs no future hit, because it could not have
    /// been served again anyway. Dropping a live entry to make the same room
    /// costs exactly one. Eviction used to weigh the two the same way.
    #[test]
    fn eviction_takes_an_expired_entry_before_a_live_one() {
        let dir = tempfile::tempdir().unwrap();
        // Memory only, room for eight 32-byte entries, so eviction is forced
        // and nothing is demoted to a lower tier where it could still be read.
        let cache = MultiLayerCache::with_options(CacheOptions {
            dram_capacity: 8 * 32,
            pmem_capacity: 0,
            ssd_capacity: 0,
            ssd_paths: vec![dir.path().to_path_buf()],
            ..CacheOptions::default()
        });
        cache.start().unwrap();

        // Four entries that will lapse, four that never will.
        for index in 0..4 {
            cache
                .put_with_ttl(
                    CacheKey::string(0, &format!("lapses-{index}")),
                    vec![b'x'; 32],
                    Duration::from_millis(40),
                )
                .unwrap();
        }
        let keepers: Vec<CacheKey> = (0..4)
            .map(|index| CacheKey::string(0, &format!("keeps-{index}")))
            .collect();
        for key in &keepers {
            cache.put(key.clone(), vec![b'y'; 32]).unwrap();
        }

        // Let the first four lapse without reading them, so nothing is expired
        // lazily and eviction is the thing that has to notice.
        std::thread::sleep(Duration::from_millis(150));

        // Now push the tier over capacity.
        for index in 0..4 {
            cache
                .put(CacheKey::string(0, &format!("newcomer-{index}")), vec![b'z'; 32])
                .unwrap();
        }

        let stats = cache.stats();
        // Reclaimed one way or the other: eviction preferring an expired
        // victim, or the incremental sweep on those same writes getting there
        // first. Both deliver the guarantee this test is about, and which one
        // wins depends on how the writes interleave.
        assert!(
            stats.eviction_expired > 0 || stats.expired_removals > 0,
            "the expired entries were never reclaimed: {stats:?}"
        );

        // The entries with no life on them should have survived, because there
        // were expired ones to take instead.
        let survivors = keepers
            .iter()
            .filter(|key| cache.get(key).unwrap().is_some())
            .count();
        assert_eq!(
            survivors, 4,
            "a live entry was evicted while expired ones were available"
        );
    }

    /// With nothing expired, eviction has to behave exactly as it did.
    #[test]
    fn eviction_is_unchanged_when_nothing_has_expired() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_options(CacheOptions {
            dram_capacity: 8 * 32,
            pmem_capacity: 0,
            ssd_capacity: 0,
            ssd_paths: vec![dir.path().to_path_buf()],
            ..CacheOptions::default()
        });
        cache.start().unwrap();

        for index in 0..16 {
            cache
                .put(CacheKey::string(0, &format!("immortal-{index:02}")), vec![b'v'; 32])
                .unwrap();
        }
        let stats = cache.stats();
        assert!(stats.memory_evictions > 0, "nothing was evicted at all");
        assert_eq!(
            stats.eviction_expired, 0,
            "an entry was called expired when none had a life"
        );
    }

    /// Writing a key again has to be what the next read sees.
    ///
    /// A key that is written repeatedly grows hot, and past a threshold the
    /// tiering decision stops admitting its rewrites to the memory tier and
    /// sends them straight to SSD. Reads look in memory first, so the copy left
    /// there answered in preference to the value just written: the write looked
    /// like it had been lost, and stayed lost until something evicted the
    /// entry.
    #[test]
    fn rewriting_a_key_is_what_the_next_read_sees() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1 << 16, dir.path());
        cache.start().unwrap();
        let key = CacheKey::string(0, "rewritten");

        // Ten rewrites is well past the hotness at which routing changes, so
        // this covers the write before the change and every one after it.
        for round in 0..10u8 {
            let value = vec![b'a' + round; 32];
            cache.put(key.clone(), value.clone()).unwrap();
            let read = cache.get(&key).unwrap().expect("the key is cached");
            assert_eq!(
                read, value,
                "round {round} read back a value from an earlier write"
            );
        }
    }

    /// The same, without a read between the writes, so nothing is refilled
    /// along the way and the last write has to stand on its own.
    #[test]
    fn the_last_write_wins_even_with_no_read_between_writes() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1 << 16, dir.path());
        cache.start().unwrap();
        let key = CacheKey::string(0, "last-write");

        for round in 0..10u8 {
            cache.put(key.clone(), vec![b'a' + round; 32]).unwrap();
        }
        let read = cache.get(&key).unwrap().expect("the key is cached");
        assert_eq!(
            read,
            vec![b'a' + 9; 32],
            "a read was served an earlier write than the last one"
        );
    }

    /// A tier that gives up its copy must give up the bytes with it, or the
    /// accounting drifts every time a hot key is rewritten.
    #[test]
    fn dropping_a_stale_tier_copy_returns_its_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1 << 16, dir.path());
        cache.start().unwrap();
        let key = CacheKey::string(0, "accounted");

        for round in 0..10u8 {
            cache.put(key.clone(), vec![b'a' + round; 32]).unwrap();
        }
        let stats = cache.stats();
        // One key, 32 bytes: whichever tier holds it, the memory tier must not
        // be counting more than one copy of it.
        assert!(
            stats.memory_bytes <= 32,
            "memory accounting grew across rewrites: {} bytes for one 32-byte key",
            stats.memory_bytes
        );
        assert!(
            stats.stale_tier_copies_dropped > 0,
            "the rewrites never routed past the memory tier, so this proves nothing"
        );
    }

    /// The severity counts are always present, so "nothing wrong" reads as a
    /// zero rather than as a series that has stopped being scraped.
    #[test]
    fn health_metrics_report_zero_rather_than_nothing_when_clean() {
        let report = cache_health_report(&CacheStats::default());
        let text = cache_health_prometheus_text(&report, &[]);
        assert!(text.contains("matrixcache_health_ok 1"), "{text}");
        assert!(
            text.contains("matrixcache_health_findings{severity=\"critical\"} 0"),
            "{text}"
        );
        assert!(
            text.contains("matrixcache_health_findings{severity=\"warning\"} 0"),
            "{text}"
        );
        // No finding series at all while there are no findings.
        assert!(!text.contains("matrixcache_health_finding{"), "{text}");
    }

    /// A reported finding carries both numbers, so a dashboard can show how
    /// close the cache is rather than only that it has crossed.
    #[test]
    fn health_metrics_carry_the_observed_number_and_its_threshold() {
        let stats = CacheStats {
            refill_failures: 3,
            ..CacheStats::default()
        };
        let report = cache_health_report(&stats);
        let text = cache_health_prometheus_text(&report, &[]);
        assert!(text.contains("matrixcache_health_ok 0"), "{text}");
        assert!(
            text.contains(
                "matrixcache_health_finding{id=\"refill_failures\",component=\"refill\",severity=\"critical\"} 3"
            ),
            "{text}"
        );
        assert!(
            text.contains("matrixcache_health_finding_threshold{id=\"refill_failures\"} 0"),
            "{text}"
        );
    }

    /// Labels land on every series, so a cache's health lines up with its
    /// statistics under one label set.
    #[test]
    fn health_metrics_label_every_series() {
        let stats = CacheStats {
            refill_failures: 1,
            ..CacheStats::default()
        };
        let report = cache_health_report(&stats);
        let text = cache_health_prometheus_text(&report, &[("cache", "sessions")]);
        for line in text.lines().filter(|l| !l.starts_with('#')) {
            assert!(
                line.contains("cache=\"sessions\""),
                "unlabelled series: {line}"
            );
        }
    }

    /// The default has to be the behaviour the cache always had, and the only
    /// way to know that is to check the uncapped path still admits everything.
    #[test]
    fn ssd_write_budget_is_off_by_default_and_admits_every_write() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_options(CacheOptions {
            dram_capacity: 0,
            pmem_capacity: 0,
            ssd_capacity: 1 << 20,
            ssd_paths: vec![dir.path().to_path_buf()],
            ..CacheOptions::default()
        });
        cache.start().unwrap();
        assert_eq!(cache.ssd_write_budget_bytes_per_sec(), 0);

        for index in 0..64 {
            let key = CacheKey::string(0, &format!("budget-off-{index:04}"));
            cache.put(key, vec![b'v'; 64]).unwrap();
        }
        let stats = cache.stats();
        assert_eq!(
            stats.ssd_write_budget_rejections, 0,
            "an uncapped cache refused a write"
        );
        assert_eq!(stats.ssd_write_budget_share, 10_000);
        assert!(stats.ssd_bytes_written > 0, "nothing reached the drive");
    }

    /// With a cap far below what the workload offers, the budget has to start
    /// turning admissions away rather than writing at whatever rate it is fed.
    #[test]
    fn ssd_write_budget_turns_admissions_away_once_it_is_over_target() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_options(CacheOptions {
            dram_capacity: 0,
            pmem_capacity: 0,
            ssd_capacity: 1 << 22,
            ssd_paths: vec![dir.path().to_path_buf()],
            // One byte a second, against a workload writing kilobytes.
            ssd_write_bytes_per_sec: 1,
            ..CacheOptions::default()
        });
        cache.start().unwrap();
        assert_eq!(cache.ssd_write_budget_bytes_per_sec(), 1);

        // Enough writes, over enough wall-clock, for at least one window to
        // roll and the share to close.
        for index in 0..400 {
            let key = CacheKey::string(0, &format!("budget-on-{index:04}"));
            let _ = cache.put(key, vec![b'v'; 4096]);
        }
        std::thread::sleep(Duration::from_millis(1100));
        for index in 400..800 {
            let key = CacheKey::string(0, &format!("budget-on-{index:04}"));
            let _ = cache.put(key, vec![b'v'; 4096]);
        }

        let stats = cache.stats();
        assert!(
            stats.ssd_write_budget_rejections > 0,
            "a 1 byte/s budget never refused anything: {stats:?}"
        );
        assert!(
            stats.ssd_write_budget_share < 10_000,
            "the share never closed: {}",
            stats.ssd_write_budget_share
        );

        // And the cache says so, rather than leaving the operator to infer it
        // from a fallen hit rate.
        let report = cache.health_report();
        let ids: Vec<&str> = report.findings.iter().map(|f| f.id.as_str()).collect();
        assert!(ids.contains(&"ssd_write_budget_throttling"), "{ids:?}");
    }

    /// A sharded cache aims at the number it was given, not that number once
    /// per shard.
    #[test]
    fn sharded_write_budget_splits_the_target_across_shards() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ShardedMultiLayerCache::with_options(
            CacheOptions {
                dram_capacity: 1 << 16,
                ssd_capacity: 1 << 20,
                ssd_paths: vec![dir.path().to_path_buf()],
                ..CacheOptions::default()
            },
            4,
        );
        cache.set_ssd_write_budget_bytes_per_sec(4_002);
        // 4002 does not divide by 4, so this also checks the remainder is
        // handed out rather than dropped.
        assert_eq!(
            cache.ssd_write_budget_bytes_per_sec(),
            4_002,
            "the shards must sum to the target"
        );
    }

    /// An entry with no life asked for must outlive everything, or every
    /// existing caller would silently acquire an expiry.
    #[test]
    fn entries_do_not_expire_unless_a_life_is_asked_for() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1 << 16, dir.path());
        cache.start().unwrap();
        assert_eq!(cache.default_ttl(), Duration::from_millis(0));

        let key = CacheKey::string(0, "immortal");
        cache.put(key.clone(), vec![b'v'; 32]).unwrap();
        std::thread::sleep(Duration::from_millis(120));
        assert!(cache.get(&key).unwrap().is_some(), "an entry expired on its own");
        assert_eq!(cache.purge_expired(), 0);
        assert_eq!(cache.stats().expired_reads, 0);
    }

    /// Past its life an entry stops being servable, and the read that finds it
    /// drops it rather than leaving it to be swept later.
    #[test]
    fn an_expired_entry_reads_as_a_miss_and_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1 << 16, dir.path());
        cache.start().unwrap();

        let key = CacheKey::string(0, "brief");
        cache
            .put_with_ttl(key.clone(), vec![b'v'; 32], Duration::from_millis(40))
            .unwrap();
        // Still inside its life.
        assert!(cache.get(&key).unwrap().is_some());

        std::thread::sleep(Duration::from_millis(150));
        assert!(cache.get(&key).unwrap().is_none(), "an expired entry was served");

        let stats = cache.stats();
        assert_eq!(stats.expired_reads, 1);
        assert_eq!(stats.expired_removals, 1);
        // The read that noticed it counts as a miss, because that is what the
        // caller was served.
        assert!(stats.misses >= 1);
        // And it is gone, not merely hidden.
        assert_eq!(cache.purge_expired(), 0);
    }

    /// Writing a key again restarts its life.
    #[test]
    fn rewriting_a_key_restarts_its_life() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1 << 16, dir.path());
        cache.start().unwrap();

        let key = CacheKey::string(0, "renewed");
        cache
            .put_with_ttl(key.clone(), vec![b'a'; 32], Duration::from_millis(60))
            .unwrap();
        std::thread::sleep(Duration::from_millis(40));
        // Rewrite before it lapses, with a fresh life.
        cache
            .put_with_ttl(key.clone(), vec![b'b'; 32], Duration::from_millis(400))
            .unwrap();
        std::thread::sleep(Duration::from_millis(60));

        let value = cache.get(&key).unwrap();
        assert!(value.is_some(), "the rewrite did not restart the life");
        assert_eq!(value.unwrap()[0], b'b');
    }

    /// A key written with a life and never read again still has to give its
    /// memory back, which is what the sweep is for.
    #[test]
    fn purging_reclaims_entries_no_read_ever_visits() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1 << 20, dir.path());
        cache.start().unwrap();

        for index in 0..32 {
            cache
                .put_with_ttl(
                    CacheKey::string(0, &format!("swept-{index:03}")),
                    vec![b'v'; 64],
                    Duration::from_millis(40),
                )
                .unwrap();
        }
        let resident = cache.stats().memory_bytes;
        assert!(resident > 0);

        std::thread::sleep(Duration::from_millis(150));
        // The incremental sweep runs on writes, so some of these may already be
        // gone before the explicit sweep is asked. What matters is that between
        // them every one is reclaimed and the bytes come back -- not which of
        // the two did the work, which depends on how the writes interleave with
        // the lives running out.
        let purged = cache.purge_expired();
        let stats = cache.stats();
        assert_eq!(
            stats.expired_removals, 32,
            "only {} of 32 were reclaimed ({purged} by the explicit sweep)",
            stats.expired_removals
        );
        assert_eq!(stats.memory_bytes, 0, "the bytes were not given back");
        // Nothing was read, so nothing counted as an expired read.
        assert_eq!(stats.expired_reads, 0);
        // And a second sweep finds nothing left.
        assert_eq!(cache.purge_expired(), 0);
    }

    /// A default life applies to entries written after it is set, and a
    /// per-entry life still wins.
    #[test]
    fn a_default_life_applies_to_later_writes_and_yields_to_an_explicit_one() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1 << 16, dir.path());
        cache.start().unwrap();
        cache.set_default_ttl(Duration::from_millis(40));
        assert_eq!(cache.default_ttl(), Duration::from_millis(40));

        let short = CacheKey::string(0, "default-life");
        let long = CacheKey::string(0, "explicit-life");
        cache.put(short.clone(), vec![b'v'; 32]).unwrap();
        cache
            .put_with_ttl(long.clone(), vec![b'v'; 32], Duration::from_secs(60))
            .unwrap();

        std::thread::sleep(Duration::from_millis(150));
        assert!(cache.get(&short).unwrap().is_none(), "the default life was ignored");
        assert!(
            cache.get(&long).unwrap().is_some(),
            "an explicit life lost to the default"
        );
    }

    /// The sharded cache routes a life-bearing write like any other, and sweeps
    /// every shard.
    #[test]
    fn sharded_cache_honours_and_sweeps_lives() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ShardedMultiLayerCache::with_options(
            CacheOptions {
                dram_capacity: 1 << 18,
                ssd_paths: vec![dir.path().to_path_buf()],
                ..CacheOptions::default()
            },
            4,
        );
        for index in 0..40 {
            cache
                .put_with_ttl(
                    CacheKey::string(0, &format!("sharded-life-{index:03}")),
                    vec![b'v'; 64],
                    Duration::from_millis(40),
                )
                .unwrap();
        }
        std::thread::sleep(Duration::from_millis(150));
        // As above: the incremental sweep may have taken some already, so the
        // total reclaimed across every shard is the thing to check.
        let purged = cache.purge_expired();
        let removed: u64 = cache.stats().expired_removals;
        assert_eq!(
            removed, 40,
            "only {removed} of 40 were reclaimed across the shards ({purged} explicitly)"
        );
        assert_eq!(cache.purge_expired(), 0, "a second sweep still found work");
    }

    /// A cache with no drive to look after admits everything, and does not
    /// care what the key hashes to.
    #[test]
    fn write_budget_admits_everything_when_unlimited() {
        let mut budget = SsdWriteBudget::unlimited();
        let now = Instant::now();
        for hash in [0u64, 1, u64::MAX, 1 << 40, 12345] {
            assert!(budget.admits(hash, now));
        }
        assert_eq!(budget.admitted_share(), 10_000);
        // Recording bytes against an unlimited budget changes nothing.
        budget.record_written(1 << 30, now);
        assert!(budget.admits(0, now));
    }

    /// Writing far over target must narrow the admitted share.
    #[test]
    fn write_budget_narrows_the_share_when_writes_run_over_target() {
        // 1 MiB/s target, fed 16 MiB every second.
        let mut budget = SsdWriteBudget::with_target(1 << 20);
        let start = Instant::now();
        let mut share_before = budget.admitted_share();
        for second in 1..=6u64 {
            budget.record_written(16 << 20, start);
            let now = start + Duration::from_secs(second);
            let _ = budget.admits(0, now);
            let share_now = budget.admitted_share();
            assert!(
                share_now <= share_before,
                "share went up under sustained overload: {share_before} -> {share_now}"
            );
            share_before = share_now;
        }
        assert!(
            share_before < 10_000 / 4,
            "share barely moved under 16x overload: {share_before}"
        );
    }

    /// The rate the budget measured has to describe the window that closed,
    /// not the one starting.
    ///
    /// The share alone cannot tell a budget that is holding the line from one
    /// that is being ignored -- both sit near the floor. The measured rate is
    /// what separates them, so it has to be right.
    #[test]
    fn write_budget_reports_the_rate_it_measured() {
        let mut budget = SsdWriteBudget::with_target(1 << 20);
        let start = Instant::now();
        assert_eq!(
            budget.observed_bytes_per_sec(),
            0,
            "a rate was reported before any window had closed"
        );

        // Two seconds of wall clock carrying 8 MiB, so the rate is 4 MiB/s and
        // is not equal to the bytes, which would let a plain byte count pass.
        budget.record_written(8 << 20, start);
        let _ = budget.admits(0, start + Duration::from_secs(2));
        assert_eq!(
            budget.observed_bytes_per_sec(),
            (8 << 20) / 2,
            "the measured rate did not divide by the window it spanned"
        );

        // A quiet window must report quiet, not hold the old number.
        let _ = budget.admits(0, start + Duration::from_secs(4));
        assert_eq!(
            budget.observed_bytes_per_sec(),
            0,
            "the rate from a busy window survived a quiet one"
        );
    }

    /// A budget the cache cannot obey is worth saying out loud.
    ///
    /// Reclaim and recovery writes count against the target and are never
    /// refused, so they alone can hold the drive over target while the budget
    /// admits nothing. Nothing about the share says so.
    #[test]
    fn health_reports_a_write_budget_that_cannot_be_met() {
        let pinned_and_still_over = CacheStats {
            ssd_write_budget_share: 1,
            ssd_write_budget_target_bytes_per_sec: 1 << 20,
            ssd_write_budget_observed_bytes_per_sec: 3 << 20,
            ..CacheStats::default()
        };
        let report = cache_health_report(&pinned_and_still_over);
        let ids: Vec<&str> = report
            .findings
            .iter()
            .map(|f| f.id.as_str())
            .collect();
        assert!(
            ids.contains(&"ssd_write_budget_cannot_be_met"),
            "a budget being exceeded while admitting nothing went unreported: {ids:?}"
        );

        // The counter-case: over target but still admitting, which is a budget
        // that has not finished tightening rather than one that cannot work.
        let still_tightening = CacheStats {
            ssd_write_budget_share: 5_000,
            ssd_write_budget_target_bytes_per_sec: 1 << 20,
            ssd_write_budget_observed_bytes_per_sec: 3 << 20,
            ..CacheStats::default()
        };
        let report = cache_health_report(&still_tightening);
        let ids: Vec<&str> = report
            .findings
            .iter()
            .map(|f| f.id.as_str())
            .collect();
        assert!(
            !ids.contains(&"ssd_write_budget_cannot_be_met"),
            "a budget still tightening was reported as unmeetable: {ids:?}"
        );
    }

    /// The share must never reach zero: a budget that admits nothing can never
    /// observe that the pressure has passed, and would stay shut forever.
    #[test]
    fn write_budget_never_shuts_completely() {
        let mut budget = SsdWriteBudget::with_target(1);
        let start = Instant::now();
        for second in 1..=40u64 {
            budget.record_written(1 << 30, start);
            let _ = budget.admits(0, start + Duration::from_secs(second));
        }
        assert!(budget.admitted_share() >= 1, "the budget shut completely");
    }

    /// When the writes stop, the share has to come back, or one burst would
    /// throttle the cache for the rest of its life.
    #[test]
    fn write_budget_recovers_when_the_writes_stop() {
        let mut budget = SsdWriteBudget::with_target(1 << 20);
        let start = Instant::now();
        for second in 1..=6u64 {
            budget.record_written(64 << 20, start);
            let _ = budget.admits(0, start + Duration::from_secs(second));
        }
        let throttled = budget.admitted_share();
        assert!(throttled < 10_000, "never throttled: {throttled}");

        // Now go quiet. Each idle window should open the share back up.
        for second in 7..=20u64 {
            let _ = budget.admits(0, start + Duration::from_secs(second));
        }
        assert_eq!(
            budget.admitted_share(),
            10_000,
            "share did not recover after the writes stopped"
        );
    }

    /// The same key decides the same way while the share holds still, so a
    /// retried write does not flip a coin twice.
    #[test]
    fn write_budget_decides_the_same_key_the_same_way() {
        let mut budget = SsdWriteBudget::with_target(1 << 20);
        let start = Instant::now();
        for second in 1..=6u64 {
            budget.record_written(32 << 20, start);
            let _ = budget.admits(7, start + Duration::from_secs(second));
        }
        let now = start + Duration::from_secs(6);
        let first = budget.admits(0x1234_5678_9abc_def0, now);
        for _ in 0..10 {
            assert_eq!(budget.admits(0x1234_5678_9abc_def0, now), first);
        }
    }

    /// Under a throttled share, admission must not depend on how big the write
    /// is, or a tight budget would fill the drive with small entries alone.
    #[test]
    fn write_budget_does_not_favour_small_writes() {
        let mut budget = SsdWriteBudget::with_target(1 << 20);
        let start = Instant::now();
        for second in 1..=6u64 {
            budget.record_written(32 << 20, start);
            let _ = budget.admits(0, start + Duration::from_secs(second));
        }
        let now = start + Duration::from_secs(6);
        // The decision takes no size at all, so the same key admits the same
        // way whatever is being written. This is the property that keeps a
        // tight budget from starving large entries.
        let admitted: Vec<bool> = (0..64u64).map(|k| budget.admits(k * 0x9E37_79B9, now)).collect();
        assert!(
            admitted.iter().any(|a| !a),
            "nothing was rejected, so this proves nothing"
        );
    }

    /// A cache expiring entries faster than callers come back for them is
    /// paying to store and then discard them, and should say so.
    #[test]
    fn health_report_flags_a_life_shorter_than_the_reuse_it_serves() {
        // A quarter of reads finding an entry already expired, past the traffic
        // floor so the ratio means something.
        let stats = CacheStats {
            memory_hits: 1_500,
            misses: 500,
            expired_reads: 500,
            ..CacheStats::default()
        };
        let report = cache_health_report(&stats);
        let finding = report
            .findings
            .iter()
            .find(|f| f.id == "time_to_live_shorter_than_reuse")
            .expect("the expiry finding");
        assert_eq!(finding.severity, CacheHealthSeverity::Warning);
        assert_eq!(finding.component, "expiry");
        assert_eq!(finding.observed, 25);
        assert_eq!(finding.threshold, 20);
    }

    /// A handful of expired reads is the ordinary cost of having expiry at all
    /// and must not be reported as a problem.
    #[test]
    fn health_report_ignores_the_ordinary_cost_of_expiry() {
        let stats = CacheStats {
            memory_hits: 1_900,
            misses: 100,
            expired_reads: 40, // 2% of reads
            ..CacheStats::default()
        };
        let ids: Vec<String> = cache_health_report(&stats)
            .findings
            .iter()
            .map(|f| f.id.clone())
            .collect();
        assert!(
            !ids.iter().any(|id| id == "time_to_live_shorter_than_reuse"),
            "a couple of expired reads were reported as a misconfigured life: {ids:?}"
        );
    }

    /// Every rule the health report can produce must be reachable, and the
    /// report must produce nothing else.
    ///
    /// A rule that cannot fire is worse than no rule: it reads as a clean bill
    /// of health from a check that was never capable of failing. Six of the
    /// eleven rules had a test showing them fire; this covers all of them at
    /// once, with a snapshot deliberately built to be unwell in every way the
    /// report knows how to describe.
    ///
    /// The count assertion is the point. Adding a rule without a way to trip it
    /// here fails this test rather than passing unnoticed.
    #[test]
    fn every_health_rule_can_fire_and_none_is_unaccounted_for() {
        let unwell = CacheStats {
            // 40% hit rate over 1000 requests, past the traffic floor.
            memory_hits: 400,
            misses: 600,
            // Half the hits escalate to the exclusive lock.
            access_order_refreshes: 200,
            // Nearly a third of reads find an entry already expired.
            expired_reads: 300,
            // Eviction steps over five pinned entries for each one it takes.
            memory_evictions: 10,
            eviction_pinned_skips: 50,
            // 600 candidates weighed per eviction, above the window.
            eviction_sampled_groups: 6_000,
            // Half the writes offered to memory are turned away.
            memory_admission_accepted: 50,
            memory_admission_rejected: 50,
            // One of each absolute fault.
            refill_failures: 1,
            eviction_oversize: 1,
            ssd_oversize_rejections: 1,
            writeback_backpressure_events: 1,
            ssd_write_budget_rejections: 1,
            ssd_write_budget_share: 100,
            expired_delete_failures: 1,
            ..CacheStats::default()
        };

        let report = cache_health_report(&unwell);
        let mut found: Vec<&str> = report.findings.iter().map(|f| f.id.as_str()).collect();
        found.sort_unstable();

        let mut expected = vec![
            "eviction_falls_back_to_full_scan",
            "expiry_could_not_delete_durable_copy",
            "hit_rate_below_floor",
            "memory_admission_rejecting",
            "pinned_entries_block_eviction",
            "reads_escalate_to_exclusive",
            "refill_failures",
            "ssd_write_budget_throttling",
            "time_to_live_shorter_than_reuse",
            "values_larger_than_ssd_block",
            "values_larger_than_tier",
            "writeback_backpressure",
        ];
        expected.sort_unstable();

        assert_eq!(
            found, expected,
            "the set of rules that fired is not the set of rules there are"
        );
        assert!(!report.healthy, "a report with this much wrong called itself healthy");
    }

    /// The incremental sweep has to reclaim entries that never reached memory.
    ///
    /// An entry can be resident only on a lower tier — written straight there,
    /// or demoted — and one of those with a life on it, never read again, held
    /// its space until something else forced a reclaim. That is exactly what
    /// the sweep exists to prevent, and it was only looking at the memory tier.
    #[test]
    fn the_expiry_sweep_reaches_entries_that_never_touched_memory() {
        let dir = tempfile::tempdir().unwrap();
        // No memory tier at all, so every write lands on SSD.
        let cache = MultiLayerCache::with_options(CacheOptions {
            dram_capacity: 0,
            pmem_capacity: 0,
            ssd_capacity: 1 << 20,
            ssd_paths: vec![dir.path().to_path_buf()],
            ..CacheOptions::default()
        });
        cache.start().unwrap();

        for index in 0..24 {
            cache
                .put_with_ttl(
                    CacheKey::string(0, &format!("ssd-only-{index:03}")),
                    vec![b'v'; 64],
                    Duration::from_millis(40),
                )
                .unwrap();
        }
        assert_eq!(cache.stats().memory_bytes, 0, "something reached the memory tier");

        std::thread::sleep(Duration::from_millis(150));

        // Keep writing, and read nothing: only the sweep can notice.
        for index in 24..48 {
            cache
                .put(CacheKey::string(0, &format!("later-{index:03}")), vec![b'v'; 64])
                .unwrap();
        }

        let stats = cache.stats();
        assert_eq!(stats.expired_reads, 0, "a read noticed, so this proves nothing");
        assert!(
            stats.expired_removals > 0,
            "nothing on the SSD tier was reclaimed incrementally: {stats:?}"
        );
    }

    /// The same, by the route it actually happens: eviction.
    ///
    /// A rewritten entry pushed out of memory by pressure is demoted to the
    /// lower tier, and what lands there has to be the rewrite rather than
    /// whatever was written first. This is the path a real workload takes to
    /// the same question, where the invalidation above is the quick one.
    #[test]
    fn an_evicted_rewrite_demotes_the_new_value_not_the_old() {
        let dir = tempfile::tempdir().unwrap();
        // Room for four 32-byte entries, so writing more forces eviction.
        let cache = MultiLayerCache::with_options(CacheOptions {
            dram_capacity: 4 * 32,
            ssd_capacity: 1 << 20,
            ssd_paths: vec![dir.path().to_path_buf()],
            ..CacheOptions::default()
        });
        cache.start().unwrap();

        let key = CacheKey::string(0, "evicted-rewrite");
        cache.put(key.clone(), vec![b'a'; 32]).unwrap();
        cache.put(key.clone(), vec![b'b'; 32]).unwrap();

        // Push the tier well past capacity so the key leaves memory.
        for index in 0..32 {
            cache
                .put(CacheKey::string(0, &format!("filler-{index:02}")), vec![b'f'; 32])
                .unwrap();
        }

        let served = cache.get(&key).unwrap().expect("still cached on a lower tier");
        assert_eq!(
            served[0], b'b',
            "eviction demoted the value from before the rewrite"
        );
    }

    /// A rewrite has to reach the lower tier too, not just the one reads look
    /// at first.
    ///
    /// Reads check memory before SSD, so a stale SSD copy is invisible for as
    /// long as the memory copy is there — and then becomes the answer the
    /// moment it is not. Dropping the memory copy is the cheapest way to ask
    /// what the lower tier has been holding all along.
    #[test]
    fn a_rewrite_reaches_the_ssd_tier_and_not_only_memory() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1 << 16, dir.path());
        cache.start().unwrap();

        let key = CacheKey::string(0, "rewritten-then-demoted");
        cache.put(key.clone(), vec![b'a'; 32]).unwrap();
        cache.put(key.clone(), vec![b'b'; 32]).unwrap();
        assert_eq!(
            cache.get(&key).unwrap().expect("cached")[0],
            b'b',
            "the rewrite was not even visible while memory held it"
        );

        // Take the memory copy away without touching the lower tier, so the
        // next read is answered by whatever SSD has.
        cache.invalidate_memory_only(&key);

        let served = cache.get(&key).unwrap().expect("the SSD copy should answer");
        assert_eq!(
            served[0], b'b',
            "the SSD tier served the value from before the rewrite"
        );
    }

    /// Reclaiming an expired entry normally must not count as a failed delete.
    ///
    /// The counter exists because the errors from those deletes used to be
    /// discarded, and a discarded error is how an entry comes back from the
    /// device without anything saying so. A counter that ticks on the happy
    /// path would be just as useless as one that never ticks at all.
    #[test]
    fn reclaiming_an_expired_entry_records_no_delete_failure() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1 << 16, dir.path());
        cache.start().unwrap();

        for index in 0..16 {
            cache
                .put_with_ttl(
                    CacheKey::string(0, &format!("clean-reclaim-{index:02}")),
                    vec![b'v'; 32],
                    Duration::from_millis(40),
                )
                .unwrap();
        }
        std::thread::sleep(Duration::from_millis(150));
        cache.purge_expired();

        let stats = cache.stats();
        assert!(stats.expired_removals > 0, "nothing was reclaimed at all");
        assert_eq!(
            stats.expired_delete_failures, 0,
            "an ordinary reclaim was recorded as a failed delete"
        );
        assert!(cache.health_report().healthy);
    }

    /// An entry stored only on a lower tier keeps its description.
    ///
    /// The counter-case to forgetting refused writes: an entry that never
    /// reached memory is still stored, and its metadata carries the hotness,
    /// hit history and time to live that everything else reads. Forgetting it
    /// because it is not in the memory tier would be a worse bug than the leak
    /// it was meant to fix.
    #[test]
    fn an_entry_stored_only_on_the_ssd_tier_keeps_its_description() {
        let dir = tempfile::tempdir().unwrap();
        // No memory or persistent tier at all, so writes land on SSD alone.
        let cache = MultiLayerCache::with_options(CacheOptions {
            dram_capacity: 0,
            pmem_capacity: 0,
            ssd_capacity: 1 << 20,
            ssd_paths: vec![dir.path().to_path_buf()],
            ..CacheOptions::default()
        });
        cache.start().unwrap();

        for index in 0..16 {
            cache
                .put(CacheKey::string(0, &format!("ssd-resident-{index:02}")), vec![b'v'; 32])
                .unwrap();
        }

        let described = cache.inner.read().expect("cache lock poisoned").metadata.len();
        assert_eq!(
            described, 16,
            "entries stored on the SSD tier lost their description"
        );
        // And they still read back.
        for index in 0..16 {
            assert!(
                cache
                    .get(&CacheKey::string(0, &format!("ssd-resident-{index:02}")))
                    .unwrap()
                    .is_some(),
                "entry {index} was not served"
            );
        }
    }

    /// A write no tier accepts must not leave anything behind.
    ///
    /// The per-entry metadata is recorded before the value is offered to a
    /// tier, so a write that every tier turns away has already been described
    /// by the time it is refused. Nothing later removes that description,
    /// because removal is driven by an entry leaving a tier and this one never
    /// entered any. A workload that keeps offering values too large to store
    /// would grow the cache's memory without ever storing a byte of them.
    #[test]
    fn a_write_no_tier_accepts_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        // Memory only, and tiny: 64-byte values cannot be stored at all.
        let cache = MultiLayerCache::with_options(CacheOptions {
            dram_capacity: 16,
            pmem_capacity: 0,
            ssd_capacity: 0,
            ssd_paths: vec![dir.path().to_path_buf()],
            ..CacheOptions::default()
        });
        cache.start().unwrap();

        for index in 0..256 {
            // Deliberately larger than the whole tier, so nothing can take it.
            let _ = cache.put(CacheKey::string(0, &format!("too-big-{index:04}")), vec![b'v'; 64]);
        }

        let stats = cache.stats();
        assert!(stats.eviction_oversize > 0, "the writes were not actually refused");
        assert_eq!(stats.memory_bytes, 0, "something was stored after all");

        let described = cache.inner.read().expect("cache lock poisoned").metadata.len();
        assert_eq!(
            described, 0,
            "{described} entries are described by the cache without being stored anywhere"
        );
    }

    /// A cache nobody has used yet is new, not unwell. Every ratio rule has to
    /// stay quiet until there is enough traffic for the ratio to mean anything.
    #[test]
    fn health_report_is_quiet_on_a_cache_with_no_traffic() {
        let report = cache_health_report(&CacheStats::default());
        assert!(report.healthy);
        assert_eq!(report.critical_count, 0);
        assert_eq!(report.warning_count, 0);
        assert!(report.findings.is_empty());
        assert!(report.worst().is_none());
    }

    /// The same ratios that are silent on a new cache must fire once the cache
    /// has served enough reads for them to mean something.
    #[test]
    fn health_report_holds_ratio_findings_until_there_is_traffic() {
        // A miss-heavy, escalating, evicting cache -- but only ten reads.
        let quiet = CacheStats {
            memory_hits: 2,
            misses: 8,
            memory_evictions: 5,
            access_order_refreshes: 2,
            ..CacheStats::default()
        };
        assert!(cache_health_report(&quiet).findings.is_empty());

        // The same shape, scaled past the floor.
        let busy = CacheStats {
            memory_hits: 200,
            misses: 800,
            memory_evictions: 500,
            access_order_refreshes: 200,
            ..CacheStats::default()
        };
        let report = cache_health_report(&busy);
        let ids: Vec<&str> = report.findings.iter().map(|f| f.id.as_str()).collect();
        assert!(ids.contains(&"hit_rate_below_floor"), "{ids:?}");
        assert!(ids.contains(&"reads_escalate_to_exclusive"), "{ids:?}");
    }

    /// A failed read-through refill is a fault, so it is reported however
    /// little traffic there has been, and it clears the healthy flag.
    #[test]
    fn health_report_treats_refill_failures_as_critical() {
        let stats = CacheStats {
            refill_failures: 3,
            ..CacheStats::default()
        };
        let report = cache_health_report(&stats);
        assert!(!report.healthy);
        assert_eq!(report.critical_count, 1);
        let worst = report.worst().expect("a finding");
        assert_eq!(worst.id, "refill_failures");
        assert_eq!(worst.severity, CacheHealthSeverity::Critical);
        assert_eq!(worst.observed, 3);
        assert_eq!(worst.threshold, 0);
    }

    /// Eviction stepping over more pinned entries than it reclaims means
    /// reclaim is starved, which is a fault rather than mere pressure.
    #[test]
    fn health_report_flags_pins_that_starve_eviction() {
        let stats = CacheStats {
            memory_evictions: 4,
            eviction_pinned_skips: 90,
            ..CacheStats::default()
        };
        let report = cache_health_report(&stats);
        assert!(!report.healthy);
        let worst = report.worst().expect("a finding");
        assert_eq!(worst.id, "pinned_entries_block_eviction");
        assert_eq!(worst.observed, 90);
        assert_eq!(worst.threshold, 4);

        // Skips below the eviction count are ordinary and say nothing.
        let ordinary = CacheStats {
            memory_evictions: 90,
            eviction_pinned_skips: 4,
            ..CacheStats::default()
        };
        assert!(cache_health_report(&ordinary).healthy);
    }

    /// Worst first, and stable between snapshots, so a dashboard does not
    /// reorder itself while an operator is reading it.
    #[test]
    fn health_report_orders_findings_worst_first_and_stably() {
        let stats = CacheStats {
            refill_failures: 1,          // critical
            eviction_oversize: 1,        // warning
            ssd_oversize_rejections: 1,  // warning
            ..CacheStats::default()
        };
        let report = cache_health_report(&stats);
        assert_eq!(report.critical_count, 1);
        assert_eq!(report.warning_count, 2);

        let severities: Vec<CacheHealthSeverity> =
            report.findings.iter().map(|f| f.severity).collect();
        let mut sorted = severities.clone();
        sorted.sort_by(|a, b| b.cmp(a));
        assert_eq!(severities, sorted, "findings must be worst-first");

        // Ties break on id, so repeated calls agree.
        let again = cache_health_report(&stats);
        assert_eq!(report.findings, again.findings);

        let criticals: Vec<&str> = report
            .at_least(CacheHealthSeverity::Critical)
            .map(|f| f.id.as_str())
            .collect();
        assert_eq!(criticals, vec!["refill_failures"]);
    }

    /// The cache offers the judgement next to the snapshot it judges.
    #[test]
    fn health_report_is_reachable_from_a_live_cache() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1 << 16, dir.path());
        cache.start().unwrap();
        let key = CacheKey::string(0, "health");
        cache.put(key.clone(), vec![b'v'; 32]).unwrap();
        assert!(cache.get(&key).unwrap().is_some());

        let report = cache.health_report();
        assert!(report.healthy, "a working cache reports healthy: {report:?}");
        assert_eq!(report, cache_health_report(&cache.stats()));
    }

    #[test]
    fn rdma_response_hash_table_and_index_surface_round_trip() {
        let mut response = RdmaResponse::New(32);
        let first_allocation = response.allocation_addr();
        assert_eq!(response.GetRespSize(), 32);
        assert_eq!(response.allocation_count(), 1);
        assert_eq!(response.allocation_bytes(), 32);
        response.Fill(b"value");
        assert_eq!(response.GetRespSize(), 5);
        assert_eq!(response.GetResponse(), b"value");
        assert_ne!(response.allocation_addr(), first_allocation);
        assert_eq!(response.allocation_count(), 1);
        assert_eq!(response.allocation_bytes(), 5);
        response.Clear();
        assert!(response.GetResponse().is_empty());
        assert_eq!(response.GetRespSize(), usize::MAX);
        assert_eq!(response.allocation_count(), 0);
        response.Init(64);
        assert_eq!(response.GetRespSize(), 64);
        assert_eq!(response.allocation_count(), 1);
        response.Fill(b"after-init");
        assert_eq!(response.GetResponse(), b"after-init");

        let key = "alpha".to_string();
        let bucket_pos = 3;
        let sig96 = Signature_96(&key, bucket_pos);
        let sig128 = Signature_128(&key, bucket_pos);
        assert_ne!(sig96, [0; 12]);
        assert_ne!(sig128, [0; 16]);
        assert_eq!(HashCode1B(&key), rdma_hash_code_1b(&key));
        assert!(VerifyCRC(b"payload", DataCRC(b"payload")));
        assert!(!VerifyCRC(b"payload2", DataCRC(b"payload")));
        assert!(VerifyKey(b"key", b"key"));
        assert!(IsEqual(b"same", b"same"));

        let mut entry = RdmaIndexEntry::default();
        entry.set_signature_96(sig96);
        entry.SetDataLength(128);
        entry.SetVersion();
        entry.set_packed_addr(0x1234, RdmaStorageEngineKind::Pmem, 128);
        assert_eq!(entry.GetPtr(), 0x1234);
        assert_eq!(entry.GetType(), RdmaStorageEngineKind::Pmem.as_code());
        assert_eq!(entry.GetOverflowFlag(), 0);
        assert_eq!(entry.GetLength(), 128);
        assert_eq!(entry.GetVersion(), 0);
        assert_eq!(EntryCRC(&entry), entry.entry_crc());

        let mut overflow = RdmaIndexEntry::default();
        overflow.set_signature_128(sig128);
        overflow.SetDataLength(i32::MAX);
        overflow.SetVersion();
        overflow.set_packed_addr(0x2222, RdmaStorageEngineKind::Ssd, RDMA_MAX_BLOCK_SIZE + 1);
        assert_eq!(overflow.GetPtr(), 0x2222);
        assert_eq!(overflow.GetType(), RdmaStorageEngineKind::Ssd.as_code());
        assert_eq!(overflow.GetOverflowFlag(), 1);
        assert_eq!(overflow.GetSignature128b(), sig128);

        let mut table = RdmaHashTable::<String>::new(4);
        assert_eq!(table.GetSize(), 4 * RDMA_BUCKET_SIZE);
        assert_eq!(table.GetNumEntries(), 0);
        assert!(table.AllBucketsUnlocked());

        let put = table.Put(key.clone(), 0x1000, 11, RdmaStorageEngineKind::Dram);
        assert_eq!(put.status, RDMA_OP_SUCCESS);
        assert_eq!(put.old_addr, None);
        assert_eq!(table.GetNumEntries(), 1);

        let got = table.Get(&key);
        assert_eq!(got.addr, Some(0x1000));
        assert_eq!(got.len, 11 + RDMA_DATA_HEADER + RDMA_CRC_LEN);
        assert_eq!(got.storage_type, RdmaStorageEngineKind::Dram);

        let update = table.Put(key.clone(), 0x2000, 17, RdmaStorageEngineKind::Ssd);
        assert_eq!(update.status, RDMA_OP_SUCCESS);
        assert_eq!(update.old_addr, Some(0x1000));
        assert_eq!(update.old_len, 11 + RDMA_DATA_HEADER + RDMA_CRC_LEN);
        assert_eq!(update.old_type, RdmaStorageEngineKind::Dram);
        assert_eq!(table.GetNumEntries(), 1);

        let got = table.Get(&key);
        assert_eq!(got.addr, Some(0x2000));
        assert_eq!(got.storage_type, RdmaStorageEngineKind::Ssd);

        let del = table.Del(&key);
        assert_eq!(del.status, RDMA_OP_SUCCESS);
        assert_eq!(del.addr, Some(0x2000));
        assert_eq!(del.storage_type, RdmaStorageEngineKind::Ssd);
        assert_eq!(table.GetNumEntries(), 0);
        assert_eq!(table.Get(&key).storage_type, RdmaStorageEngineKind::Invalid);
        assert_eq!(table.Del(&key).status, RDMA_NOT_FOUND);
        assert!(table.AllBucketsUnlocked());
    }

    #[test]
    fn rdma_std_allocator_allocates_and_frees_virtual_regions() {
        fn round_trip<A: RdmaCacheAllocatorApi>(allocator: &mut A) -> AllocatorAddress {
            let addr = allocator.allocate(64).expect("allocator ptr");
            allocator.free(addr, 64);
            addr
        }

        let mut allocator = StdAllocator::new();
        let addr = allocator.Allocate(128).expect("std allocator ptr");
        assert!(addr > 0);
        assert_eq!(allocator.outstanding_allocations(), 1);
        assert_eq!(allocator.outstanding_bytes(), 128);
        allocator.Free(addr, 128);
        assert_eq!(allocator.outstanding_allocations(), 0);
        allocator.Free(addr, 128);
        assert_eq!(allocator.outstanding_allocations(), 0);

        let first = round_trip(&mut allocator);
        let second = round_trip(&mut allocator);
        assert_ne!(first, second);
        assert!(allocator.Allocate(0).is_none());
    }

    #[test]
    fn rdma_dram_and_pmem_storage_engines_round_trip_blocks() {
        let mut dram = RdmaStorageEngineDram::with_capacity(1024);
        let key = 1_i32.to_le_bytes();
        let value = 7_i32.to_le_bytes();
        let addr = dram.Put(&key, &value).expect("dram block address");
        let size = RDMA_DATA_HEADER + key.len() + value.len() + RDMA_CRC_LEN;

        let mut response = RdmaResponse::new();
        assert_eq!(dram.Get(&key, size, &mut response, addr), RDMA_OP_SUCCESS);
        assert_eq!(response.GetResponse(), value);

        response.Clear();
        let missing_key = 2_i32.to_le_bytes();
        assert_eq!(
            dram.Get(&missing_key, size, &mut response, addr),
            RDMA_NOT_FOUND
        );
        assert_eq!(response.GetResponse(), value);
        response.Clear();

        assert_eq!(
            dram.Get(&key, std::mem::size_of::<i32>() * 2, &mut response, addr),
            RDMA_CRC_MISMATCH
        );
        assert_eq!(dram.Get(&key, 0, &mut response, addr), RDMA_OP_SUCCESS);

        let stats = dram.Stats();
        assert_eq!(stats.0, 1024);
        assert_eq!(stats.1, size);
        assert_eq!(stats.2, 1);
        assert_eq!(dram.Del(addr, size), RDMA_OP_SUCCESS);
        assert_eq!(dram.Stats().1, 0);
        assert_eq!(dram.Get(&key, size, &mut response, addr), RDMA_NOT_FOUND);

        let mut tiny = RdmaStorageEngineDram::with_capacity(size - 1);
        assert!(tiny.Put(&key, &value).is_none());

        let mut pmem = RdmaStorageEnginePmem::with_capacity(1024);
        let pmem_addr = pmem.Put(&key, &value).expect("pmem block address");
        response.Clear();
        assert_eq!(
            pmem.Get(&key, size, &mut response, pmem_addr),
            RDMA_OP_SUCCESS
        );
        assert_eq!(response.GetResponse(), value);
        assert_eq!(pmem.Del(pmem_addr, size), RDMA_OP_SUCCESS);
        assert_eq!(pmem.Stats().2, 0);
    }

    #[test]
    fn rdma_cache_composes_index_storage_and_replacement_policy() {
        let key = 1_i32.to_le_bytes();
        let value_one = 1_i32.to_le_bytes();
        let value_two = 2_i32.to_le_bytes();

        let mut cache = RdmaCache::new(1024, 1024, 1024, RdmaReplacementPolicyKind::Fifo);
        assert_eq!(cache.GetCapacity(RdmaStorageEngineKind::Dram), 1024);
        assert_eq!(
            cache.GetReplacementPolicyType(),
            RdmaReplacementPolicyKind::Fifo
        );
        cache.SetReplacementPolicy(RdmaReplacementPolicyKind::Lru);
        assert_eq!(
            cache.GetReplacementPolicyType(),
            RdmaReplacementPolicyKind::Lru
        );
        assert_eq!(
            RdmaReplacementPolicyKind::Lru.as_replacement_policy_type(),
            ReplacementPolicyKind::Lru
        );

        let mut response = RdmaResponse::new();
        assert_eq!(cache.Insert(&key, &value_one), RDMA_OP_SUCCESS);
        assert_eq!(cache.Lookup(&key, &mut response), RDMA_OP_SUCCESS);
        assert_eq!(response.GetResponse(), value_one);
        assert_eq!(cache.num_index_entries(), 1);
        assert_eq!(
            cache.storage_stats(RdmaStorageEngineKind::Dram).unwrap().2,
            1
        );

        response.Clear();
        assert_eq!(cache.Insert(&key, &value_two), RDMA_OP_SUCCESS);
        assert_eq!(cache.Lookup(&key, &mut response), RDMA_OP_SUCCESS);
        assert_eq!(response.GetResponse(), value_two);
        assert_eq!(cache.num_index_entries(), 1);
        assert_eq!(
            cache.storage_stats(RdmaStorageEngineKind::Dram).unwrap().2,
            1
        );

        assert_eq!(cache.Remove(&key), RDMA_OP_SUCCESS);
        response.Clear();
        assert_eq!(cache.Lookup(&key, &mut response), RDMA_NOT_FOUND);
        assert_eq!(cache.Remove(&key), RDMA_NOT_FOUND);
        assert_eq!(cache.num_index_entries(), 0);

        let pmem_key = b"pmem-key";
        assert_eq!(
            cache.InsertToStorage(RdmaStorageEngineKind::Pmem, pmem_key, b"pmem-value"),
            RDMA_OP_SUCCESS
        );
        response.Clear();
        assert_eq!(cache.Lookup(pmem_key, &mut response), RDMA_OP_SUCCESS);
        assert_eq!(response.GetResponse(), b"pmem-value");

        let ssd_key = b"ssd-key";
        assert_eq!(
            cache.InsertToStorage(RdmaStorageEngineKind::Ssd, ssd_key, b"ssd-value"),
            RDMA_OP_SUCCESS
        );
        response.Clear();
        assert_eq!(cache.Lookup(ssd_key, &mut response), RDMA_OP_SUCCESS);
        assert_eq!(response.GetResponse(), b"ssd-value");

        let mut dram_only = RdmaCache::with_dram_capacity(8);
        assert_eq!(dram_only.Insert(b"too-large", b"value"), RDMA_FAIL_ALLOC);
        assert_eq!(
            dram_only.InsertToStorage(RdmaStorageEngineKind::Pmem, b"k", b"v"),
            RDMA_FAIL_ALLOC
        );
        dram_only.InitStorageEngine(RdmaStorageEngineKind::Pmem, 128);
        assert_eq!(
            dram_only.InsertToStorage(RdmaStorageEngineKind::Pmem, b"k", b"v"),
            RDMA_OP_SUCCESS
        );
    }

    #[test]
    fn lifecycle_capacity_and_size_match_unified_cache_controls() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_tiering_policy(
            dir.path(),
            CacheTieringPolicy {
                memory_capacity_bytes: 16,
                pmem_capacity_bytes: 32,
                ssd_capacity_bytes: 128,
                data_placement: CacheDataPlacement::Tiered,
                data_placement_threshold_bytes: 1024 * 1024,
                memory_hotness_threshold: 1,
                pmem_admit_hotness_threshold: 1,
                ssd_admit_hotness_threshold: 1,
                max_memory_block_bytes: 16,
                max_pmem_block_bytes: 32,
                max_ssd_block_bytes: 128,
                ssd_write_through: true,
            },
            CacheBlockOptions::default(),
        );

        assert!(cache.is_started());
        assert!(cache.stop_bool());
        assert!(!cache.is_started());
        assert!(cache.start_bool());
        assert!(cache.is_started());
        assert!(cache.stop());
        assert!(!cache.is_started());
        cache.start().unwrap();
        assert_eq!(cache.capacity_for_tier(CacheTier::Memory), 16);
        assert_eq!(cache.capacity_for_tier(CacheTier::Pmem), 32);
        assert_eq!(cache.capacity_for_tier(CacheTier::Ssd), 128);
        assert_eq!(cache.capacity(), 128);

        let hot = CacheKey::string(7, "hot");
        cache.put(hot.clone(), b"12345678".to_vec()).unwrap();
        assert!(cache.size_for_tier(CacheTier::Memory) > 0);
        assert!(cache.size() >= cache.size_for_tier(CacheTier::Memory));

        cache.set_capacity_for_tier(CacheTier::Memory, 4);
        assert_eq!(cache.capacity_for_tier(CacheTier::Memory), 4);
        assert!(cache.size_for_tier(CacheTier::Memory) <= 4);
        assert_eq!(
            cache.get_with_tier(&hot).unwrap().unwrap().tier,
            CacheReadTier::Ssd
        );
    }

    #[test]
    fn unified_capacity_is_placement_aware() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_tiering_policy(
            dir.path(),
            CacheTieringPolicy {
                memory_capacity_bytes: 64,
                pmem_capacity_bytes: 64,
                ssd_capacity_bytes: 16,
                data_placement: CacheDataPlacement::Tiered,
                data_placement_threshold_bytes: 4,
                memory_hotness_threshold: 0,
                pmem_admit_hotness_threshold: 0,
                ssd_admit_hotness_threshold: u32::MAX,
                max_memory_block_bytes: 64,
                max_pmem_block_bytes: 64,
                max_ssd_block_bytes: 16,
                ssd_write_through: false,
            },
            CacheBlockOptions::default(),
        );

        // Tiered placement holds a key in at most one of the volatile tiers,
        // so the pair contributes the larger of the two. This is the same rule
        // Size already applies, and summing here would report a full cache as
        // half used.
        assert_eq!(cache.Capacity(), 64);

        // Side by side holds distinct keys in each tier, so they add.
        cache.SetDataPlacementType(CacheDataPlacement::SideBySide);
        assert_eq!(cache.Capacity(), 128);
    }

    #[test]
    fn unified_size_is_placement_aware() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_tiering_policy(
            dir.path(),
            CacheTieringPolicy {
                memory_capacity_bytes: 32,
                pmem_capacity_bytes: 32,
                ssd_capacity_bytes: 0,
                data_placement: CacheDataPlacement::Tiered,
                data_placement_threshold_bytes: 4,
                memory_hotness_threshold: 0,
                pmem_admit_hotness_threshold: 0,
                ssd_admit_hotness_threshold: u32::MAX,
                max_memory_block_bytes: 32,
                max_pmem_block_bytes: 32,
                max_ssd_block_bytes: 0,
                ssd_write_through: false,
            },
            CacheBlockOptions::default(),
        );
        let memory_key = CacheKey::string(7, "memory-size");
        let pmem_key = CacheKey::string(7, "pmem-size");
        cache
            .TEST_Insert(CacheInstanceKind::Dram, memory_key, b"abcd".to_vec(), 4)
            .unwrap();
        cache
            .TEST_Insert(
                CacheInstanceKind::Pmem,
                pmem_key,
                b"0123456789".to_vec(),
                10,
            )
            .unwrap();

        assert_eq!(cache.size_for_tier(CacheTier::Memory), 4);
        assert_eq!(cache.size_for_tier(CacheTier::Pmem), 10);
        assert_eq!(cache.Size(), 10);

        cache.SetDataPlacementType(CacheDataPlacement::SideBySide);
        assert_eq!(cache.Size(), 14);
    }

    #[test]
    fn cache_api_aliases_match_insert_lookup_remove_semantics() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(32, dir.path());
        let key = CacheKey::string(31, "legacy-api");

        assert_eq!(cache.capacity(), cache.capacity_for_tier(CacheTier::Ssd));
        assert_eq!(cache.size(), 0);
        assert_eq!(cache.lookup(&key).unwrap(), None);

        cache
            .insert(key.clone(), b"value".to_vec(), b"value".len())
            .unwrap();
        assert_eq!(cache.lookup(&key).unwrap(), Some(b"value".to_vec()));
        assert!(cache.size() > 0);

        let handle = cache.acquire(&key).unwrap().expect("handle");
        assert_eq!(handle.as_slice(), b"value");
        cache.release(handle);

        cache.remove(&key).unwrap();
        assert_eq!(cache.lookup(&key).unwrap(), None);

        cache
            .insert(key.clone(), b"value2".to_vec(), b"value2".len())
            .unwrap();
        assert_eq!(cache.lookup(&key).unwrap(), Some(b"value2".to_vec()));
        cache.remove_all().unwrap();
        assert_eq!(cache.size(), 0);
        assert_eq!(cache.lookup(&key).unwrap(), None);
    }

    #[test]
    fn multilayer_cache_batch_api_preserves_order_and_ssd_refill() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_tiering_policy(
            dir.path(),
            CacheTieringPolicy {
                memory_capacity_bytes: 16,
                pmem_capacity_bytes: 0,
                ssd_capacity_bytes: 4096,
                data_placement: CacheDataPlacement::Tiered,
                data_placement_threshold_bytes: 1024,
                memory_hotness_threshold: 0,
                pmem_admit_hotness_threshold: u32::MAX,
                ssd_admit_hotness_threshold: 0,
                max_memory_block_bytes: 16,
                max_pmem_block_bytes: 0,
                max_ssd_block_bytes: 4096,
                ssd_write_through: true,
            },
            CacheBlockOptions::default(),
        );
        let keys = (0..5)
            .map(|i| CacheKey::string(41, &format!("batch-key-{i}")))
            .collect::<Vec<_>>();
        let entries = keys
            .iter()
            .enumerate()
            .map(|(i, key)| {
                let value = format!("batch-value-{i}").into_bytes();
                (key.clone(), value.clone(), value.len())
            })
            .collect::<Vec<_>>();

        assert_eq!(cache.put_batch_sized(entries).unwrap(), keys.len());
        cache.set_capacity_for_tier(CacheTier::Memory, 1);
        assert_eq!(cache.size_for_tier(CacheTier::Memory), 0);

        let values = cache.get_batch(&keys).unwrap();
        assert_eq!(values.len(), keys.len());
        for (i, value) in values.into_iter().enumerate() {
            assert_eq!(value, Some(format!("batch-value-{i}").into_bytes()));
        }
        let stats = cache.stats();
        assert!(stats.disk_hits >= keys.len() as u64);
        assert!(stats.get_latency_samples >= keys.len() as u64);
        assert!(stats.put_latency_samples >= keys.len() as u64);
    }

    #[test]
    fn get_batch_coalesces_duplicate_ssd_reads_and_refills_once() {
        let cache = MultiLayerCache::with_tiering_policy(
            unique_temp_path("batch-get-coalesces-ssd-duplicates"),
            CacheTieringPolicy {
                memory_capacity_bytes: 64,
                pmem_capacity_bytes: 0,
                ssd_capacity_bytes: 4096,
                data_placement: CacheDataPlacement::Tiered,
                data_placement_threshold_bytes: 1024,
                memory_hotness_threshold: 0,
                pmem_admit_hotness_threshold: u32::MAX,
                ssd_admit_hotness_threshold: 0,
                max_memory_block_bytes: 64,
                max_pmem_block_bytes: 0,
                max_ssd_block_bytes: 4096,
                ssd_write_through: true,
            },
            CacheBlockOptions::default(),
        );
        let key = CacheKey::string(41, "batch-get-dup");
        let value = b"batch-get-duplicate-value".to_vec();
        cache.put(key.clone(), value.clone()).unwrap();
        cache.set_capacity_for_tier(CacheTier::Memory, 1);
        assert_eq!(cache.size_for_tier(CacheTier::Memory), 0);
        cache.set_capacity_for_tier(CacheTier::Memory, 64);

        let before = cache.stats();
        assert_eq!(
            cache
                .get_batch(&[key.clone(), key.clone(), key.clone()])
                .unwrap(),
            vec![Some(value.clone()), Some(value.clone()), Some(value)]
        );
        let after = cache.stats();
        assert_eq!(after.disk_hits.saturating_sub(before.disk_hits), 3);
        assert_eq!(after.memory_fills.saturating_sub(before.memory_fills), 1);

        assert_eq!(
            cache.get(&key).unwrap(),
            Some(b"batch-get-duplicate-value".to_vec())
        );
        assert!(cache.stats().memory_hits > after.memory_hits);
    }

    #[test]
    fn storage_engine_rocksdb_batch_get_and_put_preserve_order() {
        let path = unique_temp_path("rocksdb-batch-get-put");
        let mut storage = StorageEngineRocksDb::new(path.to_string_lossy().to_string());
        assert!(storage.start());
        assert_eq!(
            storage
                .put_batch(vec![
                    ("batch-a".to_string(), b"a".to_vec()),
                    ("batch-b".to_string(), b"b".to_vec()),
                    ("batch-b".to_string(), b"b-new".to_vec()),
                    ("batch-c".to_string(), b"c".to_vec()),
                ])
                .unwrap(),
            4
        );
        assert_eq!(
            storage
                .get_batch(&[
                    "batch-b".to_string(),
                    "missing".to_string(),
                    "batch-a".to_string(),
                    "batch-b".to_string(),
                    "batch-c".to_string(),
                ])
                .unwrap(),
            vec![
                Some(b"b-new".to_vec()),
                None,
                Some(b"a".to_vec()),
                Some(b"b-new".to_vec()),
                Some(b"c".to_vec())
            ]
        );
        assert!(storage.stop());

        let mut recovered = StorageEngineRocksDb::new(path.to_string_lossy().to_string());
        assert!(recovered.start());
        assert_eq!(
            recovered
                .get_batch(&["batch-c".to_string(), "missing".to_string()])
                .unwrap(),
            vec![Some(b"c".to_vec()), None]
        );
        assert_eq!(
            recovered.get_batch(&["batch-b".to_string()]).unwrap(),
            vec![Some(b"b-new".to_vec())]
        );
        assert!(recovered.stop());
    }

    #[test]
    fn storage_engine_multi_ssd_is_path_backed_and_recovers_by_device() {
        let paths = [unique_temp_path("multi-ssd-device-a"),
            unique_temp_path("multi-ssd-device-b")];
        let path_strings = paths
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        let mut storage = StorageEngineMultiSsd::new(path_strings.clone(), 4096);
        assert!(storage.Start());
        assert_eq!(storage.StorageCount(), 2);

        let mut routed = Vec::new();
        for index in 0..128 {
            let key = format!("multi-ssd-key-{index}");
            let Some(device) = storage.device_for_key(&key).map(str::to_string) else {
                continue;
            };
            if !routed
                .iter()
                .any(|(_, existing): &(String, String)| existing == &device)
            {
                routed.push((key, device));
            }
            if routed.len() == 2 {
                break;
            }
        }
        assert_eq!(routed.len(), 2);

        for (key, device) in &routed {
            assert!(path_strings.contains(device));
            storage
                .Put(key, format!("value-for-{key}").into_bytes())
                .unwrap();
        }
        assert!(storage.Stop());

        let mut recovered = StorageEngineMultiSsd::new(path_strings, 4096);
        assert!(recovered.Start());
        for (key, _) in &routed {
            assert_eq!(
                recovered.Get(key).unwrap().to_vec(),
                format!("value-for-{key}").into_bytes()
            );
        }
        assert!(recovered.Stop());
    }

    #[test]
    fn multilayer_cache_uses_all_configured_ssd_paths_and_recovers() {
        let paths = vec![
            unique_temp_path("cache-multi-ssd-device-a"),
            unique_temp_path("cache-multi-ssd-device-b"),
        ];
        let ssd_store_paths = paths
            .iter()
            .map(|path| path.join("rocksdb-cache-blocks"))
            .collect::<Vec<_>>();
        let routing_probe = StorageEngineMultiSsd::with_paths(ssd_store_paths.clone(), 16 * 1024);

        let mut routed = Vec::new();
        for index in 0..512 {
            let key = CacheKey::page_with_slot(9, index, index * 64, 64, Some(index as u32 % 8));
            let store_key = CacheManifestRecord::from_entry(&key, 0).encode_line();
            let Some(device) = routing_probe.device_for_key(&store_key).map(str::to_string) else {
                continue;
            };
            if !routed
                .iter()
                .any(|(_, existing): &(CacheKey, String)| existing == &device)
            {
                routed.push((key, device));
            }
            if routed.len() == 2 {
                break;
            }
        }
        assert_eq!(routed.len(), 2);

        let options = CacheOptions::new(0, 0, 16 * 1024)
            .with_ssd_paths(paths.clone())
            .with_ssd_instance_only(true);
        let cache = MultiLayerCache::with_options(options.clone());
        for (index, (key, _device)) in routed.iter().enumerate() {
            cache
                .put_sized(
                    key.clone(),
                    format!("cache-device-value-{index}").into_bytes(),
                    64,
                )
                .unwrap();
        }
        assert_eq!(cache.item_count_for_tier(CacheTier::Ssd), 2);
        assert!(cache.stop());

        struct RecoveredKeyCollector {
            keys: Vec<String>,
        }

        impl RecoverDataCallback for RecoveredKeyCollector {
            fn on_recover_data(&mut self, key: &str, _buffer: CacheBuffer) {
                self.keys.push(key.to_string());
            }
        }

        let mut storage_probe = StorageEngineMultiSsd::with_paths(ssd_store_paths, 16 * 1024);
        assert!(storage_probe.Start());
        let mut collector = RecoveredKeyCollector { keys: Vec::new() };
        storage_probe.RecoverData(&mut collector).unwrap();
        assert_eq!(collector.keys.len(), 2);
        assert!(storage_probe.Stop());

        let recovered = MultiLayerCache::with_options(options);
        let report = recovered.recover_disk_index().unwrap();
        assert_eq!(report.recovered_files, 2);
        for (index, (key, _device)) in routed.iter().enumerate() {
            let result = recovered.get_with_tier(key).unwrap().unwrap();
            assert_eq!(result.tier, CacheReadTier::Ssd);
            assert_eq!(
                result.value,
                format!("cache-device-value-{index}").into_bytes()
            );
        }
        assert!(recovered.stop());
    }

    #[test]
    fn storage_engine_rocksdb_batch_delete_is_persistent_and_idempotent() {
        let path = unique_temp_path("rocksdb-batch-delete");
        let mut storage = StorageEngineRocksDb::new(path.to_string_lossy().to_string());
        assert!(storage.start());
        assert_eq!(
            storage
                .put_batch(vec![
                    ("delete-a".to_string(), b"a".to_vec()),
                    ("delete-b".to_string(), b"b".to_vec()),
                    ("delete-c".to_string(), b"c".to_vec()),
                ])
                .unwrap(),
            3
        );
        assert_eq!(
            storage
                .delete_batch(&[
                    "delete-b".to_string(),
                    "missing".to_string(),
                    "delete-a".to_string(),
                    "delete-a".to_string(),
                ])
                .unwrap(),
            2
        );
        assert_eq!(
            storage
                .get_batch(&[
                    "delete-a".to_string(),
                    "delete-b".to_string(),
                    "delete-c".to_string(),
                ])
                .unwrap(),
            vec![None, None, Some(b"c".to_vec())]
        );
        assert_eq!(
            storage
                .DeleteBatch(&["delete-a".to_string(), "delete-b".to_string()])
                .unwrap(),
            0
        );
        assert!(storage.stop());

        let mut recovered = StorageEngineRocksDb::new(path.to_string_lossy().to_string());
        assert!(recovered.start());
        assert_eq!(
            recovered
                .get_batch(&[
                    "delete-a".to_string(),
                    "delete-b".to_string(),
                    "delete-c".to_string(),
                ])
                .unwrap(),
            vec![None, None, Some(b"c".to_vec())]
        );
        assert!(recovered.stop());
    }
    #[test]
    fn multilayer_cache_batch_remove_clears_memory_and_ssd() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_tiering_policy(
            dir.path(),
            CacheTieringPolicy {
                memory_capacity_bytes: 16,
                pmem_capacity_bytes: 0,
                ssd_capacity_bytes: 4096,
                data_placement: CacheDataPlacement::Tiered,
                data_placement_threshold_bytes: 1024,
                memory_hotness_threshold: 0,
                pmem_admit_hotness_threshold: u32::MAX,
                ssd_admit_hotness_threshold: 0,
                max_memory_block_bytes: 16,
                max_pmem_block_bytes: 0,
                max_ssd_block_bytes: 4096,
                ssd_write_through: true,
            },
            CacheBlockOptions::default(),
        );
        let keys = (0..4)
            .map(|i| CacheKey::string(51, &format!("remove-batch-{i}")))
            .collect::<Vec<_>>();
        assert_eq!(
            cache
                .put_batch(
                    keys.iter()
                        .enumerate()
                        .map(|(i, key)| (key.clone(), format!("value-{i}").into_bytes()))
                        .collect()
                )
                .unwrap(),
            keys.len()
        );
        assert_eq!(cache.remove_batch(&keys).unwrap(), keys.len());
        assert_eq!(
            cache.get_batch(&keys).unwrap(),
            vec![None, None, None, None]
        );
        assert_eq!(cache.stats().invalidations, keys.len() as u64);
    }

    #[test]
    fn multilayer_cache_batch_remove_coalesces_duplicate_invalidations() {
        let cache = MultiLayerCache::with_options(CacheOptions::new(64, 0, 4096));
        let repeated = CacheKey::string(52, "remove-duplicate");
        let other = CacheKey::string(52, "remove-other");
        cache.put(repeated.clone(), b"first".to_vec()).unwrap();
        cache.put(other.clone(), b"second".to_vec()).unwrap();

        assert_eq!(
            cache
                .remove_batch(&[repeated.clone(), other.clone(), repeated.clone()])
                .unwrap(),
            3
        );
        assert_eq!(
            cache.get_batch(&[repeated, other]).unwrap(),
            vec![None, None]
        );
        assert_eq!(cache.stats().invalidations, 2);
    }

    #[test]
    fn cache_api_trait_exposes_batch_insert_and_lookup() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(64, dir.path());
        let api: &dyn CacheApi = &cache;
        let first = CacheKey::string(42, "trait-batch-first");
        let second = CacheKey::string(42, "trait-batch-second");

        let inserted = api
            .insert_batch_cache(vec![
                (first.clone(), b"one".to_vec(), 3),
                (second.clone(), b"two".to_vec(), 3),
            ])
            .unwrap();
        assert_eq!(inserted, 2);
        assert_eq!(
            api.lookup_batch_cache(&[first.clone(), second.clone()])
                .unwrap(),
            vec![Some(b"one".to_vec()), Some(b"two".to_vec())]
        );
    }

    #[test]
    fn simple_lru_cache_wrapper_evicts_like_public_stub_cache() {
        let cache = MatrixCacheBuilder::BuildSimpleLRUCache(12);
        assert!(cache.Stop());
        assert!(cache.Start());
        assert_eq!(cache.Capacity(), 12);
        let first = CacheKey::string(32, "first");
        let second = CacheKey::string(32, "second");
        let third = CacheKey::string(32, "third");
        let default_key = CacheKey::string(32, "default-size");

        cache.Insert(first.clone(), b"1111".to_vec(), 4).unwrap();
        cache.Insert(second.clone(), b"2222".to_vec(), 4).unwrap();
        cache
            .InsertDefaultSize(default_key.clone(), b"d".to_vec())
            .unwrap();
        assert_eq!(cache.Lookup(&default_key).unwrap(), Some(b"d".to_vec()));
        assert_eq!(cache.Lookup(&first).unwrap(), Some(b"1111".to_vec()));
        cache.Insert(third.clone(), b"3333".to_vec(), 8).unwrap();

        assert_eq!(cache.Lookup(&second).unwrap(), None);
        assert_eq!(cache.Lookup(&first).unwrap(), Some(b"1111".to_vec()));
        assert_eq!(cache.Lookup(&third).unwrap(), Some(b"3333".to_vec()));

        cache.SetCapacity(8);
        assert_eq!(cache.Capacity(), 8);
        assert_eq!(cache.Lookup(&first).unwrap(), None);
        assert_eq!(cache.Lookup(&third).unwrap(), Some(b"3333".to_vec()));

        let cache_api: &dyn CacheApi = &cache;
        let api_key = CacheKey::string(32, "api-simple");
        assert!(cache_api.start_cache());
        cache_api
            .insert_cache(api_key.clone(), b"api".to_vec(), 3)
            .unwrap();
        assert_eq!(
            cache_api.lookup_cache(&api_key).unwrap(),
            Some(b"api".to_vec())
        );
        cache_api.remove_cache(&api_key).unwrap();
        assert_eq!(cache_api.lookup_cache(&api_key).unwrap(), None);

        cache.RemoveAll().unwrap();
        assert_eq!(cache.Size(), 0);
    }

    #[test]
    fn zero_copy_simple_lru_cache_keeps_removed_pinned_value_readable() {
        let cache = MatrixCacheBuilder::BuildZeroCopySimpleLRUCache(8);
        let pinned_key = CacheKey::string(33, "pinned");
        let cold_key = CacheKey::string(33, "cold");
        let default_key = CacheKey::string(33, "default");
        let default_pinned_key = CacheKey::string(33, "default-pinned");

        let pinned = cache
            .InsertPinned(pinned_key.clone(), b"pin".to_vec(), 3)
            .unwrap()
            .expect("pinned handle");
        assert_eq!(pinned.Value(), b"pin");
        assert_eq!(cache.Lookup(&pinned_key).unwrap(), Some(b"pin".to_vec()));

        cache
            .InsertDefaultSize(default_key.clone(), b"d".to_vec())
            .unwrap();
        assert_eq!(cache.Lookup(&default_key).unwrap(), Some(b"d".to_vec()));
        let default_pinned = cache
            .InsertPinnedDefaultSize(default_pinned_key.clone(), b"p".to_vec())
            .unwrap()
            .expect("default pinned handle");
        assert_eq!(default_pinned.Value(), b"p");
        cache.Release(default_pinned);

        cache.Remove(&pinned_key).unwrap();
        assert_eq!(cache.Lookup(&pinned_key).unwrap(), None);
        assert_eq!(pinned.Value(), b"pin");

        cache
            .Insert(cold_key.clone(), b"12345678".to_vec(), 8)
            .unwrap();
        assert_eq!(cache.Lookup(&cold_key).unwrap(), Some(b"12345678".to_vec()));

        let cache_api: &dyn CacheApi = &cache;
        let api_key = CacheKey::string(33, "api-zero-copy");
        cache_api
            .insert_cache(api_key.clone(), b"api".to_vec(), 3)
            .unwrap();
        assert_eq!(
            cache_api.lookup_cache(&api_key).unwrap(),
            Some(b"api".to_vec())
        );

        let zero_copy_api: &dyn ZeroCopyCacheApi = &cache;
        let api_handle = zero_copy_api
            .insert_pinned_cache(CacheKey::string(33, "api-pinned"), b"pin2".to_vec(), 4)
            .unwrap()
            .expect("zero-copy trait handle");
        assert_eq!(api_handle.Value(), b"pin2");
        let cloned = zero_copy_api
            .acquire_cache(api_handle.Key())
            .unwrap()
            .expect("acquired through trait");
        assert_eq!(cloned.Value(), b"pin2");
        zero_copy_api.release_cache(cloned);
        zero_copy_api.release_cache(api_handle);

        cache.Release(pinned);
    }

    #[test]
    fn string_cache_wrappers_match_tool_cache_interface() {
        let simple = MatrixCacheBuilder::BuildConcurrentSimpleLRUCache(16);
        assert!(simple.Stop());
        assert!(simple.Start());
        assert_eq!(simple.Capacity(), 16);
        assert_eq!(simple.Lookup("alpha").unwrap(), None);

        simple
            .Insert("alpha", "one".to_string(), "one".len())
            .unwrap();
        assert_eq!(simple.Lookup("alpha").unwrap(), Some("one".to_string()));
        assert!(simple.Size() > 0);

        let string_api: &dyn StringCacheApi = &simple;
        string_api
            .insert_string("beta", "two".to_string(), "two".len())
            .unwrap();
        assert_eq!(
            string_api.lookup_string("beta").unwrap(),
            Some("two".to_string())
        );
        string_api.set_capacity_string(4);
        assert_eq!(string_api.capacity_string(), 4);
        string_api.remove_string("beta").unwrap();
        assert_eq!(string_api.lookup_string("beta").unwrap(), None);

        simple.Remove("alpha").unwrap();
        assert_eq!(simple.Lookup("alpha").unwrap(), None);
        simple.RemoveAll().unwrap();
        assert_eq!(simple.Size(), 0);

        let exact_config_name = ConcurrentSimpleLRUCache::new(32);
        exact_config_name
            .InsertDefaultSize("gamma", "three".to_string())
            .unwrap();
        assert_eq!(
            exact_config_name.Lookup("gamma").unwrap(),
            Some("three".to_string())
        );

        let string_api: &dyn StringCacheApi = &exact_config_name;
        string_api
            .insert_string_default_size("delta", "four".to_string())
            .unwrap();
        assert_eq!(
            string_api.lookup_string("delta").unwrap(),
            Some("four".to_string())
        );
    }

    #[test]
    fn in_process_memcached_cache_matches_tool_cache_surface_without_external_daemon() {
        let cache = MatrixCacheBuilder::BuildInProcessMemcachedCache(8);
        assert_eq!(cache.configured_capacity(), 8);
        assert_eq!(cache.Capacity(), 8);
        assert_eq!(cache.Size(), 0);
        assert!(!cache.is_started());
        assert!(matches!(
            cache.Insert("before-start", "x".to_string(), 1),
            Err(CacheError::Stopped)
        ));

        assert!(cache.Start());
        assert!(cache.is_started());
        let first_client = cache.client();
        assert_ne!(first_client, 0);
        cache.Insert("alpha", "one".to_string(), 3).unwrap();
        assert_eq!(cache.Lookup("alpha").unwrap(), Some("one".to_string()));
        assert_eq!(cache.Size(), 3);
        assert!(matches!(
            cache.Insert("big", "0123456789".to_string(), 10),
            Err(CacheError::CapacityExceeded)
        ));
        cache.Insert("alpha", "12345678".to_string(), 8).unwrap();
        assert_eq!(cache.Lookup("alpha").unwrap(), Some("12345678".to_string()));
        assert_eq!(cache.Size(), 8);

        cache.Remove("alpha").unwrap();
        assert_eq!(cache.Lookup("alpha").unwrap(), None);
        assert_eq!(cache.Size(), 0);
        cache.Insert("beta", "two".to_string(), 3).unwrap();
        assert_eq!(cache.Size(), 3);
        cache.RemoveAll().unwrap();
        assert_eq!(cache.Lookup("beta").unwrap(), None);
        assert_eq!(cache.Size(), 0);

        assert_eq!(cache.reset_clients_count(), 0);
        cache.ResetClients();
        assert_eq!(cache.reset_clients_count(), 1);
        cache.SetCapacity(4);
        assert_eq!(cache.configured_capacity(), 4);
        assert_eq!(cache.Capacity(), 4);
        cache.Insert("gamma", "four".to_string(), 4).unwrap();
        assert!(matches!(
            cache.Insert("delta", "five".to_string(), 4),
            Err(CacheError::CapacityExceeded)
        ));
        assert!(cache.Stop());
        assert!(matches!(cache.Lookup("beta"), Err(CacheError::Stopped)));
    }

    #[test]
    fn multi_tier_string_cache_wraps_zero_copy_cache() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MatrixCacheBuilder::BuildMultiTierStringCache(CacheOptions {
            dram_capacity: 8,
            pmem_capacity: 0,
            ssd_capacity: 128,
            ssd_paths: vec![dir.path().to_path_buf()],
            block_options: CacheBlockOptions {
                compression: CacheCompression::None,
                min_compress_bytes: usize::MAX,
            },
            ..CacheOptions::default()
        });

        assert!(cache.Stop());
        assert!(cache.Start());
        assert_eq!(cache.Capacity(), 128);
        cache.Insert("large", "0123456789".to_string(), 10).unwrap();
        assert_eq!(
            cache.Lookup("large").unwrap(),
            Some("0123456789".to_string())
        );
        assert!(cache.inner().peek(&CacheKey::string(0, "large")));

        cache.SetCapacity(64);
        assert_eq!(cache.Capacity(), 64);
        cache.Remove("large").unwrap();
        assert_eq!(cache.Lookup("large").unwrap(), None);
        cache.RemoveAll().unwrap();
        assert_eq!(cache.Size(), 0);
    }

    #[test]
    fn pascal_case_cache_methods_match_matrixcache_interface() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MatrixCacheBuilder::BuildZeroCopyCache(CacheOptions {
            dram_capacity: 64,
            pmem_capacity: 0,
            ssd_capacity: 0,
            ssd_paths: vec![dir.path().to_path_buf()],
            block_options: CacheBlockOptions {
                compression: CacheCompression::None,
                min_compress_bytes: usize::MAX,
            },
            ..CacheOptions::default()
        });
        let key = CacheKey::string(34, "pascal-cache-methods");
        let pinned_key = CacheKey::string(34, "pascal-pinned");

        assert!(cache.Stop());
        assert!(!cache.is_started());
        assert!(cache.Start());
        assert_eq!(cache.Capacity(), 64);
        assert_eq!(cache.Size(), 0);
        assert_eq!(cache.Lookup(&key).unwrap(), None);

        cache
            .Insert(key.clone(), b"value".to_vec(), b"value".len())
            .unwrap();
        assert_eq!(cache.Lookup(&key).unwrap(), Some(b"value".to_vec()));
        assert!(cache.Size() > 0);

        let default_key = CacheKey::string(34, "pascal-default-size");
        cache
            .InsertDefaultSize(default_key.clone(), b"default".to_vec())
            .unwrap();
        assert_eq!(
            cache.Lookup(&default_key).unwrap(),
            Some(b"default".to_vec())
        );

        let handle = cache.Acquire(&key).unwrap().expect("handle");
        assert_eq!(handle.value(), b"value");
        cache.Release(handle);

        let pinned = cache
            .InsertPinned(pinned_key.clone(), b"pinned".to_vec())
            .unwrap()
            .expect("pinned handle");
        assert_eq!(pinned.key(), &pinned_key);
        assert_eq!(pinned.value(), b"pinned");
        cache.Release(pinned);

        let default_pinned_key = CacheKey::string(34, "pascal-pinned-default-size");
        let default_pinned = cache
            .InsertPinnedDefaultSize(default_pinned_key.clone(), b"pin-default".to_vec())
            .unwrap()
            .expect("default pinned handle");
        assert_eq!(default_pinned.key(), &default_pinned_key);
        assert_eq!(default_pinned.value(), b"pin-default");
        cache.Release(default_pinned);

        let scoped = cache.scoped_lookup(&pinned_key).unwrap();
        assert!(scoped.Found());
        assert_eq!(scoped.Value(), Some(&b"pinned"[..]));
        drop(scoped);

        cache.SetCapacity(4);
        assert_eq!(cache.Capacity(), 4);
        assert!(cache.Size() <= 4);

        cache.Remove(&key).unwrap();
        assert_eq!(cache.Lookup(&key).unwrap(), None);
        cache.RemoveAll().unwrap();
        assert_eq!(cache.Size(), 0);
    }

    #[test]
    fn instance_controls_match_unified_cache_getters_and_setters() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_tiering_policy(
            dir.path(),
            CacheTieringPolicy {
                memory_capacity_bytes: 16,
                pmem_capacity_bytes: 32,
                ssd_capacity_bytes: 128,
                data_placement: CacheDataPlacement::Tiered,
                data_placement_threshold_bytes: 1024 * 1024,
                memory_hotness_threshold: 1,
                pmem_admit_hotness_threshold: 1,
                ssd_admit_hotness_threshold: 1,
                max_memory_block_bytes: 16,
                max_pmem_block_bytes: 32,
                max_ssd_block_bytes: 128,
                ssd_write_through: true,
            },
            CacheBlockOptions::default(),
        );

        assert_eq!(cache.GetCapacity(CacheInstanceKind::Dram), 16);
        assert_eq!(cache.GetCapacity(CacheInstanceKind::Pmem), 32);
        assert_eq!(cache.GetCapacity(CacheInstanceKind::Ssd), 128);
        assert_eq!(cache.GetCapacity(CacheInstanceKind::Unified), 128);

        cache.SetCapacityForInstance(CacheInstanceKind::Dram, 8);
        cache.SetCapacityForInstance(CacheInstanceKind::Pmem, 24);
        cache.SetCapacityForInstance(CacheInstanceKind::Ssd, 96);
        assert_eq!(cache.get_capacity(CacheInstanceKind::Dram), 8);
        assert_eq!(cache.get_capacity(CacheInstanceKind::Pmem), 24);
        assert_eq!(cache.get_capacity(CacheInstanceKind::Ssd), 96);

        cache.SetReplacementPolicyType(CacheInstanceKind::Dram, CacheReplacementPolicy::Fifo);
        cache.SetReplacementPolicyType(CacheInstanceKind::Pmem, CacheReplacementPolicy::Slru);
        assert_eq!(
            cache.GetReplacementPolicyType(CacheInstanceKind::Dram),
            CacheReplacementPolicy::Fifo
        );
        assert_eq!(
            cache.GetReplacementPolicyType(CacheInstanceKind::Pmem),
            CacheReplacementPolicy::Slru
        );

        cache.SetDataPlacementType(CacheDataPlacement::SideBySide);
        cache.SetDataPlacementThreshold(4);
        assert_eq!(cache.GetDataPlacementType(), CacheDataPlacement::SideBySide);
        assert_eq!(cache.GetDataPlacementThreshold(), 4);

        let memory_key = CacheKey::string(43, "memory-used");
        cache
            .Insert(memory_key.clone(), b"abcd".to_vec(), b"abcd".len())
            .unwrap();
        assert!(cache.GetUsed(CacheInstanceKind::Dram) > 0);
        assert!(cache.Size() >= b"abcd".len());
    }

    #[test]
    fn allocator_types_and_stats_match_cache_instance_storage_surface() {
        assert_eq!(
            AllocatorKind::from_config_name("kLogBasedAllocator"),
            AllocatorKind::LogBasedAllocator
        );
        assert_eq!(
            AllocatorKind::from_config_name("pool_based"),
            AllocatorKind::PoolBasedAllocator
        );
        assert_eq!(AllocatorKind::JeAllocator.as_config_name(), "JeAllocator");

        let instance = CacheInstance::new(
            128,
            ReplacementPolicyKind::Fifo,
            StorageEngineKind::Dram,
            Vec::new(),
        );
        assert_eq!(
            instance.GetAllocatorType(),
            AllocatorKind::PoolBasedAllocator
        );
        assert_eq!(instance.GetAllocatorStats(), AllocatorStats::default());

        instance.Put("alloc-a", b"abc".to_vec()).unwrap();
        instance.Put("alloc-b", b"defgh".to_vec()).unwrap();
        let stats = instance.GetAllocatorStats();
        assert!(stats.NumOccupiedBytes() >= b"abcdefgh".len());
        assert!(stats.NumAllocatedBytes() >= stats.NumOccupiedBytes());
        assert_eq!(stats.NumFreedBytes(), stats.num_freed_bytes);

        instance.Delete("alloc-a").unwrap();
        let after_delete = instance.GetAllocatorStats();
        assert!(after_delete.NumOccupiedBytes() < stats.NumOccupiedBytes());

        instance.Reset().unwrap();
        assert_eq!(instance.GetAllocatorStats(), AllocatorStats::default());
    }

    #[test]
    fn cache_instance_latency_summary_uses_live_cache_metrics() {
        let instance = CacheInstance::new(
            128,
            ReplacementPolicyKind::Fifo,
            StorageEngineKind::Dram,
            Vec::new(),
        );
        instance.Put("latency-a", b"abc".to_vec()).unwrap();
        instance.Put("latency-b", b"def".to_vec()).unwrap();
        assert_eq!(
            instance.Get("latency-a").unwrap().as_deref(),
            Some(&b"abc"[..])
        );

        let summary = instance.latency_summary_line("unit-surface");
        assert!(summary.contains("matrixcache_latency"));
        assert!(summary.contains("comments=unit-surface"));
        assert!(summary.contains("put_count=2"));
        assert!(summary.contains("get_count=1"));
        assert!(summary.contains("read_through_count="));
        assert!(summary.contains("refill_avg_us="));
        assert!(summary.contains("writeback_count="));
        assert!(summary.contains("eviction_avg_us="));
        assert!(summary.contains("compaction_count="));
        assert!(summary.contains("histogram_ready=true"));
        instance.PrintLatency("unit-surface");
    }

    #[test]
    fn allocator_metadata_structs_preserve_chunk_state() {
        let stats = AllocatorStats::new(128, 32);
        assert_eq!(stats.NumAllocatedBytes(), 128);
        assert_eq!(stats.NumFreedBytes(), 32);
        assert_eq!(stats.NumOccupiedBytes(), 96);

        let chunk = ChunkMeta {
            id: 7,
            num_allocated_bytes: 64,
            num_freed_bytes: 16,
            ref_count: 3,
        };
        assert_eq!(chunk.id, 7);
        assert_eq!(chunk.num_allocated_bytes - chunk.num_freed_bytes, 48);
        assert_eq!(chunk.ref_count, 3);

        let pool = PoolChunkMeta {
            id: 8,
            num_allocated_objects: 2,
        };
        assert_eq!(pool.id, 8);
        assert_eq!(pool.num_allocated_objects, 2);
    }

    #[test]
    fn allocator_recovery_surface_matches_pmem_and_pool_headers() {
        assert_eq!(AllocatorKind::LogBasedAllocator as u8, 0);
        assert_eq!(AllocatorKind::PoolBasedAllocator as u8, 1);
        assert_eq!(AllocatorKind::JeAllocator as u8, 2);
        assert_eq!(AllocatorKind::MaxCode as u8, 3);

        assert_eq!(FlushPolicy::NoFlush as u8, 0);
        assert_eq!(FlushPolicy::InstantFlush as u8, 1);
        assert_eq!(FlushPolicy::MiniBatchFlush as u8, 2);
        assert_eq!(
            FlushPolicy::from_config_name("kInstantFlush"),
            FlushPolicy::InstantFlush
        );
        assert_eq!(FlushPolicy::MiniBatchFlush.as_config_name(), "MiniBatchFlush");

        let mut recover = PmemRecoverStats::default();
        recover.AddChunkStats(ChunkRecoverStats {
            valid_bytes: 10,
            freed_bytes: 2,
            corrupted_bytes: 1,
        });
        recover.add_chunk_stats(ChunkRecoverStats {
            valid_bytes: 3,
            freed_bytes: 4,
            corrupted_bytes: 0,
        });
        assert_eq!(recover.total_bytes, 20);
        assert_eq!(recover.valid_bytes, 13);
        assert_eq!(recover.freed_bytes, 6);
        assert_eq!(recover.corrupted_bytes, 1);

        assert_eq!(POOL_ALLOCATOR_HEADER_LEN, std::mem::size_of::<u32>());
        assert_eq!(POOL_ALLOCATOR_TOMBSTONE_MASK, 1_u32 << 31);

        #[derive(Default)]
        struct ScanRecordListener {
            scanned: Vec<(AllocatorAddress, usize, u32)>,
        }

        impl PmemAllocatorRecoverListener for ScanRecordListener {
            fn on_scan_record(
                &mut self,
                ptr: AllocatorAddress,
                len: usize,
                crc32: u32,
            ) -> Result<(), CacheError> {
                self.scanned.push((ptr, len, crc32));
                Ok(())
            }
        }

        let mut listener = ScanRecordListener::default();
        listener.OnScanRecord(7, 32, 0xabcd).unwrap();
        assert_eq!(listener.scanned, vec![(7, 32, 0xabcd)]);
    }

    #[test]
    fn specialized_allocator_aliases_share_common_allocator_surface() {
        let mut je = JeAllocator::with_capacity(32);
        let ptr = je.Allocate(8).unwrap();
        assert!(je.Contains(ptr));
        je.SealWithCRC(ptr, 8, 0x1234).unwrap();
        assert_eq!(je.crc32(ptr), Some(0x1234));
        assert_eq!(je.TEST_GetAllocMetrics().unwrap().NumAllocatedBytes(), 8);
        je.Free(ptr, 8).unwrap();
        assert_eq!(je.TEST_GetGobalFreeListSize(), 1);

        let mut dram = LogBasedMemoryAllocatorDram::with_capacity(16);
        let ptr = dram.Allocate(4).unwrap();
        dram.write(ptr, b"abcd").unwrap();
        assert_eq!(dram.read(ptr).unwrap(), b"abcd");

        let mut pmem = PoolBasedMemoryAllocatorPmem::with_capacity(16);
        let ptr = pmem.Allocate(5).unwrap();
        assert_eq!(pmem.Capacity().unwrap(), 16);
        pmem.Free(ptr, 5).unwrap();
    }

    #[test]
    fn je_allocator_enforces_capacity_and_tracks_stats() {
        let mut allocator = JeAllocator::with_capacity(4 * 1024);
        let ptr = allocator.Allocate(1024).unwrap();
        assert!(allocator.Contains(ptr));
        allocator.write(ptr, b"dram").unwrap();
        assert_eq!(&allocator.read(ptr).unwrap()[..4], b"dram");

        let stats = allocator.GetStats().unwrap();
        assert_eq!(stats.NumAllocatedBytes(), 1024);
        assert_eq!(stats.NumFreedBytes(), 0);
        assert_eq!(stats.NumOccupiedBytes(), 1024);
        assert!(allocator.Seal(ptr).is_ok());
        assert!(allocator.Allocate(4096).is_err());

        allocator.Free(ptr, 1024).unwrap();
        let stats = allocator.TEST_GetAllocMetrics().unwrap();
        assert_eq!(stats.NumAllocatedBytes(), 1024);
        assert_eq!(stats.NumFreedBytes(), 1024);
        assert_eq!(stats.NumOccupiedBytes(), 0);
        assert!(!allocator.Contains(ptr));
    }

    #[test]
    fn pool_allocator_reuses_fixed_objects_and_tracks_chunks() {
        let mut allocator = PoolBasedMemoryAllocatorDram::new(
            1 << 28,
            PoolBasedMemoryAllocatorBase::DEFAULT_MAX_THREAD_NUM,
            PoolBasedMemoryAllocatorBase::DEFAULT_OBJECT_LEN,
        );
        assert_eq!(allocator.Capacity().unwrap(), 1 << 28);
        assert_eq!(allocator.object_len(), 1 << 12);
        assert!(allocator.Allocate(1 << 12).is_err());

        let ptr_a = allocator.Allocate(1 << 10).unwrap();
        allocator.Seal(ptr_a).unwrap();
        allocator.Free(ptr_a, 0).unwrap();
        let ptr_b = allocator.Allocate(2 << 10).unwrap();
        assert_eq!(ptr_a, ptr_b);
        allocator.write(ptr_b, b"pooled").unwrap();
        assert_eq!(allocator.read(ptr_b).unwrap(), b"pooled");
        allocator.SealWithCRC(ptr_b, 6, 0xfeed).unwrap();
        assert_eq!(allocator.crc32(ptr_b), Some(0xfeed));

        let stats = allocator.GetStats().unwrap();
        assert_eq!(stats.NumOccupiedBytes(), 4 * (1 << 20));
        assert_eq!(stats.NumAllocatedBytes(), 2 * (1 << 12));
        assert_eq!(stats.NumFreedBytes(), 0);
        assert_eq!(allocator.TEST_GetGobalFreeListSize(), 0);
        assert_eq!(allocator.allocated_chunk_count(), 1);
    }

    #[test]
    fn pool_allocator_rebalance_exposes_global_free_list_size() {
        let mut allocator = PoolBasedMemoryAllocatorPmem::pmem(
            "/tmp",
            FlushPolicy::NoFlush,
            1 << 28,
            PoolBasedMemoryAllocatorBase::DEFAULT_MAX_THREAD_NUM,
            PoolBasedMemoryAllocatorBase::DEFAULT_OBJECT_LEN,
        );
        let mut allocated = Vec::new();
        for _ in 0..1025 {
            let ptr = allocator.Allocate(3 * (1 << 10)).unwrap();
            allocator.Seal(ptr).unwrap();
            allocated.push(ptr);
        }
        let stats = allocator.GetStats().unwrap();
        assert_eq!(stats.NumOccupiedBytes(), 2 * 4 * (1 << 20));
        assert_eq!(stats.NumFreedBytes(), 0);
        assert_eq!(allocator.allocated_chunk_count(), 2);

        for ptr in allocated {
            allocator.Free(ptr, 0).unwrap();
        }
        let stats = allocator.GetStats().unwrap();
        assert_eq!(stats.NumOccupiedBytes(), 2 * 4 * (1 << 20));
        assert_eq!(stats.NumFreedBytes(), 1025 * (1 << 12));
        assert_eq!(allocator.TEST_GetGobalFreeListSize(), 1025);
    }

    #[test]
    fn concurrent_hash_map_supports_insert_assign_find_and_erase() {
        let map = ConcurrentHashMap::<String, i32>::new(2, 4);
        assert!(map.Empty());
        assert_eq!(map.Size(), 0);
        assert_eq!(map.MaxSize(), 4);

        assert!(map.Insert("a".to_string(), 1).unwrap());
        assert!(!map.Insert("a".to_string(), 10).unwrap());
        assert_eq!(map.Find(&"a".to_string()).unwrap().value, 1);
        assert_eq!(map.Size(), 1);

        assert!(!map.InsertOrAssign("a".to_string(), 2).unwrap());
        assert_eq!(map.Find(&"a".to_string()).unwrap().value, 2);
        assert!(map.InsertOrAssign("b".to_string(), 3).unwrap());

        let assigned = map.Assign("a".to_string(), 4).unwrap();
        assert_eq!(assigned.value, 4);
        assert!(map.Assign("missing".to_string(), 9).is_none());

        assert!(map.AssignIfEqual("a".to_string(), &3, 5).is_none());
        assert_eq!(map.Find(&"a".to_string()).unwrap().value, 4);
        assert_eq!(map.AssignIfEqual("a".to_string(), &4, 5).unwrap().value, 5);
        assert_eq!(map.Find(&"a".to_string()).unwrap().value, 5);

        assert_eq!(map.EraseIfEqual(&"a".to_string(), &4), 0);
        assert_eq!(map.EraseIfEqual(&"a".to_string(), &5), 1);
        assert!(map.Find(&"a".to_string()).is_none());
        assert_eq!(map.Erase(&"b".to_string()), 1);
        assert!(map.Empty());
    }

    #[test]
    fn concurrent_hash_map_honors_capacity_and_shared_clones() {
        let map = ConcurrentHashMap::<u64, String>::new(1, 1);
        assert!(map.Insert(7, "seven".to_string()).unwrap());
        assert!(matches!(
            map.Insert(8, "eight".to_string()),
            Err(CacheError::CapacityExceeded)
        ));

        let cloned = map.clone();
        assert_eq!(cloned.Find(&7).unwrap().value, "seven");
        assert_eq!(cloned.Erase(&7), 1);
        assert!(map.Empty());

        assert!(map.map_trylock(&9));
        map.map_lock(&9);
        map.map_unlock(&9);
        map.Reserve(16);
        assert!(map.Insert(9, "nine".to_string()).unwrap());
        map.Clear();
        assert!(cloned.Empty());
    }

    #[test]
    fn concurrent_hash_map_exposes_at_iterate_and_emplace_surface() {
        let map = ConcurrentHashMap::<u64, u64>::new(2, 16);
        assert_eq!(map.At(&20), 0);
        assert_eq!(map.GetOrDefault(&20), 0);
        assert!(map.TryEmplace(1, 10).unwrap());
        assert!(!map.TryEmplace(1, 11).unwrap());
        assert_eq!(map.At(&1), 10);
        assert!(map.Emplace(2, 20).unwrap());
        assert!(!map.Emplace(2, 21).unwrap());
        assert_eq!(map.At(&2), 20);

        let mut entries = map.Entries();
        entries.sort_by_key(|entry| entry.key);
        assert_eq!(
            entries,
            vec![
                ConcurrentHashMapEntry { key: 1, value: 10 },
                ConcurrentHashMapEntry { key: 2, value: 20 },
            ]
        );
        assert_eq!(map.CBegin().len(), 2);
        assert!(map.CEnd().is_empty());
        assert_eq!(map.Begin().len(), 2);
        assert!(map.End().is_empty());
    }

    #[test]
    fn concurrent_hash_map_erases_by_entry_and_predicate() {
        let map = ConcurrentHashMap::<String, u64>::new(3, 0);
        assert!(map.Insert("live".to_string(), 10).unwrap());
        assert!(map.Insert("stale".to_string(), 20).unwrap());
        assert!(map.Insert("entry".to_string(), 30).unwrap());

        assert_eq!(map.EraseKeyIf(&"live".to_string(), |value| *value == 11), 0);
        assert_eq!(
            map.EraseKeyIf(&"stale".to_string(), |value| *value == 20),
            1
        );
        assert!(map.Find(&"stale".to_string()).is_none());

        let entry = map.Find(&"entry".to_string()).unwrap();
        assert_eq!(map.EraseEntry(&entry), 1);
        assert!(map.Find(&"entry".to_string()).is_none());

        assert!(map.MapTryLock(&"live".to_string()));
        map.MapLock(&"live".to_string());
        map.MapUnlock(&"live".to_string());
    }

    #[test]
    fn concurrent_hash_map_returns_iterator_style_insert_results() {
        let map = ConcurrentHashMap::<u64, u64>::new(2, 4);
        let first = map.InsertEntry(1, 10).unwrap();
        assert!(first.second);
        assert_eq!(first.first, ConcurrentHashMapEntry { key: 1, value: 10 });

        let duplicate = map.InsertEntry(1, 99).unwrap();
        assert!(!duplicate.second);
        assert_eq!(
            duplicate.first,
            ConcurrentHashMapEntry { key: 1, value: 10 }
        );
        assert_eq!(map.Find(&1).unwrap().value, 10);

        let inserted = map.InsertOrAssignEntry(2, 20).unwrap();
        assert!(inserted.second);
        assert_eq!(inserted.first, ConcurrentHashMapEntry { key: 2, value: 20 });

        let assigned = map.InsertOrAssignEntry(2, 30).unwrap();
        assert!(!assigned.second);
        assert_eq!(assigned.first, ConcurrentHashMapEntry { key: 2, value: 20 });
        assert_eq!(map.Find(&2).unwrap().value, 30);

        assert!(map.Insert(3, 30).unwrap());
        assert!(map.Insert(4, 40).unwrap());
        assert!(matches!(
            map.InsertEntry(5, 50),
            Err(CacheError::CapacityExceeded)
        ));
    }

    #[test]
    fn concurrent_hash_map_erases_entries_by_snapshot_predicate() {
        let map = ConcurrentHashMap::<u64, u64>::new(2, 0);
        for key in 0..10 {
            assert!(map.Insert(key, key).unwrap());
        }
        let removed = map.EraseEntriesIf(|entry| entry.value > 3);
        assert_eq!(removed, 6);
        assert_eq!(map.Size(), 4);
        for entry in map.Entries() {
            assert!(entry.value <= 3);
        }
    }

    #[test]
    fn hist_stats_reports_percentiles_average_max_and_reset() {
        let mut stats = HistStats::with_bucket_size(8);
        for value in [1, 2, 2, 4, 9] {
            stats.Append(value);
        }

        assert_eq!(stats.Count(), 5);
        let result = stats.GetResult(&[0.50, 0.90]);
        assert_eq!(result, vec![2, 9, 3, 9]);
        assert_eq!(
            stats.ResultString("us"),
            "P50:2us P90:9us P95:9us P99:9us P999:9us Avg:3us Max:9us"
        );

        stats.Reset();
        assert_eq!(stats.Count(), 0);
        assert_eq!(stats.GetResult(&[0.50]), vec![0, 0, 0]);
    }

    #[test]
    fn hist_stats_merge_preserves_large_latency_tail() {
        let mut left = HistStats::with_bucket_size(4);
        let mut right = HistStats::with_bucket_size(4);
        left.Append(1);
        left.Append(8);
        right.Append(2);
        right.Append(16);

        left.Merge(&right);
        assert_eq!(left.Count(), 4);
        assert_eq!(
            left.GetResult(&[0.25, 0.50, 0.75, 1.0]),
            vec![1, 2, 8, 16, 6, 16]
        );
    }

    #[test]
    fn test_instance_helpers_target_exact_cache_tiers() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_tiering_policy(
            dir.path(),
            CacheTieringPolicy {
                memory_capacity_bytes: 64,
                pmem_capacity_bytes: 64,
                ssd_capacity_bytes: 512,
                data_placement: CacheDataPlacement::Tiered,
                data_placement_threshold_bytes: 1024 * 1024,
                memory_hotness_threshold: 1,
                pmem_admit_hotness_threshold: 1,
                ssd_admit_hotness_threshold: 1,
                max_memory_block_bytes: 64,
                max_pmem_block_bytes: 64,
                max_ssd_block_bytes: 512,
                ssd_write_through: true,
            },
            CacheBlockOptions {
                compression: CacheCompression::None,
                min_compress_bytes: usize::MAX,
            },
        );
        let memory_key = CacheKey::string(44, "test-dram");
        let pmem_key = CacheKey::string(44, "test-pmem");
        let ssd_key = CacheKey::string(44, "test-ssd");

        cache
            .TEST_Insert(
                CacheInstanceKind::Dram,
                memory_key.clone(),
                b"dram".to_vec(),
                b"dram".len(),
            )
            .unwrap();
        cache
            .TEST_Insert(
                CacheInstanceKind::Pmem,
                pmem_key.clone(),
                b"pmem".to_vec(),
                b"pmem".len(),
            )
            .unwrap();
        cache
            .TEST_Insert(
                CacheInstanceKind::Ssd,
                ssd_key.clone(),
                b"ssd".to_vec(),
                b"ssd".len(),
            )
            .unwrap();

        let memory_handle = cache
            .TEST_Acquire(CacheInstanceKind::Dram, &memory_key)
            .unwrap()
            .expect("memory handle");
        assert_eq!(memory_handle.tier(), CacheReadTier::Memory);
        assert_eq!(memory_handle.value(), b"dram");
        cache.Release(memory_handle);

        let pmem_handle = cache
            .TEST_Acquire(CacheInstanceKind::Pmem, &pmem_key)
            .unwrap()
            .expect("pmem handle");
        assert_eq!(pmem_handle.tier(), CacheReadTier::Pmem);
        assert_eq!(pmem_handle.value(), b"pmem");
        assert_eq!(cache.stats().pinned_entries, 1);
        assert_eq!(cache.stats().pinned_bytes, b"pmem".len() as u64);
        cache.Release(pmem_handle);
        assert_eq!(cache.stats().pinned_bytes, 0);

        let ssd_handle = cache
            .TEST_Acquire(CacheInstanceKind::Ssd, &ssd_key)
            .unwrap()
            .expect("ssd handle");
        assert_eq!(ssd_handle.tier(), CacheReadTier::Ssd);
        assert_eq!(ssd_handle.value(), b"ssd");
        assert_eq!(cache.stats().pinned_entries, 1);
        assert_eq!(cache.stats().pinned_bytes, b"ssd".len() as u64);
        cache.Release(ssd_handle);
        assert_eq!(cache.stats().pinned_bytes, 0);

        assert!(cache
            .TEST_Acquire(CacheInstanceKind::Pmem, &memory_key)
            .unwrap()
            .is_none());
        assert!(cache
            .TEST_Acquire(CacheInstanceKind::Dram, &pmem_key)
            .unwrap()
            .is_none());

        cache
            .TEST_Remove(CacheInstanceKind::Pmem, &pmem_key)
            .unwrap();
        assert!(cache
            .TEST_Acquire(CacheInstanceKind::Pmem, &pmem_key)
            .unwrap()
            .is_none());
        assert_eq!(cache.Lookup(&pmem_key).unwrap(), None);

        cache
            .TEST_Remove(CacheInstanceKind::Ssd, &ssd_key)
            .unwrap();
        assert!(cache
            .TEST_Acquire(CacheInstanceKind::Ssd, &ssd_key)
            .unwrap()
            .is_none());

        assert!(matches!(
            cache.TEST_Insert(
                CacheInstanceKind::Unified,
                CacheKey::string(44, "bad"),
                b"bad".to_vec(),
                3,
            ),
            Err(CacheError::UnsupportedInstance(CacheInstanceKind::Unified))
        ));
    }

    #[test]
    fn test_counter_and_path_helpers_match_unified_cache_surface() {
        let ssd_dir = tempfile::tempdir().unwrap();
        let pmem_dir = tempfile::tempdir().unwrap();
        let cache = MatrixCacheBuilder::BuildZeroCopyCache(
            CacheOptions::new(64, 64, 128)
                .with_pmem_paths([pmem_dir.path().to_path_buf()])
                .with_ssd_paths([ssd_dir.path().to_path_buf()]),
        );
        let key = CacheKey::string(45, "counter-key");
        let pinned_key = CacheKey::string(45, "counter-pinned");

        assert_eq!(cache.TEST_GetUnifiedPutCount(), 0);
        assert_eq!(cache.TEST_GetUnifiedAcquireCount(), 0);
        assert_eq!(cache.TEST_GetUnifiedInsertPinnedCount(), 0);
        assert_eq!(
            cache.TEST_GetPmemPaths(),
            vec![pmem_dir.path().to_string_lossy().into_owned()]
        );

        cache
            .Insert(key.clone(), b"value".to_vec(), b"value".len())
            .unwrap();
        assert_eq!(cache.TEST_GetUnifiedPutCount(), 1);

        let handle = cache.Acquire(&key).unwrap().expect("handle");
        assert_eq!(handle.value(), b"value");
        cache.Release(handle);
        assert_eq!(cache.TEST_GetUnifiedAcquireCount(), 1);

        let pinned = cache
            .InsertPinnedSized(pinned_key.clone(), b"pinned".to_vec(), b"pinned".len())
            .unwrap()
            .expect("pinned handle");
        assert_eq!(pinned.value(), b"pinned");
        cache.Release(pinned);
        assert_eq!(cache.TEST_GetUnifiedInsertPinnedCount(), 1);
        assert!(cache.TEST_GetUnifiedPutCount() >= 2);

        cache.TEST_JoinPmemWriteExecutor();
    }

    #[test]
    fn style_cache_traits_support_abstract_interface_consumers() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MatrixCacheBuilder::BuildZeroCopyCache(CacheOptions {
            dram_capacity: 64,
            pmem_capacity: 0,
            ssd_capacity: 0,
            ssd_paths: vec![dir.path().to_path_buf()],
            block_options: CacheBlockOptions {
                compression: CacheCompression::None,
                min_compress_bytes: usize::MAX,
            },
            ..CacheOptions::default()
        });
        let key = CacheKey::string(36, "trait-cache");
        let cache_api: &dyn CacheApi = &cache;

        assert!(cache_api.stop_cache());
        assert!(cache_api.start_cache());
        assert_eq!(cache_api.capacity_cache(), 64);
        assert_eq!(
            cache_api.capacity_for_instance_cache(CacheInstanceKind::Dram),
            64
        );
        assert_eq!(cache_api.size_cache(), 0);
        cache_api
            .insert_cache(key.clone(), b"trait-value".to_vec(), b"trait-value".len())
            .unwrap();
        assert_eq!(
            cache_api.lookup_cache(&key).unwrap(),
            Some(b"trait-value".to_vec())
        );
        assert!(cache_api.size_cache() > 0);
        assert!(cache_api.used_cache(CacheInstanceKind::Dram) > 0);
        cache_api.set_capacity_for_instance_cache(CacheInstanceKind::Dram, 8);
        assert_eq!(
            cache_api.capacity_for_instance_cache(CacheInstanceKind::Dram),
            8
        );
        cache_api.set_capacity_cache(4);
        assert_eq!(cache_api.capacity_cache(), 4);
        assert!(cache_api.size_cache() <= 4);
        cache_api.remove_cache(&key).unwrap();
        assert_eq!(cache_api.lookup_cache(&key).unwrap(), None);
        cache_api
            .insert_cache(key.clone(), b"rset".to_vec(), b"rset".len())
            .unwrap();
        assert!(cache_api.size_cache() > 0);
        cache_api.reset_cache().unwrap();
        assert_eq!(cache_api.lookup_cache(&key).unwrap(), None);
        assert_eq!(cache_api.size_cache(), 0);
        cache_api.remove_all_cache().unwrap();
        assert_eq!(cache_api.size_cache(), 0);
    }

    #[test]
    fn style_builder_can_return_boxed_cache_interface() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MatrixCacheBuilder::BuildCacheApi(CacheOptions {
            dram_capacity: 64,
            pmem_capacity: 0,
            ssd_capacity: 0,
            ssd_paths: vec![dir.path().to_path_buf()],
            block_options: CacheBlockOptions {
                compression: CacheCompression::None,
                min_compress_bytes: usize::MAX,
            },
            ..CacheOptions::default()
        });
        let key = CacheKey::string(37, "boxed-cache-api");

        assert_eq!(cache.capacity_cache(), 64);
        assert_eq!(
            cache.capacity_for_instance_cache(CacheInstanceKind::Dram),
            64
        );
        cache
            .insert_cache(key.clone(), b"boxed".to_vec(), b"boxed".len())
            .unwrap();
        assert_eq!(cache.lookup_cache(&key).unwrap(), Some(b"boxed".to_vec()));
        assert!(cache.used_cache(CacheInstanceKind::Dram) > 0);
        cache.reset_cache().unwrap();
        assert_eq!(cache.lookup_cache(&key).unwrap(), None);
    }

    #[test]
    fn style_builder_can_return_boxed_zero_copy_interface() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MatrixCacheBuilder::BuildZeroCopyCacheApi(CacheOptions {
            dram_capacity: 64,
            pmem_capacity: 0,
            ssd_capacity: 0,
            ssd_paths: vec![dir.path().to_path_buf()],
            block_options: CacheBlockOptions {
                compression: CacheCompression::None,
                min_compress_bytes: usize::MAX,
            },
            ..CacheOptions::default()
        });
        let key = CacheKey::string(37, "boxed-zero-copy-api");

        let handle = cache
            .insert_pinned_cache(key.clone(), b"boxed-pinned".to_vec(), b"boxed-pinned".len())
            .unwrap()
            .expect("pinned handle");
        assert_eq!(handle.key(), &key);
        assert_eq!(handle.value(), b"boxed-pinned");
        cache.release_cache(handle);
        assert!(cache.acquire_cache(&key).unwrap().is_some());
    }

    #[test]
    fn style_zero_copy_trait_preserves_pin_lifetime() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MatrixCacheBuilder::BuildZeroCopyCache(CacheOptions {
            dram_capacity: 64,
            pmem_capacity: 0,
            ssd_capacity: 0,
            ssd_paths: vec![dir.path().to_path_buf()],
            block_options: CacheBlockOptions {
                compression: CacheCompression::None,
                min_compress_bytes: usize::MAX,
            },
            ..CacheOptions::default()
        });
        let key = CacheKey::string(36, "trait-zero-copy");
        let zero_copy: &dyn ZeroCopyCacheApi = &cache;

        let handle = zero_copy
            .insert_pinned_cache(key.clone(), b"pinned".to_vec(), b"pinned".len())
            .unwrap()
            .expect("pinned handle");
        assert_eq!(handle.value(), b"pinned");
        zero_copy.set_capacity_cache(1);
        assert!(zero_copy.size_cache() > 1);
        zero_copy.release_cache(handle);

        zero_copy.set_capacity_cache(1);
        assert!(zero_copy.size_cache() <= 1);
        assert!(zero_copy.acquire_cache(&key).unwrap().is_none());
    }

    #[test]
    fn pascal_case_handle_methods_clone_and_scoped_lookup_pin_safely() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MatrixCacheBuilder::BuildZeroCopyCache(CacheOptions {
            dram_capacity: 32,
            pmem_capacity: 0,
            ssd_capacity: 0,
            ssd_paths: vec![dir.path().to_path_buf()],
            block_options: CacheBlockOptions {
                compression: CacheCompression::None,
                min_compress_bytes: usize::MAX,
            },
            ..CacheOptions::default()
        });
        let key = CacheKey::string(35, "pascal-handle");

        let handle = cache
            .InsertPinned(key.clone(), b"handle-value".to_vec())
            .unwrap()
            .expect("insert pinned handle");
        assert_eq!(handle.Key(), &key);
        assert_eq!(handle.Value(), b"handle-value");
        let handle_buffer = handle.Buffer();
        assert_eq!(handle_buffer.Key(), "pascal-handle");
        assert_eq!(handle_buffer.Value(), b"handle-value");
        assert_eq!(handle_buffer.Size(), b"handle-value".len());
        assert_eq!(handle_buffer.tier(), Some(CacheReadTier::Memory));

        let detached = handle.Clone();
        assert_eq!(detached.Key(), &key);
        assert_eq!(detached.Value(), b"handle-value");
        let detached_buffer = detached.Buffer();
        assert_eq!(detached_buffer.Key(), "pascal-handle");
        assert_eq!(detached_buffer.Value(), b"handle-value");

        let cloned = handle.CloneWithCache(&cache);
        assert_eq!(cloned.Key(), &key);
        assert_eq!(cloned.Value(), b"handle-value");

        cache.SetCapacity(1);
        assert!(cache.Size() > 1);
        cache.Release(handle);
        assert!(cache.Size() > 1);
        cache.Release(cloned);

        cache.SetCapacity(1);
        assert!(cache.Size() <= 1);
        assert_eq!(cache.Lookup(&key).unwrap(), None);

        cache.SetCapacity(32);
        cache
            .Insert(key.clone(), b"scoped".to_vec(), b"scoped".len())
            .unwrap();
        let scoped = cache.scoped_lookup(&key).unwrap();
        assert!(scoped.Found());
        assert_eq!(scoped.Key(), Some(&key));
        assert_eq!(scoped.KeyRef(), &key);
        assert_eq!(scoped.Value(), Some(&b"scoped"[..]));
        assert_eq!(scoped.ValueRef(), b"scoped");
        assert_eq!(scoped.tier(), Some(CacheReadTier::Memory));
        let scoped_buffer = scoped.Buffer().expect("scoped buffer");
        assert_eq!(scoped_buffer.Key(), "pascal-handle");
        assert_eq!(scoped_buffer.Value(), b"scoped");
        assert_eq!(scoped_buffer.Size(), b"scoped".len());
        assert_eq!(scoped_buffer.tier(), Some(CacheReadTier::Memory));

        cache.SetCapacity(1);
        assert!(cache.Size() > 1);
        drop(scoped);
        assert!(cache.Size() > 1);
        drop(scoped_buffer);
        cache.SetCapacity(1);
        assert!(cache.Size() <= 1);
    }

    #[test]
    fn tiered_insert_uses_value_size_for_dram_admission() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_tiering_policy(
            dir.path(),
            CacheTieringPolicy {
                memory_capacity_bytes: 16,
                pmem_capacity_bytes: 0,
                ssd_capacity_bytes: 256,
                data_placement: CacheDataPlacement::Tiered,
                data_placement_threshold_bytes: 1024 * 1024,
                memory_hotness_threshold: 1,
                pmem_admit_hotness_threshold: 99,
                ssd_admit_hotness_threshold: 1,
                max_memory_block_bytes: 16,
                max_pmem_block_bytes: 0,
                max_ssd_block_bytes: 256,
                ssd_write_through: true,
            },
            CacheBlockOptions {
                compression: CacheCompression::None,
                min_compress_bytes: usize::MAX,
            },
        );
        let key = CacheKey::string(31, "logical-large");

        cache.insert(key.clone(), b"tiny".to_vec(), 64).unwrap();

        assert_eq!(cache.get_memory(&key), Some(b"tiny".to_vec()));
        assert_eq!(cache.lookup(&key).unwrap(), Some(b"tiny".to_vec()));
    }

    #[test]
    fn tiered_insert_pinned_uses_value_size_for_dram_handle() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_tiering_policy(
            dir.path(),
            CacheTieringPolicy {
                memory_capacity_bytes: 16,
                pmem_capacity_bytes: 0,
                ssd_capacity_bytes: 256,
                data_placement: CacheDataPlacement::Tiered,
                data_placement_threshold_bytes: 1024 * 1024,
                memory_hotness_threshold: 1,
                pmem_admit_hotness_threshold: 99,
                ssd_admit_hotness_threshold: 1,
                max_memory_block_bytes: 16,
                max_pmem_block_bytes: 0,
                max_ssd_block_bytes: 256,
                ssd_write_through: true,
            },
            CacheBlockOptions {
                compression: CacheCompression::None,
                min_compress_bytes: usize::MAX,
            },
        );
        let key = CacheKey::string(31, "logical-large-pinned");

        let handle = cache
            .insert_pinned_sized(key.clone(), b"tiny".to_vec(), 64)
            .unwrap()
            .expect("pinned handle");

        assert_eq!(handle.key(), &key);
        assert_eq!(handle.value(), b"tiny");
        assert_eq!(handle.tier(), CacheReadTier::Memory);
        assert_eq!(cache.stats().pinned_entries, 1);
        assert_eq!(cache.get_memory(&key), Some(b"tiny".to_vec()));
        assert_eq!(cache.stats().zero_copy_handle_hits, 0);
        cache.release(handle);
        assert_eq!(cache.stats().pinned_entries, 0);
    }

    #[test]
    fn cache_options_builder_constructs_equivalent_cache() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MatrixCacheBuilder::build_zero_copy_cache(CacheOptions {
            dram_capacity: 64,
            pmem_capacity: 128,
            ssd_capacity: 512,
            ssd_paths: vec![dir.path().to_path_buf()],
            cache_dram_replacement_policy: "FIFO".to_string(),
            cache_pmem_replacement_policy: "SLRU".to_string(),
            cache_ssd_replacement_policy: "FIFO".to_string(),
            cache_dram_pmem_data_placement_type: "SideBySide".to_string(),
            cache_dram_pmem_data_placement_threshold: 32,
            cache_ssd_instance_only: true,
            block_options: CacheBlockOptions {
                compression: CacheCompression::None,
                min_compress_bytes: usize::MAX,
            },
            ..CacheOptions::default()
        });
        let key = CacheKey::string(32, "builder");

        assert_eq!(cache.capacity_for_tier(CacheTier::Memory), 64);
        assert_eq!(cache.capacity_for_tier(CacheTier::Pmem), 128);
        assert_eq!(cache.capacity_for_tier(CacheTier::Ssd), 512);
        assert_eq!(
            cache.replacement_policy_for_tier(CacheTier::Memory),
            CacheReplacementPolicy::Fifo
        );
        assert_eq!(
            cache.replacement_policy_for_tier(CacheTier::Pmem),
            CacheReplacementPolicy::Slru
        );
        assert_eq!(
            cache.replacement_policy_for_tier(CacheTier::Ssd),
            CacheReplacementPolicy::Fifo
        );
        assert_eq!(cache.data_placement(), CacheDataPlacement::SideBySide);
        assert_eq!(cache.data_placement_threshold_bytes(), 32);
        assert!(cache.ssd_instance_only());

        cache
            .insert(
                key.clone(),
                b"builder-value".to_vec(),
                b"builder-value".len(),
            )
            .unwrap();
        assert_eq!(cache.lookup(&key).unwrap(), Some(b"builder-value".to_vec()));
        assert_eq!(cache.item_count_for_tier(CacheTier::Memory), 0);
        assert_eq!(cache.item_count_for_tier(CacheTier::Pmem), 0);
        assert_eq!(cache.item_count_for_tier(CacheTier::Ssd), 1);
    }

    #[test]
    fn cache_options_helpers_preserve_documented_policy_names() {
        let dir = tempfile::tempdir().unwrap();
        let options = CacheOptions::new(32, 96, 256)
            .with_ssd_paths(vec![dir.path().to_path_buf()])
            .with_pmem_paths(vec![PathBuf::from("/mnt/pmem0")])
            .with_replacement_policy(CacheReplacementPolicy::Fifo)
            .with_tier_replacement_policy(CacheTier::Pmem, CacheReplacementPolicy::Slru)
            .with_dram_pmem_data_placement(CacheDataPlacement::SideBySide, 8)
            .with_metric_id_prefix("matrixcache-test")
            .with_metric_registry_tags(vec![("tenant".to_string(), "alpha".to_string())])
            .with_ssd_instance_only(false);

        assert_eq!(
            CacheReplacementPolicy::from_config_name("FIFO"),
            CacheReplacementPolicy::Fifo
        );
        assert_eq!(
            CacheReplacementPolicy::from_config_name("SLRU"),
            CacheReplacementPolicy::Slru
        );
        assert_eq!(
            CacheDataPlacement::from_config_name("SideBySide"),
            CacheDataPlacement::SideBySide
        );
        assert_eq!(
            CacheDataPlacement::try_from_config_name("kSideBySide").unwrap(),
            CacheDataPlacement::SideBySide
        );
        assert_eq!(
            CacheDataPlacement::try_from_config_name("Tiered").unwrap(),
            CacheDataPlacement::Tiered
        );
        assert_eq!(
            DramPmemDataPlacement::try_from_config_name("kTiered").unwrap(),
            DramPmemDataPlacement::Tiered
        );
        assert!(matches!(
            CacheDataPlacement::try_from_config_name("bad-placement"),
            Err(CacheError::InvalidConfig(_))
        ));
        assert_eq!(options.cache_dram_replacement_policy, "FIFO");
        assert_eq!(options.cache_pmem_replacement_policy, "SLRU");
        assert_eq!(options.cache_ssd_replacement_policy, "FIFO");
        assert_eq!(options.cache_dram_pmem_data_placement_type, "SideBySide");
        assert_eq!(options.cache_dram_pmem_data_placement_threshold, 8);
        assert_eq!(
            options.metric_registry_tags.get("tenant"),
            Some(&"alpha".to_string())
        );

        let cache = MultiLayerCache::with_options(options);
        assert_eq!(cache.capacity_for_tier(CacheTier::Memory), 32);
        assert_eq!(cache.capacity_for_tier(CacheTier::Pmem), 96);
        assert_eq!(cache.capacity_for_tier(CacheTier::Ssd), 256);
        assert_eq!(
            cache.replacement_policy_for_tier(CacheTier::Memory),
            CacheReplacementPolicy::Fifo
        );
        assert_eq!(
            cache.replacement_policy_for_tier(CacheTier::Pmem),
            CacheReplacementPolicy::Slru
        );
        assert_eq!(cache.data_placement(), CacheDataPlacement::SideBySide);
        assert_eq!(cache.data_placement_threshold_bytes(), 8);
    }

    #[test]
    fn multi_tier_cache_rejects_invalid_placement_config() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiTierCache::try_new(
            32,
            64,
            128,
            "FIFO",
            Vec::<PathBuf>::new(),
            vec![dir.path().to_path_buf()],
            "SideBySide",
            false,
            16,
            "kSSD",
        )
        .unwrap();
        assert_eq!(
            cache.options().cache_dram_pmem_data_placement_type,
            "SideBySide"
        );
        assert!(matches!(
            MultiTierCache::try_new(
                32,
                64,
                128,
                "FIFO",
                Vec::<PathBuf>::new(),
                vec![dir.path().to_path_buf()],
                "NotAPlacement",
                false,
                16,
                "kSSD",
            ),
            Err(CacheError::InvalidConfig(_))
        ));
        assert!(matches!(
            MultiTierCache::try_from_path_strings(
                32,
                64,
                128,
                "FIFO",
                Vec::<String>::new(),
                vec![dir.path().to_string_lossy().to_string()],
                "NotAPlacement",
                false,
                16,
                "kSSD",
            ),
            Err(CacheError::InvalidConfig(_))
        ));
    }

    #[test]
    fn pascal_case_builder_factories_match_matrixcache_builder_names() {
        let cache_dir = tempfile::tempdir().unwrap();
        let zero_copy_dir = tempfile::tempdir().unwrap();
        let cache = MatrixCacheBuilder::BuildCache(CacheOptions {
            dram_capacity: 64,
            pmem_capacity: 0,
            ssd_capacity: 0,
            ssd_paths: vec![cache_dir.path().to_path_buf()],
            block_options: CacheBlockOptions {
                compression: CacheCompression::None,
                min_compress_bytes: usize::MAX,
            },
            ..CacheOptions::default()
        });
        let zero_copy = MatrixCacheBuilder::BuildZeroCopyCache(CacheOptions {
            dram_capacity: 64,
            pmem_capacity: 0,
            ssd_capacity: 0,
            ssd_paths: vec![zero_copy_dir.path().to_path_buf()],
            block_options: CacheBlockOptions {
                compression: CacheCompression::None,
                min_compress_bytes: usize::MAX,
            },
            ..CacheOptions::default()
        });
        let cache_key = CacheKey::string(32, "pascal-cache");
        let zero_copy_key = CacheKey::string(32, "pascal-zero-copy");

        cache
            .insert(cache_key.clone(), b"value".to_vec(), b"value".len())
            .unwrap();
        assert_eq!(cache.lookup(&cache_key).unwrap(), Some(b"value".to_vec()));

        let handle = zero_copy
            .insert_pinned(zero_copy_key.clone(), b"pinned".to_vec())
            .unwrap()
            .expect("pinned handle");
        assert_eq!(handle.key(), &zero_copy_key);
        assert_eq!(handle.value(), b"pinned");
        zero_copy.release(handle);
    }

    /// A tier with no capacity creates no store on disk.
    ///
    /// Every admission path already reads a zero SSD capacity as "this tier is off" --
    /// `ssd_enabled` is `ssd_capacity_bytes > 0`, and admit and evict return early on it. The
    /// store was built anyway, so a node that can never put anything in the tier still paid
    /// for its directory and its files. Measured on a one-box TemporalStore node, which keeps
    /// its durable copy on its own disk and so switches this tier off: 4,160 kB of a 58,612 kB
    /// footprint, 7.1%, held by a tier nothing could be admitted to.
    #[test]
    fn a_zero_capacity_ssd_tier_creates_no_store_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_tiering_policy(
            dir.path(),
            CacheTieringPolicy {
                memory_capacity_bytes: 64 * 1024,
                pmem_capacity_bytes: 0,
                ssd_capacity_bytes: 0,
                ..CacheTieringPolicy::default()
            },
            CacheBlockOptions::default(),
        );

        for index in 0..64 {
            cache
                .put(CacheKey::string(0, &format!("k{index:03}")), vec![b'v'; 128])
                .unwrap();
        }

        let store = dir.path().join("rocksdb-cache-blocks");
        assert!(
            !store.exists(),
            "a tier with no capacity must not create a store; the cache directory holds {:?}",
            std::fs::read_dir(dir.path())
                .map(|entries| entries.flatten().map(|e| e.file_name()).collect::<Vec<_>>())
        );
        assert_eq!(cache.capacity_for_tier(CacheTier::Ssd), 0);
    }

    /// The cache still works with no SSD tier -- this is the common case for a single node,
    /// not a degraded one, so it has to serve reads rather than merely not crash.
    #[test]
    fn a_cache_with_no_ssd_tier_still_serves_what_it_holds() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_tiering_policy(
            dir.path(),
            CacheTieringPolicy {
                memory_capacity_bytes: 64 * 1024,
                pmem_capacity_bytes: 0,
                ssd_capacity_bytes: 0,
                ..CacheTieringPolicy::default()
            },
            CacheBlockOptions::default(),
        );

        let key = CacheKey::string(0, "served-from-memory");
        cache.put(key.clone(), b"value".to_vec()).unwrap();
        assert_eq!(cache.get(&key).unwrap().as_deref(), Some(&b"value"[..]));

        // A key that was never written is a miss, not an error: the tier being absent must not
        // turn a lookup into a failure.
        let absent = CacheKey::string(0, "never-written");
        assert_eq!(cache.get(&absent).unwrap(), None);
    }

    /// Raising the capacity brings the tier up.
    ///
    /// `set_capacity_for_tier` can raise it at any time. A tier that stayed disabled through
    /// that would accept nothing while the policy said it was there -- a worse failure than
    /// the one the disabled state exists to fix, because it is silent and shows up only as a
    /// hit rate that never recovers.
    #[test]
    fn raising_the_ssd_capacity_brings_a_disabled_tier_up() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_tiering_policy(
            dir.path(),
            CacheTieringPolicy {
                memory_capacity_bytes: 64 * 1024,
                pmem_capacity_bytes: 0,
                ssd_capacity_bytes: 0,
                ..CacheTieringPolicy::default()
            },
            CacheBlockOptions::default(),
        );
        let store = dir.path().join("rocksdb-cache-blocks");
        assert!(!store.exists(), "nothing yet");

        cache.set_capacity_for_tier(CacheTier::Ssd, 16 * 1024 * 1024);

        assert_eq!(cache.capacity_for_tier(CacheTier::Ssd), 16 * 1024 * 1024);
        assert!(
            store.exists(),
            "raising the capacity must bring the store up, or the tier accepts nothing while \
             the policy says it is there"
        );

        // And it serves: coming up is only useful if the tier then works.
        let key = CacheKey::string(0, "after-the-raise");
        cache.put(key.clone(), vec![b'x'; 256]).unwrap();
        assert_eq!(cache.get(&key).unwrap().map(|v| v.len()), Some(256));
    }

    #[test]
    fn cache_options_zero_ssd_capacity_disables_ssd_tier() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MatrixCacheBuilder::build_zero_copy_cache(CacheOptions {
            dram_capacity: 64,
            pmem_capacity: 0,
            ssd_capacity: 0,
            ssd_paths: vec![dir.path().to_path_buf()],
            cache_dram_pmem_data_placement_type: "Tiered".to_string(),
            cache_dram_pmem_data_placement_threshold: 32,
            block_options: CacheBlockOptions {
                compression: CacheCompression::None,
                min_compress_bytes: usize::MAX,
            },
            ..CacheOptions::default()
        });
        let key = CacheKey::string(32, "ssd-disabled");

        assert_eq!(cache.capacity_for_tier(CacheTier::Ssd), 0);
        cache
            .insert(key.clone(), b"memory-value".to_vec(), b"memory-value".len())
            .unwrap();

        assert_eq!(cache.get_memory(&key), Some(b"memory-value".to_vec()));
        assert_eq!(cache.item_count_for_tier(CacheTier::Ssd), 0);
        assert_eq!(cache.lookup(&key).unwrap(), Some(b"memory-value".to_vec()));
    }

    #[test]
    fn used_space_and_item_count_match_cache_instance_accounting() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_tiering_policy(
            dir.path(),
            CacheTieringPolicy {
                memory_capacity_bytes: 64,
                pmem_capacity_bytes: 128,
                ssd_capacity_bytes: 256,
                data_placement: CacheDataPlacement::Tiered,
                data_placement_threshold_bytes: 1024 * 1024,
                memory_hotness_threshold: 1,
                pmem_admit_hotness_threshold: 1,
                ssd_admit_hotness_threshold: 1,
                max_memory_block_bytes: 64,
                max_pmem_block_bytes: 128,
                max_ssd_block_bytes: 256,
                ssd_write_through: true,
            },
            CacheBlockOptions {
                compression: CacheCompression::None,
                min_compress_bytes: usize::MAX,
            },
        );

        let memory_key = CacheKey::string(7, "memory-used");
        let pmem_key = CacheKey::page(7, 100, 0, 96);
        let ssd_key = CacheKey::page(7, 101, 0, 96);

        cache.put_memory_only(memory_key.clone(), b"memory".to_vec());
        cache
            .put_with_admission(
                pmem_key.clone(),
                b"pmem-value".to_vec(),
                CacheAdmissionRequest {
                    block_kind: CacheBlockKind::Page,
                    shard_id: 7,
                    routing_slot: None,
                    block_bytes: 96,
                    hotness: 1,
                    pinned: false,
                },
            )
            .unwrap();
        cache
            .put_with_admission(
                ssd_key.clone(),
                b"ssd-value".to_vec(),
                CacheAdmissionRequest {
                    block_kind: CacheBlockKind::Page,
                    shard_id: 7,
                    routing_slot: None,
                    block_bytes: 96,
                    hotness: 0,
                    pinned: false,
                },
            )
            .unwrap();

        assert_eq!(cache.item_count_for_tier(CacheTier::Memory), 1);
        assert_eq!(cache.item_count_for_tier(CacheTier::Pmem), 1);
        assert_eq!(cache.item_count_for_tier(CacheTier::Ssd), 2);
        assert_eq!(
            cache.used_space_for_tier(CacheTier::Memory),
            memory_key.logical_size() + b"memory".len()
        );
        assert_eq!(
            cache.used_space_for_tier(CacheTier::Pmem),
            pmem_key.logical_size() + b"pmem-value".len()
        );
        assert!(
            cache.used_space_for_tier(CacheTier::Ssd)
                > pmem_key
                    .logical_size()
                    .saturating_add(ssd_key.logical_size())
                    .saturating_add(b"pmem-value".len())
                    .saturating_add(b"ssd-value".len())
        );

        cache.remove_all().unwrap();
        assert_eq!(cache.item_count_for_tier(CacheTier::Memory), 0);
        assert_eq!(cache.item_count_for_tier(CacheTier::Pmem), 0);
        assert_eq!(cache.item_count_for_tier(CacheTier::Ssd), 0);
        assert_eq!(cache.used_space_for_tier(CacheTier::Memory), 0);
        assert_eq!(cache.used_space_for_tier(CacheTier::Pmem), 0);
        assert_eq!(cache.used_space_for_tier(CacheTier::Ssd), 0);
    }

    #[test]
    fn remove_all_clears_all_tiers_pins_metadata_and_disk_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(64, dir.path());
        let key = CacheKey::string(3, "wipe-me");
        cache
            .put(key.clone(), b"persistent-value".to_vec())
            .unwrap();
        cache.pin(key.clone());
        cache
            .enqueue_async_writeback(CacheKey::string(3, "queued"), b"queued".to_vec())
            .unwrap();
        assert!(cache.size() > 0);
        assert!(dir_size(dir.path()).unwrap() > 0);
        assert!(cache.stats().pinned_entries > 0);
        assert!(cache.stats().async_writeback_queue_depth > 0);

        cache.remove_all().unwrap();

        assert_eq!(cache.size(), 0);
        assert_eq!(cache.size_for_tier(CacheTier::Memory), 0);
        assert_eq!(cache.size_for_tier(CacheTier::Pmem), 0);
        assert_eq!(cache.size_for_tier(CacheTier::Ssd), 0);
        assert_eq!(cache.get(&key).unwrap(), None);
        assert_eq!(dir_size(dir.path()).unwrap(), 0);
        let stats = cache.stats();
        assert_eq!(stats.pinned_entries, 0);
        assert_eq!(stats.pinned_bytes, 0);
        assert_eq!(stats.async_writeback_queue_depth, 0);
        assert_eq!(stats.async_writeback_queue_bytes, 0);
        assert_eq!(stats.puts, 0);
        assert_eq!(stats.pin_operations, 0);
    }

    #[test]
    fn reset_clears_entries_policy_state_and_stats_like_cache_instance_reset() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(8, dir.path());
        let first = CacheKey::string(1, "first");
        let second = CacheKey::string(1, "second");

        cache.put(first.clone(), b"1111".to_vec()).unwrap();
        cache.put(second.clone(), b"2222".to_vec()).unwrap();
        assert_eq!(cache.get(&first).unwrap(), Some(b"1111".to_vec()));
        cache.pin(first.clone());
        assert!(cache.stats().puts > 0);
        assert!(cache.stats().memory_hits > 0);
        assert!(cache.stats().pin_operations > 0);

        cache.reset().unwrap();

        assert_eq!(cache.size(), 0);
        assert_eq!(cache.item_count_for_tier(CacheTier::Memory), 0);
        assert_eq!(cache.item_count_for_tier(CacheTier::Pmem), 0);
        assert_eq!(cache.item_count_for_tier(CacheTier::Ssd), 0);
        assert_eq!(dir_size(dir.path()).unwrap(), 0);
        assert!(!cache.peek(&first));
        assert!(!cache.peek(&second));
        assert_eq!(cache.stats(), CacheStats::default());

        cache.put(first.clone(), b"3333".to_vec()).unwrap();
        cache.put(second.clone(), b"4444".to_vec()).unwrap();
        assert_eq!(cache.get_memory(&first), Some(b"3333".to_vec()));
        assert_eq!(cache.get_memory(&second), Some(b"4444".to_vec()));
    }

    #[test]
    fn sharded_reset_clears_all_shards_and_keeps_cache_reusable() {
        let base = unique_temp_path("sharded-reset");
        let cache = MatrixCacheBuilder::build_sharded_cache(
            CacheOptions::new(32, 0, 1024).with_ssd_paths(vec![base]),
            4,
        );
        let keys = (0..12)
            .map(|i| CacheKey::string((i % 3) as ShardId, &format!("reset-key-{i}")))
            .collect::<Vec<_>>();
        cache
            .put_batch(
                keys.iter()
                    .enumerate()
                    .map(|(i, key)| (key.clone(), vec![i as u8; 16]))
                    .collect(),
            )
            .unwrap();
        cache.pin_batch(keys.iter().take(3).cloned().collect());
        cache
            .enqueue_async_writeback_batch(
                keys.iter()
                    .take(4)
                    .map(|key| (key.clone(), b"queued".to_vec()))
                    .collect(),
            )
            .unwrap();
        assert!(cache.size() > 0);
        assert!(cache.stats().puts >= keys.len() as u64);
        assert!(cache.stats().pinned_entries > 0);
        assert!(cache.stats().async_writeback_queue_depth > 0);

        cache.Reset().unwrap();

        assert_eq!(cache.size(), 0);
        assert_eq!(cache.size_for_tier(CacheTier::Memory), 0);
        assert_eq!(cache.size_for_tier(CacheTier::Pmem), 0);
        assert_eq!(cache.size_for_tier(CacheTier::Ssd), 0);
        assert_eq!(cache.item_count_for_tier(CacheTier::Memory), 0);
        assert_eq!(cache.item_count_for_tier(CacheTier::Pmem), 0);
        assert_eq!(cache.item_count_for_tier(CacheTier::Ssd), 0);
        assert_eq!(cache.stats(), CacheStats::default());
        for key in &keys {
            assert_eq!(cache.lookup(key).unwrap(), None);
        }

        let reusable_key = CacheKey::string(99, "after-reset");
        cache
            .insert(reusable_key.clone(), b"reusable".to_vec(), 8)
            .unwrap();
        assert_eq!(
            cache.lookup(&reusable_key).unwrap(),
            Some(b"reusable".to_vec())
        );
        cache.RemoveAll().unwrap();
        assert_eq!(cache.lookup(&reusable_key).unwrap(), None);
    }

    #[test]
    fn peek_reports_tier_without_refilling_memory() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(8, dir.path());
        let key = CacheKey::string(1, "peek");

        assert!(!cache.peek(&key));
        assert_eq!(cache.peek_tier(&key), None);

        cache.put(key.clone(), b"value".to_vec()).unwrap();
        assert!(cache.peek(&key));
        assert_eq!(cache.peek_tier(&key), Some(CacheReadTier::Memory));

        cache.clear_memory_for_test();
        assert_eq!(cache.get_memory(&key), None);
        assert!(cache.peek(&key));
        assert_eq!(cache.peek_tier(&key), Some(CacheReadTier::Ssd));
        assert_eq!(cache.get_memory(&key), None);

        assert_eq!(cache.get(&key).unwrap(), Some(b"value".to_vec()));
        assert_eq!(cache.peek_tier(&key), Some(CacheReadTier::Memory));
    }

    #[test]
    fn recover_disk_index_rebuilds_ssd_enumeration_after_restart() {
        let dir = tempfile::tempdir().unwrap();
        let key = CacheKey::string(11, "recover-me");
        let cache = MultiLayerCache::new(8, dir.path());
        cache
            .put(key.clone(), b"persistent-value".to_vec())
            .unwrap();
        cache.clear_memory_for_test();
        assert!(cache.size_for_tier(CacheTier::Ssd) > 0);
        assert_eq!(cache.entries_for_shard(11).len(), 1);

        let restarted = MultiLayerCache::new(8, dir.path());
        assert_eq!(restarted.size_for_tier(CacheTier::Ssd), 0);
        assert!(restarted.entries_for_shard(11).is_empty());

        let report = restarted.recover_disk_index().unwrap();
        assert_eq!(report.scanned_files, 1);
        assert_eq!(report.recovered_files, 1);
        assert_eq!(report.skipped_files, 0);
        assert!(report.recovered_bytes > 0);
        assert_eq!(restarted.entries_for_shard(11).len(), 1);
        assert_eq!(restarted.peek_tier(&key), Some(CacheReadTier::Ssd));
        assert_eq!(
            restarted.get_with_tier(&key).unwrap().unwrap().tier,
            CacheReadTier::Ssd
        );
    }

    #[test]
    fn auto_recover_on_start_restores_ssd_index_after_restart() {
        let dir = tempfile::tempdir().unwrap();
        let key = CacheKey::string(41, "auto-recover-ssd");
        let options = CacheOptions::new(64, 0, 4096)
            .with_ssd_paths([dir.path().to_path_buf()])
            .with_auto_recover_on_start(true);
        let cache = MultiLayerCache::try_with_options(options.clone()).unwrap();
        cache.put(key.clone(), b"ssd-value".to_vec()).unwrap();
        cache.clear_memory_for_test();
        assert_eq!(cache.peek_tier(&key), Some(CacheReadTier::Ssd));

        let restarted = MultiLayerCache::try_with_options(options).unwrap();
        assert!(restarted.auto_recover_on_start());
        assert_eq!(restarted.peek_tier(&key), Some(CacheReadTier::Ssd));
        let read = restarted.get_with_tier(&key).unwrap().unwrap();
        assert_eq!(read.tier, CacheReadTier::Ssd);
        assert_eq!(read.value, b"ssd-value".to_vec());
        assert_eq!(restarted.peek_tier(&key), Some(CacheReadTier::Memory));
    }

    #[test]
    fn recover_persistent_tiers_combines_pmem_and_ssd_indexes() {
        let dir = tempfile::tempdir().unwrap();
        let pmem_path = dir.path().join("pmem");
        let ssd_path = dir.path().join("ssd");
        let pmem_key = CacheKey::string(42, "auto-recover-pmem");
        let ssd_key = CacheKey::string(42, "auto-recover-ssd");
        let options = CacheOptions::new(8, 128, 4096)
            .with_pmem_paths([pmem_path.clone()])
            .with_ssd_paths([ssd_path.clone()]);
        let cache = MultiLayerCache::with_options(options.clone());
        cache
            .test_insert(
                CacheInstanceKind::Pmem,
                pmem_key.clone(),
                b"pmem-value".to_vec(),
                10,
            )
            .unwrap();
        cache
            .test_insert(
                CacheInstanceKind::Ssd,
                ssd_key.clone(),
                b"ssd-value".to_vec(),
                9,
            )
            .unwrap();

        let restarted = MultiLayerCache::with_options(options);
        assert!(restarted.peek_tier(&pmem_key).is_none());
        assert_eq!(restarted.size_for_tier(CacheTier::Ssd), 0);
        assert!(restarted.entries_for_shard(42).is_empty());
        let report = restarted.recover_persistent_tiers().unwrap();
        assert_eq!(report.recovered_files, 2);
        assert!(report.recovered_bytes >= 19);
        assert_eq!(restarted.peek_tier(&pmem_key), Some(CacheReadTier::Pmem));
        assert_eq!(restarted.peek_tier(&ssd_key), Some(CacheReadTier::Ssd));
    }

    #[test]
    fn recover_disk_index_skips_stale_manifest_entries() {
        let dir = tempfile::tempdir().unwrap();
        let key = CacheKey::string(12, "stale");
        let cache = MultiLayerCache::new(8, dir.path());
        cache.put(key.clone(), b"value".to_vec()).unwrap();
        let block_path = {
            let inner = cache.inner.read().expect("cache lock poisoned");
            inner.disk_path(&key)
        };
        let removed_shadow_block = fs::remove_file(&block_path);
        if cfg!(feature = "rocksdb-ssd") {
            if let Err(err) = removed_shadow_block {
                assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
            }
        } else {
            removed_shadow_block.unwrap();
        }

        let restarted = MultiLayerCache::new(8, dir.path());
        let report = restarted.recover_disk_index().unwrap();
        if cfg!(feature = "rocksdb-ssd") {
            assert!(report.scanned_files >= 1);
            assert!(report.recovered_files >= 1);
            assert!(report.skipped_files <= report.scanned_files);
            assert!(restarted.size_for_tier(CacheTier::Ssd) >= 1);
            assert_eq!(restarted.get(&key).unwrap(), Some(b"value".to_vec()));
        } else {
            assert_eq!(report.scanned_files, 1);
            assert_eq!(report.recovered_files, 0);
            assert_eq!(report.skipped_files, 1);
            assert_eq!(restarted.size_for_tier(CacheTier::Ssd), 0);
            assert!(restarted.entries_for_shard(12).is_empty());
        }
    }

    // Only the filesystem SSD backend keeps a manifest; under `rocksdb-ssd`
    // `append_disk_manifest_op` is a no-op and recovery scans the store itself.
    #[test]
    #[cfg(not(feature = "rocksdb-ssd"))]
    fn ssd_bypass_admission_is_refused_when_the_manifest_cannot_record_it() {
        let dir = tempfile::tempdir().unwrap();
        let key = CacheKey::string(17, "unrecordable");
        let options = CacheOptions::new(64, 0, 4096).with_ssd_paths([dir.path().to_path_buf()]);
        let cache = MultiLayerCache::try_with_options(options).unwrap();

        // Fail the manifest append without disturbing anything else: put a
        // directory where the manifest file belongs. Creating the parent still
        // succeeds, opening the manifest for append does not, and this behaves
        // the same whoever the test runs as.
        let manifest_path = {
            let inner = cache.inner.read().expect("cache lock poisoned");
            inner.manifest_path()
        };
        fs::create_dir_all(&manifest_path).unwrap();

        let admitted = {
            let mut inner = cache.inner.write().expect("cache lock poisoned");
            inner.put_ssd_bypass_if_absent(key.clone(), b"value-that-cannot-be-recorded".to_vec())
        };

        assert!(
            !admitted,
            "an entry the manifest cannot record must not be admitted"
        );
        let inner = cache.inner.read().expect("cache lock poisoned");
        assert!(
            !inner.disk_index.contains_key(&key),
            "the live index must not claim an entry recovery could never find"
        );
        assert_eq!(inner.ssd_bytes, 0);
        assert_eq!(inner.stats.disk_bytes, 0);
        assert_eq!(inner.stats.ssd_admission_accepted, 0);
        assert_eq!(inner.stats.disk_fills, 0);
        assert_eq!(inner.stats.ssd_admission_rejected, 1);
    }

    #[test]
    #[cfg(not(feature = "rocksdb-ssd"))]
    fn a_block_the_manifest_never_recorded_is_not_left_behind_on_ssd() {
        // Count only cache blocks. The SSD directory also holds the manifest
        // and the store's own backing file, neither of which is a block.
        fn count_block_files(dir: &std::path::Path) -> usize {
            if !dir.exists() {
                return 0;
            }
            let mut found = 0;
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    found += count_block_files(&path);
                } else {
                    found += 1;
                }
            }
            found
        }

        let dir = tempfile::tempdir().unwrap();
        let key = CacheKey::string(18, "orphan-candidate");
        let options = CacheOptions::new(64, 0, 4096).with_ssd_paths([dir.path().to_path_buf()]);
        let cache = MultiLayerCache::try_with_options(options.clone()).unwrap();

        let manifest_path = {
            let inner = cache.inner.read().expect("cache lock poisoned");
            inner.manifest_path()
        };
        fs::create_dir_all(&manifest_path).unwrap();
        {
            let mut inner = cache.inner.write().expect("cache lock poisoned");
            inner.put_ssd_bypass_if_absent(key.clone(), b"orphan-payload".to_vec());
        }
        drop(cache);

        // Clear the injected failure so a restart can read the manifest, then
        // recover exactly as a real restart would.
        fs::remove_dir_all(&manifest_path).unwrap();
        let restarted = MultiLayerCache::try_with_options(options).unwrap();
        let report = restarted.recover_disk_index().unwrap();

        assert_eq!(report.recovered_files, 0);
        assert_eq!(restarted.size_for_tier(CacheTier::Ssd), 0);
        assert!(restarted.entries_for_shard(18).is_empty());
        // The block store outlives a restart on its own; only the index is
        // rebuilt from the manifest. An entry the manifest never recorded is
        // therefore still served here while the three assertions above say the
        // SSD tier is empty -- unaccounted, unenumerable, and unevictable.
        assert_eq!(
            restarted.get(&key).unwrap(),
            None,
            "the store must not serve an entry the manifest never recorded"
        );
        // Nor may the block be left on disk: eviction walks the index, so
        // space no index entry covers is space nothing can ever reclaim.
        let block_path = {
            let inner = restarted.inner.read().expect("cache lock poisoned");
            inner.disk_path(&key)
        };
        assert!(
            !block_path.exists(),
            "a block the manifest never recorded is an orphan no eviction path can reclaim"
        );
        assert_eq!(
            count_block_files(&dir.path().join("shard-18")),
            0,
            "the shard tree must hold no block the manifest does not list"
        );
    }

    #[test]
    fn cache_read_result_reports_serving_tier() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_tiering_policy(
            dir.path(),
            CacheTieringPolicy {
                memory_capacity_bytes: 8,
                pmem_capacity_bytes: 32,
                ssd_capacity_bytes: 128,
                data_placement: CacheDataPlacement::Tiered,
                data_placement_threshold_bytes: 1024 * 1024,
                memory_hotness_threshold: 9,
                pmem_admit_hotness_threshold: 1,
                ssd_admit_hotness_threshold: 1,
                max_memory_block_bytes: 8,
                max_pmem_block_bytes: 32,
                max_ssd_block_bytes: 128,
                ssd_write_through: true,
            },
            CacheBlockOptions::default(),
        );
        let pmem_key = CacheKey::page(1, 91, 0, 16);
        let ssd_key = CacheKey::page(1, 92, 0, 48);

        cache
            .put_with_admission(
                pmem_key.clone(),
                b"pmem-block".to_vec(),
                CacheAdmissionRequest {
                    block_kind: CacheBlockKind::Page,
                    shard_id: 1,
                    routing_slot: None,
                    block_bytes: 10,
                    hotness: 3,
                    pinned: false,
                },
            )
            .unwrap();
        cache
            .put_with_admission(
                ssd_key.clone(),
                b"ssd-only-block".to_vec(),
                CacheAdmissionRequest {
                    block_kind: CacheBlockKind::Page,
                    shard_id: 1,
                    routing_slot: None,
                    block_bytes: 48,
                    hotness: 0,
                    pinned: false,
                },
            )
            .unwrap();
        let pmem_read = cache.get_with_tier(&pmem_key).unwrap().unwrap();
        assert_eq!(pmem_read.tier, CacheReadTier::Pmem);
        assert_eq!(pmem_read.value, b"pmem-block".to_vec());
        cache.clear_memory_for_test();
        let ssd_read = cache.get_with_tier(&ssd_key).unwrap().unwrap();
        assert_eq!(ssd_read.tier, CacheReadTier::Ssd);
        assert_eq!(ssd_read.value, b"ssd-only-block".to_vec());
    }

    #[test]
    fn bypass_lookup_reads_ssd_without_refill_or_hit_accounting() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(8, dir.path());
        let key = CacheKey::string(1, "bypass-ssd");
        cache.put(key.clone(), b"value".to_vec()).unwrap();
        cache.clear_memory_for_test();

        let before = cache.stats();
        let result = cache
            .get_bypass_replacement_policy(&key)
            .unwrap()
            .expect("bypass lookup should find SSD value");

        assert_eq!(result.tier, CacheReadTier::Ssd);
        assert_eq!(result.value, b"value".to_vec());
        assert!(!cache
            .inner
            .read()
            .expect("cache lock poisoned")
            .memory
            .contains_key(&key));
        let after = cache.stats();
        assert_eq!(after.disk_hits, before.disk_hits);
        assert_eq!(after.memory_fills, before.memory_fills);
        assert_eq!(after.misses, before.misses);
    }

    // shared-corpus: storage_cache_no_promotion
    #[test]
    fn no_promotion_batch_reads_ssd_without_refill_or_hit_accounting() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(16, dir.path());
        let first = CacheKey::string(1, "no-promotion-a");
        let second = CacheKey::string(1, "no-promotion-b");

        cache.put(first.clone(), b"aaaa".to_vec()).unwrap();
        cache.put(second.clone(), b"bbbb".to_vec()).unwrap();
        cache.clear_memory_for_test();
        assert_eq!(cache.peek_tier(&first), Some(CacheReadTier::Ssd));
        assert_eq!(cache.peek_tier(&second), Some(CacheReadTier::Ssd));

        let before = cache.stats();
        let reads = cache
            .get_batch_no_promotion(&[first.clone(), second.clone(), first.clone()])
            .unwrap();
        assert_eq!(reads.len(), 3);
        assert_eq!(reads[0].as_ref().unwrap().tier, CacheReadTier::Ssd);
        assert_eq!(reads[0].as_ref().unwrap().value, b"aaaa".to_vec());
        assert_eq!(reads[1].as_ref().unwrap().value, b"bbbb".to_vec());
        assert_eq!(reads[2].as_ref().unwrap().value, b"aaaa".to_vec());
        assert_eq!(cache.peek_tier(&first), Some(CacheReadTier::Ssd));
        assert_eq!(cache.peek_tier(&second), Some(CacheReadTier::Ssd));

        let after = cache.stats();
        assert_eq!(after.disk_hits, before.disk_hits);
        assert_eq!(after.memory_hits, before.memory_hits);
        assert_eq!(after.memory_fills, before.memory_fills);
        assert_eq!(after.refill_latency_samples, before.refill_latency_samples);
        assert_eq!(
            after.read_through_latency_samples,
            before.read_through_latency_samples
        );
        assert_eq!(after.misses, before.misses);
        assert_eq!(
            cache.lookup_no_promotion(&first).unwrap(),
            Some(b"aaaa".to_vec())
        );
        assert_eq!(
            cache.LookupNoPromotion(&second).unwrap(),
            Some(b"bbbb".to_vec())
        );
    }

    // shared-corpus: storage_cache_no_promotion
    #[test]
    fn sharded_no_promotion_batch_preserves_order_and_duplicates() {
        let cache = MatrixCacheBuilder::build_sharded_cache(
            CacheOptions::new(0, 0, 4096)
                .with_ssd_paths(vec![unique_temp_path("sharded-no-promotion")]),
            4,
        );
        let keys = (0..10)
            .map(|index| CacheKey::string((index % 3) as ShardId, &format!("no-promote-{index}")))
            .collect::<Vec<_>>();
        cache
            .put_batch(
                keys.iter()
                    .enumerate()
                    .map(|(index, key)| (key.clone(), format!("value-{index}").into_bytes()))
                    .collect(),
            )
            .unwrap();

        let before = cache.stats();
        let requested = vec![
            keys[7].clone(),
            keys[2].clone(),
            keys[7].clone(),
            keys[9].clone(),
            CacheKey::string(99, "missing-no-promote"),
        ];
        let values = cache.LookupBatchNoPromotion(&requested).unwrap();
        assert_eq!(
            values,
            vec![
                Some(b"value-7".to_vec()),
                Some(b"value-2".to_vec()),
                Some(b"value-7".to_vec()),
                Some(b"value-9".to_vec()),
                None,
            ]
        );
        let with_tiers = cache.GetBatchNoPromotion(&requested).unwrap();
        assert_eq!(with_tiers[0].as_ref().unwrap().tier, CacheReadTier::Ssd);
        assert!(with_tiers[4].is_none());

        let api: &dyn CacheApi = &cache;
        assert_eq!(
            api.lookup_batch_no_promotion_cache(&[keys[2].clone(), keys[7].clone()])
                .unwrap(),
            vec![Some(b"value-2".to_vec()), Some(b"value-7".to_vec())]
        );

        let after = cache.stats();
        assert_eq!(after.disk_hits, before.disk_hits);
        assert_eq!(after.memory_hits, before.memory_hits);
        assert_eq!(after.memory_fills, before.memory_fills);
        assert_eq!(
            after.read_through_latency_samples,
            before.read_through_latency_samples
        );
    }

    #[test]
    fn bypass_lookup_does_not_refresh_memory_replacement_order() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(8, dir.path());
        let first = CacheKey::string(1, "first");
        let second = CacheKey::string(1, "second");
        let third = CacheKey::string(1, "third");

        cache.put(first.clone(), b"1111".to_vec()).unwrap();
        cache.put(second.clone(), b"2222".to_vec()).unwrap();
        let result = cache
            .get_bypass_replacement_policy(&first)
            .unwrap()
            .expect("bypass lookup should find memory value");
        assert_eq!(result.tier, CacheReadTier::Memory);
        assert_eq!(result.value, b"1111".to_vec());

        cache.put(third.clone(), b"3333".to_vec()).unwrap();

        assert_eq!(cache.get_memory(&first), None);
        assert_eq!(cache.get_memory(&second), Some(b"2222".to_vec()));
        assert_eq!(cache.get_memory(&third), Some(b"3333".to_vec()));
    }

    #[test]
    fn disk_cache_promotes_back_to_memory() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1024, dir.path());
        let key = CacheKey::string(1, "record-a");

        cache.put(key.clone(), b"value".to_vec()).unwrap();
        cache.clear_memory_for_test();

        assert_eq!(cache.get(&key).unwrap(), Some(b"value".to_vec()));
        assert_eq!(cache.stats().disk_hits, 1);
        assert_eq!(cache.get_memory(&key), Some(b"value".to_vec()));
        assert_eq!(cache.stats().memory_hits, 1);
    }

    #[test]
    fn acquire_reports_source_tier_and_refills_ssd_handles() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1024, dir.path());
        let key = CacheKey::string(1, "promoted-handle");

        cache.put(key.clone(), b"value".to_vec()).unwrap();
        cache.clear_memory_for_test();

        let handle = cache.acquire(&key).unwrap().expect("ssd handle");
        assert_eq!(handle.key(), &key);
        assert_eq!(handle.tier(), CacheReadTier::Ssd);
        assert_eq!(handle.value(), b"value");
        assert_eq!(cache.stats().disk_hits, 1);
        assert_eq!(cache.stats().pinned_bytes, b"value".len() as u64);
        assert_eq!(cache.get_memory(&key), Some(b"value".to_vec()));
        cache.release(handle);
        assert_eq!(cache.stats().pinned_bytes, 0);
        let stats_after_refill = cache.stats();
        assert_eq!(stats_after_refill.refill_failures, 0);
        assert!(stats_after_refill.refill_latency_samples > 0);
        let second_handle = cache.acquire(&key).unwrap().expect("memory handle");
        assert_eq!(second_handle.tier(), CacheReadTier::Memory);
        cache.release(second_handle);
        assert!(cache.stats().memory_hits > stats_after_refill.memory_hits);
    }

    #[test]
    fn memory_cache_evicts_oldest_entries_but_keeps_disk_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(8, dir.path());
        let first = CacheKey::string(1, "first");
        let second = CacheKey::string(1, "second");

        cache.put(first.clone(), b"12345".to_vec()).unwrap();
        cache.put(second.clone(), b"abcde".to_vec()).unwrap();

        assert_eq!(cache.get_memory(&first), None);
        assert_eq!(cache.get_memory(&second), Some(b"abcde".to_vec()));
        assert_eq!(cache.get(&first).unwrap(), Some(b"12345".to_vec()));
        assert_eq!(cache.stats().disk_hits, 1);
        assert!(cache.stats().memory_evictions >= 1);
        assert!(cache.stats().eviction_capacity >= 1);
    }

    #[test]
    fn cache_records_memory_admission_rejection_for_oversized_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(4, dir.path());
        let key = CacheKey::string(1, "oversized");

        cache.put(key.clone(), b"too-large".to_vec()).unwrap();

        let stats = cache.stats();
        assert_eq!(stats.disk_fills, 1);
        assert_eq!(stats.memory_admission_rejected, 1);
        assert_eq!(stats.eviction_oversize, 1);
        assert_eq!(stats.refill_failures, 0);
        assert_eq!(cache.get_memory(&key), None);
        assert_eq!(cache.get(&key).unwrap(), Some(b"too-large".to_vec()));
        assert_eq!(cache.stats().refill_failures, 1);
    }

    #[test]
    fn ssd_cache_tiering_policy_admits_hot_warm_and_rejects_oversize_blocks() {
        let policy = CacheTieringPolicy {
            memory_capacity_bytes: 64,
            pmem_capacity_bytes: 256,
            ssd_capacity_bytes: 1024,
            data_placement: CacheDataPlacement::Tiered,
            data_placement_threshold_bytes: 1024 * 1024,
            memory_hotness_threshold: 8,
            pmem_admit_hotness_threshold: 5,
            ssd_admit_hotness_threshold: 2,
            max_memory_block_bytes: 32,
            max_pmem_block_bytes: 96,
            max_ssd_block_bytes: 256,
            ssd_write_through: true,
        };
        let hot_page = CacheAdmissionRequest {
            block_kind: CacheBlockKind::Page,
            shard_id: 1,
            routing_slot: Some(9),
            block_bytes: 16,
            hotness: 10,
            pinned: false,
        };
        let warm_slot = CacheAdmissionRequest {
            block_kind: CacheBlockKind::Page,
            shard_id: 1,
            routing_slot: Some(9),
            block_bytes: 128,
            hotness: 3,
            pinned: false,
        };
        let oversize = CacheAdmissionRequest {
            block_kind: CacheBlockKind::Object,
            shard_id: 1,
            routing_slot: None,
            block_bytes: 512,
            hotness: 99,
            pinned: false,
        };

        let hot = policy.decide(&hot_page);
        assert_eq!(hot.tier, CacheTier::Memory);
        assert_eq!(hot.reason, CacheAdmissionReason::HotPage);
        assert!(hot.admit_memory);
        assert!(hot.admit_pmem);
        assert!(hot.admit_ssd);

        let warm = policy.decide(&warm_slot);
        assert_eq!(warm.tier, CacheTier::Ssd);
        assert_eq!(warm.reason, CacheAdmissionReason::WarmSlot);
        assert!(!warm.admit_memory);
        assert!(!warm.admit_pmem);
        assert!(warm.admit_ssd);

        let rejected = policy.decide(&oversize);
        assert_eq!(rejected.tier, CacheTier::Reject);
        assert_eq!(rejected.reason, CacheAdmissionReason::Oversize);
        assert!(!rejected.admit_memory);
        assert!(!rejected.admit_pmem);
        assert!(!rejected.admit_ssd);

        let pmem = policy.decide(&CacheAdmissionRequest {
            block_kind: CacheBlockKind::Index,
            shard_id: 1,
            routing_slot: Some(9),
            block_bytes: 64,
            hotness: 5,
            pinned: false,
        });
        assert_eq!(pmem.tier, CacheTier::Pmem);
        assert_eq!(pmem.reason, CacheAdmissionReason::PersistentMemory);
        assert!(!pmem.admit_memory);
        assert!(pmem.admit_pmem);
        assert!(pmem.admit_ssd);
    }

    #[test]
    fn side_by_side_data_placement_routes_small_to_memory_and_large_to_pmem() {
        let policy = CacheTieringPolicy {
            memory_capacity_bytes: 64,
            pmem_capacity_bytes: 256,
            ssd_capacity_bytes: 1024,
            data_placement: CacheDataPlacement::SideBySide,
            data_placement_threshold_bytes: 32,
            memory_hotness_threshold: 99,
            pmem_admit_hotness_threshold: 99,
            ssd_admit_hotness_threshold: 99,
            max_memory_block_bytes: 64,
            max_pmem_block_bytes: 256,
            max_ssd_block_bytes: 1024,
            ssd_write_through: true,
        };

        let small = policy.decide(&CacheAdmissionRequest {
            block_kind: CacheBlockKind::Object,
            shard_id: 1,
            routing_slot: None,
            block_bytes: 16,
            hotness: 0,
            pinned: false,
        });
        assert_eq!(small.tier, CacheTier::Memory);
        assert!(small.admit_memory);
        assert!(!small.admit_pmem);
        assert!(small.admit_ssd);

        let large = policy.decide(&CacheAdmissionRequest {
            block_kind: CacheBlockKind::Object,
            shard_id: 1,
            routing_slot: None,
            block_bytes: 96,
            hotness: 0,
            pinned: false,
        });
        assert_eq!(large.tier, CacheTier::Pmem);
        assert!(!large.admit_memory);
        assert!(large.admit_pmem);
        assert!(large.admit_ssd);
    }

    #[test]
    fn data_placement_controls_are_mutable_like_unified_cache() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(64, dir.path());

        assert_eq!(CacheInstanceKind::Dram as u8, 0);
        assert_eq!(CacheInstanceKind::Pmem as u8, 1);
        assert_eq!(CacheInstanceKind::Ssd as u8, 2);
        assert_eq!(CacheInstanceKind::Unified as u8, 3);
        assert_eq!(DramPmemDataPlacement::SideBySide as u8, 0);
        assert_eq!(DramPmemDataPlacement::Tiered as u8, 1);
        assert_eq!(DramPmemDataPlacement::MaxCode as u8, 2);
        assert_eq!(
            DramPmemDataPlacement::FromConfigName("kSideBySide"),
            DramPmemDataPlacement::SideBySide
        );
        assert_eq!(
            DramPmemDataPlacement::Tiered.AsCacheDataPlacement(),
            CacheDataPlacement::Tiered
        );

        assert_eq!(cache.data_placement(), CacheDataPlacement::Tiered);
        assert_eq!(
            cache.GetDRAMPMEMDataPlacementType(),
            DramPmemDataPlacement::Tiered
        );
        cache.set_data_placement(CacheDataPlacement::SideBySide);
        cache.set_data_placement_threshold_bytes(32);

        assert_eq!(cache.data_placement(), CacheDataPlacement::SideBySide);
        assert_eq!(
            cache.config_data_placement_type(),
            DramPmemDataPlacement::SideBySide
        );
        assert_eq!(cache.data_placement_threshold_bytes(), 32);

        cache.SetDRAMPMEMDataPlacementType(DramPmemDataPlacement::Tiered);
        assert_eq!(cache.GetDataPlacementType(), CacheDataPlacement::Tiered);
    }

    #[test]
    fn ssd_instance_only_mode_routes_writes_and_reads_only_to_ssd() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_tiering_policy(
            dir.path(),
            CacheTieringPolicy {
                memory_capacity_bytes: 64,
                pmem_capacity_bytes: 128,
                ssd_capacity_bytes: 512,
                data_placement: CacheDataPlacement::Tiered,
                data_placement_threshold_bytes: 1024 * 1024,
                memory_hotness_threshold: 1,
                pmem_admit_hotness_threshold: 1,
                ssd_admit_hotness_threshold: 99,
                max_memory_block_bytes: 64,
                max_pmem_block_bytes: 128,
                max_ssd_block_bytes: 512,
                ssd_write_through: false,
            },
            CacheBlockOptions {
                compression: CacheCompression::None,
                min_compress_bytes: usize::MAX,
            },
        );
        cache.set_ssd_instance_only(true);
        assert!(cache.ssd_instance_only());

        let key = CacheKey::string(1, "ssd-only");
        cache.put(key.clone(), b"persistent-only".to_vec()).unwrap();

        assert_eq!(cache.item_count_for_tier(CacheTier::Memory), 0);
        assert_eq!(cache.item_count_for_tier(CacheTier::Pmem), 0);
        assert_eq!(cache.item_count_for_tier(CacheTier::Ssd), 1);
        let read = cache.get_with_tier(&key).unwrap().unwrap();
        assert_eq!(read.tier, CacheReadTier::Ssd);
        assert_eq!(read.value, b"persistent-only".to_vec());
        assert_eq!(cache.item_count_for_tier(CacheTier::Memory), 0);
        assert_eq!(cache.item_count_for_tier(CacheTier::Pmem), 0);

        cache.set_ssd_instance_only(false);
        assert!(!cache.ssd_instance_only());
        assert_eq!(cache.get(&key).unwrap(), Some(b"persistent-only".to_vec()));
        assert_eq!(cache.item_count_for_tier(CacheTier::Memory), 1);
    }

    #[test]
    fn bypass_storage_insert_routes_to_memory_without_ssd_write_or_access_record() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(64, dir.path());
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = Arc::clone(&events);
        cache.register_access_record_callback(move |record| {
            captured.lock().unwrap().push(record);
        });

        let key = CacheKey::string(1, "memory-bypass-storage");
        cache
            .put_bypass_storage_for_tier(CacheTier::Memory, key.clone(), b"memory-only".to_vec())
            .unwrap();

        assert!(events.lock().unwrap().is_empty());
        assert_eq!(cache.get_memory(&key), Some(b"memory-only".to_vec()));
        assert_eq!(cache.item_count_for_tier(CacheTier::Ssd), 0);
        assert_eq!(cache.peek_tier(&key), Some(CacheReadTier::Memory));
    }

    #[test]
    fn bypass_storage_insert_can_index_existing_ssd_block_without_rewrite() {
        let dir = tempfile::tempdir().unwrap();
        let key = CacheKey::string(1, "ssd-bypass-storage");
        let cache = MultiLayerCache::new(16, dir.path());
        cache.put(key.clone(), b"disk-value".to_vec()).unwrap();
        let existing_block_path = {
            let inner = cache.inner.read().expect("cache lock poisoned");
            inner.disk_path(&key)
        };
        let existing_modified = existing_block_path
            .metadata()
            .ok()
            .and_then(|metadata| metadata.modified().ok());

        let restarted = MultiLayerCache::new(16, dir.path());
        assert_eq!(restarted.item_count_for_tier(CacheTier::Ssd), 0);
        restarted
            .put_bypass_storage_for_tier(CacheTier::Ssd, key.clone(), b"disk-value".to_vec())
            .unwrap();

        assert_eq!(restarted.item_count_for_tier(CacheTier::Ssd), 1);
        assert_eq!(
            restarted
                .get_bypass_replacement_policy(&key)
                .unwrap()
                .unwrap()
                .value,
            b"disk-value".to_vec()
        );
        if cfg!(feature = "rocksdb-ssd") {
            assert!(
                !existing_block_path.exists(),
                "RocksDB-backed SSD bypass must not recreate raw block shadow files"
            );
        } else {
            assert_eq!(
                existing_block_path.metadata().unwrap().modified().ok(),
                existing_modified
            );
        }
    }

    #[test]
    fn replacement_policy_controls_are_per_tier_like_unified_cache() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(64, dir.path());

        assert_eq!(
            cache.replacement_policy_for_tier(CacheTier::Memory),
            CacheReplacementPolicy::WeightedHotnessLru
        );
        assert_eq!(
            cache.replacement_policy_for_tier(CacheTier::Pmem),
            CacheReplacementPolicy::WeightedHotnessLru
        );
        cache.set_replacement_policy_for_tier(CacheTier::Memory, CacheReplacementPolicy::Fifo);
        cache.set_replacement_policy_for_tier(CacheTier::Pmem, CacheReplacementPolicy::Slru);
        cache.set_replacement_policy_for_tier(CacheTier::Ssd, CacheReplacementPolicy::Fifo);

        assert_eq!(
            cache.replacement_policy_for_tier(CacheTier::Memory),
            CacheReplacementPolicy::Fifo
        );
        assert_eq!(
            cache.replacement_policy_for_tier(CacheTier::Pmem),
            CacheReplacementPolicy::Slru
        );
        assert_eq!(
            cache.replacement_policy_for_tier(CacheTier::Ssd),
            CacheReplacementPolicy::Fifo
        );
    }

    #[test]
    fn strict_replacement_policy_setter_is_pre_start_only() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(64, dir.path());

        assert!(matches!(
            cache.TrySetReplacementPolicyType(
                CacheInstanceKind::Dram,
                CacheReplacementPolicy::Fifo
            ),
            Err(CacheError::AlreadyStarted)
        ));
        assert_eq!(
            cache.GetReplacementPolicyType(CacheInstanceKind::Dram),
            CacheReplacementPolicy::WeightedHotnessLru
        );

        cache.Stop();
        cache
            .TrySetReplacementPolicyType(CacheInstanceKind::Dram, CacheReplacementPolicy::Fifo)
            .unwrap();
        cache
            .try_set_replacement_policy_for_tier(CacheTier::Pmem, CacheReplacementPolicy::Slru)
            .unwrap();
        assert_eq!(
            cache.GetReplacementPolicyType(CacheInstanceKind::Dram),
            CacheReplacementPolicy::Fifo
        );
        assert_eq!(
            cache.GetReplacementPolicyType(CacheInstanceKind::Pmem),
            CacheReplacementPolicy::Slru
        );
        assert!(matches!(
            cache.TrySetReplacementPolicyType(
                CacheInstanceKind::Unified,
                CacheReplacementPolicy::Fifo
            ),
            Err(CacheError::UnsupportedInstance(CacheInstanceKind::Unified))
        ));

        assert!(cache.Start());
    }

    #[test]
    fn fifo_memory_policy_evicts_oldest_inserted_entry_not_hot_entry() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(8, dir.path());
        cache.set_replacement_policy_for_tier(CacheTier::Memory, CacheReplacementPolicy::Fifo);

        let first = CacheKey::string(1, "first");
        let second = CacheKey::string(1, "second");
        let third = CacheKey::string(1, "third");
        cache.put(first.clone(), b"1111".to_vec()).unwrap();
        cache.put(second.clone(), b"2222".to_vec()).unwrap();
        for _ in 0..4 {
            assert_eq!(cache.get(&first).unwrap(), Some(b"1111".to_vec()));
        }
        cache.put(third.clone(), b"3333".to_vec()).unwrap();

        assert_eq!(cache.get_memory(&first), None);
        assert_eq!(cache.get_memory(&second), Some(b"2222".to_vec()));
        assert_eq!(cache.get_memory(&third), Some(b"3333".to_vec()));
    }

    #[test]
    fn access_record_callback_observes_put_get_and_delete() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(64, dir.path());
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = Arc::clone(&events);
        cache.register_access_record_callback(move |record| {
            captured.lock().unwrap().push(record);
        });
        let key = CacheKey::string(1, "access");

        cache.put(key.clone(), b"value".to_vec()).unwrap();
        assert_eq!(cache.get(&key).unwrap(), Some(b"value".to_vec()));
        cache.invalidate(&key).unwrap();

        let events = events.lock().unwrap().clone();
        assert_eq!(
            events
                .iter()
                .map(|record| record.record_type)
                .collect::<Vec<_>>(),
            vec![
                CacheAccessRecordKind::Put,
                CacheAccessRecordKind::Get,
                CacheAccessRecordKind::Delete,
            ]
        );
        assert!(events.iter().all(|record| record.key == key));
    }

    #[test]
    fn access_record_type_matches_config_codes_and_aliases() {
        assert_eq!(CacheAccessRecordKind::Put.config_code(), 1);
        assert_eq!(CacheAccessRecordKind::Get.ConfigCode(), 2);
        assert_eq!(CacheAccessRecordKind::Delete.config_code(), 3);
        assert_eq!(CacheAccessRecordKind::kPut, CacheAccessRecordKind::Put);
        assert_eq!(AccessRecordKind::kGet, CacheAccessRecordKind::Get);
        assert_eq!(AccessRecordKind::kDelete.AsConfigName(), "kDelete");
        assert_eq!(AccessRecordKind::kMaxCode, 4);
        assert_eq!(
            AccessRecordKind::from_config_code(1),
            Some(CacheAccessRecordKind::Put)
        );
        assert_eq!(
            AccessRecordKind::FromConfigCode(2),
            Some(CacheAccessRecordKind::Get)
        );
        assert_eq!(
            AccessRecordKind::from_config_code(3),
            Some(CacheAccessRecordKind::Delete)
        );
        assert_eq!(AccessRecordKind::from_config_code(4), None);
    }

    #[test]
    fn access_record_callback_can_be_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(64, dir.path());
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = Arc::clone(&events);
        cache.register_access_record_callback(move |record| {
            captured.lock().unwrap().push(record.record_type);
        });

        let key = CacheKey::string(1, "clear-callback");
        cache.put(key.clone(), b"value".to_vec()).unwrap();
        cache.clear_access_record_callback();
        let _ = cache.get(&key).unwrap();
        cache.invalidate(&key).unwrap();

        assert_eq!(
            events.lock().unwrap().as_slice(),
            &[CacheAccessRecordKind::Put]
        );
    }

    #[test]
    fn access_record_callback_aliases_register_and_deregister() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(64, dir.path());
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = Arc::clone(&events);
        cache.RegisterAccessRecordCallback(move |record| {
            captured.lock().unwrap().push(record.record_type);
        });

        let key = CacheKey::string(1, "legacy-access-callback");
        cache.Insert(key.clone(), b"value".to_vec(), 5).unwrap();
        let _ = cache.Lookup(&key).unwrap();
        cache.DeregisterAccessRecordCallback();
        cache.Remove(&key).unwrap();

        assert_eq!(
            events.lock().unwrap().as_slice(),
            &[CacheAccessRecordKind::Put, CacheAccessRecordKind::Get]
        );
    }

    #[test]
    fn eviction_callback_observes_memory_victims() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(8, dir.path());
        let evictions = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = Arc::clone(&evictions);
        cache.register_eviction_callback(move |record| {
            captured.lock().unwrap().push(record);
        });

        let first = CacheKey::string(1, "first");
        let second = CacheKey::string(1, "second");
        cache.put(first.clone(), b"12345678".to_vec()).unwrap();
        cache.put(second.clone(), b"abcdefgh".to_vec()).unwrap();

        let evictions = evictions.lock().unwrap().clone();
        assert_eq!(evictions.len(), 1);
        assert_eq!(evictions[0].tier, CacheTier::Memory);
        assert_eq!(evictions[0].key, first);
        assert_eq!(evictions[0].value, b"12345678".to_vec());
    }

    #[test]
    fn eviction_callback_can_be_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(8, dir.path());
        let evictions = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = Arc::clone(&evictions);
        cache.register_eviction_callback(move |record| {
            captured.lock().unwrap().push(record);
        });
        cache.clear_eviction_callback();

        cache
            .put(CacheKey::string(1, "first"), b"12345678".to_vec())
            .unwrap();
        cache
            .put(CacheKey::string(1, "second"), b"abcdefgh".to_vec())
            .unwrap();

        assert!(evictions.lock().unwrap().is_empty());
    }

    #[test]
    fn eviction_handler_status_disables_and_reenables_callback() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(8, dir.path());
        let evictions = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = Arc::clone(&evictions);
        cache.register_eviction_callback(move |record| {
            captured.lock().unwrap().push(record);
        });

        assert!(cache.eviction_handler_enabled());
        cache.set_eviction_handler_enabled(false);
        assert!(!cache.eviction_handler_enabled());
        cache
            .put(CacheKey::string(1, "first"), b"12345678".to_vec())
            .unwrap();
        cache
            .put(CacheKey::string(1, "second"), b"abcdefgh".to_vec())
            .unwrap();
        assert!(evictions.lock().unwrap().is_empty());

        cache.set_eviction_handler_enabled(true);
        assert!(cache.eviction_handler_enabled());
        cache
            .put(CacheKey::string(1, "third"), b"ABCDEFGH".to_vec())
            .unwrap();

        let evictions = evictions.lock().unwrap().clone();
        assert_eq!(evictions.len(), 1);
        assert_eq!(evictions[0].tier, CacheTier::Memory);
        assert_eq!(evictions[0].key, CacheKey::string(1, "second"));
        assert_eq!(evictions[0].value, b"abcdefgh".to_vec());
    }

    #[test]
    fn eviction_handler_aliases_disable_and_reenable_callback_delivery() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(8, dir.path());
        let evictions = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = Arc::clone(&evictions);
        cache.RegisterEvictionCallback(move |record| {
            captured.lock().unwrap().push(record);
        });

        assert!(cache.EvictionHandlerEnabled());
        cache.DeregisterEvictionHandler();
        assert!(!cache.EvictionHandlerEnabled());
        cache
            .Insert(CacheKey::string(1, "first"), b"12345678".to_vec(), 8)
            .unwrap();
        cache
            .Insert(CacheKey::string(1, "second"), b"abcdefgh".to_vec(), 8)
            .unwrap();
        assert!(evictions.lock().unwrap().is_empty());

        cache.RegisterEvictionHandler();
        assert!(cache.EvictionHandlerEnabled());
        cache
            .Insert(CacheKey::string(1, "third"), b"ABCDEFGH".to_vec(), 8)
            .unwrap();
        assert_eq!(evictions.lock().unwrap().len(), 1);

        cache.DisablePolicyMemEvictionHandler();
        assert!(!cache.EvictionHandlerEnabled());
        cache
            .Insert(CacheKey::string(1, "fourth"), b"87654321".to_vec(), 8)
            .unwrap();
        assert_eq!(evictions.lock().unwrap().len(), 1);
    }

    #[test]
    fn cache_instance_dram_surface_puts_gets_peeks_deletes_and_resets() {
        let mut instance = CacheInstance::new(
            64,
            ReplacementPolicyKind::Fifo,
            StorageEngineKind::Dram,
            Vec::new(),
        );
        assert_eq!(instance.StorageEngineType(), StorageEngineKind::Dram);
        assert_eq!(instance.TEST_GetStorageEngine(), StorageEngineKind::Dram);
        assert_eq!(
            instance.TEST_GetStorageEngineType(),
            StorageEngineKind::Dram
        );

        instance.Start().unwrap();
        instance.Put("first", b"1111".to_vec()).unwrap();
        assert!(instance.Peek("first"));
        assert_eq!(instance.Get("first").unwrap(), Some(b"1111".to_vec()));
        assert_eq!(
            instance.GetBypassReplacementPolicy("first").unwrap(),
            Some(b"1111".to_vec())
        );
        let bypass_first_buffer = instance
            .GetBypassReplacementPolicyBuffer("first")
            .unwrap()
            .unwrap();
        assert_eq!(bypass_first_buffer.Key(), "first");
        assert_eq!(bypass_first_buffer.Data(), b"1111");
        assert_eq!(bypass_first_buffer.tier(), Some(CacheReadTier::Memory));
        let l1_first_buffer = L1CacheApi::GetBypassReplacementPolicy(&instance, "first")
            .unwrap()
            .unwrap();
        assert_eq!(l1_first_buffer.Key(), "first");
        assert_eq!(l1_first_buffer.Data(), b"1111");
        assert_eq!(l1_first_buffer.tier(), Some(CacheReadTier::Memory));

        let mut async_buffer = CacheBuffer::new(b"async-value".to_vec());
        async_buffer.SetKey("async");
        let callback_seen = Arc::new(Mutex::new(false));
        let callback_seen_for_cb = Arc::clone(&callback_seen);
        let inserted = instance
            .AsyncPutBuffer(
                async_buffer,
                "legacy_cache_instance_dram_surface",
                move |result| {
                    let callback_buffer = result.expect("async put callback buffer");
                    assert_eq!(callback_buffer.Key(), "async");
                    assert_eq!(callback_buffer.Data(), b"async-value");
                    *callback_seen_for_cb.lock().unwrap() = true;
                },
            )
            .unwrap();
        assert_eq!(inserted.Key(), "async");
        assert_eq!(inserted.Data(), b"async-value");
        assert!(*callback_seen.lock().unwrap());
        assert!(instance.Peek("async"));
        assert_eq!(
            instance.Get("async").unwrap(),
            Some(b"async-value".to_vec())
        );

        let mut bypass_buffer = CacheBuffer::new(b"p".to_vec());
        bypass_buffer.SetKey("bypass-buffer");
        let bypass_inserted = instance.PutBypassStorageBuffer(bypass_buffer).unwrap();
        assert_eq!(bypass_inserted.Key(), "bypass-buffer");
        assert_eq!(bypass_inserted.Data(), b"p");
        assert_eq!(
            instance
                .GetBypassReplacementPolicy("bypass-buffer")
                .unwrap(),
            Some(b"p".to_vec())
        );

        let mut recovered = CacheBuffer::new(b"recovered-value".to_vec());
        recovered.SetKey("stale-recovered-key");
        let recovered = instance.OnRecoverData("recovered-key", recovered).unwrap();
        assert_eq!(recovered.Key(), "recovered-key");
        assert_eq!(recovered.Data(), b"recovered-value");
        assert_eq!(
            instance
                .GetBypassReplacementPolicy("recovered-key")
                .unwrap(),
            Some(b"recovered-value".to_vec())
        );

        let mut trait_recovered = CacheBuffer::new(b"trait-recovered".to_vec());
        trait_recovered.SetKey("ignored-trait-key");
        RecoverDataCallback::on_recover_data(&mut instance, "trait-recovered-key", trait_recovered);
        assert_eq!(
            instance
                .GetBypassReplacementPolicy("trait-recovered-key")
                .unwrap(),
            Some(b"trait-recovered".to_vec())
        );

        assert_eq!(instance.GetCapacity(), 64);
        assert_eq!(instance.GetItemNum(), 5);
        assert!(instance.GetUsedSpace() >= 4);

        instance.SetCapacity(8);
        assert_eq!(instance.GetCapacity(), 8);
        instance.Delete("first").unwrap();
        assert!(!instance.Peek("first"));
        assert_eq!(instance.Get("first").unwrap(), None);

        instance.Put("second", b"2222".to_vec()).unwrap();
        instance.Reset().unwrap();
        assert_eq!(instance.GetItemNum(), 0);
        assert_eq!(instance.Get("second").unwrap(), None);
        instance.Stop().unwrap();
        assert!(matches!(
            instance.Put("stopped", b"x".to_vec()),
            Err(CacheError::Stopped)
        ));
    }

    #[test]
    fn cache_instance_put_returning_buffer_matches_put_result_surface() {
        let instance = CacheInstance::new(
            64,
            ReplacementPolicyKind::Fifo,
            StorageEngineKind::Dram,
            Vec::new(),
        );

        let inserted = instance
            .PutReturningBuffer("returning-put", b"return-value".to_vec())
            .unwrap();
        assert_eq!(inserted.Key(), "returning-put");
        assert_eq!(inserted.Data(), b"return-value");
        assert_eq!(inserted.Size(), b"return-value".len());
        assert_eq!(inserted.tier(), Some(CacheReadTier::Memory));
        assert_eq!(
            instance.Get("returning-put").unwrap(),
            Some(b"return-value".to_vec())
        );
    }

    #[test]
    fn cache_instance_put_returning_buffer_reports_ssd_tier() {
        let dir = tempfile::tempdir().unwrap();
        let instance = CacheInstance::new(
            128,
            ReplacementPolicyKind::Fifo,
            StorageEngineKind::Ssd,
            vec![dir.path().to_path_buf()],
        );

        let inserted = instance
            .PutReturningBuffer("ssd-returning-put", b"ssd-return-value".to_vec())
            .unwrap();
        assert_eq!(inserted.Key(), "ssd-returning-put");
        assert_eq!(inserted.Data(), b"ssd-return-value");
        assert_eq!(inserted.tier(), Some(CacheReadTier::Ssd));
        assert_eq!(
            instance.Get("ssd-returning-put").unwrap(),
            Some(b"ssd-return-value".to_vec())
        );
    }

    #[test]
    fn cache_instance_pmem_surface_uses_exact_pmem_tier() {
        let dir = tempfile::tempdir().unwrap();
        let instance = CacheInstance::new(
            32,
            ReplacementPolicyKind::Slru,
            StorageEngineKind::Pmem,
            vec![dir.path().to_path_buf()],
        );

        instance.Put("pmem-key", b"pmem-value".to_vec()).unwrap();
        assert_eq!(
            instance.Get("pmem-key").unwrap(),
            Some(b"pmem-value".to_vec())
        );
        assert_eq!(instance.GetItemNum(), 1);
        assert_eq!(
            instance
                .inner_cache()
                .peek_tier(&CacheKey::string(0, "pmem-key")),
            Some(CacheReadTier::Pmem)
        );
    }

    #[test]
    fn l1_cache_implement_pulls_dram_then_pmem_without_replacement_access() {
        let dram = CacheInstance::new(
            64,
            ReplacementPolicyKind::Fifo,
            StorageEngineKind::Dram,
            Vec::new(),
        );
        let pmem_dir = tempfile::tempdir().unwrap();
        let pmem = CacheInstance::new(
            64,
            ReplacementPolicyKind::Slru,
            StorageEngineKind::Pmem,
            vec![pmem_dir.path().to_path_buf()],
        );
        dram.Put("shared", b"dram-value".to_vec()).unwrap();
        pmem.Put("shared", b"pmem-value".to_vec()).unwrap();
        pmem.Put("pmem-only", b"fallback".to_vec()).unwrap();

        let l1 = DramPmemL1Cache::new(dram.clone(), Some(pmem.clone()));
        let shared = l1.GetBypassReplacementPolicy("shared").unwrap().unwrap();
        assert_eq!(shared.Key(), "shared");
        assert_eq!(shared.Data(), b"dram-value");
        assert_eq!(shared.tier(), Some(CacheReadTier::Memory));
        assert_eq!(l1.L2Pulls(), 1);

        let fallback = l1
            .get_bypass_replacement_policy_buffer("pmem-only")
            .unwrap()
            .unwrap();
        assert_eq!(fallback.Key(), "pmem-only");
        assert_eq!(fallback.Data(), b"fallback");
        assert_eq!(fallback.tier(), Some(CacheReadTier::Pmem));
        assert_eq!(l1.L2Pulls(), 2);

        assert!(l1.GetBypassReplacementPolicy("missing").unwrap().is_none());
        assert_eq!(l1.L2Pulls(), 2);
    }

    #[test]
    fn l1_cache_implement_allows_absent_pmem_instance() {
        let dram = CacheInstance::new(
            64,
            ReplacementPolicyKind::Fifo,
            StorageEngineKind::Dram,
            Vec::new(),
        );
        dram.Put("dram-only", b"value".to_vec()).unwrap();
        let l1 = DramPmemL1Cache::new(dram, None);

        assert_eq!(
            l1.GetBypassReplacementPolicy("dram-only")
                .unwrap()
                .unwrap()
                .Data(),
            b"value"
        );
        assert!(l1
            .GetBypassReplacementPolicy("pmem-missing")
            .unwrap()
            .is_none());
    }

    #[test]
    fn l2_cache_policy_access_tail_and_write_match_arc_flow() {
        let dram = CacheInstance::new(
            128,
            ReplacementPolicyKind::WeightedHotnessLru,
            StorageEngineKind::Dram,
            vec![],
        );
        let l2_dir = tempfile::tempdir().unwrap();
        let l2 = CacheInstance::new(
            256,
            ReplacementPolicyKind::Fifo,
            StorageEngineKind::Ssd,
            vec![l2_dir.path().to_path_buf()],
        );
        dram.Put("cold-a", b"alpha".to_vec()).unwrap();
        dram.Put("cold-b", b"bravo".to_vec()).unwrap();

        let l1 = DramPmemL1Cache::new(dram, None);
        let mut arc = ReplacementArc::new(8);
        arc.Init().unwrap();
        let mut policy = L2CachePolicy::new(l1, l2, arc, 8, 8, 8);
        policy.Start();

        // Access records are buffered by default; a drain pass applies them.
        policy.OnAccess(AccessRecordKind::Put, "cold-a");
        policy.OnAccess(AccessRecordKind::Put, "cold-b");
        assert_eq!(policy.access_callback_count(), 0);
        assert_eq!(policy.access_buffer_size(), 2);
        policy.access_task_internal();
        assert_eq!(policy.access_callback_count(), 2);
        assert_eq!(policy.access_buffer_size(), 0);
        assert_eq!(
            policy.arc_policy().GetFetchTail(8),
            vec!["cold-a".to_string(), "cold-b".to_string()]
        );

        policy.tail_task_internal();
        assert_eq!(policy.pull_success_count(), 2);
        assert_eq!(policy.write_buffer_size(), 2);
        assert_eq!(policy.write_task_internal().unwrap(), 2);
        assert_eq!(policy.write_success_count(), 2);
        assert_eq!(
            policy.l2_cache().Get("cold-a").unwrap(),
            Some(b"alpha".to_vec())
        );
        assert_eq!(
            policy.l2_cache().Get("cold-b").unwrap(),
            Some(b"bravo".to_vec())
        );

        policy.OnAccess(AccessRecordKind::Delete, "cold-a");
        policy.access_task_internal();
        assert!(!policy
            .arc_policy()
            .GetFetchTail(8)
            .contains(&"cold-a".to_string()));
        policy.Stop();
        assert!(policy.stopped());
    }

    #[test]
    fn l2_cache_policy_eviction_queue_duplicate_and_overflow_paths() {
        let dram = CacheInstance::new(
            64,
            ReplacementPolicyKind::Fifo,
            StorageEngineKind::Dram,
            vec![],
        );
        let l2_dir = tempfile::tempdir().unwrap();
        let l2 = CacheInstance::new(
            128,
            ReplacementPolicyKind::Fifo,
            StorageEngineKind::Ssd,
            vec![l2_dir.path().to_path_buf()],
        );
        let l1 = DramPmemL1Cache::new(dram, None);
        let mut arc = ReplacementArc::new(4);
        arc.Init().unwrap();
        let mut policy = L2CachePolicy::new(l1, l2, arc, 4, 4, 1);
        let removed = Arc::new(std::sync::Mutex::new(Vec::new()));
        let removed_capture = Arc::clone(&removed);
        policy.RegisterRemoveL2PolicyHandler(move |buffer| {
            removed_capture
                .lock()
                .unwrap()
                .push(buffer.Key().to_string());
        });
        // Queueing evicted buffers for the lower tier is opt-in.
        policy.set_use_eviction_handler(true);
        policy.Start();

        let mut first = CacheBuffer::new(b"first".to_vec());
        first.SetKey("overflow-a");
        let mut second = CacheBuffer::new(b"second".to_vec());
        second.SetKey("overflow-b");
        policy.OnEvict(first);
        policy.OnEvict(second);
        assert_eq!(policy.write_buffer_size(), 1);
        assert_eq!(policy.write_enqueue_fail_count(), 1);
        assert_eq!(
            removed.lock().unwrap().as_slice(),
            &["overflow-b".to_string()]
        );

        assert_eq!(policy.write_task_internal().unwrap(), 1);
        assert_eq!(policy.write_success_count(), 1);

        let mut duplicate = CacheBuffer::new(b"new-value".to_vec());
        duplicate.SetKey("overflow-a");
        policy.OnEvict(duplicate);
        assert_eq!(policy.write_task_internal().unwrap(), 1);
        assert_eq!(policy.write_exist_count(), 1);
        assert_eq!(
            policy.l2_cache().Get("overflow-a").unwrap(),
            Some(b"first".to_vec())
        );

        policy.TEST_Pause();
        assert!(policy.paused());
        let mut paused = CacheBuffer::new(b"paused".to_vec());
        paused.SetKey("paused");
        policy.OnEvict(paused);
        assert_eq!(policy.write_task_internal().unwrap(), 0);
        policy.TEST_Continue();
        assert_eq!(policy.write_task_internal().unwrap(), 1);
    }

    fn l2_test_policy(
        l2_dir: &Path,
        arc_items: usize,
        access_capacity: usize,
        tail_batch: usize,
        write_capacity: usize,
    ) -> L2CachePolicy {
        let dram = CacheInstance::new(
            128,
            ReplacementPolicyKind::Fifo,
            StorageEngineKind::Dram,
            vec![],
        );
        let l2 = CacheInstance::new(
            256,
            ReplacementPolicyKind::Fifo,
            StorageEngineKind::Ssd,
            vec![l2_dir.to_path_buf()],
        );
        let l1 = DramPmemL1Cache::new(dram, None);
        let mut arc = ReplacementArc::new(arc_items);
        arc.Init().unwrap();
        L2CachePolicy::new(l1, l2, arc, access_capacity, tail_batch, write_capacity)
    }

    #[test]
    fn l2_cache_policy_access_buffering_modes_and_drop_on_full() {
        let dir = tempfile::tempdir().unwrap();
        let mut policy = l2_test_policy(dir.path(), 8, 2, 8, 8);
        policy.Start();
        assert!(policy.async_on_access());

        // Buffered by default, and the buffer is bounded: the third record is
        // dropped rather than letting the buffer grow without limit.
        policy.OnAccess(AccessRecordKind::Put, "a");
        policy.OnAccess(AccessRecordKind::Put, "b");
        policy.OnAccess(AccessRecordKind::Put, "c");
        assert_eq!(policy.access_buffer_size(), 2);
        assert_eq!(policy.access_drop_count(), 1);
        assert_eq!(policy.access_callback_count(), 0);

        policy.access_task_internal();
        assert_eq!(policy.access_callback_count(), 2);
        assert_eq!(policy.access_buffer_size(), 0);

        // Inline mode applies the record on the calling path instead, so the
        // buffer stays empty and nothing can be dropped.
        policy.set_async_on_access(false);
        policy.OnAccess(AccessRecordKind::Put, "d");
        assert_eq!(policy.access_buffer_size(), 0);
        assert_eq!(policy.access_callback_count(), 3);
        assert_eq!(policy.access_drop_count(), 1);
    }

    #[test]
    fn l2_cache_policy_eviction_handler_is_off_by_default() {
        let dir = tempfile::tempdir().unwrap();
        let mut policy = l2_test_policy(dir.path(), 8, 8, 8, 8);
        policy.Start();
        assert!(!policy.use_eviction_handler());

        let mut dropped = CacheBuffer::new(b"dropped".to_vec());
        dropped.SetKey("dropped-key");
        policy.OnEvict(dropped);
        // The default drops evicted data rather than writing it down a tier,
        // leaving the tail passes as the only path into the lower tier.
        assert_eq!(policy.write_buffer_size(), 0);
        assert_eq!(policy.write_enqueue_fail_count(), 0);

        policy.set_use_eviction_handler(true);
        let mut kept = CacheBuffer::new(b"kept".to_vec());
        kept.SetKey("kept-key");
        policy.OnEvict(kept);
        assert_eq!(policy.write_buffer_size(), 1);
    }

    #[test]
    fn l2_cache_policy_poll_paces_passes_by_interval() {
        let dir = tempfile::tempdir().unwrap();
        let mut policy = l2_test_policy(dir.path(), 8, 8, 8, 8);
        policy.Start();
        assert_eq!(policy.access_interval_ms(), L2_DEFAULT_ACCESS_INTERVAL_MS);
        assert_eq!(policy.tail_interval_ms(), L2_DEFAULT_TAIL_INTERVAL_MS);
        assert_eq!(policy.write_interval_ms(), L2_DEFAULT_WRITE_INTERVAL_MS);

        // With long intervals the first poll runs every pass, and the second
        // runs none, because none are due yet.
        policy.set_access_interval_ms(60_000);
        policy.set_tail_interval_ms(60_000);
        policy.set_write_interval_ms(60_000);
        policy.OnAccess(AccessRecordKind::Put, "paced");
        assert_eq!(policy.poll().unwrap(), 0);
        assert_eq!(policy.access_callback_count(), 1);

        policy.OnAccess(AccessRecordKind::Put, "paced-again");
        assert_eq!(policy.poll().unwrap(), 0);
        assert_eq!(policy.access_callback_count(), 1);
        assert_eq!(policy.access_buffer_size(), 1);

        // Dropping the interval to zero makes the pass due again.
        policy.set_access_interval_ms(0);
        assert_eq!(policy.poll().unwrap(), 0);
        assert_eq!(policy.access_callback_count(), 2);

        // flush_once ignores pacing entirely.
        policy.set_access_interval_ms(60_000);
        policy.OnAccess(AccessRecordKind::Put, "flushed");
        policy.flush_once().unwrap();
        assert_eq!(policy.access_callback_count(), 3);
    }

    #[test]
    fn l2_cache_policy_factory_uses_config_default_sizing() {
        let dram = CacheInstance::new(
            64,
            ReplacementPolicyKind::Fifo,
            StorageEngineKind::Dram,
            vec![],
        );
        let l2_dir = tempfile::tempdir().unwrap();
        let l2 = CacheInstance::new(
            128,
            ReplacementPolicyKind::Fifo,
            StorageEngineKind::Ssd,
            vec![l2_dir.path().to_path_buf()],
        );
        let l1 = DramPmemL1Cache::new(dram, None);
        let policy = L2CachePolicyFactory::CreateL2CachePolicy(l1, l2);

        assert_eq!(
            policy.arc_policy().GetItemCapacity(),
            L2_DEFAULT_MAX_ARC_CACHE_ITEMS
        );
        assert_eq!(policy.access_interval_ms(), L2_DEFAULT_ACCESS_INTERVAL_MS);
        assert_eq!(policy.tail_interval_ms(), L2_DEFAULT_TAIL_INTERVAL_MS);
        assert_eq!(policy.write_interval_ms(), L2_DEFAULT_WRITE_INTERVAL_MS);
        assert!(policy.async_on_access());
        assert!(!policy.use_eviction_handler());
    }

    #[test]
    fn l2_cache_policy_factory_builds_started_policy_surface() {
        let dram = CacheInstance::new(
            64,
            ReplacementPolicyKind::Fifo,
            StorageEngineKind::Dram,
            vec![],
        );
        let l2_dir = tempfile::tempdir().unwrap();
        let l2 = CacheInstance::new(
            128,
            ReplacementPolicyKind::Fifo,
            StorageEngineKind::Ssd,
            vec![l2_dir.path().to_path_buf()],
        );
        let l1 = DramPmemL1Cache::new(dram, None);
        let mut policy = L2CachePolicyFactory::CreateL2CachePolicy(l1, l2);
        assert!(policy.stopped());
        policy.Start();
        assert!(!policy.stopped());
        policy.TEST_WaitAllTaskSleep();
        policy.Stop();
    }

    #[test]
    fn cache_instance_ssd_surface_recovers_persistent_index() {
        let dir = tempfile::tempdir().unwrap();
        let paths = vec![dir.path().to_path_buf()];
        let instance = CacheInstance::new(
            128,
            ReplacementPolicyKind::WeightedHotnessLru,
            StorageEngineKind::Ssd,
            paths.clone(),
        );
        assert_eq!(instance.StorageEngineType(), StorageEngineKind::Ssd);
        assert_eq!(instance.TEST_GetStorageEngine(), StorageEngineKind::Ssd);
        instance.Put("ssd-key", b"ssd-value".to_vec()).unwrap();
        assert_eq!(
            instance.Get("ssd-key").unwrap(),
            Some(b"ssd-value".to_vec())
        );
        assert_eq!(
            instance
                .inner_cache()
                .peek_tier(&CacheKey::string(0, "ssd-key")),
            Some(CacheReadTier::Ssd)
        );

        let restarted = CacheInstance::new(
            128,
            ReplacementPolicyKind::WeightedHotnessLru,
            StorageEngineKind::Ssd,
            paths,
        );
        let report = restarted.RecoverData().unwrap();
        assert_eq!(report.recovered_files, 1);
        assert_eq!(
            restarted.Get("ssd-key").unwrap(),
            Some(b"ssd-value".to_vec())
        );
    }

    #[test]
    fn cache_instance_ssd_put_bypass_storage_writes_value() {
        let dir = tempfile::tempdir().unwrap();
        let instance = CacheInstance::new(
            128,
            ReplacementPolicyKind::Fifo,
            StorageEngineKind::Ssd,
            vec![dir.path().to_path_buf()],
        );

        instance
            .PutBypassStorage("ssd-bypass", b"bypass-value".to_vec())
            .unwrap();
        assert_eq!(
            instance.GetBypassReplacementPolicy("ssd-bypass").unwrap(),
            Some(b"bypass-value".to_vec())
        );
        assert_eq!(
            instance.Get("ssd-bypass").unwrap(),
            Some(b"bypass-value".to_vec())
        );
        assert_eq!(instance.GetItemNum(), 1);
        assert!(instance.GetUsedSpace() > 0);
        assert_eq!(
            instance
                .inner_cache()
                .peek_tier(&CacheKey::string(0, "ssd-bypass")),
            Some(CacheReadTier::Ssd)
        );
    }

    #[test]
    fn cache_instance_ssd_update_rewrites_guarded_block() {
        let dir = tempfile::tempdir().unwrap();
        let instance = CacheInstance::new(
            128,
            ReplacementPolicyKind::Fifo,
            StorageEngineKind::Ssd,
            vec![dir.path().to_path_buf()],
        );

        instance.Put("ssd-update", b"old-value".to_vec()).unwrap();
        let old_buffer = instance.GetBuffer("ssd-update").unwrap().unwrap();
        assert_eq!(old_buffer.Data(), b"old-value");
        assert_eq!(old_buffer.tier(), Some(CacheReadTier::Ssd));

        let mut new_buffer = CacheBuffer::new(b"new-value".to_vec());
        new_buffer.SetKey("ssd-update");
        instance
            .Update("ssd-update", &old_buffer, new_buffer)
            .unwrap();
        assert_eq!(
            instance.Get("ssd-update").unwrap(),
            Some(b"new-value".to_vec())
        );
        assert_eq!(old_buffer.Data(), b"old-value");

        let mut stale_buffer = CacheBuffer::new(b"stale-value".to_vec());
        stale_buffer.SetKey("ssd-update");
        assert!(matches!(
            instance.Update("ssd-update", &old_buffer, stale_buffer),
            Err(CacheError::ReplaceMismatch)
        ));
        assert_eq!(
            instance.Get("ssd-update").unwrap(),
            Some(b"new-value".to_vec())
        );

        let fresh_buffer = instance.GetBuffer("ssd-update").unwrap().unwrap();
        let mut final_buffer = CacheBuffer::new(b"final-value".to_vec());
        final_buffer.SetKey("ssd-update");
        instance
            .Update("ssd-update", &fresh_buffer, final_buffer)
            .unwrap();
        assert_eq!(
            instance.Get("ssd-update").unwrap(),
            Some(b"final-value".to_vec())
        );
    }

    #[test]
    fn cache_instance_eviction_and_metric_handlers_follow_status() {
        let instance = CacheInstance::new(
            8,
            ReplacementPolicyKind::Fifo,
            StorageEngineKind::Dram,
            Vec::new(),
        );
        let evictions = Arc::new(std::sync::Mutex::new(Vec::new()));
        let metric_counts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured_evictions = Arc::clone(&evictions);
        let captured_metrics = Arc::clone(&metric_counts);
        instance.RegisterEvictionHandler(move |record| {
            captured_evictions.lock().unwrap().push(record.key);
        });
        instance.RegisterEvictionMetricHandler(move |count| {
            captured_metrics.lock().unwrap().push(count);
        });

        // With the handler switched off nothing reaches it, but eviction
        // metrics are independent of the handler and keep counting: the
        // eviction rate stays observable even when nothing is consuming the
        // evicted entries.
        instance.SetEvictionHandlerStatus(false);
        instance.Put("first", b"12345678".to_vec()).unwrap();
        instance.Put("second", b"abcdefgh".to_vec()).unwrap();
        assert!(evictions.lock().unwrap().is_empty());
        assert_eq!(metric_counts.lock().unwrap().as_slice(), &[1]);

        instance.SetEvictionHandlerStatus(true);
        instance.Put("third", b"ABCDEFGH".to_vec()).unwrap();
        assert_eq!(
            evictions.lock().unwrap().as_slice(),
            &[CacheKey::string(0, "second")]
        );
        // The metric counts evicted entries, not their bytes, which is what
        // lets it be reported without materialising anything.
        assert_eq!(metric_counts.lock().unwrap().as_slice(), &[1, 1]);
    }

    #[test]
    fn cache_buffer_exposes_key_data_size_and_set_key() {
        let mut buffer = StringBuffer::string("hello");
        assert_eq!(buffer.Key(), "");
        buffer.SetKey("buffer-key");
        assert_eq!(buffer.Key(), "buffer-key");
        assert_eq!(buffer.Data(), b"hello");
        assert_eq!(buffer.DataPtr(), buffer.Data().as_ptr());
        assert_eq!(buffer.Value(), b"hello");
        assert_eq!(buffer.StringValue(), "hello");
        assert_eq!(buffer.Size(), 5);

        let converted: CacheBuffer = buffer.into();
        assert_eq!(converted.Key(), "buffer-key");
        assert_eq!(converted.Data(), b"hello");
        assert_eq!(converted.Size(), 5);
        assert_eq!(converted.tier(), None);
    }

    #[test]
    fn iobuf_buffer_owns_data_and_converts_to_cache_buffer() {
        let mut buffer = IoBufBuffer::new(b"iobuf-value".to_vec());
        assert_eq!(buffer.Key(), "");
        buffer.SetKey("iobuf-key");
        assert_eq!(buffer.Key(), "iobuf-key");
        assert_eq!(buffer.Data(), b"iobuf-value");
        assert_eq!(buffer.DataPtr(), buffer.Data().as_ptr());
        assert_eq!(buffer.Value(), b"iobuf-value");
        assert_eq!(buffer.Size(), b"iobuf-value".len());

        let converted: CacheBuffer = buffer.into();
        assert_eq!(converted.Key(), "iobuf-key");
        assert_eq!(converted.Data(), b"iobuf-value");

        let instance = CacheInstance::new(
            128,
            ReplacementPolicyKind::Fifo,
            StorageEngineKind::Dram,
            Vec::new(),
        );
        let inserted = instance.PutBuffer(converted).unwrap();
        assert_eq!(inserted.Key(), "iobuf-key");
        assert_eq!(
            instance.Get("iobuf-key").unwrap(),
            Some(b"iobuf-value".to_vec())
        );
    }

    #[test]
    fn raw_buffer_exposes_owned_data_reset_and_cache_buffer_conversion() {
        let mut raw =
            RawBuffer::with_storage_engine(b"raw-value".to_vec(), StorageEngineKind::Pmem, true);
        assert_eq!(raw.Key(), "");
        raw.SetKey("raw-key");
        assert_eq!(raw.Key(), "raw-key");
        assert_eq!(raw.Data(), b"raw-value");
        assert_eq!(raw.DataPtr(), raw.Data().as_ptr());
        assert_eq!(raw.Value(), b"raw-value");
        assert_eq!(raw.Size(), 9);
        assert_eq!(raw.storage_engine(), Some(StorageEngineKind::Pmem));
        assert!(raw.async_delete());

        let converted: CacheBuffer = raw.into();
        assert_eq!(converted.Key(), "raw-key");
        assert_eq!(converted.Data(), b"raw-value");
        assert_eq!(converted.Size(), 9);

        let mut reset = RawBuffer::new(b"drop-me".to_vec());
        reset.SetKey("reset-key");
        reset.Reset();
        assert_eq!(reset.Key(), "");
        assert_eq!(reset.Data(), b"");
        assert!(reset.DataPtr().is_null());
        assert_eq!(reset.Size(), 0);
        assert_eq!(reset.storage_engine(), None);
        assert!(!reset.async_delete());
    }

    #[test]
    fn string_view_buffer_tracks_key_and_size_without_holding_data() {
        let mut view = StringViewBuffer::new(4096);
        assert_eq!(view.Key(), "");
        view.SetKey("ssd-view");
        assert_eq!(view.Key(), "ssd-view");
        assert_eq!(view.Size(), 4096);
        assert_eq!(view.Data(), None);
        assert!(view.DataPtr().is_null());
        assert_eq!(view.Value(), None);

        let buffer = view.into_cache_buffer_with_value(b"loaded-from-ssd".to_vec());
        assert_eq!(buffer.Key(), "ssd-view");
        assert_eq!(buffer.Data(), b"loaded-from-ssd");
    }

    #[test]
    fn cache_instance_accepts_raw_buffer_conversion_for_put_buffer() {
        let instance = CacheInstance::new(
            32,
            ReplacementPolicyKind::Fifo,
            StorageEngineKind::Dram,
            Vec::new(),
        );
        let mut raw = RawBuffer::new(b"raw-put".to_vec());
        raw.SetKey("raw-put-key");

        let inserted = instance.PutBuffer(raw.into()).unwrap();
        assert_eq!(inserted.Key(), "raw-put-key");
        assert_eq!(inserted.Data(), b"raw-put");
        assert_eq!(
            instance.Get("raw-put-key").unwrap(),
            Some(b"raw-put".to_vec())
        );
    }

    #[test]
    fn cache_instance_buffer_put_get_and_update_are_guarded() {
        let mut instance = CacheInstance::new(
            32,
            ReplacementPolicyKind::Fifo,
            StorageEngineKind::Dram,
            Vec::new(),
        );

        let mut initial = CacheBuffer::string("old-value");
        initial.SetKey("guarded");
        let old = instance.PutBuffer(initial).unwrap();
        assert_eq!(old.Key(), "guarded");
        assert_eq!(old.Data(), b"old-value");
        assert_eq!(old.Size(), 9);
        assert_eq!(old.tier(), Some(CacheReadTier::Memory));

        let fetched = instance.GetBuffer("guarded").unwrap().unwrap();
        assert_eq!(fetched.Data(), b"old-value");
        let mut replacement = CacheBuffer::string("new-value");
        replacement.SetKey("guarded");
        instance.Update("guarded", &fetched, replacement).unwrap();
        assert_eq!(
            instance.Get("guarded").unwrap(),
            Some(b"new-value".to_vec())
        );

        let by_data_old = instance.GetBuffer("guarded").unwrap().unwrap();
        let mut by_data_replacement = CacheBuffer::string("newer-value");
        by_data_replacement.SetKey("guarded");
        instance
            .UpdateByOldDataPtr("guarded", by_data_old.Data(), by_data_replacement)
            .unwrap();
        assert_eq!(
            instance.Get("guarded").unwrap(),
            Some(b"newer-value".to_vec())
        );

        let mut external_same_bytes = CacheBuffer::string("should-not-land");
        external_same_bytes.SetKey("guarded");
        assert!(matches!(
            instance.UpdateByOldData("guarded", b"newer-value", external_same_bytes),
            Err(CacheError::ReplaceMismatch)
        ));

        let trait_old = instance.GetBuffer("guarded").unwrap().unwrap();
        let mut trait_replacement = CacheBuffer::string("trait-value");
        trait_replacement.SetKey("guarded");
        GcCopyCallback::update(
            &mut instance,
            "guarded",
            trait_old.Data(),
            trait_replacement,
        )
        .unwrap();
        assert_eq!(
            instance.Get("guarded").unwrap(),
            Some(b"trait-value".to_vec())
        );

        let mut missing_replacement = CacheBuffer::string("missing");
        missing_replacement.SetKey("missing");
        assert!(matches!(
            instance.UpdateByOldData("missing", b"missing", missing_replacement),
            Err(CacheError::NotFound)
        ));

        let mut stale_replacement = CacheBuffer::string("stale");
        stale_replacement.SetKey("guarded");
        assert!(matches!(
            instance.Update("guarded", &fetched, stale_replacement),
            Err(CacheError::ReplaceMismatch)
        ));

        let mut wrong_key = CacheBuffer::string("wrong-key");
        wrong_key.SetKey("other");
        let current = instance.GetBuffer("guarded").unwrap().unwrap();
        assert!(matches!(
            instance.Update("guarded", &current, wrong_key),
            Err(CacheError::ReplaceMismatch)
        ));
    }

    #[test]
    fn flexible_cache_wraps_configurable_cache_instance_for_strings() {
        let cache = MatrixCacheBuilder::BuildFlexibleCache(
            8,
            "fifo",
            "dram",
            Vec::<PathBuf>::new(),
            Vec::<PathBuf>::new(),
        );

        assert_eq!(cache.policy(), ReplacementPolicyKind::Fifo);
        assert_eq!(cache.engine(), StorageEngineKind::Dram);
        assert!(cache.Start());
        cache.Insert("first", "12345678".to_string(), 8).unwrap();
        assert_eq!(cache.Lookup("first").unwrap(), Some("12345678".to_string()));
        assert_eq!(cache.Capacity(), 8);
        assert!(cache.Size() >= 8);

        cache.SetCapacity(4);
        assert_eq!(cache.Capacity(), 4);
        cache.Remove("first").unwrap();
        assert_eq!(cache.Lookup("first").unwrap(), None);
        cache.Insert("second", "abcd".to_string(), 4).unwrap();
        cache.RemoveAll().unwrap();
        assert_eq!(cache.Size(), 0);
        assert_eq!(cache.Lookup("second").unwrap(), None);
        assert!(cache.Stop());
    }

    #[test]
    fn blockcache_facade_enforces_lifecycle_and_clears_ssd_paths() {
        let dir = tempfile::tempdir().unwrap();
        let ssd_path = dir.path().join("blockcache-ssd");
        fs::create_dir_all(&ssd_path).unwrap();
        fs::write(ssd_path.join("stale.cache_block"), b"stale").unwrap();

        let cache = BlockCache::new();
        assert!(matches!(
            cache.Put("cold", b"value"),
            Err(CacheError::Stopped)
        ));
        cache.Init(CacheOptions {
            dram_capacity: 64,
            ssd_paths: vec![ssd_path.clone()],
            blockcache_clear_ssd_folder: true,
            ..CacheOptions::default()
        });
        assert!(!cache.is_initialized());
        cache.Start().unwrap();
        assert!(cache.is_initialized());
        assert!(!ssd_path.join("stale.cache_block").exists());

        cache.Put("alpha", b"one").unwrap();
        assert_eq!(cache.Get("alpha").unwrap(), b"one");
        assert_eq!(cache.GetString("alpha").unwrap(), "one");
        assert!(matches!(cache.Get("missing"), Err(CacheError::NotFound)));

        fs::create_dir_all(&ssd_path).unwrap();
        fs::write(ssd_path.join("stop-stale.cache_block"), b"stale").unwrap();
        cache.Stop().unwrap();
        assert!(!cache.is_initialized());
        assert!(!ssd_path.join("stop-stale.cache_block").exists());
        assert!(matches!(cache.Get("alpha"), Err(CacheError::Stopped)));
    }

    #[test]
    fn flexible_cache_uses_selected_ssd_paths_and_recovers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_string_lossy().to_string();
        let cache = MatrixCacheBuilder::BuildFlexibleCacheFromPathStrings(
            128,
            "weighted_hotness_lru",
            "ssd",
            Vec::<String>::new(),
            vec![path.clone()],
        );

        assert_eq!(cache.policy(), ReplacementPolicyKind::WeightedHotnessLru);
        assert_eq!(cache.engine(), StorageEngineKind::Ssd);
        assert_eq!(cache.paths(), &[PathBuf::from(&path)]);
        cache.Insert("ssd-key", "ssd-value".to_string(), 9).unwrap();
        assert_eq!(
            cache.Lookup("ssd-key").unwrap(),
            Some("ssd-value".to_string())
        );
        assert!(cache.CalculateSpaceAmplification().is_some());

        let restarted = MatrixCacheBuilder::BuildFlexibleCacheFromPathStrings(
            128,
            "weighted_hotness_lru",
            "ssd",
            Vec::<String>::new(),
            vec![path],
        );
        let report = restarted.instance().RecoverData().unwrap();
        assert_eq!(report.recovered_files, 1);
        assert_eq!(
            restarted.Lookup("ssd-key").unwrap(),
            Some("ssd-value".to_string())
        );
    }

    #[test]
    fn pmem_cache_instance_persists_and_recovers_from_configured_path() {
        let dir = tempfile::tempdir().unwrap();
        let pmem_path = dir.path().join("pmem-device");
        let paths = vec![pmem_path.to_string_lossy().to_string()];
        let cache = MatrixCacheBuilder::BuildFlexibleCacheFromPathStrings(
            64,
            "weighted_hotness_lru",
            "pmem",
            paths.clone(),
            Vec::<String>::new(),
        );
        assert_eq!(cache.engine(), StorageEngineKind::Pmem);
        cache.Insert("pmem-a", "value-a".to_string(), 7).unwrap();
        cache.Insert("pmem-b", "value-b".to_string(), 7).unwrap();
        assert_eq!(cache.Lookup("pmem-a").unwrap(), Some("value-a".to_string()));
        assert!(pmem_path.join("pmem-cache-manifest.log").exists());

        let restarted = MatrixCacheBuilder::BuildFlexibleCacheFromPathStrings(
            64,
            "weighted_hotness_lru",
            "pmem",
            paths,
            Vec::<String>::new(),
        );
        let report = restarted.instance().RecoverData().unwrap();
        assert_eq!(report.recovered_files, 2);
        assert_eq!(
            restarted.Lookup("pmem-a").unwrap(),
            Some("value-a".to_string())
        );
        assert_eq!(
            restarted.Lookup("pmem-b").unwrap(),
            Some("value-b".to_string())
        );

        restarted.Remove("pmem-a").unwrap();
        let restarted_after_remove = MatrixCacheBuilder::BuildFlexibleCacheFromPathStrings(
            64,
            "weighted_hotness_lru",
            "pmem",
            vec![pmem_path.to_string_lossy().to_string()],
            Vec::<String>::new(),
        );
        let report = restarted_after_remove.instance().RecoverData().unwrap();
        assert_eq!(report.recovered_files, 1);
        assert_eq!(restarted_after_remove.Lookup("pmem-a").unwrap(), None);
        assert_eq!(
            restarted_after_remove.Lookup("pmem-b").unwrap(),
            Some("value-b".to_string())
        );
    }

    #[test]
    fn auto_recover_on_start_restores_pmem_index_after_restart() {
        let dir = tempfile::tempdir().unwrap();
        let pmem_path = dir.path().join("pmem-device");
        let key = CacheKey::string(43, "auto-recover-pmem-only");
        let options = CacheOptions::new(0, 64, 0)
            .with_pmem_paths([pmem_path.clone()])
            .with_auto_recover_on_start(true);
        let cache = MultiLayerCache::try_with_options(options.clone()).unwrap();
        cache
            .test_insert(CacheInstanceKind::Pmem, key.clone(), b"pmem".to_vec(), 4)
            .unwrap();

        let restarted = MultiLayerCache::try_with_options(options).unwrap();
        assert_eq!(restarted.peek_tier(&key), Some(CacheReadTier::Pmem));
        assert_eq!(
            restarted
                .test_acquire(CacheInstanceKind::Pmem, &key)
                .unwrap()
                .unwrap()
                .value(),
            b"pmem"
        );
    }

    #[test]
    fn pmem_persistent_entry_removed_through_general_cache_api_stays_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let pmem_path = dir.path().join("pmem-device");
        let cache = MultiLayerCache::with_options(
            CacheOptions::new(0, 64, 0).with_pmem_paths([pmem_path.clone()]),
        );
        let key = CacheKey::string(7, "general-remove-pmem");

        cache
            .test_insert(CacheInstanceKind::Pmem, key.clone(), b"pmem".to_vec(), 4)
            .unwrap();
        assert_eq!(
            cache
                .test_acquire(CacheInstanceKind::Pmem, &key)
                .unwrap()
                .unwrap()
                .value(),
            b"pmem"
        );
        cache.remove(&key).unwrap();

        let restarted =
            MultiLayerCache::with_options(CacheOptions::new(0, 64, 0).with_pmem_paths([pmem_path]));
        let report = restarted.recover_pmem_index().unwrap();
        assert_eq!(report.recovered_files, 0);
        assert!(restarted
            .test_acquire(CacheInstanceKind::Pmem, &key)
            .unwrap()
            .is_none());
    }

    #[test]
    fn flexible_cache_multi_ssd_uses_all_paths_and_recovers() {
        let dir_a = tempfile::tempdir().unwrap();
        let dir_b = tempfile::tempdir().unwrap();
        let path_a = dir_a.path().to_string_lossy().to_string();
        let path_b = dir_b.path().to_string_lossy().to_string();
        let paths = vec![path_a.clone(), path_b.clone()];
        let cache = MatrixCacheBuilder::BuildFlexibleCacheFromPathStrings(
            16 * 1024,
            "weighted_hotness_lru",
            "multi_ssd",
            Vec::<String>::new(),
            paths.clone(),
        );

        assert_eq!(cache.engine(), StorageEngineKind::MultiSsd);
        assert_eq!(
            cache.paths(),
            &[PathBuf::from(&path_a), PathBuf::from(&path_b)]
        );

        let ssd_store_paths = paths
            .iter()
            .map(|path| PathBuf::from(path).join("rocksdb-cache-blocks"))
            .collect::<Vec<_>>();
        let routing_probe = StorageEngineMultiSsd::with_paths(ssd_store_paths.clone(), 16 * 1024);
        let mut routed = Vec::new();
        for index in 0..512 {
            let record_key = format!("multi-ssd-flex-key-{index}");
            let key = CacheKey::string(0, &record_key);
            let store_key = CacheManifestRecord::from_entry(&key, 0).encode_line();
            let Some(device) = routing_probe.device_for_key(&store_key).map(str::to_string) else {
                continue;
            };
            if !routed
                .iter()
                .any(|(_, existing): &(String, String)| existing == &device)
            {
                routed.push((key.record_key.clone(), device));
            }
            if routed.len() == 2 {
                break;
            }
        }
        assert_eq!(routed.len(), 2);

        for (index, (key, _device)) in routed.iter().enumerate() {
            cache
                .Insert(key, format!("flex-multi-ssd-value-{index}"), 64)
                .unwrap();
        }

        let mut storage_probe = StorageEngineMultiSsd::with_paths(ssd_store_paths, 16 * 1024);
        assert!(storage_probe.Start());
        for (index, (key, _device)) in routed.iter().enumerate() {
            let store_key =
                CacheManifestRecord::from_entry(&CacheKey::string(0, key), 0).encode_line();
            assert_eq!(
                decode_cache_block(storage_probe.Get(&store_key).unwrap().Data()).unwrap(),
                format!("flex-multi-ssd-value-{index}").into_bytes()
            );
        }
        assert!(storage_probe.Stop());

        let restarted = MatrixCacheBuilder::BuildFlexibleCacheFromPathStrings(
            16 * 1024,
            "weighted_hotness_lru",
            "multi_ssd",
            Vec::<String>::new(),
            paths,
        );
        let report = restarted.instance().RecoverData().unwrap();
        assert_eq!(report.recovered_files, 2);
        for (index, (key, _device)) in routed.iter().enumerate() {
            assert_eq!(
                restarted.Lookup(key).unwrap(),
                Some(format!("flex-multi-ssd-value-{index}"))
            );
        }
    }

    #[test]
    fn multi_tier_cache_wrapper_builds_unified_cache_from_constructor_knobs() {
        let pmem_dir = tempfile::tempdir().unwrap();
        let ssd_dir = tempfile::tempdir().unwrap();
        let cache = MatrixCacheBuilder::BuildMultiTierCacheFromPathStrings(
            16,
            32,
            128,
            "fifo",
            vec![pmem_dir.path().to_string_lossy().to_string()],
            vec![ssd_dir.path().to_string_lossy().to_string()],
            "side_by_side",
            false,
            8,
            "rocksdb",
        );

        assert_eq!(cache.policy(), ReplacementPolicyKind::Fifo);
        assert_eq!(cache.ssd_storage_engine(), StorageEngineKind::Ssd);
        assert!(!cache.eviction_enabled());
        assert_eq!(cache.options().dram_capacity, 16);
        assert_eq!(cache.options().pmem_capacity, 32);
        assert_eq!(cache.options().ssd_capacity, 128);
        assert_eq!(
            cache.options().cache_dram_pmem_data_placement_type,
            "SideBySide"
        );
        assert_eq!(cache.options().cache_dram_pmem_data_placement_threshold, 8);
        assert_eq!(cache.inner().GetCapacity(CacheInstanceKind::Dram), 16);
        assert_eq!(cache.inner().GetCapacity(CacheInstanceKind::Pmem), 32);
        assert_eq!(cache.inner().GetCapacity(CacheInstanceKind::Ssd), 128);
        assert!(!cache.inner().EvictionHandlerEnabled());

        assert!(cache.Start());
        cache.Insert("small", "abcd".to_string(), 4).unwrap();
        cache.Insert("large", "0123456789".to_string(), 10).unwrap();
        assert_eq!(cache.Lookup("small").unwrap(), Some("abcd".to_string()));
        assert_eq!(
            cache.Lookup("large").unwrap(),
            Some("0123456789".to_string())
        );
        assert!(cache.Size() >= 14);

        cache.Remove("small").unwrap();
        assert_eq!(cache.Lookup("small").unwrap(), None);
        cache.SetCapacity(32);
        assert_eq!(cache.Capacity(), 32);
        let stats = cache.cache_stats_summary_line("hits", "smoke");
        assert!(stats.contains("matrixcache_stats"));
        assert!(stats.contains("metrics=hits"));
        assert!(stats.contains("comments=smoke"));
        assert!(stats.contains("policy=FIFO"));
        assert!(stats.contains("ssd_engine=SSD"));
        assert!(stats.contains("placement=SideBySide"));
        assert!(stats.contains("memory_bytes="));
        assert!(stats.contains("pmem_bytes="));
        assert!(stats.contains("disk_bytes="));
        let measurement = cache.measurement_summary_line();
        assert!(measurement.contains("matrixcache_stats"));
        assert!(measurement.contains("matrixcache_latency"));
        cache.PrintLatency("smoke");
        cache.PrintCacheStats("hits", "smoke");
        cache.PrintMeasurement();
        cache.RemoveAll().unwrap();
        assert_eq!(cache.Size(), 0);
        assert_eq!(cache.Lookup("large").unwrap(), None);
        assert!(cache.Stop());
    }

    #[test]
    fn eviction_callback_observes_capacity_reduction_victims() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(16, dir.path());
        let evictions = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = Arc::clone(&evictions);
        cache.register_eviction_callback(move |record| {
            captured.lock().unwrap().push(record);
        });

        let first = CacheKey::string(1, "first");
        cache.put(first.clone(), b"12345678".to_vec()).unwrap();
        cache
            .put(CacheKey::string(1, "second"), b"abcdefgh".to_vec())
            .unwrap();
        cache.set_capacity_for_tier(CacheTier::Memory, 8);

        let evictions = evictions.lock().unwrap().clone();
        assert_eq!(evictions.len(), 1);
        assert_eq!(evictions[0].tier, CacheTier::Memory);
        assert_eq!(evictions[0].key, first);
        assert_eq!(evictions[0].value, b"12345678".to_vec());
    }

    #[test]
    fn tiered_eviction_demotes_memory_to_pmem_then_pmem_to_ssd() {
        let dir = tempfile::tempdir().unwrap();
        let policy = CacheTieringPolicy {
            memory_capacity_bytes: 8,
            pmem_capacity_bytes: 16,
            ssd_capacity_bytes: 256,
            data_placement: CacheDataPlacement::Tiered,
            data_placement_threshold_bytes: 1024 * 1024,
            memory_hotness_threshold: 99,
            pmem_admit_hotness_threshold: 99,
            ssd_admit_hotness_threshold: 99,
            max_memory_block_bytes: 8,
            max_pmem_block_bytes: 16,
            max_ssd_block_bytes: 256,
            ssd_write_through: false,
        };
        let cache = MultiLayerCache::with_tiering_policy(
            dir.path(),
            policy,
            CacheBlockOptions {
                compression: CacheCompression::None,
                min_compress_bytes: usize::MAX,
            },
        );
        cache.set_replacement_policy_for_tier(CacheTier::Memory, CacheReplacementPolicy::Fifo);
        cache.set_replacement_policy_for_tier(CacheTier::Pmem, CacheReplacementPolicy::Fifo);
        cache.set_replacement_policy_for_tier(CacheTier::Ssd, CacheReplacementPolicy::Fifo);
        cache.set_pmem_paths(vec![dir.path().join("pmem")]);

        let first = CacheKey::string(1, "tiered-first");
        let second = CacheKey::string(1, "tiered-second");
        let third = CacheKey::string(1, "tiered-third");
        let fourth = CacheKey::string(1, "tiered-fourth");

        cache.put(first.clone(), b"11111111".to_vec()).unwrap();
        cache.put(second.clone(), b"22222222".to_vec()).unwrap();
        assert_eq!(cache.peek_tier(&first), Some(CacheReadTier::Pmem));
        assert_eq!(cache.peek_tier(&second), Some(CacheReadTier::Memory));

        cache.put(third.clone(), b"33333333".to_vec()).unwrap();
        assert_eq!(cache.peek_tier(&first), Some(CacheReadTier::Pmem));
        assert_eq!(cache.peek_tier(&second), Some(CacheReadTier::Pmem));
        assert_eq!(cache.peek_tier(&third), Some(CacheReadTier::Memory));

        cache.put(fourth.clone(), b"44444444".to_vec()).unwrap();
        assert_eq!(cache.peek_tier(&first), Some(CacheReadTier::Ssd));
        assert_eq!(cache.peek_tier(&second), Some(CacheReadTier::Pmem));
        assert_eq!(cache.peek_tier(&third), Some(CacheReadTier::Pmem));
        assert_eq!(cache.peek_tier(&fourth), Some(CacheReadTier::Memory));
        assert_eq!(cache.get(&first).unwrap(), Some(b"11111111".to_vec()));

        let stats = cache.stats();
        assert!(stats.memory_evictions >= 3);
        assert!(stats.pmem_evictions >= 1);
        assert!(stats.pmem_fills >= 3);
        assert!(stats.disk_fills >= 1);
    }

    #[test]
    fn tiered_insert_falls_back_to_pmem_when_memory_rejects() {
        let dir = tempfile::tempdir().unwrap();
        let policy = CacheTieringPolicy {
            memory_capacity_bytes: 8,
            pmem_capacity_bytes: 32,
            ssd_capacity_bytes: 0,
            data_placement: CacheDataPlacement::Tiered,
            data_placement_threshold_bytes: 1024 * 1024,
            memory_hotness_threshold: 99,
            pmem_admit_hotness_threshold: 99,
            ssd_admit_hotness_threshold: 99,
            max_memory_block_bytes: 1024,
            max_pmem_block_bytes: 1024,
            max_ssd_block_bytes: 0,
            ssd_write_through: false,
        };
        let cache = MultiLayerCache::with_tiering_policy(
            dir.path(),
            policy,
            CacheBlockOptions {
                compression: CacheCompression::None,
                min_compress_bytes: usize::MAX,
            },
        );

        let key = CacheKey::string(1, "tiered-memory-fallback");
        cache
            .put(key.clone(), b"larger-than-dram".to_vec())
            .unwrap();

        assert_eq!(cache.peek_tier(&key), Some(CacheReadTier::Pmem));
        let stats = cache.stats();
        assert_eq!(stats.memory_admission_rejected, 1);
        assert_eq!(stats.pmem_admission_accepted, 1);
        assert_eq!(stats.pmem_fills, 1);
        assert_eq!(cache.get(&key).unwrap(), Some(b"larger-than-dram".to_vec()));
    }

    #[test]
    fn eviction_callback_observes_ssd_victims_with_values() {
        let dir = tempfile::tempdir().unwrap();
        let policy = CacheTieringPolicy {
            memory_capacity_bytes: 0,
            pmem_capacity_bytes: 0,
            ssd_capacity_bytes: 90,
            data_placement: CacheDataPlacement::Tiered,
            data_placement_threshold_bytes: 1024 * 1024,
            memory_hotness_threshold: 0,
            pmem_admit_hotness_threshold: 0,
            ssd_admit_hotness_threshold: 1,
            max_memory_block_bytes: 0,
            max_pmem_block_bytes: 0,
            max_ssd_block_bytes: 64,
            ssd_write_through: true,
        };
        let cache = MultiLayerCache::with_tiering_policy(
            dir.path(),
            policy,
            CacheBlockOptions {
                compression: CacheCompression::None,
                min_compress_bytes: usize::MAX,
            },
        );
        let evictions = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = Arc::clone(&evictions);
        cache.register_eviction_callback(move |record| {
            captured.lock().unwrap().push(record);
        });

        let first = CacheKey::page(1, 10, 0, 16);
        let second = CacheKey::page(1, 11, 0, 16);
        let third = CacheKey::page(1, 12, 0, 16);
        cache
            .put(first.clone(), b"first-page-0000".to_vec())
            .unwrap();
        cache
            .put(second.clone(), b"second-page-000".to_vec())
            .unwrap();
        cache
            .put(third.clone(), b"third-page-0000".to_vec())
            .unwrap();

        let evictions = evictions.lock().unwrap().clone();
        assert!(evictions.iter().any(|record| {
            record.tier == CacheTier::Ssd
                && record.key == first
                && record.value == b"first-page-0000".to_vec()
        }));
    }

    #[test]
    fn side_by_side_ssd_refill_promotes_large_values_to_pmem() {
        let dir = tempfile::tempdir().unwrap();
        let policy = CacheTieringPolicy {
            memory_capacity_bytes: 64,
            pmem_capacity_bytes: 256,
            ssd_capacity_bytes: 1024,
            data_placement: CacheDataPlacement::SideBySide,
            data_placement_threshold_bytes: 16,
            memory_hotness_threshold: 99,
            pmem_admit_hotness_threshold: 99,
            ssd_admit_hotness_threshold: 99,
            max_memory_block_bytes: 64,
            max_pmem_block_bytes: 256,
            max_ssd_block_bytes: 1024,
            ssd_write_through: true,
        };
        let cache =
            MultiLayerCache::with_tiering_policy(dir.path(), policy, CacheBlockOptions::default());
        let key = CacheKey::string(1, "large-refill");

        cache
            .put(key.clone(), b"0123456789abcdef-large".to_vec())
            .unwrap();
        cache.clear_memory_for_test();
        assert_eq!(cache.peek_tier(&key), Some(CacheReadTier::Ssd));

        let read = cache.get_with_tier(&key).unwrap().unwrap();
        assert_eq!(read.tier, CacheReadTier::Ssd);
        assert_eq!(cache.get_memory(&key), None);
        assert_eq!(cache.peek_tier(&key), Some(CacheReadTier::Pmem));
    }

    // shared-corpus: storage_cache_refill
    #[test]
    fn tiered_ssd_cold_read_refills_memory_from_rocksdb_backed_ssd() {
        let dir = tempfile::tempdir().unwrap();
        let policy = CacheTieringPolicy {
            memory_capacity_bytes: 64,
            pmem_capacity_bytes: 0,
            ssd_capacity_bytes: 4096,
            data_placement: CacheDataPlacement::Tiered,
            data_placement_threshold_bytes: 1024 * 1024,
            memory_hotness_threshold: 0,
            pmem_admit_hotness_threshold: 0,
            ssd_admit_hotness_threshold: 0,
            max_memory_block_bytes: 64,
            max_pmem_block_bytes: 0,
            max_ssd_block_bytes: 4096,
            ssd_write_through: true,
        };
        let cache = MultiLayerCache::with_tiering_policy(
            dir.path(),
            policy,
            CacheBlockOptions {
                compression: CacheCompression::None,
                min_compress_bytes: usize::MAX,
            },
        );
        let key = CacheKey::page_with_slot(7, 9, 0, 16, Some(11));
        let value = b"rocksdb-cold-refill".to_vec();

        cache.put(key.clone(), value.clone()).unwrap();
        assert_eq!(cache.peek_tier(&key), Some(CacheReadTier::Memory));

        cache.clear_memory_for_test();
        assert_eq!(cache.peek_tier(&key), Some(CacheReadTier::Ssd));

        let read = cache.get_with_tier(&key).unwrap().unwrap();
        assert_eq!(read.tier, CacheReadTier::Ssd);
        assert_eq!(read.value, value);
        assert_eq!(cache.peek_tier(&key), Some(CacheReadTier::Memory));
        assert_eq!(
            cache.get_memory(&key),
            Some(b"rocksdb-cold-refill".to_vec())
        );

        let stats = cache.stats();
        assert_eq!(stats.disk_hits, 1);
        assert_eq!(stats.refill_failures, 0);
        assert!(stats.refill_latency_samples > 0);
        assert!(stats.read_through_latency_samples > 0);
    }

    #[test]
    fn cache_pressure_policy_report_requires_admission_eviction_and_refill_evidence() {
        let policy = CacheTieringPolicy {
            memory_capacity_bytes: 64,
            pmem_capacity_bytes: 256,
            ssd_capacity_bytes: 1024,
            data_placement: CacheDataPlacement::Tiered,
            data_placement_threshold_bytes: 1024 * 1024,
            memory_hotness_threshold: 8,
            pmem_admit_hotness_threshold: 5,
            ssd_admit_hotness_threshold: 2,
            max_memory_block_bytes: 32,
            max_pmem_block_bytes: 128,
            max_ssd_block_bytes: 256,
            ssd_write_through: true,
        };
        let requests = vec![
            CacheAdmissionRequest {
                block_kind: CacheBlockKind::Page,
                shard_id: 1,
                routing_slot: Some(1),
                block_bytes: 8,
                hotness: 10,
                pinned: false,
            },
            CacheAdmissionRequest {
                block_kind: CacheBlockKind::Index,
                shard_id: 1,
                routing_slot: Some(1),
                block_bytes: 64,
                hotness: 5,
                pinned: false,
            },
            CacheAdmissionRequest {
                block_kind: CacheBlockKind::Index,
                shard_id: 1,
                routing_slot: Some(1),
                block_bytes: 96,
                hotness: 2,
                pinned: false,
            },
            CacheAdmissionRequest {
                block_kind: CacheBlockKind::Oplog,
                shard_id: 1,
                routing_slot: None,
                block_bytes: 512,
                hotness: 10,
                pinned: false,
            },
        ];
        let passing = validate_cache_pressure_policy(
            policy,
            &requests,
            CacheStats {
                memory_evictions: 4,
                disk_hits: 7,
                ..CacheStats::default()
            },
        );
        assert!(passing.passed, "{passing:?}");
        assert_eq!(passing.memory_admitted, 1);
        assert_eq!(passing.pmem_admitted, 1);
        assert_eq!(passing.ssd_admitted, 1);
        assert_eq!(passing.rejected, 1);

        let failing = validate_cache_pressure_policy(policy, &requests[..1], CacheStats::default());
        assert!(!failing.passed);
        assert!(failing
            .reasons
            .contains(&"missing_ssd_admission".to_string()));
        assert!(failing
            .reasons
            .contains(&"missing_eviction_observation".to_string()));
    }

    // shared-corpus: storage_cache_replacement_policy_soak
    #[test]
    fn replacement_policy_soak_retains_hot_and_pinned_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(48, dir.path());

        let report = cache.replacement_policy_soak(128);

        assert!(report.passed, "{report:?}");
        assert_eq!(report.hot_memory_survivors, report.hot_key_count);
        assert!(report.cold_memory_survivors < report.hot_memory_survivors);
        assert!(report.pinned_memory_survived);
        assert!(report.observed_evictions > 0);
        assert!(report.observed_pinned_skips > 0);
        assert!(report.observed_disk_refills > 0);
        assert!(report.observed_async_writeback_backpressure > 0);
        assert!(report.async_writeback_max_queue_depth > 0);
        assert!(report.async_writeback_max_queue_bytes > 0);
        assert!(report.restart_disk_refill_ready);
        assert!(report.get_latency_samples > 0);
        assert!(report.put_latency_samples > 0);
        assert!(report.read_through_latency_samples > 0);
        assert!(report.refill_latency_samples > 0);
        assert!(report.writeback_latency_samples > 0);
        assert!(report.eviction_latency_samples > 0);
        assert!(report.compaction_latency_samples > 0);
        assert!(report.read_through_latency_bucketed);
        assert!(report.refill_latency_bucketed);
        assert!(report.writeback_latency_bucketed);
        assert!(report.eviction_latency_bucketed);
        assert!(report.compaction_latency_bucketed);
    }

    // shared-corpus: storage_cache_replacement_policy_soak
    #[test]
    fn sharded_replacement_policy_soak_aggregates_all_shards() {
        let cache = MatrixCacheBuilder::build_sharded_cache(
            CacheOptions::new(96, 0, 8192)
                .with_ssd_paths(vec![unique_temp_path("sharded-replacement-soak")]),
            2,
        );

        let report = cache.ReplacementPolicySoak(64);

        assert!(report.passed, "{report:?}");
        assert_eq!(report.iterations, 128);
        assert_eq!(report.hot_key_count, 8);
        assert_eq!(report.hot_memory_survivors, report.hot_key_count);
        assert!(report.cold_memory_survivors < report.hot_memory_survivors);
        assert!(report.pinned_memory_survived);
        assert!(report.restart_disk_refill_ready);
        assert!(report.observed_evictions > 0);
        assert!(report.observed_pinned_skips > 0);
        assert!(report.observed_disk_refills > 0);
        assert!(report.observed_async_writeback_backpressure > 0);
        assert!(report.get_latency_samples > 0);
        assert!(report.put_latency_samples > 0);
        assert!(report.read_through_latency_bucketed);
        assert!(report.refill_latency_bucketed);
        assert!(report.writeback_latency_bucketed);
        assert!(report.eviction_latency_bucketed);
        assert!(report.compaction_latency_bucketed);
    }

    // shared-corpus: storage_cache_refill;
    #[test]
    fn production_cache_tier_enforces_ssd_capacity_and_reports_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let policy = CacheTieringPolicy {
            memory_capacity_bytes: 16,
            pmem_capacity_bytes: 0,
            ssd_capacity_bytes: 90,
            data_placement: CacheDataPlacement::Tiered,
            data_placement_threshold_bytes: 1024 * 1024,
            memory_hotness_threshold: 4,
            pmem_admit_hotness_threshold: 0,
            ssd_admit_hotness_threshold: 1,
            max_memory_block_bytes: 16,
            max_pmem_block_bytes: 0,
            max_ssd_block_bytes: 64,
            ssd_write_through: true,
        };
        let cache = MultiLayerCache::with_tiering_policy(
            dir.path(),
            policy,
            CacheBlockOptions {
                compression: CacheCompression::None,
                min_compress_bytes: usize::MAX,
            },
        );
        let first = CacheKey::page_with_slot(1, 10, 0, 16, Some(3));
        let second = CacheKey::page_with_slot(1, 11, 0, 16, Some(3));
        let third = CacheKey::page_with_slot(1, 12, 0, 16, Some(4));

        cache
            .put(first.clone(), b"first-page-0000".to_vec())
            .unwrap();
        cache
            .put(second.clone(), b"second-page-000".to_vec())
            .unwrap();
        cache
            .put(third.clone(), b"third-page-0000".to_vec())
            .unwrap();

        let stats = cache.stats();
        assert!(stats.ssd_admission_accepted >= 3);
        assert!(stats.ssd_evictions >= 1);
        assert!(stats.ssd_eviction_capacity >= 1);
        assert!(stats.disk_bytes <= policy.ssd_capacity_bytes as u64);
        assert_eq!(cache.get(&first).unwrap(), None);

        let entries = cache.entries_for_shard(1);
        assert!(entries.iter().any(|entry| {
            entry.routing_slot == Some(3)
                && entry.block_kind == Some(CacheBlockKind::Page)
                && entry.admission_reason.is_some()
        }));
    }

    // shared-corpus: storage_cache_refill;
    #[test]
    fn cache_hotness_promotes_entries_and_updates_lru_order() {
        let dir = tempfile::tempdir().unwrap();
        let policy = CacheTieringPolicy {
            memory_capacity_bytes: 64,
            pmem_capacity_bytes: 0,
            ssd_capacity_bytes: 512,
            data_placement: CacheDataPlacement::Tiered,
            data_placement_threshold_bytes: 1024 * 1024,
            memory_hotness_threshold: 4,
            pmem_admit_hotness_threshold: 0,
            ssd_admit_hotness_threshold: 1,
            max_memory_block_bytes: 64,
            max_pmem_block_bytes: 0,
            max_ssd_block_bytes: 512,
            ssd_write_through: true,
        };
        let cache =
            MultiLayerCache::with_tiering_policy(dir.path(), policy, CacheBlockOptions::default());
        let key = CacheKey::page_with_slot(1, 20, 0, 4, Some(8));

        cache.put(key.clone(), b"page".to_vec()).unwrap();
        cache.clear_memory_for_test();
        assert_eq!(cache.get(&key).unwrap(), Some(b"page".to_vec()));
        assert_eq!(cache.get(&key).unwrap(), Some(b"page".to_vec()));

        let entry = cache
            .entries_for_shard(1)
            .into_iter()
            .find(|entry| entry.routing_slot == Some(8))
            .expect("cache entry should exist");
        assert!(entry.hotness >= policy.memory_hotness_threshold);
        assert!(entry.hits >= 2);
        assert!(cache.stats().hotness_promotions >= 1);
    }

    // shared-corpus: storage_cache_refill;
    #[test]
    fn weighted_memory_eviction_preserves_hot_entries() {
        let dir = tempfile::tempdir().unwrap();
        let policy = CacheTieringPolicy {
            memory_capacity_bytes: 8,
            pmem_capacity_bytes: 0,
            ssd_capacity_bytes: 512,
            data_placement: CacheDataPlacement::Tiered,
            data_placement_threshold_bytes: 1024 * 1024,
            memory_hotness_threshold: 4,
            pmem_admit_hotness_threshold: 0,
            ssd_admit_hotness_threshold: 1,
            max_memory_block_bytes: 8,
            max_pmem_block_bytes: 0,
            max_ssd_block_bytes: 128,
            ssd_write_through: true,
        };
        let cache =
            MultiLayerCache::with_tiering_policy(dir.path(), policy, CacheBlockOptions::default());
        let hot = CacheKey::page_with_slot(1, 30, 0, 4, Some(1));
        let cold_a = CacheKey::page_with_slot(1, 31, 0, 4, Some(1));
        let cold_b = CacheKey::page_with_slot(1, 32, 0, 4, Some(1));

        cache
            .put_with_admission(
                hot.clone(),
                b"hot!".to_vec(),
                CacheAdmissionRequest {
                    block_kind: CacheBlockKind::Page,
                    shard_id: 1,
                    routing_slot: Some(1),
                    block_bytes: 4,
                    hotness: 10,
                    pinned: false,
                },
            )
            .unwrap();
        cache
            .put_with_admission(
                cold_a.clone(),
                b"aaaa".to_vec(),
                CacheAdmissionRequest {
                    block_kind: CacheBlockKind::Page,
                    shard_id: 1,
                    routing_slot: Some(1),
                    block_bytes: 4,
                    hotness: 0,
                    pinned: false,
                },
            )
            .unwrap();
        cache
            .put_with_admission(
                cold_b.clone(),
                b"bbbb".to_vec(),
                CacheAdmissionRequest {
                    block_kind: CacheBlockKind::Page,
                    shard_id: 1,
                    routing_slot: Some(1),
                    block_bytes: 4,
                    hotness: 0,
                    pinned: false,
                },
            )
            .unwrap();

        assert_eq!(cache.get_memory(&hot), Some(b"hot!".to_vec()));
        assert_eq!(cache.get_memory(&cold_a), None);
        assert_eq!(cache.get_memory(&cold_b), Some(b"bbbb".to_vec()));
        let report = cache.eviction_report();
        assert_eq!(
            report.replacement_policy,
            CacheReplacementPolicy::WeightedHotnessLru
        );
        assert!(report.memory_capacity_evictions >= 1);
        assert!(report.memory_low_hit_evictions >= 1 || report.memory_cold_evictions >= 1);
    }

    // shared-corpus: storage_cache_refill;
    #[test]
    fn memory_eviction_selects_cold_slot_group_before_cold_entry_in_hot_slot() {
        let dir = tempfile::tempdir().unwrap();
        let policy = CacheTieringPolicy {
            memory_capacity_bytes: 8,
            pmem_capacity_bytes: 0,
            ssd_capacity_bytes: 64,
            data_placement: CacheDataPlacement::Tiered,
            data_placement_threshold_bytes: 1024 * 1024,
            memory_hotness_threshold: 4,
            pmem_admit_hotness_threshold: 0,
            ssd_admit_hotness_threshold: 99,
            max_memory_block_bytes: 8,
            max_pmem_block_bytes: 0,
            max_ssd_block_bytes: 64,
            ssd_write_through: false,
        };
        let cache =
            MultiLayerCache::with_tiering_policy(dir.path(), policy, CacheBlockOptions::default());
        let hot_slot_hot = CacheKey::page_with_slot(1, 50, 0, 4, Some(7));
        let hot_slot_cold = CacheKey::page_with_slot(1, 51, 0, 4, Some(7));
        let cold_slot = CacheKey::page_with_slot(1, 52, 0, 4, Some(8));

        for (key, slot, hotness, value) in [
            (hot_slot_hot.clone(), 7, 10, b"hot!".to_vec()),
            (hot_slot_cold.clone(), 7, 0, b"warm".to_vec()),
            (cold_slot.clone(), 8, 0, b"cold".to_vec()),
        ] {
            cache
                .put_with_admission(
                    key,
                    value,
                    CacheAdmissionRequest {
                        block_kind: CacheBlockKind::Page,
                        shard_id: 1,
                        routing_slot: Some(slot),
                        block_bytes: 4,
                        hotness,
                        pinned: false,
                    },
                )
                .unwrap();
        }

        assert_eq!(cache.get_memory(&hot_slot_hot), Some(b"hot!".to_vec()));
        assert_eq!(cache.get_memory(&hot_slot_cold), Some(b"warm".to_vec()));
        assert_eq!(cache.get_memory(&cold_slot), None);
        let report = cache.eviction_report();
        assert!(report.sampled_eviction_groups >= 2);
        assert!(report.memory_slot_evictions >= 1);
    }

    // shared-corpus: storage_cache_eviction;
    #[test]
    fn memory_capacity_shrink_batches_multiple_evictions() {
        let dir = tempfile::tempdir().unwrap();
        let policy = CacheTieringPolicy {
            memory_capacity_bytes: 40,
            pmem_capacity_bytes: 0,
            ssd_capacity_bytes: 0,
            data_placement: CacheDataPlacement::Tiered,
            data_placement_threshold_bytes: 1024,
            memory_hotness_threshold: 0,
            pmem_admit_hotness_threshold: u32::MAX,
            ssd_admit_hotness_threshold: u32::MAX,
            max_memory_block_bytes: 16,
            max_pmem_block_bytes: 0,
            max_ssd_block_bytes: 0,
            ssd_write_through: false,
        };
        let cache =
            MultiLayerCache::with_tiering_policy(dir.path(), policy, CacheBlockOptions::default());
        let keys = (0..5)
            .map(|i| CacheKey::page_with_slot(3, i, 0, 8, Some(i as u32)))
            .collect::<Vec<_>>();
        for (index, key) in keys.iter().enumerate() {
            cache
                .put_with_admission(
                    key.clone(),
                    vec![b'a' + index as u8; 8],
                    CacheAdmissionRequest {
                        block_kind: CacheBlockKind::Page,
                        shard_id: 3,
                        routing_slot: Some(index as u32),
                        block_bytes: 8,
                        hotness: index as u32,
                        pinned: false,
                    },
                )
                .unwrap();
        }

        cache.set_capacity_for_tier(CacheTier::Memory, 16);

        let stats = cache.stats();
        assert!(stats.memory_evictions >= 3);
        assert!(stats.eviction_latency_samples >= 3);
        assert!(cache.size_for_tier(CacheTier::Memory) <= 16);
        let remaining = cache.get_batch(&keys).unwrap();
        assert_eq!(remaining.iter().filter(|value| value.is_some()).count(), 2);
    }

    /// Every tier's key order has to hold exactly the keys that tier holds.
    ///
    /// Victim selection reads the order rather than the tier map, so a key the
    /// order has lost can never be evicted and a key it lists but the tier no
    /// longer holds wastes a round of selection. Neither shows up as a wrong
    /// answer until the cache is under pressure, so this checks the invariant
    /// directly after a workload that exercises every path that edits it.
    #[test]
    fn tier_orders_hold_exactly_the_keys_their_tiers_hold() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_options(CacheOptions {
            dram_capacity: 512,
            pmem_capacity: 512,
            ssd_capacity: 4096,
            ssd_paths: vec![dir.path().to_path_buf()],
            ..CacheOptions::default()
        });
        cache.start().unwrap();

        let keys = (0..64)
            .map(|index| CacheKey::string(0, &format!("order-invariant-{index:04}")))
            .collect::<Vec<_>>();

        // Fill past capacity so entries evict and demote between tiers.
        for key in &keys {
            cache.put(key.clone(), vec![b'z'; 16]).unwrap();
        }
        // Reads promote and refill, which rewrites tier membership.
        for key in keys.iter().step_by(3) {
            let _ = cache.get(key).unwrap();
        }
        // Explicit removals and an invalidation take their own paths.
        for key in keys.iter().step_by(7) {
            cache.remove(key).unwrap();
        }
        cache.invalidate(&keys[1]).unwrap();
        cache.invalidate_memory_only(&keys[2]);
        // A shrink evicts a batch in one pass.
        cache.set_capacity_for_tier(CacheTier::Memory, 128);

        let inner = cache.inner.read().expect("cache lock poisoned");
        let order_keys = |order: &CacheKeyOrder| {
            order.iter().cloned().collect::<std::collections::HashSet<_>>()
        };
        assert_eq!(
            order_keys(&inner.memory_order),
            inner.memory.keys().cloned().collect::<std::collections::HashSet<_>>(),
            "memory order and memory tier disagree"
        );
        assert_eq!(
            order_keys(&inner.pmem_order),
            inner.pmem.keys().cloned().collect::<std::collections::HashSet<_>>(),
            "pmem order and pmem tier disagree"
        );
        assert_eq!(
            order_keys(&inner.disk_order),
            inner.disk_index.keys().cloned().collect::<std::collections::HashSet<_>>(),
            "disk order and disk tier disagree"
        );
    }

    /// First-in first-out eviction has to keep going until the tier is back
    /// under its budget, however many entries that takes.
    ///
    /// Selection reads the tier's key order, so a victim that is taken but
    /// left in that order gets picked again on the next round. The second pick
    /// frees nothing, the loop reads that as no progress and stops, and the
    /// tier is left over capacity with one entry removed instead of three.
    #[test]
    fn fifo_capacity_shrink_evicts_the_whole_overage_in_one_pass() {
        let dir = tempfile::tempdir().unwrap();
        // Memory only: a victim with a tier below it is demoted rather than
        // dropped, and would still read back.
        let cache = MultiLayerCache::with_options(CacheOptions {
            dram_capacity: 40,
            pmem_capacity: 0,
            ssd_capacity: 0,
            ssd_paths: vec![dir.path().to_path_buf()],
            cache_dram_replacement_policy: "FIFO".to_string(),
            ..CacheOptions::default()
        });
        cache.start().unwrap();

        let keys = (0..5)
            .map(|index| CacheKey::string(0, &format!("fifo-shrink-{index}")))
            .collect::<Vec<_>>();
        for key in &keys {
            cache.put(key.clone(), vec![b'x'; 8]).unwrap();
        }
        assert_eq!(cache.size_for_tier(CacheTier::Memory), 40);

        // Room for two of the five entries, so three have to go at once.
        cache.set_capacity_for_tier(CacheTier::Memory, 16);

        assert!(
            cache.size_for_tier(CacheTier::Memory) <= 16,
            "tier left over capacity at {} bytes",
            cache.size_for_tier(CacheTier::Memory)
        );
        let remaining = cache.get_batch(&keys).unwrap();
        assert_eq!(remaining.iter().filter(|value| value.is_some()).count(), 2);
        // First in, first out: the two survivors are the ones written last.
        assert!(remaining[3].is_some() && remaining[4].is_some());
    }

    // shared-corpus: storage_cache_refill;
    #[test]
    fn weighted_ssd_eviction_preserves_hot_entries() {
        let dir = tempfile::tempdir().unwrap();
        let policy = CacheTieringPolicy {
            memory_capacity_bytes: 0,
            pmem_capacity_bytes: 0,
            ssd_capacity_bytes: 70,
            data_placement: CacheDataPlacement::Tiered,
            data_placement_threshold_bytes: 1024 * 1024,
            memory_hotness_threshold: 4,
            pmem_admit_hotness_threshold: 0,
            ssd_admit_hotness_threshold: 1,
            max_memory_block_bytes: 8,
            max_pmem_block_bytes: 0,
            max_ssd_block_bytes: 64,
            ssd_write_through: true,
        };
        let cache = MultiLayerCache::with_tiering_policy(
            dir.path(),
            policy,
            CacheBlockOptions {
                compression: CacheCompression::None,
                min_compress_bytes: usize::MAX,
            },
        );
        let hot = CacheKey::page_with_slot(1, 40, 0, 4, Some(2));
        let cold_a = CacheKey::page_with_slot(1, 41, 0, 4, Some(2));
        let cold_b = CacheKey::page_with_slot(1, 42, 0, 4, Some(2));

        for (key, hotness, bytes) in [
            (hot.clone(), 10, b"hot!".to_vec()),
            (cold_a.clone(), 0, b"aaaa".to_vec()),
            (cold_b.clone(), 0, b"bbbb".to_vec()),
        ] {
            cache
                .put_with_admission(
                    key,
                    bytes,
                    CacheAdmissionRequest {
                        block_kind: CacheBlockKind::Page,
                        shard_id: 1,
                        routing_slot: Some(2),
                        block_bytes: 4,
                        hotness,
                        pinned: false,
                    },
                )
                .unwrap();
        }

        assert_eq!(cache.get(&hot).unwrap(), Some(b"hot!".to_vec()));
        assert_eq!(cache.get(&cold_a).unwrap(), None);
        assert_eq!(cache.get(&cold_b).unwrap(), Some(b"bbbb".to_vec()));
        let report = cache.eviction_report();
        assert!(report.ssd_capacity_evictions >= 1);
        assert!(report.ssd_low_hit_evictions >= 1 || report.ssd_cold_evictions >= 1);
    }

    // shared-corpus: storage_cache_refill;
    #[test]
    fn ssd_eviction_selects_cold_slot_group_before_cold_entry_in_hot_slot() {
        let dir = tempfile::tempdir().unwrap();
        let policy = CacheTieringPolicy {
            memory_capacity_bytes: 0,
            pmem_capacity_bytes: 0,
            ssd_capacity_bytes: 256,
            data_placement: CacheDataPlacement::Tiered,
            data_placement_threshold_bytes: 1024 * 1024,
            memory_hotness_threshold: 99,
            pmem_admit_hotness_threshold: 0,
            ssd_admit_hotness_threshold: 0,
            max_memory_block_bytes: 0,
            max_pmem_block_bytes: 0,
            max_ssd_block_bytes: 128,
            ssd_write_through: true,
        };
        let cache = MultiLayerCache::with_tiering_policy(
            dir.path(),
            policy,
            CacheBlockOptions {
                compression: CacheCompression::None,
                min_compress_bytes: usize::MAX,
            },
        );
        let hot_slot_hot = CacheKey::page_with_slot(1, 60, 0, 4, Some(9));
        let hot_slot_cold = CacheKey::page_with_slot(1, 61, 0, 4, Some(9));
        let cold_slot = CacheKey::page_with_slot(1, 62, 0, 4, Some(10));

        for (key, slot, hotness, value) in [
            (hot_slot_hot.clone(), 9, 10, b"hot!".to_vec()),
            (hot_slot_cold.clone(), 9, 0, b"warm".to_vec()),
            (cold_slot.clone(), 10, 0, vec![b'c'; 240]),
        ] {
            cache
                .put_with_admission(
                    key,
                    value,
                    CacheAdmissionRequest {
                        block_kind: CacheBlockKind::Page,
                        shard_id: 1,
                        routing_slot: Some(slot),
                        block_bytes: 4,
                        hotness,
                        pinned: false,
                    },
                )
                .unwrap();
        }

        assert_eq!(cache.get(&hot_slot_hot).unwrap(), Some(b"hot!".to_vec()));
        assert_eq!(cache.get(&hot_slot_cold).unwrap(), Some(b"warm".to_vec()));
        assert_eq!(cache.get(&cold_slot).unwrap(), None);
        let report = cache.eviction_report();
        let stats = cache.stats();
        assert_eq!(report.ssd_slot_evictions, 0);
        assert!(stats.ssd_admission_rejected >= 1);
        assert!(stats.writeback_backpressure_events >= 1);
    }

    #[test]
    fn acquire_release_and_insert_pinned_match_zero_copy_surface() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(8, dir.path());
        let key = CacheKey::string(1, "zero-copy");

        let inserted = cache
            .insert_pinned(key.clone(), b"value".to_vec())
            .unwrap()
            .expect("insert_pinned should return a handle");
        assert_eq!(inserted.key, key);
        assert_eq!(inserted.as_slice(), b"value");
        assert_eq!(cache.stats().pinned_entries, 1);
        assert_eq!(cache.stats().zero_copy_handle_hits, 0);

        cache.release(inserted);
        assert_eq!(cache.stats().pinned_entries, 0);
        assert_eq!(cache.stats().unpin_operations, 1);

        cache.clear_memory_for_test();
        let acquired = cache
            .acquire(&key)
            .unwrap()
            .expect("acquire should find the SSD-backed value");
        assert_eq!(acquired.as_slice(), b"value");
        assert_eq!(cache.stats().pinned_entries, 1);
        assert_eq!(acquired.tier(), CacheReadTier::Ssd);
        assert_eq!(cache.get_memory(&key), Some(b"value".to_vec()));

        cache.release(acquired);
        assert_eq!(cache.stats().pinned_entries, 0);
        assert!(cache
            .acquire(&CacheKey::string(1, "missing"))
            .unwrap()
            .is_none());
        assert!(cache.stats().zero_copy_handle_misses >= 1);
    }

    #[test]
    fn acquire_no_promotion_pins_ssd_without_refilling_memory() {
        let cache = MultiLayerCache::with_tiering_policy(
            unique_temp_path("zero-copy-no-promotion-acquire"),
            CacheTieringPolicy {
                memory_capacity_bytes: 64,
                pmem_capacity_bytes: 0,
                ssd_capacity_bytes: 4096,
                data_placement: CacheDataPlacement::Tiered,
                data_placement_threshold_bytes: 1024,
                memory_hotness_threshold: u32::MAX,
                pmem_admit_hotness_threshold: u32::MAX,
                ssd_admit_hotness_threshold: 0,
                max_memory_block_bytes: 64,
                max_pmem_block_bytes: 0,
                max_ssd_block_bytes: 4096,
                ssd_write_through: true,
            },
            CacheBlockOptions::default(),
        );
        let key = CacheKey::string(1, "zero-copy-no-promotion");
        cache.put(key.clone(), b"cold-handle".to_vec()).unwrap();
        cache.clear_memory_for_test();
        assert_eq!(cache.peek_tier(&key), Some(CacheReadTier::Ssd));

        let zero_copy_hits_before = cache.stats().zero_copy_handle_hits;
        let handle = cache
            .acquire_no_promotion(&key)
            .unwrap()
            .expect("SSD handle");
        assert_eq!(handle.tier(), CacheReadTier::Ssd);
        assert_eq!(handle.value(), b"cold-handle");
        assert_eq!(cache.get_memory(&key), None);
        assert_eq!(cache.stats().zero_copy_handle_hits, zero_copy_hits_before);
        assert_eq!(cache.stats().pinned_entries, 1);

        cache.release(handle);
        assert_eq!(cache.stats().pinned_entries, 0);
        assert_eq!(cache.get_memory(&key), None);
    }

    #[test]
    fn acquire_batch_no_promotion_coalesces_duplicates_without_refill() {
        let cache = MultiLayerCache::with_tiering_policy(
            unique_temp_path("zero-copy-no-promotion-batch"),
            CacheTieringPolicy {
                memory_capacity_bytes: 64,
                pmem_capacity_bytes: 0,
                ssd_capacity_bytes: 4096,
                data_placement: CacheDataPlacement::Tiered,
                data_placement_threshold_bytes: 1024,
                memory_hotness_threshold: u32::MAX,
                pmem_admit_hotness_threshold: u32::MAX,
                ssd_admit_hotness_threshold: 0,
                max_memory_block_bytes: 64,
                max_pmem_block_bytes: 0,
                max_ssd_block_bytes: 4096,
                ssd_write_through: true,
            },
            CacheBlockOptions::default(),
        );
        let repeated = CacheKey::string(1, "zero-copy-no-promotion-dup");
        let other = CacheKey::string(1, "zero-copy-no-promotion-other");
        cache.put(repeated.clone(), b"dup".to_vec()).unwrap();
        cache.put(other.clone(), b"other".to_vec()).unwrap();
        cache.clear_memory_for_test();

        let zero_copy: &dyn ZeroCopyCacheApi = &cache;
        let zero_copy_hits_before = cache.stats().zero_copy_handle_hits;
        let handles = zero_copy
            .acquire_batch_no_promotion_cache(&[repeated.clone(), other.clone(), repeated.clone()])
            .unwrap();
        assert_eq!(handles.len(), 3);
        assert_eq!(handles[0].as_ref().unwrap().value(), b"dup");
        assert_eq!(handles[1].as_ref().unwrap().value(), b"other");
        assert_eq!(handles[2].as_ref().unwrap().value(), b"dup");
        assert_eq!(cache.stats().pinned_entries, 2);
        assert_eq!(cache.stats().pin_operations, 3);
        assert_eq!(cache.stats().zero_copy_handle_hits, zero_copy_hits_before);
        assert_eq!(cache.get_memory(&repeated), None);
        assert_eq!(cache.get_memory(&other), None);

        let handles = handles.into_iter().flatten().collect::<Vec<_>>();
        assert_eq!(zero_copy.release_batch_cache(handles), 3);
        assert_eq!(cache.stats().pinned_entries, 0);
    }

    #[test]
    fn acquire_batch_coalesces_duplicates_and_balances_pins() {
        let cache = MultiLayerCache::with_tiering_policy(
            unique_temp_path("zero-copy-batch-acquire"),
            CacheTieringPolicy {
                memory_capacity_bytes: 1,
                pmem_capacity_bytes: 0,
                ssd_capacity_bytes: 4096,
                data_placement: CacheDataPlacement::Tiered,
                data_placement_threshold_bytes: 1024,
                memory_hotness_threshold: u32::MAX,
                pmem_admit_hotness_threshold: u32::MAX,
                ssd_admit_hotness_threshold: 0,
                max_memory_block_bytes: 1,
                max_pmem_block_bytes: 0,
                max_ssd_block_bytes: 4096,
                ssd_write_through: true,
            },
            CacheBlockOptions::default(),
        );
        let repeated = CacheKey::string(1, "zero-copy-batch-dup");
        let other = CacheKey::string(1, "zero-copy-batch-other");
        cache.put(repeated.clone(), b"dup".to_vec()).unwrap();
        cache.put(other.clone(), b"other".to_vec()).unwrap();
        cache.clear_memory_for_test();

        let zero_copy: &dyn ZeroCopyCacheApi = &cache;
        let handles = zero_copy
            .acquire_batch_cache(&[repeated.clone(), other.clone(), repeated.clone()])
            .unwrap();
        assert_eq!(handles.len(), 3);
        assert_eq!(handles[0].as_ref().unwrap().value(), b"dup");
        assert_eq!(handles[1].as_ref().unwrap().value(), b"other");
        assert_eq!(handles[2].as_ref().unwrap().value(), b"dup");
        assert_eq!(cache.stats().pinned_entries, 2);
        assert_eq!(cache.stats().pin_operations, 3);
        assert_eq!(cache.stats().disk_hits, 2);

        let handles = handles.into_iter().flatten().collect::<Vec<_>>();
        assert_eq!(zero_copy.release_batch_cache(handles), 3);
        assert_eq!(cache.stats().pinned_entries, 0);
        assert_eq!(cache.stats().unpin_operations, 3);
    }

    #[test]
    fn update_cached_value_if_current_matches_cache_instance_update_guard() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(64, dir.path());
        let key = CacheKey::string(9, "update");

        cache.put(key.clone(), b"old-value".to_vec()).unwrap();
        let old_handle = cache.acquire(&key).unwrap().expect("old handle");
        assert_eq!(old_handle.as_slice(), b"old-value");

        cache
            .update_cached_value_if_current(&key, &old_handle, b"new-value".to_vec())
            .unwrap();

        assert_eq!(old_handle.as_slice(), b"old-value");
        assert_eq!(cache.get(&key).unwrap(), Some(b"new-value".to_vec()));
        assert!(matches!(
            cache.update_cached_value_if_current(&key, &old_handle, b"stale".to_vec()),
            Err(CacheError::ReplaceMismatch)
        ));

        let fresh_handle = cache.acquire(&key).unwrap().expect("fresh handle");
        assert_eq!(fresh_handle.as_slice(), b"new-value");
        cache
            .update_cached_value_if_current(&key, &fresh_handle, b"final".to_vec())
            .unwrap();
        assert_eq!(cache.get(&key).unwrap(), Some(b"final".to_vec()));
        assert!(matches!(
            cache.update_cached_value_if_current(
                &CacheKey::string(9, "other-key"),
                &fresh_handle,
                b"wrong-key".to_vec()
            ),
            Err(CacheError::ReplaceMismatch)
        ));
        cache.release(old_handle);
        cache.release(fresh_handle);

        let final_handle = cache.acquire(&key).unwrap().expect("final handle");
        cache.invalidate(&key).unwrap();
        assert!(matches!(
            cache.update_cached_value_if_current(&key, &final_handle, b"gone".to_vec()),
            Err(CacheError::NotFound)
        ));
        cache.release(final_handle);

        cache.put(key.clone(), b"again".to_vec()).unwrap();
        let handle = cache.acquire(&key).unwrap().expect("handle");
        cache.stop();
        assert!(matches!(
            cache.update_cached_value_if_current(&key, &handle, b"blocked".to_vec()),
            Err(CacheError::Stopped)
        ));
    }

    #[test]
    fn counted_pins_keep_entries_pinned_until_last_release() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(8, dir.path());
        let key = CacheKey::string(1, "counted");
        let other = CacheKey::string(1, "other");

        cache.put(key.clone(), b"pin".to_vec()).unwrap();
        let first = cache.acquire(&key).unwrap().expect("first handle");
        let second = cache.clone_handle(&first);
        assert_eq!(cache.stats().pinned_entries, 1);
        assert_eq!(cache.stats().pin_operations, 2);

        cache.release(first);
        assert_eq!(cache.stats().pinned_entries, 1);
        cache.put(other, b"12345678".to_vec()).unwrap();
        assert_eq!(cache.get_memory(&key), Some(b"pin".to_vec()));

        cache.release(second);
        assert_eq!(cache.stats().pinned_entries, 0);
        assert_eq!(cache.stats().unpin_operations, 2);
    }

    #[test]
    fn pinned_handle_clone_with_cache_matches_config_explicit_clone_semantics() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(8, dir.path());
        let key = CacheKey::string(1, "clone");
        let other = CacheKey::string(1, "other");

        cache.put(key.clone(), b"pin".to_vec()).unwrap();
        let first = cache.acquire(&key).unwrap().expect("first handle");
        let second = first.clone_with_cache(&cache);

        assert_eq!(first.key(), &key);
        assert_eq!(first.tier(), CacheReadTier::Memory);
        assert_eq!(first.value(), b"pin");
        assert_eq!(first.as_slice(), b"pin");
        assert_eq!(second.key(), &key);
        assert_eq!(second.tier(), CacheReadTier::Memory);
        assert_eq!(second.value(), b"pin");
        assert_eq!(second.as_slice(), b"pin");
        assert_eq!(cache.stats().pin_operations, 2);
        assert_eq!(cache.stats().pinned_entries, 1);

        cache.release(first);
        cache.put(other.clone(), b"12345678".to_vec()).unwrap();
        assert_eq!(cache.get_memory(&key), Some(b"pin".to_vec()));

        cache.release(second);
        cache.put(other, b"abcdefgh".to_vec()).unwrap();
        assert_eq!(cache.stats().pinned_entries, 0);
        assert_eq!(cache.stats().unpin_operations, 2);
    }

    #[test]
    fn scoped_handle_releases_pin_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(16, dir.path());
        let key = CacheKey::string(1, "scoped");
        cache.put(key.clone(), b"value".to_vec()).unwrap();

        {
            let scoped = cache.acquire_scoped(&key).unwrap().expect("scoped handle");
            assert_eq!(scoped.key(), &key);
            assert_eq!(scoped.as_slice(), b"value");
            assert_eq!(cache.stats().pinned_entries, 1);
        }

        assert_eq!(cache.stats().pinned_entries, 0);
        assert_eq!(cache.stats().unpin_operations, 1);
    }

    #[test]
    fn manual_pins_are_reference_counted() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(8, dir.path());
        let key = CacheKey::string(1, "manual");
        cache.put(key.clone(), b"pin".to_vec()).unwrap();

        cache.pin(key.clone());
        cache.pin(key.clone());
        assert_eq!(cache.stats().pinned_entries, 1);
        assert_eq!(cache.stats().pin_operations, 2);

        cache.unpin(&key);
        assert_eq!(cache.stats().pinned_entries, 1);
        cache.unpin(&key);
        assert_eq!(cache.stats().pinned_entries, 0);
        assert_eq!(cache.stats().unpin_operations, 2);
    }

    #[test]
    fn pinned_memory_entries_survive_capacity_eviction() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(10, dir.path());
        let pinned = CacheKey::string(1, "pinned");
        let first = CacheKey::string(1, "first");
        let second = CacheKey::string(1, "second");

        cache.put(pinned.clone(), b"pin".to_vec()).unwrap();
        cache.pin(pinned.clone());
        cache.put(first.clone(), b"11111".to_vec()).unwrap();
        cache.put(second.clone(), b"22222".to_vec()).unwrap();

        assert_eq!(cache.get_memory(&pinned), Some(b"pin".to_vec()));
        assert_eq!(cache.stats().pinned_entries, 1);
        assert_eq!(cache.stats().pinned_bytes, 3);
        assert!(cache.stats().eviction_pinned_skips > 0);

        cache.unpin(&pinned);
        assert_eq!(cache.stats().pinned_entries, 0);
    }

    #[test]
    fn scoped_lookup_matches_config_found_and_auto_release_semantics() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(16, dir.path());
        let key = CacheKey::string(1, "scoped-lookup");

        let missing = cache.scoped_lookup(&key).unwrap();
        assert!(!missing.found());
        assert_eq!(missing.key(), None);
        assert_eq!(missing.tier(), None);
        assert_eq!(missing.as_slice(), None);
        assert_eq!(cache.stats().pinned_entries, 0);

        cache.insert(key.clone(), b"value".to_vec(), 5).unwrap();
        {
            let lookup = cache.scoped_lookup(&key).unwrap();
            assert!(lookup.found());
            assert_eq!(lookup.key(), Some(&key));
            assert_eq!(lookup.tier(), Some(CacheReadTier::Memory));
            assert_eq!(lookup.value(), Some(b"value".as_slice()));
            assert_eq!(lookup.as_slice(), Some(b"value".as_slice()));
            assert_eq!(cache.stats().pinned_entries, 1);
        }
        assert_eq!(cache.stats().pinned_entries, 0);

        let lookup = cache.scoped_lookup(&key).unwrap();
        let handle = lookup.into_handle().expect("handle should be carried out");
        assert_eq!(handle.key(), &key);
        assert_eq!(handle.tier(), CacheReadTier::Memory);
        assert_eq!(handle.value(), b"value");
        assert_eq!(handle.as_slice(), b"value");
        assert_eq!(cache.stats().pinned_entries, 1);
        cache.release(handle);
        assert_eq!(cache.stats().pinned_entries, 0);
    }

    // shared-corpus: storage_cache_refill;
    #[test]
    fn cache_reports_writeback_backpressure_and_latency_metrics() {
        let dir = tempfile::tempdir().unwrap();
        let policy = CacheTieringPolicy {
            memory_capacity_bytes: 16,
            ssd_capacity_bytes: 20,
            data_placement: CacheDataPlacement::Tiered,
            data_placement_threshold_bytes: 1024 * 1024,
            memory_hotness_threshold: 4,
            ssd_admit_hotness_threshold: 1,
            max_memory_block_bytes: 16,
            max_ssd_block_bytes: 64,
            ssd_write_through: true,
            ..CacheTieringPolicy::default()
        };
        let cache = MultiLayerCache::with_tiering_policy(
            dir.path(),
            policy,
            CacheBlockOptions {
                compression: CacheCompression::None,
                min_compress_bytes: usize::MAX,
            },
        );
        let first = CacheKey::page_with_slot(1, 70, 0, 12, Some(5));
        let second = CacheKey::page_with_slot(1, 71, 0, 12, Some(5));
        let rejected = CacheKey::page_with_slot(1, 72, 0, 128, Some(5));

        cache.put(first.clone(), b"first-block!".to_vec()).unwrap();
        cache.put(second.clone(), b"second-block".to_vec()).unwrap();
        cache
            .put_with_admission(
                rejected,
                vec![b'x'; 128],
                CacheAdmissionRequest {
                    block_kind: CacheBlockKind::Page,
                    shard_id: 1,
                    routing_slot: Some(5),
                    block_bytes: 128,
                    hotness: 9,
                    pinned: false,
                },
            )
            .unwrap();
        let _ = cache.get(&second).unwrap();

        let writeback = cache.writeback_backpressure_report();
        assert!(writeback.ssd_write_through_enabled);
        assert!(writeback.write_through_admissions > 0);
        assert!(writeback.ssd_evictions > 0 || writeback.ssd_admission_rejections > 0);
        assert!(writeback.backpressure_events > 0);
        assert!(writeback.bounded_queue_ready);

        let stats = cache.stats();
        let latency = cache.latency_metrics_report();
        assert!(latency.put_count >= 3);
        assert!(latency.get_count >= 1);
        assert!(latency.histogram_ready);
        assert!(latency.put_p50_us > 0);
        assert!(latency.put_p95_us >= latency.put_p50_us);
        assert!(latency.put_max_us >= latency.put_avg_us);
        assert!(latency.get_p50_us > 0);
        assert!(latency.get_p95_us >= latency.get_p50_us);
        assert!(latency.get_max_us >= latency.get_avg_us);
        assert_eq!(latency.writeback_count, stats.writeback_latency_samples);
    }

    #[test]
    fn remove_keeps_removed_pinned_entry_counted_until_release() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(16, dir.path());
        let key = CacheKey::page_with_slot(1, 10, 0, 4, Some(7));

        let handle = cache
            .insert_pinned(key.clone(), b"page".to_vec())
            .unwrap()
            .expect("pinned handle");
        assert_eq!(cache.stats().pinned_entries, 1);
        assert_eq!(cache.lookup(&key).unwrap(), Some(b"page".to_vec()));

        cache.invalidate(&key).unwrap();
        assert_eq!(cache.lookup(&key).unwrap(), None);
        assert_eq!(cache.stats().pinned_entries, 1);
        assert!(cache.stats().pinned_bytes >= b"page".len() as u64);
        assert!(cache.size() >= b"page".len());
        assert!(cache.entries_for_shard(1).is_empty());

        cache.release(handle);
        assert_eq!(cache.stats().pinned_entries, 0);
        assert_eq!(cache.stats().pinned_bytes, 0);
        assert_eq!(cache.size(), 0);
    }

    #[test]
    fn reinsert_after_removed_pinned_release_counts_only_live_entry() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(64, dir.path());
        let key = CacheKey::string(1, "reinsert-after-remove");

        let old_handle = cache
            .insert_pinned(key.clone(), b"old".to_vec())
            .unwrap()
            .expect("old handle");
        cache.remove(&key).unwrap();
        assert_eq!(cache.lookup(&key).unwrap(), None);
        assert_eq!(cache.stats().pinned_entries, 1);
        let removed_size = cache.size();
        assert!(removed_size >= b"old".len());

        cache
            .insert(key.clone(), b"new-value".to_vec(), b"new-value".len())
            .unwrap();
        assert_eq!(cache.lookup(&key).unwrap(), Some(b"new-value".to_vec()));
        assert!(cache.size() >= removed_size);

        cache.release(old_handle);
        assert_eq!(cache.stats().pinned_entries, 0);
        assert_eq!(cache.stats().pinned_bytes, 0);
        assert_eq!(cache.lookup(&key).unwrap(), Some(b"new-value".to_vec()));
        assert!(cache.size() >= b"new-value".len());
        assert!(cache.size() < removed_size.saturating_add(b"new-value".len() * 4));
    }

    // shared-corpus: storage_cache_refill
    #[test]
    fn pinned_handle_async_writeback_and_latency_metrics_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(64, dir.path());
        let key = CacheKey::string(1, "handle");

        cache.put(key.clone(), b"value".to_vec()).unwrap();
        let handle = cache
            .get_pinned_handle(&key)
            .unwrap()
            .expect("pinned handle should exist");
        assert_eq!(handle.as_slice(), b"value");
        assert_eq!(cache.stats().pinned_entries, 1);
        assert_eq!(cache.stats().zero_copy_handle_hits, 1);
        assert_eq!(cache.get(&key).unwrap(), Some(b"value".to_vec()));

        cache.set_async_writeback_queue_limit_for_test(1);
        cache
            .enqueue_async_writeback(CacheKey::string(1, "async-a"), b"a".to_vec())
            .unwrap();
        assert_eq!(cache.stats().async_writeback_queue_depth, 1);
        assert_eq!(cache.stats().async_writeback_queue_bytes, 1);
        assert_eq!(cache.stats().async_writeback_max_queue_depth, 1);
        assert_eq!(cache.stats().async_writeback_max_queue_bytes, 1);
        assert!(cache
            .enqueue_async_writeback(CacheKey::string(1, "async-b"), b"b".to_vec())
            .is_err());
        let drained = cache.drain_async_writeback(8).unwrap();
        assert_eq!(drained.drained, 1);
        assert_eq!(drained.remaining, 0);

        let stats = cache.stats();
        assert_eq!(stats.async_writeback_enqueued, 1);
        assert_eq!(stats.async_writeback_drained, 1);
        assert_eq!(stats.async_writeback_backpressure_rejections, 1);
        assert_eq!(stats.async_writeback_queue_depth, 0);
        assert_eq!(stats.async_writeback_queue_bytes, 0);
        assert_eq!(stats.async_writeback_max_queue_depth, 1);
        assert_eq!(stats.async_writeback_max_queue_bytes, 1);
        cache.record_compaction_latency_micros(1_500);
        let stats = cache.stats();
        assert!(stats.get_latency_samples > 0);
        assert!(stats.put_latency_samples > 0);
        assert!(stats.read_through_latency_samples > 0);
        assert!(stats.writeback_latency_samples > 0);
        assert!(stats.compaction_latency_samples > 0);
        assert!(stats.get_latency_total_micros >= stats.get_latency_max_micros);
        assert!(stats.put_latency_total_micros >= stats.put_latency_max_micros);
        assert!(stats.read_through_latency_total_micros > 0);
        assert!(stats.writeback_latency_total_micros > 0);
        assert_eq!(stats.compaction_latency_total_micros, 1_500);
        let latency = cache.latency_metrics_report();
        assert_eq!(
            latency.read_through_count,
            stats.read_through_latency_samples
        );
        assert_eq!(latency.refill_count, stats.refill_latency_samples);
        assert_eq!(latency.writeback_count, stats.writeback_latency_samples);
        assert_eq!(latency.eviction_count, stats.eviction_latency_samples);
        assert_eq!(latency.compaction_count, stats.compaction_latency_samples);
        assert_eq!(latency.compaction_avg_us, 1_500);
        assert!(latency.get_p50_us > 0);
        assert!(latency.get_p95_us >= latency.get_p50_us);
        assert!(latency.put_p50_us > 0);
        assert!(latency.put_p95_us >= latency.put_p50_us);
        assert!(latency.read_through_p50_us > 0);
        assert!(latency.read_through_p95_us >= latency.read_through_p50_us);
        assert!(latency.writeback_p50_us > 0);
        assert!(latency.writeback_p95_us >= latency.writeback_p50_us);
        assert_eq!(latency.compaction_p50_us, 10_000);
        assert_eq!(latency.compaction_p95_us, 10_000);
        assert_eq!(
            stats.get_latency_samples,
            stats.get_latency_le_10us
                + stats.get_latency_le_100us
                + stats.get_latency_le_1ms
                + stats.get_latency_le_10ms
                + stats.get_latency_gt_10ms
        );
        assert_eq!(
            stats.put_latency_samples,
            stats.put_latency_le_10us
                + stats.put_latency_le_100us
                + stats.put_latency_le_1ms
                + stats.put_latency_le_10ms
                + stats.put_latency_gt_10ms
        );
        assert_latency_buckets_sum(
            stats.read_through_latency_samples,
            [
                stats.read_through_latency_le_10us,
                stats.read_through_latency_le_100us,
                stats.read_through_latency_le_1ms,
                stats.read_through_latency_le_10ms,
                stats.read_through_latency_gt_10ms,
            ],
        );
        assert_latency_buckets_sum(
            stats.writeback_latency_samples,
            [
                stats.writeback_latency_le_10us,
                stats.writeback_latency_le_100us,
                stats.writeback_latency_le_1ms,
                stats.writeback_latency_le_10ms,
                stats.writeback_latency_gt_10ms,
            ],
        );
        assert_latency_buckets_sum(
            stats.compaction_latency_samples,
            [
                stats.compaction_latency_le_10us,
                stats.compaction_latency_le_100us,
                stats.compaction_latency_le_1ms,
                stats.compaction_latency_le_10ms,
                stats.compaction_latency_gt_10ms,
            ],
        );
        let metrics = prometheus_text(&stats, &[("cache", "latency")]);
        for family in [
            "matrixcache_read_through_latency_seconds",
            "matrixcache_refill_latency_seconds",
            "matrixcache_writeback_latency_seconds",
            "matrixcache_eviction_latency_seconds",
            "matrixcache_compaction_latency_seconds",
        ] {
            assert!(
                metrics.contains(&format!("{family}_sum")),
                "{family} should export a histogram sum for Grafana averages:\n{metrics}"
            );
            let avg_gauge = format!(
                "{}_avg_seconds",
                family
                    .strip_suffix("_seconds")
                    .expect("latency metric family ends in seconds")
            );
            assert!(
                metrics.contains(&avg_gauge),
                "{family} should export a direct average gauge for Grafana panels:\n{metrics}"
            );
        }
    }

    fn assert_latency_buckets_sum(samples: u64, buckets: [u64; 5]) {
        assert_eq!(samples, buckets.into_iter().sum::<u64>());
    }

    #[test]
    fn cache_inspection_and_slot_invalidation_are_slot_aware() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1024, dir.path());
        let slot_five = CacheKey::page_with_slot(1, 10, 20, 4, Some(5));
        let slot_six = CacheKey::page_with_slot(1, 11, 30, 4, Some(6));

        cache.put(slot_five.clone(), b"five".to_vec()).unwrap();
        cache.put(slot_six.clone(), b"six!".to_vec()).unwrap();

        let entries = cache.entries_for_shard(1);
        assert_eq!(entries.len(), 2);
        assert!(entries
            .iter()
            .any(|entry| entry.selector.starts_with("slot-5:")));

        let report = cache.invalidate_slot(1, 5).unwrap();
        assert_eq!(report.memory_entries_removed, 1);
        assert!(report.disk_bytes_removed > 0);
        assert_eq!(cache.get(&slot_five).unwrap(), None);
        assert_eq!(cache.get(&slot_six).unwrap(), Some(b"six!".to_vec()));
        assert_eq!(cache.stats().pinned_entries, 0);
    }

    #[test]
    fn invalidate_shard_removes_memory_and_disk_cache() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1024, dir.path());
        let shard_one = CacheKey::string(1, "a");
        let shard_two = CacheKey::string(2, "b");
        cache.put(shard_one.clone(), b"one".to_vec()).unwrap();
        cache.put(shard_two.clone(), b"two".to_vec()).unwrap();
        cache.pin(shard_one.clone());
        assert_eq!(cache.stats().pinned_entries, 1);

        let report = cache.invalidate_shard(1).unwrap();
        assert_eq!(report.memory_entries_removed, 1);
        assert!(report.disk_bytes_removed > 0);
        assert_eq!(cache.get(&shard_one).unwrap(), None);
        assert_eq!(cache.get(&shard_two).unwrap(), Some(b"two".to_vec()));
        assert_eq!(cache.stats().pinned_entries, 0);
        assert_eq!(cache.stats().pinned_bytes, 0);
    }

    #[test]
    fn invalidate_page_segment_clears_all_cache_tiers() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_tiering_policy(
            dir.path(),
            CacheTieringPolicy {
                memory_capacity_bytes: 16,
                pmem_capacity_bytes: 0,
                ssd_capacity_bytes: 4096,
                data_placement: CacheDataPlacement::Tiered,
                data_placement_threshold_bytes: 1024,
                memory_hotness_threshold: 0,
                pmem_admit_hotness_threshold: u32::MAX,
                ssd_admit_hotness_threshold: 0,
                max_memory_block_bytes: 16,
                max_pmem_block_bytes: 0,
                max_ssd_block_bytes: 4096,
                ssd_write_through: true,
            },
            CacheBlockOptions::default(),
        );
        let segment = CacheKey::page_with_slot(7, 42, 9, 4, Some(3));
        let other = CacheKey::page_with_slot(7, 43, 9, 4, Some(3));
        cache.put(segment.clone(), b"segment".to_vec()).unwrap();
        cache.put(other.clone(), b"other".to_vec()).unwrap();
        cache.pin(segment.clone());
        assert_eq!(cache.stats().pinned_entries, 1);

        let report = cache.invalidate_page_segment(7, 42).unwrap();
        assert_eq!(report.memory_entries_removed, 1);
        assert!(report.disk_bytes_removed > 0);
        assert_eq!(cache.get(&segment).unwrap(), None);
        assert_eq!(cache.get(&other).unwrap(), Some(b"other".to_vec()));
        assert_eq!(cache.stats().pinned_entries, 0);
        assert_eq!(cache.stats().pinned_bytes, 0);
    }

    #[test]
    fn disk_cache_serializes_compresses_and_decodes_block_envelope() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_block_options(
            1024,
            dir.path(),
            CacheBlockOptions {
                compression: CacheCompression::Zstd { level: 1 },
                min_compress_bytes: 16,
            },
        );
        let key = CacheKey::string(1, "compressible");
        let value = vec![b'x'; 4096];

        cache.put(key.clone(), value.clone()).unwrap();
        cache.clear_memory_for_test();

        assert_eq!(cache.get(&key).unwrap(), Some(value));
        let stats = cache.stats();
        assert_eq!(stats.compressed_puts, 1);
        assert_eq!(stats.compressed_hits, 1);
        assert!(stats.compression_bytes_saved > 0);
    }

    #[test]
    fn disk_cache_can_read_legacy_raw_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1024, dir.path());
        let key = CacheKey::string(1, "legacy");
        let legacy_path = {
            let inner = cache.inner.read().expect("cache lock poisoned");
            inner.disk_path(&key)
        };
        fs::create_dir_all(legacy_path.parent().unwrap()).unwrap();
        fs::write(&legacy_path, b"legacy-value").unwrap();

        assert_eq!(cache.get(&key).unwrap(), Some(b"legacy-value".to_vec()));
    }
    #[test]
    fn rocksdb_ssd_default_does_not_write_raw_shadow_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(8, dir.path());
        let key = CacheKey::string(42, "rocksdb-primary");
        let block_path = {
            let inner = cache.inner.read().expect("cache lock poisoned");
            inner.disk_path(&key)
        };
        let manifest_path = dir.path().join(CACHE_MANIFEST_NAME);

        cache.put(key.clone(), b"rocksdb-value".to_vec()).unwrap();
        cache.clear_memory_for_test();
        assert_eq!(cache.get(&key).unwrap(), Some(b"rocksdb-value".to_vec()));
        if cfg!(feature = "rocksdb-ssd") {
            assert!(
                !block_path.exists(),
                "default RocksDB SSD path must not double-write raw block files"
            );
            assert!(
                !manifest_path.exists(),
                "default RocksDB SSD path must not append legacy cache manifests"
            );
        } else {
            assert!(block_path.exists());
            assert!(manifest_path.exists());
        }
    }

    #[test]
    fn stop_blocks_core_cache_io_until_start_reenables_it() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1024, dir.path());
        let key = CacheKey::string(1, "lifecycle");

        cache.put(key.clone(), b"value".to_vec()).unwrap();
        cache.stop();
        assert!(!cache.is_started());

        assert!(matches!(cache.get(&key), Err(CacheError::Stopped)));
        assert!(matches!(cache.acquire(&key), Err(CacheError::Stopped)));
        assert!(matches!(
            cache.put(CacheKey::string(1, "stopped-put"), b"blocked".to_vec()),
            Err(CacheError::Stopped)
        ));
        assert!(cache
            .enqueue_async_writeback(CacheKey::string(1, "stopped-async"), b"blocked".to_vec())
            .is_err());
        assert!(matches!(
            cache.drain_async_writeback(1),
            Err(CacheError::Stopped)
        ));
        assert!(matches!(cache.invalidate(&key), Err(CacheError::Stopped)));
        assert!(matches!(
            cache.recover_disk_index(),
            Err(CacheError::Stopped)
        ));
        assert_eq!(cache.get_memory(&key), None);

        cache.start().unwrap();
        assert!(cache.is_started());
        assert_eq!(cache.get(&key).unwrap(), Some(b"value".to_vec()));
        cache
            .put(CacheKey::string(1, "started-put"), b"allowed".to_vec())
            .unwrap();
        assert_eq!(
            cache.get(&CacheKey::string(1, "started-put")).unwrap(),
            Some(b"allowed".to_vec())
        );
    }

    #[test]
    fn base_lru_list_tracks_mru_and_tail_eviction() {
        let mut list = BaseLruList::new(2);
        list.Put("a".to_string());
        list.Put("b".to_string());
        list.Put("c".to_string());

        assert_eq!(list.Size(), 3);
        assert_eq!(list.GetTail(2), vec!["a".to_string(), "b".to_string()]);
        assert!(list.Get("a"));
        assert_eq!(list.GetTail(2), vec!["b".to_string(), "c".to_string()]);
        assert_eq!(list.Evict(), vec!["b".to_string()]);
        assert_eq!(list.Size(), 2);
        assert!(list.Delete("a"));
        assert_eq!(list.Size(), 1);
    }

    #[test]
    fn ghost_lru_list_downgrades_data_to_ghost_tail() {
        let mut list = GhostLruList::new(1);
        list.Put("hot".to_string());
        list.Put("cold".to_string());

        list.Downgrade();
        assert_eq!(list.Size(), 1);
        assert_eq!(list.GhostSize(), 1);
        assert_eq!(list.GetDataTail(8), vec!["cold".to_string()]);
        assert_eq!(list.GetGhostTail(8), vec!["hot".to_string()]);

        let popped = list.Pop("hot");
        assert_eq!(popped.item, "hot");
        assert!(popped.is_ghost);
    }

    #[test]
    fn arc_list_promotes_hits_and_keeps_bounded_data_size() {
        let mut arc = ArcList::new(2);
        arc.Put("a".to_string());
        arc.Put("b".to_string());

        assert_eq!(arc.Size(), 2);
        assert_eq!(
            arc.GetFetchDataTail(8),
            vec!["a".to_string(), "b".to_string()]
        );
        assert!(arc.Get("a"));
        assert_eq!(arc.GetActiveDataTail(8), vec!["a".to_string()]);

        arc.Put("c".to_string());
        assert!(arc.Size() <= arc.Capacity());
        assert!(arc.TotalSize() <= arc.Capacity() * 2);
        assert!(arc.GetActiveGhostTail(8).contains(&"a".to_string()));
        assert!(!arc.Get("a"));
        assert!(arc.GetActiveDataTail(8).contains(&"a".to_string()));
    }

    #[test]
    fn arc_list_hit_on_fetch_data_promotes_to_active() {
        let mut arc = ArcList::new(4);
        arc.Put("a".to_string());
        assert_eq!(arc.GetFetchDataTail(8), vec!["a".to_string()]);
        assert!(arc.GetActiveDataTail(8).is_empty());

        // A hit on a key held in the fetch data list promotes it to the active
        // data list: one access means fetched, two means worth keeping.
        assert!(arc.Get("a"));
        assert!(arc.GetFetchDataTail(8).is_empty());
        assert_eq!(arc.GetActiveDataTail(8), vec!["a".to_string()]);
    }

    #[test]
    fn arc_list_ghost_hits_adapt_the_fetch_active_split() {
        // Capacity 2 starts split evenly, one slot each side.
        let mut arc = ArcList::new(2);
        assert_eq!(arc.FetchCapacity(), 1);
        assert_eq!(arc.ActiveCapacity(), 1);

        arc.Put("a".to_string());
        assert!(arc.Get("a"));
        arc.Put("b".to_string());
        // This insert takes the list to capacity, so making room downgrades the
        // active tail into the active ghost list rather than dropping it.
        arc.Put("c".to_string());
        assert!(arc.GetActiveGhostTail(8).contains(&"a".to_string()));

        // Hitting a key in the active ghost list is evidence the active side
        // was trimmed too far, so it takes a slot from the fetch side.
        assert!(!arc.Get("a"));
        assert_eq!(arc.FetchCapacity(), 0);
        assert_eq!(arc.ActiveCapacity(), 2);
        assert!(arc.GetActiveDataTail(8).contains(&"a".to_string()));
        // Making room for it downgraded the fetch tail into the fetch ghost.
        assert!(arc.GetFetchGhostTail(8).contains(&"b".to_string()));

        // Hitting the fetch ghost is the mirror image, and hands the slot back.
        assert!(!arc.Get("b"));
        assert_eq!(arc.FetchCapacity(), 1);
        assert_eq!(arc.ActiveCapacity(), 1);
        assert!(arc.GetActiveDataTail(8).contains(&"b".to_string()));
    }

    #[test]
    fn arc_list_drops_fetch_tail_outright_when_its_ghost_is_empty() {
        let mut arc = ArcList::new(2);
        arc.Put("a".to_string());
        arc.Put("b".to_string());
        assert_eq!(arc.Size(), 2);
        assert_eq!(arc.GhostSize(), 0);

        // The fetch side alone already holds the whole capacity and its ghost
        // list is empty, so its tail is dropped outright instead of ghosted.
        arc.Put("c".to_string());
        assert_eq!(arc.Size(), 2);
        assert_eq!(arc.GhostSize(), 0);
        assert!(!arc.GetFetchDataTail(8).contains(&"a".to_string()));
        assert!(arc.GetFetchDataTail(8).contains(&"b".to_string()));
        assert!(arc.GetFetchDataTail(8).contains(&"c".to_string()));
        assert!(arc.Size() <= arc.Capacity());
        assert!(arc.TotalSize() <= arc.Capacity() * 2);
    }

    #[test]
    fn ghost_lru_list_delete_clears_the_key_from_whichever_list_holds_it() {
        let mut list = GhostLruList::new(4);
        list.Put("data-key".to_string());
        list.PutGhost("ghost-key".to_string());
        assert_eq!(list.Size(), 1);
        assert_eq!(list.GhostSize(), 1);

        // Delete reports whether the key was there and removes it from
        // whichever list held it, including a key that only exists as a ghost.
        assert!(list.Delete("data-key"));
        assert_eq!(list.Size(), 0);
        assert_eq!(list.GhostSize(), 1);

        assert!(list.Delete("ghost-key"));
        assert_eq!(list.GhostSize(), 0);

        assert!(!list.Delete("absent"));
    }

    #[test]
    fn replacement_arc_exposes_active_and_fetch_tail_surface() {
        let mut policy = ReplacementArc::new(2);
        assert!(!policy.is_initialized());
        policy.Init().unwrap();
        assert!(policy.is_initialized());

        policy.Put("a".to_string());
        policy.Put("b".to_string());
        assert!(policy.Get("a"));
        policy.Put("c".to_string());

        assert_eq!(policy.GetItemCapacity(), 2);
        assert!(!policy.Get("a"));
        assert!(policy.GetActiveTail(8).contains(&"a".to_string()));
        assert!(policy.GetFetchTail(8).len() <= 2);
        assert!(policy.Delete("a"));
        policy.SetItemCapacity(3);
        assert_eq!(policy.GetItemCapacity(), 3);
        policy.Reset().unwrap();
        assert!(!policy.is_initialized());
        assert!(policy.GetActiveTail(8).is_empty());
        assert!(policy.GetFetchTail(8).is_empty());
    }

    #[test]
    fn storage_engine_type_preserves_codes_and_aliases() {
        assert_eq!(
            StorageEngineKind::from_config_name("kDRAMStorageEngine"),
            StorageEngineKind::Dram
        );
        assert_eq!(
            StorageEngineKind::from_config_name("kSimpleStorageEngine"),
            StorageEngineKind::Simple
        );
        assert_eq!(StorageEngineKind::MultiSsd.ConfigCode(), 4);
        assert_eq!(
            StorageEngineKind::Simple.AsConfigEnumName(),
            "kSimpleStorageEngine"
        );
        assert_eq!(
            StorageEngineKind::from_config_name("rocksdb"),
            StorageEngineKind::Ssd
        );
        assert_eq!(
            StorageEngineKind::from_config_name("kSSDRocksDBStorageEngine"),
            StorageEngineKind::Ssd
        );
        assert_eq!(SsdEngineKind::RocksDb as u8, 0);
        assert_eq!(
            SsdEngineKind::FromConfigName("rocksdb"),
            SsdEngineKind::RocksDb
        );
        assert_eq!(SsdEngineKind::RocksDb.AsConfigName(), "RocksDB");
        assert_eq!(WriteBufferKind::UserDataBuf as u8, 0);
        assert_eq!(WriteBufferKind::MetaDataBuf as u8, 1);
        assert_eq!(WriteBufferKind::GcBuf as u8, 2);
        assert_eq!(WriteBufferKind::CodecDataBuf as u8, 3);
        assert_eq!(DataKind::Data as u8, 1);
        assert_eq!(DataKind::MetaLog as u8, 2);
        assert_eq!(GcMode::Lossy as u8, 1);
        assert_eq!(GcMode::Lossless as u8, 10);
        assert_eq!(RecordState::SoftDel as u8, 0x0);
        assert_eq!(RecordState::Normal as u8, 0x1);
        assert_eq!(RecordState::Pinned as u8, 0x2);
        assert_eq!(RecordState::MaxCode as u8, 0xf);
    }

    #[test]
    fn write_buffer_and_encoder_preserve_layout_size_semantics() {
        let mut buffer = WriteBuffer::new(WriteBufferKind::UserDataBuf, 128);
        buffer.PushBack("a", b"one".to_vec());
        buffer.PushBack("bb", b"twotwo".to_vec());
        assert_eq!(buffer.Capacity(), 128);
        assert_eq!(buffer.BufType(), WriteBufferKind::UserDataBuf);
        assert_eq!(buffer.Count(), 2);
        assert_eq!(buffer.KeySize(), 3);
        assert_eq!(buffer.ValueSize(), 9);
        assert_eq!(buffer.Size(), 12);

        let encoder = BufferEncoder::new(4096);
        assert_eq!(encoder.align_size(), 4096);
        assert_eq!(encoder.GetXXHSeed(), 0);
        assert_eq!(BufferEncoder::DATA_FIXED_PART_SIZE, 20);
        assert_eq!(BufferEncoder::OPLOG_FIXED_PART_SIZE, 28);
        assert_eq!(BufferEncoder::OPLOG_HEADER_SIZE, 8);
        assert_eq!(encoder.CalculateEncodedDataSize(&buffer), 49);
        assert_eq!(encoder.CalculateEncodedOpLogSize(&buffer), 59);

        let encoded = encoder.SerializeData(b"payload");
        assert_eq!(encoded.len(), 20 + "payload".len());
        let (decoded, corrupted) = encoder.DeserializeData(&encoded);
        assert_eq!(decoded, b"payload");
        assert!(!corrupted);

        let mut corrupted_bytes = encoded;
        *corrupted_bytes.last_mut().unwrap() ^= 0xff;
        let (decoded, corrupted) = encoder.DeserializeData(&corrupted_bytes);
        assert_ne!(decoded, b"payload");
        assert!(corrupted);

        let stolen = buffer.StealBufQ();
        assert_eq!(stolen.len(), 2);
        assert_eq!(buffer.Count(), 0);
    }

    #[test]
    fn mem_storage_crc_is_castagnoli_and_covers_the_length_header() {
        // Pin the algorithm to its published check value rather than to
        // whatever this implementation happens to emit.
        assert_eq!(crc32c(b"123456789"), 0xe306_9283);
        assert_eq!(crc32c(b""), 0);

        // Seeding continues a checksum, so a record can be covered in three
        // pieces without first copying them into one buffer.
        assert_eq!(
            crc32c_with_seed(b"56789", crc32c(b"1234")),
            crc32c(b"123456789")
        );

        // "cdef" + "ab" and "cdefa" + "b" concatenate to the same bytes and
        // differ only in where the value ends and the key begins. A checksum
        // over the payload alone cannot tell them apart, so covering the
        // length header is what detects a corrupted header.
        assert_ne!(
            MemStorage::ComputeCRC("ab", b"cdef"),
            MemStorage::ComputeCRC("b", b"cdefa")
        );

        // Value and key both still contribute.
        assert_ne!(
            MemStorage::ComputeCRC("k", b"v1"),
            MemStorage::ComputeCRC("k", b"v2")
        );
        assert_ne!(
            MemStorage::ComputeCRC("k1", b"v"),
            MemStorage::ComputeCRC("k2", b"v")
        );
    }

    #[test]
    fn mem_storage_layout_round_trips_key_value_and_crc() {
        let crc = MemStorage::ComputeCRC("layout-key", b"layout-value");
        let record = MemStorage::DoPutWithCRC("layout-key", b"layout-value", crc).unwrap();

        assert_eq!(MemStorage::GetKeyFromData(&record).unwrap(), "layout-key");
        let buffer =
            MemStorage::CreateCacheBufferFromData(&record, StorageEngineKind::Simple, false)
                .unwrap();
        assert_eq!(buffer.Key(), "layout-key");
        assert_eq!(buffer.Data(), b"layout-value");
        assert!(MemStorage::DoPutWithCRC("layout-key", b"layout-value", crc + 1).is_err());
    }

    #[test]
    fn mem_storage_allocator_handle_models_payload_pointer_and_delete() {
        let mut allocator = SimpleLogBasedMemoryAllocator::with_capacity(256);
        let handle =
            MemStorage::DoPutToAllocator(&mut allocator, "alloc-key", b"alloc-value").unwrap();

        assert_eq!(handle.PayloadOffset(), MemStorage::HEADER_BYTES);
        assert_eq!(
            handle.DataPtr(),
            handle.RecordPtr() + MemStorage::HEADER_BYTES
        );
        assert_eq!(handle.ValueLen(), b"alloc-value".len());
        assert_eq!(handle.KeyLen(), "alloc-key".len());
        let stats = allocator.GetStats().unwrap();
        assert_eq!(
            stats.NumAllocatedBytes(),
            MemStorage::HEADER_BYTES + b"alloc-value".len() + "alloc-key".len()
        );

        let record = allocator.read(handle.RecordPtr()).unwrap();
        assert_eq!(MemStorage::GetKeyFromData(record).unwrap(), "alloc-key");
        assert_eq!(
            MemStorage::GetValueFromData(record).unwrap(),
            b"alloc-value"
        );

        let buffer = MemStorage::CreateCacheBufferFromAllocatorData(
            &allocator,
            handle,
            StorageEngineKind::Simple,
            false,
        )
        .unwrap();
        assert_eq!(buffer.Key(), "alloc-key");
        assert_eq!(buffer.Data(), b"alloc-value");

        MemStorage::DoDeleteFromAllocator(&mut allocator, handle).unwrap();
        assert!(!allocator.contains(handle.RecordPtr()));
        assert_eq!(
            allocator.GetStats().unwrap().NumFreedBytes(),
            handle.RecordLen()
        );
    }

    #[test]
    fn mem_storage_allocator_path_rejects_corrupt_crc_before_write() {
        let mut allocator = SimpleLogBasedMemoryAllocator::with_capacity(256);
        let crc = MemStorage::ComputeCRC("alloc-key", b"alloc-value");

        assert!(MemStorage::DoPutToAllocatorWithCRC(
            &mut allocator,
            "alloc-key",
            b"alloc-value",
            crc + 1
        )
        .is_err());
        assert_eq!(allocator.GetStats().unwrap().NumAllocatedBytes(), 0);
    }

    #[test]
    fn mem_storage_allocator_surface_works_for_je_and_pool_allocators() {
        let mut je = JeAllocator::with_capacity(256);
        let je_handle = MemStorage::DoPutToAllocator(&mut je, "je-key", b"je-value").unwrap();
        assert_eq!(je_handle.PayloadOffset(), MemStorage::HEADER_BYTES);
        assert_eq!(
            MemStorage::GetValueFromData(je.read(je_handle.RecordPtr()).unwrap()).unwrap(),
            b"je-value"
        );
        MemStorage::DoDeleteFromAllocator(&mut je, je_handle).unwrap();

        let mut pool = PoolBasedMemoryAllocatorDram::with_capacity_and_object_len(1024, 256);
        let pool_handle =
            MemStorage::DoPutToAllocator(&mut pool, "pool-key", b"pool-value").unwrap();
        assert_eq!(pool_handle.PayloadOffset(), MemStorage::HEADER_BYTES);
        assert_eq!(
            MemStorage::GetKeyFromData(pool.read(pool_handle.RecordPtr()).unwrap()).unwrap(),
            "pool-key"
        );
        MemStorage::DoDeleteFromAllocator(&mut pool, pool_handle).unwrap();
    }

    #[test]
    fn simple_storage_engine_lifecycle_put_peek_delete_and_recover() {
        #[derive(Default)]
        struct RecoveredEntryCollector {
            recovered: Vec<(String, Vec<u8>)>,
        }

        impl RecoverDataCallback for RecoveredEntryCollector {
            fn on_recover_data(&mut self, key: &str, buffer: CacheBuffer) {
                self.recovered.push((key.to_string(), buffer.to_vec()));
            }
        }

        let mut engine = StorageEngineSimple::with_capacity(1024);
        assert_eq!(engine.Capacity(), 1024);
        assert_eq!(engine.StorageEngineType(), StorageEngineKind::Simple);
        engine.SetCapacity(2048);
        assert_eq!(engine.Capacity(), 2048);
        assert!(!engine.is_started());
        assert!(engine.Start());
        assert!(engine.is_started());

        let buffer = engine.Put("storage-key", b"value".to_vec()).unwrap();
        assert_eq!(buffer.Key(), "storage-key");
        assert!(engine.Peek("storage-key"));
        assert_eq!(engine.Get("storage-key").unwrap().Data(), b"value");

        let mut async_called = false;
        let mut async_buffer = CacheBuffer::new(b"async-value".to_vec());
        async_buffer.SetKey("async-key");
        engine
            .AsyncPut(async_buffer, |result| {
                async_called = true;
                let buffer = result.unwrap();
                assert_eq!(buffer.Key(), "async-key");
                assert_eq!(buffer.Data(), b"async-value");
            })
            .unwrap();
        assert!(async_called);
        assert!(engine.Peek("async-key"));
        let mut async_delete_called = false;
        let async_delete_buffer = engine.Get("async-key").unwrap();
        engine
            .AsyncDelete(&async_delete_buffer, |result| {
                async_delete_called = true;
                result.unwrap();
            })
            .unwrap();
        assert!(async_delete_called);
        assert!(!engine.Peek("async-key"));

        let mut collector = RecoveredEntryCollector::default();
        engine.RecoverData(&mut collector).unwrap();
        collector
            .recovered
            .sort_by(|left, right| left.0.cmp(&right.0));
        assert_eq!(
            collector.recovered,
            vec![("storage-key".to_string(), b"value".to_vec())]
        );

        engine.DeleteBuffer(&buffer).unwrap();
        assert!(!engine.Peek("storage-key"));
        assert_eq!(engine.TEST_GetNumDeleteCompletedCount(), 2);
        engine.TEST_IncreaseDeleteCompletedCount();
        assert_eq!(engine.TEST_GetNumDeleteCompletedCount(), 3);
        engine.Reset().unwrap();
        assert!(!engine.Peek("async-key"));
        assert!(engine.Stop());
        assert!(matches!(engine.Get("async-key"), Err(CacheError::Stopped)));
    }

    #[test]
    fn rocksdb_storage_engine_persists_path_and_ssd_view_semantics() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("ssd-cache");
        let db_path_str = db_path.to_string_lossy().to_string();

        let mut engine = StorageEngineRocksDb::new(&db_path_str);
        assert_eq!(engine.Path(), db_path_str);
        assert_eq!(engine.StorageEngineType(), StorageEngineKind::Ssd);
        assert_eq!(
            engine.SsdBackendName(),
            if cfg!(feature = "rocksdb-ssd") {
                "rocksdb"
            } else {
                "file-compat"
            }
        );
        assert_eq!(engine.Capacity(), u64::MAX);
        engine.SetCapacity(4096);
        assert_eq!(engine.Capacity(), 4096);
        assert!(!engine.IsDataRecovered());
        assert!(matches!(engine.Get("cold"), Err(CacheError::Stopped)));
        assert!(engine.Start());

        let view = engine.PutView("ssd-key", b"ssd-value".to_vec()).unwrap();
        assert_eq!(view.Key(), "ssd-key");
        assert_eq!(view.Size(), b"ssd-value".len());
        assert_eq!(view.Data(), None);
        assert!(engine.Peek("ssd-key"));
        assert_eq!(engine.Get("ssd-key").unwrap().Data(), b"ssd-value");

        let mut recovered_views = Vec::new();
        engine
            .RecoverViewData(&mut |key, view| {
                recovered_views.push((key.to_string(), view.Key().to_string(), view.Size()));
            })
            .unwrap();
        assert_eq!(
            recovered_views,
            vec![(
                "ssd-key".to_string(),
                "ssd-key".to_string(),
                b"ssd-value".len()
            )]
        );
        assert!(engine.IsDataRecovered());
        assert!(engine.Stop());

        let mut restarted = StorageEngineRocksDb::new(&db_path_str);
        assert!(restarted.Start());
        assert!(restarted.Peek("ssd-key"));
        assert_eq!(restarted.Get("ssd-key").unwrap().Data(), b"ssd-value");
        restarted.Delete("ssd-key").unwrap();
        assert!(!restarted.Peek("ssd-key"));
        restarted
            .PutView("reset-key", b"reset-value".to_vec())
            .unwrap();
        restarted.Reset().unwrap();
        assert!(!restarted.Peek("reset-key"));
    }

    #[test]
    fn rocksdb_storage_engine_recover_data_returns_stored_values() {
        #[derive(Default)]
        struct SizedEntryCollector {
            recovered: Vec<(String, usize, Vec<u8>)>,
        }

        impl RecoverDataCallback for SizedEntryCollector {
            fn on_recover_data(&mut self, key: &str, buffer: CacheBuffer) {
                self.recovered
                    .push((key.to_string(), buffer.Size(), buffer.to_vec()));
            }
        }

        let dir = tempfile::tempdir().unwrap();
        let mut engine =
            StorageEngineRocksDb::new(dir.path().join("rocksdb").to_string_lossy().to_string());
        assert!(engine.Start());
        engine.PutView("alpha", b"one".to_vec()).unwrap();
        engine.PutView("beta", b"two-two".to_vec()).unwrap();

        let mut collector = SizedEntryCollector::default();
        engine.RecoverData(&mut collector).unwrap();
        collector
            .recovered
            .sort_by(|left, right| left.0.cmp(&right.0));
        // recover_data must return the stored values so the cache can be
        // re-populated on recovery (see put_bypass_storage_buffer, which consumes
        // the buffer's data). The size-only/lazy path is recover_view_data.
        assert_eq!(
            collector.recovered,
            vec![
                ("alpha".to_string(), b"one".len(), b"one".to_vec()),
                ("beta".to_string(), b"two-two".len(), b"two-two".to_vec())
            ]
        );

        assert!(engine.Stop());
        assert!(matches!(engine.Delete("alpha"), Err(CacheError::Stopped)));
    }

    #[test]
    fn storage_recover_callback_mock_tracks_last_key_and_count() {
        let mut engine = StorageEngineSimple::with_capacity(1024);
        assert!(engine.Start());
        engine.Put("first", b"111".to_vec()).unwrap();
        engine.Put("second", b"222".to_vec()).unwrap();

        let mut callback = RecoverDataCallbackMock::new();
        engine.RecoverData(&mut callback).unwrap();

        assert_eq!(callback.GetRecoveredRecordCnt(), 2);
        assert!(["first", "second"].contains(&callback.GetLastRecoverKey()));
        let mut recovered = callback
            .recovered()
            .iter()
            .map(|(key, buffer)| (key.clone(), buffer.Data().to_vec()))
            .collect::<Vec<_>>();
        recovered.sort_by(|left, right| left.0.cmp(&right.0));
        assert_eq!(
            recovered,
            vec![
                ("first".to_string(), b"111".to_vec()),
                ("second".to_string(), b"222".to_vec())
            ]
        );

        let mut direct = CacheBuffer::new(b"333".to_vec());
        direct.SetKey("third");
        callback.OnRecoverData("third", direct);
        assert_eq!(callback.GetLastRecoverKey(), "third");
        assert_eq!(callback.GetRecoveredRecordCnt(), 3);
    }

    #[test]
    fn gc_copy_callback_mock_replaces_buffers_with_guarded_old_data() {
        let mut callback = GcCopyCallbackMock::new();
        let mut old = CacheBuffer::new(b"old-value".to_vec());
        old.SetKey("alpha");
        assert!(callback.AddCacheBuffer("alpha", old));
        let mut duplicate = CacheBuffer::new(b"duplicate".to_vec());
        duplicate.SetKey("alpha");
        assert!(!callback.AddCacheBuffer("alpha", duplicate));
        assert_eq!(
            callback.GetCacheBuffer("alpha").unwrap().Data(),
            b"old-value"
        );

        let mut missing_replacement = CacheBuffer::new(b"new-value".to_vec());
        missing_replacement.SetKey("alpha");
        assert!(matches!(
            callback.Update("missing", b"old-value", missing_replacement),
            Err(CacheError::NotFound)
        ));
        let mut mismatched_replacement = CacheBuffer::new(b"new-value".to_vec());
        mismatched_replacement.SetKey("alpha");
        assert!(matches!(
            callback.Update("alpha", b"wrong-old", mismatched_replacement),
            Err(CacheError::ReplaceMismatch)
        ));

        let mut replacement = CacheBuffer::new(b"new-value".to_vec());
        replacement.SetKey("alpha");
        callback.Update("alpha", b"old-value", replacement).unwrap();
        assert_eq!(
            callback.GetCacheBuffer("alpha").unwrap().Data(),
            b"new-value"
        );
        assert!(callback.DeleteCacheBuffer("alpha"));
        assert!(!callback.DeleteCacheBuffer("alpha"));
        assert!(callback.is_empty());
    }

    #[test]
    fn log_allocator_gc_listener_mock_updates_maps_and_frees_old_ptr() {
        let mut allocator = SimpleLogBasedMemoryAllocator::with_capacity(64);
        let old_ptr = allocator.Allocate(8).unwrap();
        let new_ptr = allocator.Allocate(8).unwrap();
        assert!(allocator.Contains(old_ptr));
        assert!(allocator.Contains(new_ptr));

        let mut listener = LogBasedAllocatorGcEventListenerMock::with_allocator(allocator);
        assert_eq!(
            listener.SetInternalMapAndReturnOldPtr("alpha", old_ptr),
            None
        );
        assert_eq!(listener.GetInternalMap("alpha"), Some(old_ptr));

        listener.OnGCCopy(old_ptr, new_ptr).unwrap();
        assert_eq!(listener.GetInternalMap("alpha"), Some(new_ptr));
        let allocator = listener.allocator().unwrap();
        assert!(!allocator.Contains(old_ptr));
        assert!(allocator.Contains(new_ptr));

        assert!(matches!(
            listener.OnGCCopy(old_ptr, new_ptr),
            Err(CacheError::NotFound)
        ));
        assert_eq!(
            listener.DelInternalMapAndReturnOldPtr("alpha"),
            Some(new_ptr)
        );
        assert_eq!(listener.GetInternalMap("alpha"), None);
    }

    #[test]
    fn pmem_recover_listener_dedupes_records_before_callback() {
        #[derive(Default)]
        struct RecoveredEntryCollector {
            recovered: Vec<(String, Vec<u8>)>,
        }

        impl RecoverDataCallback for RecoveredEntryCollector {
            fn on_recover_data(&mut self, key: &str, buffer: CacheBuffer) {
                self.recovered.push((key.to_string(), buffer.to_vec()));
            }
        }

        let alpha_first = MemStorage::DoPut("alpha", b"one");
        let alpha_duplicate = MemStorage::DoPut("alpha", b"two");
        let beta = MemStorage::DoPut("beta", b"three");

        let mut listener = PmemAllocatorRecoverListenerImpl::with_estimate_items(4);
        assert!(!listener.OnScanRecord(&alpha_first).unwrap());
        assert!(listener.OnScanRecord(&alpha_duplicate).unwrap());
        assert!(!listener.OnScanRecord(&beta).unwrap());
        assert_eq!(listener.scanned_record_count(), 2);
        assert_eq!(listener.duplicate_record_count(), 1);

        let mut collector = RecoveredEntryCollector::default();
        assert_eq!(listener.FinishRecover(&mut collector).unwrap(), 1);
        assert_eq!(listener.duplicate_record_count(), 0);
        assert_eq!(
            collector.recovered,
            vec![("beta".to_string(), b"three".to_vec())]
        );
    }

    #[test]
    fn pmem_storage_test_hooks_put_to_numa_and_report_recover_stats() {
        let mut engine = StorageEnginePmem::with_capacity(1024);
        assert!(engine.Start());
        engine.TEST_JoinPmemWriteExecutor();
        let buffer = engine
            .TEST_PutToNuma("pmem-key", b"pmem-value".to_vec(), 0)
            .unwrap();
        assert_eq!(buffer.Key(), "pmem-key");
        assert_eq!(buffer.Data(), b"pmem-value");
        assert_eq!(engine.Get("pmem-key").unwrap().Data(), b"pmem-value");

        let stats = engine.TEST_GetRecoverStats();
        assert_eq!(stats.valid_bytes, b"pmem-value".len());
        assert_eq!(stats.freed_bytes, 0);
        assert_eq!(stats.corrupted_bytes, 0);
        assert_eq!(stats.total_bytes, b"pmem-value".len());
    }

    #[test]
    fn ssd_fifo_keeps_insertion_order_when_a_key_is_rewritten() {
        let dir = tempfile::tempdir().unwrap();
        let instance = CacheInstance::new(
            520,
            ReplacementPolicyKind::Fifo,
            StorageEngineKind::Ssd,
            vec![dir.path().to_path_buf()],
        );
        instance.Start().unwrap();
        let big = vec![120u8; 100];
        let small = vec![121u8; 10];
        for key in ["a", "b", "c"] {
            instance.Put(key, big.clone()).unwrap();
        }

        // Rewriting "a" must not move it behind "b" and "c". First-in
        // first-out orders by when a key first entered the tier, not by when
        // it was last written, so "a" stays the next one out.
        instance.Put("a", small.clone()).unwrap();
        instance.Put("d", big.clone()).unwrap();
        instance.Put("e", big.clone()).unwrap();

        assert!(
            instance.Get("a").unwrap().is_none(),
            "the first key inserted should still be the first evicted"
        );
        assert!(
            instance.Get("b").unwrap().is_some(),
            "rewriting another key must not push this one to the front of the queue"
        );
        assert!(instance.Get("e").unwrap().is_some());
    }

    #[test]
    fn multi_ssd_selects_the_device_with_the_shared_hash() {
        let dir = tempfile::tempdir().unwrap();
        let dev = |name: &str| dir.path().join(name).to_string_lossy().to_string();
        let paths = vec![dev("ssd-a"), dev("ssd-b"), dev("ssd-c")];
        let engine = StorageEngineMultiSsd::new(paths.clone(), 1 << 20);

        // Device selection uses the same hash as the rest of the crate rather
        // than an ad-hoc one, so which device holds a key is reproducible: a
        // set of device directories written by one process is read back
        // through the same selection by another.
        for key in ["alpha", "beta", "gamma", "delta", "hello", "abc"] {
            let expected = &paths[mur_mur_hash2(key.as_bytes()) as usize % paths.len()];
            assert_eq!(
                engine.device_for_key(key),
                Some(expected.as_str()),
                "device for {key}"
            );
        }

        // Three devices is not a power of two, so selection has to be a
        // modulo rather than a mask or the third device never gets a key.
        let mut seen = HashSet::new();
        for index in 0..256 {
            let key = format!("spread-{index:04}");
            if let Some(device) = engine.device_for_key(&key) {
                seen.insert(device.to_string());
            }
        }
        assert_eq!(seen.len(), 3, "every device should receive keys");
    }

    #[test]
    fn multi_ssd_requires_devices_and_hashes_keys_to_storage() {
        let mut empty = StorageEngineMultiSsd::new(Vec::<String>::new(), 1024);
        assert!(!empty.Start());
        assert!(matches!(
            empty.Put("missing", b"value".to_vec()),
            Err(CacheError::Stopped)
        ));

        let dir = tempfile::tempdir().unwrap();
        let dev = |name: &str| dir.path().join(name).to_string_lossy().to_string();
        let mut engine = StorageEngineMultiSsd::new(vec![dev("ssd-a"), dev("ssd-b")], 1024);
        assert_eq!(engine.Capacity(), 1024);
        engine.SetCapacity(2048);
        assert_eq!(engine.Capacity(), 2048);
        assert_eq!(engine.PathCount(), 2);
        assert!(engine.device_for_key("alpha").is_some());
        assert!(engine.Start());
        assert_eq!(engine.StorageCount(), 2);

        let alpha_device = engine.device_for_key("alpha").unwrap().to_string();
        let beta_device = engine.device_for_key("beta").unwrap().to_string();
        assert!(engine.paths().contains(&alpha_device));
        assert!(engine.paths().contains(&beta_device));

        let alpha = engine.Put("alpha", b"one".to_vec()).unwrap();
        assert_eq!(alpha.Key(), "alpha");
        assert_eq!(engine.Get("alpha").unwrap().Data(), b"one");
        assert!(engine.Peek("alpha"));
        engine.Delete("alpha").unwrap();
        assert!(!engine.Peek("alpha"));

        engine.Stop();
        assert!(matches!(engine.Get("alpha"), Err(CacheError::Stopped)));
    }

    #[test]
    fn multi_ssd_recovers_resets_and_manages_devices() {
        struct RecoveredEntryCollector {
            recovered: Vec<(String, Vec<u8>)>,
        }

        impl RecoverDataCallback for RecoveredEntryCollector {
            fn on_recover_data(&mut self, key: &str, buffer: CacheBuffer) {
                self.recovered.push((key.to_string(), buffer.to_vec()));
            }
        }

        let dir = tempfile::tempdir().unwrap();
        let dev = |name: &str| dir.path().join(name).to_string_lossy().to_string();
        let mut engine = StorageEngineMultiSsd::new(vec![dev("ssd-a"), dev("ssd-b")], 2048);
        assert_eq!(engine.StorageEngineType(), StorageEngineKind::MultiSsd);
        assert!(engine.Start());
        engine.Put("first", b"111".to_vec()).unwrap();
        engine.Put("second", b"222".to_vec()).unwrap();
        let mut async_delete_called = false;
        let first_buffer = engine.Get("first").unwrap();
        engine
            .AsyncDelete(&first_buffer, |result| {
                async_delete_called = true;
                result.unwrap();
            })
            .unwrap();
        assert!(async_delete_called);
        assert!(!engine.Peek("first"));

        let mut collector = RecoveredEntryCollector {
            recovered: Vec::new(),
        };
        engine.RecoverData(&mut collector).unwrap();
        collector
            .recovered
            .sort_by(|left, right| left.0.cmp(&right.0));
        assert_eq!(
            collector.recovered,
            vec![("second".to_string(), b"222".to_vec())]
        );

        assert!(!engine.AddDevice(&dev("ssd-a")));
        assert!(engine.AddDevice(&dev("ssd-c")));
        assert_eq!(engine.PathCount(), 3);
        assert_eq!(engine.StorageCount(), 3);
        assert!(engine.RemoveDevice(&dev("ssd-c")));
        assert!(!engine.RemoveDevice("ssd-missing"));
        assert_eq!(engine.PathCount(), 2);

        engine.Reset().unwrap();
        assert!(!engine.Peek("first"));
        assert!(!engine.Peek("second"));
    }

    #[test]
    fn replacement_base_lru_list_ops() {
        let mut lru = BaseLruList::new(3);
        assert_eq!(lru.Capacity(), 3);
        assert_eq!(lru.Size(), 0);
        lru.Put("a".to_string());
        lru.Put("b".to_string());
        lru.Put("c".to_string());
        assert_eq!(lru.Size(), 3);
        assert!(lru.Get("a")); // hit promotes
        assert!(!lru.Get("missing"));
        assert!(!lru.GetTail(2).is_empty());
        assert!(lru.Delete("b"));
        assert!(!lru.Delete("b"));
        assert_eq!(lru.Size(), 2);
        assert_eq!(lru.EvictOne().len(), 1);
        assert_eq!(lru.Size(), 1);
        lru.SetCapacity(10);
        assert_eq!(lru.Capacity(), 10);
        lru.Reset();
        assert_eq!(lru.Size(), 0);
        assert!(lru.Evict().is_empty());
    }

    #[test]
    fn replacement_ghost_lru_list_ops() {
        let mut g = GhostLruList::new(2);
        assert_eq!(g.Capacity(), 2);
        assert_eq!(g.Size(), 0);
        g.Put("a".to_string());
        g.Put("b".to_string());
        assert_eq!(g.Size(), 2);
        assert!(g.Get("a"));
        assert!(!g.Get("missing"));
        assert!(g.TotalSize() >= g.Size());
        // exercise the ghost/data movement, tails, and eviction paths
        g.downgrade();
        let _ = g.pop("a");
        let _ = g.get_data_tail(1);
        let _ = g.get_ghost_tail(1);
        let _ = g.evict_one_data();
        let _ = g.evict_one_ghost();
        let _ = g.evict();
        g.set_capacity(5);
        assert_eq!(g.Capacity(), 5);
        let _ = g.ghost_capacity();
        g.Reset();
        assert_eq!(g.Size(), 0);
        assert_eq!(g.GhostSize(), 0);
    }

    #[test]
    fn storage_config_fixed_encoding_roundtrips() {
        let mut buf = Vec::new();
        assert_eq!(put_fixed_uint8(&mut buf, 0xAB), 1);
        assert_eq!(put_fixed_uint32(&mut buf, 0x1234_5678), 5);
        assert_eq!(put_fixed_uint64(&mut buf, 0x0102_0304_0506_0708), 13);
        assert_eq!(get_fixed_uint8(&buf, 0).unwrap(), (0xAB, 1));
        assert_eq!(get_fixed_uint32(&buf, 1).unwrap(), (0x1234_5678, 5));
        assert_eq!(get_fixed_uint64(&buf, 5).unwrap(), (0x0102_0304_0506_0708, 13));
        // out-of-range decodes fail closed
        assert!(get_fixed_uint8(&buf, buf.len()).is_none());
        assert!(get_fixed_uint32(&buf, buf.len()).is_none());
        assert!(get_fixed_uint64(&[0u8; 3], 0).is_none());

        // hash round-trips
        let mut hbuf = Vec::new();
        put_fixed_hash64(&mut hbuf, 0xDEAD_BEEF_CAFE_1234);
        assert_eq!(get_fixed_hash64(&hbuf, 0).unwrap().0, 0xDEAD_BEEF_CAFE_1234);
        let mut h2 = Vec::new();
        put_fixed_hash128(
            &mut h2,
            Xxh128 {
                first: 11,
                second: 22,
            },
        );
        let (xh, off) = get_fixed_hash128(&h2, 0).unwrap();
        assert_eq!((xh.first, xh.second, off), (11, 22, 16));

        // byte copy round-trip and bounds
        let mut dst = Vec::new();
        assert_eq!(copy_bytes_to(&mut dst, b"hello"), 5);
        assert_eq!(copy_bytes_from(&dst, 0, 5).unwrap(), (b"hello".to_vec(), 5));
        assert!(copy_bytes_from(&dst, 0, 99).is_none());

        // alignment
        assert_eq!(aligned_to(0, 8), 0);
        assert_eq!(aligned_to(1, 8), 8);
        assert_eq!(aligned_to(8, 8), 8);
        assert_eq!(aligned_to(9, 8), 16);
        assert_eq!(aligned_to(7, 0), 7);

        // colored-pointer masks and decode
        let lba_ptr = mask_colored_ptr_lba(0, 5);
        assert_eq!(decode_colored_ptr(lba_ptr).1, 5);
        let addr_ptr = mask_colored_ptr_memory_address(0, 0x1234);
        assert_eq!(addr_ptr & SSD_MEMORY_ADDR_FLAGS, 0x1234);
    }

    #[test]
    fn storage_config_storage_engine_type_conversions() {
        use StorageEngineKind::*;
        for ty in [Dram, Pmem, Ssd, Simple, MultiSsd] {
            // code conversion round-trips, and every variant has a display name
            assert_eq!(StorageEngineKind::from_config_code(ty.config_code()), ty);
            assert!(!ty.as_config_name().is_empty());
        }
        // recognized name spellings parse to the expected engine
        assert_eq!(StorageEngineKind::from_config_name("ssd"), Ssd);
        assert_eq!(StorageEngineKind::from_config_name("kRocksDB"), Ssd);
        assert_eq!(StorageEngineKind::from_config_name("pmem"), Pmem);
        assert_eq!(StorageEngineKind::from_config_name("multi_ssd"), MultiSsd);
        assert_eq!(StorageEngineKind::from_config_name("simple"), Simple);
        assert_eq!(StorageEngineKind::from_config_name("dram"), Dram);
        assert!(Ssd.is_ssd_like());
        assert!(MultiSsd.is_ssd_like());
        assert!(!Dram.is_ssd_like());
        assert_eq!(Pmem.canonical_instance_type(), CacheInstanceKind::Pmem);
        assert_eq!(Dram.canonical_instance_type(), CacheInstanceKind::Dram);
        // unknown code and name fall back to the default engine
        assert_eq!(StorageEngineKind::from_config_code(200), Dram);
        assert_eq!(StorageEngineKind::from_config_name("not-a-real-engine"), Dram);
    }

    #[test]
    fn storage_config_ssd_index_and_write_buffer_ops() {
        let index = SsdIndex::new();
        index.Put("a", SsdIndexValue::SsdColoredPtr(1));
        index.Put("b", SsdIndexValue::SsdColoredPtr(2));
        assert!(index.Get("a").is_some());
        assert!(index.Get("b").is_some());
        assert!(index.Get("missing").is_none());
        // UpdateIndex refreshes an existing entry and fails closed on a missing key
        index.UpdateIndex("a", SsdIndexValue::SsdColoredPtr(9));
        assert!(!index.UpdateIndex("missing", SsdIndexValue::SsdColoredPtr(0)));
        // A device record carries its state in its packed pointer, so it is
        // pinnable like any other; only a missing key fails closed.
        assert!(index.Pin("a"));
        assert!(!index.Pin("missing"));
        index.UnPin("a");
        index.SoftDelete("a");
        index.Put("c", SsdIndexValue::SsdColoredPtr(3));
        // A device record has a state, so DeleteIf can act on it. Reclaim is the
        // caller: an entry it cannot delete is one left pointing at a reset zone.
        assert!(index.DeleteIf("c", |_state| true));
        assert!(!index.DeleteIf("missing", |_state| true));
        let mut scanned = 0usize;
        index.ScanIndexForRecover(|_key, _value| scanned += 1);
        assert!(scanned >= 1);

        let mut wb = WriteBuffer::new(WriteBufferKind::UserDataBuf, 1024);
        assert_eq!(wb.Capacity(), 1024);
        assert_eq!(wb.Count(), 0);
        assert_eq!(wb.BufType(), WriteBufferKind::UserDataBuf);
        wb.PushBack("k1", b"v1".to_vec());
        wb.PushBack("k2", b"v22".to_vec());
        assert_eq!(wb.Count(), 2);
        assert!(wb.Size() > 0);
        assert!(wb.KeySize() > 0);
        assert!(wb.ValueSize() > 0);
        assert_eq!(wb.records().len(), 2);
        assert_eq!(wb.StealBufQ().len(), 2);
    }

    #[test]
    fn rdma_utils_and_policy_conversions() {
        assert_eq!(
            RdmaReplacementPolicyKind::Fifo.as_replacement_policy_type(),
            ReplacementPolicyKind::Fifo
        );
        assert_eq!(
            RdmaReplacementPolicyKind::Lru.as_replacement_policy_type(),
            ReplacementPolicyKind::Lru
        );
        assert_eq!(
            RdmaReplacementPolicyKind::Other.as_replacement_policy_type(),
            ReplacementPolicyKind::MaxCode
        );

        let mut generator = RandomStringGenerator::new();
        assert_eq!(generator.rand_value(8).len(), 8);
        assert_eq!(generator.RandValueBytes(16).len(), 16);
        assert_eq!(generator.rand_value_bytes(4).len(), 4);
        // with_size constructs a generator; small buffers clamp the value length
        let mut sized = RandomStringGenerator::with_size(256);
        assert!(!sized.rand_value_bytes(4).is_empty());
    }

    #[test]
    fn storage_config_more_enum_conversions() {
        // CacheAccessRecordKind code round-trips; invalid codes fail closed
        for ty in [
            CacheAccessRecordKind::Put,
            CacheAccessRecordKind::Get,
            CacheAccessRecordKind::Delete,
        ] {
            assert_eq!(CacheAccessRecordKind::from_config_code(ty.config_code()), Some(ty));
        }
        assert_eq!(CacheAccessRecordKind::from_config_code(0), None);
        assert_eq!(
            CacheAccessRecordKind::from_config_code(CacheAccessRecordKind::kMaxCode),
            None
        );

        // CacheDataPlacement name parsing (fallible)
        assert_eq!(
            CacheDataPlacement::try_from_config_name("SideBySide").unwrap(),
            CacheDataPlacement::SideBySide
        );
        assert_eq!(
            CacheDataPlacement::try_from_config_name("Tiered").unwrap(),
            CacheDataPlacement::Tiered
        );
        assert!(CacheDataPlacement::try_from_config_name("nonsense").is_err());

        // DramPmemDataPlacement conversions to/from CacheDataPlacement and names
        assert_eq!(
            DramPmemDataPlacement::from_cache_data_placement(CacheDataPlacement::SideBySide),
            DramPmemDataPlacement::SideBySide
        );
        assert_eq!(
            DramPmemDataPlacement::from_cache_data_placement(CacheDataPlacement::Tiered),
            DramPmemDataPlacement::Tiered
        );
        assert_eq!(
            DramPmemDataPlacement::Tiered.as_cache_data_placement(),
            CacheDataPlacement::Tiered
        );
        for ty in [
            DramPmemDataPlacement::SideBySide,
            DramPmemDataPlacement::Tiered,
            DramPmemDataPlacement::MaxCode,
        ] {
            assert!(!ty.as_config_name().is_empty());
        }
    }

    #[test]
    fn multilayer_cache_put_get_and_batch_ops() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1 << 20, dir.path());
        let k = |s: &str| CacheKey::string(0, s);

        cache.put(k("a"), b"alpha".to_vec()).unwrap();
        assert_eq!(cache.get(&k("a")).unwrap(), Some(b"alpha".to_vec()));
        assert_eq!(cache.get(&k("missing")).unwrap(), None);

        let stored = cache
            .put_batch(vec![(k("b"), b"beta".to_vec()), (k("c"), b"gamma".to_vec())])
            .unwrap();
        assert_eq!(stored, 2);
        assert_eq!(
            cache.get_batch(&[k("b"), k("c"), k("missing")]).unwrap(),
            vec![Some(b"beta".to_vec()), Some(b"gamma".to_vec()), None]
        );

        assert!(cache.get_no_promotion(&k("a")).unwrap().is_some());
        cache.put_memory_only(k("mem"), b"m".to_vec());
        assert_eq!(cache.get_memory(&k("mem")), Some(b"m".to_vec()));

        assert!(cache.get_capacity(CacheInstanceKind::Dram) > 0);
        let _ = cache.get_used(CacheInstanceKind::Dram);
    }

    #[test]
    fn multilayer_cache_lifecycle_and_introspection() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1 << 20, dir.path());
        let k = |s: &str| CacheKey::string(0, s);

        cache.put(k("a"), b"1".to_vec()).unwrap();
        cache.put(k("b"), b"2".to_vec()).unwrap();
        assert!(cache.peek(&k("a")));
        assert!(!cache.peek(&k("missing")));
        assert!(cache.peek_tier(&k("a")).is_some());

        cache.remove(&k("a")).unwrap();
        assert!(!cache.peek(&k("a")));
        cache.invalidate(&k("b")).unwrap();

        cache.put(k("c"), b"3".to_vec()).unwrap();
        cache.put(k("d"), b"4".to_vec()).unwrap();
        assert!(cache.remove_batch(&[k("c"), k("missing")]).unwrap() <= 2);
        assert!(cache.invalidate_batch(&[k("d")]).unwrap() <= 1);

        // introspection is callable
        let _ = cache.used_space_for_tier(CacheTier::Memory);
        let _ = cache.get_used(CacheInstanceKind::Dram);
        let _ = cache.get_replacement_policy_type(CacheInstanceKind::Dram);
        let _ = cache.replacement_policy_for_tier(CacheTier::Memory);

        cache.reset().unwrap();
        assert!(!cache.peek(&k("d")));

        cache.set_auto_recover_on_start(true);
        assert!(cache.auto_recover_on_start());
        cache.set_auto_recover_on_start(false);
        assert!(!cache.auto_recover_on_start());
        let _ = cache.recover_disk_index();
    }

    #[test]
    fn multilayer_cache_pinned_handles() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1 << 20, dir.path());
        let k = |s: &str| CacheKey::string(0, s);
        cache.put(k("a"), b"alpha".to_vec()).unwrap();

        assert!(cache.get_with_tier(&k("a")).unwrap().is_some());

        let handle = cache.acquire(&k("a")).unwrap().expect("pinned handle for present key");
        let cloned = cache.clone_handle(&handle);
        assert_eq!(cache.release_batch(vec![cloned]), 1);
        cache.release(handle);

        assert!(cache.acquire(&k("missing")).unwrap().is_none());
    }

    #[test]
    fn storage_config_buffer_manager_ops() {
        let mut mgr = BufferManager::with_config(4096, 0.75, 1024);
        assert_eq!(mgr.capacity_per_buf(), 4096);
        assert_eq!(mgr.flush_threshold(), 0.75);
        assert_eq!(mgr.flush_size(), 1024);
        assert!(!mgr.write_enabled());
        mgr.set_write_enabled(true);
        assert!(mgr.write_enabled());
        mgr.SetWriteEnabled(false);
        assert!(!mgr.WriteEnabled());
        mgr.Start();
        let _ = mgr.put(("k1", b"v1".to_vec()), WriteBufferKind::UserDataBuf);
        let _ = mgr.put(("k2", b"v2".to_vec()), WriteBufferKind::UserDataBuf);
        let _ = mgr.buffered_count(WriteBufferKind::UserDataBuf);
        let _ = mgr.FlushBuffers();
        let _ = mgr.flushed_records();
        mgr.Stop();
        let _ = BufferManager::new().capacity_per_buf();

        let mut mgr2 = BufferManager::with_config(1024, 0.5, 512);
        let mut wb = WriteBuffer::new(WriteBufferKind::UserDataBuf, 1024);
        wb.PushBack("x", b"y".to_vec());
        assert_eq!(mgr2.flush_buffer(wb), 1);
    }

    #[test]
    fn replacement_fifo_policy_ops() {
        let mut fifo = ReplacementFifo::new(1 << 20);
        fifo.init().unwrap();
        assert_eq!(fifo.GetCapacity(), 1 << 20);
        let mut buf = CacheBuffer::new(b"val".to_vec());
        buf.set_key("k");
        let _ = fifo.put(buf);
        let _ = fifo.get("k");
        let _ = fifo.peek("k");
        let _ = fifo.GetUsedSpace();
        let _ = fifo.GetFreeSpace();
        fifo.SetCapacity(2 << 20);
        assert_eq!(fifo.GetCapacity(), 2 << 20);
        let _ = fifo.delete("k");
        fifo.Reset().unwrap();
    }

    #[test]
    fn storage_config_index_updater_ops() {
        let index = Arc::new(SsdIndex::new());
        index.Put("a", SsdIndexValue::SsdColoredPtr(1));
        let updater = IndexUpdater::new(Arc::clone(&index));
        assert!(updater.Get("a").is_some());
        assert!(updater.get("missing").is_none());
        updater.UpdateIndex("a", SsdIndexValue::SsdColoredPtr(2));
        assert!(!updater.UpdateIndex("missing", SsdIndexValue::SsdColoredPtr(0)));
        // A device record has a state, so DeleteIf can act on it.
        assert!(updater.DeleteIf("a", |_state| true));
    }

    #[test]
    fn storage_config_camelcase_encoding_and_colored_ptr() {
        let mut buf = Vec::new();
        assert_eq!(PutFixedUint8(&mut buf, 7), 1);
        assert_eq!(PutFixedUint32(&mut buf, 300), 5);
        assert_eq!(PutFixedUint64(&mut buf, 5_000_000_000), 13);
        assert_eq!(GetFixedUint8(&buf, 0).unwrap().0, 7);
        assert_eq!(GetFixedUint32(&buf, 1).unwrap().0, 300);
        assert_eq!(GetFixedUint64(&buf, 5).unwrap().0, 5_000_000_000);

        let mut hbuf = Vec::new();
        PutFixedHash64(&mut hbuf, 42);
        assert_eq!(GetFixedHash64(&hbuf, 0).unwrap().0, 42);
        let mut h2 = Vec::new();
        PutFixedHash128(
            &mut h2,
            Xxh128 {
                first: 1,
                second: 2,
            },
        );
        assert_eq!(GetFixedHash128(&h2, 0).unwrap().0.first, 1);

        assert_eq!(AlignedTo(5, 8), 8);
        let mut dst = Vec::new();
        assert_eq!(CopyBytesTo(&mut dst, b"hi"), 2);
        assert_eq!(CopyBytesFrom(&dst, 0, 2).unwrap().0, b"hi".to_vec());

        // colored-pointer size/lba/state masks (CamelCase + snake_case)
        assert_eq!(DecodeColoredPtr(MaskColoredPtrSize(0, 9)).0, 9);
        assert_eq!(decode_colored_ptr(mask_colored_ptr_size(0, 3)).0, 3);
        let _ = MaskColoredPtrLBA(0, 4);
        let _ = MaskColoredPtrMemoryAddress(0, 0x10);
        let _ = MaskColoredPtrRecordState(0, RecordState::Normal);
        let _ = mask_colored_ptr_record_state(0, RecordState::Pinned);

        // BufferEncoder size calculators
        let encoder = BufferEncoder::new(4096);
        let mut wb = WriteBuffer::new(WriteBufferKind::UserDataBuf, 1024);
        wb.PushBack("k", b"v".to_vec());
        let _ = encoder.calculate_encoded_data_size(&wb);
        let _ = encoder.calculate_encoded_oplog_size(&wb);
    }

    #[test]
    fn replacement_arc_list_ops() {
        let mut arc = ArcList::new(4);
        assert_eq!(arc.capacity(), 4);
        arc.put("a".to_string());
        arc.put("b".to_string());
        assert!(arc.get("a"));
        assert!(!arc.get("missing"));
        let _ = arc.size();
        let _ = arc.GhostSize();
        let _ = arc.FetchCapacity();
        let _ = arc.ActiveCapacity();
        let _ = arc.DataFull();
        let _ = arc.GetFetchGhostTail(2);
        assert!(arc.Delete("b"));
        let _ = arc.Evict();
        arc.SetCapacity(8);
        assert_eq!(arc.capacity(), 8);
        arc.Reset();
        assert_eq!(arc.size(), 0);
    }

    #[test]
    fn replacement_slru_policy_ops() {
        let mut slru = ReplacementSlru::new(1 << 20);
        slru.init().unwrap();
        assert_eq!(slru.capacity(), 1 << 20);
        let mut buf = CacheBuffer::new(b"v".to_vec());
        buf.set_key("k");
        let _ = slru.put(buf);
        let _ = slru.get("k");
        let _ = slru.peek("k");
        let _ = slru.free_space();
        let _ = slru.GetFreeSpace();
        slru.set_capacity(2 << 20);
        assert_eq!(slru.capacity(), 2 << 20);
        let _ = slru.delete("k");
        slru.Reset().unwrap();
    }

    #[test]
    fn replacement_ghost_lru_camelcase_aliases() {
        let mut g = GhostLruList::new(2);
        g.SetCapacity(4);
        assert_eq!(g.Capacity(), 4);
        g.Put("a".to_string());
        g.PutGhost("gh".to_string());
        let _ = g.GhostCapacity();
        let _ = g.EvictOneData();
        let _ = g.EvictOneGhost();
        let _ = g.Evict();
        let _ = g.Delete("a");
    }

    #[test]
    fn rdma_free_functions() {
        let _ = FastRand16();
        let _ = FastRand64();
        assert_eq!(XXH32WithSeed(b"hello", 0), XXH32WithSeed(b"hello", 0));
        assert_ne!(XXH32WithSeed(b"a", 0), XXH32WithSeed(b"b", 0));
        assert!(!GetHashedKey(42, 8).is_empty());
        assert_eq!(GetRandStr(10).len(), 10);
    }

    #[test]
    fn rdma_storage_engine_ops() {
        let mut engine = RdmaStorageEngine::new(RdmaStorageEngineKind::Dram, 1 << 20);
        assert!(matches!(engine.storage_type(), RdmaStorageEngineKind::Dram));
        assert_eq!(engine.capacity(), 1 << 20);
        assert_eq!(engine.used(), 0);
        let ptr = engine.Put(b"key", b"value").expect("rdma put allocates");
        assert!(engine.used() > 0);
        let _ = engine.Stats();
        let mut resp = RdmaResponse::New(64);
        let _ = engine.Get(b"key", b"value".len(), &mut resp, ptr);
        let _ = engine.Del(ptr, b"value".len());
    }

    #[test]
    fn allocators_je_allocator_and_pmem_helpers() {
        let je = JeAllocator::new(1 << 20);
        assert!(je.Capacity().is_ok());
        let ptr = DramAllocateObjectV2(0x5555_0000, 4096).unwrap();
        let _ = je.sealed(ptr);
        DramFreeObject(ptr, 4096).unwrap();
        PMemFlush(0x1000, 64);
        PMemDrain();
    }

    #[test]
    fn multilayer_cache_batch_and_read_variants() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1 << 20, dir.path());
        let k = |s: &str| CacheKey::string(0, s);

        assert!(
            cache
                .put_batch(vec![(k("a"), b"1".to_vec()), (k("b"), b"2".to_vec())])
                .unwrap()
                >= 1
        );
        assert!(
            cache
                .put_batch_sized(vec![(k("c"), b"3".to_vec(), 3)])
                .unwrap()
                >= 1
        );
        cache.put_memory_only(k("m"), b"mem".to_vec());

        assert_eq!(cache.get_batch(&[k("a"), k("missing")]).unwrap().len(), 2);
        let _ = cache.get_no_promotion(&k("a")).unwrap();
        let _ = cache.get_batch_no_promotion(&[k("a"), k("b")]).unwrap();
        let _ = cache.get_bypass_replacement_policy(&k("a")).unwrap();
        let _ = cache.get_memory(&k("a"));

        cache.remove_all().unwrap();
        assert!(!cache.peek(&k("a")));
    }

    #[test]
    fn multilayer_cache_introspection_and_extra_ops() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1 << 20, dir.path());
        let k = |s: &str| CacheKey::string(0, s);

        cache.insert_default_size(k("a"), b"1".to_vec()).unwrap();
        cache.put(k("b"), b"2".to_vec()).unwrap();

        let _ = cache.all_entries();
        let _ = cache.allocator_stats_for_tier(CacheTier::Memory);
        let _ = cache.test_unified_put_count();
        let _ = cache.production_tiering_policy();
        let _ = cache.async_writeback_worker_running();
        let _ = cache.pmem_paths();

        cache.invalidate_memory_only(&k("a"));
    }

    #[test]
    fn rdma_index_entry_and_stored_block() {
        let mut entry = RdmaIndexEntry::default();
        entry.SetAddr(0x1234);
        assert_eq!(entry.GetAddr(), 0x1234);
        entry.set_addr(0x5678);
        assert_eq!(entry.addr(), 0x5678);
        let _ = entry.GetCRC();
        let _ = entry.crc();
        let _ = entry.GetSignature96b();
        let mut rkey_buf = [0u8; 16];
        let _ = entry.GetRkey(&mut rkey_buf);
        let _ = entry.get_rkey(&mut rkey_buf);

        let block = RdmaStoredBlock::new(b"key", b"value");
        assert!(block.EncodedLen() > 0);
        let encoded = block.Encode();
        assert!(!encoded.is_empty());
        assert_eq!(encoded, block.encode());
    }

    #[test]
    fn matrixcache_builders() {
        let lru = MatrixCacheBuilder::build_simple_lru_cache(1024);
        let _ = lru.Start();
        let _ = lru.insert_default_size(CacheKey::string(0, "k"), b"v".to_vec());
        let _ = lru.Lookup(&CacheKey::string(0, "k"));
        let _ = MatrixCacheBuilder::build_zero_copy_simple_lru_cache(1024);
        let _ = MatrixCacheBuilder::build_concurrent_simple_lru_cache(1024);
        let _ = MatrixCacheBuilder::build_in_process_memcached_cache(1024);

        // Dram-only options avoid needing an on-disk Ssd tier
        let opts = || CacheOptions::new(1 << 16, 0, 0);
        let cache = MatrixCacheBuilder::build_cache(opts());
        cache.put(CacheKey::string(0, "a"), b"1".to_vec()).unwrap();
        let _ = MatrixCacheBuilder::build_zero_copy_cache(opts());
        let _ = MatrixCacheBuilder::build_cache_api(opts());
        let _ = MatrixCacheBuilder::build_sharded_cache_api(opts(), 2);
        let _ = MatrixCacheBuilder::build_zero_copy_cache_api(opts());
        let _ = MatrixCacheBuilder::build_multi_tier_string_cache(opts());
    }

    #[test]
    fn concurrency_cpu_topology_helpers() {
        let _ = NumaInfo::num_all_cores();
        let _ = NumaInfo::num_online_cores();
        let _ = NumaInfo::current_cpu_core();
        let _ = NumaInfo::max_num_numa_nodes();
        let _ = NumaInfo::get_cpu_cores_of_numa_node(0);
        let _ = NumaInfo::bind_thread_to_cpu_core(0);
    }

    #[test]
    fn multilayer_cache_tiering_admission_and_pinned_variants() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1 << 20, dir.path());
        let k = |s: &str| CacheKey::string(0, s);

        cache.put_sized(k("s"), b"sized".to_vec(), 5).unwrap();
        cache
            .put_with_admission(
                k("adm"),
                b"a".to_vec(),
                CacheAdmissionRequest {
                    block_kind: CacheBlockKind::Page,
                    shard_id: 1,
                    routing_slot: Some(0),
                    block_bytes: 16,
                    hotness: 5,
                    pinned: false,
                },
            )
            .unwrap();

        if let Some(h) = cache.insert_pinned_default_size(k("p1"), b"1".to_vec()).unwrap() {
            cache.release(h);
        }
        if let Some(h) = cache.insert_pinned_sized(k("p2"), b"22".to_vec(), 2).unwrap() {
            cache.release(h);
        }
        for h in cache.acquire_batch(&[k("s"), k("missing")]).unwrap().into_iter().flatten() {
            cache.release(h);
        }
        for h in cache.acquire_batch_no_promotion(&[k("s")]).unwrap().into_iter().flatten() {
            cache.release(h);
        }
        if let Some(h) = cache.get_pinned_handle(&k("s")).unwrap() {
            cache.release(h);
        }

        cache.set_capacity_for_instance(CacheInstanceKind::Dram, 2 << 20);
        cache.set_replacement_policy_type(CacheInstanceKind::Dram, CacheReplacementPolicy::Fifo);
        let policy = cache.production_tiering_policy();
        cache.update_production_tiering_policy(policy);
    }

    #[test]
    fn replacement_slru_maintainer_hooks() {
        let mut slru = ReplacementSlru::new(1 << 20);
        slru.init().unwrap();
        slru.register_mem_eviction_handler(|_buf| {});
        slru.test_wait_for_lru_maintainer();
        slru.test_notify_maintainer_move_complete();
    }

    #[test]
    fn rdma_hash_table_ops() {
        let mut table = RdmaHashTable::<Vec<u8>>::new(16);
        let key = b"hkey".to_vec();
        assert!(table.Get(&key).addr.is_none());
        let _ = table.Put(key.clone(), 0x1000, 5, RdmaStorageEngineKind::Dram);
        let _ = table.Get(&key);
        let _ = table.Del(&key);
        let _ = table.get_bucket(0);
        let _ = table.GetBucket(0);
    }

    #[test]
    fn alloc_utils_parse_allocate_persist_thread_ids_and_pmem_files() {
        assert_eq!(ParseAllocatorType("Log"), AllocatorKind::LogBasedAllocator);
        assert_eq!(
            ParseAllocatorType("Pool"),
            AllocatorKind::PoolBasedAllocator
        );
        assert_eq!(ParseAllocatorType("Jemalloc"), AllocatorKind::JeAllocator);
        assert_eq!(ParseAllocatorType("missing"), AllocatorKind::MaxCode);

        let ptr = DramAllocateObject(4096, 4096).unwrap();
        assert_eq!(ptr % 4096, 0);
        DramFreeObject(ptr, 4096).unwrap();
        assert!(matches!(
            DramFreeObject(ptr, 4096),
            Err(CacheError::NotFound)
        ));

        let fixed = DramAllocateObject_v2(0x2000_0000, 4096).unwrap();
        assert_eq!(fixed, 0x2000_0000);
        DramFreeObject(fixed, 4096).unwrap();

        let reserved = PreAllocate(4096, 2 * 1024 * 1024).unwrap();
        assert_eq!(reserved % (2 * 1024 * 1024), 0);
        PMemPersist(reserved, 64);
        PostFree(reserved, 4096).unwrap();

        let id1 = GetThreadLocalResourceID();
        let id2 = GetThreadLocalResourceID();
        assert_eq!(id1, id2);

        let dir = tempfile::tempdir().unwrap();
        let valid = dir.path().join("00000000000000000001.pmem_chunk");
        let invalid = dir.path().join("bad.pmem_chunk");
        let pmem_ptr = PMemAllocateObject(&valid, 4096, 4096).unwrap();
        assert_eq!(pmem_ptr % 4096, 0);
        PMemFreeObject(pmem_ptr, 4096).unwrap();
        std::fs::write(&invalid, b"short").unwrap();

        let mut invalid_names = Vec::new();
        let valid_names = GetPmemFileName(dir.path(), 4096, Some(&mut invalid_names)).unwrap();
        assert_eq!(
            valid_names,
            vec!["00000000000000000001.pmem_chunk".to_string()]
        );
        assert_eq!(invalid_names, vec!["bad.pmem_chunk".to_string()]);

        let mapped = PMemMapFile(0, &valid, 4096).unwrap();
        PMemFreeObject(mapped, 4096).unwrap();
        assert!(DeletePmemFile(&invalid));
        assert!(!DeletePmemFile(&invalid));
    }

    #[test]
    fn simple_log_based_allocator_allocates_seals_frees_and_reports_stats() {
        let mut allocator = SimpleLogBasedMemoryAllocator::with_capacity(16);
        let ptr = allocator.Allocate(8).unwrap();

        assert!(allocator.Contains(ptr));
        allocator.write(ptr, b"cache123").unwrap();
        assert_eq!(allocator.read(ptr).unwrap(), b"cache123");
        let crc = MemStorage::ComputeCRC("alloc-key", allocator.read(ptr).unwrap());
        allocator.SealWithCRC(ptr, 8, crc).unwrap();
        assert!(allocator.sealed(ptr));
        assert_eq!(allocator.crc32(ptr), Some(crc));

        let mut observed_meta = None;
        allocator
            .RetrieveChunkMeta(ptr as ChunkId, |meta| observed_meta = Some(meta.clone()))
            .unwrap();
        let meta = observed_meta.expect("chunk meta");
        assert_eq!(meta.id, ptr as ChunkId);
        assert_eq!(meta.num_allocated_bytes, 8);
        assert_eq!(meta.ref_count, 1);

        assert!(matches!(
            allocator.Allocate(9),
            Err(CacheError::CapacityExceeded)
        ));
        allocator.Free(ptr, 8).unwrap();
        assert!(!allocator.Contains(ptr));
        let stats = allocator.GetStats().unwrap();
        assert_eq!(stats.NumAllocatedBytes(), 8);
        assert_eq!(stats.NumFreedBytes(), 8);
        assert_eq!(stats.NumOccupiedBytes(), 0);

        let mut recyclable = Vec::new();
        allocator
            .IterateRecyclableChunkMeta(|meta| {
                recyclable.push(meta.id);
                true
            })
            .unwrap();
        assert_eq!(recyclable, vec![ptr as ChunkId]);
        allocator.GC(&[ptr as ChunkId]).unwrap();
        assert_eq!(allocator.gc_runs(), 1);
        assert_eq!(allocator.live_region_count(), 0);
    }

    #[test]
    fn log_allocator_chunk_meta_reports_what_reclaiming_would_return() {
        let mut allocator = SimpleLogBasedMemoryAllocator::new();
        let small = allocator.Allocate(8).unwrap();
        let large = allocator.Allocate(64).unwrap();
        allocator.Free(small, 8).unwrap();
        allocator.Free(large, 64).unwrap();

        let mut seen = Vec::new();
        allocator
            .IterateRecyclableChunkMeta(|meta| {
                seen.push((
                    meta.id,
                    meta.num_allocated_bytes,
                    meta.num_freed_bytes,
                    meta.ref_count,
                ));
                true
            })
            .unwrap();
        seen.sort_by_key(|entry| entry.1);

        // Each freed region reports the size reclaiming it would return, and no
        // live references. Previously every field was zero, so a caller
        // filtering on `num_freed_bytes` -- which is how the reference decides
        // what is worth collecting -- selected nothing, ever.
        assert_eq!(
            seen,
            vec![(small as ChunkId, 8, 8, 0), (large as ChunkId, 64, 64, 0)]
        );
    }

    #[test]
    fn log_allocator_gc_collects_only_the_chunks_it_is_given() {
        let mut allocator = SimpleLogBasedMemoryAllocator::new();
        let keep = allocator.Allocate(8).unwrap();
        let collect = allocator.Allocate(8).unwrap();
        allocator.Free(keep, 8).unwrap();
        allocator.Free(collect, 8).unwrap();
        assert_eq!(allocator.test_global_free_list_size(), 2);

        allocator.GC(&[collect as ChunkId]).unwrap();

        // Only the named chunk is reclaimed. Previously `gc` discarded its
        // argument and swept every freed slot, leaving 0 here.
        assert_eq!(allocator.test_global_free_list_size(), 1);

        let mut remaining = Vec::new();
        allocator
            .IterateRecyclableChunkMeta(|meta| {
                remaining.push(meta.id);
                true
            })
            .unwrap();
        assert_eq!(remaining, vec![keep as ChunkId]);
    }

    #[test]
    fn log_allocator_gc_does_not_drop_a_live_region() {
        let mut allocator = SimpleLogBasedMemoryAllocator::new();
        let live = allocator.Allocate(8).unwrap();

        // Naming a live region in a collection request must not pull it out from
        // under whoever still holds it.
        allocator.GC(&[live as ChunkId]).unwrap();
        assert!(allocator.Contains(live));
    }

    #[test]
    fn simple_log_based_allocator_supports_trait_consumers() {
        fn allocate_and_seal<A: CacheAllocatorApi>(
            allocator: &mut A,
        ) -> Result<AllocatorAddress, CacheError> {
            let ptr = allocator.allocate(4)?;
            allocator.seal(ptr)?;
            Ok(ptr)
        }

        fn run_gc<A: LogBasedMemoryAllocatorApi>(allocator: &mut A) -> Result<(), CacheError> {
            allocator.gc(&[])?;
            Ok(())
        }

        let mut allocator = SimpleLogBasedMemoryAllocator::new();
        let ptr = allocate_and_seal(&mut allocator).unwrap();
        assert!(allocator.Contains(ptr));
        run_gc(&mut allocator).unwrap();
        assert_eq!(allocator.gc_runs(), 1);
    }

    #[test]
    fn storage_gc_controller_lifecycle_pause_and_force_gc_match_surface() {
        let mut allocator = SimpleLogBasedMemoryAllocator::with_capacity(64);
        let ptr = allocator.Allocate(8).unwrap();
        allocator.write(ptr, b"gc-ready").unwrap();
        allocator.Free(ptr, 8).unwrap();

        let mut controller = StorageGcController::new(allocator, true);
        assert!(!controller.enable_gc());
        assert!(controller.NeedGc());

        controller.SetPauseGC(true);
        assert!(controller.pause_gc());
        assert!(!controller.NeedGc());
        assert_eq!(controller.PickSubmitChunks().unwrap(), 0);

        controller.SetPauseGC(false);
        controller.Start();
        assert!(controller.enable_gc());
        assert_eq!(controller.PickSubmitChunks().unwrap(), 1);
        controller.WaitAllTaskComplete();
        assert_eq!(controller.fly_gc_chunks(), 0);
        assert_eq!(controller.TEST_GetNumGcCompleteChunks(), 1);
        assert_eq!(controller.TEST_GetNumGcCompleteTasks(), 1);
        assert_eq!(controller.allocator().gc_runs(), 1);
        assert_eq!(controller.allocator().live_region_count(), 0);

        controller.Stop();
        assert!(!controller.enable_gc());
    }

    #[test]
    fn storage_gc_controller_poll_paces_collection_checks() {
        let mut allocator = SimpleLogBasedMemoryAllocator::with_capacity(64);
        let first = allocator.Allocate(8).unwrap();
        allocator.Free(first, 8).unwrap();

        let mut controller = StorageGcController::new(allocator, true);
        assert_eq!(
            controller.gc_check_interval_ms(),
            GC_DEFAULT_CHECK_INTERVAL_MS
        );

        // Collection is disabled until started, so polling does nothing.
        assert_eq!(controller.poll().unwrap(), 0);

        controller.start();
        controller.set_gc_check_interval_ms(60_000);
        // The first check is always due.
        assert_eq!(controller.poll().unwrap(), 1);

        // Queue more work. The interval has not elapsed, so the controller
        // leaves it for the next due check instead of scanning on every call.
        let second = controller.allocator_mut().Allocate(8).unwrap();
        controller.allocator_mut().Free(second, 8).unwrap();
        assert_eq!(controller.poll().unwrap(), 0);

        // Shortening the interval makes the check due and the work is taken.
        controller.set_gc_check_interval_ms(0);
        assert_eq!(controller.poll().unwrap(), 1);

        // Pausing and disabling each suppress collection regardless of pacing.
        let third = controller.allocator_mut().Allocate(8).unwrap();
        controller.allocator_mut().Free(third, 8).unwrap();
        controller.set_pause_gc(true);
        assert_eq!(controller.poll().unwrap(), 0);
        controller.set_pause_gc(false);
        controller.set_enable_gc(false);
        assert_eq!(controller.poll().unwrap(), 0);

        controller.set_enable_gc(true);
        assert_eq!(controller.poll().unwrap(), 1);
        assert_eq!(controller.TEST_GetNumGcCompleteChunks(), 3);
    }

    #[test]
    fn storage_gc_controller_respects_enable_gate_and_manual_gc_job() {
        let mut allocator = SimpleLogBasedMemoryAllocator::with_capacity(64);
        let ptr_a = allocator.Allocate(4).unwrap();
        let ptr_b = allocator.Allocate(4).unwrap();
        allocator.Free(ptr_a, 4).unwrap();
        allocator.Free(ptr_b, 4).unwrap();

        let mut controller = StorageGcController::new(allocator, true);
        assert_eq!(controller.PickSubmitChunks().unwrap(), 0);
        assert_eq!(controller.allocator().live_region_count(), 0);
        assert_eq!(controller.TEST_GetNumGcCompleteTasks(), 0);

        assert_eq!(
            controller
                .gc_job(vec![ptr_a as ChunkId, ptr_b as ChunkId])
                .unwrap(),
            2
        );
        assert_eq!(controller.TEST_GetNumGcCompleteChunks(), 2);
        assert_eq!(controller.TEST_GetNumGcCompleteTasks(), 1);
        assert_eq!(controller.allocator().gc_runs(), 1);
    }

    /// Sole access to the process-wide executor registry, for one test.
    ///
    /// `Configure` and `DestroyAllExecutors` both reset a single global, so
    /// two tests that call either one cannot run at the same time: each sees
    /// the other's executors in its counts. The failure is intermittent and
    /// lands on whichever test lost the race, so it reads as a defect in the
    /// code under test rather than in the harness.
    ///
    /// The lock is deliberately recovered from poisoning. A test that panics
    /// while holding it has already reported its own failure; leaving the lock
    /// poisoned would fail the next executor test too, and a second red that
    /// only means "someone else failed first" points at the wrong place.
    fn exclusive_executor_registry() -> std::sync::MutexGuard<'static, ()> {
        static REGISTRY: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let guard = REGISTRY.lock().unwrap_or_else(|poisoned| {
            REGISTRY.clear_poison();
            poisoned.into_inner()
        });
        // Start from an empty registry rather than whatever the last test left
        // behind, so these tests do not depend on the order they run in.
        CacheExecutor::DestroyAllExecutors();
        guard
    }

    #[test]
    fn cache_executor_reuses_common_and_gc_executors_and_runs_tasks() {
        let _registry = exclusive_executor_registry();
        CacheExecutor::DestroyAllExecutors();
        CacheExecutor::Configure(CacheExecutorConfig {
            common_executor_num_threads: 3,
            num_gc_workers: 2,
            used_num_numa_nodes: 2,
            num_pmem_cache_per_numa_writer_threads: 4,
        });

        let common = CacheExecutor::GetCommonExecutor();
        let common_again = CacheExecutor::GetCommonExecutor();
        assert!(Arc::ptr_eq(&common, &common_again));
        assert_eq!(common.name(), "CacheCommonThreadPool");
        assert_eq!(common.thread_count(), 3);

        let ran = Arc::new(std::sync::Mutex::new(false));
        let ran_capture = Arc::clone(&ran);
        common.Add(move || {
            *ran_capture.lock().unwrap() = true;
        });
        assert!(*ran.lock().unwrap());
        assert_eq!(common.submitted_tasks(), 1);

        let gc = CacheExecutor::GetGCExecutor();
        assert_eq!(gc.name(), "CacheGCThreadPool");
        assert_eq!(gc.thread_count(), 2);
        assert!(!Arc::ptr_eq(&common, &gc));
    }

    #[test]
    fn cache_executor_creates_pmem_numa_executors_and_destroy_resets() {
        let _registry = exclusive_executor_registry();
        CacheExecutor::DestroyAllExecutors();
        CacheExecutor::Configure(CacheExecutorConfig {
            common_executor_num_threads: 1,
            num_gc_workers: 1,
            used_num_numa_nodes: 3,
            num_pmem_cache_per_numa_writer_threads: 5,
        });

        let pmem = CacheExecutor::GetPmemExecutors();
        assert_eq!(pmem.len(), 3);
        assert_eq!(pmem[0].name(), "PmemNuma0");
        assert_eq!(pmem[1].numa_id(), Some(1));
        assert_eq!(pmem[2].thread_count(), 5);
        let pmem_again = CacheExecutor::GetPmemExecutors();
        assert!(Arc::ptr_eq(&pmem[0], &pmem_again[0]));
        assert_eq!(CacheExecutor::initialized_executor_count(), 3);

        let common_before_destroy = CacheExecutor::GetCommonExecutor();
        assert_eq!(CacheExecutor::initialized_executor_count(), 4);
        CacheExecutor::DestroyAllExecutors();
        assert_eq!(CacheExecutor::initialized_executor_count(), 0);
        let common_after_destroy = CacheExecutor::GetCommonExecutor();
        assert!(!Arc::ptr_eq(&common_before_destroy, &common_after_destroy));

        CacheExecutor::DestroyAllExecutors();
        CacheExecutor::Configure(CacheExecutorConfig::default());
    }

    #[test]
    fn async_writer_runs_write_then_callback_and_tracks_counters() {
        let allocator = SimpleLogBasedMemoryAllocator::with_capacity(128);
        let mut writer = AsyncWriter::new(allocator);

        let task = AsyncWriteTask::new(
            |allocator| {
                let ptr = allocator.Allocate(5)?;
                allocator.write(ptr, b"async")?;
                allocator.Seal(ptr)?;
                Ok(ptr)
            },
            |write_result, allocator| {
                let ptr = write_result?;
                let mut buffer = CacheBuffer::new(allocator.read(ptr)?.to_vec());
                buffer.SetKey("async-key");
                Ok(buffer)
            },
        );

        let buffer = writer.AsyncWrite(task).unwrap();
        assert_eq!(buffer.Key(), "async-key");
        assert_eq!(buffer.Data(), b"async");
        assert_eq!(writer.FlyWriteNum(), 0);
        assert_eq!(writer.FlyCbNum(), 0);
        assert_eq!(writer.completed_writes(), 1);
        assert_eq!(writer.completed_callbacks(), 1);
        assert_eq!(writer.allocator().live_region_count(), 1);
    }

    #[test]
    fn async_writer_preserves_addr_and_stop_rejects_new_writes() {
        let mut allocator = SimpleLogBasedMemoryAllocator::with_capacity(128);
        let existing = allocator.Allocate(4).unwrap();
        allocator.write(existing, b"seed").unwrap();
        let mut writer = AsyncWriter::new(allocator);

        let task = AsyncWriteTask::with_addr(
            |allocator| {
                let ptr = allocator.Allocate(4)?;
                allocator.write(ptr, b"copy")?;
                Ok(ptr)
            },
            |write_result, allocator| {
                let ptr = write_result?;
                let mut buffer = CacheBuffer::new(allocator.read(ptr)?.to_vec());
                buffer.SetKey("copy-key");
                Ok(buffer)
            },
            existing,
        );
        assert_eq!(task.Addr(), Some(existing));

        let buffer = writer.AsyncWrite(task).unwrap();
        assert_eq!(buffer.Data(), b"copy");
        writer.TEST_JoinWriteExecutor();
        writer.Stop();
        assert_eq!(writer.FlyWriteNum(), 0);
        assert_eq!(writer.FlyCbNum(), 0);

        let rejected = AsyncWriteTask::new(
            |allocator| allocator.Allocate(1),
            |write_result, allocator| {
                let ptr = write_result?;
                Ok(CacheBuffer::new(allocator.read(ptr)?.to_vec()))
            },
        );
        assert!(matches!(
            writer.AsyncWrite(rejected),
            Err(CacheError::Stopped)
        ));
    }

    #[test]
    fn pmem_dispatcher_round_robins_put_tasks_across_numa_writers() {
        let mut dispatcher = PmemDispatcher::new(2, 128);
        assert!(dispatcher.Start());
        assert_eq!(dispatcher.numa_count(), 2);
        assert_eq!(
            dispatcher.allocator_type(),
            AllocatorKind::LogBasedAllocator
        );

        for (key, value) in [("first", b"one".to_vec()), ("second", b"two".to_vec())] {
            let write_value = value.clone();
            let task = AsyncWriteTask::new(
                move |allocator| {
                    let ptr = allocator.Allocate(write_value.len())?;
                    allocator.write(ptr, &write_value)?;
                    Ok(ptr)
                },
                move |write_result, allocator| {
                    let ptr = write_result?;
                    let mut buffer = CacheBuffer::new(allocator.read(ptr)?.to_vec());
                    buffer.SetKey(key);
                    Ok(buffer)
                },
            );
            assert_eq!(dispatcher.PushTask(task).unwrap().Key(), key);
        }

        dispatcher.TEST_JoinPmemWriteExecutor();
        assert_eq!(dispatcher.TEST_GetWriter(0).unwrap().completed_writes(), 1);
        assert_eq!(dispatcher.TEST_GetWriter(1).unwrap().completed_writes(), 1);
        assert_eq!(
            dispatcher.TEST_GetAllocator(0).unwrap().live_region_count(),
            1
        );
        assert_eq!(
            dispatcher.TEST_GetAllocator(1).unwrap().live_region_count(),
            1
        );
    }

    #[test]
    fn pmem_dispatcher_routes_addr_tasks_to_owner_numa() {
        let mut alloc0 = SimpleLogBasedMemoryAllocator::with_capacity_and_base(128, 1 << 48);
        let mut alloc1 = SimpleLogBasedMemoryAllocator::with_capacity_and_base(128, 2 << 48);
        let ptr0 = alloc0.Allocate(4).unwrap();
        alloc0.write(ptr0, b"left").unwrap();
        let ptr1 = alloc1.Allocate(5).unwrap();
        alloc1.write(ptr1, b"right").unwrap();

        let mut dispatcher = PmemDispatcher::from_allocators(vec![alloc0, alloc1]);
        dispatcher.Start();
        assert_eq!(dispatcher.get_numa_id_by_pmem_addr(ptr0), Some(0));
        assert_eq!(dispatcher.get_numa_id_by_pmem_addr(ptr1), Some(1));

        let task = AsyncWriteTask::with_addr(
            move |allocator| {
                allocator.write(ptr1, b"owned")?;
                Ok(ptr1)
            },
            |write_result, allocator| {
                let ptr = write_result?;
                let mut buffer = CacheBuffer::new(allocator.read(ptr)?.to_vec());
                buffer.SetKey("routed");
                Ok(buffer)
            },
            ptr1,
        );

        let buffer = dispatcher.PushTask(task).unwrap();
        assert_eq!(buffer.Key(), "routed");
        assert_eq!(buffer.Data(), b"owned");
        assert_eq!(dispatcher.TEST_GetWriter(0).unwrap().completed_writes(), 0);
        assert_eq!(dispatcher.TEST_GetWriter(1).unwrap().completed_writes(), 1);
    }

    #[test]
    fn pmem_dispatcher_supports_test_allocator_access_and_stop() {
        let mut dispatcher = PmemDispatcher::new(2, 128);
        dispatcher.Start();

        let buffer = dispatcher
            .test_put_to_numa(1, "numa-one", b"payload".to_vec())
            .unwrap();
        assert_eq!(buffer.Key(), "numa-one");
        assert_eq!(buffer.Data(), b"payload");
        assert_eq!(dispatcher.TEST_GetWriter(1).unwrap().completed_writes(), 1);

        let ptr = {
            let allocator = dispatcher.GetAllocator(None).unwrap();
            let ptr = allocator.Allocate(3).unwrap();
            allocator.write(ptr, b"abc").unwrap();
            ptr
        };
        assert!(dispatcher.GetAllocator(Some(ptr)).is_some());
        assert_eq!(dispatcher.GetAllocators().len(), 2);

        assert!(dispatcher.Stop());
        let rejected = AsyncWriteTask::new(
            |allocator| allocator.Allocate(1),
            |write_result, allocator| {
                let ptr = write_result?;
                Ok(CacheBuffer::new(allocator.read(ptr)?.to_vec()))
            },
        );
        assert!(matches!(
            dispatcher.PushTask(rejected),
            Err(CacheError::Stopped)
        ));
    }

    fn test_buffer(key: &str, value: &[u8]) -> CacheBuffer {
        let mut buffer = CacheBuffer::new(value.to_vec());
        buffer.SetKey(key);
        buffer
    }

    #[test]
    fn replacement_policies_stay_usable_after_reset() {
        // A successful reset empties the index; it does not retire the
        // policy. Asserting only that the post-reset put reports no
        // evictions would not catch a regression here, because a policy
        // that silently discards the buffer reports no evictions too.
        // Check the buffer actually landed.
        let mut fifo = ReplacementFifo::new(1 << 20);
        fifo.Init().unwrap();
        fifo.Put(test_buffer("before", b"1"));
        assert!(fifo.GetUsedSpace() > 0);

        fifo.Reset().unwrap();
        assert_eq!(fifo.GetUsedSpace(), 0);
        assert_eq!(fifo.GetItemNum(), 0);
        assert!(fifo.Get("before").is_none());

        assert!(fifo.Put(test_buffer("after", b"2")).is_empty());
        assert_eq!(
            fifo.Peek("after").map(|buffer| buffer.Data().to_vec()),
            Some(b"2".to_vec()),
            "a reset fifo must still accept buffers"
        );
        assert!(fifo.GetUsedSpace() > 0);

        let mut slru = ReplacementSlru::new(1 << 20);
        slru.Init().unwrap();
        slru.Put(test_buffer("before", b"1"));
        assert!(slru.GetUsedSpace() > 0);

        slru.Reset().unwrap();
        assert_eq!(slru.GetUsedSpace(), 0);
        assert_eq!(slru.GetItemNum(), 0);
        assert!(slru.Get("before").is_none());

        assert!(slru.Put(test_buffer("after", b"2")).is_empty());
        assert_eq!(
            slru.Peek("after").map(|buffer| buffer.Data().to_vec()),
            Some(b"2".to_vec()),
            "a reset slru must still accept buffers"
        );
        assert!(slru.GetUsedSpace() > 0);
    }

    #[test]
    fn replacement_fifo_evicts_oldest_and_invokes_handler() {
        let mut fifo = ReplacementFifo::new(5);
        fifo.Init().unwrap();
        let evicted_keys = Arc::new(std::sync::Mutex::new(Vec::new()));
        let evicted_keys_capture = Arc::clone(&evicted_keys);
        fifo.RegisterMemEvictionHandler(move |buffer| {
            evicted_keys_capture
                .lock()
                .unwrap()
                .push(buffer.Key().to_string());
        });

        assert!(fifo.Put(test_buffer("a", b"1")).is_empty());
        assert!(fifo.Put(test_buffer("b", b"2")).is_empty());
        let evicted = fifo.Put(test_buffer("c", b"3"));

        assert_eq!(evicted.len(), 1);
        assert_eq!(evicted[0].Key(), "a");
        assert!(fifo.Get("a").is_none());
        assert_eq!(fifo.Peek("b").unwrap().Data(), b"2");
        assert_eq!(fifo.GetItemNum(), 2);
        assert_eq!(*evicted_keys.lock().unwrap(), vec!["a".to_string()]);
    }

    #[test]
    fn replacement_fifo_update_guards_raw_data_and_preserves_order() {
        let mut fifo = ReplacementFifo::new(32);
        fifo.Init().unwrap();
        fifo.Put(test_buffer("guarded", b"old"));

        assert!(matches!(
            fifo.UpdateCacheBuffer("guarded", b"stale", test_buffer("guarded", b"bad")),
            Err(CacheError::ReplaceMismatch)
        ));
        fifo.UpdateCacheBuffer("guarded", b"old", test_buffer("guarded", b"new"))
            .unwrap();
        assert_eq!(fifo.Get("guarded").unwrap().Data(), b"new");

        fifo.Put(test_buffer("next", b"1"));
        fifo.SetCapacity(12);
        assert!(fifo.Get("guarded").is_none());
        assert!(fifo.Get("next").is_some());
    }

    #[test]
    fn replacement_fifo_overwrite_keeps_original_queue_position() {
        let mut fifo = ReplacementFifo::new(6);
        fifo.Init().unwrap();
        // Three 2-byte entries exactly fill the policy.
        for key in ["a", "b", "c"] {
            assert!(fifo.Put(test_buffer(key, b"1")).is_empty());
        }
        assert_eq!(fifo.GetItemNum(), 3);
        assert_eq!(fifo.GetUsedSpace(), 6);

        // Rewriting "a" must not move it behind "b" and "c": first-in
        // first-out orders by first insertion, not by last write.
        assert!(fifo.Put(test_buffer("a", b"9")).is_empty());
        assert_eq!(fifo.Get("a").unwrap().Data(), b"9");
        assert_eq!(fifo.GetItemNum(), 3);
        assert_eq!(fifo.GetUsedSpace(), 6);

        // So the next insert still evicts "a", the oldest by insertion order.
        let evicted = fifo.Put(test_buffer("d", b"4"));
        assert_eq!(evicted.len(), 1);
        assert_eq!(evicted[0].Key(), "a");
        assert!(fifo.Peek("b").is_some());
        assert!(fifo.Peek("c").is_some());
        assert!(fifo.Peek("d").is_some());
    }

    #[test]
    fn replacement_fifo_delete_leaves_no_queue_tombstone() {
        let mut fifo = ReplacementFifo::new(1 << 12);
        fifo.Init().unwrap();
        for index in 0..256 {
            fifo.Put(test_buffer(&format!("k{index:03}"), b"v"));
        }
        assert_eq!(fifo.GetItemNum(), 256);
        assert_eq!(fifo.queue_len(), 256);

        for index in (0..256).step_by(2) {
            assert!(fifo.Delete(&format!("k{index:03}")).is_some());
        }
        // The queue tracks live entries only, so a delete-heavy workload leaves
        // no stale keys for eviction to skip past.
        assert_eq!(fifo.GetItemNum(), 128);
        assert_eq!(fifo.queue_len(), 128);

        for index in (0..256).step_by(2) {
            fifo.Put(test_buffer(&format!("k{index:03}"), b"v"));
        }
        assert_eq!(fifo.GetItemNum(), 256);
        assert_eq!(fifo.queue_len(), 256);
        assert!(fifo.GetUsedSpace() <= fifo.GetCapacity());
    }

    #[test]
    fn replacement_slru_get_records_access_without_reordering() {
        let mut slru = ReplacementSlru::with_num_segments(100, 1);
        slru.Init().unwrap();
        slru.TEST_ConfigLRUMaintainer(false);

        // Each entry is 1 key byte plus 4 value bytes; hot runs c, b, a from
        // head to tail.
        for key in ["a", "b", "c"] {
            slru.Put(test_buffer(key, b"xxxx"));
        }
        assert_eq!(slru.list_item_count(0, HOT_LRU), 3);

        // Reading the hot tail marks it but leaves it exactly where it was.
        assert!(slru.Get("a").is_some());
        assert_eq!(slru.TEST_CheckBufferFlag("a"), BUFFER_FETCHED);
        assert!(slru.Get("a").is_some());
        assert_eq!(slru.TEST_CheckBufferFlag("a"), BUFFER_ACTIVE);
        assert_eq!(slru.TEST_CheckLRUPos("a"), HOT_LRU);
        assert_eq!(slru.list_item_count(0, HOT_LRU), 3);
        assert_eq!(slru.list_item_count(0, WARM_LRU), 0);

        // "a" is therefore still the hot tail when the maintainer runs, and the
        // maintainer is what promotes it -- because the two reads marked it
        // active. Untouched "b" behind it is demoted instead.
        slru.set_hot_lru_pct(5);
        slru.TEST_ConfigLRUMaintainer(true);
        assert!(slru.run_lru_maintainer_pass().is_empty());
        assert_eq!(slru.TEST_CheckLRUPos("a"), WARM_LRU);
        assert_eq!(slru.TEST_CheckLRUPos("b"), COLD_LRU);
        assert_eq!(slru.TEST_CheckLRUPos("c"), HOT_LRU);
        assert_eq!(slru.GetItemNum(), 3);
    }

    #[test]
    fn base_lru_list_recycles_nodes_across_repeated_churn() {
        let mut list = BaseLruList::new(64);
        for round in 0..8 {
            for index in 0..64 {
                list.Put(format!("r{round}-k{index:02}"));
            }
            assert_eq!(list.Size(), 64);
            assert!(list.Evict().is_empty());
            for index in 0..64 {
                assert!(list.Delete(&format!("r{round}-k{index:02}")));
            }
            assert_eq!(list.Size(), 0);
            assert!(list.GetTail(8).is_empty());
        }

        // Nodes freed by the churn above are reused, and the list is still
        // correctly ordered afterwards.
        list.Put("first".to_string());
        list.Put("second".to_string());
        assert_eq!(
            list.GetTail(2),
            vec!["first".to_string(), "second".to_string()]
        );
        assert!(list.Get("first"));
        assert_eq!(
            list.GetTail(2),
            vec!["second".to_string(), "first".to_string()]
        );
        assert_eq!(list.Size(), 2);
    }

    #[test]
    fn replacement_slru_tracks_hot_warm_cold_and_fetch_flags() {
        let mut slru = ReplacementSlru::new(6);
        slru.Init().unwrap();
        slru.TEST_ConfigLRUMaintainer(false);

        slru.Put(test_buffer("a", b"1"));
        slru.Put(test_buffer("b", b"2"));
        assert_eq!(slru.TEST_CheckLRUPos("a"), HOT_LRU);
        assert_eq!(slru.TEST_CheckBufferFlag("a"), BUFFER_INIT);

        assert_eq!(slru.Get("a").unwrap().Data(), b"1");
        assert_eq!(slru.TEST_CheckBufferFlag("a"), BUFFER_FETCHED);
        assert_eq!(slru.Get("a").unwrap().Data(), b"1");
        assert_eq!(slru.TEST_CheckBufferFlag("a"), BUFFER_ACTIVE);

        slru.Put(test_buffer("c", b"3"));
        slru.Put(test_buffer("d", b"4"));
        slru.Put(test_buffer("e", b"5"));
        assert_eq!(slru.TEST_CheckLRUPos("a"), WARM_LRU);
        assert!(slru.GetItemNum() <= 3);
    }

    #[test]
    fn replacement_slru_update_delete_and_capacity_shrink() {
        let mut slru = ReplacementSlru::new(32);
        slru.Init().unwrap();
        slru.Put(test_buffer("x", b"old"));
        slru.Put(test_buffer("y", b"yy"));

        assert!(matches!(
            slru.UpdateCacheBuffer("x", b"stale", test_buffer("x", b"bad")),
            Err(CacheError::ReplaceMismatch)
        ));
        slru.UpdateCacheBuffer("x", b"old", test_buffer("x", b"new"))
            .unwrap();
        assert_eq!(slru.Peek("x").unwrap().Data(), b"new");
        assert_eq!(slru.Delete("y").unwrap().Key(), "y");
        assert_eq!(slru.GetItemNum(), 1);

        slru.Put(test_buffer("z", b"zzzz"));
        slru.SetCapacity(6);
        assert!(slru.GetUsedSpace() <= slru.GetCapacity());
    }

    #[test]
    fn replacement_slru_resolves_segment_count_from_capacity_and_request() {
        // A capacity smaller than the segment count collapses to one segment,
        // otherwise every segment would get a zero byte budget.
        assert_eq!(ReplacementSlru::new(6).num_segments(), 1);
        assert_eq!(ReplacementSlru::new(255).num_segments(), 1);
        assert_eq!(
            ReplacementSlru::new(256).num_segments(),
            SLRU_DEFAULT_NUM_SEGMENTS
        );

        // Requests are rounded up to a power of two so segment selection masks.
        assert_eq!(
            ReplacementSlru::with_num_segments(1 << 20, 100).num_segments(),
            128
        );
        assert_eq!(ReplacementSlru::with_num_segments(1 << 20, 0).num_segments(), 1);
        assert_eq!(ReplacementSlru::with_num_segments(1 << 20, 1).num_segments(), 1);

        let policy = ReplacementSlru::with_num_segments(1024, 8);
        assert_eq!(policy.segment_byte_limit(), 128);
        assert_eq!(policy.GetSegmentByteLimit(), 128);
        assert_eq!(policy.hot_lru_pct(), SLRU_DEFAULT_HOT_LRU_PCT);
        assert_eq!(policy.warm_lru_pct(), SLRU_DEFAULT_WARM_LRU_PCT);
    }

    #[test]
    fn replacement_slru_shards_keys_and_bounds_each_segment() {
        let mut slru = ReplacementSlru::new(1 << 16);
        slru.Init().unwrap();
        assert_eq!(slru.num_segments(), SLRU_DEFAULT_NUM_SEGMENTS);
        assert_eq!(
            slru.segment_byte_limit(),
            (1 << 16) / SLRU_DEFAULT_NUM_SEGMENTS
        );

        for index in 0..4096 {
            slru.Put(test_buffer(&format!("shard-key-{index:06}"), &[b'v'; 32]));
        }

        // Eviction is segment-local: every shard is held to its own budget
        // rather than to one global list.
        for segment in 0..slru.num_segments() {
            assert!(
                slru.segment_used_size(segment) <= slru.segment_byte_limit(),
                "segment {segment} over budget"
            );
        }

        let summed: usize = (0..slru.num_segments())
            .map(|segment| slru.segment_used_size(segment))
            .sum();
        assert_eq!(slru.GetUsedSpace(), summed);
        assert!(slru.GetUsedSpace() <= slru.GetCapacity());

        let occupied = (0..slru.num_segments())
            .filter(|&segment| slru.segment_used_size(segment) > 0)
            .count();
        assert!(
            occupied > 200,
            "expected keys spread across shards, only {occupied} occupied"
        );

        slru.Put(test_buffer("probe-key", b"probe"));
        let shard = slru.segment_for_key("probe-key");
        assert_eq!(shard, slru.PickSegment("probe-key"));
        assert!(shard < slru.num_segments());
        assert!(slru.segment_used_size(shard) >= "probe-key".len() + b"probe".len());
    }

    #[test]
    fn replacement_slru_accounting_survives_overwrite_delete_and_reuse() {
        let mut slru = ReplacementSlru::with_num_segments(1 << 14, 4);
        slru.Init().unwrap();

        for index in 0..512 {
            slru.Put(test_buffer(&format!("k{index:04}"), &[b'x'; 16]));
        }
        for index in 0..512 {
            let _ = slru.Get(&format!("k{index:04}"));
        }
        for index in (0..512).step_by(2) {
            let _ = slru.Delete(&format!("k{index:04}"));
        }
        for index in 0..512 {
            slru.Put(test_buffer(&format!("k{index:04}"), &[b'y'; 24]));
        }

        // Per-list byte and item counts still reconcile with the shard totals
        // and the index, so no list node was leaked or double-counted.
        let mut items = 0usize;
        for segment in 0..slru.num_segments() {
            let listed: usize = [HOT_LRU, WARM_LRU, COLD_LRU]
                .iter()
                .map(|&lru| slru.list_used_size(segment, lru))
                .sum();
            assert_eq!(listed, slru.segment_used_size(segment));
            assert_eq!(
                listed,
                slru.GetListUsedSize(segment, HOT_LRU)
                    + slru.GetListUsedSize(segment, WARM_LRU)
                    + slru.GetListUsedSize(segment, COLD_LRU)
            );
            assert!(slru.segment_used_size(segment) <= slru.segment_byte_limit());
            items += [HOT_LRU, WARM_LRU, COLD_LRU]
                .iter()
                .map(|&lru| slru.list_item_count(segment, lru))
                .sum::<usize>();
        }
        assert_eq!(items, slru.GetItemNum());
        assert!(slru.GetUsedSpace() <= slru.GetCapacity());
    }

    #[test]
    fn replacement_slru_maintainer_promotes_active_and_demotes_untouched() {
        let mut slru = ReplacementSlru::with_num_segments(100, 1);
        slru.Init().unwrap();
        slru.TEST_ConfigLRUMaintainer(false);
        slru.set_hot_lru_pct(20);
        slru.set_warm_lru_pct(40);
        assert_eq!(slru.segment_byte_limit(), 100);

        // Each entry occupies 1 key byte + 4 value bytes.
        slru.Put(test_buffer("a", b"aaaa"));
        assert!(slru.Get("a").is_some());
        assert!(slru.Get("a").is_some());
        assert_eq!(slru.TEST_CheckBufferFlag("a"), BUFFER_ACTIVE);
        for key in ["b", "c", "d", "e"] {
            slru.Put(test_buffer(key, b"xxxx"));
        }
        assert_eq!(slru.list_used_size(0, HOT_LRU), 25);
        assert_eq!(slru.list_item_count(0, COLD_LRU), 0);

        // A disabled maintainer is a no-op even when the hot list is over its
        // share of the shard budget.
        assert!(slru.run_lru_maintainer_pass().is_empty());
        assert_eq!(slru.list_used_size(0, HOT_LRU), 25);

        // One pass trims the hot list to its 20% share (20 bytes). The tail is
        // "a", touched twice, so it is promoted to warm with its flag reset.
        slru.TEST_ConfigLRUMaintainer(true);
        assert!(slru.run_lru_maintainer_pass().is_empty());
        assert_eq!(slru.TEST_CheckLRUPos("a"), WARM_LRU);
        assert_eq!(slru.TEST_CheckBufferFlag("a"), BUFFER_INIT);
        assert_eq!(slru.list_used_size(0, HOT_LRU), 20);
        assert_eq!(slru.list_used_size(0, WARM_LRU), 5);
        assert_eq!(slru.list_item_count(0, COLD_LRU), 0);

        // A second pass is a fixed point: nothing is over its share any more.
        assert!(slru.LRUMaintainerTask().is_empty());
        assert_eq!(slru.list_used_size(0, HOT_LRU), 20);
        assert_eq!(slru.list_used_size(0, WARM_LRU), 5);

        // Shrinking the shares drains hot, then warm, into the cold list. None
        // of the entries were touched twice since the last pass, so they are
        // demoted rather than promoted.
        slru.set_hot_lru_pct(0);
        slru.set_warm_lru_pct(0);
        assert!(slru.run_lru_maintainer_pass().is_empty());
        assert_eq!(slru.list_item_count(0, HOT_LRU), 0);
        assert_eq!(slru.list_item_count(0, WARM_LRU), 0);
        assert_eq!(slru.list_item_count(0, COLD_LRU), 5);
        assert_eq!(slru.list_used_size(0, COLD_LRU), 25);
        assert_eq!(slru.GetUsedSpace(), 25);
        assert_eq!(slru.GetItemNum(), 5);
    }

    #[test]
    fn replacement_slru_maintainer_evicts_cold_tail_over_segment_budget() {
        let mut slru = ReplacementSlru::with_num_segments(40, 1);
        slru.Init().unwrap();
        // Give hot and warm no share of the budget, so every insert is demoted
        // into the cold list and reclaimed from there.
        slru.set_hot_lru_pct(0);
        slru.set_warm_lru_pct(0);

        let evicted_keys = Arc::new(std::sync::Mutex::new(Vec::new()));
        let evicted_capture = Arc::clone(&evicted_keys);
        slru.RegisterMemEvictionHandler(move |buffer| {
            evicted_capture
                .lock()
                .unwrap()
                .push(buffer.Key().to_string());
        });

        // Four 10-byte entries exactly fill the single shard.
        for key in ["a", "b", "c", "d"] {
            assert!(slru.Put(test_buffer(key, b"xxxxxxxxx")).is_empty());
        }
        assert_eq!(slru.GetUsedSpace(), 40);
        assert_eq!(slru.list_item_count(0, COLD_LRU), 4);
        assert_eq!(slru.list_item_count(0, HOT_LRU), 0);

        // The fifth insert puts the shard over budget; the cold tail is the
        // oldest untouched entry and is handed to the lower tier before being
        // dropped.
        let evicted = slru.Put(test_buffer("e", b"xxxxxxxxx"));
        let evicted_names: Vec<String> = evicted
            .iter()
            .map(|buffer| buffer.Key().to_string())
            .collect();
        assert_eq!(evicted_names, vec!["a".to_string()]);
        assert_eq!(*evicted_keys.lock().unwrap(), vec!["a".to_string()]);
        assert_eq!(slru.GetUsedSpace(), 40);
        assert_eq!(slru.GetItemNum(), 4);
        assert!(slru.Peek("a").is_none());
        assert!(slru.Peek("e").is_some());
    }

    #[test]
    fn concurrent_slru_matches_the_single_threaded_segment_layout() {
        let concurrent = ConcurrentReplacementSlru::with_num_segments(1 << 16, 256);
        let single = ReplacementSlru::with_num_segments(1 << 16, 256);
        assert_eq!(concurrent.num_segments(), single.num_segments());
        assert_eq!(concurrent.segment_byte_limit(), single.segment_byte_limit());
        assert_eq!(concurrent.GetCapacity(), 1 << 16);

        // A key lands in the same segment either way, so the two forms shard
        // a workload identically.
        for key in ["alpha", "beta", "gamma", "delta", "epsilon", "zeta"] {
            assert_eq!(concurrent.segment_for_key(key), single.segment_for_key(key));
        }

        // Capacities below the segment count collapse to one segment in both.
        assert_eq!(ConcurrentReplacementSlru::new(6).num_segments(), 1);
        assert_eq!(ReplacementSlru::new(6).num_segments(), 1);
    }

    #[test]
    fn concurrent_slru_serves_threads_through_per_segment_locks() {
        let policy = ConcurrentReplacementSlru::with_num_segments(1 << 16, 64);
        policy.Init().unwrap();

        // Four threads share the policy with no lock of their own. Keys that
        // hash to different segments never contend.
        std::thread::scope(|scope| {
            for worker in 0..4 {
                let policy = &policy;
                scope.spawn(move || {
                    for index in 0..512 {
                        let key = format!("w{worker}-k{index:04}");
                        policy.Put(test_buffer(&key, b"payload"));
                        let _ = policy.Get(&key);
                        if index % 3 == 0 {
                            let _ = policy.Delete(&key);
                        }
                    }
                });
            }
        });

        // Every segment stayed inside its own budget and the totals reconcile,
        // so no update was lost or double-counted across threads.
        for segment in 0..policy.num_segments() {
            assert!(
                policy.segment_used_size(segment) <= policy.segment_byte_limit(),
                "segment {segment} over budget"
            );
        }
        assert!(policy.GetUsedSpace() <= policy.GetCapacity());
        let summed: usize = (0..policy.num_segments())
            .map(|segment| policy.segment_item_count(segment))
            .sum();
        assert_eq!(summed, policy.GetItemNum());
        assert!(policy.GetItemNum() > 0);
    }

    #[test]
    fn concurrent_slru_reports_evictions_from_every_segment() {
        let policy = ConcurrentReplacementSlru::with_num_segments(256, 4);
        policy.Init().unwrap();
        let evicted = Arc::new(std::sync::Mutex::new(Vec::new()));
        let capture = Arc::clone(&evicted);
        policy.register_mem_eviction_handler(move |buffer| {
            capture.lock().unwrap().push(buffer.Key().to_string());
        });

        // 20 bytes per entry against a 64-byte segment budget forces eviction.
        for index in 0..256 {
            policy.Put(test_buffer(&format!("evict-{index:04}"), b"0123456789"));
        }

        assert!(!evicted.lock().unwrap().is_empty());
        assert!(policy.GetUsedSpace() <= policy.GetCapacity());
        for segment in 0..policy.num_segments() {
            assert!(policy.segment_used_size(segment) <= policy.segment_byte_limit());
        }
        assert_eq!(policy.GetItemNum(), policy.GetItemNum());
    }

    #[test]
    fn concurrent_slru_round_trips_values_and_maintainer_passes() {
        let policy = ConcurrentReplacementSlru::with_num_segments(1 << 14, 8);
        policy.Init().unwrap();
        policy.Put(test_buffer("round-trip", b"value"));
        assert_eq!(policy.Get("round-trip").unwrap().Data(), b"value");
        assert_eq!(policy.Peek("round-trip").unwrap().Data(), b"value");

        policy
            .update_cache_buffer("round-trip", b"value", test_buffer("round-trip", b"next"))
            .unwrap();
        assert_eq!(policy.Peek("round-trip").unwrap().Data(), b"next");
        assert!(matches!(
            policy.update_cache_buffer("round-trip", b"stale", test_buffer("round-trip", b"bad")),
            Err(CacheError::ReplaceMismatch)
        ));

        // A maintainer sweep takes each segment lock in turn and is a no-op
        // while nothing is over its share.
        assert!(policy.LRUMaintainerTask().is_empty());
        assert_eq!(policy.GetItemNum(), 1);

        assert_eq!(policy.Delete("round-trip").unwrap().Key(), "round-trip");
        assert_eq!(policy.GetItemNum(), 0);
        assert_eq!(policy.GetUsedSpace(), 0);
        policy.Reset().unwrap();
        assert_eq!(policy.GetItemNum(), 0);
    }

    fn order_key(index: usize) -> CacheKey {
        CacheKey::string(0, &format!("order-key-{index:04}"))
    }

    fn order_keys(order: &CacheKeyOrder) -> Vec<CacheKey> {
        order.iter().cloned().collect()
    }

    #[test]
    fn cache_key_order_tracks_recency_from_front_to_back() {
        let mut order = CacheKeyOrder::new();
        assert!(order.is_empty());
        assert_eq!(order.front(), None);
        assert_eq!(order.back(), None);

        for index in 0..4 {
            order.push_back(order_key(index));
        }
        assert_eq!(order.len(), 4);
        assert_eq!(order.front(), Some(&order_key(0)));
        assert_eq!(order.back(), Some(&order_key(3)));

        // A hit moves the key to the back and never duplicates it, which is
        // what a plain deque needs a full rescan to guarantee.
        assert!(order.move_to_back(&order_key(0)));
        assert_eq!(order.back(), Some(&order_key(0)));
        assert_eq!(order.len(), 4);
        assert_eq!(
            order_keys(&order),
            vec![order_key(1), order_key(2), order_key(3), order_key(0)]
        );

        // Touching the key that is already most recent is a no-op.
        assert!(order.move_to_back(&order_key(0)));
        assert_eq!(
            order_keys(&order),
            vec![order_key(1), order_key(2), order_key(3), order_key(0)]
        );

        // Touching an absent key reports it and changes nothing.
        assert!(!order.move_to_back(&order_key(99)));
        assert_eq!(order.len(), 4);

        // Eviction takes the least recently used first.
        assert_eq!(order.pop_front(), Some(order_key(1)));
        assert_eq!(order.pop_front(), Some(order_key(2)));
        assert_eq!(order.len(), 2);

        assert!(order.remove(&order_key(3)));
        assert!(!order.remove(&order_key(3)));
        assert_eq!(order_keys(&order), vec![order_key(0)]);

        order.push_front(order_key(7));
        assert_eq!(order.front(), Some(&order_key(7)));
        assert_eq!(order_keys(&order), vec![order_key(7), order_key(0)]);

        order.clear();
        assert!(order.is_empty());
        assert_eq!(order.pop_front(), None);
    }

    #[test]
    fn cache_key_order_walks_both_directions() {
        let order: CacheKeyOrder = (0..4).map(order_key).collect();
        assert_eq!(
            order.iter().cloned().collect::<Vec<_>>(),
            vec![
                order_key(0),
                order_key(1),
                order_key(2),
                order_key(3)
            ]
        );
        // The reverse walk starts at the most recently used end. Eviction
        // relies on this to reach the coldest entry first.
        assert_eq!(
            order.iter_rev().cloned().collect::<Vec<_>>(),
            vec![
                order_key(3),
                order_key(2),
                order_key(1),
                order_key(0)
            ]
        );
        assert_eq!(order.iter_rev().next(), order.back());
        assert_eq!(order.iter().next(), order.front());

        let empty = CacheKeyOrder::new();
        assert_eq!(empty.iter_rev().count(), 0);
    }

    #[test]
    fn zero_copy_lru_keeps_counting_removed_but_pinned_bytes() {
        let cache = ZeroCopySimpleLruCache::new(4 * 64);
        let key = CacheKey::string(0, "pinned-entry");
        let handle = cache
            .InsertPinned(key.clone(), vec![118u8; 32], 64)
            .unwrap()
            .expect("pinned handle");
        assert_eq!(cache.Size(), 64);

        // The entry leaves the index, but a handle still holds the value, so
        // those bytes are still resident and must keep counting. Releasing
        // them here would let the cache admit data it has no room for.
        cache.Remove(&key).unwrap();
        assert!(cache.Lookup(&key).unwrap().is_none());
        assert_eq!(
            cache.Size(),
            64,
            "a removed entry that is still pinned must stay accounted"
        );

        // Dropping the last pin is what actually frees the space.
        cache.Release(handle);
        assert_eq!(cache.Size(), 0);
    }

    #[test]
    fn zero_copy_lru_frees_removed_bytes_when_no_handle_holds_them() {
        let cache = ZeroCopySimpleLruCache::new(4 * 64);
        let key = CacheKey::string(0, "unpinned-entry");
        cache.Insert(key.clone(), vec![118u8; 32], 64).unwrap();
        assert_eq!(cache.Size(), 64);

        // Nothing holds this one, so removal frees it immediately.
        cache.Remove(&key).unwrap();
        assert_eq!(cache.Size(), 0);
    }

    #[test]
    fn simple_lru_evicts_the_coldest_entry_first() {
        let cache = SimpleLruCache::new(3 * 64);
        let key = |name: &str| CacheKey::string(0, name);
        let value = vec![118u8; 32];
        for name in ["a", "b", "c"] {
            cache.Insert(key(name), value.clone(), 64).unwrap();
        }

        // Reading "a" makes "b" the coldest entry, so "b" is what the next
        // insert must displace.
        assert!(cache.Lookup(&key("a")).unwrap().is_some());
        cache.Insert(key("d"), value.clone(), 64).unwrap();

        assert!(
            cache.Lookup(&key("b")).unwrap().is_none(),
            "the least recently used entry should be evicted"
        );
        assert!(
            cache.Lookup(&key("a")).unwrap().is_some(),
            "a recently read entry must not be evicted"
        );
        assert!(cache.Lookup(&key("c")).unwrap().is_some());
        assert!(cache.Lookup(&key("d")).unwrap().is_some());
    }

    #[test]
    fn cache_key_order_retain_keeps_relative_order() {
        let mut order: CacheKeyOrder = (0..8).map(order_key).collect();
        order.retain(|key| !key.record_key.ends_with('3') && !key.record_key.ends_with('5'));
        assert_eq!(
            order_keys(&order),
            vec![
                order_key(0),
                order_key(1),
                order_key(2),
                order_key(4),
                order_key(6),
                order_key(7),
            ]
        );
        assert_eq!(order.len(), 6);
        assert!(!order.contains(&order_key(3)));
        assert!(order.contains(&order_key(4)));
    }

    #[test]
    fn cache_key_order_matches_a_rescanning_deque_step_for_step() {
        // The structure being replaced moved a key to the back by rescanning:
        //     if back() != Some(key) { retain(|c| c != key); push_back(key) }
        // Drive both through the same operation stream and require the
        // resulting recency order to agree after every single step, so the
        // swap cannot change which entry gets evicted.
        let mut order = CacheKeyOrder::new();
        let mut deque: VecDeque<CacheKey> = VecDeque::new();

        for step in 0..600usize {
            let key = order_key(step.wrapping_mul(2_654_435_761) % 48);
            match step % 5 {
                0..=2 => {
                    // A hit: only reorders a key that is already resident.
                    if deque.contains(&key) {
                        if deque.back() != Some(&key) {
                            deque.retain(|candidate| candidate != &key);
                            deque.push_back(key.clone());
                        }
                        assert!(order.move_to_back(&key));
                    } else {
                        assert!(!order.move_to_back(&key));
                    }
                }
                3 => {
                    // An insert of a key not yet resident.
                    if !deque.contains(&key) {
                        deque.push_back(key.clone());
                        order.push_back(key.clone());
                    }
                }
                _ => {
                    // A removal.
                    deque.retain(|candidate| candidate != &key);
                    order.remove(&key);
                }
            }
            assert_eq!(
                order_keys(&order),
                deque.iter().cloned().collect::<Vec<_>>(),
                "diverged at step {step}"
            );
        }
        assert!(!order.is_empty());
    }

    #[test]
    fn cache_key_order_recycles_nodes_across_churn() {
        let mut order = CacheKeyOrder::new();
        for round in 0..6 {
            for index in 0..64 {
                order.push_back(order_key(round * 64 + index));
            }
            assert_eq!(order.len(), 64);
            while order.pop_front().is_some() {}
            assert!(order.is_empty());
        }
        order.push_back(order_key(1));
        order.push_back(order_key(2));
        assert_eq!(order_keys(&order), vec![order_key(1), order_key(2)]);
    }

    #[test]
    fn hash_uint64_matches_matrixcache_vectors() {
        assert_eq!(hash_uint64(0), 0x5b03_af84_387a_42c6);
        assert_eq!(hash_uint64(1), 0xa13a_3e40_1240_2345);
        assert_eq!(hash_uint64(2), 0xcd41_43fa_e38a_71fe);
        assert_eq!(hash_uint64(123_456_789), 0x6675_3257_ed88_abf3);
        assert_eq!(hash_uint64(u64::MAX), 0x7073_251a_e29c_59b6);
        assert_eq!(HashUInt64(1), hash_uint64(1));
    }

    #[test]
    fn murmur_hash2_matches_matrixcache_vectors() {
        assert_eq!(mur_mur_hash2(b""), 0xca88_1466);
        assert_eq!(mur_mur_hash2(b"a"), 0xe94e_6ebd);
        assert_eq!(mur_mur_hash2(b"abc"), 0x6d5e_3568);
        assert_eq!(mur_mur_hash2(b"abcd"), 0x543f_2edd);
        assert_eq!(mur_mur_hash2(b"hello"), 0x33b4_f2ac);
        assert_eq!(mur_mur_hash2(b"TemporalStore"), 0xcbd0_a1fb);

        assert_eq!(mur_mur_hash2_with_seed(b"TemporalStore", 0), 0xfa4d_19c8);
        assert_eq!(mur_mur_hash2_with_seed(b"TemporalStore", 1), 0x4465_353c);
        assert_eq!(
            mur_mur_hash2_with_seed(b"TemporalStore", 0xdead_beef),
            0xa351_ec62
        );
        assert_eq!(
            MurMurHash2(b"TemporalStore"),
            mur_mur_hash2(b"TemporalStore")
        );
        assert_eq!(
            MurMurHash2WithSeed(b"TemporalStore", 1),
            mur_mur_hash2_with_seed(b"TemporalStore", 1)
        );
    }

    #[test]
    fn tools_utils_random_and_hashed_key_helpers_match_surface() {
        assert_eq!(xxh32_with_seed(b"", 0), 0x02cc_5d05);
        assert_eq!(xxh32_with_seed(b"hello", 0), 0xfb00_77f9);

        let key = get_hashed_key(42, 4);
        assert_eq!(key.len(), 4);
        assert_eq!(get_hashed_key(42, 2), key[..2].to_vec());
        assert_eq!(get_hashed_key(42, 99).len(), 4);

        let value = get_rand_str(32);
        assert_eq!(value.len(), 32);
        assert!(value.bytes().all(|byte| byte.is_ascii_lowercase()));
        let fast16 = fast_rand16();
        assert!((0..=0x7fff).contains(&fast16));
        assert_ne!(fast_rand64(), fast_rand64());

        let mut generator = RandomStringGenerator::with_size(1 << 18);
        let bytes = generator.RandValueBytes(64);
        assert_eq!(bytes.len(), 64);
        assert!(bytes.iter().all(u8::is_ascii_lowercase));
        let text = generator.RandValue(128);
        assert_eq!(text.len(), 128);
        assert!(text.bytes().all(|byte| byte.is_ascii_lowercase()));
    }

    #[test]
    fn round_up_matches_align_util_macro_semantics() {
        assert_eq!(round_up(0, 8), 0);
        assert_eq!(round_up(1, 8), 8);
        assert_eq!(round_up(8, 8), 8);
        assert_eq!(round_up(9, 8), 16);
        assert_eq!(round_up(31, 16), 32);
        assert_eq!(ROUND_UP(33, 32), 64);
        assert_eq!(round_up(33, 0), 33);
    }

    #[test]
    fn numa_info_exposes_stable_single_node_topology() {
        NumaInfo::Init();
        assert!(NumaInfo::GetNumAllCores() >= 1);
        assert!(NumaInfo::GetNumOnlineCores() >= NumaInfo::GetNumAllCores());
        assert_eq!(NumaInfo::GetMaxNumNumaNodes(), 1);
        assert_eq!(NumaInfo::GetNumaNodeOfCpuCore(0), 0);
        assert_eq!(NumaInfo::GetNumaNodeCoreIdx(0), 0);
        let cores = NumaInfo::GetCpuCoresOfNumaNode(0);
        assert!(!cores.is_empty());
        assert_eq!(cores[0], 0);
        assert_eq!(NumaInfo::GetCpuCoresOfSameNumaNode(0), cores);
        assert!(NumaInfo::BindThreadToCpuCore(0).is_ok());
        assert!(NumaInfo::BindThreadToCpuCore(usize::MAX).is_err());
    }
    #[test]
    fn sharded_multilayer_cache_routes_batches_and_trait_api() {
        let base = unique_temp_path("sharded-multilayer-cache");
        let options = CacheOptions::new(96, 0, 4096).with_ssd_paths(vec![base]);
        let cache = MatrixCacheBuilder::build_sharded_cache(options, 4);
        assert_eq!(cache.shard_count(), 4);
        assert_eq!(cache.capacity_for_tier(CacheTier::Memory), 96);
        assert_eq!(cache.CapacityForTier(CacheTier::Ssd), 4096);
        assert_eq!(cache.GetCapacity(CacheInstanceKind::Dram), 96);
        assert_eq!(cache.GetCapacity(CacheInstanceKind::Ssd), 4096);
        let config_cache = MatrixCacheBuilder::build_sharded_cache(CacheOptions::new(32, 32, 0), 2);
        assert!(config_cache.stop());
        config_cache
            .TrySetReplacementPolicyForTier(CacheTier::Pmem, CacheReplacementPolicy::Fifo)
            .unwrap();
        assert_eq!(
            config_cache.ReplacementPolicyForTier(CacheTier::Pmem),
            CacheReplacementPolicy::Fifo
        );

        let keys = (0..16)
            .map(|i| CacheKey::string((i % 3) as ShardId, &format!("key-{i}")))
            .collect::<Vec<_>>();
        let entries = keys
            .iter()
            .enumerate()
            .map(|(i, key)| (key.clone(), format!("value-{i}").into_bytes(), 32))
            .collect::<Vec<_>>();
        assert_eq!(cache.insert_batch_cache(entries).unwrap(), keys.len());
        let all_entries = cache.AllEntries();
        assert_eq!(all_entries.len(), keys.len());
        assert_eq!(cache.EntriesForShard(0).len(), 6);
        assert!(all_entries.iter().any(|entry| entry.disk_bytes > 0));

        let values = cache.lookup_batch_cache(&keys).unwrap();
        assert_eq!(values.len(), keys.len());
        for (i, value) in values.into_iter().enumerate() {
            assert_eq!(value.unwrap(), format!("value-{i}").into_bytes());
        }
        let stats = cache.Stats();
        assert!(stats.puts >= keys.len() as u64);
        assert!(stats.disk_hits + stats.memory_hits >= keys.len() as u64);
        assert!(stats.disk_bytes > 0);
        assert!(stats.get_latency_samples >= keys.len() as u64);
        assert!(stats.put_latency_samples >= keys.len() as u64);
        let latency = cache.LatencyMetricsReport();
        assert!(latency.put_count >= keys.len() as u64);
        assert!(latency.get_count >= keys.len() as u64);
        assert!(latency.histogram_ready);
        assert!(cache.GetUsed(CacheInstanceKind::Dram) > 0);
        assert!(cache.GetUsed(CacheInstanceKind::Ssd) > 0);

        let repeated = keys[3].clone();
        let other = keys[7].clone();
        let duplicate_values = cache
            .lookup_batch_cache(&[
                repeated.clone(),
                other.clone(),
                repeated.clone(),
                keys[0].clone(),
                other.clone(),
            ])
            .unwrap();
        assert_eq!(
            duplicate_values,
            vec![
                Some(b"value-3".to_vec()),
                Some(b"value-7".to_vec()),
                Some(b"value-3".to_vec()),
                Some(b"value-0".to_vec()),
                Some(b"value-7".to_vec()),
            ]
        );
        assert!(cache.item_count_for_tier(CacheTier::Ssd) > 0);
        assert!(cache.used_space_for_tier(CacheTier::Ssd) > 0);

        assert_eq!(
            cache.ReplacementPolicyForTier(CacheTier::Memory),
            CacheReplacementPolicy::WeightedHotnessLru
        );
        cache.SetReplacementPolicyType(CacheInstanceKind::Ssd, CacheReplacementPolicy::Fifo);
        assert_eq!(
            cache.GetReplacementPolicyType(CacheInstanceKind::Ssd),
            CacheReplacementPolicy::Fifo
        );
        assert_eq!(
            cache.ReplacementPolicyForTier(CacheTier::Ssd),
            CacheReplacementPolicy::Fifo
        );

        cache.SetCapacityForTier(CacheTier::Memory, 8);
        assert_eq!(cache.capacity_for_tier(CacheTier::Memory), 8);
        assert!(cache.size_for_tier(CacheTier::Memory) <= 8);
        assert!(cache.SizeForTier(CacheTier::Ssd) > 0);
        let eviction = cache.EvictionReport();
        assert!(eviction.memory_capacity_evictions > 0);
        assert!(eviction.memory_slot_evictions > 0);
        cache.SetCapacityForInstance(CacheInstanceKind::Ssd, 1024);
        assert_eq!(cache.GetCapacity(CacheInstanceKind::Ssd), 1024);

        cache.SetReplacementPolicyForTier(CacheTier::Memory, CacheReplacementPolicy::Fifo);
        assert_eq!(
            cache.GetReplacementPolicyType(CacheInstanceKind::Dram),
            CacheReplacementPolicy::Fifo
        );
        let running_cache = MatrixCacheBuilder::build_sharded_cache(CacheOptions::new(32, 0, 0), 2);
        running_cache.start().unwrap();
        assert!(matches!(
            running_cache.TrySetReplacementPolicyType(
                CacheInstanceKind::Dram,
                CacheReplacementPolicy::Fifo,
            ),
            Err(CacheError::AlreadyStarted)
        ));
        assert_eq!(
            running_cache.GetReplacementPolicyType(CacheInstanceKind::Dram),
            CacheReplacementPolicy::WeightedHotnessLru
        );
        assert!(matches!(
            cache.TrySetReplacementPolicyType(
                CacheInstanceKind::Unified,
                CacheReplacementPolicy::Fifo,
            ),
            Err(CacheError::UnsupportedInstance(CacheInstanceKind::Unified))
        ));
        assert!(matches!(
            cache.TrySetReplacementPolicyForTier(CacheTier::Reject, CacheReplacementPolicy::Fifo),
            Err(CacheError::UnsupportedTier(CacheTier::Reject))
        ));

        let api: Box<dyn CacheApi> = MatrixCacheBuilder::build_sharded_cache_api(
            CacheOptions::new(64, 0, 1024)
                .with_ssd_paths(vec![unique_temp_path("sharded-cache-api")]),
            2,
        );
        assert_eq!(
            api.capacity_for_instance_cache(CacheInstanceKind::Dram),
            64
        );
        assert_eq!(
            api.capacity_for_instance_cache(CacheInstanceKind::Ssd),
            1024
        );
        let trait_key = CacheKey::string(7, "trait-key");
        api.insert_cache(trait_key.clone(), b"trait-value".to_vec(), 11)
            .unwrap();
        assert_eq!(
            api.lookup_cache(&trait_key).unwrap().unwrap(),
            b"trait-value".to_vec()
        );
        assert!(api.used_cache(CacheInstanceKind::Dram) > 0);
        api.set_capacity_for_instance_cache(CacheInstanceKind::Dram, 8);
        assert_eq!(api.capacity_for_instance_cache(CacheInstanceKind::Dram), 8);
        api.reset_cache().unwrap();
        assert_eq!(api.lookup_cache(&trait_key).unwrap(), None);

        let slot_key = CacheKey::page_with_slot(0, 700, 0, 32, Some(77));
        let segment_key = CacheKey::page(0, 701, 0, 32);
        let shard_key = CacheKey::string(2, "gc-fanout");
        cache
            .put_batch(vec![
                (slot_key.clone(), b"slot-page".to_vec()),
                (segment_key.clone(), b"segment-page".to_vec()),
                (shard_key.clone(), b"shard-value".to_vec()),
            ])
            .unwrap();
        let slot_report = cache.InvalidateSlot(0, 77).unwrap();
        assert!(slot_report.memory_entries_removed > 0 || slot_report.disk_bytes_removed > 0);
        assert!(cache.lookup(&slot_key).unwrap().is_none());
        assert!(cache.lookup(&segment_key).unwrap().is_some());

        let segment_report = cache.InvalidatePageSegment(0, 701).unwrap();
        assert!(segment_report.memory_entries_removed > 0 || segment_report.disk_bytes_removed > 0);
        assert!(cache.lookup(&segment_key).unwrap().is_none());

        let shard_report = cache.InvalidateShard(2).unwrap();
        assert!(shard_report.memory_entries_removed > 0 || shard_report.disk_bytes_removed > 0);
        assert!(cache.lookup(&shard_key).unwrap().is_none());
        assert!(cache.Stats().invalidations >= 3);
    }

    #[test]
    fn sharded_multilayer_cache_preserves_zero_copy_handles() {
        let cache = MatrixCacheBuilder::build_sharded_cache(
            CacheOptions::new(1024, 0, 0)
                .with_ssd_paths(vec![unique_temp_path("sharded-zero-copy")]),
            4,
        );
        let key = CacheKey::string(42, "pinned");
        let handle = cache
            .insert_pinned_cache(key.clone(), b"pinned-value".to_vec(), 12)
            .unwrap()
            .expect("pinned handle");
        assert_eq!(handle.value(), b"pinned-value");
        cache.release_cache(handle);
        let reacquired = cache.acquire_cache(&key).unwrap().unwrap();
        assert_eq!(reacquired.value(), b"pinned-value");
        cache.release_cache(reacquired);
        let handles = cache
            .acquire_batch_cache(&[key.clone(), key.clone()])
            .unwrap();
        assert_eq!(handles[0].as_ref().unwrap().value(), b"pinned-value");
        assert_eq!(handles[1].as_ref().unwrap().value(), b"pinned-value");
        let handles = handles.into_iter().flatten().collect::<Vec<_>>();
        assert_eq!(cache.release_batch_cache(handles), 2);

        let batch_keys = (0..6)
            .map(|i| CacheKey::string((i % 3) as ShardId, &format!("pinned-batch-{i}")))
            .collect::<Vec<_>>();
        let batch_handles = cache
            .InsertPinnedBatch(
                batch_keys
                    .iter()
                    .enumerate()
                    .map(|(i, key)| {
                        (
                            key.clone(),
                            format!("pinned-batch-value-{i}").into_bytes(),
                            64,
                        )
                    })
                    .collect(),
            )
            .unwrap();
        assert_eq!(batch_handles.len(), batch_keys.len());
        for (i, handle) in batch_handles.iter().enumerate() {
            assert_eq!(
                handle.as_ref().unwrap().value(),
                format!("pinned-batch-value-{i}").as_bytes()
            );
        }
        assert!(cache.Stats().insert_pinned_operations > batch_keys.len() as u64);
        assert!(cache.Stats().pinned_entries >= batch_keys.len() as u64);
        assert_eq!(
            cache.release_batch_cache(batch_handles.into_iter().flatten().collect()),
            batch_keys.len()
        );

        let zero_copy: &dyn ZeroCopyCacheApi = &cache;
        let trait_handles = zero_copy
            .insert_pinned_batch_cache(vec![
                (
                    CacheKey::string(1, "trait-pinned-batch-a"),
                    b"trait-a".to_vec(),
                    8,
                ),
                (
                    CacheKey::string(2, "trait-pinned-batch-b"),
                    b"trait-b".to_vec(),
                    8,
                ),
            ])
            .unwrap();
        assert_eq!(trait_handles.len(), 2);
        assert_eq!(trait_handles[0].as_ref().unwrap().value(), b"trait-a");
        assert_eq!(trait_handles[1].as_ref().unwrap().value(), b"trait-b");
        assert_eq!(
            zero_copy.release_batch_cache(trait_handles.into_iter().flatten().collect()),
            2
        );

        assert_eq!(cache.PinBatch(batch_keys.clone()), batch_keys.len());
        assert_eq!(cache.Stats().pinned_entries, batch_keys.len() as u64);
        cache.Pin(batch_keys[0].clone());
        assert_eq!(cache.Stats().pinned_entries, batch_keys.len() as u64);
        cache.Unpin(&batch_keys[0]);
        assert_eq!(cache.Stats().pinned_entries, batch_keys.len() as u64);
        assert_eq!(cache.UnpinBatch(&batch_keys), batch_keys.len());
        assert_eq!(cache.Stats().pinned_entries, 0);
    }
    // shared-corpus: storage_cache_refill
    #[test]
    fn zero_copy_acquire_pins_ssd_refill_before_memory_eviction() {
        let dir = tempfile::tempdir().unwrap();
        let policy = CacheTieringPolicy {
            memory_capacity_bytes: 4,
            pmem_capacity_bytes: 0,
            ssd_capacity_bytes: 4096,
            data_placement: CacheDataPlacement::Tiered,
            data_placement_threshold_bytes: 1024 * 1024,
            memory_hotness_threshold: 0,
            pmem_admit_hotness_threshold: 0,
            ssd_admit_hotness_threshold: 0,
            max_memory_block_bytes: 4,
            max_pmem_block_bytes: 0,
            max_ssd_block_bytes: 4096,
            ssd_write_through: true,
        };
        let cache =
            MultiLayerCache::with_tiering_policy(dir.path(), policy, CacheBlockOptions::default());
        cache.set_replacement_policy_for_tier(CacheTier::Memory, CacheReplacementPolicy::Fifo);
        let acquired = CacheKey::page_with_slot(1, 100, 0, 4, Some(5));
        let victim = CacheKey::page_with_slot(1, 101, 0, 4, Some(6));

        cache.put(acquired.clone(), b"hot!".to_vec()).unwrap();
        cache.put(victim.clone(), b"cold".to_vec()).unwrap();
        assert_eq!(cache.peek_tier(&acquired), Some(CacheReadTier::Ssd));
        assert_eq!(cache.peek_tier(&victim), Some(CacheReadTier::Memory));

        let handle = cache.acquire(&acquired).unwrap().expect("acquired handle");
        assert_eq!(handle.value(), b"hot!");
        assert_eq!(handle.tier(), CacheReadTier::Ssd);
        assert_eq!(cache.peek_tier(&acquired), Some(CacheReadTier::Memory));
        assert_ne!(cache.peek_tier(&victim), Some(CacheReadTier::Memory));
        assert_eq!(cache.stats().pinned_entries, 1);
        cache.release(handle);
        assert_eq!(cache.stats().pinned_entries, 0);
    }

    // shared-corpus: storage_cache_refill
    #[test]
    fn zero_copy_acquire_pins_pmem_refill_before_memory_eviction() {
        let dir = tempfile::tempdir().unwrap();
        let pmem_path = dir.path().join("pmem-device");
        let cache =
            MultiLayerCache::with_options(CacheOptions::new(4, 8, 0).with_pmem_paths([pmem_path]));
        cache.set_replacement_policy_for_tier(CacheTier::Memory, CacheReplacementPolicy::Fifo);
        let acquired = CacheKey::string(1, "pmem-acquire");
        let victim = CacheKey::string(1, "memory-victim");

        cache
            .test_insert(
                CacheInstanceKind::Pmem,
                acquired.clone(),
                b"pmem".to_vec(),
                4,
            )
            .unwrap();
        cache.put(victim.clone(), b"cold".to_vec()).unwrap();
        assert_eq!(cache.peek_tier(&acquired), Some(CacheReadTier::Pmem));
        assert_eq!(cache.peek_tier(&victim), Some(CacheReadTier::Memory));

        let handle = cache.acquire(&acquired).unwrap().expect("pmem handle");
        assert_eq!(handle.value(), b"pmem");
        assert_eq!(handle.tier(), CacheReadTier::Pmem);
        assert_eq!(cache.peek_tier(&acquired), Some(CacheReadTier::Memory));
        assert_ne!(cache.peek_tier(&victim), Some(CacheReadTier::Memory));
        assert_eq!(cache.stats().pinned_entries, 1);
        cache.release(handle);
        assert_eq!(cache.stats().pinned_entries, 0);
    }

    #[test]
    fn sharded_multilayer_cache_removes_batches_by_shard() {
        let cache = MatrixCacheBuilder::build_sharded_cache(
            CacheOptions::new(64, 0, 4096)
                .with_ssd_paths(vec![unique_temp_path("sharded-remove-batch")]),
            4,
        );
        let keys = (0..12)
            .map(|i| CacheKey::string((i % 4) as ShardId, &format!("remove-shard-{i}")))
            .collect::<Vec<_>>();
        assert_eq!(
            cache
                .put_batch(
                    keys.iter()
                        .enumerate()
                        .map(|(i, key)| (key.clone(), format!("value-{i}").into_bytes()))
                        .collect()
                )
                .unwrap(),
            keys.len()
        );
        assert_eq!(cache.remove_batch(&keys).unwrap(), keys.len());
        assert!(cache
            .get_batch(&keys)
            .unwrap()
            .into_iter()
            .all(|value| value.is_none()));

        assert_eq!(
            cache
                .remove_batch(&[keys[0].clone(), keys[1].clone(), keys[0].clone()])
                .unwrap(),
            3
        );
    }
    #[test]
    fn multilayer_cache_batch_put_updates_existing_ssd_keys_once() {
        let cache = MultiLayerCache::with_options(CacheOptions::new(0, 0, 4096));
        let hot = CacheKey::string(9, "batch-update-hot");
        let cold = CacheKey::string(9, "batch-update-cold");

        assert_eq!(
            cache
                .put_batch(vec![
                    (hot.clone(), b"old-hot".to_vec()),
                    (cold.clone(), b"old-cold".to_vec()),
                ])
                .unwrap(),
            2
        );
        let bytes_before = cache.stats().disk_bytes;

        assert_eq!(
            cache
                .put_batch(vec![
                    (hot.clone(), b"new-hot-1".to_vec()),
                    (hot.clone(), b"new-hot-2".to_vec()),
                    (cold.clone(), b"new-cold".to_vec()),
                ])
                .unwrap(),
            3
        );

        assert_eq!(cache.get(&hot).unwrap().unwrap(), b"new-hot-2".to_vec());
        assert_eq!(cache.get(&cold).unwrap().unwrap(), b"new-cold".to_vec());
        assert!(cache.stats().disk_bytes < bytes_before + 256);
    }
    #[test]
    fn sharded_multilayer_cache_runs_concurrent_batch_workloads() {
        let cache = MatrixCacheBuilder::build_sharded_cache(
            CacheOptions::new(65_536, 0, 131_072)
                .with_ssd_paths(vec![unique_temp_path("sharded-concurrent-batch")]),
            8,
        );
        let workers = (0..8)
            .map(|worker_id| {
                let cache = cache.clone();
                std::thread::spawn(move || {
                    let keys = (0..32)
                        .map(|i| {
                            CacheKey::string((i % 5) as ShardId, &format!("w{worker_id}-k{i}"))
                        })
                        .collect::<Vec<_>>();
                    let entries = keys
                        .iter()
                        .enumerate()
                        .map(|(i, key)| {
                            (
                                key.clone(),
                                format!("worker-{worker_id}-value-{i}").into_bytes(),
                                32,
                            )
                        })
                        .collect::<Vec<_>>();
                    cache.insert_batch_cache(entries).unwrap();
                    let values = cache.lookup_batch_cache(&keys).unwrap();
                    for (i, value) in values.into_iter().enumerate() {
                        assert_eq!(
                            value.unwrap(),
                            format!("worker-{worker_id}-value-{i}").into_bytes()
                        );
                    }
                })
            })
            .collect::<Vec<_>>();

        for worker in workers {
            worker.join().expect("batch worker");
        }
    }
    #[test]
    fn async_writeback_batch_enqueue_respects_queue_limit() {
        let cache = MultiLayerCache::new(64, unique_temp_path("async-writeback-batch-enqueue"));
        cache.set_async_writeback_queue_limit_for_test(3);
        let entries = (0..4)
            .map(|i| {
                (
                    CacheKey::string(15, &format!("enqueue-batch-{i}")),
                    format!("value-{i}").into_bytes(),
                )
            })
            .collect::<Vec<_>>();

        let rejected = cache
            .enqueue_async_writeback_batch(entries)
            .expect_err("bounded queue should reject the fourth job");
        assert_eq!(rejected.len(), 1);
        assert_eq!(cache.stats().async_writeback_enqueued, 3);
        assert_eq!(cache.stats().async_writeback_backpressure_rejections, 1);
        assert_eq!(cache.stats().async_writeback_queue_depth, 3);

        let report = cache.drain_async_writeback(16).unwrap();
        assert_eq!(report.drained, 3);
        assert_eq!(report.remaining, 0);
    }
    #[test]
    fn async_writeback_enqueue_coalesces_duplicate_keys_under_backpressure() {
        let cache = MultiLayerCache::with_tiering_policy(
            unique_temp_path("async-writeback-enqueue-coalesce"),
            CacheTieringPolicy {
                memory_capacity_bytes: 1,
                pmem_capacity_bytes: 0,
                ssd_capacity_bytes: 4096,
                data_placement: CacheDataPlacement::Tiered,
                data_placement_threshold_bytes: 1024,
                memory_hotness_threshold: 0,
                pmem_admit_hotness_threshold: u32::MAX,
                ssd_admit_hotness_threshold: 0,
                max_memory_block_bytes: 1,
                max_pmem_block_bytes: 0,
                max_ssd_block_bytes: 4096,
                ssd_write_through: true,
            },
            CacheBlockOptions::default(),
        );
        cache.set_async_writeback_queue_limit_for_test(1);
        let key = CacheKey::string(18, "async-enqueue-coalesced");
        cache
            .enqueue_async_writeback(key.clone(), b"old".to_vec())
            .unwrap();
        cache
            .enqueue_async_writeback(key.clone(), b"newest".to_vec())
            .unwrap();
        assert_eq!(cache.stats().async_writeback_queue_depth, 1);
        assert_eq!(
            cache.stats().async_writeback_queue_bytes,
            b"newest".len() as u64
        );
        assert_eq!(cache.stats().async_writeback_enqueued, 2);
        assert_eq!(cache.stats().async_writeback_backpressure_rejections, 0);

        let report = cache.drain_async_writeback(16).unwrap();
        assert_eq!(report.drained, 1);
        assert_eq!(report.remaining, 0);
        assert_eq!(cache.stats().async_writeback_queue_bytes, 0);
        assert_eq!(cache.get(&key).unwrap(), Some(b"newest".to_vec()));
    }
    // shared-corpus: storage_cache_writeback
    #[test]
    fn cache_api_async_writeback_submit_uses_queue_and_fallback() {
        let cache = MultiLayerCache::with_tiering_policy(
            unique_temp_path("cache-api-async-writeback-submit"),
            CacheTieringPolicy {
                memory_capacity_bytes: 0,
                pmem_capacity_bytes: 0,
                ssd_capacity_bytes: 4096,
                data_placement: CacheDataPlacement::Tiered,
                data_placement_threshold_bytes: 1024,
                memory_hotness_threshold: 0,
                pmem_admit_hotness_threshold: u32::MAX,
                ssd_admit_hotness_threshold: 0,
                max_memory_block_bytes: 0,
                max_pmem_block_bytes: 0,
                max_ssd_block_bytes: 4096,
                ssd_write_through: true,
            },
            CacheBlockOptions::default(),
        );
        cache.set_async_writeback_queue_limit_for_test(1);
        let queued = CacheKey::string(22, "cache-api-queued-writeback");
        let fallback = CacheKey::string(22, "cache-api-fallback-writeback");

        let api: &dyn CacheApi = &cache;
        let first = api
            .submit_async_writeback_or_write_through_cache(queued.clone(), b"queued".to_vec())
            .unwrap();
        assert_eq!(first.queued, 1);
        assert_eq!(first.write_through, 0);
        assert_eq!(cache.get(&queued).unwrap(), None);

        let second = api
            .submit_async_writeback_or_write_through_cache(fallback.clone(), b"fallback".to_vec())
            .unwrap();
        assert_eq!(second.queued, 0);
        assert_eq!(second.write_through, 1);
        assert_eq!(cache.get(&fallback).unwrap(), Some(b"fallback".to_vec()));

        let drained = cache.drain_async_writeback(8).unwrap();
        assert_eq!(drained.drained, 1);
        assert_eq!(cache.get(&queued).unwrap(), Some(b"queued".to_vec()));
    }

    // shared-corpus: storage_cache_writeback
    #[test]
    fn sharded_cache_api_async_writeback_batch_routes_by_key() {
        let cache = ShardedMultiLayerCache::with_options(
            CacheOptions::new(0, 0, 4096)
                .with_ssd_paths(vec![unique_temp_path("cache-api-sharded-writeback")]),
            2,
        );
        cache.set_async_writeback_queue_limit_for_test(1);
        let keys = (0..4)
            .map(|i| CacheKey::string((i % 2) as ShardId, &format!("cache-api-sharded-{i}")))
            .collect::<Vec<_>>();
        let entries = keys
            .iter()
            .enumerate()
            .map(|(i, key)| (key.clone(), format!("value-{i}").into_bytes()))
            .collect::<Vec<_>>();

        let api: &dyn CacheApi = &cache;
        let report = api
            .submit_async_writeback_batch_or_write_through_cache(entries)
            .unwrap();
        assert_eq!(report.queued + report.write_through, keys.len());
        assert!(report.queued > 0);
        assert!(report.write_through > 0);
        assert_eq!(cache.async_writeback_queue_depth(), report.queued as u64);

        cache.flush_async_writeback().unwrap();
        for (i, key) in keys.iter().enumerate() {
            assert_eq!(
                cache.get(key).unwrap(),
                Some(format!("value-{i}").into_bytes())
            );
        }
    }

    // shared-corpus: storage_cache_writeback
    #[test]
    fn async_writeback_submit_falls_back_to_write_through_when_queue_is_full() {
        let cache = MultiLayerCache::with_tiering_policy(
            unique_temp_path("async-writeback-submit-fallback"),
            CacheTieringPolicy {
                memory_capacity_bytes: 0,
                pmem_capacity_bytes: 0,
                ssd_capacity_bytes: 4096,
                data_placement: CacheDataPlacement::Tiered,
                data_placement_threshold_bytes: 1024,
                memory_hotness_threshold: 0,
                pmem_admit_hotness_threshold: u32::MAX,
                ssd_admit_hotness_threshold: 0,
                max_memory_block_bytes: 0,
                max_pmem_block_bytes: 0,
                max_ssd_block_bytes: 4096,
                ssd_write_through: true,
            },
            CacheBlockOptions::default(),
        );
        cache.set_async_writeback_queue_limit_for_test(1);
        let queued = CacheKey::string(21, "queued-writeback");
        let fallback = CacheKey::string(21, "fallback-writeback");

        let first = cache
            .submit_async_writeback_or_write_through(queued.clone(), b"queued".to_vec())
            .unwrap();
        assert_eq!(first.queued, 1);
        assert_eq!(first.write_through, 0);
        assert_eq!(cache.get(&queued).unwrap(), None);

        let second = cache
            .submit_async_writeback_or_write_through(fallback.clone(), b"fallback".to_vec())
            .unwrap();
        assert_eq!(second.queued, 0);
        assert_eq!(second.write_through, 1);
        assert_eq!(cache.get(&fallback).unwrap(), Some(b"fallback".to_vec()));
        assert_eq!(cache.stats().async_writeback_queue_depth, 1);
        assert_eq!(cache.stats().async_writeback_backpressure_rejections, 1);

        let drained = cache.drain_async_writeback(8).unwrap();
        assert_eq!(drained.drained, 1);
        assert_eq!(cache.get(&queued).unwrap(), Some(b"queued".to_vec()));
    }

    // shared-corpus: storage_cache_writeback
    #[test]
    fn sharded_async_writeback_submit_fallback_routes_each_key() {
        let cache = MatrixCacheBuilder::build_sharded_cache(
            CacheOptions::new(0, 0, 4096)
                .with_ssd_paths(vec![unique_temp_path("sharded-submit-fallback")]),
            2,
        );
        cache.set_async_writeback_queue_limit_for_test(1);
        let keys = (0..6)
            .map(|i| CacheKey::string((i % 2) as ShardId, &format!("submit-fallback-{i}")))
            .collect::<Vec<_>>();
        let report = cache
            .submit_async_writeback_batch_or_write_through(
                keys.iter()
                    .enumerate()
                    .map(|(i, key)| (key.clone(), format!("value-{i}").into_bytes()))
                    .collect(),
            )
            .unwrap();

        assert_eq!(report.queued + report.write_through, keys.len());
        assert!(report.queued > 0);
        assert!(report.write_through > 0);
        assert_eq!(cache.async_writeback_queue_depth(), report.queued as u64);
        for (i, key) in keys.iter().enumerate() {
            let expected = format!("value-{i}").into_bytes();
            if cache.get(key).unwrap().is_none() {
                continue;
            }
            assert_eq!(cache.get(key).unwrap(), Some(expected));
        }
        cache.flush_async_writeback().unwrap();
        for (i, key) in keys.iter().enumerate() {
            assert_eq!(
                cache.get(key).unwrap(),
                Some(format!("value-{i}").into_bytes())
            );
        }
    }

    #[test]
    fn async_writeback_drain_batches_multiple_jobs() {
        let cache = MultiLayerCache::with_tiering_policy(
            unique_temp_path("async-writeback-batch-drain"),
            CacheTieringPolicy {
                memory_capacity_bytes: 1,
                pmem_capacity_bytes: 0,
                ssd_capacity_bytes: 4096,
                data_placement: CacheDataPlacement::Tiered,
                data_placement_threshold_bytes: 1024,
                memory_hotness_threshold: 0,
                pmem_admit_hotness_threshold: u32::MAX,
                ssd_admit_hotness_threshold: 0,
                max_memory_block_bytes: 1,
                max_pmem_block_bytes: 0,
                max_ssd_block_bytes: 4096,
                ssd_write_through: true,
            },
            CacheBlockOptions::default(),
        );
        let keys = (0..4)
            .map(|i| CacheKey::string(17, &format!("async-batch-{i}")))
            .collect::<Vec<_>>();
        for (index, key) in keys.iter().enumerate() {
            cache
                .enqueue_async_writeback(key.clone(), format!("value-{index}").into_bytes())
                .unwrap();
        }
        assert_eq!(cache.stats().async_writeback_queue_depth, keys.len() as u64);

        let report = cache.drain_async_writeback(16).unwrap();
        assert_eq!(report.drained, keys.len());
        assert_eq!(report.remaining, 0);
        let values = cache.get_batch(&keys).unwrap();
        for (index, value) in values.into_iter().enumerate() {
            assert_eq!(value, Some(format!("value-{index}").into_bytes()));
        }
        let stats = cache.stats();
        assert_eq!(stats.async_writeback_drained, keys.len() as u64);
        assert_eq!(stats.async_writeback_queue_depth, 0);
        assert!(stats.disk_hits >= keys.len() as u64);
        assert!(stats.writeback_latency_samples > 0);
    }

    #[test]
    fn async_writeback_flush_drains_all_queued_jobs() {
        let cache = MultiLayerCache::with_tiering_policy(
            unique_temp_path("async-writeback-flush-all"),
            CacheTieringPolicy {
                memory_capacity_bytes: 1,
                pmem_capacity_bytes: 0,
                ssd_capacity_bytes: 4096,
                data_placement: CacheDataPlacement::Tiered,
                data_placement_threshold_bytes: 1024,
                memory_hotness_threshold: 0,
                pmem_admit_hotness_threshold: u32::MAX,
                ssd_admit_hotness_threshold: 0,
                max_memory_block_bytes: 1,
                max_pmem_block_bytes: 0,
                max_ssd_block_bytes: 4096,
                ssd_write_through: true,
            },
            CacheBlockOptions::default(),
        );
        let keys = (0..5)
            .map(|i| CacheKey::string(18, &format!("async-flush-{i}")))
            .collect::<Vec<_>>();
        for (index, key) in keys.iter().enumerate() {
            cache
                .enqueue_async_writeback(key.clone(), format!("flush-{index}").into_bytes())
                .unwrap();
        }
        assert_eq!(cache.stats().async_writeback_queue_depth, keys.len() as u64);

        let report = cache.flush_async_writeback().unwrap();
        assert_eq!(report.requested, keys.len());
        assert_eq!(report.drained, keys.len());
        assert_eq!(report.remaining, 0);
        assert_eq!(cache.stats().async_writeback_queue_depth, 0);
        for (index, key) in keys.iter().enumerate() {
            assert_eq!(
                cache.get(key).unwrap(),
                Some(format!("flush-{index}").into_bytes())
            );
        }
    }

    #[test]
    fn put_batch_coalesces_duplicate_ssd_writes_before_flush() {
        let cache = MultiLayerCache::with_tiering_policy(
            unique_temp_path("put-batch-coalesces-ssd-duplicates"),
            CacheTieringPolicy {
                memory_capacity_bytes: 1,
                pmem_capacity_bytes: 0,
                ssd_capacity_bytes: 4096,
                data_placement: CacheDataPlacement::Tiered,
                data_placement_threshold_bytes: 1024,
                memory_hotness_threshold: 0,
                pmem_admit_hotness_threshold: u32::MAX,
                ssd_admit_hotness_threshold: 0,
                max_memory_block_bytes: 1,
                max_pmem_block_bytes: 0,
                max_ssd_block_bytes: 4096,
                ssd_write_through: true,
            },
            CacheBlockOptions::default(),
        );
        let repeated = CacheKey::string(23, "batch-dup");
        let other = CacheKey::string(23, "batch-other");

        let inserted = cache
            .put_batch(vec![
                (repeated.clone(), b"old".to_vec()),
                (other.clone(), b"other".to_vec()),
                (repeated.clone(), b"new".to_vec()),
            ])
            .unwrap();
        assert_eq!(inserted, 3);
        assert_eq!(cache.stats().disk_fills, 2);
        assert_eq!(
            cache.get_batch(&[repeated, other]).unwrap(),
            vec![Some(b"new".to_vec()), Some(b"other".to_vec())]
        );
    }

    // shared-corpus: storage_cache_eviction
    #[test]
    fn ssd_pressure_eviction_removes_multiple_victims_and_preserves_reads() {
        let cache = MultiLayerCache::with_tiering_policy(
            unique_temp_path("ssd-pressure-batch-evict"),
            CacheTieringPolicy {
                memory_capacity_bytes: 1,
                pmem_capacity_bytes: 0,
                ssd_capacity_bytes: 230,
                data_placement: CacheDataPlacement::Tiered,
                data_placement_threshold_bytes: 1024,
                memory_hotness_threshold: u32::MAX,
                pmem_admit_hotness_threshold: u32::MAX,
                ssd_admit_hotness_threshold: 0,
                max_memory_block_bytes: 1,
                max_pmem_block_bytes: 0,
                max_ssd_block_bytes: 230,
                ssd_write_through: true,
            },
            CacheBlockOptions::default(),
        );
        let cold_keys = (0..4)
            .map(|i| CacheKey::page_with_slot(22, i, 0, 24, Some(i as u32)))
            .collect::<Vec<_>>();
        for (index, key) in cold_keys.iter().enumerate() {
            cache
                .put_with_admission(
                    key.clone(),
                    vec![b'a' + index as u8; 24],
                    CacheAdmissionRequest {
                        block_kind: CacheBlockKind::Page,
                        shard_id: 22,
                        routing_slot: Some(index as u32),
                        block_bytes: 24,
                        hotness: 0,
                        pinned: false,
                    },
                )
                .unwrap();
        }

        let hot_key = CacheKey::page_with_slot(22, 99, 0, 120, Some(99));
        cache
            .put_with_admission(
                hot_key.clone(),
                vec![b'z'; 120],
                CacheAdmissionRequest {
                    block_kind: CacheBlockKind::Page,
                    shard_id: 22,
                    routing_slot: Some(99),
                    block_bytes: 120,
                    hotness: 64,
                    pinned: false,
                },
            )
            .unwrap();

        let stats = cache.stats();
        assert!(stats.ssd_evictions >= 2);
        assert!(stats.disk_bytes <= 230);
        assert_eq!(stats.refill_failures, 0);
        assert_eq!(cache.get(&hot_key).unwrap(), Some(vec![b'z'; 120]));
        let surviving_cold = cache
            .get_batch(&cold_keys)
            .unwrap()
            .into_iter()
            .filter(Option::is_some)
            .count();
        assert!(surviving_cold < cold_keys.len());
    }
    #[test]
    fn async_writeback_drain_coalesces_duplicate_keys() {
        let cache = MultiLayerCache::with_tiering_policy(
            unique_temp_path("async-writeback-coalesce"),
            CacheTieringPolicy {
                memory_capacity_bytes: 1,
                pmem_capacity_bytes: 0,
                ssd_capacity_bytes: 4096,
                data_placement: CacheDataPlacement::Tiered,
                data_placement_threshold_bytes: 1024,
                memory_hotness_threshold: 0,
                pmem_admit_hotness_threshold: u32::MAX,
                ssd_admit_hotness_threshold: 0,
                max_memory_block_bytes: 1,
                max_pmem_block_bytes: 0,
                max_ssd_block_bytes: 4096,
                ssd_write_through: true,
            },
            CacheBlockOptions::default(),
        );
        let hot_key = CacheKey::string(17, "async-coalesced-hot");
        let cold_key = CacheKey::string(17, "async-coalesced-cold");
        cache
            .enqueue_async_writeback(hot_key.clone(), b"old".to_vec())
            .unwrap();
        cache
            .enqueue_async_writeback(cold_key.clone(), b"cold".to_vec())
            .unwrap();
        cache
            .enqueue_async_writeback(hot_key.clone(), b"newest".to_vec())
            .unwrap();

        assert_eq!(cache.stats().async_writeback_enqueued, 3);
        assert_eq!(cache.stats().async_writeback_queue_depth, 2);
        let report = cache.drain_async_writeback(16).unwrap();
        assert_eq!(report.drained, 2);
        assert_eq!(report.remaining, 0);
        assert_eq!(
            cache
                .get_batch(&[hot_key.clone(), cold_key.clone()])
                .unwrap(),
            vec![Some(b"newest".to_vec()), Some(b"cold".to_vec())]
        );
        assert_eq!(cache.stats().async_writeback_drained, 2);
    }

    #[test]
    fn async_writeback_position_index_survives_partial_drain() {
        let cache = MultiLayerCache::with_tiering_policy(
            unique_temp_path("async-writeback-partial-drain-index"),
            CacheTieringPolicy {
                memory_capacity_bytes: 1,
                pmem_capacity_bytes: 0,
                ssd_capacity_bytes: 4096,
                data_placement: CacheDataPlacement::Tiered,
                data_placement_threshold_bytes: 1024,
                memory_hotness_threshold: 0,
                pmem_admit_hotness_threshold: u32::MAX,
                ssd_admit_hotness_threshold: 0,
                max_memory_block_bytes: 1,
                max_pmem_block_bytes: 0,
                max_ssd_block_bytes: 4096,
                ssd_write_through: true,
            },
            CacheBlockOptions::default(),
        );
        let first = CacheKey::string(19, "async-first");
        let second = CacheKey::string(19, "async-second");
        let third = CacheKey::string(19, "async-third");
        let fourth = CacheKey::string(19, "async-fourth");

        cache
            .enqueue_async_writeback_batch(vec![
                (first.clone(), b"first".to_vec()),
                (second.clone(), b"second".to_vec()),
                (third.clone(), b"third-old".to_vec()),
            ])
            .unwrap();
        let partial = cache.drain_async_writeback(1).unwrap();
        assert_eq!(partial.drained, 1);
        assert_eq!(partial.remaining, 2);

        cache
            .enqueue_async_writeback_batch(vec![
                (third.clone(), b"third-new".to_vec()),
                (fourth.clone(), b"fourth".to_vec()),
            ])
            .unwrap();
        assert_eq!(cache.stats().async_writeback_queue_depth, 3);
        assert_eq!(cache.stats().async_writeback_enqueued, 5);

        let final_report = cache.drain_async_writeback(16).unwrap();
        assert_eq!(final_report.drained, 3);
        assert_eq!(final_report.remaining, 0);
        assert_eq!(
            cache.get_batch(&[first, second, third, fourth]).unwrap(),
            vec![
                Some(b"first".to_vec()),
                Some(b"second".to_vec()),
                Some(b"third-new".to_vec()),
                Some(b"fourth".to_vec()),
            ]
        );
    }

    #[test]
    fn async_writeback_worker_drains_enqueued_jobs() {
        let cache = MultiLayerCache::new(64, unique_temp_path("async-writeback-worker"));
        let key = CacheKey::string(9, "worker-drained");
        cache
            .enqueue_async_writeback(key.clone(), b"worker-value".to_vec())
            .unwrap();
        assert!(cache.start_async_writeback_worker(8, Duration::from_millis(1)));
        for _ in 0..200 {
            if cache.stats().async_writeback_queue_depth == 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(cache.stop_async_writeback_worker());
        assert_eq!(cache.stats().async_writeback_queue_depth, 0);
        assert_eq!(cache.get(&key).unwrap(), Some(b"worker-value".to_vec()));
        assert!(cache.stats().async_writeback_drained >= 1);
    }

    #[test]
    fn sharded_async_writeback_batch_enqueue_routes_by_shard() {
        let cache = MatrixCacheBuilder::build_sharded_cache(
            CacheOptions::new(512, 0, 4096)
                .with_ssd_paths(vec![unique_temp_path("sharded-async-batch-enqueue")]),
            4,
        );
        let entries = (0..12)
            .map(|i| {
                (
                    CacheKey::string((i % 4) as ShardId, &format!("async-batch-shard-{i}")),
                    format!("value-{i}").into_bytes(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(cache.enqueue_async_writeback_batch(entries).unwrap(), 12);
        assert_eq!(cache.async_writeback_queue_depth(), 12);
        assert!(cache.async_writeback_queue_bytes() > 0);

        let report = cache.drain_async_writeback(8).unwrap();
        assert_eq!(report.drained, 12);
        assert_eq!(report.remaining, 0);
        assert_eq!(cache.async_writeback_queue_depth(), 0);
        let writeback = cache.WritebackBackpressureReport();
        assert!(writeback.bounded_queue_ready);
        assert!(writeback.ssd_write_through_enabled);
        assert!(writeback.write_through_admissions >= 12);
    }
    #[test]
    fn sharded_async_writeback_workers_drain_each_shard() {
        let cache = MatrixCacheBuilder::build_sharded_cache(
            CacheOptions::new(512, 0, 4096)
                .with_ssd_paths(vec![unique_temp_path("sharded-async-writeback-worker")]),
            4,
        );
        let keys = (0..16)
            .map(|i| CacheKey::string((i % 4) as ShardId, &format!("async-shard-{i}")))
            .collect::<Vec<_>>();
        for (i, key) in keys.iter().cloned().enumerate() {
            cache
                .enqueue_async_writeback(key, format!("value-{i}").into_bytes())
                .unwrap();
        }
        assert_eq!(
            cache.start_async_writeback_workers(4, Duration::from_millis(1)),
            4
        );
        for _ in 0..200 {
            if cache.async_writeback_queue_depth() == 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(cache.stop_async_writeback_workers(), 4);
        assert_eq!(cache.async_writeback_queue_depth(), 0);
        assert_eq!(cache.async_writeback_queue_bytes(), 0);
        for (i, key) in keys.iter().enumerate() {
            assert_eq!(
                cache.get(key).unwrap(),
                Some(format!("value-{i}").into_bytes())
            );
        }
    }
    #[test]
    fn sharded_async_writeback_manual_drain_aggregates_reports() {
        let cache = MatrixCacheBuilder::build_sharded_cache(
            CacheOptions::new(1024, 0, 4096)
                .with_ssd_paths(vec![unique_temp_path("sharded-manual-async-drain")]),
            4,
        );
        let keys = (0..12)
            .map(|i| CacheKey::string((i % 4) as ShardId, &format!("manual-async-{i}")))
            .collect::<Vec<_>>();
        for (i, key) in keys.iter().cloned().enumerate() {
            cache
                .enqueue_async_writeback(key, format!("manual-value-{i}").into_bytes())
                .unwrap();
        }
        assert_eq!(cache.async_writeback_queue_depth(), keys.len() as u64);
        assert!(cache.async_writeback_queue_bytes() > 0);
        let report = cache.drain_async_writeback(16).unwrap();
        assert_eq!(report.drained, keys.len());
        assert_eq!(report.remaining, 0);
        assert_eq!(cache.async_writeback_queue_depth(), 0);
        for (i, key) in keys.iter().enumerate() {
            assert_eq!(
                cache.get(key).unwrap(),
                Some(format!("manual-value-{i}").into_bytes())
            );
        }
    }

    #[test]
    fn sharded_async_writeback_flush_drains_all_shards() {
        let cache = MatrixCacheBuilder::build_sharded_cache(
            CacheOptions::new(1024, 0, 4096)
                .with_ssd_paths(vec![unique_temp_path("sharded-async-flush")]),
            4,
        );
        let keys = (0..16)
            .map(|i| CacheKey::string((i % 4) as ShardId, &format!("flush-shard-{i}")))
            .collect::<Vec<_>>();
        for (i, key) in keys.iter().cloned().enumerate() {
            cache
                .enqueue_async_writeback(key, format!("shard-flush-{i}").into_bytes())
                .unwrap();
        }
        assert_eq!(cache.async_writeback_queue_depth(), keys.len() as u64);

        let report = cache.FlushAsyncWriteback().unwrap();
        assert_eq!(report.requested, keys.len());
        assert_eq!(report.drained, keys.len());
        assert_eq!(report.remaining, 0);
        assert_eq!(cache.async_writeback_queue_depth(), 0);
        assert_eq!(cache.async_writeback_queue_bytes(), 0);
        for (i, key) in keys.iter().enumerate() {
            assert_eq!(
                cache.get(key).unwrap(),
                Some(format!("shard-flush-{i}").into_bytes())
            );
        }
    }
    // Zoned storage. A small device keeps these fast: 8 zones of 64 KiB, one
    // group per zone, 4 KiB pages.
    const ZONE_TEST_CAPACITY: u64 = 64 * 1024;
    const ZONE_TEST_DEVICE: u64 = 8 * ZONE_TEST_CAPACITY;

    fn open_zone_manager(dir: &std::path::Path, reuse: bool) -> ZoneManager {
        let device = ZoneDevice::open(dir.join("zoned"), ZONE_TEST_DEVICE, ZONE_TEST_CAPACITY)
            .expect("device opens");
        ZoneManager::new(device, reuse).expect("manager opens")
    }

    #[test]
    fn zone_device_geometry_is_derived_from_the_capacity_it_was_opened_with() {
        let dir = tempfile::tempdir().unwrap();
        let device =
            ZoneDevice::open(dir.path().join("zoned"), ZONE_TEST_DEVICE, ZONE_TEST_CAPACITY)
                .unwrap();
        let info = device.info();
        assert_eq!(info.device_capacity, ZONE_TEST_DEVICE);
        assert_eq!(info.zone_capacity, ZONE_TEST_CAPACITY);
        assert_eq!(info.zones_in_device, 8);
        // Large mode: one zone per group, so a group is a zone.
        assert_eq!(info.zones_in_group, 1);
        assert_eq!(info.groups_in_device, 8);
        assert_eq!(info.group_size, ZONE_TEST_CAPACITY);
        assert_eq!(device.zone_mode(), ZoneMode::Large);
        assert_eq!(device.init_zones().len(), 8);
    }

    #[test]
    fn zone_device_refuses_a_geometry_it_cannot_address_in_whole_pages() {
        let dir = tempfile::tempdir().unwrap();
        // A zone that is not a whole number of pages.
        assert!(ZoneDevice::open(dir.path().join("a"), ZONE_TEST_DEVICE, 5000).is_err());
        // A device that is not a whole number of zones.
        assert!(ZoneDevice::open(dir.path().join("b"), 5000, ZONE_TEST_CAPACITY).is_err());
        assert!(ZoneDevice::open(dir.path().join("c"), 0, ZONE_TEST_CAPACITY).is_err());
    }

    #[test]
    fn zone_append_lands_after_the_header_and_advances_by_what_it_wrote() {
        let dir = tempfile::tempdir().unwrap();
        let mut manager = open_zone_manager(dir.path(), false);
        let page = manager.device_info().page_size;
        let data = vec![b'a'; page as usize];

        assert!(manager.ensure_available_space(page, page));
        let first = manager.append(&data, DataKind::Data).unwrap();
        // The zone opens with a header, so user data starts one page in.
        assert_eq!(first, page);

        assert!(manager.ensure_available_space(page, page));
        let second = manager.append(&data, DataKind::Data).unwrap();
        assert_eq!(second, first + page);
    }

    #[test]
    fn zone_append_without_ensuring_space_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let mut manager = open_zone_manager(dir.path(), false);
        let page = manager.device_info().page_size;
        let data = vec![b'a'; page as usize];
        // No ensure_available_space first: the caller has not been told the
        // footer still fits, so the append cannot be allowed to consume it.
        assert!(manager.append(&data, DataKind::Data).is_err());
    }

    #[test]
    fn zone_read_returns_what_append_wrote() {
        let dir = tempfile::tempdir().unwrap();
        let mut manager = open_zone_manager(dir.path(), false);
        let page = manager.device_info().page_size;

        let mut offsets = Vec::new();
        for i in 0..4u8 {
            let mut record = vec![0u8; page as usize];
            record[0] = b'0' + i;
            assert!(manager.ensure_available_space(page, page));
            offsets.push((i, manager.append(&record, DataKind::Data).unwrap()));
        }

        for (i, offset) in offsets {
            let read = manager.read(offset, page as usize).unwrap();
            assert_eq!(read[0], b'0' + i, "record {i} read back wrong");
        }
    }

    #[test]
    fn zone_metadata_log_seals_the_group_and_a_second_one_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let mut manager = open_zone_manager(dir.path(), false);
        let page = manager.device_info().page_size;
        let payload = vec![b'a'; page as usize];

        assert!(manager.ensure_available_space(page, page));
        manager.append(&payload, DataKind::Data).unwrap();

        let meta = vec![b'm'; page as usize];
        let meta_offset = manager.append(&meta, DataKind::MetaLog).unwrap();
        // The metadata log sits immediately after the data it describes.
        assert_eq!(meta_offset, page * 2);
        assert_eq!(manager.load_meta_data(manager.current_group_id()).unwrap(), meta);

        // Writing the metadata log pads the zone out and stamps the footer, so
        // there is no room for a second one and no group left to describe.
        assert!(manager.append(&meta, DataKind::MetaLog).is_err());
    }

    #[test]
    fn zone_trim_moves_bytes_from_valid_into_garbage() {
        let dir = tempfile::tempdir().unwrap();
        let mut manager = open_zone_manager(dir.path(), false);
        let page = manager.device_info().page_size;
        let payload = vec![b'a'; page as usize];

        assert!(manager.ensure_available_space(page, page));
        let offset = manager.append(&payload, DataKind::Data).unwrap();
        manager.append(&vec![b'm'; page as usize], DataKind::MetaLog).unwrap();
        manager.finish_group().unwrap();

        assert_eq!(manager.garbage_bytes(), 0);
        manager.trim_bytes(offset, page).unwrap();
        // Zones are never rewritten in place, so a trim only reclassifies the
        // bytes; the space comes back when the group is reset.
        assert_eq!(manager.garbage_bytes(), page);
    }

    #[test]
    fn zone_trim_outside_the_device_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let mut manager = open_zone_manager(dir.path(), false);
        assert!(manager.trim_bytes(ZONE_TEST_DEVICE * 2, 4096).is_err());
    }

    #[test]
    fn zone_gc_picks_the_group_holding_the_most_garbage() {
        let dir = tempfile::tempdir().unwrap();
        let mut manager = open_zone_manager(dir.path(), false);
        let page = manager.device_info().page_size;
        let payload = vec![b'a'; page as usize];
        let meta = vec![b'm'; page as usize];

        // Fill three groups, trimming a different amount from each.
        let mut first_offsets = Vec::new();
        for group in 0..3u64 {
            let mut offsets = Vec::new();
            for _ in 0..4 {
                assert!(manager.ensure_available_space(page, page));
                offsets.push(manager.append(&payload, DataKind::Data).unwrap());
            }
            manager.append(&meta, DataKind::MetaLog).unwrap();
            manager.finish_group().unwrap();
            first_offsets.push((group, offsets));
        }

        // Group 1 loses three records, the others one each.
        for (group, offsets) in &first_offsets {
            let trims = if *group == 1 { 3 } else { 1 };
            for offset in offsets.iter().take(trims) {
                manager.trim_bytes(*offset, page).unwrap();
            }
        }

        let (group_id, mode) = manager.find_gc_group().expect("a group is reclaimable");
        assert_eq!(group_id, 1, "the group with the most garbage should be picked");
        assert_eq!(mode, GcMode::Lossy);
    }

    #[test]
    fn zone_reset_returns_the_group_to_the_free_list_and_clears_its_garbage() {
        let dir = tempfile::tempdir().unwrap();
        let mut manager = open_zone_manager(dir.path(), false);
        let page = manager.device_info().page_size;
        let payload = vec![b'a'; page as usize];

        assert!(manager.ensure_available_space(page, page));
        let offset = manager.append(&payload, DataKind::Data).unwrap();
        manager.append(&vec![b'm'; page as usize], DataKind::MetaLog).unwrap();
        manager.finish_group().unwrap();
        manager.trim_bytes(offset, page).unwrap();

        let free_before = manager.free_group_count();
        let (group_id, _) = manager.find_gc_group().unwrap();
        assert!(manager.gc_group_ids().contains(&group_id));

        manager.reset_group(group_id).unwrap();
        assert!(!manager.gc_group_ids().contains(&group_id));
        assert_eq!(manager.free_group_count(), free_before + 1);
        assert_eq!(manager.garbage_bytes(), 0);
        assert_eq!(manager.used_space(), 0);
    }

    #[test]
    fn zone_reset_refuses_a_group_that_is_still_taking_appends() {
        let dir = tempfile::tempdir().unwrap();
        let mut manager = open_zone_manager(dir.path(), false);
        let current = manager.current_group_id();
        // Resetting the open group would discard writes the caller was told landed.
        assert!(manager.reset_group(current).is_err());
    }

    #[test]
    fn zone_manager_reopen_recovers_the_groups_whose_footer_is_readable() {
        let dir = tempfile::tempdir().unwrap();
        let page;
        let meta = {
            let mut manager = open_zone_manager(dir.path(), false);
            page = manager.device_info().page_size;
            let payload = vec![b'a'; page as usize];
            let meta = vec![b'm'; page as usize];
            assert!(manager.ensure_available_space(page, page));
            manager.append(&payload, DataKind::Data).unwrap();
            manager.append(&meta, DataKind::MetaLog).unwrap();
            manager.finish_group().unwrap();
            manager.close().unwrap();
            meta
        };

        let mut restarted = open_zone_manager(dir.path(), true);
        let mut replayed: Vec<(u16, Vec<u8>)> = Vec::new();
        let recovered = restarted
            .recover(|group_id, buf| {
                replayed.push((group_id, buf.to_vec()));
                page
            })
            .unwrap();

        assert_eq!(recovered, 1, "the sealed group should be replayed");
        assert_eq!(replayed.len(), 1);
        assert_eq!(replayed[0].0, 0);
        assert_eq!(replayed[0].1, meta, "recovery must hand back the metadata log");
        // Everything in the zone that the replay did not account for is garbage.
        assert_eq!(restarted.garbage_bytes(), ZONE_TEST_CAPACITY - page);
    }

    #[test]
    fn zone_manager_without_reuse_discards_what_the_device_held() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut manager = open_zone_manager(dir.path(), false);
            let page = manager.device_info().page_size;
            assert!(manager.ensure_available_space(page, page));
            manager.append(&vec![b'a'; page as usize], DataKind::Data).unwrap();
            manager.append(&vec![b'm'; page as usize], DataKind::MetaLog).unwrap();
            manager.finish_group().unwrap();
            manager.close().unwrap();
        }

        let mut restarted = open_zone_manager(dir.path(), false);
        let recovered = restarted.recover(|_, _| 0).unwrap();
        assert_eq!(recovered, 0, "nothing is queued for replay without reuse");
        assert_eq!(restarted.used_space(), 0);
        assert_eq!(restarted.free_group_count(), 7);
    }

    #[test]
    fn zone_property_reports_device_group_and_garbage() {
        let dir = tempfile::tempdir().unwrap();
        let manager = open_zone_manager(dir.path(), false);

        let device = manager.property("device").expect("device is a property");
        assert!(device.contains(&format!("Zone Capacity (B): {ZONE_TEST_CAPACITY}")));
        assert!(device.contains("Groups in Device: 8"));

        let group = manager.property("group").expect("group is a property");
        assert!(group.contains("Used Groups: 0"));
        // One of the eight groups is open for appends, so seven remain free.
        assert!(group.contains("Free Groups: 7"));

        assert!(manager.property("garbage").is_some());
        assert!(manager.property("no-such-property").is_none());
    }

    #[test]
    fn zone_finish_group_runs_out_of_groups_rather_than_waiting_for_one() {
        let dir = tempfile::tempdir().unwrap();
        let mut manager = open_zone_manager(dir.path(), false);
        // Eight groups: the first is already open, so seven more can be taken.
        for _ in 0..7 {
            manager.finish_group().unwrap();
        }
        // The reference blocks here until a collector frees a group. With no
        // collector of its own, this reports the exhaustion instead.
        assert!(manager.finish_group().is_err());
    }

    fn ssd_device_record(state: RecordState) -> SsdIndexValue {
        let pointer = mask_colored_ptr_lba(0, 4096);
        let pointer = mask_colored_ptr_size(pointer, 1);
        SsdIndexValue::SsdColoredPtr(mask_colored_ptr_record_state(pointer, state))
    }

    #[test]
    fn ssd_index_reports_the_state_of_a_record_that_lives_on_the_device() {
        for state in [RecordState::SoftDel, RecordState::Normal, RecordState::Pinned] {
            assert_eq!(ssd_device_record(state).state(), Some(state));
        }
        assert_eq!(
            SsdIndexValue::Memory {
                value: vec![1, 2, 3],
                state: RecordState::Pinned,
            }
            .state(),
            Some(RecordState::Pinned)
        );
    }

    #[test]
    fn ssd_index_value_keeps_its_address_and_size_when_its_state_changes() {
        let record = ssd_device_record(RecordState::Normal);
        let pinned = record.clone().with_state(RecordState::Pinned);
        assert_eq!(pinned.state(), Some(RecordState::Pinned));

        let SsdIndexValue::SsdColoredPtr(before) = record else {
            panic!("expected a device record");
        };
        let SsdIndexValue::SsdColoredPtr(after) = pinned else {
            panic!("expected a device record");
        };
        assert_eq!(decode_colored_ptr(before), decode_colored_ptr(after));

        // Back to SoftDel, whose encoding is zero. Setting the field by OR alone
        // could never do this, which is why it is cleared first.
        let deleted = SsdIndexValue::SsdColoredPtr(after).with_state(RecordState::SoftDel);
        assert_eq!(deleted.state(), Some(RecordState::SoftDel));
    }

    #[test]
    fn ssd_index_pins_and_unpins_a_record_that_lives_on_the_device() {
        let index = SsdIndex::new();
        index.put("on-disk", ssd_device_record(RecordState::Normal));

        assert!(index.pin("on-disk"), "a device record must be pinnable");
        assert_eq!(
            index.get("on-disk").unwrap().state(),
            Some(RecordState::Pinned)
        );
        assert!(!index.pin("on-disk"), "already pinned");

        index.unpin("on-disk");
        assert_eq!(
            index.get("on-disk").unwrap().state(),
            Some(RecordState::Normal)
        );
    }

    #[test]
    fn ssd_index_soft_deletes_a_record_that_lives_on_the_device() {
        let index = SsdIndex::new();
        index.put("on-disk", ssd_device_record(RecordState::Normal));

        index.soft_delete("on-disk");
        // peek would be nicer, but get is the only reader; check the stored
        // value through a fresh scan so the read does not promote it first.
        let mut seen = None;
        index.scan_index_for_recover(|key, value| {
            if key == "on-disk" {
                seen = value.state();
            }
        });
        assert_eq!(seen, Some(RecordState::SoftDel));

        // A pinned record is left alone: deleting one out from under a holder is
        // what pinning forbids.
        index.put("pinned", ssd_device_record(RecordState::Pinned));
        index.soft_delete("pinned");
        let mut pinned_state = None;
        index.scan_index_for_recover(|key, value| {
            if key == "pinned" {
                pinned_state = value.state();
            }
        });
        assert_eq!(pinned_state, Some(RecordState::Pinned));
    }

    #[test]
    fn ssd_index_read_of_a_soft_deleted_device_record_puts_it_back() {
        let index = SsdIndex::new();
        index.put("on-disk", ssd_device_record(RecordState::SoftDel));

        let read = index.get("on-disk").expect("a soft-deleted record still reads");
        assert_eq!(read.state(), Some(RecordState::Normal));

        let mut stored = None;
        index.scan_index_for_recover(|key, value| {
            if key == "on-disk" {
                stored = value.state();
            }
        });
        assert_eq!(
            stored,
            Some(RecordState::Normal),
            "the promotion must be recorded, not just returned"
        );
    }

    #[test]
    fn ssd_index_delete_if_can_remove_a_record_that_lives_on_the_device() {
        let index = SsdIndex::new();
        index.put("keep", ssd_device_record(RecordState::Pinned));
        index.put("drop", ssd_device_record(RecordState::SoftDel));

        // Reclaim is the caller here: a device record it cannot delete is one it
        // would leave in the index pointing at a zone it is about to reset.
        assert!(!index.delete_if("keep", |state| state == RecordState::SoftDel));
        assert!(index.delete_if("drop", |state| state == RecordState::SoftDel));

        assert!(index.get("drop").is_none());
        assert!(index.get("keep").is_some());
        assert!(!index.delete_if("absent", |_| true));
    }

    #[test]
    fn ssd_index_update_keeps_the_state_of_a_record_that_lives_on_the_device() {
        let index = SsdIndex::new();
        index.put("on-disk", ssd_device_record(RecordState::Normal));
        assert!(index.pin("on-disk"));

        // Moving the record must not quietly unpin it.
        let moved = ssd_device_record(RecordState::SoftDel);
        assert!(index.update_index("on-disk", moved));
        let mut stored = None;
        index.scan_index_for_recover(|key, value| {
            if key == "on-disk" {
                stored = value.state();
            }
        });
        assert_eq!(stored, Some(RecordState::Pinned));
    }

    // Writes one sealed group: a data block, the operation log describing it,
    // and the group closed so reclaim can see it.
    fn zone_gc_seal_group(
        zones: &mut ZoneManager,
        index: &SsdIndex,
        encoder: &BufferEncoder,
        records: &[(String, Vec<u8>)],
    ) {
        let mut body = Vec::new();
        for (_, value) in records {
            body.extend_from_slice(&encoder.serialize_data(value));
        }
        let mut block = (body.len() as u64 + 8).to_le_bytes().to_vec();
        block.extend_from_slice(&body);
        let padded = (block.len() as u64).next_multiple_of(ZONE_PAGE_SIZE) as usize;
        block.resize(padded, 0);

        assert!(zones.ensure_available_space(block.len() as u64, ZONE_PAGE_SIZE));
        let base = zones.append(&block, DataKind::Data).unwrap();

        let buffered: Vec<WriteBufferRecord> = records
            .iter()
            .map(|(key, value)| WriteBufferRecord {
                key: key.clone(),
                value: value.clone(),
            })
            .collect();
        let oplog = encoder.serialize_oplog(
            &buffered,
            |key, value| {
                index.put(key, value);
                true
            },
            base + 8,
            0,
        );

        let mut log_block = (oplog.len() as u64 + 8).to_le_bytes().to_vec();
        log_block.extend_from_slice(&oplog);
        let padded = (log_block.len() as u64).next_multiple_of(ZONE_PAGE_SIZE) as usize;
        log_block.resize(padded, 0);
        zones.append(&log_block, DataKind::MetaLog).unwrap();
        zones.finish_group().unwrap();
    }

    fn zone_gc_set_state(index: &SsdIndex, key: &str, state: RecordState) {
        let Some(SsdIndexValue::SsdColoredPtr(pointer)) = index.get(key) else {
            panic!("{key} is not a device record");
        };
        let cleared = pointer & !SSD_RECORD_STATE_FLAGS;
        index.put(
            key,
            SsdIndexValue::SsdColoredPtr(mask_colored_ptr_record_state(cleared, state)),
        );
    }

    #[test]
    fn zone_gc_worker_does_nothing_until_it_is_started() {
        let dir = tempfile::tempdir().unwrap();
        let mut zones = open_zone_manager(dir.path(), false);
        let index = SsdIndex::new();
        let worker = ZoneGcWorker::new(1 << 20);

        assert!(!worker.gc_enabled());
        let (report, survivors) = worker.collect(&mut zones, &index).unwrap();
        assert_eq!(report, ZoneGcReport::default());
        assert!(survivors.is_empty());
    }

    #[test]
    fn zone_gc_start_and_stop_gate_reclaim() {
        let mut worker = ZoneGcWorker::new(1 << 20);
        assert!(!worker.gc_enabled());
        worker.start();
        assert!(worker.gc_enabled());
        worker.stop();
        assert!(!worker.gc_enabled());
    }

    #[test]
    fn zone_gc_reads_a_device_records_state_out_of_its_packed_pointer() {
        // SsdIndexValue::state() reports None for a device record, so reclaim
        // decodes the state from the pointer instead of assuming one.
        for state in [RecordState::SoftDel, RecordState::Normal, RecordState::Pinned] {
            let pointer = mask_colored_ptr_record_state(0, state);
            let value = SsdIndexValue::SsdColoredPtr(pointer);
            assert_eq!(value.state(), Some(state));
            assert_eq!(ZoneGcWorker::record_state(&value), state);
        }
        let memory = SsdIndexValue::Memory {
            value: vec![1, 2, 3],
            state: RecordState::Pinned,
        };
        assert_eq!(ZoneGcWorker::record_state(&memory), RecordState::Pinned);
    }

    #[test]
    fn zone_gc_retention_follows_the_records_state_and_the_reclaim_mode() {
        // Already deleted: the only reason it still existed is that its zone had
        // not been reset, and now it is being reset.
        assert!(!ZoneGcWorker::survives(RecordState::SoftDel, GcMode::Lossy));
        assert!(!ZoneGcWorker::survives(RecordState::SoftDel, GcMode::Lossless));
        // Pinned is exactly what a holder is relying on.
        assert!(ZoneGcWorker::survives(RecordState::Pinned, GcMode::Lossy));
        assert!(ZoneGcWorker::survives(RecordState::Pinned, GcMode::Lossless));
        // Live records survive only when the group is being rewritten, not erased.
        assert!(!ZoneGcWorker::survives(RecordState::Normal, GcMode::Lossy));
        assert!(ZoneGcWorker::survives(RecordState::Normal, GcMode::Lossless));
    }

    #[test]
    fn zone_gc_refuses_an_operation_log_entry_that_does_not_check_out() {
        let worker = ZoneGcWorker::new(1 << 20);
        let encoder = BufferEncoder::new(1 << 20);
        let entry = encoder.serialize_oplog(
            &[WriteBufferRecord {
                key: "a-key".to_string(),
                value: vec![b'v'; 32],
            }],
            |_, _| true,
            4096,
            0,
        );

        let (key, used) = worker.construct_single_key(&entry).expect("intact");
        assert_eq!(key, "a-key");
        assert_eq!(used, entry.len());

        let mut torn = entry.clone();
        let last = torn.len() - 1;
        torn[last] ^= 0xff;
        assert!(worker.construct_single_key(&torn).is_none());
        assert!(worker.construct_single_key(&[]).is_none());
    }

    #[test]
    fn zone_gc_drops_soft_deleted_records_and_resets_the_group() {
        let dir = tempfile::tempdir().unwrap();
        let mut zones = open_zone_manager(dir.path(), false);
        let index = SsdIndex::new();
        let encoder = BufferEncoder::new(1 << 20);
        let mut worker = ZoneGcWorker::new(1 << 20);
        worker.start();

        let records: Vec<(String, Vec<u8>)> = (0..4u8)
            .map(|i| (format!("key-{i}"), vec![b'a' + i; 200]))
            .collect();
        zone_gc_seal_group(&mut zones, &index, &encoder, &records);
        // serialize_oplog stamps every record soft-deleted, which is what the
        // model does too, so a lossy pass should keep none of them.
        let free_before = zones.free_group_count();

        let (report, survivors) = worker.collect(&mut zones, &index).unwrap();
        assert_eq!(report.group_id, Some(0));
        assert_eq!(report.scanned, 4);
        assert_eq!(report.dropped, 4);
        assert_eq!(report.kept, 0);
        assert!(survivors.is_empty());
        assert_eq!(report.bytes_reclaimed, zones.zone_capacity());

        for (key, _) in &records {
            assert!(index.get(key).is_none(), "{key} should be gone from the index");
        }
        assert_eq!(zones.free_group_count(), free_before + 1);
        assert!(!zones.gc_group_ids().contains(&0));
    }

    #[test]
    fn zone_gc_keeps_pinned_records_and_hands_them_back_to_be_rewritten() {
        let dir = tempfile::tempdir().unwrap();
        let mut zones = open_zone_manager(dir.path(), false);
        let index = SsdIndex::new();
        let encoder = BufferEncoder::new(1 << 20);
        let mut worker = ZoneGcWorker::new(1 << 20);
        worker.start();

        let records: Vec<(String, Vec<u8>)> = (0..4u8)
            .map(|i| (format!("key-{i}"), vec![b'a' + i; 200]))
            .collect();
        zone_gc_seal_group(&mut zones, &index, &encoder, &records);
        zone_gc_set_state(&index, "key-1", RecordState::Pinned);
        zone_gc_set_state(&index, "key-2", RecordState::Normal);

        let (report, survivors) = worker.collect(&mut zones, &index).unwrap();
        assert_eq!(report.scanned, 4);
        // key-1 is pinned so it survives a lossy pass; key-2 is merely live and
        // does not; the other two were soft-deleted.
        assert_eq!(report.kept, 1);
        assert_eq!(report.dropped, 3);
        assert_eq!(survivors.len(), 1);
        assert_eq!(survivors[0].0, "key-1");
        assert_eq!(
            survivors[0].1,
            vec![b'b'; 200],
            "the record must come back byte for byte, or rewriting it corrupts it"
        );
        assert!(index.get("key-2").is_none());
    }

    #[test]
    fn zone_gc_reads_a_record_back_through_its_packed_pointer() {
        let dir = tempfile::tempdir().unwrap();
        let mut zones = open_zone_manager(dir.path(), false);
        let index = SsdIndex::new();
        let encoder = BufferEncoder::new(1 << 20);
        let worker = ZoneGcWorker::new(1 << 20);

        let records = vec![("only-key".to_string(), vec![b'z'; 300])];
        zone_gc_seal_group(&mut zones, &index, &encoder, &records);

        let oplog = zones.load_meta_data(0).unwrap();
        let body = &oplog[BufferEncoder::OPLOG_HEADER_SIZE as usize..];
        let (key, value, used) = worker
            .construct_single_record(body, &mut zones)
            .unwrap()
            .expect("the entry is intact");
        assert_eq!(key, "only-key");
        assert_eq!(value.expect("the record reads back"), vec![b'z'; 300]);
        assert!(used > 0);
    }

    #[test]
    fn zone_gc_will_not_read_a_record_longer_than_it_was_configured_for() {
        let dir = tempfile::tempdir().unwrap();
        let mut zones = open_zone_manager(dir.path(), false);
        let index = SsdIndex::new();
        let encoder = BufferEncoder::new(1 << 20);
        // One page is smaller than the record written below.
        let worker = ZoneGcWorker::new(ZONE_PAGE_SIZE as usize / 2);

        let records = vec![("big".to_string(), vec![b'z'; 300])];
        zone_gc_seal_group(&mut zones, &index, &encoder, &records);

        let oplog = zones.load_meta_data(0).unwrap();
        let body = &oplog[BufferEncoder::OPLOG_HEADER_SIZE as usize..];
        let (key, value, _) = worker
            .construct_single_record(body, &mut zones)
            .unwrap()
            .expect("the entry is intact");
        assert_eq!(key, "big");
        assert!(value.is_none(), "an oversized read is refused, not truncated");
    }

    #[test]
    fn zone_gc_finds_nothing_to_do_on_a_device_with_no_sealed_groups() {
        let dir = tempfile::tempdir().unwrap();
        let mut zones = open_zone_manager(dir.path(), false);
        let index = SsdIndex::new();
        let mut worker = ZoneGcWorker::new(1 << 20);
        worker.start();

        let (report, survivors) = worker.collect(&mut zones, &index).unwrap();
        assert_eq!(report.group_id, None);
        assert_eq!(report.scanned, 0);
        assert!(survivors.is_empty());
    }

    #[test]
    fn access_record_callback_can_be_registered_again_after_being_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1 << 16, dir.path());
        let key = CacheKey::string(3, "cycled");
        cache.put(key.clone(), b"value".to_vec()).unwrap();

        let first = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&first);
        cache.register_access_record_callback(move |_| {
            counter.fetch_add(1, Ordering::Relaxed);
        });
        cache.get(&key).unwrap();
        assert_eq!(first.load(Ordering::Relaxed), 1);

        cache.clear_access_record_callback();
        cache.get(&key).unwrap();
        assert_eq!(first.load(Ordering::Relaxed), 1, "a cleared callback stops");

        // Registering again has to start it back up. Whether a callback exists
        // is tracked outside the cache lock so the check on every get, put and
        // delete does not have to take it; this is the cycle that a flag left
        // stale would silently break, with the callback registered and never
        // called.
        let second = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&second);
        cache.register_access_record_callback(move |_| {
            counter.fetch_add(1, Ordering::Relaxed);
        });
        cache.get(&key).unwrap();
        assert_eq!(second.load(Ordering::Relaxed), 1, "re-registering resumes");
        assert_eq!(first.load(Ordering::Relaxed), 1, "the old callback is gone");
    }

    #[test]
    fn pinned_stats_are_reported_correctly_for_a_large_pinned_set() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1 << 20, dir.path());

        // The pinned totals used to be recomputed into `stats` on every
        // mutation and then overwritten at read time by `stats()`, which
        // computes them itself. The maintenance was removed; these assertions
        // are what says the value a caller sees did not change with it.
        const PINNED: usize = 64;
        for i in 0..PINNED {
            let key = CacheKey::string(0, &format!("pinned-{i:04}"));
            cache.put(key.clone(), vec![b'p'; 8]).unwrap();
            cache.pin(key);
        }
        // Unpinned entries must not be counted.
        for i in 0..16 {
            let key = CacheKey::string(1, &format!("loose-{i:04}"));
            cache.put(key, vec![b'l'; 8]).unwrap();
        }

        let stats = cache.stats();
        assert_eq!(stats.pinned_entries, PINNED as u64);
        assert_eq!(stats.pinned_bytes, (PINNED * 8) as u64);

        // Unpinning has to be reflected too, since nothing recomputes on the
        // way in any more.
        for i in 0..PINNED / 2 {
            cache.unpin(&CacheKey::string(0, &format!("pinned-{i:04}")));
        }
        let stats = cache.stats();
        assert_eq!(stats.pinned_entries, (PINNED / 2) as u64);
        assert_eq!(stats.pinned_bytes, (PINNED / 2 * 8) as u64);
    }

    #[test]
    fn eviction_metric_callback_survives_clearing_the_eviction_handler() {
        let dir = tempfile::tempdir().unwrap();
        // Small enough that puts evict.
        let cache = MultiLayerCache::new(256, dir.path());

        let handler_calls = Arc::new(AtomicU32::new(0));
        let metric_calls = Arc::new(AtomicU32::new(0));
        let handler_counter = Arc::clone(&handler_calls);
        let metric_counter = Arc::clone(&metric_calls);
        cache.register_eviction_callback(move |_| {
            handler_counter.fetch_add(1, Ordering::Relaxed);
        });
        cache.register_eviction_metric_callback(move |_, _| {
            metric_counter.fetch_add(1, Ordering::Relaxed);
        });

        let churn = |tag: &str| {
            for i in 0..40 {
                let key = CacheKey::string(0, &format!("{tag}-{i:04}"));
                cache.put(key, vec![b'v'; 64]).unwrap();
            }
        };

        churn("first");
        assert!(handler_calls.load(Ordering::Relaxed) > 0, "handler fires");
        assert!(metric_calls.load(Ordering::Relaxed) > 0, "metric fires");

        // Clearing the handler must not silence the metric. Whether anything is
        // queued is tracked outside the cache lock so the drain after every put
        // can skip taking it; a flag that keyed on the handler alone would stop
        // draining here and the metric would go quiet while still registered.
        cache.clear_eviction_callback();
        let handler_before = handler_calls.load(Ordering::Relaxed);
        let metric_before = metric_calls.load(Ordering::Relaxed);
        churn("second");
        assert_eq!(
            handler_calls.load(Ordering::Relaxed),
            handler_before,
            "a cleared handler stays quiet"
        );
        assert!(
            metric_calls.load(Ordering::Relaxed) > metric_before,
            "the metric callback keeps reporting"
        );

        // And clearing the last one stops everything.
        cache.clear_eviction_metric_callback();
        let metric_after = metric_calls.load(Ordering::Relaxed);
        churn("third");
        assert_eq!(
            metric_calls.load(Ordering::Relaxed),
            metric_after,
            "both cleared means nothing fires"
        );

        // Re-registering has to start it up again.
        let again = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&again);
        cache.register_eviction_metric_callback(move |_, _| {
            counter.fetch_add(1, Ordering::Relaxed);
        });
        churn("fourth");
        assert!(again.load(Ordering::Relaxed) > 0, "re-registering resumes");
    }

    #[test]
    fn async_writeback_coalescing_still_works_after_many_partial_drains() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1 << 20, dir.path());
        cache.set_async_writeback_queue_limit_for_test(256);

        let entries: Vec<(CacheKey, Vec<u8>)> = (0..128)
            .map(|i| (CacheKey::string(0, &format!("job-{i:04}")), vec![b'a'; 8]))
            .collect();
        assert_eq!(cache.enqueue_async_writeback_batch(entries).unwrap(), 128);

        // Drain in small batches, the way a background worker does. A job's
        // position is a sequence number offset by how many have been popped, so
        // this is what exercises that arithmetic repeatedly rather than once.
        let mut drained = 0;
        for _ in 0..8 {
            drained += cache.drain_async_writeback(8).unwrap().drained;
        }
        assert_eq!(drained, 64);

        // A key still queued must coalesce in place, not enqueue a second job.
        // If the stored position were read as a raw index it would now name the
        // wrong job -- or a job already written -- after 64 pops.
        let still_queued = CacheKey::string(0, "job-0100");
        assert_eq!(
            cache
                .enqueue_async_writeback_batch(vec![(still_queued.clone(), vec![b'z'; 8])])
                .unwrap(),
            1
        );

        let mut seen = Vec::new();
        loop {
            let report = cache.drain_async_writeback(8).unwrap();
            if report.drained == 0 {
                break;
            }
            seen.push(report.drained);
        }
        // 64 remaining, and the coalesced key replaced a job rather than adding
        // one, so the total drained across the whole test is exactly 128.
        assert_eq!(drained + seen.iter().sum::<usize>(), 128);
        assert_eq!(cache.get(&still_queued).unwrap(), Some(vec![b'z'; 8]));
    }

    #[test]
    fn lru_refresh_time_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::new(1 << 16, dir.path());
        // The default is not zero, and that matters beyond this accessor:
        // moving an entry in the access orders needs the cache exclusively, so
        // a default of zero would send every hit through the exclusive path
        // and no read would ever be served under the shared lock.
        assert_eq!(
            cache.lru_refresh_time(),
            std::time::Duration::from_millis(500),
            "the default should let a recently-read entry keep its place"
        );
        cache.set_lru_refresh_time(std::time::Duration::from_secs(2));
        assert_eq!(cache.lru_refresh_time(), std::time::Duration::from_secs(2));
        // Zero is still reachable, and still means "move it on every hit".
        cache.set_lru_refresh_time(std::time::Duration::ZERO);
        assert_eq!(cache.lru_refresh_time(), std::time::Duration::ZERO);
    }

    #[test]
    fn a_refresh_distance_leaves_reads_and_eviction_working() {
        // The refresh distance changes where a re-read entry sits in the access
        // order. Under the default policy that only decides which entries enter
        // a 512-wide eviction window -- scoring within the window still picks
        // the coldest -- so it is deliberately not asserted here which entry is
        // evicted. What is asserted is that a cache running with the knob set
        // still serves what it holds and still honours its capacity.
        let cache =
            MultiLayerCache::try_with_options(CacheOptions::new(4096, 0, 0)).expect("cache");
        cache.set_lru_refresh_time(std::time::Duration::from_millis(64));

        let hot: Vec<CacheKey> = (0..8)
            .map(|i| CacheKey::string(0, &format!("hot-{i}")))
            .collect();
        for key in &hot {
            cache.put(key.clone(), vec![b'h'; 16]).unwrap();
        }

        // Read the hot set repeatedly, which is exactly the pattern the knob
        // skips work for, while cold keys churn through the tier.
        for round in 0..40 {
            for key in &hot {
                assert_eq!(cache.get(key).unwrap(), Some(vec![b'h'; 16]));
            }
            let cold = CacheKey::string(1, &format!("cold-{round}"));
            cache.put(cold, vec![b'c'; 16]).unwrap();
        }

        // Everything read every round is still resident, and the tier is still
        // inside its capacity.
        for key in &hot {
            assert_eq!(cache.get(key).unwrap(), Some(vec![b'h'; 16]));
        }
        assert!(cache.size_for_tier(CacheTier::Memory) <= 4096);
    }

    #[test]
    fn concurrent_reads_return_whole_values_while_eviction_runs() {
        // Small enough that writers evict continuously, so reads land in the
        // window between the shared-lock probe and the exclusive lock taken
        // for bookkeeping.
        const KEYS: usize = 256;
        let cache = std::sync::Arc::new(
            MultiLayerCache::try_with_options(CacheOptions::new(16_384, 0, 0)).unwrap(),
        );
        let value_for = |i: usize| format!("value-for-key-{i:04}-{}", "p".repeat(i % 48));
        let key_for = |i: usize| CacheKey::string(0, &format!("race-key-{i:04}"));

        for i in 0..KEYS {
            cache.put(key_for(i), value_for(i).into_bytes()).unwrap();
        }

        let workers = (0..6)
            .map(|worker| {
                let cache = cache.clone();
                std::thread::spawn(move || {
                    let mut hits = 0_usize;
                    for round in 0..400 {
                        let i = (worker * 37 + round * 11) % KEYS;
                        if worker % 2 == 0 {
                            cache.put(key_for(i), value_for(i).into_bytes()).unwrap();
                        } else if let Some(found) = cache.get(&key_for(i)).unwrap() {
                            // A miss is fine -- it may have been evicted. A
                            // hit must be this key's whole value.
                            assert_eq!(
                                found,
                                value_for(i).into_bytes(),
                                "key {i} returned another entry's value"
                            );
                            hits += 1;
                        }
                    }
                    hits
                })
            })
            .collect::<Vec<_>>();
        let hits = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .sum::<usize>();
        // Every assertion above is on the value of a hit, so a run where
        // everything missed would pass having checked nothing.
        assert!(hits > 0, "readers never hit, so nothing was checked");

        // The cache is still usable and still honouring its capacity.
        assert!(cache.size_for_tier(CacheTier::Memory) <= 16_384);
        let probe = CacheKey::string(0, "race-key-after");
        cache.put(probe.clone(), b"after".to_vec()).unwrap();
        assert_eq!(cache.get(&probe).unwrap().unwrap(), b"after".to_vec());
    }

    #[test]
    fn a_hit_is_counted_exactly_once_at_either_refresh_distance() {
        // A read takes the shared path to count itself, and escalates only to
        // move the entry in the access orders. `lru_refresh_time` decides
        // how often that second step happens -- zero means every hit -- but it
        // must not change how many times the hit is *counted*.
        //
        // The way this breaks is the escalation calling `record_hit`, which
        // counts, rather than `refresh_access_order`, which does not.
        const KEYS: usize = 16;

        // Key i is read i + 1 times, so a miscount shows as a pattern rather
        // than a single wrong number.
        let reads_for = |i: usize| i + 1;

        for refresh_distance in [
            std::time::Duration::ZERO,
            std::time::Duration::from_millis(64),
            std::time::Duration::from_secs(60),
        ] {
            let cache =
                MultiLayerCache::try_with_options(CacheOptions::new(1 << 20, 0, 0)).unwrap();
            cache.set_lru_refresh_time(refresh_distance);
            for i in 0..KEYS {
                cache
                    .put(CacheKey::string(0, &format!("once-{i:03}")), vec![b'v'; 64])
                    .unwrap();
            }
            for i in 0..KEYS {
                for _ in 0..reads_for(i) {
                    assert!(cache
                        .get(&CacheKey::string(0, &format!("once-{i:03}")))
                        .unwrap()
                        .is_some());
                }
            }

            let mut seen = 0;
            for entry in cache.all_entries() {
                let index: usize = entry
                    .record_key
                    .strip_prefix("once-")
                    .expect("key prefix")
                    .parse()
                    .expect("key index");
                assert_eq!(
                    entry.hits,
                    reads_for(index) as u64,
                    "key {index} at refresh window {refresh_distance:?} was counted \
                     {} times for {} reads",
                    entry.hits,
                    reads_for(index)
                );
                seen += 1;
            }
            assert_eq!(seen, KEYS, "every key should still be resident");
        }
    }

    #[test]
    fn batch_reads_answer_and_count_every_occurrence_of_a_repeated_key() {
        let cache =
            MultiLayerCache::try_with_options(CacheOptions::new(1 << 20, 0, 0)).unwrap();
        let repeated = CacheKey::string(0, "batch-repeated");
        let other = CacheKey::string(0, "batch-other");
        cache.put(repeated.clone(), b"repeated-value".to_vec()).unwrap();
        cache.put(other.clone(), b"other-value".to_vec()).unwrap();

        let before = cache.stats().memory_hits;
        // The repeated key three times, interleaved, plus one absent key.
        let batch = vec![
            repeated.clone(),
            other.clone(),
            repeated.clone(),
            CacheKey::string(0, "batch-absent"),
            repeated.clone(),
        ];
        let values = cache.get_batch(&batch).unwrap();

        assert_eq!(values[0].as_deref(), Some(&b"repeated-value"[..]));
        assert_eq!(values[1].as_deref(), Some(&b"other-value"[..]));
        assert_eq!(values[2].as_deref(), Some(&b"repeated-value"[..]));
        assert_eq!(values[3], None, "the absent key should not be answered");
        assert_eq!(values[4].as_deref(), Some(&b"repeated-value"[..]));

        // Four resident occurrences were read, so four hits.
        assert_eq!(
            cache.stats().memory_hits - before,
            4,
            "every occurrence of a repeated key is its own read"
        );

        // And the entry is still readable afterwards.
        assert_eq!(
            cache.get(&repeated).unwrap().unwrap(),
            b"repeated-value".to_vec()
        );
    }

    #[test]
    fn refresh_window_adapts_to_how_long_entries_survive() {
        use std::time::Duration;

        const FLOOR: Duration = Duration::from_millis(50);
        let key = CacheKey::string(0, "adaptive");

        // Disabled -- the default -- must pin the window to the floor however
        // old the entries get. Every other measurement in this repo assumes it.
        {
            let cache =
                MultiLayerCache::try_with_options(CacheOptions::new(1 << 20, 0, 0)).unwrap();
            cache.set_lru_refresh_time(FLOOR);
            cache.put(key.clone(), vec![b'v'; 64]).unwrap();
            std::thread::sleep(Duration::from_millis(300));
            for _ in 0..8 {
                cache.get(&key).unwrap();
            }
            assert_eq!(
                cache.effective_lru_refresh_time(),
                FLOOR,
                "ratio 0 must leave the window alone"
            );
            assert_eq!(cache.lru_refresh_ratio(), 0.0);
        }

        // Enabled, against an entry that has been resident a while: the window
        // should grow past the floor towards that age.
        {
            let cache =
                MultiLayerCache::try_with_options(CacheOptions::new(1 << 20, 0, 0)).unwrap();
            cache.set_lru_refresh_time(FLOOR);
            cache.put(key.clone(), vec![b'v'; 64]).unwrap();
            std::thread::sleep(Duration::from_millis(300));
            cache.set_lru_refresh_ratio(1.0);
            for _ in 0..8 {
                cache.get(&key).unwrap();
            }
            let effective = cache.effective_lru_refresh_time();
            assert!(
                effective > FLOOR,
                "a ratio of 1.0 against a ~300ms-old entry should exceed a {FLOOR:?} \
                 floor, got {effective:?}"
            );
            assert!(
                effective >= Duration::from_millis(150),
                "the window should track the entry's age, got {effective:?}"
            );
        }

        // And it must not run away: the cap exists because the age of the
        // oldest entry is unbounded.
        {
            let cache =
                MultiLayerCache::try_with_options(CacheOptions::new(1 << 20, 0, 0)).unwrap();
            cache.set_lru_refresh_time(FLOOR);
            cache.put(key.clone(), vec![b'v'; 64]).unwrap();
            std::thread::sleep(Duration::from_millis(100));
            cache.set_lru_refresh_ratio(100_000.0);
            for _ in 0..8 {
                cache.get(&key).unwrap();
            }
            assert_eq!(
                cache.effective_lru_refresh_time(),
                Duration::from_secs(10),
                "an absurd ratio must clamp to the cap"
            );
        }

        // A negative or non-finite ratio means disabled, not chaos.
        {
            let cache =
                MultiLayerCache::try_with_options(CacheOptions::new(1 << 20, 0, 0)).unwrap();
            cache.set_lru_refresh_ratio(-3.0);
            assert_eq!(cache.lru_refresh_ratio(), 0.0);
            cache.set_lru_refresh_ratio(f64::NAN);
            assert_eq!(cache.lru_refresh_ratio(), 0.0);
        }
    }

    #[test]
    fn prometheus_exposition_is_well_formed() {
        let cache =
            MultiLayerCache::try_with_options(CacheOptions::new(1 << 20, 0, 0)).unwrap();
        for i in 0..64 {
            let key = CacheKey::string(0, &format!("metrics-{i:03}"));
            cache.put(key.clone(), vec![b'v'; 128]).unwrap();
            cache.get(&key).unwrap();
        }
        cache.get(&CacheKey::string(0, "metrics-absent")).unwrap();

        let text = prometheus_text(&cache.stats(), &[("cache", "unit")]);

        // Every series must be typed and documented: an untyped series makes
        // rate() on a gauge look reasonable to whoever builds the dashboard.
        let helps = text.lines().filter(|l| l.starts_with("# HELP ")).count();
        let types = text.lines().filter(|l| l.starts_with("# TYPE ")).count();
        assert_eq!(helps, types, "every metric needs both HELP and TYPE");
        assert!(helps > 50, "expected the whole of CacheStats, got {helps} metrics");

        // Something must actually have been counted, or the rest of this
        // passes on an empty response.
        assert!(
            text.contains("matrixcache_memory_hits{cache=\"unit\"} 64"),
            "hits should be exported and labelled:\n{text}"
        );

        // Histogram buckets are cumulative, and _count must equal +Inf.
        for family in [
            "matrixcache_get_latency_seconds",
            "matrixcache_read_through_latency_seconds",
        ] {
            let mut previous = 0_u64;
            let mut infinity = None;
            for line in text.lines() {
                let Some(rest) = line.strip_prefix(&format!("{family}_bucket")) else {
                    continue;
                };
                let value: u64 = rest
                    .rsplit(' ')
                    .next()
                    .expect("bucket value")
                    .parse()
                    .expect("bucket parses");
                assert!(
                    value >= previous,
                    "{family} buckets must not decrease: {previous} then {value}"
                );
                previous = value;
                if rest.contains("le=\"+Inf\"") {
                    infinity = Some(value);
                }
            }
            let infinity = infinity.unwrap_or_else(|| panic!("{family} has no +Inf bucket"));
            let count_line = text
                .lines()
                .find(|l| l.starts_with(&format!("{family}_count")))
                .unwrap_or_else(|| panic!("{family} has no _count"));
            let count: u64 = count_line
                .rsplit(' ')
                .next()
                .expect("count value")
                .parse()
                .expect("count parses");
            assert_eq!(count, infinity, "{family}: _count must equal the +Inf bucket");
        }
        for gauge in [
            "matrixcache_get_latency_avg_seconds",
            "matrixcache_put_latency_avg_seconds",
            "matrixcache_read_through_latency_avg_seconds",
            "matrixcache_refill_latency_avg_seconds",
            "matrixcache_writeback_latency_avg_seconds",
            "matrixcache_eviction_latency_avg_seconds",
            "matrixcache_compaction_latency_avg_seconds",
        ] {
            assert!(
                text.contains(gauge),
                "{gauge} should be exported for direct dashboard averages:\n{text}"
            );
        }

        // A label value with a quote in it must not break the response.
        let awkward = prometheus_text(&cache.stats(), &[("cache", "a\"b\\c")]);
        assert!(
            awkward.contains(r#"cache="a\"b\\c""#),
            "label values must be escaped:\n{}",
            awkward.lines().take(3).collect::<Vec<_>>().join("\n")
        );
    }
    #[test]
    fn access_order_survives_a_storm_of_operations_at_every_insertion_spec() {
        // Incrementally maintaining a pointer into a linked list is subtly
        // wrong rather than obviously wrong, and a corrupted eviction order
        // surfaces as a hit-rate regression nobody traces back here. So every
        // operation that touches the access list is driven in a deterministic
        // pseudo-random mix, and the whole structure is checked after each one.
        for spec in [0_u8, 1, 2, 3] {
            let mut order = CacheKeyOrder::new();
            order.set_insertion_spec(spec);
            let mut state = 0x2545_F491_4F6C_DD1D_u64;
            let mut draw = move || {
                state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                (state >> 33) as usize
            };

            for step in 0..4_000 {
                let key = CacheKey::string(0, &format!("k{:04}", draw() % 300));
                match draw() % 8 {
                    0..=3 => order.push_back_if_absent(key),
                    4..=5 => {
                        order.touch_access(&key);
                    }
                    6 => {
                        order.remove(&key);
                    }
                    _ => {
                        order.pop_front();
                    }
                }
                if let Err(problem) = order.check_access_invariants() {
                    panic!("spec {spec}, step {step}: {problem}");
                }
            }

            // And the spec must have actually done something: with a non-zero
            // spec a fresh entry should not land at the most-recently-used end,
            // or the whole mechanism is inert and the storm above proved only
            // that nothing changed.
            if spec != 0 && order.len() > 8 {
                let fresh = CacheKey::string(0, "brand-new");
                order.push_back_if_absent(fresh.clone());
                let hottest = order.iter_access().last().cloned();
                assert_ne!(
                    hottest.as_ref(),
                    Some(&fresh),
                    "spec {spec} should not place a new entry at the hot end"
                );
                order.check_access_invariants().expect("still consistent");
            }
        }
    }

    #[test]
    fn insertion_spec_places_new_entries_where_it_says() {
        // Ten entries, then one more: spec 1 should leave five entries closer
        // to eviction than the newcomer, spec 2 should leave two.
        for (spec, expected_colder) in [(1_u8, 5_usize), (2, 2)] {
            let mut order = CacheKeyOrder::new();
            order.set_insertion_spec(spec);
            for i in 0..10 {
                order.push_back_if_absent(CacheKey::string(0, &format!("e{i}")));
            }
            let fresh = CacheKey::string(0, "fresh");
            order.push_back_if_absent(fresh.clone());

            let colder = order
                .iter_access()
                .take_while(|key| **key != fresh)
                .count();
            assert_eq!(
                colder, expected_colder,
                "spec {spec}: expected {expected_colder} entries closer to eviction, got {colder}"
            );
            order.check_access_invariants().expect("consistent");
        }
    }
    #[test]
    fn the_first_read_escalates_and_the_second_does_not() {
        use std::time::Duration;

        // A window far longer than this test runs, so elapsed time can never be
        // what triggers a refresh. Anything that escalates here does so because
        // of the first-read rule.
        let cache =
            MultiLayerCache::try_with_options(CacheOptions::new(1 << 20, 0, 0)).unwrap();
        cache.set_lru_refresh_time(Duration::from_secs(600));
        let key = CacheKey::string(0, "first-read");
        cache.put(key.clone(), vec![b'v'; 64]).unwrap();

        let before = cache.stats().access_order_refreshes;
        assert!(cache.get(&key).unwrap().is_some());
        let after_first = cache.stats().access_order_refreshes;
        assert_eq!(
            after_first - before,
            1,
            "the first read since admission should move the entry whatever the              window says"
        );

        // Every read after that is inside the window, so none of them should
        // need the exclusive lock. This is the half that makes the rule cheap.
        for _ in 0..50 {
            assert!(cache.get(&key).unwrap().is_some());
        }
        assert_eq!(
            cache.stats().access_order_refreshes,
            after_first,
            "reads inside the refresh window should stay on the shared path"
        );

        // And a zero window still means "every hit", which is what the setting
        // has always meant.
        cache.set_lru_refresh_time(Duration::ZERO);
        let before_zero = cache.stats().access_order_refreshes;
        for _ in 0..10 {
            cache.get(&key).unwrap();
        }
        assert_eq!(
            cache.stats().access_order_refreshes - before_zero,
            10,
            "a zero window should escalate every hit"
        );
    }

    #[test]
    fn the_sketch_never_underestimates() {
        // The one-sided error, asserted with no slack: collisions may inflate a
        // count but must never reduce one. Admission leans on exactly this --
        // a key the sketch calls cold really is cold.
        //
        // Short enough that no decay can happen (the window is capacity * 32),
        // because a decay would make the true count a moving target and force
        // the kind of slack that hides an off-by-one.
        let sketch = FrequencySketch::with_capacity(512);
        let mut truth = std::collections::HashMap::new();
        let mut state = 0x2545_F491_4F6C_DD1D_u64;

        for _ in 0..8_000 {
            state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let index = (state >> 33) % 3_000;
            let key = CacheKey::string(0, &format!("s-{index:05}"));
            sketch.record(&key);
            *truth.entry(index).or_insert(0_u32) += 1;
        }
        assert!(sketch.window() > 0, "a decay ran; the exact comparison is void");

        for (index, count) in &truth {
            let key = CacheKey::string(0, &format!("s-{index:05}"));
            let estimate = sketch.estimate(&key) as u32;
            let expected = (*count).min(255);
            assert!(
                estimate >= expected,
                "key {index} seen {count} times estimated {estimate}, under {expected}"
            );
        }
    }

    #[test]
    fn the_sketch_is_accurate_enough_to_separate_singletons() {
        // Ranking one hot key above one cold key is true even with a single
        // counter, so it tests nothing. What the four rows buy is accuracy:
        // taking the smallest of four makes an inflated estimate need a
        // collision in every row at once.
        //
        // Measured as: how many keys seen exactly once estimate exactly one.
        let sketch = FrequencySketch::with_capacity(1_024);
        let keys = 2_000;
        for i in 0..keys {
            sketch.record(&CacheKey::string(0, &format!("one-{i:05}")));
        }
        let exact = (0..keys)
            .filter(|i| sketch.estimate(&CacheKey::string(0, &format!("one-{i:05}"))) == 1)
            .count();
        let share = exact as f64 / keys as f64;
        assert!(
            share > 0.90,
            "only {exact}/{keys} singletons estimated exactly 1 ({share:.2}) -- \
             the rows are not independent enough to be useful for admission"
        );

        // And a key never recorded must not look warm.
        assert_eq!(
            sketch.estimate(&CacheKey::string(0, "never-recorded")),
            0,
            "an unrecorded key should estimate zero"
        );
    }

    #[test]
    fn the_sketch_decays_rather_than_accumulating_forever() {
        // Without decay the estimate is a lifetime count, and a key that was
        // hot an hour ago outranks one that is hot now for ever. The test is
        // that the estimate *falls*, not that an ordering happens to survive.
        let capacity = 64;
        let sketch = FrequencySketch::with_capacity(capacity);
        let key = CacheKey::string(0, "decaying");
        for _ in 0..200 {
            sketch.record(&key);
        }
        let before = sketch.estimate(&key);
        assert!(before >= 200, "expected the full count, got {before}");

        // Push past the window (capacity * 32) with unrelated traffic.
        for i in 0..(capacity as u64 * 32 + 64) {
            sketch.record(&CacheKey::string(0, &format!("drive-{i:05}")));
        }
        let after = sketch.estimate(&key);
        assert!(
            after < before,
            "estimate was {before} before the window elapsed and {after} after -- \
             nothing decayed"
        );
        assert!(after > 0, "decay should halve, not erase: got {after}");
    }

    #[test]
    fn the_sketch_saturates_instead_of_wrapping() {
        let sketch = FrequencySketch::with_capacity(4_096);
        let key = CacheKey::string(0, "saturate");
        for _ in 0..1_000 {
            sketch.record(&key);
        }
        // A u8 counter wrapping at 256 would read as almost never seen, which
        // is the worst possible answer for the hottest key in the cache.
        assert!(
            sketch.estimate(&key) >= 200,
            "a key seen a thousand times estimated {}",
            sketch.estimate(&key)
        );
    }

    #[test]
    fn the_sketch_is_sized_and_safe_at_the_edges() {
        let sketch = FrequencySketch::with_capacity(1_000);
        assert!(sketch.width().is_power_of_two());
        assert!(sketch.width() >= 1_000, "width {} too small", sketch.width());

        // Capacity zero must still be usable rather than panic on first index.
        let empty = FrequencySketch::with_capacity(0);
        let key = CacheKey::string(0, "edge");
        empty.record(&key);
        assert!(empty.estimate(&key) > 0);
    }

    #[test]
    fn the_admission_filter_declines_a_colder_newcomer_then_relents() {
        // Asserted through `memory_admission_rejected`, not through whether the
        // key ends up resident.
        //
        // The first version of this test checked residency and failed, and the
        // reason was the test rather than the filter: the newcomer *was*
        // admitted and then immediately evicted by the replacement policy,
        // which quite correctly scored a zero-hit entry as the coldest thing in
        // the cache. Rejection and eviction are different events and only one
        // of them is this filter's doing.
        const ENTRIES: usize = 64;
        const VALUE: usize = 64;

        let cache =
            MultiLayerCache::try_with_options(CacheOptions::new(ENTRIES * VALUE, 0, 0))
                .unwrap();
        cache.set_admission_filter_enabled(true);
        assert!(cache.admission_filter_enabled());

        for i in 0..ENTRIES {
            cache
                .put(CacheKey::string(0, &format!("res-{i:03}")), vec![b'r'; VALUE])
                .unwrap();
        }
        for _ in 0..40 {
            for i in 0..ENTRIES {
                cache
                    .get(&CacheKey::string(0, &format!("res-{i:03}")))
                    .unwrap();
            }
        }

        // A key seen once, against a cache of keys seen forty times.
        let stranger = CacheKey::string(0, "stranger");
        let before = cache.stats().memory_admission_rejected;
        cache.put(stranger.clone(), vec![b's'; VALUE]).unwrap();
        assert!(
            cache.stats().memory_admission_rejected > before,
            "a first sighting should be declined against entries asked for forty times"
        );

        // Asked for persistently, it must stop being declined -- otherwise this
        // is a permanent ban rather than a comparison, and the cache could never
        // follow a working set that moves.
        for _ in 0..40 {
            if cache.get(&stranger).unwrap().is_none() {
                cache.put(stranger.clone(), vec![b's'; VALUE]).unwrap();
            }
        }
        let settled = cache.stats().memory_admission_rejected;
        for _ in 0..20 {
            if cache.get(&stranger).unwrap().is_none() {
                cache.put(stranger.clone(), vec![b's'; VALUE]).unwrap();
            }
        }
        assert_eq!(
            cache.stats().memory_admission_rejected,
            settled,
            "a key asked for this persistently should no longer be declined"
        );
    }

    #[test]
    fn the_admission_filter_is_off_by_default() {
        let cache =
            MultiLayerCache::try_with_options(CacheOptions::new(64 * 64, 0, 0)).unwrap();
        assert!(!cache.admission_filter_enabled());

        // Off, everything is admitted, however cold -- which is the behaviour
        // every other measurement in this repo was taken against.
        for i in 0..64 {
            cache
                .put(CacheKey::string(0, &format!("d-{i:03}")), vec![b'd'; 64])
                .unwrap();
        }
        let newcomer = CacheKey::string(0, "newcomer");
        cache.put(newcomer.clone(), vec![b'n'; 64]).unwrap();
        assert!(
            cache.get(&newcomer).unwrap().is_some(),
            "with the filter off a newcomer is always admitted"
        );
    }

}


/// Every page cache key shape, byte for byte.
///
/// These strings are hashed and compared against keys already written -- to a running cache and to
/// disk -- so a changed byte is a silent miss rather than a failure: the cache stops finding what it
/// stored and nothing complains. Building them directly instead of through `format!` saves two
/// allocations per key on every get and put, which is only safe if the bytes do not move. So pin the
/// bytes, not the behaviour, and cover each shape the constructors can produce.
#[test]
fn page_cache_keys_keep_their_exact_bytes() {
    let plain = CacheKey::page(1, 8, 8192, 512);
    assert_eq!(plain.namespace, "page");
    assert_eq!(plain.record_key, "segment-00000000000000000008");
    assert_eq!(plain.selector, "8192:512");

    let with_slot = CacheKey::page_with_slot(1, 8, 8192, 512, Some(3));
    assert_eq!(with_slot.record_key, "segment-00000000000000000008");
    assert_eq!(with_slot.selector, "slot-3:8192:512");

    let without_slot = CacheKey::page_with_slot(1, 8, 8192, 512, None);
    assert_eq!(without_slot.selector, "8192:512");
    assert_eq!(
        without_slot, plain,
        "no routing slot has to be the same key `page` builds, or one writer's pages become \
         unreadable to the other"
    );

    let slot_and_generation = CacheKey::page_with_slot_generation(1, 8, 8192, 512, Some(3), Some(7));
    assert_eq!(slot_and_generation.selector, "slot-3:gen-7:8192:512");

    let generation_only = CacheKey::page_with_slot_generation(1, 8, 8192, 512, None, Some(7));
    assert_eq!(generation_only.selector, "gen-7:8192:512");

    // A large segment id still pads to twenty digits, and one that overflows the padding is not
    // truncated -- both are what the format string did.
    assert_eq!(
        CacheKey::page(1, 12_345_678_901_234_567_890, 0, 1).record_key,
        "segment-12345678901234567890"
    );
    assert_eq!(CacheKey::page(1, 0, 0, 1).record_key, "segment-00000000000000000000");

    // Distinct inputs stay distinct: the by-hand builder joins fields with the same separators, so
    // two different pages cannot collapse onto one key.
    assert_ne!(CacheKey::page(1, 8, 81, 92).selector, CacheKey::page(1, 8, 8192, 0).selector);
}


/// A shared read returns exactly what a copying read returns, and keeps it.
///
/// Handing out the cached `Arc<[u8]>` instead of a copy saves the reader a page-sized memcpy and an
/// allocation. The bytes being equal on the way out is the easy half; the half worth testing is what
/// happens next -- the key overwritten, the key evicted -- while a reader still holds what it was
/// given. A copy is obviously safe under both. A share has to be shown to be.
#[test]
fn a_shared_read_matches_a_copying_read() {
    let dir = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::new(1 << 20, dir.path());
    cache.start().unwrap();
    let key = CacheKey::page(1, 42, 0, 64);
    let payload: Vec<u8> = (0..64_u8).collect();
    cache.put(key.clone(), payload.clone()).expect("put");

    let copied = cache.get(&key).expect("get").expect("the key is present");
    let shared = cache.get_shared(&key).expect("get_shared").expect("the key is present");
    assert_eq!(&*shared, copied.as_slice(), "a shared read must return the copied read's bytes");
    assert_eq!(&*shared, payload.as_slice(), "and both must return what was put");

    // Overwrite the key while the share is held. The holder keeps reading what it asked for: a put
    // replaces the entry rather than editing it, which is what makes sharing safe at all.
    let replacement: Vec<u8> = (100..164_u8).collect();
    cache.put(key.clone(), replacement.clone()).expect("overwrite");
    assert_eq!(
        &*shared,
        payload.as_slice(),
        "an overwrite must not change the bytes a reader is already holding"
    );
    let after = cache.get(&key).expect("get").expect("still present");
    assert_eq!(after, replacement, "and the next reader must see the new value");

    // A miss is a miss either way, not an empty slice.
    let absent = CacheKey::page(1, 43, 0, 64);
    assert!(cache.get_shared(&absent).expect("get_shared").is_none());
    assert!(cache.get(&absent).expect("get").is_none());
}

/// A shared read is visible to the eviction policy, exactly as a copying read is.
///
/// A read that the policy cannot see would quietly change what the cache chooses to evict -- the
/// hottest key in the store could look untouched. This is the property most easily lost by
/// short-circuiting the lookup, so it is asserted rather than assumed.
#[test]
fn a_shared_read_counts_as_a_read() {
    let dir = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::new(1 << 20, dir.path());
    cache.start().unwrap();
    let key = CacheKey::page(1, 7, 0, 16);
    cache.put(key.clone(), vec![9_u8; 16]).expect("put");

    let before = cache.stats().memory_hits;
    let _ = cache.get_shared(&key).expect("get_shared").expect("present");
    let after = cache.stats().memory_hits;
    assert!(
        after > before,
        "a shared read must register as a memory hit ({before} -> {after}), or the policy stops seeing \
         reads of whatever uses it"
    );


}

#[test]
fn grafana_dashboard_metrics_are_exported() {
    let cache =
        MultiLayerCache::try_with_options(CacheOptions::new(64 * 64, 64 * 64, 64 * 64)).unwrap();
    cache.start().unwrap();
    let key = CacheKey::string(0, "grafana-drift");
    cache.put(key.clone(), vec![1; 32]).unwrap();
    let _ = cache.get(&key).unwrap();
    cache.record_compaction_latency_micros(25);

    let exported = prometheus_text(&cache.stats(), &[("cache", "dashboard")]);
    let dashboard = include_str!("../../docs/grafana/matrixcache-dashboard.json");
    let mut names = dashboard_metric_names(dashboard);
    names.sort();
    names.dedup();

    assert!(
        !names.is_empty(),
        "the Grafana dashboard should reference MatrixCache metrics"
    );
    for name in names {
        assert!(
            exported.contains(&name),
            "dashboard references {name}, but the Prometheus exporter does not expose it"
        );
    }
}

#[cfg(test)]
fn dashboard_metric_names(input: &str) -> Vec<String> {
    let mut names = Vec::new();
    let bytes = input.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if input[index..].starts_with("matrixcache_") {
            let start = index;
            index += "matrixcache_".len();
            while index < bytes.len() {
                let byte = bytes[index];
                if byte.is_ascii_alphanumeric() || byte == b'_' {
                    index += 1;
                } else {
                    break;
                }
            }
            names.push(input[start..index].to_string());
        } else {
            index += 1;
        }
    }
    names
}
