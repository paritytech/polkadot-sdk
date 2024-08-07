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

//! Staging Primitives.
use crate::{CandidateReceipt, CommittedCandidateReceipt, ValidityAttestation};

// Put any primitives used by staging APIs functions here
use super::{
	Balance, BlakeTwo256, BlockNumber, CandidateCommitments, CandidateDescriptor, CandidateHash,
	CollatorId, CollatorSignature, CoreIndex, Hash, HashT, Id, Id as ParaId, ValidationCodeHash,
	ON_DEMAND_DEFAULT_QUEUE_MAX_SIZE,
};
use bitvec::prelude::*;
use sp_application_crypto::ByteArray;

use alloc::collections::{btree_map::BTreeMap, vec_deque::VecDeque};
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_arithmetic::Perbill;
use sp_core::RuntimeDebug;
use sp_staking::SessionIndex;

/// Scheduler configuration parameters. All coretime/ondemand parameters are here.
#[derive(
	RuntimeDebug,
	Copy,
	Clone,
	PartialEq,
	Encode,
	Decode,
	TypeInfo,
	serde::Serialize,
	serde::Deserialize,
)]
pub struct SchedulerParams<BlockNumber> {
	/// How often parachain groups should be rotated across parachains.
	///
	/// Must be non-zero.
	pub group_rotation_frequency: BlockNumber,
	/// Availability timeout for a block on a core, measured in blocks.
	///
	/// This is the maximum amount of blocks after a core became occupied that validators have time
	/// to make the block available.
	///
	/// This value only has effect on group rotations. If backers backed something at the end of
	/// their rotation, the occupied core affects the backing group that comes afterwards. We limit
	/// the effect one backing group can have on the next to `paras_availability_period` blocks.
	///
	/// Within a group rotation there is no timeout as backers are only affecting themselves.
	///
	/// Must be at least 1. With a value of 1, the previous group will not be able to negatively
	/// affect the following group at the expense of a tight availability timeline at group
	/// rotation boundaries.
	pub paras_availability_period: BlockNumber,
	/// The maximum number of validators to have per core.
	///
	/// `None` means no maximum.
	pub max_validators_per_core: Option<u32>,
	/// The amount of blocks ahead to schedule paras.
	pub lookahead: u32,
	/// How many cores are managed by the coretime chain.
	pub num_cores: u32,
	/// The max number of times a claim can time out in availability.
	pub max_availability_timeouts: u32,
	/// The maximum queue size of the pay as you go module.
	pub on_demand_queue_max_size: u32,
	/// The target utilization of the spot price queue in percentages.
	pub on_demand_target_queue_utilization: Perbill,
	/// How quickly the fee rises in reaction to increased utilization.
	/// The lower the number the slower the increase.
	pub on_demand_fee_variability: Perbill,
	/// The minimum amount needed to claim a slot in the spot pricing queue.
	pub on_demand_base_fee: Balance,
	/// The number of blocks a claim stays in the scheduler's claim queue before getting cleared.
	/// This number should go reasonably higher than the number of blocks in the async backing
	/// lookahead.
	pub ttl: BlockNumber,
}

impl<BlockNumber: Default + From<u32>> Default for SchedulerParams<BlockNumber> {
	fn default() -> Self {
		Self {
			group_rotation_frequency: 1u32.into(),
			paras_availability_period: 1u32.into(),
			max_validators_per_core: Default::default(),
			lookahead: 1,
			num_cores: Default::default(),
			max_availability_timeouts: Default::default(),
			on_demand_queue_max_size: ON_DEMAND_DEFAULT_QUEUE_MAX_SIZE,
			on_demand_target_queue_utilization: Perbill::from_percent(25),
			on_demand_fee_variability: Perbill::from_percent(3),
			on_demand_base_fee: 10_000_000u128,
			ttl: 5u32.into(),
		}
	}
}

/// A unique descriptor of the candidate receipt.
#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
#[cfg_attr(feature = "std", derive(Hash))]
pub struct CandidateDescriptorV2<H = Hash> {
	/// The ID of the para this is a candidate for.
	para_id: ParaId,
	/// The hash of the relay-chain block this is executed in the context of.
	relay_parent: H,
	/// The core index where the candidate is backed.
	core_index: u16,
	/// The session index of the candidate relay parent.
	session_index: SessionIndex,
	/// Reserved bytes.
	reserved26b: [u8; 26],
	/// The blake2-256 hash of the persisted validation data. This is extra data derived from
	/// relay-chain state which may vary based on bitfields included before the candidate.
	/// Thus it cannot be derived entirely from the relay-parent.
	persisted_validation_data_hash: Hash,
	/// The blake2-256 hash of the PoV.
	pov_hash: Hash,
	/// The root of a block's erasure encoding Merkle tree.
	erasure_root: Hash,
	/// Reserved bytes.
	reserved64b: [u8; 64],
	/// Hash of the para header that is being generated by this candidate.
	para_head: Hash,
	/// The blake2-256 hash of the validation code bytes.
	validation_code_hash: ValidationCodeHash,
}

impl<H> CandidateDescriptorV2<H> {
	/// Constructor
	pub fn new(
		para_id: Id,
		relay_parent: H,
		core_index: CoreIndex,
		session_index: SessionIndex,
		persisted_validation_data_hash: Hash,
		pov_hash: Hash,
		erasure_root: Hash,
		para_head: Hash,
		validation_code_hash: ValidationCodeHash,
	) -> Self {
		Self {
			para_id,
			relay_parent,
			core_index: core_index.0 as u16,
			session_index,
			reserved26b: [0; 26],
			persisted_validation_data_hash,
			pov_hash,
			erasure_root,
			reserved64b: [0; 64],
			para_head,
			validation_code_hash,
		}
	}
}
/// Version 1 API to access information stored by candidate descriptors.
pub trait CandidateApiV1 {
	/// Returns the ID of the para this is a candidate for.
	fn para_id(&self) -> Id;

	/// Returns the blake2-256 hash of the persisted validation data.
	fn persisted_validation_data_hash(&self) -> Hash;

	/// Returns the blake2-256 hash of the PoV.
	fn pov_hash(&self) -> Hash;

	/// Returns the root of a block's erasure encoding Merkle tree.
	fn erasure_root(&self) -> Hash;

	/// Returns the hash of the para header generated by this candidate.
	fn para_head(&self) -> Hash;

	/// Return the blake2-256 hash of the validation code bytes.
	fn validation_code_hash(&self) -> ValidationCodeHash;

	/// The collator's sr25519 public key.
	fn collator(&self) -> Option<&CollatorId>;

	/// The parachain index, the relay parent, the validation data hash, and the `pov_hash`.
	fn signature(&self) -> Option<&CollatorSignature>;
}

/// Version 2 API to access additional information stored by candidate descriptors
pub trait CandidateApiV2 {
	/// Returns the core index where the candidate is backed.
	fn core_index(&self) -> Option<CoreIndex>;

	/// Returns the session index of the candidate relay parent.
	fn session_index(&self) -> Option<SessionIndex>;
}


impl<H> CandidateApiV2 for CandidateDescriptorV2<H> {
	fn core_index(&self) -> Option<CoreIndex> {
		Some(CoreIndex(self.core_index as u32))
	}

	fn session_index(&self) -> Option<SessionIndex> {
		Some(self.session_index)
	}
}

/// A candidate-receipt at version 2.
#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
#[cfg_attr(feature = "std", derive(Hash))]
pub struct CandidateReceiptV2<H = Hash> {
	/// The descriptor of the candidate.
	pub descriptor: CandidateDescriptorV2<H>,
	/// The hash of the encoded commitments made as a result of candidate execution.
	pub commitments_hash: Hash,
}

/// A candidate-receipt with commitments directly included.
#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
#[cfg_attr(feature = "std", derive(Hash))]
pub struct CommittedCandidateReceiptV2<H = Hash> {
	/// The descriptor of the candidate.
	pub descriptor: CandidateDescriptorV2<H>,
	/// The commitments of the candidate receipt.
	pub commitments: CandidateCommitments,
}

impl<H> CandidateReceiptV2<H> {
	/// Get a reference to the candidate descriptor.
	pub fn descriptor(&self) -> &CandidateDescriptorV2<H> {
		&self.descriptor
	}

	/// Computes the blake2-256 hash of the receipt.
	pub fn hash(&self) -> CandidateHash
	where
		H: Encode,
	{
		CandidateHash(BlakeTwo256::hash_of(self))
	}
}

impl<H: Clone> CommittedCandidateReceiptV2<H> {
	/// Transforms this into a plain `CandidateReceipt`.
	pub fn to_plain(&self) -> CandidateReceiptV2<H> {
		CandidateReceiptV2 {
			descriptor: self.descriptor.clone(),
			commitments_hash: self.commitments.hash(),
		}
	}

	/// Computes the hash of the committed candidate receipt.
	///
	/// This computes the canonical hash, not the hash of the directly encoded data.
	/// Thus this is a shortcut for `candidate.to_plain().hash()`.
	pub fn hash(&self) -> CandidateHash
	where
		H: Encode,
	{
		self.to_plain().hash()
	}

	/// Does this committed candidate receipt corresponds to the given [`CandidateReceipt`]?
	pub fn corresponds_to(&self, receipt: &CandidateReceiptV2<H>) -> bool
	where
		H: PartialEq,
	{
		receipt.descriptor == self.descriptor && receipt.commitments_hash == self.commitments.hash()
	}
}

impl PartialOrd for CommittedCandidateReceiptV2 {
	fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
		Some(self.cmp(other))
	}
}

impl Ord for CommittedCandidateReceiptV2 {
	fn cmp(&self, other: &Self) -> core::cmp::Ordering {
		self.descriptor()
			.para_id
			.cmp(&other.descriptor().para_id)
			.then_with(|| self.commitments.head_data.cmp(&other.commitments.head_data))
	}
}

/// A strictly increasing sequence number, tipically this would be the parachain block number.
#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
pub struct CoreSelector(pub BlockNumber);

/// An offset in the relay chain claim queue.
#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
pub struct ClaimQueueOffset(pub u8);

/// Default claim queue offset
pub const DEFAULT_CLAIM_QUEUE_OFFSET: ClaimQueueOffset = ClaimQueueOffset(1);

#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
pub enum UMPSignal {
	/// A message sent by a parachain to select the core the candidate is commited to.
	/// Relay chain validators, in particular backers, use the `CoreSelector` and
	/// `ClaimQueueOffset` to compute the index of the core the candidate has commited to.
	SelectCore(CoreSelector, ClaimQueueOffset),
}
/// Separator between `XCM` and `UMPSignal`.
pub const UMP_SEPARATOR: Vec<u8> = vec![];

/// A versioned unique descriptor of the candidate receipt.
#[derive(PartialEq, Eq, Clone, TypeInfo, RuntimeDebug)]
pub enum VersionedCandidateReceipt<H = Hash> {
	/// Version 1 of candidate receipt.
	V1(CandidateReceipt<H>),
	/// Version 2 of candidate receipts with `core_index` and `session_index`.
	V2(CandidateReceiptV2<H>),
}

impl Encode for VersionedCandidateReceipt<Hash> {
	fn encode(&self) -> Vec<u8> {
		match self {
			VersionedCandidateReceipt::V1(inner) => inner.encode(),
			VersionedCandidateReceipt::V2(inner) => inner.encode(),
		}
	}
}

impl Decode for VersionedCandidateReceipt<Hash> {
	fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		let descriptor: CandidateDescriptorV2 = Decode::decode(input)?;
		let commitments_hash = Hash::decode(input)?;

		// Check if descriptor is v2.
		if descriptor.reserved64b == [0u8; 64] && descriptor.reserved26b == [0u8; 26] {
			return Ok(VersionedCandidateReceipt::V2(CandidateReceiptV2 {
				descriptor,
				commitments_hash,
			}))
		}

		// Fall back to v1.
		// TODO: optimize, an additional encode and decode is not nice.
		let descriptor = CandidateDescriptor::decode(&mut descriptor.encode().as_slice())?;

		Ok(VersionedCandidateReceipt::V1(CandidateReceipt { descriptor, commitments_hash }))
	}
}

impl Decode for VersionedCommittedCandidateReceipt<Hash> {
	fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		let descriptor = CandidateDescriptorV2::decode(input)?;
		let commitments = CandidateCommitments::decode(input)?;

		// Check if descriptor is v2.
		if descriptor.reserved64b == [0u8; 64] && descriptor.reserved26b == [0u8; 26] {
			return Ok(VersionedCommittedCandidateReceipt::V2(CommittedCandidateReceiptV2 {
				descriptor,
				commitments,
			}))
		}

		// Fall back to v1.
		// TODO: optimize, an additional encode and decode is not nice.
		let descriptor = CandidateDescriptor::decode(&mut descriptor.encode().as_slice())?;

		Ok(VersionedCommittedCandidateReceipt::V1(CommittedCandidateReceipt {
			descriptor,
			commitments,
		}))
	}
}

/// A versioned unique descriptor of the candidate receipt.
#[derive(PartialEq, Eq, Clone, TypeInfo, RuntimeDebug)]
pub enum VersionedCommittedCandidateReceipt<H> {
	/// Version 1
	V1(super::CommittedCandidateReceipt<H>),
	/// Version 2
	V2(CommittedCandidateReceiptV2<H>),
}

impl<H> VersionedCandidateReceipt<H> {
	/// Returns the core index the candidate has commited to. Returns `None`` if
	/// the receipt is version 1.
	pub fn core_index(&self) -> Option<CoreIndex> {
		match self {
			Self::V1(_receipt_v1) => None,
			Self::V2(receipt_v2) => receipt_v2.descriptor.core_index(),
		}
	}

	/// Returns the session index of the relay parent. Returns `None`` if
	/// the receipt is version 1.
	pub fn session_index(&self) -> Option<SessionIndex> {
		match self {
			Self::V1(_receipt_v1) => None,
			Self::V2(receipt_v2) => receipt_v2.descriptor.session_index(),
		}
	}

	/// Computes the blake2-256 hash of the receipt.
	pub fn hash(&self) -> CandidateHash
	where
		H: Encode,
	{
		match self {
			Self::V1(receipt_v1) => receipt_v1.hash(),
			Self::V2(receipt_v2) => receipt_v2.hash(),
		}
	}
}

impl<H: Clone> VersionedCommittedCandidateReceipt<H> {
	/// Returns the core index the candidate has commited to. Returns `None`` if
	/// the receipt is version 1.
	pub fn core_index(&self) -> Option<CoreIndex> {
		match self {
			Self::V1(_receipt_v1) => None,
			Self::V2(receipt_v2) => receipt_v2.descriptor.core_index(),
		}
	}

	/// Returns the session index of the relay parent. Returns `None` if
	/// the receipt is version 1.
	pub fn session_index(&self) -> Option<SessionIndex> {
		match self {
			Self::V1(_receipt_v1) => None,
			Self::V2(receipt_v2) => receipt_v2.descriptor.session_index(),
		}
	}

	/// Convert to a plain `CandidateReceipt`.
	pub fn to_plain(&self) -> VersionedCandidateReceipt<H> {
		match self {
			Self::V1(receipt) => VersionedCandidateReceipt::V1(receipt.to_plain()),
			Self::V2(receipt) => VersionedCandidateReceipt::V2(receipt.to_plain()),
		}
	}

	/// Returns the candidate commitments.
	 /// Convert to a plain `CandidateReceipt`.
	pub fn commitments(&self) -> &CandidateCommitments {
		match self {
			Self::V1(receipt) => &receipt.commitments,
			Self::V2(receipt) => &receipt.commitments,
		}
	}
}

impl CandidateCommitments {
	/// Returns the core selector and claim queue offset the candidate has commited to, if any.
	pub fn selected_core(&self) -> Option<(CoreSelector, ClaimQueueOffset)> {
		// We need at least 2 messages for the separator and core index
		if self.upward_messages.len() < 2 {
			return None
		}

		let upward_commitments = self
			.upward_messages
			.iter()
			.cloned()
			.rev()
			.take_while(|message| message != &UMP_SEPARATOR)
			.collect::<Vec<_>>();

		// We didn't find the separator, no core index commitment.
		if upward_commitments.len() == self.upward_messages.len() || upward_commitments.is_empty() {
			return None
		}

		// Use first commitment
		let Some(message) = upward_commitments.into_iter().rev().next() else { return None };

		match UMPSignal::decode(&mut message.as_slice()).ok()? {
			UMPSignal::SelectCore(core_selector, cq_offset) => Some((core_selector, cq_offset)),
		}
	}
}

/// CandidateReceipt construction errors.
#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
pub enum CandidateReceiptError {
	/// The specified core index is invalid.
	InvalidCoreIndex,
	/// The core index in commitments doesnt match the one in descriptor
	CoreIndexMismatch,
	/// The core selector or claim queue offset is invalid.
	InvalidSelectedCore,
	/// The parachain is not assigned to any core at specified claim queue offset.
	NoAssignment,
	/// No core was selected.
	NoCoreSelected,
}

macro_rules! impl_candidate_api_v1 {
	($field:ident, $type:ident) => {
		fn $field(&self) -> $type {
			match self {
				Self::V1(receipt) => receipt.descriptor.$field,
				Self::V2(receipt) => receipt.descriptor.$field,
			}
		}
	};
}

impl<H> CandidateApiV1 for VersionedCommittedCandidateReceipt<H> {
	impl_candidate_api_v1!(erasure_root, Hash);
	impl_candidate_api_v1!(para_head, Hash);
	impl_candidate_api_v1!(para_id, ParaId);
	impl_candidate_api_v1!(persisted_validation_data_hash, Hash);
	impl_candidate_api_v1!(pov_hash, Hash);
	impl_candidate_api_v1!(validation_code_hash, ValidationCodeHash);

	fn collator(&self) -> Option<&CollatorId> {
		match self {
			Self::V1(receipt) => Some(&receipt.descriptor.collator),
			Self::V2(receipt) => None,
		}
	}

	fn signature(&self) -> Option<&CollatorSignature> {
		match self {
			Self::V1(receipt) => Some(&receipt.descriptor.signature),
			Self::V2(receipt) => None,
		}
	}
}

impl CommittedCandidateReceiptV2 {
	/// Returns a v2 commited candidate receipt if the committed selected core
	/// matches the core index in the descriptor.
	pub fn new(descriptor: CandidateDescriptorV2, commitments: CandidateCommitments) -> Self {
		Self { descriptor, commitments }
	}

	/// Returns a reference to commitments
	pub fn commitments(&self) -> &CandidateCommitments {
		&self.commitments
	}

	/// Returns a reference to the descriptor
	pub fn descriptor(&self) -> &CandidateDescriptorV2 {
		&self.descriptor
	}

	/// Performs a sanity check of the receipt.
	///
	/// Returns error if:
	/// - descriptor core index is different than the core selected
	/// by the commitments
	/// - the core index is out of bounds wrt `n_cores`.
	pub fn check(
		&self,
		n_cores: u32,
		// TODO: consider promoting `ClaimQueueSnapshot` as primitive
		claim_queue: &BTreeMap<CoreIndex, VecDeque<ParaId>>,
	) -> Result<(), CandidateReceiptError> {
		if claim_queue.is_empty() {
			return Err(CandidateReceiptError::NoAssignment)
		}

		let claim_queue_depth = claim_queue
			.first_key_value()
			.ok_or(CandidateReceiptError::NoAssignment)?
			.1
			.len();

		let descriptor_core_index = CoreIndex(self.descriptor.core_index as u32);
		let (core_selector, cq_offset) =
			self.commitments.selected_core().ok_or(CandidateReceiptError::NoCoreSelected)?;
		let para_id = self.descriptor.para_id;

		if cq_offset.0 as usize >= claim_queue_depth {
			return Err(CandidateReceiptError::InvalidSelectedCore)
		}

		// Get a vec of the core indices the parachain is assigned to at `cq_offset`.
		let assigned_cores = claim_queue
			.iter()
			.filter_map(|(core_index, queue)| {
				let queued_para = queue.get(cq_offset.0 as usize)?;

				if queued_para == &para_id {
					Some(core_index)
				} else {
					None
				}
			})
			.cloned()
			.collect::<Vec<_>>();

		if assigned_cores.is_empty() {
			return Err(CandidateReceiptError::NoAssignment)
		}

		let core_index = *assigned_cores
			.get(core_selector.0 as usize % assigned_cores.len())
			.expect("provided index is always less than queue len; qed");

		if core_index != descriptor_core_index {
			return Err(CandidateReceiptError::CoreIndexMismatch)
		}

		if descriptor_core_index.0 > n_cores - 1 {
			return Err(CandidateReceiptError::InvalidCoreIndex)
		}

		Ok(())
	}
}

impl<H> From<CommittedCandidateReceipt<H>> for VersionedCommittedCandidateReceipt<H> {
	fn from(value: CommittedCandidateReceipt<H>) -> Self {
		Self::V1(value)
	}
}

impl<H> From<CommittedCandidateReceiptV2<H>> for VersionedCommittedCandidateReceipt<H> {
	fn from(value: CommittedCandidateReceiptV2<H>) -> Self {
		Self::V2(value)
	}
}

impl<H> From<CandidateReceipt<H>> for VersionedCandidateReceipt<H> {
	fn from(value: CandidateReceipt<H>) -> Self {
		Self::V1(value)
	}
}

impl<H> From<CandidateReceiptV2<H>> for VersionedCandidateReceipt<H> {
	fn from(value: CandidateReceiptV2<H>) -> Self {
		Self::V2(value)
	}
}

/// A backed (or backable, depending on context) candidate.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct BackedCandidate<H = Hash> {
	/// The candidate referred to.
	candidate: VersionedCommittedCandidateReceipt<H>,
	/// The validity votes themselves, expressed as signatures.
	validity_votes: Vec<ValidityAttestation>,
	/// The indices of the validators within the group, expressed as a bitfield. May be extended
	/// beyond the backing group size to contain the assigned core index, if ElasticScalingMVP is
	/// enabled.
	validator_indices: BitVec<u8, bitvec::order::Lsb0>,
}

impl<H> BackedCandidate<H> {
	/// Constructor
	pub fn new(
		candidate: VersionedCommittedCandidateReceipt<H>,
		validity_votes: Vec<ValidityAttestation>,
		validator_indices: BitVec<u8, bitvec::order::Lsb0>,
		core_index: Option<CoreIndex>,
	) -> Self {
		let mut instance = Self { candidate, validity_votes, validator_indices };
		if let Some(core_index) = core_index {
			instance.inject_core_index(core_index);
		}
		instance
	}

	/// Get a reference to the committed candidate receipt of the candidate.
	pub fn candidate(&self) -> &VersionedCommittedCandidateReceipt<H> {
		&self.candidate
	}

	/// Get a reference to the validity votes of the candidate.
	pub fn validity_votes(&self) -> &[ValidityAttestation] {
		&self.validity_votes
	}

	/// Get a mutable reference to validity votes of the para.
	pub fn validity_votes_mut(&mut self) -> &mut Vec<ValidityAttestation> {
		&mut self.validity_votes
	}

	/// Compute this candidate's hash.
	pub fn hash(&self) -> CandidateHash
	where
		H: Clone + Encode,
	{
		self.candidate.to_plain().hash()
	}

	/// Get this candidate's receipt.
	pub fn receipt(&self) -> VersionedCandidateReceipt<H>
	where
		H: Clone,
	{
		self.candidate.to_plain()
	}

	/// Get a copy of the validator indices and the assumed core index, if any.
	pub fn validator_indices_and_core_index(
		&self,
		core_index_enabled: bool,
	) -> (&BitSlice<u8, bitvec::order::Lsb0>, Option<CoreIndex>) {
		// This flag tells us if the block producers must enable Elastic Scaling MVP hack.
		// It extends `BackedCandidate::validity_indices` to store a 8 bit core index.
		if core_index_enabled {
			let core_idx_offset = self.validator_indices.len().saturating_sub(8);
			if core_idx_offset > 0 {
				let (validator_indices_slice, core_idx_slice) =
					self.validator_indices.split_at(core_idx_offset);
				return (
					validator_indices_slice,
					Some(CoreIndex(core_idx_slice.load::<u8>() as u32)),
				);
			}
		}

		(&self.validator_indices, None)
	}

	/// Inject a core index in the validator_indices bitvec.
	fn inject_core_index(&mut self, core_index: CoreIndex) {
		let core_index_to_inject: BitVec<u8, bitvec::order::Lsb0> =
			BitVec::from_vec(vec![core_index.0 as u8]);
		self.validator_indices.extend(core_index_to_inject);
	}

	/// Update the validator indices and core index in the candidate.
	pub fn set_validator_indices_and_core_index(
		&mut self,
		new_indices: BitVec<u8, bitvec::order::Lsb0>,
		maybe_core_index: Option<CoreIndex>,
	) {
		self.validator_indices = new_indices;

		if let Some(core_index) = maybe_core_index {
			self.inject_core_index(core_index);
		}
	}
}

#[cfg(test)]
mod tests {
	use core::prelude::v1;

	use super::*;
	use crate::{
		v7::{
			tests::dummy_committed_candidate_receipt as dummy_old_committed_candidate_receipt,
			CandidateDescriptor, CandidateReceipt as OldCandidateReceipt,
			CommittedCandidateReceipt, Hash, HeadData, ValidationCode,
		},
		vstaging::{
			CandidateApiV1, CandidateApiV2, CandidateDescriptorV2, CommittedCandidateReceiptV2,
		},
	};
	// use polkadot_primitives_test_helpers::dummy_committed_candidate_receipt_v2;

	pub fn dummy_committed_candidate_receipt_v2() -> CommittedCandidateReceiptV2 {
		let zeros = Hash::zero();
		let reserved64b = [0; 64];

		CommittedCandidateReceiptV2 {
			descriptor: CandidateDescriptorV2 {
				para_id: 0.into(),
				relay_parent: zeros,
				core_index: 123,
				session_index: 1,
				reserved26b: Default::default(),
				persisted_validation_data_hash: zeros,
				pov_hash: zeros,
				erasure_root: zeros,
				reserved64b,
				para_head: zeros,
				validation_code_hash: ValidationCode(vec![1, 2, 3, 4, 5, 6, 7, 8, 9]).hash(),
			},
			commitments: CandidateCommitments {
				head_data: HeadData(vec![]),
				upward_messages: vec![].try_into().expect("empty vec fits within bounds"),
				new_validation_code: None,
				horizontal_messages: vec![].try_into().expect("empty vec fits within bounds"),
				processed_downward_messages: 0,
				hrmp_watermark: 0_u32,
			},
		}
	}

	#[test]
	fn is_binary_compatibile() {
		let old_ccr = dummy_old_committed_candidate_receipt();
		let new_ccr = dummy_committed_candidate_receipt_v2();

		assert_eq!(old_ccr.encoded_size(), new_ccr.encoded_size());

		let encoded_old = old_ccr.encode();

		// Deserialize from old candidate receipt.
		let new_ccr: CommittedCandidateReceiptV2 =
			Decode::decode(&mut encoded_old.as_slice()).unwrap();

		// We get same candidate hash.
		assert_eq!(old_ccr.hash(), new_ccr.hash());
	}

	#[test]
	fn test_ump_commitment() {
		let mut new_ccr = dummy_committed_candidate_receipt_v2();
		new_ccr.descriptor.core_index = 123;
		new_ccr.descriptor.para_id = ParaId::new(1000);

		// dummy XCM messages
		new_ccr.commitments.upward_messages.force_push(vec![0u8; 256]);
		new_ccr.commitments.upward_messages.force_push(vec![0xff; 256]);

		// separator
		new_ccr.commitments.upward_messages.force_push(UMP_SEPARATOR);

		// CoreIndex commitment
		new_ccr
			.commitments
			.upward_messages
			.force_push(UMPSignal::SelectCore(CoreSelector(0), ClaimQueueOffset(1)).encode());

		let mut claim_queue = BTreeMap::new();
		claim_queue.insert(
			new_ccr.descriptor().core_index().unwrap(),
			vec![2.into(), new_ccr.descriptor.para_id, 3.into()].into(),
		);

		assert_eq!(new_ccr.check(200, &claim_queue), Ok(()));
	}

	#[test]
	fn test_versioned_receipt() {
		let mut new_ccr = dummy_committed_candidate_receipt_v2();
		new_ccr.descriptor.core_index = 123;
		new_ccr.descriptor.para_id = ParaId::new(1000);

		// dummy XCM messages
		new_ccr.commitments.upward_messages.force_push(vec![0u8; 256]);
		new_ccr.commitments.upward_messages.force_push(vec![0xff; 256]);

		// separator
		new_ccr.commitments.upward_messages.force_push(UMP_SEPARATOR);

		// CoreIndex commitment
		new_ccr
			.commitments
			.upward_messages
			.force_push(UMPSignal::SelectCore(CoreSelector(0), ClaimQueueOffset(1)).encode());

		let mut claim_queue = BTreeMap::new();
		claim_queue.insert(
			new_ccr.descriptor().core_index().unwrap(),
			vec![2.into(), new_ccr.descriptor.para_id, 3.into()].into(),
		);

		let encoded_ccr = new_ccr.encode();
		let versioned_ccr =
			VersionedCommittedCandidateReceipt::decode(&mut encoded_ccr.as_slice()).unwrap();

		assert_eq!(versioned_ccr.core_index(), Some(CoreIndex(123)));

		if let VersionedCommittedCandidateReceipt::V2(ref v2_receipt) = versioned_ccr {
			assert_eq!(v2_receipt.check(200, &claim_queue), Ok(()));
		}

		assert_eq!(new_ccr.hash(), versioned_ccr.to_plain().hash());
	}

	#[test]
	fn test_backward_compatible() {
		// Testing edge case when collators provide zeroed signature and collator id.
		let mut old_ccr = dummy_old_committed_candidate_receipt();
		old_ccr.descriptor.para_id = ParaId::new(1000);
		let encoded_ccr: Vec<u8> = old_ccr.encode();

		let versioned_ccr =
			VersionedCommittedCandidateReceipt::decode(&mut encoded_ccr.as_slice()).unwrap();

		// Since collator sig and id are zeroed, it was decoded as V2.
		// We expect the check to fail in such case because there will be no `SelectCore`
		// commitment.
		let mut claim_queue = BTreeMap::new();
		claim_queue.insert(
			versioned_ccr.core_index().unwrap(),
			vec![2.into(), versioned_ccr.para_id(), 3.into()].into(),
		);

		if let VersionedCommittedCandidateReceipt::V2(ref v2_receipt) = versioned_ccr {
			assert_eq!(
				v2_receipt.check(200, &claim_queue),
				Err(CandidateReceiptError::NoCoreSelected)
			);
		} else {
			panic!("Should have decoded as V2")
		}

		// Adding collator signature should make it decode as v1.
		old_ccr.descriptor.signature =
			CollatorSignature::from_slice(&vec![99u8; 64]).expect("64 bytes; qed");
		let encoded_ccr: Vec<u8> = old_ccr.encode();
		let versioned_ccr =
			VersionedCommittedCandidateReceipt::decode(&mut encoded_ccr.as_slice()).unwrap();

		if let VersionedCommittedCandidateReceipt::V1(ref v1_receipt) = versioned_ccr {
			assert_eq!(v1_receipt.descriptor.signature, old_ccr.descriptor.signature);
		} else {
			panic!("Should have decoded as V1")
		}

		assert_eq!(versioned_ccr.core_index(), None);
		assert_eq!(versioned_ccr.para_id(), ParaId::new(1000));
		assert_eq!(old_ccr.hash(), versioned_ccr.to_plain().hash());
	}
}
