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

//! `ChainModel`: an in-memory replacement for `runtime-api` + `chain-api`.

use crate::{
	contract::Query,
	harness::dispatcher::AnswerQuery,
};
use polkadot_node_subsystem::messages::{ChainApiMessage, RuntimeApiMessage, RuntimeApiRequest};
use polkadot_primitives::{
	async_backing::{AsyncBackingParams, Constraints, InboundHrmpLimitations},
	BlockNumber, CommittedCandidateReceiptV2 as CommittedCandidateReceipt, CoreIndex,
	GroupRotationInfo, Hash, HeadData, Header, Id as ParaId, NodeFeatures, SessionIndex,
	ValidationCodeHash, ValidatorId, ValidatorIndex,
};
use sp_consensus_babe::digests::{CompatibleDigestItem, PreDigest, SecondaryPlainPreDigest};
use sp_consensus_slots::Slot;
use sp_runtime::{Digest, DigestItem};
use std::collections::{BTreeMap, VecDeque};

/// Per-session validator config.
#[derive(Clone, Debug)]
pub struct SessionInfo {
	/// Validator public keys for the session.
	pub validators: Vec<ValidatorId>,
	/// Group memberships.
	pub validator_groups: Vec<Vec<ValidatorIndex>>,
	/// Group rotation info as the runtime would report it.
	pub group_rotation_info: GroupRotationInfo,
}

/// Per-block facts the chain model knows.
#[derive(Clone, Debug)]
pub struct BlockInfo {
	/// Hash of this block.
	pub hash: Hash,
	/// Hash of the parent block. `Hash::zero()` for genesis.
	pub parent_hash: Hash,
	/// Block number.
	pub number: BlockNumber,
	/// BABE slot.
	pub slot: Slot,
	/// Session this block belongs to.
	pub session_index: SessionIndex,
}

impl BlockInfo {
	/// Materialise a `Header` for this block. Includes the BABE pre-digest derived from
	/// `slot` so V3 scheduling-parent validation can extract the slot from the header.
	pub fn header(&self) -> Header {
		let pre_digest =
			PreDigest::SecondaryPlain(SecondaryPlainPreDigest { authority_index: 0, slot: self.slot });
		Header {
			parent_hash: self.parent_hash,
			number: self.number,
			state_root: Default::default(),
			extrinsics_root: Default::default(),
			digest: Digest { logs: vec![DigestItem::babe_pre_digest(pre_digest)] },
		}
	}
}

/// In-memory model of the relay chain.
///
/// Constructed via [`ChainModel::new`] (genesis only) and grown via [`extend`]. Sessions are
/// added via [`add_session`]; per-block claim queues via [`set_claim_queue_at`].
///
/// The model is single-threaded and owned by the [`Sim`]. Test code mutates it directly.
///
/// [`extend`]: ChainModel::extend
/// [`add_session`]: ChainModel::add_session
/// [`set_claim_queue_at`]: ChainModel::set_claim_queue_at
/// [`Sim`]: crate::harness::Sim
#[derive(Debug)]
pub struct ChainModel {
	blocks: BTreeMap<Hash, BlockInfo>,
	children: BTreeMap<Hash, Vec<Hash>>,
	sessions: BTreeMap<SessionIndex, SessionInfo>,
	claim_queues: BTreeMap<Hash, BTreeMap<CoreIndex, VecDeque<ParaId>>>,
	scheduling_lookahead: u32,
	async_backing_params: AsyncBackingParams,
	node_features: NodeFeatures,
	minimum_backing_votes: u32,
	pending_availability: BTreeMap<ParaId, Vec<CommittedCandidateReceipt>>,
	backing_constraints: BTreeMap<ParaId, Constraints>,
	genesis: Hash,
	tip: Hash,
}

impl ChainModel {
	/// New chain model with a single genesis block at slot `genesis_slot`, session 0.
	/// Genesis hash is fixed at `Hash::from_low_u64_be(0xC0FFEE)` so two chain models in
	/// the same test process don't accidentally share / collide.
	pub fn new(genesis_slot: Slot) -> Self {
		let genesis_hash = Hash::from_low_u64_be(0xC0FFEE);
		let genesis_info = BlockInfo {
			hash: genesis_hash,
			parent_hash: Hash::zero(),
			number: 0,
			slot: genesis_slot,
			session_index: 0,
		};
		let mut blocks = BTreeMap::new();
		blocks.insert(genesis_hash, genesis_info);
		Self {
			blocks,
			children: BTreeMap::new(),
			sessions: BTreeMap::new(),
			claim_queues: BTreeMap::new(),
			scheduling_lookahead: 3,
			async_backing_params: AsyncBackingParams {
				max_candidate_depth: 4,
				allowed_ancestry_len: 2,
			},
			node_features: NodeFeatures::EMPTY,
			minimum_backing_votes: 1,
			pending_availability: BTreeMap::new(),
			backing_constraints: BTreeMap::new(),
			genesis: genesis_hash,
			tip: genesis_hash,
		}
	}

	/// Genesis hash.
	pub fn genesis(&self) -> Hash {
		self.genesis
	}

	/// Current tip (most recently extended block).
	pub fn tip(&self) -> Hash {
		self.tip
	}

	/// Look up a block by hash.
	pub fn block(&self, hash: &Hash) -> Option<&BlockInfo> {
		self.blocks.get(hash)
	}

	/// Append a child block onto `parent`. Slot increments by one, session is inherited from
	/// the parent. Returns the new block's hash.
	///
	/// Panics if `parent` is not known.
	pub fn extend(&mut self, parent: Hash) -> Hash {
		let parent_info = self.blocks.get(&parent).cloned().expect("parent block must exist");
		let number = parent_info.number + 1;
		let slot = parent_info.slot + 1;
		let session_index = parent_info.session_index;
		// Deterministic child hash: parent number XOR child number, low-u64.
		let hash = synthetic_child_hash(parent, number);
		let info = BlockInfo { hash, parent_hash: parent, number, slot, session_index };
		self.blocks.insert(hash, info);
		self.children.entry(parent).or_default().push(hash);
		self.tip = hash;
		hash
	}

	/// Install or replace the session info for a session index.
	pub fn add_session(&mut self, session_index: SessionIndex, info: SessionInfo) {
		self.sessions.insert(session_index, info);
	}

	/// Replace the claim queue at a specific block.
	pub fn set_claim_queue_at(
		&mut self,
		block: Hash,
		queue: BTreeMap<CoreIndex, VecDeque<ParaId>>,
	) {
		assert!(self.blocks.contains_key(&block), "claim queue set on unknown block");
		self.claim_queues.insert(block, queue);
	}

	/// Override the scheduling lookahead value runtime returns.
	pub fn set_scheduling_lookahead(&mut self, lookahead: u32) {
		self.scheduling_lookahead = lookahead;
	}

	/// Install backing constraints for a para. If unset the model returns a permissive
	/// default that lets advertisements pass the prospective-parachains acceptance check.
	pub fn set_backing_constraints(&mut self, para: ParaId, constraints: Constraints) {
		self.backing_constraints.insert(para, constraints);
	}

	/// Override the async backing params (max candidate depth, allowed ancestry length).
	pub fn set_async_backing_params(&mut self, params: AsyncBackingParams) {
		self.async_backing_params = params;
	}

	/// Replace the node-features bitvec returned by `RuntimeApi::NodeFeatures`.
	pub fn set_node_features(&mut self, features: NodeFeatures) {
		self.node_features = features;
	}

	/// Override the minimum-backing-votes value returned by
	/// `RuntimeApi::MinimumBackingVotes`.
	pub fn set_minimum_backing_votes(&mut self, votes: u32) {
		self.minimum_backing_votes = votes;
	}

	/// Install the candidates-pending-availability list for a para.
	pub fn set_pending_availability(
		&mut self,
		para: ParaId,
		candidates: Vec<CommittedCandidateReceipt>,
	) {
		self.pending_availability.insert(para, candidates);
	}

	/// Walk ancestry of `from`. Yields parent, grandparent, ... up to (but not including) the
	/// genesis pre-image. Used by `ChainApi::Ancestors`.
	pub fn ancestors(&self, from: Hash, k: usize) -> Vec<Hash> {
		let mut out = Vec::with_capacity(k);
		let mut cursor = from;
		for _ in 0..k {
			match self.blocks.get(&cursor) {
				Some(info) if info.parent_hash != Hash::zero() => {
					out.push(info.parent_hash);
					cursor = info.parent_hash;
				},
				_ => break,
			}
		}
		out
	}

	fn session_info(&self, session_index: SessionIndex) -> &SessionInfo {
		self.sessions
			.get(&session_index)
			.unwrap_or_else(|| panic!("ChainModel: no SessionInfo registered for {}", session_index))
	}

	fn answer_runtime(&self, msg: RuntimeApiMessage) {
		match msg {
			RuntimeApiMessage::Request(parent, req) => self.answer_runtime_req(parent, req),
		}
	}

	fn answer_runtime_req(&self, parent: Hash, req: RuntimeApiRequest) {
		let info = self.blocks.get(&parent).unwrap_or_else(|| {
			panic!("ChainModel: RuntimeApi request for unknown block {:?}", parent)
		});
		match req {
			RuntimeApiRequest::SessionIndexForChild(tx) => {
				let _ = tx.send(Ok(info.session_index));
			},
			RuntimeApiRequest::ClaimQueue(tx) => {
				let queue = self.claim_queues.get(&parent).cloned().unwrap_or_default();
				let _ = tx.send(Ok(queue));
			},
			RuntimeApiRequest::Validators(tx) => {
				let _ = tx.send(Ok(self.session_info(info.session_index).validators.clone()));
			},
			RuntimeApiRequest::ValidatorGroups(tx) => {
				let session = self.session_info(info.session_index);
				let mut rotation = session.group_rotation_info.clone();
				rotation.now = info.number;
				let _ = tx.send(Ok((session.validator_groups.clone(), rotation)));
			},
			RuntimeApiRequest::SchedulingLookahead(_session, tx) => {
				let _ = tx.send(Ok(self.scheduling_lookahead));
			},
			RuntimeApiRequest::AsyncBackingParams(tx) => {
				let _ = tx.send(Ok(self.async_backing_params.clone()));
			},
			RuntimeApiRequest::NodeFeatures(_session, tx) => {
				let _ = tx.send(Ok(self.node_features.clone()));
			},
			RuntimeApiRequest::MinimumBackingVotes(_session, tx) => {
				let _ = tx.send(Ok(self.minimum_backing_votes));
			},
			RuntimeApiRequest::BackingConstraints(para, tx) => {
				let constraints = self
					.backing_constraints
					.get(&para)
					.cloned()
					.unwrap_or_else(default_constraints);
				let _ = tx.send(Ok(Some(constraints)));
			},
			RuntimeApiRequest::CandidatesPendingAvailability(para, tx) => {
				let candidates = self.pending_availability.get(&para).cloned().unwrap_or_default();
				let _ = tx.send(Ok(candidates));
			},
			RuntimeApiRequest::DisabledValidators(tx) => {
				let _ = tx.send(Ok(Vec::new()));
			},
			RuntimeApiRequest::AvailabilityCores(tx) => {
				let _ = tx.send(Ok(Vec::new()));
			},
			RuntimeApiRequest::SessionInfo(_session, tx) => {
				let _ = tx.send(Ok(None));
			},
			RuntimeApiRequest::SessionExecutorParams(_session, tx) => {
				let _ = tx.send(Ok(None));
			},
			RuntimeApiRequest::ParaIds(_session, tx) => {
				let _ = tx.send(Ok(Vec::new()));
			},
			RuntimeApiRequest::ValidationCodeBombLimit(_session, tx) => {
				let _ = tx.send(Ok(60 * 1024 * 1024));
			},
			RuntimeApiRequest::MaxRelayParentSessionAge(_session, tx) => {
				let _ = tx.send(Ok(8));
			},
			other => panic!(
				"ChainModel does not implement RuntimeApiRequest::{:?} yet — extend the model when a subsystem starts asking for it",
				other
			),
		}
	}

	fn answer_chain_api(&self, msg: ChainApiMessage) {
		match msg {
			ChainApiMessage::BlockHeader(hash, tx) => {
				let header = self.blocks.get(&hash).map(BlockInfo::header);
				let _ = tx.send(Ok(header));
			},
			ChainApiMessage::BlockNumber(hash, tx) => {
				let number = self.blocks.get(&hash).map(|info| info.number);
				let _ = tx.send(Ok(number));
			},
			ChainApiMessage::Ancestors { hash, k, response_channel } => {
				let ancestors = self.ancestors(hash, k);
				let _ = response_channel.send(Ok(ancestors));
			},
			ChainApiMessage::FinalizedBlockNumber(tx) => {
				// No finalized block tracking yet — pretend genesis is the latest finalized.
				let _ = tx.send(Ok(0));
			},
			ChainApiMessage::FinalizedBlockHash(number, tx) => {
				// Map any finalized-block-number lookup to genesis if number == 0.
				let hash = if number == 0 { Some(self.genesis) } else { None };
				let _ = tx.send(Ok(hash));
			},
			other => panic!(
				"ChainModel does not implement ChainApiMessage::{:?} yet — extend the model when a subsystem starts asking for it",
				other
			),
		}
	}
}

impl AnswerQuery for ChainModel {
	fn try_answer(&mut self, query: Query) -> Option<Query> {
		match query {
			Query::Runtime(msg) => {
				self.answer_runtime(msg);
				None
			},
			Query::ChainApi(msg) => {
				self.answer_chain_api(msg);
				None
			},
			other => Some(other),
		}
	}

	fn answer(&mut self, query: Query) {
		// Direct-answer surface for unit tests; declines fall through to a panic so test
		// authors notice when they hand the chain model a query it doesn't own.
		if let Some(declined) = self.try_answer(query) {
			panic!(
				"ChainModel does not handle non-runtime/chain-api queries; declined: {:?}",
				declined
			);
		}
	}
}

/// Shared ownership of a [`ChainModel`].
///
/// The harness installs this in its responder chain so the model's mutable state stays
/// accessible to the test (via [`SharedChain::lock`]) while queries the subsystem fires
/// against the responder are routed back into the same model.
#[derive(Clone, Debug)]
pub struct SharedChain(std::sync::Arc<std::sync::Mutex<ChainModel>>);

impl SharedChain {
	/// Wrap a chain model for shared use.
	pub fn new(chain: ChainModel) -> Self {
		Self(std::sync::Arc::new(std::sync::Mutex::new(chain)))
	}

	/// Lock the inner model. Panics if poisoned.
	pub fn lock(&self) -> std::sync::MutexGuard<'_, ChainModel> {
		self.0.lock().expect("ChainModel mutex poisoned")
	}
}

impl AnswerQuery for SharedChain {
	fn try_answer(&mut self, query: Query) -> Option<Query> {
		self.lock().try_answer(query)
	}

	fn answer(&mut self, query: Query) {
		self.lock().answer(query)
	}
}

/// Permissive default backing constraints. Tests that need stricter shapes pass their own
/// via [`ChainModel::set_backing_constraints`].
fn default_constraints() -> Constraints {
	Constraints {
		min_relay_parent_number: 0,
		max_pov_size: 5 * 1024 * 1024,
		max_code_size: 3 * 1024 * 1024,
		max_head_data_size: 20 * 1024,
		ump_remaining: 32,
		ump_remaining_bytes: 64 * 1024,
		max_ump_num_per_candidate: 16,
		dmp_remaining_messages: Vec::new(),
		hrmp_inbound: InboundHrmpLimitations { valid_watermarks: Vec::new() },
		hrmp_channels_out: Vec::new(),
		max_hrmp_num_per_candidate: 16,
		required_parent: HeadData(Vec::new()),
		validation_code_hash: ValidationCodeHash::from(Hash::zero()),
		upgrade_restriction: None,
		future_validation_code: None,
	}
}

fn synthetic_child_hash(parent: Hash, number: BlockNumber) -> Hash {
	// Deterministic child hash by mixing parent low-u64 with the child number. Tests do not
	// assert on the exact value; identity is what matters.
	let parent_low = parent.to_low_u64_be();
	Hash::from_low_u64_be(parent_low.wrapping_add(0x100_0000 + number as u64))
}

#[cfg(test)]
mod tests {
	use super::*;

	fn empty_session() -> SessionInfo {
		SessionInfo {
			validators: Vec::new(),
			validator_groups: Vec::new(),
			group_rotation_info: GroupRotationInfo {
				session_start_block: 0,
				group_rotation_frequency: 1,
				now: 0,
			},
		}
	}

	#[test]
	fn extend_grows_chain_and_advances_slot() {
		let mut chain = ChainModel::new(Slot::from(100));
		let g = chain.genesis();
		let a = chain.extend(g);
		let b = chain.extend(a);
		assert_eq!(chain.block(&a).unwrap().parent_hash, g);
		assert_eq!(chain.block(&b).unwrap().parent_hash, a);
		assert_eq!(chain.block(&a).unwrap().number, 1);
		assert_eq!(chain.block(&b).unwrap().number, 2);
		assert_eq!(chain.block(&a).unwrap().slot, Slot::from(101));
		assert_eq!(chain.block(&b).unwrap().slot, Slot::from(102));
		assert_eq!(chain.tip(), b);
	}

	#[test]
	fn ancestors_walks_back_until_genesis() {
		let mut chain = ChainModel::new(Slot::from(100));
		let a = chain.extend(chain.genesis());
		let b = chain.extend(a);
		let c = chain.extend(b);
		let anc = chain.ancestors(c, 5);
		assert_eq!(anc, vec![b, a, chain.genesis()]);
	}

	#[test]
	fn runtime_session_index_for_child_returns_block_session() {
		let mut chain = ChainModel::new(Slot::from(0));
		chain.add_session(0, empty_session());
		let leaf = chain.extend(chain.genesis());
		let (tx, rx) = futures::channel::oneshot::channel();
		chain.answer(Query::Runtime(RuntimeApiMessage::Request(
			leaf,
			RuntimeApiRequest::SessionIndexForChild(tx),
		)));
		let got = futures::executor::block_on(rx).unwrap().unwrap();
		assert_eq!(got, 0);
	}

	#[test]
	fn runtime_validator_groups_uses_session_info() {
		let mut chain = ChainModel::new(Slot::from(0));
		let mut session = empty_session();
		session.validator_groups = vec![vec![ValidatorIndex(0)]];
		chain.add_session(0, session);
		let leaf = chain.extend(chain.genesis());
		let (tx, rx) = futures::channel::oneshot::channel();
		chain.answer(Query::Runtime(RuntimeApiMessage::Request(
			leaf,
			RuntimeApiRequest::ValidatorGroups(tx),
		)));
		let (groups, rotation) = futures::executor::block_on(rx).unwrap().unwrap();
		assert_eq!(groups, vec![vec![ValidatorIndex(0)]]);
		assert_eq!(rotation.now, 1); // leaf number.
	}

	#[test]
	fn runtime_claim_queue_returns_per_block_queue() {
		let mut chain = ChainModel::new(Slot::from(0));
		chain.add_session(0, empty_session());
		let leaf = chain.extend(chain.genesis());
		let mut q: BTreeMap<CoreIndex, VecDeque<ParaId>> = BTreeMap::new();
		q.insert(CoreIndex(0), VecDeque::from_iter(std::iter::repeat(ParaId::from(2000)).take(3)));
		chain.set_claim_queue_at(leaf, q.clone());
		let (tx, rx) = futures::channel::oneshot::channel();
		chain.answer(Query::Runtime(RuntimeApiMessage::Request(
			leaf,
			RuntimeApiRequest::ClaimQueue(tx),
		)));
		let got = futures::executor::block_on(rx).unwrap().unwrap();
		assert_eq!(got, q);
	}

	#[test]
	fn chain_api_block_header_round_trips_slot() {
		let mut chain = ChainModel::new(Slot::from(42));
		let leaf = chain.extend(chain.genesis());
		let (tx, rx) = futures::channel::oneshot::channel();
		chain.answer(Query::ChainApi(ChainApiMessage::BlockHeader(leaf, tx)));
		let header = futures::executor::block_on(rx).unwrap().unwrap().expect("header present");
		// Extract slot back out.
		let slot = header
			.digest
			.logs()
			.iter()
			.find_map(|log| log.as_babe_pre_digest())
			.expect("BABE pre-digest present")
			.slot();
		assert_eq!(slot, Slot::from(43)); // genesis slot 42 + 1.
	}

	#[test]
	fn chain_api_ancestors_returns_walk() {
		let mut chain = ChainModel::new(Slot::from(0));
		let a = chain.extend(chain.genesis());
		let b = chain.extend(a);
		let (tx, rx) = futures::channel::oneshot::channel();
		chain.answer(Query::ChainApi(ChainApiMessage::Ancestors {
			hash: b,
			k: 2,
			response_channel: tx,
		}));
		let got = futures::executor::block_on(rx).unwrap().unwrap();
		assert_eq!(got, vec![a, chain.genesis()]);
	}
}
