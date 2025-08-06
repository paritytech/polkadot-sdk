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
// along with Polkadot.  If not, see <http://www.g	nu.org/licenses/>.

//! Staging Primitives.
use core::fmt::Formatter;

use crate::{
	slashing::DisputesTimeSlot,
	v9::{
		CandidateDescriptorV2, CandidateDescriptorVersion, CandidateEvent,
		CommittedCandidateReceiptV2, InternalVersion,
	},
	CandidateReceiptV2, ValidatorId, ValidatorIndex, ValidityAttestation,
};

// Put any primitives used by staging APIs functions here
use super::{
	BlakeTwo256, BlockNumber, CandidateCommitments, CandidateDescriptor, CandidateHash, CollatorId,
	CollatorSignature, CoreIndex, GroupIndex, Hash, HashT, HeadData, Header, Id, Id as ParaId,
	MultiDisputeStatementSet, ScheduledCore, UncheckedSignedAvailabilityBitfields,
	ValidationCodeHash,
};
use alloc::{
	collections::{BTreeMap, BTreeSet, VecDeque},
	vec,
	vec::Vec,
};
use bitvec::prelude::*;
use bounded_collections::BoundedVec;
use codec::{Decode, DecodeWithMemTracking, Encode};
use scale_info::TypeInfo;
use sp_application_crypto::ByteArray;
use sp_core::{ConstU32, RuntimeDebug};
use sp_runtime::traits::Header as HeaderT;
use sp_staking::SessionIndex;

/// The default claim queue offset to be used if it's not configured/accessible in the parachain
/// runtime
pub const DEFAULT_CLAIM_QUEUE_OFFSET: u8 = 0;

impl<H: Encode + Copy> From<CandidateEvent<H>> for super::v9::LegacyCandidateEvent<H> {
	fn from(value: CandidateEvent<H>) -> Self {
		match value {
			CandidateEvent::CandidateBacked(receipt, head_data, core_index, group_index) =>
				super::v9::LegacyCandidateEvent::CandidateBacked(
					receipt.into(),
					head_data,
					core_index,
					group_index,
				),
			CandidateEvent::CandidateIncluded(receipt, head_data, core_index, group_index) =>
				super::v9::LegacyCandidateEvent::CandidateIncluded(
					receipt.into(),
					head_data,
					core_index,
					group_index,
				),
			CandidateEvent::CandidateTimedOut(receipt, head_data, core_index) =>
				super::v9::LegacyCandidateEvent::CandidateTimedOut(
					receipt.into(),
					head_data,
					core_index,
				),
		}
	}
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

impl<H: Copy> From<super::v9::CandidateReceipt<H>> for CandidateReceiptV2<H> {
	fn from(value: super::v9::CandidateReceipt<H>) -> Self {
		CandidateReceiptV2 {
			descriptor: value.descriptor.into(),
			commitments_hash: value.commitments_hash,
		}
	}
}

impl<H: Copy> From<super::v9::CommittedCandidateReceipt<H>> for CommittedCandidateReceiptV2<H> {
	fn from(value: super::v9::CommittedCandidateReceipt<H>) -> Self {
		CommittedCandidateReceiptV2 {
			descriptor: value.descriptor.into(),
			commitments: value.commitments,
		}
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

	/// Does this committed candidate receipt corresponds to the given [`CandidateReceiptV2`]?
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
		self.descriptor
			.para_id
			.cmp(&other.descriptor.para_id)
			.then_with(|| self.commitments.head_data.cmp(&other.commitments.head_data))
	}
}

impl<H: Copy> From<CommittedCandidateReceiptV2<H>> for super::v9::CommittedCandidateReceipt<H> {
	fn from(value: CommittedCandidateReceiptV2<H>) -> Self {
		Self { descriptor: value.descriptor.into(), commitments: value.commitments }
	}
}

impl<H: Copy> From<CandidateReceiptV2<H>> for super::v9::CandidateReceipt<H> {
	fn from(value: CandidateReceiptV2<H>) -> Self {
		Self { descriptor: value.descriptor.into(), commitments_hash: value.commitments_hash }
	}
}

/// A strictly increasing sequence number, typically this would be the least significant byte of the
/// block number.
#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, Debug, Copy)]
pub struct CoreSelector(pub u8);

/// An offset in the relay chain claim queue.
#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, Debug, Copy)]
pub struct ClaimQueueOffset(pub u8);

/// Approved PeerId type. PeerIds in polkadot should typically be 32 bytes long but for identity
/// multihash can go up to 64. Cannot reuse the PeerId type definition from the networking code as
/// it's too generic and extensible.
pub type ApprovedPeerId = BoundedVec<u8, ConstU32<64>>;

/// Signals that a parachain can send to the relay chain via the UMP queue.
#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, Debug)]
pub enum UMPSignal {
	/// A message sent by a parachain to select the core the candidate is committed to.
	/// Relay chain validators, in particular backers, use the `CoreSelector` and
	/// `ClaimQueueOffset` to compute the index of the core the candidate has committed to.
	SelectCore(CoreSelector, ClaimQueueOffset),
	/// A message sent by a parachain to promote the reputation of a given peerid.
	ApprovedPeer(ApprovedPeerId),
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug, Default)]
/// User-friendly representation of a candidate's UMP signals.
pub struct CandidateUMPSignals {
	pub(super) select_core: Option<(CoreSelector, ClaimQueueOffset)>,
	pub(super) approved_peer: Option<ApprovedPeerId>,
}

impl CandidateUMPSignals {
	/// Get the core selector UMP signal.
	pub fn core_selector(&self) -> Option<(CoreSelector, ClaimQueueOffset)> {
		self.select_core
	}

	/// Get a reference to the approved peer UMP signal.
	pub fn approved_peer(&self) -> Option<&ApprovedPeerId> {
		self.approved_peer.as_ref()
	}

	/// Returns `true` if UMP signals are empty.
	pub fn is_empty(&self) -> bool {
		self.select_core.is_none() && self.approved_peer.is_none()
	}

	fn try_decode_signal(
		&mut self,
		buffer: &mut impl codec::Input,
	) -> Result<(), CommittedCandidateReceiptError> {
		match UMPSignal::decode(buffer)
			.map_err(|_| CommittedCandidateReceiptError::UmpSignalDecode)?
		{
			UMPSignal::ApprovedPeer(approved_peer_id) if self.approved_peer.is_none() => {
				self.approved_peer = Some(approved_peer_id);
			},
			UMPSignal::SelectCore(core_selector, cq_offset) if self.select_core.is_none() => {
				self.select_core = Some((core_selector, cq_offset));
			},
			_ => {
				// This means that we got duplicate UMP signals.
				return Err(CommittedCandidateReceiptError::DuplicateUMPSignal)
			},
		};

		Ok(())
	}
}

/// Separator between `XCM` and `UMPSignal`.
pub const UMP_SEPARATOR: Vec<u8> = vec![];

/// Utility function for skipping the ump signals.
pub fn skip_ump_signals<'a>(
	upward_messages: impl Iterator<Item = &'a Vec<u8>>,
) -> impl Iterator<Item = &'a Vec<u8>> {
	upward_messages.take_while(|message| *message != &UMP_SEPARATOR)
}

impl CandidateCommitments {
	/// Returns the ump signals of this candidate, if any, or an error if they violate the expected
	/// format.
	pub fn ump_signals(&self) -> Result<CandidateUMPSignals, CommittedCandidateReceiptError> {
		let mut res = CandidateUMPSignals::default();

		let mut signals_iter =
			self.upward_messages.iter().skip_while(|message| *message != &UMP_SEPARATOR);

		if signals_iter.next().is_none() {
			// No UMP separator
			return Ok(res)
		}

		// Process first signal
		let Some(first_signal) = signals_iter.next() else { return Ok(res) };
		res.try_decode_signal(&mut first_signal.as_slice())?;

		// Process second signal
		let Some(second_signal) = signals_iter.next() else { return Ok(res) };
		res.try_decode_signal(&mut second_signal.as_slice())?;

		// At most two signals are allowed
		if signals_iter.next().is_some() {
			return Err(CommittedCandidateReceiptError::TooManyUMPSignals)
		}

		Ok(res)
	}
}

/// CommittedCandidateReceiptError construction errors.
#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
#[cfg_attr(feature = "std", derive(thiserror::Error))]
pub enum CommittedCandidateReceiptError {
	/// The specified core index is invalid.
	#[cfg_attr(feature = "std", error("The specified core index is invalid"))]
	InvalidCoreIndex,
	/// The core index in commitments doesn't match the one in descriptor
	#[cfg_attr(
		feature = "std",
		error("The core index in commitments ({commitments:?}) doesn't match the one in descriptor ({descriptor:?})")
	)]
	CoreIndexMismatch {
		/// The core index as found in the descriptor.
		descriptor: CoreIndex,
		/// The core index as found in the commitments.
		commitments: CoreIndex,
	},
	/// The core selector or claim queue offset is invalid.
	#[cfg_attr(feature = "std", error("The core selector or claim queue offset is invalid"))]
	InvalidSelectedCore,
	#[cfg_attr(feature = "std", error("Could not decode UMP signal"))]
	/// Could not decode UMP signal.
	UmpSignalDecode,
	/// The parachain is not assigned to any core at specified claim queue offset.
	#[cfg_attr(
		feature = "std",
		error("The parachain is not assigned to any core at specified claim queue offset")
	)]
	NoAssignment,
	/// Unknown version.
	#[cfg_attr(feature = "std", error("Unknown internal version"))]
	UnknownVersion(InternalVersion),
	/// The allowed number of `UMPSignal` messages in the queue was exceeded.
	#[cfg_attr(feature = "std", error("Too many UMP signals"))]
	TooManyUMPSignals,
	/// Duplicated UMP signal.
	#[cfg_attr(feature = "std", error("Duplicate UMP signal"))]
	DuplicateUMPSignal,
	/// If the parachain runtime started sending ump signals, v1 descriptors are no longer
	/// allowed.
	#[cfg_attr(feature = "std", error("Version 1 receipt does not support ump signals"))]
	UMPSignalWithV1Decriptor,
}

impl<H: Copy> CommittedCandidateReceiptV2<H> {
	/// Performs checks on the UMP signals and returns them.
	///
	/// Also checks if descriptor core index is equal to the committed core index.
	///
	/// Params:
	/// - `cores_per_para` is a claim queue snapshot at the candidate's relay parent, stored as
	/// a mapping between `ParaId` and the cores assigned per depth.
	pub fn parse_ump_signals(
		&self,
		cores_per_para: &TransposedClaimQueue,
	) -> Result<CandidateUMPSignals, CommittedCandidateReceiptError> {
		let signals = self.commitments.ump_signals()?;

		match self.descriptor.version() {
			CandidateDescriptorVersion::V1 => {
				// If the parachain runtime started sending ump signals, v1 descriptors are no
				// longer allowed.
				if !signals.is_empty() {
					return Err(CommittedCandidateReceiptError::UMPSignalWithV1Decriptor)
				} else {
					// Nothing else to check for v1 descriptors.
					return Ok(CandidateUMPSignals::default())
				}
			},
			CandidateDescriptorVersion::V2 => {},
			CandidateDescriptorVersion::Unknown =>
				return Err(CommittedCandidateReceiptError::UnknownVersion(self.descriptor.version)),
		}

		// Check the core index
		let (maybe_core_index_selector, cq_offset) = signals
			.core_selector()
			.map(|(selector, offset)| (Some(selector), offset))
			.unwrap_or_else(|| (None, ClaimQueueOffset(DEFAULT_CLAIM_QUEUE_OFFSET)));

		self.check_core_index(cores_per_para, maybe_core_index_selector, cq_offset)?;

		// Nothing to further check for the approved peer. If everything passed so far, return the
		// signals.
		Ok(signals)
	}

	/// Checks if descriptor core index is equal to the committed core index.
	/// Input `cores_per_para` is a claim queue snapshot at the candidate's relay parent, stored as
	/// a mapping between `ParaId` and the cores assigned per depth.
	fn check_core_index(
		&self,
		cores_per_para: &TransposedClaimQueue,
		maybe_core_index_selector: Option<CoreSelector>,
		cq_offset: ClaimQueueOffset,
	) -> Result<(), CommittedCandidateReceiptError> {
		let assigned_cores = cores_per_para
			.get(&self.descriptor.para_id())
			.ok_or(CommittedCandidateReceiptError::NoAssignment)?
			.get(&cq_offset.0)
			.ok_or(CommittedCandidateReceiptError::NoAssignment)?;

		if assigned_cores.is_empty() {
			return Err(CommittedCandidateReceiptError::NoAssignment)
		}

		let descriptor_core_index = CoreIndex(self.descriptor.core_index as u32);

		let core_index_selector = if let Some(core_index_selector) = maybe_core_index_selector {
			// We have a committed core selector, we can use it.
			core_index_selector
		} else if assigned_cores.len() > 1 {
			// We got more than one assigned core and no core selector. Special care is needed.
			if !assigned_cores.contains(&descriptor_core_index) {
				// core index in the descriptor is not assigned to the para. Error.
				return Err(CommittedCandidateReceiptError::InvalidCoreIndex)
			} else {
				// the descriptor core index is indeed assigned to the para. This is the most we can
				// check for now
				return Ok(())
			}
		} else {
			// No core selector but there's only one assigned core, use it.
			CoreSelector(0)
		};

		let core_index = assigned_cores
			.iter()
			.nth(core_index_selector.0 as usize % assigned_cores.len())
			.ok_or(CommittedCandidateReceiptError::InvalidSelectedCore)
			.copied()?;

		if core_index != descriptor_core_index {
			return Err(CommittedCandidateReceiptError::CoreIndexMismatch {
				descriptor: descriptor_core_index,
				commitments: core_index,
			})
		}

		Ok(())
	}
}

/// A backed (or backable, depending on context) candidate.
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct BackedCandidate<H = Hash> {
	/// The candidate referred to.
	candidate: CommittedCandidateReceiptV2<H>,
	/// The validity votes themselves, expressed as signatures.
	validity_votes: Vec<ValidityAttestation>,
	/// The indices of the validators within the group, expressed as a bitfield. May be extended
	/// beyond the backing group size to contain the assigned core index, if ElasticScalingMVP is
	/// enabled.
	validator_indices: BitVec<u8, bitvec::order::Lsb0>,
}

/// Parachains inherent-data passed into the runtime by a block author
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, RuntimeDebug, TypeInfo)]
pub struct InherentData<HDR: HeaderT = Header> {
	/// Signed bitfields by validators about availability.
	pub bitfields: UncheckedSignedAvailabilityBitfields,
	/// Backed candidates for inclusion in the block.
	pub backed_candidates: Vec<BackedCandidate<HDR::Hash>>,
	/// Sets of dispute votes for inclusion,
	pub disputes: MultiDisputeStatementSet,
	/// The parent block header. Used for checking state proofs.
	pub parent_header: HDR,
}

impl<H> BackedCandidate<H> {
	/// Constructor
	pub fn new(
		candidate: CommittedCandidateReceiptV2<H>,
		validity_votes: Vec<ValidityAttestation>,
		validator_indices: BitVec<u8, bitvec::order::Lsb0>,
		core_index: CoreIndex,
	) -> Self {
		let mut instance = Self { candidate, validity_votes, validator_indices };
		instance.inject_core_index(core_index);
		instance
	}

	/// Get a reference to the committed candidate receipt of the candidate.
	pub fn candidate(&self) -> &CommittedCandidateReceiptV2<H> {
		&self.candidate
	}

	/// Get a mutable reference to the committed candidate receipt of the candidate.
	/// Only for testing.
	#[cfg(feature = "test")]
	pub fn candidate_mut(&mut self) -> &mut CommittedCandidateReceiptV2<H> {
		&mut self.candidate
	}
	/// Get a reference to the descriptor of the candidate.
	pub fn descriptor(&self) -> &CandidateDescriptorV2<H> {
		&self.candidate.descriptor
	}

	/// Get a mutable reference to the descriptor of the candidate. Only for testing.
	#[cfg(feature = "test")]
	pub fn descriptor_mut(&mut self) -> &mut CandidateDescriptorV2<H> {
		&mut self.candidate.descriptor
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
	pub fn receipt(&self) -> CandidateReceiptV2<H>
	where
		H: Clone,
	{
		self.candidate.to_plain()
	}

	/// Get a copy of the validator indices and the assumed core index, if any.
	pub fn validator_indices_and_core_index(
		&self,
	) -> (&BitSlice<u8, bitvec::order::Lsb0>, Option<CoreIndex>) {
		// `BackedCandidate::validity_indices` are extended to store a 8 bit core index.
		let core_idx_offset = self.validator_indices.len().saturating_sub(8);
		if core_idx_offset > 0 {
			let (validator_indices_slice, core_idx_slice) =
				self.validator_indices.split_at(core_idx_offset);
			return (validator_indices_slice, Some(CoreIndex(core_idx_slice.load::<u8>() as u32)));
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

/// Scraped runtime backing votes and resolved disputes.
#[derive(Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
#[cfg_attr(feature = "std", derive(PartialEq))]
pub struct ScrapedOnChainVotes<H: Encode + Decode = Hash> {
	/// The session in which the block was included.
	pub session: SessionIndex,
	/// Set of backing validators for each candidate, represented by its candidate
	/// receipt.
	pub backing_validators_per_candidate:
		Vec<(CandidateReceiptV2<H>, Vec<(ValidatorIndex, ValidityAttestation)>)>,
	/// On-chain-recorded set of disputes.
	/// Note that the above `backing_validators` are
	/// unrelated to the backers of the disputes candidates.
	pub disputes: MultiDisputeStatementSet,
}

impl<H: Encode + Decode + Copy> From<ScrapedOnChainVotes<H>>
	for super::v9::LegacyScrapedOnChainVotes<H>
{
	fn from(value: ScrapedOnChainVotes<H>) -> Self {
		Self {
			session: value.session,
			backing_validators_per_candidate: value
				.backing_validators_per_candidate
				.into_iter()
				.map(|(receipt, validators)| (receipt.into(), validators))
				.collect::<Vec<_>>(),
			disputes: value.disputes,
		}
	}
}

/// Information about a core which is currently occupied.
#[derive(Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
#[cfg_attr(feature = "std", derive(PartialEq))]
pub struct OccupiedCore<H = Hash, N = BlockNumber> {
	// NOTE: this has no ParaId as it can be deduced from the candidate descriptor.
	/// If this core is freed by availability, this is the assignment that is next up on this
	/// core, if any. None if there is nothing queued for this core.
	pub next_up_on_available: Option<ScheduledCore>,
	/// The relay-chain block number this began occupying the core at.
	pub occupied_since: N,
	/// The relay-chain block this will time-out at, if any.
	pub time_out_at: N,
	/// If this core is freed by being timed-out, this is the assignment that is next up on this
	/// core. None if there is nothing queued for this core or there is no possibility of timing
	/// out.
	pub next_up_on_time_out: Option<ScheduledCore>,
	/// A bitfield with 1 bit for each validator in the set. `1` bits mean that the corresponding
	/// validators has attested to availability on-chain. A 2/3+ majority of `1` bits means that
	/// this will be available.
	pub availability: BitVec<u8, bitvec::order::Lsb0>,
	/// The group assigned to distribute availability pieces of this candidate.
	pub group_responsible: GroupIndex,
	/// The hash of the candidate occupying the core.
	pub candidate_hash: CandidateHash,
	/// The descriptor of the candidate occupying the core.
	pub candidate_descriptor: CandidateDescriptorV2<H>,
}

impl<H, N> OccupiedCore<H, N> {
	/// Get the Para currently occupying this core.
	pub fn para_id(&self) -> Id {
		self.candidate_descriptor.para_id
	}
}

/// The state of a particular availability core.
#[derive(Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
#[cfg_attr(feature = "std", derive(PartialEq))]
pub enum CoreState<H = Hash, N = BlockNumber> {
	/// The core is currently occupied.
	#[codec(index = 0)]
	Occupied(OccupiedCore<H, N>),
	/// The core is currently free, with a para scheduled and given the opportunity
	/// to occupy.
	///
	/// If a particular Collator is required to author this block, that is also present in this
	/// variant.
	#[codec(index = 1)]
	Scheduled(ScheduledCore),
	/// The core is currently free and there is nothing scheduled. This can be the case for
	/// parathread cores when there are no parathread blocks queued. Parachain cores will never be
	/// left idle.
	#[codec(index = 2)]
	Free,
}

impl<N> CoreState<N> {
	/// Returns the scheduled `ParaId` for the core or `None` if nothing is scheduled.
	///
	/// This function is deprecated. `ClaimQueue` should be used to obtain the scheduled `ParaId`s
	/// for each core.
	#[deprecated(
		note = "`para_id` will be removed. Use `ClaimQueue` to query the scheduled `para_id` instead."
	)]
	pub fn para_id(&self) -> Option<Id> {
		match self {
			Self::Occupied(ref core) => core.next_up_on_available.as_ref().map(|n| n.para_id),
			Self::Scheduled(core) => Some(core.para_id),
			Self::Free => None,
		}
	}

	/// Is this core state `Self::Occupied`?
	pub fn is_occupied(&self) -> bool {
		matches!(self, Self::Occupied(_))
	}
}

impl<H: Copy> From<OccupiedCore<H>> for super::v9::LegacyOccupiedCore<H> {
	fn from(value: OccupiedCore<H>) -> Self {
		Self {
			next_up_on_available: value.next_up_on_available,
			occupied_since: value.occupied_since,
			time_out_at: value.time_out_at,
			next_up_on_time_out: value.next_up_on_time_out,
			availability: value.availability,
			group_responsible: value.group_responsible,
			candidate_hash: value.candidate_hash,
			candidate_descriptor: value.candidate_descriptor.into(),
		}
	}
}

impl<H: Copy> From<CoreState<H>> for super::v9::LegacyCoreState<H> {
	fn from(value: CoreState<H>) -> Self {
		match value {
			CoreState::Free => super::v9::LegacyCoreState::Free,
			CoreState::Scheduled(core) => super::v9::LegacyCoreState::Scheduled(core),
			CoreState::Occupied(occupied_core) =>
				super::v9::LegacyCoreState::Occupied(occupied_core.into()),
		}
	}
}

/// The claim queue mapped by parachain id.
pub type TransposedClaimQueue = BTreeMap<ParaId, BTreeMap<u8, BTreeSet<CoreIndex>>>;

/// Returns a mapping between the para id and the core indices assigned at different
/// depths in the claim queue.
pub fn transpose_claim_queue(
	claim_queue: BTreeMap<CoreIndex, VecDeque<Id>>,
) -> TransposedClaimQueue {
	let mut per_para_claim_queue = BTreeMap::new();

	for (core, paras) in claim_queue {
		// Iterate paras assigned to this core at each depth.
		for (depth, para) in paras.into_iter().enumerate() {
			let depths: &mut BTreeMap<u8, BTreeSet<CoreIndex>> =
				per_para_claim_queue.entry(para).or_insert_with(|| Default::default());

			depths.entry(depth as u8).or_default().insert(core);
		}
	}

	per_para_claim_queue
}

// Approval Slashes primitives
/// Supercedes the old 'SlashingOffenceKind' enum.
#[derive(PartialEq, Eq, Clone, Copy, Encode, Decode, DecodeWithMemTracking, TypeInfo, Debug)]
pub enum DisputeOffenceKind {
	/// A severe offence when a validator backed an invalid block
	/// (backing only)
	#[codec(index = 0)]
	ForInvalidBacked,
	/// A minor offence when a validator disputed a valid block.
	/// (approval checking and dispute vote only)
	#[codec(index = 1)]
	AgainstValid,
	/// A medium offence when a validator approved an invalid block
	/// (approval checking and dispute vote only)
	#[codec(index = 2)]
	ForInvalidApproved,
}

/// impl for a conversion from SlashingOffenceKind to DisputeOffenceKind
/// This creates DisputeOffenceKind that never contains ForInvalidApproved since it was not
/// supported in the past
impl From<super::v9::slashing::SlashingOffenceKind> for DisputeOffenceKind {
	fn from(value: super::v9::slashing::SlashingOffenceKind) -> Self {
		match value {
			super::v9::slashing::SlashingOffenceKind::ForInvalid => Self::ForInvalidBacked,
			super::v9::slashing::SlashingOffenceKind::AgainstValid => Self::AgainstValid,
		}
	}
}

/// impl for a tryFrom conversion from DisputeOffenceKind to SlashingOffenceKind
impl TryFrom<DisputeOffenceKind> for super::v9::slashing::SlashingOffenceKind {
	type Error = ();

	fn try_from(value: DisputeOffenceKind) -> Result<Self, Self::Error> {
		match value {
			DisputeOffenceKind::ForInvalidBacked => Ok(Self::ForInvalid),
			DisputeOffenceKind::AgainstValid => Ok(Self::AgainstValid),
			DisputeOffenceKind::ForInvalidApproved => Err(()),
		}
	}
}

/// Slashes that are waiting to be applied once we have validator key
/// identification.
#[derive(Encode, Decode, TypeInfo, Debug, Clone)]
pub struct PendingSlashes {
	/// Indices and keys of the validators who lost a dispute and are pending
	/// slashes.
	pub keys: BTreeMap<ValidatorIndex, ValidatorId>,
	/// The dispute outcome.
	pub kind: DisputeOffenceKind,
}

impl From<super::v9::slashing::LegacyPendingSlashes> for PendingSlashes {
	fn from(old: super::v9::slashing::LegacyPendingSlashes) -> Self {
		let keys = old.keys;
		let kind = old.kind.into();
		Self { keys, kind }
	}
}

impl TryFrom<PendingSlashes> for super::v9::slashing::LegacyPendingSlashes {
	type Error = ();

	fn try_from(value: PendingSlashes) -> Result<Self, Self::Error> {
		Ok(Self { keys: value.keys, kind: value.kind.try_into()? })
	}
}

/// We store most of the information about a lost dispute on chain. This struct
/// is required to identify and verify it.
#[derive(PartialEq, Eq, Clone, Encode, Decode, DecodeWithMemTracking, TypeInfo, Debug)]
pub struct DisputeProof {
	/// Time slot when the dispute occurred.
	pub time_slot: DisputesTimeSlot,
	/// The dispute outcome.
	pub kind: DisputeOffenceKind,
	/// The index of the validator who lost a dispute.
	pub validator_index: ValidatorIndex,
	/// The parachain session key of the validator.
	pub validator_id: ValidatorId,
}

impl From<super::v9::slashing::LegacyDisputeProof> for DisputeProof {
	fn from(old: super::v9::slashing::LegacyDisputeProof) -> Self {
		let time_slot = old.time_slot;
		let kind = old.kind.into(); // infallible conversion
		let validator_index = old.validator_index;
		let validator_id = old.validator_id;
		Self { time_slot, kind, validator_index, validator_id }
	}
}

impl TryFrom<DisputeProof> for super::v9::slashing::LegacyDisputeProof {
	type Error = ();

	fn try_from(value: DisputeProof) -> Result<Self, Self::Error> {
		Ok(Self {
			time_slot: value.time_slot,
			kind: value.kind.try_into()?,
			validator_index: value.validator_index,
			validator_id: value.validator_id,
		})
	}
}
