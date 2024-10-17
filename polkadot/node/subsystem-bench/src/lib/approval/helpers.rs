// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use crate::configuration::TestAuthorities;
use itertools::Itertools;
use polkadot_node_network_protocol::{
	grid_topology::{SessionGridTopology, TopologyPeerInfo},
	View,
};
use polkadot_node_primitives::approval::time::{Clock, SystemClock, Tick};
use polkadot_node_subsystem::messages::{
	ApprovalDistributionMessage, ApprovalVotingParallelMessage,
};
use polkadot_node_subsystem_types::messages::{
	network_bridge_event::NewGossipTopology, NetworkBridgeEvent,
};
use polkadot_overseer::AllMessages;
use polkadot_primitives::{
	vstaging::{CandidateEvent, CandidateReceiptV2 as CandidateReceipt},
	BlockNumber, CoreIndex, GroupIndex, Hash, Header, Id as ParaId, Slot, ValidatorIndex,
};
use polkadot_primitives_test_helpers::dummy_candidate_receipt_bad_sig;
use rand::{seq::SliceRandom, SeedableRng};
use rand_chacha::ChaCha20Rng;
use sc_network_types::PeerId;
use sp_consensus_babe::{
	digests::{CompatibleDigestItem, PreDigest, SecondaryVRFPreDigest},
	AllowedSlots, BabeEpochConfiguration, Epoch as BabeEpoch, VrfSignature, VrfTranscript,
};
use sp_core::crypto::VrfSecret;
use sp_keyring::sr25519::Keyring as Sr25519Keyring;
use sp_runtime::{Digest, DigestItem};
use std::sync::{atomic::AtomicU64, Arc};

/// A fake system clock used for driving the approval voting and make
/// it process blocks, assignments and approvals from the past.
#[derive(Clone)]
pub struct PastSystemClock {
	/// The real system clock
	real_system_clock: SystemClock,
	/// The difference in ticks between the real system clock and the current clock.
	delta_ticks: Arc<AtomicU64>,
}

impl PastSystemClock {
	/// Creates a new fake system clock  with `delta_ticks` between the real time and the fake one.
	pub fn new(real_system_clock: SystemClock, delta_ticks: Arc<AtomicU64>) -> Self {
		PastSystemClock { real_system_clock, delta_ticks }
	}
}

impl Clock for PastSystemClock {
	fn tick_now(&self) -> Tick {
		self.real_system_clock.tick_now() -
			self.delta_ticks.load(std::sync::atomic::Ordering::SeqCst)
	}

	fn wait(
		&self,
		tick: Tick,
	) -> std::pin::Pin<Box<dyn futures::prelude::Future<Output = ()> + Send + 'static>> {
		self.real_system_clock
			.wait(tick + self.delta_ticks.load(std::sync::atomic::Ordering::SeqCst))
	}
}

/// Helper function to generate a  babe epoch for this benchmark.
/// It does not change for the duration of the test.
pub fn generate_babe_epoch(current_slot: Slot, authorities: TestAuthorities) -> BabeEpoch {
	let authorities = authorities
		.validator_babe_id
		.into_iter()
		.enumerate()
		.map(|(index, public)| (public, index as u64))
		.collect_vec();
	BabeEpoch {
		epoch_index: 1,
		start_slot: current_slot.saturating_sub(1u64),
		duration: 200,
		authorities,
		randomness: [0xde; 32],
		config: BabeEpochConfiguration { c: (1, 4), allowed_slots: AllowedSlots::PrimarySlots },
	}
}

/// Generates a topology to be used for this benchmark.
pub fn generate_topology(test_authorities: &TestAuthorities) -> SessionGridTopology {
	let keyrings = test_authorities
		.validator_authority_id
		.clone()
		.into_iter()
		.zip(test_authorities.peer_ids.clone())
		.collect_vec();

	let topology = keyrings
		.clone()
		.into_iter()
		.enumerate()
		.map(|(index, (discovery_id, peer_id))| TopologyPeerInfo {
			peer_ids: vec![peer_id],
			validator_index: ValidatorIndex(index as u32),
			discovery_id,
		})
		.collect_vec();
	let shuffled = (0..keyrings.len()).collect_vec();

	SessionGridTopology::new(shuffled, topology)
}

/// Generates new session topology message.
pub fn generate_new_session_topology(
	test_authorities: &TestAuthorities,
	test_node: ValidatorIndex,
	approval_voting_parallel_enabled: bool,
) -> Vec<AllMessages> {
	let topology = generate_topology(test_authorities);

	let event = NetworkBridgeEvent::NewGossipTopology(NewGossipTopology {
		session: 1,
		topology,
		local_index: Some(test_node),
	});
	vec![if approval_voting_parallel_enabled {
		AllMessages::ApprovalVotingParallel(ApprovalVotingParallelMessage::NetworkBridgeUpdate(
			event,
		))
	} else {
		AllMessages::ApprovalDistribution(ApprovalDistributionMessage::NetworkBridgeUpdate(event))
	}]
}

/// Generates a peer view change for the passed `block_hash`
pub fn generate_peer_view_change_for(
	block_hash: Hash,
	peer_id: PeerId,
	approval_voting_parallel_enabled: bool,
) -> AllMessages {
	let network = NetworkBridgeEvent::PeerViewChange(peer_id, View::new([block_hash], 0));
	if approval_voting_parallel_enabled {
		AllMessages::ApprovalVotingParallel(ApprovalVotingParallelMessage::NetworkBridgeUpdate(
			network,
		))
	} else {
		AllMessages::ApprovalDistribution(ApprovalDistributionMessage::NetworkBridgeUpdate(network))
	}
}

/// Helper function to create a a signature for the block header.
fn garbage_vrf_signature() -> VrfSignature {
	let transcript = VrfTranscript::new(b"test-garbage", &[]);
	Sr25519Keyring::Alice.pair().vrf_sign(&transcript.into())
}

/// Helper function to create a block header.
pub fn make_header(parent_hash: Hash, slot: Slot, number: u32) -> Header {
	let digest =
		{
			let mut digest = Digest::default();
			let vrf_signature = garbage_vrf_signature();
			digest.push(DigestItem::babe_pre_digest(PreDigest::SecondaryVRF(
				SecondaryVRFPreDigest { authority_index: 0, slot, vrf_signature },
			)));
			digest
		};

	Header {
		digest,
		extrinsics_root: Default::default(),
		number,
		state_root: Default::default(),
		parent_hash,
	}
}

/// Helper function to create a candidate receipt.
fn make_candidate(para_id: ParaId, hash: &Hash) -> CandidateReceipt {
	let mut r = dummy_candidate_receipt_bad_sig(*hash, Some(Default::default()));
	r.descriptor.para_id = para_id;
	r.into()
}

/// Helper function to create a list of candidates that are included in the block
pub fn make_candidates(
	block_hash: Hash,
	block_number: BlockNumber,
	num_cores: u32,
	num_candidates: u32,
) -> Vec<CandidateEvent> {
	let seed = [block_number as u8; 32];
	let mut rand_chacha = ChaCha20Rng::from_seed(seed);
	let mut candidates = (0..num_cores)
		.map(|core| {
			CandidateEvent::CandidateIncluded(
				make_candidate(ParaId::from(core), &block_hash),
				Vec::new().into(),
				CoreIndex(core),
				GroupIndex(core),
			)
		})
		.collect_vec();
	let (candidates, _) = candidates.partial_shuffle(&mut rand_chacha, num_candidates as usize);
	candidates
		.iter_mut()
		.map(|val| val.clone())
		.sorted_by(|a, b| match (a, b) {
			(
				CandidateEvent::CandidateIncluded(_, _, core_a, _),
				CandidateEvent::CandidateIncluded(_, _, core_b, _),
			) => core_a.0.cmp(&core_b.0),
			(_, _) => todo!("Should not happen"),
		})
		.collect_vec()
}
