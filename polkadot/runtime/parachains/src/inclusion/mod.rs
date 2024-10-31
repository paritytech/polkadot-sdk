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

//! The inclusion pallet is responsible for inclusion and availability of scheduled parachains.
//!
//! It is responsible for carrying candidates from being backable to being backed, and then from
//! backed to included.

use crate::{
	configuration::{self, HostConfiguration},
	disputes, dmp, hrmp,
	paras::{self, UpgradeStrategy},
	scheduler,
	shared::{self, AllowedRelayParentsTracker},
	util::make_persisted_validation_data_with_parent,
};
use alloc::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet, vec_deque::VecDeque},
	vec,
	vec::Vec,
};
use bitvec::{order::Lsb0 as BitOrderLsb0, vec::BitVec};
use codec::{Decode, Encode};
use core::fmt;
use frame_support::{
	defensive,
	pallet_prelude::*,
	traits::{EnqueueMessage, Footprint, QueueFootprint},
	BoundedSlice,
};
use frame_system::pallet_prelude::*;
use pallet_message_queue::OnQueueChanged;
use polkadot_primitives::{
	effective_minimum_backing_votes, supermajority_threshold,
	vstaging::{
		BackedCandidate, CandidateDescriptorV2 as CandidateDescriptor,
		CandidateReceiptV2 as CandidateReceipt,
		CommittedCandidateReceiptV2 as CommittedCandidateReceipt,
	},
	well_known_keys, CandidateCommitments, CandidateHash, CoreIndex, GroupIndex, HeadData,
	Id as ParaId, SignedAvailabilityBitfields, SigningContext, UpwardMessage, ValidatorId,
	ValidatorIndex, ValidityAttestation,
};
use scale_info::TypeInfo;
use sp_runtime::{traits::One, DispatchError, SaturatedConversion, Saturating};

pub use pallet::*;

#[cfg(test)]
pub(crate) mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod migration;

pub trait WeightInfo {
	/// Weight for `enact_candidate` extrinsic given the number of sent messages
	/// (ump, hrmp) and whether there is a new code for a runtime upgrade.
	///
	/// NOTE: due to a shortcoming of the current benchmarking framework,
	/// we use `u32` for the code upgrade, even though it is a `bool`.
	fn enact_candidate(u: u32, h: u32, c: u32) -> Weight;
}

pub struct TestWeightInfo;
impl WeightInfo for TestWeightInfo {
	fn enact_candidate(_u: u32, _h: u32, _c: u32) -> Weight {
		Weight::zero()
	}
}

impl WeightInfo for () {
	fn enact_candidate(_u: u32, _h: u32, _c: u32) -> Weight {
		Weight::zero()
	}
}

/// Maximum value that `config.max_upward_message_size` can be set to.
///
/// This is used for benchmarking sanely bounding relevant storage items. It is expected from the
/// `configuration` pallet to check these values before setting.
pub const MAX_UPWARD_MESSAGE_SIZE_BOUND: u32 = 128 * 1024;

/// A backed candidate pending availability.
#[derive(Encode, Decode, PartialEq, TypeInfo, Clone)]
#[cfg_attr(test, derive(Debug))]
pub struct CandidatePendingAvailability<H, N> {
	/// The availability core this is assigned to.
	core: CoreIndex,
	/// The candidate hash.
	hash: CandidateHash,
	/// The candidate descriptor.
	descriptor: CandidateDescriptor<H>,
	/// The candidate commitments.
	commitments: CandidateCommitments,
	/// The received availability votes. One bit per validator.
	availability_votes: BitVec<u8, BitOrderLsb0>,
	/// The backers of the candidate pending availability.
	backers: BitVec<u8, BitOrderLsb0>,
	/// The block number of the relay-parent of the receipt.
	relay_parent_number: N,
	/// The block number of the relay-chain block this was backed in.
	backed_in_number: N,
	/// The group index backing this block.
	backing_group: GroupIndex,
}

impl<H, N> CandidatePendingAvailability<H, N> {
	/// Get the availability votes on the candidate.
	pub(crate) fn availability_votes(&self) -> &BitVec<u8, BitOrderLsb0> {
		&self.availability_votes
	}

	/// Get the relay-chain block number this was backed in.
	pub(crate) fn backed_in_number(&self) -> N
	where
		N: Clone,
	{
		self.backed_in_number.clone()
	}

	/// Get the core index.
	pub(crate) fn core_occupied(&self) -> CoreIndex {
		self.core
	}

	/// Get the candidate hash.
	pub(crate) fn candidate_hash(&self) -> CandidateHash {
		self.hash
	}

	/// Get the candidate descriptor.
	pub(crate) fn candidate_descriptor(&self) -> &CandidateDescriptor<H> {
		&self.descriptor
	}

	/// Get the candidate commitments.
	pub(crate) fn candidate_commitments(&self) -> &CandidateCommitments {
		&self.commitments
	}

	/// Get the candidate's relay parent's number.
	pub(crate) fn relay_parent_number(&self) -> N
	where
		N: Clone,
	{
		self.relay_parent_number.clone()
	}

	#[cfg(any(feature = "runtime-benchmarks", test))]
	pub(crate) fn new(
		core: CoreIndex,
		hash: CandidateHash,
		descriptor: CandidateDescriptor<H>,
		commitments: CandidateCommitments,
		availability_votes: BitVec<u8, BitOrderLsb0>,
		backers: BitVec<u8, BitOrderLsb0>,
		relay_parent_number: N,
		backed_in_number: N,
		backing_group: GroupIndex,
	) -> Self {
		Self {
			core,
			hash,
			descriptor,
			commitments,
			availability_votes,
			backers,
			relay_parent_number,
			backed_in_number,
			backing_group,
		}
	}
}

/// A hook for applying validator rewards
pub trait RewardValidators {
	// Reward the validators with the given indices for issuing backing statements.
	fn reward_backing(validators: impl IntoIterator<Item = ValidatorIndex>);
	// Reward the validators with the given indices for issuing availability bitfields.
	// Validators are sent to this hook when they have contributed to the availability
	// of a candidate by setting a bit in their bitfield.
	fn reward_bitfields(validators: impl IntoIterator<Item = ValidatorIndex>);
}

/// Reads the footprint of queues for a specific origin type.
pub trait QueueFootprinter {
	type Origin;

	fn message_count(origin: Self::Origin) -> u64;
}

impl QueueFootprinter for () {
	type Origin = UmpQueueId;

	fn message_count(_: Self::Origin) -> u64 {
		0
	}
}

/// Aggregate message origin for the `MessageQueue` pallet.
///
/// Can be extended to serve further use-cases besides just UMP. Is stored in storage, so any change
/// to existing values will require a migration.
#[derive(Encode, Decode, Clone, MaxEncodedLen, Eq, PartialEq, RuntimeDebug, TypeInfo)]
pub enum AggregateMessageOrigin {
	/// Inbound upward message.
	#[codec(index = 0)]
	Ump(UmpQueueId),
}

/// Identifies a UMP queue inside the `MessageQueue` pallet.
///
/// It is written in verbose form since future variants like `Here` and `Bridged` are already
/// foreseeable.
#[derive(Encode, Decode, Clone, MaxEncodedLen, Eq, PartialEq, RuntimeDebug, TypeInfo)]
pub enum UmpQueueId {
	/// The message originated from this parachain.
	#[codec(index = 0)]
	Para(ParaId),
}

#[cfg(feature = "runtime-benchmarks")]
impl From<u32> for AggregateMessageOrigin {
	fn from(n: u32) -> Self {
		// Some dummy for the benchmarks.
		Self::Ump(UmpQueueId::Para(n.into()))
	}
}

/// The maximal length of a UMP message.
pub type MaxUmpMessageLenOf<T> =
	<<T as Config>::MessageQueue as EnqueueMessage<AggregateMessageOrigin>>::MaxMessageLen;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);
	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config:
		frame_system::Config
		+ shared::Config
		+ paras::Config
		+ dmp::Config
		+ hrmp::Config
		+ configuration::Config
		+ scheduler::Config
	{
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		type DisputesHandler: disputes::DisputesHandler<BlockNumberFor<Self>>;
		type RewardValidators: RewardValidators;

		/// The system message queue.
		///
		/// The message queue provides general queueing and processing functionality. Currently it
		/// replaces the old `UMP` dispatch queue. Other use-cases can be implemented as well by
		/// adding new variants to `AggregateMessageOrigin`.
		type MessageQueue: EnqueueMessage<AggregateMessageOrigin>;

		/// Weight info for the calls of this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A candidate was backed. `[candidate, head_data]`
		CandidateBacked(CandidateReceipt<T::Hash>, HeadData, CoreIndex, GroupIndex),
		/// A candidate was included. `[candidate, head_data]`
		CandidateIncluded(CandidateReceipt<T::Hash>, HeadData, CoreIndex, GroupIndex),
		/// A candidate timed out. `[candidate, head_data]`
		CandidateTimedOut(CandidateReceipt<T::Hash>, HeadData, CoreIndex),
		/// Some upward messages have been received and will be processed.
		UpwardMessagesReceived { from: ParaId, count: u32 },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Validator index out of bounds.
		ValidatorIndexOutOfBounds,
		/// Candidate submitted but para not scheduled.
		UnscheduledCandidate,
		/// Head data exceeds the configured maximum.
		HeadDataTooLarge,
		/// Code upgrade prematurely.
		PrematureCodeUpgrade,
		/// Output code is too large
		NewCodeTooLarge,
		/// The candidate's relay-parent was not allowed. Either it was
		/// not recent enough or it didn't advance based on the last parachain block.
		DisallowedRelayParent,
		/// Failed to compute group index for the core: either it's out of bounds
		/// or the relay parent doesn't belong to the current session.
		InvalidAssignment,
		/// Invalid group index in core assignment.
		InvalidGroupIndex,
		/// Insufficient (non-majority) backing.
		InsufficientBacking,
		/// Invalid (bad signature, unknown validator, etc.) backing.
		InvalidBacking,
		/// The validation data hash does not match expected.
		ValidationDataHashMismatch,
		/// The downward message queue is not processed correctly.
		IncorrectDownwardMessageHandling,
		/// At least one upward message sent does not pass the acceptance criteria.
		InvalidUpwardMessages,
		/// The candidate didn't follow the rules of HRMP watermark advancement.
		HrmpWatermarkMishandling,
		/// The HRMP messages sent by the candidate is not valid.
		InvalidOutboundHrmp,
		/// The validation code hash of the candidate is not valid.
		InvalidValidationCodeHash,
		/// The `para_head` hash in the candidate descriptor doesn't match the hash of the actual
		/// para head in the commitments.
		ParaHeadMismatch,
	}

	/// Candidates pending availability by `ParaId`. They form a chain starting from the latest
	/// included head of the para.
	/// Use a different prefix post-migration to v1, since the v0 `PendingAvailability` storage
	/// would otherwise have the exact same prefix which could cause undefined behaviour when doing
	/// the migration.
	#[pallet::storage]
	#[pallet::storage_prefix = "V1"]
	pub(crate) type PendingAvailability<T: Config> = StorageMap<
		_,
		Twox64Concat,
		ParaId,
		VecDeque<CandidatePendingAvailability<T::Hash, BlockNumberFor<T>>>,
	>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {}
}

const LOG_TARGET: &str = "runtime::inclusion";

/// The reason that a candidate's outputs were rejected for.
#[derive(Debug)]
enum AcceptanceCheckErr {
	HeadDataTooLarge,
	/// Code upgrades are not permitted at the current time.
	PrematureCodeUpgrade,
	/// The new runtime blob is too large.
	NewCodeTooLarge,
	/// The candidate violated this DMP acceptance criteria.
	ProcessedDownwardMessages,
	/// The candidate violated this UMP acceptance criteria.
	UpwardMessages,
	/// The candidate violated this HRMP watermark acceptance criteria.
	HrmpWatermark,
	/// The candidate violated this outbound HRMP acceptance criteria.
	OutboundHrmp,
}

impl From<dmp::ProcessedDownwardMessagesAcceptanceErr> for AcceptanceCheckErr {
	fn from(_: dmp::ProcessedDownwardMessagesAcceptanceErr) -> Self {
		Self::ProcessedDownwardMessages
	}
}

impl From<UmpAcceptanceCheckErr> for AcceptanceCheckErr {
	fn from(_: UmpAcceptanceCheckErr) -> Self {
		Self::UpwardMessages
	}
}

impl<BlockNumber> From<hrmp::HrmpWatermarkAcceptanceErr<BlockNumber>> for AcceptanceCheckErr {
	fn from(_: hrmp::HrmpWatermarkAcceptanceErr<BlockNumber>) -> Self {
		Self::HrmpWatermark
	}
}

impl From<hrmp::OutboundHrmpAcceptanceErr> for AcceptanceCheckErr {
	fn from(_: hrmp::OutboundHrmpAcceptanceErr) -> Self {
		Self::OutboundHrmp
	}
}

/// An error returned by [`Pallet::check_upward_messages`] that indicates a violation of one of
/// acceptance criteria rules.
#[cfg_attr(test, derive(PartialEq))]
#[allow(dead_code)]
pub(crate) enum UmpAcceptanceCheckErr {
	/// The maximal number of messages that can be submitted in one batch was exceeded.
	MoreMessagesThanPermitted { sent: u32, permitted: u32 },
	/// The maximal size of a single message was exceeded.
	MessageSize { idx: u32, msg_size: u32, max_size: u32 },
	/// The allowed number of messages in the queue was exceeded.
	CapacityExceeded { count: u64, limit: u64 },
	/// The allowed combined message size in the queue was exceeded.
	TotalSizeExceeded { total_size: u64, limit: u64 },
	/// A para-chain cannot send UMP messages while it is offboarding.
	IsOffboarding,
	/// The allowed number of `UMPSignal` messages in the queue was exceeded.
	/// Currenly only one such message is allowed.
	TooManyUMPSignals { count: u32 },
	/// The UMP queue contains an invalid `UMPSignal`
	NoUmpSignal,
}

impl fmt::Debug for UmpAcceptanceCheckErr {
	fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
		match *self {
			UmpAcceptanceCheckErr::MoreMessagesThanPermitted { sent, permitted } => write!(
				fmt,
				"more upward messages than permitted by config ({} > {})",
				sent, permitted,
			),
			UmpAcceptanceCheckErr::MessageSize { idx, msg_size, max_size } => write!(
				fmt,
				"upward message idx {} larger than permitted by config ({} > {})",
				idx, msg_size, max_size,
			),
			UmpAcceptanceCheckErr::CapacityExceeded { count, limit } => write!(
				fmt,
				"the ump queue would have more items than permitted by config ({} > {})",
				count, limit,
			),
			UmpAcceptanceCheckErr::TotalSizeExceeded { total_size, limit } => write!(
				fmt,
				"the ump queue would have grown past the max size permitted by config ({} > {})",
				total_size, limit,
			),
			UmpAcceptanceCheckErr::IsOffboarding => {
				write!(fmt, "upward message rejected because the para is off-boarding")
			},
			UmpAcceptanceCheckErr::TooManyUMPSignals { count } => {
				write!(fmt, "the ump queue has too many `UMPSignal` messages ({} > 1 )", count)
			},
			UmpAcceptanceCheckErr::NoUmpSignal => {
				write!(fmt, "Required UMP signal not found")
			},
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Block initialization logic, called by initializer.
	pub(crate) fn initializer_initialize(_now: BlockNumberFor<T>) -> Weight {
		Weight::zero()
	}

	/// Block finalization logic, called by initializer.
	pub(crate) fn initializer_finalize() {}

	/// Handle an incoming session change.
	pub(crate) fn initializer_on_new_session(
		_notification: &crate::initializer::SessionChangeNotification<BlockNumberFor<T>>,
		outgoing_paras: &[ParaId],
	) {
		// unlike most drain methods, drained elements are not cleared on `Drop` of the iterator
		// and require consumption.
		for _ in PendingAvailability::<T>::drain() {}

		Self::cleanup_outgoing_ump_dispatch_queues(outgoing_paras);
	}

	pub(crate) fn cleanup_outgoing_ump_dispatch_queues(outgoing: &[ParaId]) {
		for outgoing_para in outgoing {
			Self::cleanup_outgoing_ump_dispatch_queue(*outgoing_para);
		}
	}

	pub(crate) fn cleanup_outgoing_ump_dispatch_queue(para: ParaId) {
		T::MessageQueue::sweep_queue(AggregateMessageOrigin::Ump(UmpQueueId::Para(para)));
	}

	pub(crate) fn get_occupied_cores(
	) -> impl Iterator<Item = (CoreIndex, CandidatePendingAvailability<T::Hash, BlockNumberFor<T>>)>
	{
		PendingAvailability::<T>::iter_values().flat_map(|pending_candidates| {
			pending_candidates.into_iter().map(|c| (c.core, c.clone()))
		})
	}

	/// Extract the freed cores based on cores that became available.
	///
	/// Bitfields are expected to have been sanitized already. E.g. via `sanitize_bitfields`!
	///
	/// Updates storage items `PendingAvailability`.
	///
	/// Returns a `Vec` of `CandidateHash`es and their respective `AvailabilityCore`s that became
	/// available, and cores free.
	pub(crate) fn update_pending_availability_and_get_freed_cores(
		validators: &[ValidatorId],
		signed_bitfields: SignedAvailabilityBitfields,
	) -> (Weight, Vec<(CoreIndex, CandidateHash)>) {
		let threshold = availability_threshold(validators.len());

		let mut votes_per_core: BTreeMap<CoreIndex, BTreeSet<ValidatorIndex>> = BTreeMap::new();

		for (checked_bitfield, validator_index) in
			signed_bitfields.into_iter().map(|signed_bitfield| {
				let validator_idx = signed_bitfield.validator_index();
				let checked_bitfield = signed_bitfield.into_payload();
				(checked_bitfield, validator_idx)
			}) {
			for (bit_idx, _) in checked_bitfield.0.iter().enumerate().filter(|(_, is_av)| **is_av) {
				let core_index = CoreIndex(bit_idx as u32);
				votes_per_core
					.entry(core_index)
					.or_insert_with(|| BTreeSet::new())
					.insert(validator_index);
			}
		}

		let mut freed_cores = vec![];
		let mut weight = Weight::zero();

		let pending_paraids: Vec<_> = PendingAvailability::<T>::iter_keys().collect();
		for paraid in pending_paraids {
			PendingAvailability::<T>::mutate(paraid, |candidates| {
				if let Some(candidates) = candidates {
					let mut last_enacted_index: Option<usize> = None;

					for (candidate_index, candidate) in candidates.iter_mut().enumerate() {
						if let Some(validator_indices) = votes_per_core.remove(&candidate.core) {
							for validator_index in validator_indices.iter() {
								// defensive check - this is constructed by loading the
								// availability bitfield record, which is always `Some` if
								// the core is occupied - that's why we're here.
								if let Some(mut bit) =
									candidate.availability_votes.get_mut(validator_index.0 as usize)
								{
									*bit = true;
								}
							}
						}

						// We check for the candidate's availability even if we didn't get any new
						// bitfields for its core, as it may have already been available at a
						// previous block but wasn't enacted due to its predecessors not being
						// available.
						if candidate.availability_votes.count_ones() >= threshold {
							// We can only enact a candidate if we've enacted all of its
							// predecessors already.
							let can_enact = if candidate_index == 0 {
								last_enacted_index == None
							} else {
								let prev_candidate_index = usize::try_from(candidate_index - 1)
									.expect("Previous `if` would have caught a 0 candidate index.");
								matches!(last_enacted_index, Some(old_index) if old_index == prev_candidate_index)
							};

							if can_enact {
								last_enacted_index = Some(candidate_index);
							}
						}
					}

					// Trim the pending availability candidates storage and enact candidates of this
					// para now.
					if let Some(last_enacted_index) = last_enacted_index {
						let evicted_candidates = candidates.drain(0..=last_enacted_index);
						for candidate in evicted_candidates {
							freed_cores.push((candidate.core, candidate.hash));

							let receipt = CommittedCandidateReceipt {
								descriptor: candidate.descriptor,
								commitments: candidate.commitments,
							};

							let has_runtime_upgrade =
								receipt.commitments.new_validation_code.as_ref().map_or(0, |_| 1);
							let u = receipt.commitments.upward_messages.len() as u32;
							let h = receipt.commitments.horizontal_messages.len() as u32;
							let enact_weight = <T as Config>::WeightInfo::enact_candidate(
								u,
								h,
								has_runtime_upgrade,
							);
							Self::enact_candidate(
								candidate.relay_parent_number,
								receipt,
								candidate.backers,
								candidate.availability_votes,
								candidate.core,
								candidate.backing_group,
							);
							weight.saturating_accrue(enact_weight);
						}
					}
				}
			});
		}

		(weight, freed_cores)
	}

	/// Process candidates that have been backed. Provide a set of
	/// candidates along with their scheduled cores.
	///
	/// Candidates of the same paraid should be sorted according to their dependency order (they
	/// should form a chain). If this condition is not met, this function will return an error.
	/// (This really should not happen here, if the candidates were properly sanitised in
	/// paras_inherent).
	pub(crate) fn process_candidates<GV>(
		allowed_relay_parents: &AllowedRelayParentsTracker<T::Hash, BlockNumberFor<T>>,
		candidates: &BTreeMap<ParaId, Vec<(BackedCandidate<T::Hash>, CoreIndex)>>,
		group_validators: GV,
		core_index_enabled: bool,
	) -> Result<
		Vec<(CandidateReceipt<T::Hash>, Vec<(ValidatorIndex, ValidityAttestation)>)>,
		DispatchError,
	>
	where
		GV: Fn(GroupIndex) -> Option<Vec<ValidatorIndex>>,
	{
		if candidates.is_empty() {
			return Ok(Default::default())
		}

		let now = frame_system::Pallet::<T>::block_number();
		let validators = shared::ActiveValidatorKeys::<T>::get();

		// Collect candidate receipts with backers.
		let mut candidate_receipt_with_backing_validator_indices =
			Vec::with_capacity(candidates.len());

		for (para_id, para_candidates) in candidates {
			let mut latest_head_data = match Self::para_latest_head_data(para_id) {
				None => {
					defensive!("Latest included head data for paraid {:?} is None", para_id);
					continue
				},
				Some(latest_head_data) => latest_head_data,
			};

			for (candidate, core) in para_candidates.iter() {
				let candidate_hash = candidate.candidate().hash();

				// The previous context is None, as it's already checked during candidate
				// sanitization.
				let check_ctx = CandidateCheckContext::<T>::new(None);
				let relay_parent_number = check_ctx.verify_backed_candidate(
					&allowed_relay_parents,
					candidate.candidate(),
					latest_head_data.clone(),
				)?;

				// The candidate based upon relay parent `N` should be backed by a
				// group assigned to core at block `N + 1`. Thus,
				// `relay_parent_number + 1` will always land in the current
				// session.
				let group_idx = scheduler::Pallet::<T>::group_assigned_to_core(
					*core,
					relay_parent_number + One::one(),
				)
				.ok_or_else(|| {
					log::warn!(
						target: LOG_TARGET,
						"Failed to compute group index for candidate {:?}",
						candidate_hash
					);
					Error::<T>::InvalidAssignment
				})?;
				let group_vals =
					group_validators(group_idx).ok_or_else(|| Error::<T>::InvalidGroupIndex)?;

				// Check backing vote count and validity.
				let (backers, backer_idx_and_attestation) = Self::check_backing_votes(
					candidate,
					&validators,
					group_vals,
					core_index_enabled,
				)?;

				// Found a valid candidate.
				latest_head_data = candidate.candidate().commitments.head_data.clone();
				candidate_receipt_with_backing_validator_indices
					.push((candidate.receipt(), backer_idx_and_attestation));

				// Update storage now
				PendingAvailability::<T>::mutate(&para_id, |pending_availability| {
					let new_candidate = CandidatePendingAvailability {
						core: *core,
						hash: candidate_hash,
						descriptor: candidate.candidate().descriptor.clone(),
						commitments: candidate.candidate().commitments.clone(),
						// initialize all availability votes to 0.
						availability_votes: bitvec::bitvec![u8, BitOrderLsb0; 0; validators.len()],
						relay_parent_number,
						backers: backers.to_bitvec(),
						backed_in_number: now,
						backing_group: group_idx,
					};

					if let Some(pending_availability) = pending_availability {
						pending_availability.push_back(new_candidate);
					} else {
						*pending_availability =
							Some([new_candidate].into_iter().collect::<VecDeque<_>>())
					}
				});

				// Deposit backed event.
				Self::deposit_event(Event::<T>::CandidateBacked(
					candidate.candidate().to_plain(),
					candidate.candidate().commitments.head_data.clone(),
					*core,
					group_idx,
				));
			}
		}

		Ok(candidate_receipt_with_backing_validator_indices)
	}

	// Get the latest backed output head data of this para (including pending availability).
	pub(crate) fn para_latest_head_data(para_id: &ParaId) -> Option<HeadData> {
		match PendingAvailability::<T>::get(para_id).and_then(|pending_candidates| {
			pending_candidates.back().map(|x| x.commitments.head_data.clone())
		}) {
			Some(head_data) => Some(head_data),
			None => paras::Heads::<T>::get(para_id),
		}
	}

	// Get the relay parent number of the most recent candidate (including pending availability).
	pub(crate) fn para_most_recent_context(para_id: &ParaId) -> Option<BlockNumberFor<T>> {
		match PendingAvailability::<T>::get(para_id)
			.and_then(|pending_candidates| pending_candidates.back().map(|x| x.relay_parent_number))
		{
			Some(relay_parent_number) => Some(relay_parent_number),
			None => paras::MostRecentContext::<T>::get(para_id),
		}
	}

	fn check_backing_votes(
		backed_candidate: &BackedCandidate<T::Hash>,
		validators: &[ValidatorId],
		group_vals: Vec<ValidatorIndex>,
		core_index_enabled: bool,
	) -> Result<(BitVec<u8, BitOrderLsb0>, Vec<(ValidatorIndex, ValidityAttestation)>), Error<T>> {
		let minimum_backing_votes = configuration::ActiveConfig::<T>::get().minimum_backing_votes;

		let mut backers = bitvec::bitvec![u8, BitOrderLsb0; 0; validators.len()];
		let signing_context = SigningContext {
			parent_hash: backed_candidate.descriptor().relay_parent(),
			session_index: shared::CurrentSessionIndex::<T>::get(),
		};

		let (validator_indices, _) =
			backed_candidate.validator_indices_and_core_index(core_index_enabled);

		// check the signatures in the backing and that it is a majority.
		let maybe_amount_validated = polkadot_primitives::check_candidate_backing(
			backed_candidate.candidate().hash(),
			backed_candidate.validity_votes(),
			validator_indices,
			&signing_context,
			group_vals.len(),
			|intra_group_vi| {
				group_vals
					.get(intra_group_vi)
					.and_then(|vi| validators.get(vi.0 as usize))
					.map(|v| v.clone())
			},
		);

		match maybe_amount_validated {
			Ok(amount_validated) => ensure!(
				amount_validated >=
					effective_minimum_backing_votes(group_vals.len(), minimum_backing_votes),
				Error::<T>::InsufficientBacking,
			),
			Err(()) => {
				Err(Error::<T>::InvalidBacking)?;
			},
		}

		let mut backer_idx_and_attestation =
			Vec::<(ValidatorIndex, ValidityAttestation)>::with_capacity(
				validator_indices.count_ones(),
			);

		for ((bit_idx, _), attestation) in validator_indices
			.iter()
			.enumerate()
			.filter(|(_, signed)| **signed)
			.zip(backed_candidate.validity_votes().iter().cloned())
		{
			let val_idx = group_vals.get(bit_idx).expect("this query succeeded above; qed");
			backer_idx_and_attestation.push((*val_idx, attestation));

			backers.set(val_idx.0 as _, true);
		}

		Ok((backers, backer_idx_and_attestation))
	}

	/// Run the acceptance criteria checks on the given candidate commitments.
	pub(crate) fn check_validation_outputs_for_runtime_api(
		para_id: ParaId,
		relay_parent_number: BlockNumberFor<T>,
		validation_outputs: polkadot_primitives::CandidateCommitments,
	) -> bool {
		let prev_context = Self::para_most_recent_context(&para_id);
		let check_ctx = CandidateCheckContext::<T>::new(prev_context);

		if let Err(err) = check_ctx.check_validation_outputs(
			para_id,
			relay_parent_number,
			&validation_outputs.head_data,
			&validation_outputs.new_validation_code,
			validation_outputs.processed_downward_messages,
			&validation_outputs.upward_messages,
			BlockNumberFor::<T>::from(validation_outputs.hrmp_watermark),
			&validation_outputs.horizontal_messages,
		) {
			log::debug!(
				target: LOG_TARGET,
				"Validation outputs checking for parachain `{}` failed, error: {:?}",
				u32::from(para_id), err
			);
			false
		} else {
			true
		}
	}

	fn enact_candidate(
		relay_parent_number: BlockNumberFor<T>,
		receipt: CommittedCandidateReceipt<T::Hash>,
		backers: BitVec<u8, BitOrderLsb0>,
		availability_votes: BitVec<u8, BitOrderLsb0>,
		core_index: CoreIndex,
		backing_group: GroupIndex,
	) {
		let plain = receipt.to_plain();
		let commitments = receipt.commitments;
		let config = configuration::ActiveConfig::<T>::get();

		T::RewardValidators::reward_backing(
			backers
				.iter()
				.enumerate()
				.filter(|(_, backed)| **backed)
				.map(|(i, _)| ValidatorIndex(i as _)),
		);

		T::RewardValidators::reward_bitfields(
			availability_votes
				.iter()
				.enumerate()
				.filter(|(_, voted)| **voted)
				.map(|(i, _)| ValidatorIndex(i as _)),
		);

		if let Some(new_code) = commitments.new_validation_code {
			// Block number of candidate's inclusion.
			let now = frame_system::Pallet::<T>::block_number();

			paras::Pallet::<T>::schedule_code_upgrade(
				receipt.descriptor.para_id(),
				new_code,
				now,
				&config,
				UpgradeStrategy::SetGoAheadSignal,
			);
		}

		// enact the messaging facet of the candidate.
		dmp::Pallet::<T>::prune_dmq(
			receipt.descriptor.para_id(),
			commitments.processed_downward_messages,
		);
		Self::receive_upward_messages(
			receipt.descriptor.para_id(),
			commitments.upward_messages.as_slice(),
		);
		hrmp::Pallet::<T>::prune_hrmp(
			receipt.descriptor.para_id(),
			BlockNumberFor::<T>::from(commitments.hrmp_watermark),
		);
		hrmp::Pallet::<T>::queue_outbound_hrmp(
			receipt.descriptor.para_id(),
			commitments.horizontal_messages,
		);

		Self::deposit_event(Event::<T>::CandidateIncluded(
			plain,
			commitments.head_data.clone(),
			core_index,
			backing_group,
		));

		paras::Pallet::<T>::note_new_head(
			receipt.descriptor.para_id(),
			commitments.head_data,
			relay_parent_number,
		);
	}

	pub(crate) fn relay_dispatch_queue_size(para_id: ParaId) -> (u32, u32) {
		let fp = T::MessageQueue::footprint(AggregateMessageOrigin::Ump(UmpQueueId::Para(para_id)));
		(fp.storage.count as u32, fp.storage.size as u32)
	}

	/// Check that all the upward messages sent by a candidate pass the acceptance criteria.
	pub(crate) fn check_upward_messages(
		config: &HostConfiguration<BlockNumberFor<T>>,
		para: ParaId,
		upward_messages: &[UpwardMessage],
	) -> Result<(), UmpAcceptanceCheckErr> {
		// Filter any pending UMP signals and the separator.
		let upward_messages = if let Some(separator_index) =
			upward_messages.iter().position(|message| message.is_empty())
		{
			let (upward_messages, ump_signals) = upward_messages.split_at(separator_index);

			if ump_signals.len() > 2 {
				return Err(UmpAcceptanceCheckErr::TooManyUMPSignals {
					count: ump_signals.len() as u32,
				})
			}

			if ump_signals.len() == 1 {
				return Err(UmpAcceptanceCheckErr::NoUmpSignal)
			}

			upward_messages
		} else {
			upward_messages
		};

		// Cannot send UMP messages while off-boarding.
		if paras::Pallet::<T>::is_offboarding(para) {
			ensure!(upward_messages.is_empty(), UmpAcceptanceCheckErr::IsOffboarding);
		}

		let additional_msgs = upward_messages.len() as u32;
		if additional_msgs > config.max_upward_message_num_per_candidate {
			return Err(UmpAcceptanceCheckErr::MoreMessagesThanPermitted {
				sent: additional_msgs,
				permitted: config.max_upward_message_num_per_candidate,
			})
		}

		let (para_queue_count, mut para_queue_size) = Self::relay_dispatch_queue_size(para);

		if para_queue_count.saturating_add(additional_msgs) > config.max_upward_queue_count {
			return Err(UmpAcceptanceCheckErr::CapacityExceeded {
				count: para_queue_count.saturating_add(additional_msgs).into(),
				limit: config.max_upward_queue_count.into(),
			})
		}

		for (idx, msg) in upward_messages.into_iter().enumerate() {
			let msg_size = msg.len() as u32;
			if msg_size > config.max_upward_message_size {
				return Err(UmpAcceptanceCheckErr::MessageSize {
					idx: idx as u32,
					msg_size,
					max_size: config.max_upward_message_size,
				})
			}
			// make sure that the queue is not overfilled.
			// we do it here only once since returning false invalidates the whole relay-chain
			// block.
			if para_queue_size.saturating_add(msg_size) > config.max_upward_queue_size {
				return Err(UmpAcceptanceCheckErr::TotalSizeExceeded {
					total_size: para_queue_size.saturating_add(msg_size).into(),
					limit: config.max_upward_queue_size.into(),
				})
			}
			para_queue_size.saturating_accrue(msg_size);
		}

		Ok(())
	}

	/// Enqueues `upward_messages` from a `para`'s accepted candidate block.
	///
	/// This function is infallible since the candidate was already accepted and we therefore need
	/// to deal with the messages as given. Messages that are too long will be ignored since such
	/// candidates should have already been rejected in [`Self::check_upward_messages`].
	pub(crate) fn receive_upward_messages(para: ParaId, upward_messages: &[Vec<u8>]) {
		let bounded = upward_messages
			.iter()
			// Stop once we hit the `UMPSignal` separator.
			.take_while(|message| !message.is_empty())
			.filter_map(|d| {
				BoundedSlice::try_from(&d[..])
					.inspect_err(|_| {
						defensive!("Accepted candidate contains too long msg, len=", d.len());
					})
					.ok()
			})
			.collect();
		Self::receive_bounded_upward_messages(para, bounded)
	}

	/// Enqueues storage-bounded `upward_messages` from a `para`'s accepted candidate block.
	pub(crate) fn receive_bounded_upward_messages(
		para: ParaId,
		messages: Vec<BoundedSlice<'_, u8, MaxUmpMessageLenOf<T>>>,
	) {
		let count = messages.len() as u32;
		if count == 0 {
			return
		}

		T::MessageQueue::enqueue_messages(
			messages.into_iter(),
			AggregateMessageOrigin::Ump(UmpQueueId::Para(para)),
		);
		Self::deposit_event(Event::UpwardMessagesReceived { from: para, count });
	}

	/// Cleans up all timed out candidates as well as their descendant candidates.
	///
	/// Returns a vector of cleaned-up core IDs.
	pub(crate) fn free_timedout() -> Vec<CoreIndex> {
		let timeout_pred = scheduler::Pallet::<T>::availability_timeout_predicate();

		let timed_out: Vec<_> = Self::free_failed_cores(
			|candidate| timeout_pred(candidate.backed_in_number).timed_out,
			None,
		)
		.collect();

		let mut timed_out_cores = Vec::with_capacity(timed_out.len());
		for candidate in timed_out.iter() {
			timed_out_cores.push(candidate.core);

			let receipt = CandidateReceipt {
				descriptor: candidate.descriptor.clone(),
				commitments_hash: candidate.commitments.hash(),
			};

			Self::deposit_event(Event::<T>::CandidateTimedOut(
				receipt,
				candidate.commitments.head_data.clone(),
				candidate.core,
			));
		}

		timed_out_cores
	}

	/// Cleans up all cores pending availability occupied by one of the disputed candidates or which
	/// are descendants of a disputed candidate.
	///
	/// Returns a vector of cleaned-up core IDs, along with the evicted candidate hashes.
	pub(crate) fn free_disputed(
		disputed: &BTreeSet<CandidateHash>,
	) -> Vec<(CoreIndex, CandidateHash)> {
		Self::free_failed_cores(
			|candidate| disputed.contains(&candidate.hash),
			Some(disputed.len()),
		)
		.map(|candidate| (candidate.core, candidate.hash))
		.collect()
	}

	// Clean up cores whose candidates are deemed as failed by the predicate. `pred` returns true if
	// a candidate is considered failed.
	// A failed candidate also frees all subsequent cores which hold descendants of said candidate.
	fn free_failed_cores<
		P: Fn(&CandidatePendingAvailability<T::Hash, BlockNumberFor<T>>) -> bool,
	>(
		pred: P,
		capacity_hint: Option<usize>,
	) -> impl Iterator<Item = CandidatePendingAvailability<T::Hash, BlockNumberFor<T>>> {
		let mut earliest_dropped_indices: BTreeMap<ParaId, usize> = BTreeMap::new();

		for (para_id, pending_candidates) in PendingAvailability::<T>::iter() {
			// We assume that pending candidates are stored in dependency order. So we need to store
			// the earliest dropped candidate. All others that follow will get freed as well.
			let mut earliest_dropped_idx = None;
			for (index, candidate) in pending_candidates.iter().enumerate() {
				if pred(candidate) {
					earliest_dropped_idx = Some(index);
					// Since we're looping the candidates in dependency order, we've found the
					// earliest failed index for this paraid.
					break;
				}
			}

			if let Some(earliest_dropped_idx) = earliest_dropped_idx {
				earliest_dropped_indices.insert(para_id, earliest_dropped_idx);
			}
		}

		let mut cleaned_up_cores =
			if let Some(capacity) = capacity_hint { Vec::with_capacity(capacity) } else { vec![] };

		for (para_id, earliest_dropped_idx) in earliest_dropped_indices {
			// Do cleanups and record the cleaned up cores
			PendingAvailability::<T>::mutate(&para_id, |record| {
				if let Some(record) = record {
					let cleaned_up = record.drain(earliest_dropped_idx..);
					cleaned_up_cores.extend(cleaned_up);
				}
			});
		}

		cleaned_up_cores.into_iter()
	}

	/// Forcibly enact the pending candidates of the given paraid as though they had been deemed
	/// available by bitfields.
	///
	/// Is a no-op if there is no candidate pending availability for this para-id.
	/// If there are multiple candidates pending availability for this para-id, it will enact all of
	/// them. This should generally not be used but it is useful during execution of Runtime APIs,
	/// where the changes to the state are expected to be discarded directly after.
	pub(crate) fn force_enact(para: ParaId) {
		PendingAvailability::<T>::mutate(&para, |candidates| {
			if let Some(candidates) = candidates {
				for candidate in candidates.drain(..) {
					let receipt = CommittedCandidateReceipt {
						descriptor: candidate.descriptor,
						commitments: candidate.commitments,
					};

					Self::enact_candidate(
						candidate.relay_parent_number,
						receipt,
						candidate.backers,
						candidate.availability_votes,
						candidate.core,
						candidate.backing_group,
					);
				}
			}
		});
	}

	/// Returns the first `CommittedCandidateReceipt` pending availability for the para provided, if
	/// any.
	/// A para_id could have more than one candidates pending availability, if it's using elastic
	/// scaling. These candidates form a chain. This function returns the first in the chain.
	pub(crate) fn first_candidate_pending_availability(
		para: ParaId,
	) -> Option<CommittedCandidateReceipt<T::Hash>> {
		PendingAvailability::<T>::get(&para).and_then(|p| {
			p.get(0).map(|p| CommittedCandidateReceipt {
				descriptor: p.descriptor.clone(),
				commitments: p.commitments.clone(),
			})
		})
	}

	/// Returns all the `CommittedCandidateReceipt` pending availability for the para provided, if
	/// any.
	pub(crate) fn candidates_pending_availability(
		para: ParaId,
	) -> Vec<CommittedCandidateReceipt<T::Hash>> {
		<PendingAvailability<T>>::get(&para)
			.map(|candidates| {
				candidates
					.into_iter()
					.map(|candidate| CommittedCandidateReceipt {
						descriptor: candidate.descriptor.clone(),
						commitments: candidate.commitments.clone(),
					})
					.collect()
			})
			.unwrap_or_default()
	}
}

const fn availability_threshold(n_validators: usize) -> usize {
	supermajority_threshold(n_validators)
}

impl AcceptanceCheckErr {
	/// Returns the same error so that it can be threaded through a needle of `DispatchError` and
	/// ultimately returned from a `Dispatchable`.
	fn strip_into_dispatch_err<T: Config>(self) -> Error<T> {
		use AcceptanceCheckErr::*;
		match self {
			HeadDataTooLarge => Error::<T>::HeadDataTooLarge,
			PrematureCodeUpgrade => Error::<T>::PrematureCodeUpgrade,
			NewCodeTooLarge => Error::<T>::NewCodeTooLarge,
			ProcessedDownwardMessages => Error::<T>::IncorrectDownwardMessageHandling,
			UpwardMessages => Error::<T>::InvalidUpwardMessages,
			HrmpWatermark => Error::<T>::HrmpWatermarkMishandling,
			OutboundHrmp => Error::<T>::InvalidOutboundHrmp,
		}
	}
}

impl<T: Config> OnQueueChanged<AggregateMessageOrigin> for Pallet<T> {
	// Write back the remaining queue capacity into `relay_dispatch_queue_remaining_capacity`.
	fn on_queue_changed(origin: AggregateMessageOrigin, fp: QueueFootprint) {
		let para = match origin {
			AggregateMessageOrigin::Ump(UmpQueueId::Para(p)) => p,
		};
		let QueueFootprint { storage: Footprint { count, size }, .. } = fp;
		let (count, size) = (count.saturated_into(), size.saturated_into());
		// TODO paritytech/polkadot#6283: Remove all usages of `relay_dispatch_queue_size`
		#[allow(deprecated)]
		well_known_keys::relay_dispatch_queue_size_typed(para).set((count, size));

		let config = configuration::ActiveConfig::<T>::get();
		let remaining_count = config.max_upward_queue_count.saturating_sub(count);
		let remaining_size = config.max_upward_queue_size.saturating_sub(size);
		well_known_keys::relay_dispatch_queue_remaining_capacity(para)
			.set((remaining_count, remaining_size));
	}
}

/// A collection of data required for checking a candidate.
pub(crate) struct CandidateCheckContext<T: Config> {
	config: configuration::HostConfiguration<BlockNumberFor<T>>,
	prev_context: Option<BlockNumberFor<T>>,
}

impl<T: Config> CandidateCheckContext<T> {
	pub(crate) fn new(prev_context: Option<BlockNumberFor<T>>) -> Self {
		Self { config: configuration::ActiveConfig::<T>::get(), prev_context }
	}

	/// Execute verification of the candidate.
	///
	/// Assures:
	///  * relay-parent in-bounds
	///  * code hash of commitments matches current code hash
	///  * para head in the descriptor and commitments match
	///
	/// Returns the relay-parent block number.
	pub(crate) fn verify_backed_candidate(
		&self,
		allowed_relay_parents: &AllowedRelayParentsTracker<T::Hash, BlockNumberFor<T>>,
		backed_candidate_receipt: &CommittedCandidateReceipt<<T as frame_system::Config>::Hash>,
		parent_head_data: HeadData,
	) -> Result<BlockNumberFor<T>, Error<T>> {
		let para_id = backed_candidate_receipt.descriptor.para_id();
		let relay_parent = backed_candidate_receipt.descriptor.relay_parent();

		// Check that the relay-parent is one of the allowed relay-parents.
		let (state_root, relay_parent_number) = {
			match allowed_relay_parents.acquire_info(relay_parent, self.prev_context) {
				None => return Err(Error::<T>::DisallowedRelayParent),
				Some((info, relay_parent_number)) => (info.state_root, relay_parent_number),
			}
		};

		{
			let persisted_validation_data = make_persisted_validation_data_with_parent::<T>(
				relay_parent_number,
				state_root,
				parent_head_data,
			);

			let expected = persisted_validation_data.hash();

			ensure!(
				expected == backed_candidate_receipt.descriptor.persisted_validation_data_hash(),
				Error::<T>::ValidationDataHashMismatch,
			);
		}

		let validation_code_hash = paras::CurrentCodeHash::<T>::get(para_id)
			// A candidate for a parachain without current validation code is not scheduled.
			.ok_or_else(|| Error::<T>::UnscheduledCandidate)?;
		ensure!(
			backed_candidate_receipt.descriptor.validation_code_hash() == validation_code_hash,
			Error::<T>::InvalidValidationCodeHash,
		);

		ensure!(
			backed_candidate_receipt.descriptor.para_head() ==
				backed_candidate_receipt.commitments.head_data.hash(),
			Error::<T>::ParaHeadMismatch,
		);

		if let Err(err) = self.check_validation_outputs(
			para_id,
			relay_parent_number,
			&backed_candidate_receipt.commitments.head_data,
			&backed_candidate_receipt.commitments.new_validation_code,
			backed_candidate_receipt.commitments.processed_downward_messages,
			&backed_candidate_receipt.commitments.upward_messages,
			BlockNumberFor::<T>::from(backed_candidate_receipt.commitments.hrmp_watermark),
			&backed_candidate_receipt.commitments.horizontal_messages,
		) {
			log::debug!(
				target: LOG_TARGET,
				"Validation outputs checking during inclusion of a candidate {:?} for parachain `{}` failed, error: {:?}",
				backed_candidate_receipt.hash(),
				u32::from(para_id),
				err
			);
			Err(err.strip_into_dispatch_err::<T>())?;
		};
		Ok(relay_parent_number)
	}

	/// Check the given outputs after candidate validation on whether it passes the acceptance
	/// criteria.
	///
	/// The things that are checked can be roughly divided into limits and minimums.
	///
	/// Limits are things like max message queue sizes and max head data size.
	///
	/// Minimums are things like the minimum amount of messages that must be processed
	/// by the parachain block.
	///
	/// Limits are checked against the current state. The parachain block must be acceptable
	/// by the current relay-chain state regardless of whether it was acceptable at some relay-chain
	/// state in the past.
	///
	/// Minimums are checked against the current state but modulated by
	/// considering the information available at the relay-parent of the parachain block.
	fn check_validation_outputs(
		&self,
		para_id: ParaId,
		relay_parent_number: BlockNumberFor<T>,
		head_data: &HeadData,
		new_validation_code: &Option<polkadot_primitives::ValidationCode>,
		processed_downward_messages: u32,
		upward_messages: &[polkadot_primitives::UpwardMessage],
		hrmp_watermark: BlockNumberFor<T>,
		horizontal_messages: &[polkadot_primitives::OutboundHrmpMessage<ParaId>],
	) -> Result<(), AcceptanceCheckErr> {
		ensure!(
			head_data.0.len() <= self.config.max_head_data_size as _,
			AcceptanceCheckErr::HeadDataTooLarge,
		);

		// if any, the code upgrade attempt is allowed.
		if let Some(new_validation_code) = new_validation_code {
			ensure!(
				paras::Pallet::<T>::can_upgrade_validation_code(para_id),
				AcceptanceCheckErr::PrematureCodeUpgrade,
			);
			ensure!(
				new_validation_code.0.len() <= self.config.max_code_size as _,
				AcceptanceCheckErr::NewCodeTooLarge,
			);
		}

		// check if the candidate passes the messaging acceptance criteria
		dmp::Pallet::<T>::check_processed_downward_messages(
			para_id,
			relay_parent_number,
			processed_downward_messages,
		)
		.map_err(|e| {
			log::debug!(
				target: LOG_TARGET,
				"Check processed downward messages for parachain `{}` on relay parent number `{:?}` failed, error: {:?}",
				u32::from(para_id),
				relay_parent_number,
				e
			);
			e
		})?;
		Pallet::<T>::check_upward_messages(&self.config, para_id, upward_messages).map_err(
			|e| {
				log::debug!(
					target: LOG_TARGET,
					"Check upward messages for parachain `{}` failed, error: {:?}",
					u32::from(para_id),
					e
				);
				e
			},
		)?;
		hrmp::Pallet::<T>::check_hrmp_watermark(para_id, relay_parent_number, hrmp_watermark)
			.map_err(|e| {
				log::debug!(
					target: LOG_TARGET,
					"Check hrmp watermark for parachain `{}` on relay parent number `{:?}` failed, error: {:?}",
					u32::from(para_id),
					relay_parent_number,
					e
				);
				e
			})?;
		hrmp::Pallet::<T>::check_outbound_hrmp(&self.config, para_id, horizontal_messages)
			.map_err(|e| {
				log::debug!(
					target: LOG_TARGET,
					"Check outbound hrmp for parachain `{}` failed, error: {:?}",
					u32::from(para_id),
					e
				);
				e
			})?;

		Ok(())
	}
}

impl<T: Config> QueueFootprinter for Pallet<T> {
	type Origin = UmpQueueId;

	fn message_count(origin: Self::Origin) -> u64 {
		T::MessageQueue::footprint(AggregateMessageOrigin::Ump(origin)).storage.count
	}
}
