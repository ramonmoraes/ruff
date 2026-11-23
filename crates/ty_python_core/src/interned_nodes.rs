//! Data structures for interning interior nodes.
//!
//! Each graph needs stable IDs for its nodes and a way to reuse an existing ID when the same
//! node is constructed again. The indexed vector owns the nodes; a hash table of IDs provides
//! lookup without storing another copy of each node.

use std::hash::{BuildHasher, Hash};
use std::ops::Index;

use hashbrown::{HashTable, hash_table::Entry};
use ruff_index::{Idx, IndexVec};
use rustc_hash::FxBuildHasher;

use crate::rank::{CHUNK_SIZE, Chunk, RankBitBox, RankBitBoxVec};

/// An indexed collection of distinct nodes with a reverse table of their IDs.
///
/// The vector owns each node at the index given by its stable ID. The hash table stores only
/// IDs, hashing and comparing them by reading the corresponding nodes from the vector. Both
/// collections are private, and [`Self::intern`] updates them together: every table entry
/// refers to a node in the vector, and an equal node reuses that node's ID. Once the graph is
/// built, [`Self::into_used_nodes`] keeps only the nodes marked as used.
#[derive(Debug)]
pub(crate) struct InternedNodes<I: Idx, N> {
    nodes: IndexVec<I, N>,
    ids: HashTable<I>,
    used: Vec<Chunk>,
}

impl<I: Idx, N> Default for InternedNodes<I, N> {
    fn default() -> Self {
        Self {
            nodes: IndexVec::default(),
            ids: HashTable::default(),
            used: Vec::new(),
        }
    }
}

impl<I: Idx, N> InternedNodes<I, N> {
    pub(crate) const fn len(&self) -> usize {
        self.nodes.raw.len()
    }

    /// Marks a node as used and returns whether this is its first visit.
    pub(crate) fn mark_used(&mut self, id: I) -> bool {
        // Use the same most-significant-bit-first order as `RankBitBox`.
        let index = id.index();
        let chunk = &mut self.used[index / CHUNK_SIZE];
        let mask = 1 << (CHUNK_SIZE - 1 - index % CHUNK_SIZE);
        if *chunk & mask != 0 {
            return false;
        }
        *chunk |= mask;
        true
    }

    /// Returns the used nodes in ID order and a rank index if any nodes were removed.
    pub(crate) fn into_used_nodes(self) -> (Box<[N]>, Option<RankBitBox>) {
        drop(self.ids);
        let mut used = RankBitBoxVec::from_vec(self.used);
        used.truncate(self.nodes.len());

        if used.first_zero().is_none() {
            return (self.nodes.raw.into_boxed_slice(), None);
        }

        let nodes = self
            .nodes
            .into_iter()
            .zip(&used)
            .filter_map(|(node, keep)| keep.then_some(node))
            .collect();
        (nodes, Some(RankBitBox::from_bits(used)))
    }
}

impl<I: Idx, N: Eq + Hash> InternedNodes<I, N> {
    pub(crate) fn find(&self, node: &N) -> Option<I> {
        self.ids
            .find(FxBuildHasher.hash_one(node), |id| self.nodes[*id].eq(node))
            .copied()
    }

    /// Returns the existing ID for an equal node, or inserts a new node.
    pub(crate) fn intern(&mut self, node: N) -> I {
        let nodes = &mut self.nodes;
        match self.ids.entry(
            FxBuildHasher.hash_one(&node),
            |id| nodes[*id].eq(&node),
            |id| FxBuildHasher.hash_one(&nodes[*id]),
        ) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let id = nodes.push(node);
                entry.insert(id);
                // Initialize a whole word of unused bits at once.
                if id.index().is_multiple_of(CHUNK_SIZE) {
                    self.used.push(0);
                }
                id
            }
        }
    }
}

impl<I: Idx, N> Index<I> for InternedNodes<I, N> {
    type Output = N;

    fn index(&self, id: I) -> &N {
        &self.nodes[id]
    }
}

#[cfg(test)]
mod tests {
    use super::InternedNodes;
    use crate::reachability_constraints::ScopedReachabilityConstraintId;

    #[test]
    fn compact_across_word_boundaries() {
        let mut nodes = InternedNodes::<ScopedReachabilityConstraintId, _>::default();
        for i in 0..65 {
            let id = nodes.intern(i);
            if matches!(i, 0 | 31 | 32 | 63 | 64) {
                assert!(nodes.mark_used(id));
                assert!(!nodes.mark_used(id));
            }
        }

        let (kept, indices) = nodes.into_used_nodes();
        assert_eq!(&*kept, &[0, 31, 32, 63, 64]);
        let indices = indices.unwrap();
        assert_eq!(indices.len(), 65);
        assert_eq!(indices.rank(64), 4);
    }
}
