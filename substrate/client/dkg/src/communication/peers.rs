use sc_network::ReputationChange;
use sc_network_types::PeerId;
use sp_runtime::traits::Hash;
use std::collections::HashMap;

/// Report specifying a reputation change for a given peer.
#[derive(Debug, PartialEq)]
pub struct PeerReport {
	pub who: PeerId,
	pub cost_benefit: ReputationChange,
}

struct PeerData<H: Hash> {
    // TODO: Is having a Hash enough information about a dealing of a peer?
	last_dealing: H::Output,
}

impl<H: Hash> Default for PeerData<H>
where
    H::Output: Default
{
	fn default() -> Self {
		PeerData { last_dealing: H::Output::default() }
	}
}

/// Keep a simple map of connected peers
/// and the most recent DKG dealing they provided.
pub struct KnownPeers<H: Hash> {
	live: HashMap<PeerId, PeerData<H>>,
}

impl<H: Hash> KnownPeers<H> {
	pub fn new() -> Self {
		Self { live: HashMap::new() }
	}

	/// Note dealing for `peer`.
	pub fn note_vote_for(&mut self, peer: PeerId, dealing: H::Output) {
		let data = self.live.entry(peer).or_default();
		data.last_dealing = dealing;
	}

	/// Remove connected `peer`.
	pub fn remove(&mut self, peer: &PeerId) {
		self.live.remove(peer);
	}

	/// Answer whether `peer` is part of `KnownPeers` set.
	pub fn contains(&self, peer: &PeerId) -> bool {
		self.live.contains_key(peer)
	}

	/// Number of peers in the set.
	pub fn len(&self) -> usize {
		self.live.len()
	}
}