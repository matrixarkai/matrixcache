// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI
//
// Reading a configuration back to the person who wrote it.
//
// Every field in `CacheOptions` has a default, and the parsers fall back to a
// default rather than refusing, which makes a misconfigured cache behave like
// a working one: a policy nobody offers becomes the default policy, and a tier
// given a size but no path becomes a tier in a temporary directory. Both are
// quiet, and both are found later by someone wondering why the numbers are
// wrong.
//
// Each finding names the field it came from and says what the cache will
// actually do, because "invalid" is not useful on its own.

/// Something about a configuration worth telling the caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheConfigFinding {
    /// Stable identifier, for matching in a test or an alert.
    pub id: String,
    pub severity: CacheHealthSeverity,
    /// The `CacheOptions` field the finding came from.
    pub field: String,
    /// What the cache will do, not merely that something is wrong.
    pub message: String,
}

fn config_finding(
    id: &str,
    severity: CacheHealthSeverity,
    field: &str,
    message: String,
) -> CacheConfigFinding {
    CacheConfigFinding {
        id: id.to_string(),
        severity,
        field: field.to_string(),
        message,
    }
}

impl CacheOptions {
    /// What this configuration will actually do, where that differs from what
    /// it appears to ask for.
    ///
    /// Pure: it reads the options and returns findings. [`Self::validate`] is
    /// what [`MultiLayerCache::try_with_options`] consults before building,
    /// and it can also be called directly by anything that loads a
    /// configuration from a file and wants to complain before starting.
    ///
    /// An empty name is not a finding. Empty means the field was not set, and
    /// every one of them has a documented default.
    pub fn validate(&self) -> Vec<CacheConfigFinding> {
        let mut findings = Vec::new();

        if self.dram_capacity == 0 && self.pmem_capacity == 0 && self.ssd_capacity == 0 {
            findings.push(config_finding(
                "no_tier_has_capacity",
                CacheHealthSeverity::Critical,
                "dram_capacity",
                "every tier has a capacity of zero, so the cache can hold nothing: \
                 each write is accepted and immediately has nowhere to go"
                    .to_string(),
            ));
        }

        // A size with nowhere to put it. The SSD tier falls back to a fresh
        // temporary directory, which looks like it works -- writes land, reads
        // are served -- right up to the restart that was the reason for having
        // a durable tier at all.
        //
        // Reported, not refused, and only as information: this is what the
        // default configuration does, and a check that complains about the
        // defaults is a check people learn to ignore.
        let ssd_tier_is_temporary = self.ssd_capacity > 0 && self.ssd_paths.is_empty();
        if ssd_tier_is_temporary {
            findings.push(config_finding(
                "ssd_tier_is_temporary",
                CacheHealthSeverity::Info,
                "ssd_paths",
                format!(
                    "the SSD tier is given {} bytes and no path, so it is placed in a                      fresh temporary directory: it serves reads until the process                      restarts, and nothing written to it survives that",
                    self.ssd_capacity
                ),
            ));
        }

        // Asking to recover from a tier that cannot have anything in it,
        // though, is a contradiction rather than a default. Recovery will read
        // a directory created moments earlier and find it empty, every time.
        if ssd_tier_is_temporary && self.auto_recover_on_start {
            findings.push(config_finding(
                "recovery_expected_from_a_temporary_tier",
                CacheHealthSeverity::Critical,
                "ssd_paths",
                "recovery on start is enabled and the SSD tier has no path, so it is                  recovered from a temporary directory created moments earlier: it will                  be empty every time, and nothing will say so"
                    .to_string(),
            ));
        }

        if self.pmem_capacity > 0 && self.pmem_paths.is_empty() {
            findings.push(config_finding(
                "pmem_tier_is_unreachable",
                CacheHealthSeverity::Info,
                "pmem_paths",
                format!(
                    "the persistent tier is given {} bytes and no path, so nothing is                      demoted into it and it holds nothing",
                    self.pmem_capacity
                ),
            ));
        }

        // Recovering a tier whose writes were never flushed. The blocks are
        // still whole -- they arrive by rename -- but a crash can lose ones the
        // cache believed it had, so recovery finds a tier with holes in it and
        // no way to know which entries used to be there.
        //
        // A warning rather than a refusal: it degrades to misses, which is what
        // a cache does anyway, and an operator who has weighed that is entitled
        // to it.
        if self.auto_recover_on_start && !self.ssd_block_durability && self.ssd_capacity > 0 {
            findings.push(config_finding(
                "recovery_expects_durability_that_is_off",
                CacheHealthSeverity::Warning,
                "ssd_block_durability",
                "recovery on start is enabled and SSD block writes are not flushed, so a \
                 machine crash loses blocks the cache recorded and recovery restores a \
                 tier with holes in it"
                    .to_string(),
            ));
        }
        // The same contradiction one tier up, and the one more likely to be
        // met by accident: this tier does not flush unless asked, and its name
        // suggests it would not need to be.
        if self.auto_recover_on_start && !self.pmem_block_durability && self.pmem_capacity > 0 {
            findings.push(config_finding(
                "recovery_expects_durability_that_is_off",
                CacheHealthSeverity::Warning,
                "pmem_block_durability",
                "recovery on start is enabled and persistent-tier writes are not flushed, \
                 so a machine crash loses blocks the cache recorded; the tier is files \
                 standing in for persistent memory, and files are not durable until they \
                 are flushed"
                    .to_string(),
            ));
        }

        // And the reverse: a path with no size, which reads as a configured
        // tier and is an absent one.
        if self.ssd_capacity == 0 && !self.ssd_paths.is_empty() {
            findings.push(config_finding(
                "ssd_path_without_capacity",
                CacheHealthSeverity::Warning,
                "ssd_capacity",
                "SSD paths are configured and the SSD capacity is zero, so the tier is \
                 off and the paths are unused"
                    .to_string(),
            ));
        }
        if self.pmem_capacity == 0 && !self.pmem_paths.is_empty() {
            findings.push(config_finding(
                "pmem_path_without_capacity",
                CacheHealthSeverity::Warning,
                "pmem_capacity",
                "persistent-memory paths are configured and the capacity is zero, so \
                 the tier is off and the paths are unused"
                    .to_string(),
            ));
        }

        for (field, name) in [
            ("cache_dram_replacement_policy", &self.cache_dram_replacement_policy),
            ("cache_pmem_replacement_policy", &self.cache_pmem_replacement_policy),
            ("cache_ssd_replacement_policy", &self.cache_ssd_replacement_policy),
        ] {
            if name.is_empty() || CacheReplacementPolicy::try_from_config_name(name).is_some() {
                continue;
            }
            findings.push(config_finding(
                "replacement_policy_not_recognised",
                CacheHealthSeverity::Warning,
                field,
                format!(
                    "\"{name}\" is not a replacement policy this cache offers, so it is \
                     silently given {} instead; the policies are {}",
                    CacheReplacementPolicy::default().as_config_name(),
                    CacheReplacementPolicy::config_names().join(", ")
                ),
            ));
        }

        // A name that is recognised and then answered with a different
        // policy is worse than one that is not recognised at all: the second
        // can be reported as a typo, and the first reads as confirmation.
        for (field, name) in [
            (
                "cache_dram_replacement_policy",
                &self.cache_dram_replacement_policy,
            ),
            (
                "cache_pmem_replacement_policy",
                &self.cache_pmem_replacement_policy,
            ),
            (
                "cache_ssd_replacement_policy",
                &self.cache_ssd_replacement_policy,
            ),
        ] {
            if CacheReplacementPolicy::try_from_config_name(name)
                != Some(CacheReplacementPolicy::Slru)
            {
                continue;
            }
            findings.push(config_finding(
                "replacement_policy_resolves_to_another",
                CacheHealthSeverity::Warning,
                field,
                "SLRU selects the same eviction as WeightedHotnessLru on a cache tier: \
                 the two share a branch in victim selection, and a scan-resistance run \
                 gives them identical hit rates and identical eviction counts. The \
                 segmented policy exists as a component, and nothing connects it to \
                 tier eviction yet"
                    .to_string(),
            ));
        }

        let placement = &self.cache_dram_pmem_data_placement_type;
        if !placement.is_empty() && CacheDataPlacement::try_from_config_name(placement).is_err() {
            findings.push(config_finding(
                "data_placement_type_not_recognised",
                CacheHealthSeverity::Warning,
                "cache_dram_pmem_data_placement_type",
                format!(
                    "\"{placement}\" is not a data placement type this cache offers, so \
                     it is silently given Tiered instead; the types are SideBySide, Tiered"
                ),
            ));
        }

        findings
    }

    /// What this configuration will do once it is split across `shard_count`
    /// shards.
    ///
    /// Everything [`Self::validate`] reports, plus what only goes wrong
    /// because the capacities were divided. A sharded cache gives each shard a
    /// slice of every tier, and a slice can be smaller than the largest value
    /// the tier would otherwise take -- so a cache with room for a value
    /// refuses it, and the configuration that was asked for never mentions
    /// shards.
    pub fn validate_for_shards(&self, shard_count: usize) -> Vec<CacheConfigFinding> {
        let mut findings = self.validate();
        let shard_count = shard_count.max(1);
        if shard_count == 1 {
            return findings;
        }
        let policy = self.tiering_policy();

        for (field, tier, capacity, largest_value) in [
            (
                "dram_capacity",
                "memory",
                self.dram_capacity,
                policy.max_memory_block_bytes,
            ),
            (
                "pmem_capacity",
                "persistent",
                self.pmem_capacity,
                policy.max_pmem_block_bytes,
            ),
            (
                "ssd_capacity",
                "SSD",
                self.ssd_capacity,
                policy.max_ssd_block_bytes,
            ),
        ] {
            if capacity == 0 {
                continue;
            }
            let per_shard = capacity / shard_count;
            if per_shard == 0 {
                findings.push(config_finding(
                    "tier_has_fewer_bytes_than_shards",
                    CacheHealthSeverity::Warning,
                    field,
                    format!(
                        "the {tier} tier has {capacity} bytes to divide between \
                         {shard_count} shards, so some shards get none of it"
                    ),
                ));
                continue;
            }
            if per_shard < largest_value {
                findings.push(config_finding(
                    "sharded_tier_refuses_values_it_has_room_for",
                    CacheHealthSeverity::Warning,
                    field,
                    format!(
                        "the {tier} tier has {capacity} bytes divided between \
                         {shard_count} shards, so each holds {per_shard}: values above \
                         that are refused, though the tier as a whole would take up to \
                         {largest_value} and a single shard would accept them"
                    ),
                ));
            }
        }

        findings
    }

    /// The findings from [`Self::validate`] that mean the cache cannot do what
    /// it was asked to do.
    pub fn critical_findings(&self) -> Vec<CacheConfigFinding> {
        self.validate()
            .into_iter()
            .filter(|finding| finding.severity == CacheHealthSeverity::Critical)
            .collect()
    }
}
