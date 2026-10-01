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

/// Most bits of a hash the lookup table is indexed by.
///
/// The table holds one `u32` per slot, so sixteen bits is 256 KiB. Past that it
/// costs more memory than the walk it saves, which is the thing it exists to
/// avoid.
const CACHE_RING_INDEX_MAX_BITS: u32 = 16;
/// Fewest, so a one-node cluster does not carry a table larger than its ring.
const CACHE_RING_INDEX_MIN_BITS: u32 = 4;

/// Points a slot is aimed at holding.
///
/// One per slot is the obvious target and it buys nothing: four still fit in a
/// cache line, so the scan is the same single miss, and the table is a quarter
/// the size. At 64 nodes that is 16 KiB of table against a 160 KiB ring rather
/// than 64 KiB.
const CACHE_RING_INDEX_POINTS_PER_SLOT: usize = 4;

/// Bits to index a ring of `points` by.
///
/// A ring too large for [`CACHE_RING_INDEX_POINTS_PER_SLOT`] at the cap gets the
/// cap and a longer scan -- ten points at four thousand nodes, which is two
/// cache lines rather than the twenty a binary search over the same ring
/// touches.
fn cache_ring_index_bits(points: usize) -> u32 {
    let slots_wanted = points.div_ceil(CACHE_RING_INDEX_POINTS_PER_SLOT).max(1);
    let wanted = usize::BITS - slots_wanted.leading_zeros();
    wanted.clamp(CACHE_RING_INDEX_MIN_BITS, CACHE_RING_INDEX_MAX_BITS)
}

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
    /// What this node fails together with -- a rack, a power domain, an
    /// availability zone, whatever the thing is that takes its members down at
    /// once. A key's copies are placed in different ones.
    ///
    /// `None` means this node is its own domain, which is the safe reading: it
    /// makes copies spread as widely as the cluster allows. Saying nothing
    /// therefore costs nothing, and a wrong zone is worse than no zone, because
    /// it claims a separation that is not there.
    #[serde(default)]
    pub zone: Option<String>,
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
    /// Every member's name, in the order the ring points index them.
    ///
    /// All members, not only the live ones. A node going down does not change
    /// any hash and does not change the order of the points that remain, so the
    /// ring does not have to be rebuilt for it -- lookups step over what is not
    /// live instead. Rebuilding cost 94ms at four thousand nodes, half of it
    /// hashing points whose hashes had not changed.
    members: Vec<String>,
    /// Whether each member is taking traffic, at the same index.
    member_live: Vec<bool>,
    /// The failure domain of each member, as an index. Two members share a
    /// number when they share a zone; one with no zone gets a number of its own.
    member_domain: Vec<u32>,
    /// How many members are live. Kept because a lookup needs to know whether
    /// stepping over the not-live ones can ever terminate.
    live_count: usize,
    /// How many distinct values `live_domain` holds. Counted once per rebuild
    /// because every copy placement needs it and a lookup must not walk the
    /// membership to find it.
    domain_count: usize,
    /// Sorted by hash, then by node, so a hash collision between two nodes
    /// resolves the same way on every machine that builds this ring.
    ring: Vec<CacheRingPoint>,
    /// Where in the ring each hash prefix begins.
    ///
    /// `prefix_index[slot]` is the first ring point whose hash is at or above
    /// that slot's start, and there is one extra entry holding the ring's
    /// length. A lookup reads one slot and then scans the handful of points
    /// inside it, instead of binary searching the whole ring.
    ///
    /// The searching was never the cost. A binary search over a ten megabyte
    /// ring is twenty probes, each landing on a different cache line and each
    /// waiting on the one before it -- twenty dependent misses. The table makes
    /// it one miss for the slot and one or two for the scan.
    prefix_index: Vec<u32>,
    /// Bits of a hash [`Self::prefix_index`] is indexed by.
    index_bits: u32,
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
            members: Vec::new(),
            member_live: Vec::new(),
            member_domain: Vec::new(),
            live_count: 0,
            domain_count: 0,
            ring: Vec::new(),
            prefix_index: Vec::new(),
            index_bits: CACHE_RING_INDEX_MIN_BITS,
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
        self.insert_node(name, weight, None);
        self.rebuild_ring();
        Ok(())
    }

    /// Adds a live node that shares a failure domain with the others in `zone`.
    ///
    /// An empty zone name is refused rather than read as "no zone": a caller
    /// passing one has a zone it failed to find, and treating that node as its
    /// own domain would quietly claim a separation nobody established.
    pub fn add_node_in_zone(
        &mut self,
        name: &str,
        weight: CacheNodeWeight,
        zone: &str,
    ) -> Result<(), CacheError> {
        self.check_new_node(name, weight)?;
        if zone.is_empty() {
            return Err(CacheError::InvalidConfig(format!(
                "cluster node {name} was given an empty zone name; a node whose \
                 failure domain is unknown is added without one, which places it \
                 in a domain of its own"
            )));
        }
        self.insert_node(name, weight, Some(zone.to_string()));
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
            self.insert_node(name, weight, None);
        }
        self.rebuild_ring();
        Ok(())
    }

    /// Adds several nodes with their zones and rebuilds the ring once.
    ///
    /// The bulk form of [`add_node_in_zone`](Self::add_node_in_zone), with the
    /// same all-or-nothing checking as [`add_nodes`](Self::add_nodes).
    pub fn add_nodes_in_zones<'a, I>(&mut self, nodes: I) -> Result<(), CacheError>
    where
        I: IntoIterator<Item = (&'a str, CacheNodeWeight, &'a str)>,
    {
        let nodes: Vec<(&str, CacheNodeWeight, &str)> = nodes.into_iter().collect();
        let mut seen = BTreeSet::new();
        for (name, weight, zone) in &nodes {
            self.check_new_node(name, *weight)?;
            if zone.is_empty() {
                return Err(CacheError::InvalidConfig(format!(
                    "cluster node {name} was given an empty zone name"
                )));
            }
            if !seen.insert(*name) {
                return Err(CacheError::InvalidConfig(format!(
                    "cluster node {name} appears twice in one batch"
                )));
            }
        }
        for (name, weight, zone) in nodes {
            self.insert_node(name, weight, Some(zone.to_string()));
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

    fn insert_node(&mut self, name: &str, weight: CacheNodeWeight, zone: Option<String>) {
        self.nodes.insert(
            name.to_string(),
            CacheClusterNode {
                name: name.to_string(),
                weight,
                state: CacheNodeState::Live,
                zone,
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
        // No hash changed and no point moved, so the ring and its index stand.
        self.refresh_live();
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
        self.live_count
    }

    /// How many things can fail independently.
    ///
    /// Nodes sharing a zone count once between them; a node with no zone counts
    /// on its own. This is the ceiling on how many copies of a key can be kept
    /// apart, so a cluster of thirty nodes in two racks can keep two.
    pub fn failure_domain_count(&self) -> usize {
        self.domain_count
    }

    /// Whether the ring point at `index` belongs to a member taking traffic.
    fn point_is_live(&self, index: usize) -> bool {
        self.member_live[self.ring[index].node as usize]
    }

    /// The member owning the ring point at `index`.
    fn point_member(&self, index: usize) -> &str {
        self.members[self.ring[index].node as usize].as_str()
    }

    /// The first live ring point at or after `index`, wrapping once.
    ///
    /// A member that is down keeps its points -- rebuilding the ring to take
    /// them out would mean rehashing every point in it -- so a lookup steps
    /// over them. With nothing down this never steps at all, and with a tenth
    /// of a cluster down it steps about once, usually within the same cache
    /// line the slot scan already touched.
    fn first_live_point_from(&self, index: usize) -> Option<usize> {
        if self.live_count == 0 {
            return None;
        }
        let points = self.ring.len();
        for step in 0..points {
            let candidate = (index + step) % points;
            if self.point_is_live(candidate) {
                return Some(candidate);
            }
        }
        None
    }

    /// Points on the ring, over every member.
    ///
    /// Including members that are down: their points stay put, because taking
    /// them out would mean rehashing the ring, and a lookup steps over them
    /// instead. So this does not move when a node goes up or down -- only when
    /// one joins or leaves.
    pub fn ring_point_count(&self) -> usize {
        self.ring.len()
    }

    /// Slots in the ring's lookup table.
    ///
    /// Four bytes each, so this is what the table costs. Reported because it is
    /// memory a caller did not ask for and should be able to see.
    pub fn ring_index_slots(&self) -> usize {
        self.prefix_index.len().saturating_sub(1)
    }

    /// The node holding this key, or `None` if no node is live.
    pub fn owner(&self, key: &CacheKey) -> Option<&str> {
        self.owner_of_hash(cache_key_route_hash(key))
    }

    /// The nodes holding this key and its copies, nearest first.
    ///
    /// Walks the ring from the key's own point and takes each node it meets
    /// whose failure domain it has not used yet, so a rack losing power does
    /// not take every copy of a key with it.
    ///
    /// When the copies asked for outnumber the domains available, the ones that
    /// cannot be separated are placed anyway, on distinct nodes, in the order
    /// the ring met them. A third copy in a domain already used is worth less
    /// than a third copy somewhere else and more than no third copy, and
    /// [`failure_domain_count`](Self::failure_domain_count) is how a caller
    /// finds out which it is getting.
    ///
    /// A cluster smaller than the number asked for returns every node it has
    /// and no duplicates: a second copy on the same node is not a copy.
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

    /// The node holding a given point of the hash space.
    ///
    /// What [`owner`](Self::owner) resolves a key to. A caller working through
    /// a [`CacheHandoff`] has hashes rather than keys -- that is what the
    /// ranges are in -- and this is how it confirms where one belongs.
    pub fn owner_of_route_hash(&self, hash: u64) -> Option<&str> {
        let point = self.first_point_at_or_after(hash)?;
        let live = self.first_live_point_from(point)?;
        Some(self.point_member(live))
    }

    fn owner_of_hash(&self, hash: u64) -> Option<&str> {
        self.owner_of_route_hash(hash)
    }

    fn owners_of_hash(&self, hash: u64, copies: usize) -> Vec<&str> {
        let wanted = copies.min(self.live_count);
        if wanted == 0 {
            return Vec::new();
        }
        let Some(start) = self.first_point_at_or_after(hash) else {
            return Vec::new();
        };
        let mut owners: Vec<&str> = Vec::with_capacity(wanted);
        let mut used_domains: Vec<u32> = Vec::with_capacity(wanted);
        // Nodes the ring offered but whose domain was already spoken for. Kept
        // in the order they were met, so falling back to them is still the
        // ring's order and not an arbitrary one.
        let mut crowded: Vec<&str> = Vec::new();
        for step in 0..self.ring.len() {
            let point = (start + step) % self.ring.len();
            if !self.point_is_live(point) {
                continue;
            }
            let node = self.ring[point].node as usize;
            let name = self.members[node].as_str();
            if owners.contains(&name) || crowded.contains(&name) {
                continue;
            }
            let domain = self.member_domain[node];
            if used_domains.contains(&domain) {
                crowded.push(name);
            } else {
                owners.push(name);
                used_domains.push(domain);
                if owners.len() == wanted {
                    break;
                }
            }
            // Enough distinct nodes to fill the request, and no domain left
            // that walking further could reach -- so there is nothing better to
            // find, and no reason to walk the rest of a ring that may hold a
            // hundred thousand points.
            //
            // Both halves are needed. Stopping on the count alone takes the
            // first crowded node the ring offers while a node in an unused
            // domain is sitting a few points further on, which is the placement
            // this whole walk exists to avoid.
            if owners.len() + crowded.len() >= wanted && used_domains.len() == self.domain_count {
                break;
            }
        }
        for name in crowded {
            if owners.len() == wanted {
                break;
            }
            owners.push(name);
        }
        owners
    }

    /// The first ring point at or after `hash`, wrapping round to the first.
    ///
    /// One read of the lookup table, then a scan within that slot. Every point
    /// in a slot shares the hash's leading bits, so a point at or after `hash`
    /// is either inside this slot or is the slot's end -- which is already the
    /// first point of the next one. Nothing before the slot can be the answer
    /// and nothing after it can be a nearer one.
    fn first_point_at_or_after(&self, hash: u64) -> Option<usize> {
        if self.ring.is_empty() {
            return None;
        }
        let slot = (hash >> (u64::BITS - self.index_bits)) as usize;
        let mut point = self.prefix_index[slot] as usize;
        let end = self.prefix_index[slot + 1] as usize;
        while point < end && self.ring[point].hash < hash {
            point += 1;
        }
        Some(if point == self.ring.len() { 0 } else { point })
    }

    fn rebuild_ring(&mut self) {
        self.members = self.nodes.values().map(|node| node.name.clone()).collect();
        // Named zones share a number; an unzoned member gets one nobody else
        // has, which is what makes "no zone" mean "its own domain" rather than
        // "the same domain as every other member that said nothing".
        let mut zone_ids: BTreeMap<String, u32> = BTreeMap::new();
        let mut next_id = 0_u32;
        self.member_domain = self
            .nodes
            .values()
            .map(|node| match node.zone.clone() {
                Some(zone) => *zone_ids.entry(zone).or_insert_with(|| {
                    let id = next_id;
                    next_id += 1;
                    id
                }),
                None => {
                    let id = next_id;
                    next_id += 1;
                    id
                }
            })
            .collect();
        let mut ring = Vec::new();
        for (index, name) in self.members.iter().enumerate() {
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
        self.rebuild_prefix_index();
        self.refresh_live();
    }

    /// Recompute what is live, without touching the ring.
    ///
    /// All a node going up or down changes. Costs one pass over the membership
    /// rather than a rehash and a sort of every ring point.
    fn refresh_live(&mut self) {
        self.member_live = self
            .nodes
            .values()
            .map(|node| node.state == CacheNodeState::Live)
            .collect();
        self.live_count = self.member_live.iter().filter(|live| **live).count();
        self.domain_count = self
            .member_domain
            .iter()
            .zip(&self.member_live)
            .filter(|(_, live)| **live)
            .map(|(domain, _)| *domain)
            .collect::<BTreeSet<_>>()
            .len();
        debug_assert_eq!(self.member_live.len(), self.members.len());
        debug_assert_eq!(self.member_domain.len(), self.members.len());
    }

    /// Rebuild the lookup table for the ring as it now stands.
    ///
    /// Walks the slots and the ring together once, so this costs the ring's
    /// length plus the table's and not a search per slot.
    fn rebuild_prefix_index(&mut self) {
        self.index_bits = cache_ring_index_bits(self.ring.len());
        let slots = 1usize << self.index_bits;
        self.prefix_index.clear();
        self.prefix_index.reserve(slots + 1);
        let mut point = 0usize;
        for slot in 0..slots {
            let slot_start = (slot as u64) << (u64::BITS - self.index_bits);
            while point < self.ring.len() && self.ring[point].hash < slot_start {
                point += 1;
            }
            self.prefix_index.push(point as u32);
        }
        // One past the end, so a lookup in the last slot has somewhere to stop.
        self.prefix_index.push(self.ring.len() as u32);
        debug_assert_eq!(self.prefix_index.len(), slots + 1);
    }
}

/// A stretch of the hash space, both ends included.
///
/// `start` above `end` means the stretch wraps past the top of the space and
/// continues from zero, which the last one on a ring always does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheHashRange {
    pub start: u64,
    pub end: u64,
}

impl CacheHashRange {
    /// Whether a key's route hash falls in this stretch.
    pub fn contains(&self, hash: u64) -> bool {
        if self.start <= self.end {
            hash >= self.start && hash <= self.end
        } else {
            hash >= self.start || hash <= self.end
        }
    }

    /// How many hashes this stretch covers.
    ///
    /// A `u128` because a range covering the whole space holds `u64::MAX + 1`
    /// of them, which is the one value a `u64` cannot hold.
    pub fn count(&self) -> u128 {
        if self.start <= self.end {
            u128::from(self.end - self.start) + 1
        } else {
            u128::from(u64::MAX - self.start) + 1 + u128::from(self.end) + 1
        }
    }
}

/// Data one node has to send another for a membership change to take effect.
///
/// The ring says where a key belongs; this says what that costs. Without it a
/// node that gains keys gains misses instead, and serves nothing until the
/// misses have refilled it from underneath.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheHandoff {
    /// The node holding the keys now.
    pub from: String,
    /// The node that will hold them.
    pub to: String,
    /// The stretches of hash space that move. A key moves exactly when its
    /// [`cache_key_route_hash`] falls in one of them.
    pub ranges: Vec<CacheHashRange>,
}

impl CacheHandoff {
    /// How many hashes this handoff moves, across all its stretches.
    pub fn count(&self) -> u128 {
        self.ranges.iter().map(CacheHashRange::count).sum()
    }
}

impl CacheClusterTopology {
    /// What has to move to get from this membership to `next`.
    ///
    /// Ownership only changes at a ring point, so the two rings' points laid
    /// together cut the hash space into stretches on which both memberships
    /// agree with themselves. Every stretch whose owner differs between the two
    /// is work for somebody; the rest is not, and the ring is built so that
    /// most of it is the rest.
    ///
    /// Stretches that move between the same pair of nodes are gathered into one
    /// handoff, and neighbouring stretches of that pair are joined, so a caller
    /// gets a list to work through rather than one entry per ring point.
    ///
    /// Empty when nothing moves, and when either membership has no live node --
    /// there is no sending keys to nowhere, and nothing to send if the keys had
    /// no owner to begin with.
    pub fn handoffs_to(&self, next: &Self) -> Vec<CacheHandoff> {
        if self.ring.is_empty() || next.ring.is_empty() {
            return Vec::new();
        }

        let mut bounds: Vec<u64> = self
            .ring
            .iter()
            .chain(next.ring.iter())
            .map(|point| point.hash)
            .collect();
        bounds.sort_unstable();
        bounds.dedup();

        // One stretch per boundary: the hashes from just past the previous
        // boundary up to and including this one. The first boundary's stretch
        // is the one that wraps, which is why it starts at the last boundary.
        let mut moved: Vec<(CacheHashRange, &str, &str)> = Vec::new();
        for (index, &end) in bounds.iter().enumerate() {
            let previous = if index == 0 {
                bounds[bounds.len() - 1]
            } else {
                bounds[index - 1]
            };
            // A single boundary covers the whole space by wrapping onto itself.
            let start = previous.wrapping_add(1);
            let (Some(before), Some(after)) = (self.owner_of_hash(end), next.owner_of_hash(end))
            else {
                continue;
            };
            if before == after {
                continue;
            }
            moved.push((CacheHashRange { start, end }, before, after));
        }

        // Join stretches that touch and move between the same two nodes. Walked
        // in boundary order, so "touches" is the previous one ending exactly
        // where this one starts.
        let mut joined: Vec<(CacheHashRange, &str, &str)> = Vec::new();
        for (range, from, to) in moved {
            match joined.last_mut() {
                Some((last, last_from, last_to))
                    if *last_from == from
                        && *last_to == to
                        && last.end.wrapping_add(1) == range.start =>
                {
                    last.end = range.end;
                }
                _ => joined.push((range, from, to)),
            }
        }
        // The first and last stretches are neighbours too, round the wrap.
        if joined.len() > 1 {
            let (last_range, last_from, last_to) = *joined.last().expect("checked above");
            let (first_range, first_from, first_to) = joined[0];
            if last_from == first_from
                && last_to == first_to
                && last_range.end.wrapping_add(1) == first_range.start
            {
                joined[0] = (
                    CacheHashRange {
                        start: last_range.start,
                        end: first_range.end,
                    },
                    first_from,
                    first_to,
                );
                joined.pop();
            }
        }

        let mut handoffs: Vec<CacheHandoff> = Vec::new();
        for (range, from, to) in joined {
            match handoffs
                .iter_mut()
                .find(|handoff| handoff.from == from && handoff.to == to)
            {
                Some(handoff) => handoff.ranges.push(range),
                None => handoffs.push(CacheHandoff {
                    from: from.to_string(),
                    to: to.to_string(),
                    ranges: vec![range],
                }),
            }
        }
        handoffs
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
