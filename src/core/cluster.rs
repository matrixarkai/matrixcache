// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

// Placing keys on a cluster of caches.
//
// A single cache decides which of its shards holds a key with
// `hash(key) % shard_count`, which is fine because a shard count is fixed for
// the life of the cache. Across machines neither half of that survives: the
// node count changes while data is live, and `% n` reassigns almost every key
// when it does; and `DefaultHasher` is explicitly not stable between builds,
// so two nodes running different binaries would not agree on where anything
// lives.
//
// This module answers the same question with a hash ring, which moves only the
// share of keys belonging to the node that joined or left, and with a hash
// whose output is fixed by this file.

/// Ring points one unit of node weight contributes.
///
/// A ring with one point per node distributes badly: the arc between two
/// neighbours is a random variable, and with few points the widest arc is many
/// times the mean. Spreading each node over many points averages those arcs
/// out. 160 is the usual choice and costs 16 bytes of ring per point.
pub const CACHE_RING_POINTS_PER_WEIGHT: u32 = 160;

/// Relative capacity of a node, in units of one ordinary node.
pub type CacheNodeWeight = u32;

/// Whether a node is taking traffic.
///
/// A node that is `Down` holds no ring points, so its keys pass to whichever
/// node follows it on the ring and every other key stays where it was. It keeps
/// its place in the membership list, so bringing it back restores exactly the
/// keys it had.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CacheNodeState {
    Live,
    Down,
}

/// One cache in the cluster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheClusterNode {
    pub name: String,
    /// How much of the key space this node should hold, relative to a node of
    /// weight 1. A node with twice the memory can carry twice the weight.
    pub weight: CacheNodeWeight,
    pub state: CacheNodeState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CacheRingPoint {
    hash: u64,
    node: u32,
}

/// Which node in a cluster holds a given key.
///
/// # Examples
///
/// ```
/// use matrixcache::{CacheClusterTopology, CacheKey};
///
/// let mut cluster = CacheClusterTopology::new();
/// cluster.add_node("cache-a", 1)?;
/// cluster.add_node("cache-b", 1)?;
///
/// let key = CacheKey::string(0, "greeting");
/// let owner = cluster.owner(&key).expect("a live cluster owns every key");
/// assert!(owner == "cache-a" || owner == "cache-b");
///
/// // The answer does not depend on how the cluster was assembled.
/// let mut other = CacheClusterTopology::new();
/// other.add_node("cache-b", 1)?;
/// other.add_node("cache-a", 1)?;
/// assert_eq!(other.owner(&key), cluster.owner(&key));
/// # Ok::<(), matrixcache::CacheError>(())
/// ```
#[derive(Debug, Clone)]
pub struct CacheClusterTopology {
    nodes: BTreeMap<String, CacheClusterNode>,
    /// Names of the live nodes, in the order the ring points index them.
    live: Vec<String>,
    /// Sorted by hash, then by node, so a hash collision between two nodes
    /// resolves the same way on every machine that builds this ring.
    ring: Vec<CacheRingPoint>,
    points_per_weight: u32,
}

impl Default for CacheClusterTopology {
    fn default() -> Self {
        Self::with_points_per_weight(CACHE_RING_POINTS_PER_WEIGHT)
    }
}

impl CacheClusterTopology {
    pub fn new() -> Self {
        Self::default()
    }

    /// A ring with a chosen number of points per unit of weight.
    ///
    /// Fewer points cost less memory and distribute worse. Zero is raised to
    /// one, because a ring with no points owns nothing and would answer every
    /// lookup with `None` however many nodes had joined.
    pub fn with_points_per_weight(points_per_weight: u32) -> Self {
        Self {
            nodes: BTreeMap::new(),
            live: Vec::new(),
            ring: Vec::new(),
            points_per_weight: points_per_weight.max(1),
        }
    }

    /// Adds a live node.
    ///
    /// Fails on a repeated name, because two caches answering to one name would
    /// each believe they held the other's keys, and on a weight of zero, which
    /// asks for a node that is a member and owns nothing -- that is what
    /// [`CacheNodeState::Down`] is for, and it says so.
    pub fn add_node(&mut self, name: &str, weight: CacheNodeWeight) -> Result<(), CacheError> {
        self.check_new_node(name, weight)?;
        self.insert_node(name, weight);
        self.rebuild_ring();
        Ok(())
    }

    /// Adds several nodes and rebuilds the ring once.
    ///
    /// Adding a thousand nodes one at a time rebuilds a thousand rings, each
    /// larger than the last, for a cluster that was always going to end up the
    /// same shape. Starting a large cluster is this call's reason to exist.
    ///
    /// Either every node joins or none does: the names and weights are all
    /// checked before the first one is inserted, so a bad entry halfway down
    /// the list does not leave a half-built cluster behind.
    pub fn add_nodes<'a, I>(&mut self, nodes: I) -> Result<(), CacheError>
    where
        I: IntoIterator<Item = (&'a str, CacheNodeWeight)>,
    {
        let nodes: Vec<(&str, CacheNodeWeight)> = nodes.into_iter().collect();
        let mut seen = BTreeSet::new();
        for (name, weight) in &nodes {
            self.check_new_node(name, *weight)?;
            if !seen.insert(*name) {
                return Err(CacheError::InvalidConfig(format!(
                    "cluster node {name} appears twice in one batch"
                )));
            }
        }
        for (name, weight) in nodes {
            self.insert_node(name, weight);
        }
        self.rebuild_ring();
        Ok(())
    }

    fn check_new_node(&self, name: &str, weight: CacheNodeWeight) -> Result<(), CacheError> {
        if name.is_empty() {
            return Err(CacheError::InvalidConfig(
                "a cluster node needs a name, which is what its ring points are built from".into(),
            ));
        }
        if weight == 0 {
            return Err(CacheError::InvalidConfig(format!(
                "cluster node {name} was given a weight of 0; a member that owns \
                 no keys is a node in the Down state, not a node of no weight"
            )));
        }
        if self.nodes.contains_key(name) {
            return Err(CacheError::InvalidConfig(format!(
                "cluster node {name} is already a member"
            )));
        }
        Ok(())
    }

    fn insert_node(&mut self, name: &str, weight: CacheNodeWeight) {
        self.nodes.insert(
            name.to_string(),
            CacheClusterNode {
                name: name.to_string(),
                weight,
                state: CacheNodeState::Live,
            },
        );
    }

    /// Drops a node from the membership entirely. Returns whether it was there.
    pub fn remove_node(&mut self, name: &str) -> bool {
        let removed = self.nodes.remove(name).is_some();
        if removed {
            self.rebuild_ring();
        }
        removed
    }

    /// Marks a member up or down. Returns whether the node is a member.
    pub fn set_node_state(&mut self, name: &str, state: CacheNodeState) -> bool {
        let Some(node) = self.nodes.get_mut(name) else {
            return false;
        };
        if node.state == state {
            return true;
        }
        node.state = state;
        self.rebuild_ring();
        true
    }

    pub fn node(&self, name: &str) -> Option<&CacheClusterNode> {
        self.nodes.get(name)
    }

    /// Every member, live or not, in name order.
    pub fn nodes(&self) -> impl Iterator<Item = &CacheClusterNode> {
        self.nodes.values()
    }

    pub fn member_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn live_node_count(&self) -> usize {
        self.live.len()
    }

    pub fn ring_point_count(&self) -> usize {
        self.ring.len()
    }

    /// The node holding this key, or `None` if no node is live.
    pub fn owner(&self, key: &CacheKey) -> Option<&str> {
        self.owner_of_hash(cache_key_route_hash(key))
    }

    /// The nodes holding this key and its copies, nearest first.
    ///
    /// Walks the ring from the key's own point, taking each new node it meets,
    /// so the copies land on distinct machines. A cluster smaller than the
    /// number asked for returns every node it has and no duplicates: a second
    /// copy on the same node is not a copy.
    pub fn owners(&self, key: &CacheKey, copies: usize) -> Vec<&str> {
        self.owners_of_hash(cache_key_route_hash(key), copies)
    }

    /// Routing for a caller that has chosen its own grouping.
    ///
    /// [`owner`](Self::owner) spreads a key space as widely as it can, which
    /// means two entries of one record can land on different nodes. A caller
    /// that would rather keep a group together hashes the group here -- the
    /// record key alone, say -- and accepts the coarser distribution that comes
    /// with it.
    pub fn owner_of_bytes(&self, key: &[u8]) -> Option<&str> {
        self.owner_of_hash(cache_route_hash(key))
    }

    pub fn owners_of_bytes(&self, key: &[u8], copies: usize) -> Vec<&str> {
        self.owners_of_hash(cache_route_hash(key), copies)
    }

    fn owner_of_hash(&self, hash: u64) -> Option<&str> {
        let point = self.first_point_at_or_after(hash)?;
        Some(self.live[self.ring[point].node as usize].as_str())
    }

    fn owners_of_hash(&self, hash: u64, copies: usize) -> Vec<&str> {
        let wanted = copies.min(self.live.len());
        if wanted == 0 {
            return Vec::new();
        }
        let Some(start) = self.first_point_at_or_after(hash) else {
            return Vec::new();
        };
        let mut owners: Vec<&str> = Vec::with_capacity(wanted);
        for step in 0..self.ring.len() {
            let node = self.ring[(start + step) % self.ring.len()].node as usize;
            let name = self.live[node].as_str();
            if !owners.contains(&name) {
                owners.push(name);
                if owners.len() == wanted {
                    break;
                }
            }
        }
        owners
    }

    /// The first ring point at or after `hash`, wrapping round to the first.
    fn first_point_at_or_after(&self, hash: u64) -> Option<usize> {
        if self.ring.is_empty() {
            return None;
        }
        let found = self.ring.partition_point(|point| point.hash < hash);
        Some(if found == self.ring.len() { 0 } else { found })
    }

    fn rebuild_ring(&mut self) {
        self.live = self
            .nodes
            .values()
            .filter(|node| node.state == CacheNodeState::Live)
            .map(|node| node.name.clone())
            .collect();
        let mut ring = Vec::new();
        for (index, name) in self.live.iter().enumerate() {
            let weight = self.nodes[name].weight;
            let points = self.points_per_weight.saturating_mul(weight);
            let mut label = String::with_capacity(name.len() + 12);
            for point in 0..points {
                label.clear();
                label.push_str(name);
                label.push('#');
                let _ = write!(label, "{point}");
                ring.push(CacheRingPoint {
                    hash: cache_route_hash(label.as_bytes()),
                    node: index as u32,
                });
            }
        }
        // By node as well as by hash: two nodes can land a point on the same
        // hash, and a ring that ordered those by chance would send that arc to
        // different nodes on different machines.
        ring.sort_unstable_by(|left, right| {
            left.hash.cmp(&right.hash).then(left.node.cmp(&right.node))
        });
        self.ring = ring;
    }
}

/// The bytes a [`CacheKey`] is routed by.
///
/// Every variable-length field but the last carries its length, so two
/// different keys cannot produce the same bytes. Concatenating them plainly
/// would let a character move across a boundary -- record key `ab` with
/// selector `c` would encode exactly as record key `a` with selector `bc`, and
/// two keys the rest of this crate treats as different would route to one node
/// and answer for each other.
///
/// A separator byte would not be enough either, unless nothing a key holds
/// could contain that byte. `record_key` and `selector` are `String`s, and a
/// `String` can hold a zero.
///
/// This encoding is part of where data lives. Changing it moves every key in
/// every cluster that uses it.
pub fn cache_key_routing_bytes(key: &CacheKey) -> Vec<u8> {
    let mut bytes =
        Vec::with_capacity(key.namespace.len() + key.record_key.len() + key.selector.len() + 16);
    bytes.extend_from_slice(&(key.namespace.len() as u32).to_le_bytes());
    bytes.extend_from_slice(key.namespace.as_bytes());
    bytes.extend_from_slice(&key.shard_id.to_le_bytes());
    bytes.extend_from_slice(&(key.record_key.len() as u32).to_le_bytes());
    bytes.extend_from_slice(key.record_key.as_bytes());
    // The last field needs no length: nothing follows it to be confused with.
    bytes.extend_from_slice(key.selector.as_bytes());
    bytes
}

/// Where a [`CacheKey`] falls on the ring.
pub fn cache_key_route_hash(key: &CacheKey) -> u64 {
    cache_route_hash(&cache_key_routing_bytes(key))
}

/// The 64-bit hash a cluster routes by.
///
/// Fixed by this function rather than taken from `DefaultHasher`, whose output
/// is documented as changing between releases. Two nodes that disagree about
/// this number disagree about which of them holds a key, and neither of them
/// finds out.
pub fn cache_route_hash(key: &[u8]) -> u64 {
    mur_mur_hash2_64a(key, MT_HASH_SEED)
}

/// MurmurHash2, 64-bit, on a byte string.
///
/// The same mixing constants [`hash_uint64`] already applies to a single
/// integer, over a key of any length.
pub fn mur_mur_hash2_64a(key: &[u8], seed: u64) -> u64 {
    const M: u64 = 0xc6a4_a793_5bd1_e995;
    const R: u32 = 47;

    let mut h = seed ^ (key.len() as u64).wrapping_mul(M);
    let (blocks, tail) = key.as_chunks::<8>();
    for block in blocks {
        let mut k = u64::from_le_bytes(*block);
        k = k.wrapping_mul(M);
        k ^= k >> R;
        k = k.wrapping_mul(M);
        h ^= k;
        h = h.wrapping_mul(M);
    }

    if !tail.is_empty() {
        let mut last = 0_u64;
        for (index, byte) in tail.iter().enumerate() {
            last |= u64::from(*byte) << (8 * index);
        }
        h ^= last;
        h = h.wrapping_mul(M);
    }

    h ^= h >> R;
    h = h.wrapping_mul(M);
    h ^= h >> R;
    h
}
