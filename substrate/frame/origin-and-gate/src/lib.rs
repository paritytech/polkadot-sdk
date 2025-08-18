// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, Error as CodecError, FullCodec, Input};
use core::marker::PhantomData;
use frame_support::{
	dispatch::{
		ClassifyDispatch, DispatchClass, DispatchResult, DispatchResultWithPostInfo,
		GetDispatchInfo, Pays, PaysFee, PostDispatchInfo, WeighData,
	},
	pallet_prelude::*,
	traits::{EnsureOrigin, Get, IsSubType, IsType},
	weights::Weight,
	BoundedBTreeMap, BoundedVec, Parameter,
};
use frame_system::{
	self, ensure_signed,
	pallet_prelude::{BlockNumberFor, *},
};
use log::info;
use scale_info::TypeInfo;
use sp_runtime::{
	impl_tx_ext_default,
	traits::{
		AtLeast32BitUnsigned, Bounded, CheckedAdd, DispatchInfoOf, DispatchOriginOf, Dispatchable,
		Member, One, SaturatedConversion, Saturating, TransactionExtension, ValidateResult, Zero,
	},
	transaction_validity::{InvalidTransaction, ValidTransaction},
};

/// Type alias for dummy storage value
pub type DummyValueOf = BoundedVec<u8, ConstU32<1024>>;

/// Composite identifier for origins that includes both the collective ID and role.
/// Allows for distinguishing between different collectives and roles within those collectives.
#[derive(
	Clone,
	Copy,
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	Encode,
	Decode,
	RuntimeDebug,
	MaxEncodedLen,
	TypeInfo,
)]
pub struct CompositeOriginId {
	/// Identifier of collective (e.g. Technical Fellowship, Ambassador Fellowship)
	pub collective_id: u32,
	/// Role or rank within that collective
	pub role: u32,
}

impl Default for CompositeOriginId {
	fn default() -> Self {
		Self { collective_id: 0, role: 0 }
	}
}

impl From<u32> for CompositeOriginId {
	fn from(id: u32) -> Self {
		Self { collective_id: id, role: 0 }
	}
}

impl DecodeWithMemTracking for CompositeOriginId {}

// Conversion from CompositeOriginId to u64 for tests
impl From<CompositeOriginId> for u64 {
	fn from(id: CompositeOriginId) -> Self {
		// Pack collective_id and role into a u64
		// collective_id takes the upper 32 bits, role takes the lower 32 bits
		((id.collective_id as u64) << 32) | (id.role as u64)
	}
}

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub mod weights;
pub use weights::*;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

/// Timepoint represents a specific moment (block number and extrinsic index).
#[derive(
	Clone, Eq, PartialEq, Encode, Decode, Default, RuntimeDebug, MaxEncodedLen, TypeInfo, Copy,
)]
pub struct Timepoint<BlockNumber> {
	/// Block number at which timepoint was recorded.
	pub height: BlockNumber,
	/// Extrinsic index within block.
	pub index: u32,
}

// Empty implementation of this trait for types that already implement Decode
impl<BlockNumber: Decode> DecodeWithMemTracking for Timepoint<BlockNumber> {}

/// Helper struct that requires approval from two origins.
pub struct AndGate<A, B>(PhantomData<(A, B)>);

/// Implementation of `EnsureOrigin` that requires approval from two different origins
/// to succeed. It creates a compound origin check where both origin A and origin B
/// must approve for overall check to pass. Used in asynchronous approval flow where
/// multiple origins need to independently approve a proposal over time.
impl<Origin, A, B> frame_support::traits::EnsureOrigin<Origin> for AndGate<A, B>
where
	Origin: Into<Result<frame_system::RawOrigin<Origin::AccountId>, Origin>>
		+ From<frame_system::RawOrigin<Origin::AccountId>>
		+ Clone,
	Origin: frame_support::traits::OriginTrait,
	A: EnsureOrigin<Origin, Success = ()>,
	B: EnsureOrigin<Origin, Success = ()>,
{
	type Success = ();

	fn try_origin(origin: Origin) -> Result<Self::Success, Origin> {
		let origin_clone = origin.clone();
		match A::try_origin(origin) {
			Ok(_) => B::try_origin(origin_clone),
			Err(_) => Err(origin_clone),
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<Origin, ()> {
		// Placeholder implementation for benchmarking to create a successful origin from A
		A::try_successful_origin()
	}
}

/// Custom weight implementation for `set_dummy`.
struct WeightForSetDummy<T>(PhantomData<T>);

impl<T> WeighData<(&DummyValueOf,)> for WeightForSetDummy<T> {
	fn weigh_data(&self, _: (&DummyValueOf,)) -> Weight {
		Weight::from_parts(100_000_000, 0)
	}
}

impl<T> ClassifyDispatch<(&DummyValueOf,)> for WeightForSetDummy<T> {
	fn classify_dispatch(&self, _: (&DummyValueOf,)) -> DispatchClass {
		DispatchClass::Normal
	}
}

impl<T> PaysFee<(&DummyValueOf,)> for WeightForSetDummy<T> {
	fn pays_fee(&self, _: (&DummyValueOf,)) -> Pays {
		Pays::Yes
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::{WeightInfo, *};
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	use sp_runtime::traits::{Dispatchable, Hash, One};
	use sp_std::{fmt::Debug, marker::PhantomData};

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The overarching call type.
		type RuntimeCall: Parameter
			+ Dispatchable<RuntimeOrigin = Self::RuntimeOrigin, PostInfo = PostDispatchInfo>
			+ GetDispatchInfo
			+ From<frame_system::Call<Self>>
			+ IsSubType<Call<Self>>
			+ IsType<<Self as frame_system::Config>::RuntimeCall>;

		/// The hashing implementation.
		type Hashing: sp_runtime::traits::Hash;

		/// Identifier type for different origins that must maintain uniqueness and comparability.
		type OriginId: Parameter + Member + TypeInfo + Copy + Ord + MaxEncodedLen + FullCodec;

		/// The required number of approvals for a single proposal.
		/// The original specification by Dr Gavin Wood requires exactly two approvals to satisfy
		/// the "AND Gate" pattern for two origins.
		#[pallet::constant]
		type RequiredApprovalsCount: Get<u32> + Clone;

		/// How long a proposal is valid for measured in blocks before it expires.
		#[pallet::constant]
		type ProposalExpiry: Get<BlockNumberFor<Self>>;

		/// How long to retain proposal data after it reaches a terminal state of (Executed,
		/// Expired).
		///
		/// Inclusions:
		///   - Retention period starts after the proposal's expiry block, or after the current
		///     block after termination through Execution if no expiry is set, and allows on-chain
		///     queries before final cleanup.
		///
		/// Exclusions:
		///   - Retention does not occur for proposals with Cancelled status that are cleaned up
		///     immediately as they have not completed their normal lifecycle.
		///
		/// Benefits:
		///   - On-chain Queryability: Providing the option to retain proposal data in storage to
		///     provide on-chain queryability allows other pallets, smart contracts, or runtime
		///     logic to query the status and details of recently executed or expired proposals.
		///     Whilst events are emitted and can be found in block explorers, or off-chain indexing
		///     could be used, they are not directly queryable on-chain. Chains with storage
		///     constraints may opt to disable retention to save on storage space.
		///   - UX: User interfaces can easily show recently executed or expired proposals without
		///     needing to scan through event logs, providing better UX for governance participants.
		///   - Dispute Resolution: For dispute about a proposal outcome the data may be readily
		///     available for a period allowing for easier verification and resolution.
		///   - Governance Analytics: Retention allows for easier on-chain analytics about proposal
		///     outcomes, success rates, and participation metrics.
		///   - Potential Recovery Actions: Governance systems may opt to challenge or reverse
		///     decisions within a certain timeframe after execution.
		#[pallet::constant]
		type ProposalRetentionPeriodWhenNotCancelled: Get<BlockNumberFor<Self>>;

		/// Maximum number of proposals to check for expiry per block.
		/// This limits the on_initialize processing to prevent excessive resource usage.
		#[pallet::constant]
		type MaxProposalsToExpirePerBlock: Get<u32>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;

		/// Maximum length of a remark.
		#[pallet::constant]
		type MaxRemarkLength: Get<u32>;

		/// Maximum length of a storage identifier (e.g. IPFS CID).
		#[pallet::constant]
		type MaxStorageIdLength: Get<u32>;

		/// Maximum length of a storage identifier description.
		#[pallet::constant]
		type MaxStorageIdDescriptionLength: Get<u32>;

		/// Maximum number of storage identifiers per proposal.
		#[pallet::constant]
		type MaxStorageIdsPerProposal: Get<u32>;

		/// Maximum number of remarks per proposal.
		#[pallet::constant]
		type MaxRemarksPerProposal: Get<u32>;

		/// Collective origin that can remove storage IDs regardless of ownership.
		///
		/// For OpenGov integration configure it as follows to allow both traditional
		/// collective origins and OpenGov referendum origins to perform collective
		/// operations like removing storage IDs:
		/// ```ignore
		/// type CollectiveOrigin = frame_support::traits::EitherOfDiverse<
		///     pallet_collective::EnsureProportionAtLeast<AccountId, CouncilCollective, 1, 2>,
		///     pallet_referenda::EnsureOrigin<Runtime, pallet_referenda::ReferendumIndex>
		/// >;
		/// ```
		type CollectiveOrigin: EnsureOrigin<Self::RuntimeOrigin>;
	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			// Process proposals that expire in this block
			let expiring = ExpiringProposals::<T>::take(n);

			// Limit to MaxProposalsToExpirePerBlock to prevent excessive resource usage
			let max_to_process = T::MaxProposalsToExpirePerBlock::get();
			let to_process = expiring.len().min(max_to_process as usize);
			let mut processed = 0;

			// Process only up to the configured limit
			for (proposal_hash, proposal_origin_id) in expiring.into_iter().take(to_process) {
				if let Some(mut proposal_info) =
					Proposals::<T>::get(&proposal_hash, &proposal_origin_id)
				{
					// Only update status if the proposal is still pending
					if proposal_info.status == ProposalStatus::Pending {
						proposal_info.status = ProposalStatus::Expired;
						Proposals::<T>::insert(
							proposal_hash,
							proposal_origin_id.clone(),
							proposal_info,
						);

						// Emit an event
						Self::deposit_event(Event::ProposalExpired {
							proposal_hash,
							proposal_origin_id,
							timepoint: Timepoint {
								height: frame_system::Pallet::<T>::block_number(),
								index: frame_system::Pallet::<T>::extrinsic_index()
									.unwrap_or_default(),
							},
						});
					}
				}
				processed += 1;
			}

			// Return weight based on actual number processed
			// TODO: Replace with actual weight calculation from WeightInfo
			Weight::from_parts(processed as u64 * 10_000_000, 0)
		}

		fn on_finalize(_n: BlockNumberFor<T>) {
			unimplemented!()
		}

		fn offchain_worker(_n: BlockNumberFor<T>) {
			unimplemented!()
		}
	}

	impl<T: Config> Pallet<T> {
		/// Helper function to get error index of specific error variant
		pub fn error_index(error: Error<T>) -> u8 {
			match error {
				Error::ProposalAlreadyExists => 0,
				Error::ProposalNotFound => 1,
				Error::CannotApproveOwnProposalUsingDifferentOrigin => 2,
				Error::TooManyApprovals => 3,
				Error::NotAuthorized => 4,
				Error::ProposalAlreadyExecuted => 5,
				Error::ProposalExpired => 6,
				Error::ProposalCancelled => 7,
				Error::AccountOriginAlreadyApproved => 8,
				Error::InsufficientApprovals => 9,
				Error::ProposalNotPending => 10,
				Error::ProposalNotInExpiredOrExecutedState => 11,
				Error::AccountOriginApprovalNotFound => 12,
				Error::ProposalRetentionPeriodNotElapsed => 13,
				Error::ProposalNotEligibleForCleanup => 14,
				Error::ProposalStorageIdNotFound => 15,
				Error::StorageIdAlreadyPresent => 16,
				Error::TooManyStorageIds => 17,
				Error::StorageIdTooLong => 18,
				Error::DescriptionTooLong => 19,
				Error::RemarkTooLong => 20,
				Error::RemarkNotFound => 21,
				Error::TooManyRemarks => 22,
				Error::WithdrawnApprovalNotFound => 23,
			}
		}

		/// Helper function to check if a proposal has sufficient approvals and execute it
		fn check_and_execute_proposal(
			proposal_hash: T::Hash,
			proposal_origin_id: T::OriginId,
			mut proposal_info: ProposalInfo<
				T::Hash,
				BlockNumberFor<T>,
				T::OriginId,
				T::AccountId,
				T::RequiredApprovalsCount,
			>,
			who: T::AccountId,
			is_collective: bool,
		) -> DispatchResultWithPostInfo {
			// Ensure proposal is in Pending state
			ensure!(
				proposal_info.status == ProposalStatus::Pending,
				Error::<T>::ProposalNotPending
			);

			// Check for number of approvals required from RequiredApprovalsCount
			if proposal_info.approvals.len() >= T::RequiredApprovalsCount::get() as usize {
				// Retrieve the actual call from storage
				if let Some(call) = <ProposalCalls<T>>::get(proposal_hash) {
					// Execute the call with root origin
					let result =
						Dispatchable::dispatch(*call, frame_system::RawOrigin::Root.into());

					// Update proposal status
					proposal_info.status = ProposalStatus::Executed;
					proposal_info.executed_at = Some(frame_system::Pallet::<T>::block_number());

					// Remove from ExpiringProposals if has expiry since don't need to
					// track executed proposals for expiry
					if let Some(expiry_at) = proposal_info.expiry_at {
						ExpiringProposals::<T>::mutate(expiry_at, |proposals| {
							proposals.retain(|(hash, id)| {
								*hash != proposal_hash || *id != proposal_origin_id
							});
						});
					}

					<Proposals<T>>::insert(
						proposal_hash,
						proposal_origin_id.clone(),
						proposal_info,
					);

					// Create timepoint for execution
					let execution_timepoint = Self::current_timepoint();

					// Store in ExecutedCalls mapping
					<ExecutedCalls<T>>::insert(execution_timepoint.clone(), proposal_hash);

					Self::deposit_event(Event::ProposalExecuted {
						proposal_hash,
						proposal_origin_id,
						result: result.map(|_| ()).map_err(|e| e.error),
						timepoint: execution_timepoint,
						is_collective,
					});

					return Ok(().into());
				}

				// If we get here then the call was not found in storage
				return Err(Error::<T>::ProposalNotFound.into());
			}

			// Return an error when there aren't enough approvals
			Err(Error::<T>::InsufficientApprovals.into())
		}

		/// Helper function to check if a proposal expired and update its status if necessary
		fn check_proposal_expiry(
			proposal_hash: T::Hash,
			proposal_origin_id: &T::OriginId,
			proposal_info: &mut ProposalInfo<
				T::Hash,
				BlockNumberFor<T>,
				T::OriginId,
				T::AccountId,
				T::RequiredApprovalsCount,
			>,
		) -> bool {
			// Only check expiry for pending proposals
			if proposal_info.status == ProposalStatus::Pending {
				let current_block = frame_system::Pallet::<T>::block_number();

				// Check if proposal has expired
				if let Some(expiry_at) = proposal_info.expiry_at {
					if current_block > expiry_at {
						// Update proposal status to expired
						proposal_info.status = ProposalStatus::Expired;

						// Update storage
						<Proposals<T>>::insert(
							proposal_hash,
							proposal_origin_id.clone(),
							proposal_info.clone(),
						);

						// Emit event
						Self::deposit_event(Event::ProposalExpired {
							proposal_hash,
							proposal_origin_id: proposal_origin_id.clone(),
							timepoint: Timepoint {
								height: current_block,
								index: frame_system::Pallet::<T>::extrinsic_index()
									.unwrap_or_default(),
							},
						});

						return true;
					}
				}
			}

			false
		}

		/// Helper to clean up all storage related to a proposal
		fn remove_proposal_storage(proposal_hash: T::Hash, proposal_origin_id: T::OriginId) {
			<ProposalCalls<T>>::remove(proposal_hash);
			<Approvals<T>>::clear_prefix(
				(proposal_hash, proposal_origin_id.clone()),
				u32::MAX,
				None,
			);
			<Proposals<T>>::remove(proposal_hash, proposal_origin_id);
			<GovernanceHashes<T>>::remove(proposal_hash);
			for ((hash, origin_id, approving_origin_id), _) in <WithdrawnApprovals<T>>::iter() {
				if hash == proposal_hash && origin_id == proposal_origin_id {
					<WithdrawnApprovals<T>>::remove((hash, origin_id, approving_origin_id));
				}
			}
		}

		/// Helper function to check if terminal proposal (Executed, Expired, Cancelled) is eligible
		/// for cleanup based on retention period
		fn is_terminal_proposal_eligible_for_cleanup(
			proposal: &ProposalInfo<
				T::Hash,
				BlockNumberFor<T>,
				T::OriginId,
				T::AccountId,
				T::RequiredApprovalsCount,
			>,
		) -> bool {
			// Get current block number
			let current_block = frame_system::Pallet::<T>::block_number();

			// Check if proposal in terminal state and not pending
			if (proposal.status == ProposalStatus::Executed
				|| proposal.status == ProposalStatus::Expired
				|| proposal.status == ProposalStatus::Cancelled)
				&& proposal.status != ProposalStatus::Pending
			{
				// Cancelled proposals are always eligible for cleanup immediately
				if proposal.status == ProposalStatus::Cancelled {
					return true;
				}

				// Calculate cleanup eligibility block for executed and expired proposals
				let cleanup_eligible_block = match proposal.status {
					// Executed proposals use execution time as base, regardless of expiry
					ProposalStatus::Executed => proposal
						.executed_at
						.unwrap_or(current_block)
						.saturating_add(T::ProposalRetentionPeriodWhenNotCancelled::get()),
					// Expired proposals use expiry as base
					ProposalStatus::Expired => match proposal.expiry_at {
						Some(expiry_at) => expiry_at
							.saturating_add(T::ProposalRetentionPeriodWhenNotCancelled::get()),
						None => current_block
							.saturating_add(T::ProposalRetentionPeriodWhenNotCancelled::get()),
					},
					// This should never be reached due to the earlier check for Cancelled status
					_ => current_block
						.saturating_add(T::ProposalRetentionPeriodWhenNotCancelled::get()),
				};

				// Proposal is eligible for cleanup if we passed cleanup_eligible_block
				current_block >= cleanup_eligible_block
			} else {
				// Non-terminal proposals are not eligible for cleanup
				false
			}
		}

		/// Helper function to publish a remark on-chain
		/// and emit the appropriate event based on the remark type.
		fn publish_remark(
			who: &T::AccountId,
			proposal_hash: T::Hash,
			proposal_origin_id: T::OriginId,
			remark: Vec<u8>,
			remark_type: RemarkType,
			storage_id: Option<BoundedVec<u8, T::MaxStorageIdLength>>,
			storage_id_description: Option<BoundedVec<u8, T::MaxStorageIdDescriptionLength>>,
			timepoint: Timepoint<BlockNumberFor<T>>,
			approving_origin_id: Option<T::OriginId>,
			approving_account_id: Option<T::AccountId>,
		) -> DispatchResultWithPostInfo {
			// Ensure the remark is not too long
			ensure!(remark.len() <= T::MaxRemarkLength::get() as usize, Error::<T>::RemarkTooLong);

			// Create on-chain remark
			let remark_call = frame_system::Call::<T>::remark { remark: remark.clone() };
			let remark_call: <T as Config>::RuntimeCall = remark_call.into();
			let _ = remark_call
				.dispatch(frame_system::RawOrigin::Signed(who.clone()).into())
				.map_err(|e| e.error)?;

			// Emit appropriate event based on remark type
			match remark_type {
				RemarkType::Initial => {
					// If approving_origin_id is None it ss from `propose` extrinsic
					if approving_origin_id.is_none() {
						Self::deposit_event(Event::ProposalCreatedWithRemark {
							proposal_hash,
							proposal_origin_id,
							proposal_account_id: who.clone(),
							timepoint,
							remark: remark.clone(),
						});
					} else {
						// Otherwise it's from `add_approval` extrinsic
						if let Some(approving_id) = approving_origin_id {
							if let Some(account_id) = approving_account_id {
								Self::deposit_event(Event::OriginApprovalAdded {
									proposal_hash,
									proposal_origin_id,
									approving_origin_id: approving_id,
									approving_account_id: account_id,
									timepoint,
								});
							}
						}
					}
				},
				RemarkType::Amend => {
					// If approving_origin_id is provided it's an approver amending remark
					if let Some(approving_id) = approving_origin_id {
						if let Some(account_id) = approving_account_id {
							Self::deposit_event(Event::OriginApprovalAmendedWithRemark {
								proposal_hash,
								proposal_origin_id,
								approving_origin_id: approving_id,
								approving_account_id: account_id,
								timepoint,
								remark: remark.clone(),
							});
						}
					} else {
						// If no approving_origin_id is provided, this is the proposer amending the
						// proposal remark
						Self::deposit_event(Event::ProposerAmendedProposalWithRemark {
							proposal_hash,
							proposal_origin_id: proposal_origin_id.clone(),
							proposal_account_id: who.clone(),
							timepoint,
							remark: remark.clone(),
						});
					}
				},
			}

			// Update the remark in GovernanceHashes
			<GovernanceHashes<T>>::try_mutate(proposal_hash, |maybe_hashes| -> DispatchResult {
				let hashes = maybe_hashes.get_or_insert((
					<T as frame_system::Config>::Hashing::hash_of(&[0u8]), // Default combined hash
					BoundedBTreeMap::new(),                                // Empty remark hashes
					BoundedVec::default(),                                 // Empty storage IDs
				));

				// Create a new map with the updated entry
				let mut new_map = hashes.1.clone();
				let remark_hash = <T as frame_system::Config>::Hashing::hash_of(&remark);
				let bounded_remark = BoundedVec::<u8, T::MaxRemarkLength>::try_from(remark.clone())
					.map_err(|_| Error::<T>::RemarkTooLong)?;
				new_map
					.try_insert(remark_hash, bounded_remark)
					.map_err(|_| Error::<T>::TooManyRemarks)?;

				// Replace the entire tuple field with the new map
				*hashes = (hashes.0.clone(), new_map, hashes.2.clone());

				// Emit RemarkStored event when a remark is stored
				Self::deposit_event(Event::RemarkStored {
					proposal_hash,
					proposal_origin_id,
					account_id: who.clone(),
					remark_hash,
				});

				Ok(())
			})?;

			// Add storage ID if provided
			if let Some(id) = storage_id {
				Self::attach_storage_id_to_proposal(
					who.clone(),
					proposal_hash,
					proposal_origin_id.clone(),
					id.clone(),
					storage_id_description.clone(),
				)?;

				// Emit event for the storage ID addition
				Self::deposit_event(Event::StorageIdAdded {
					proposal_hash,
					proposal_origin_id,
					account_id: who.clone(),
					storage_id: id,
					storage_id_description,
				});
			}

			Ok(().into())
		}

		/// Helper function to get the current timepoint
		fn current_timepoint() -> Timepoint<BlockNumberFor<T>> {
			Timepoint {
				height: frame_system::Pallet::<T>::block_number(),
				index: frame_system::Pallet::<T>::extrinsic_index().unwrap_or_default(),
			}
		}

		/// Helper function to add a proposal to the expiring proposals storage
		fn add_to_expiring_proposals(
			proposal_hash: T::Hash,
			proposal_origin_id: T::OriginId,
			expiry: BlockNumberFor<T>,
		) -> DispatchResultWithPostInfo {
			ExpiringProposals::<T>::mutate(expiry, |proposals| {
				if let Err(_) = proposals.try_push((proposal_hash, proposal_origin_id.clone())) {
					log::warn!("Too many proposals expiring in the same block. Some proposals may not be automatically expired.");
				}
			});
			Ok(().into())
		}

		/// Helper function to check if an account can manage storage identifiers
		/// Only proposers and approvers of a proposal are authorized to manage storage IDs.
		fn ensure_account_origin_authorized_to_manage_proposal_storage_ids(
			who: &T::AccountId,
			proposal_hash: T::Hash,
			proposal_origin_id: T::OriginId,
		) -> DispatchResult {
			// Get proposal info
			let proposal_info = <Proposals<T>>::get(proposal_hash, proposal_origin_id)
				.ok_or(Error::<T>::ProposalNotFound)?;

			// Check if account is proposer
			if &proposal_info.proposer == who {
				return Ok(());
			}

			// Check if account is an approver
			if proposal_info.approvals.iter().any(|(account_id, _)| account_id == who) {
				return Ok(());
			}

			Err(Error::<T>::NotAuthorized.into())
		}

		/// Helper function to check if an account can remove a specific storage identifier
		fn ensure_account_origin_authorized_to_remove_proposal_storage_id(
			who: &T::AccountId,
			proposal_hash: T::Hash,
			proposal_origin_id: T::OriginId,
			storage_id: &BoundedVec<u8, T::MaxStorageIdLength>,
		) -> DispatchResult {
			if let Some((_, _, ids)) = <GovernanceHashes<T>>::get(proposal_hash) {
				if let Some((_, _, added_by, _)) = ids.iter().find(|(id, _, _, _)| id == storage_id)
				{
					// Check if account added the identifier
					if added_by == who {
						return Ok(());
					}
				}
			}

			// Check if they can manage storage IDs in general if it was not the owner
			Self::ensure_account_origin_authorized_to_manage_proposal_storage_ids(
				who,
				proposal_hash,
				proposal_origin_id,
			)
		}

		/// Helper function to add a storage identifier to a proposal
		fn attach_storage_id_to_proposal(
			who: T::AccountId,
			proposal_hash: T::Hash,
			proposal_origin_id: T::OriginId,
			storage_id: BoundedVec<u8, T::MaxStorageIdLength>,
			storage_id_description: Option<BoundedVec<u8, T::MaxStorageIdDescriptionLength>>,
		) -> DispatchResult {
			// Check proposal exists
			ensure!(
				<Proposals<T>>::contains_key(proposal_hash, proposal_origin_id),
				Error::<T>::ProposalNotFound
			);

			// Check account can add storage IDs
			Self::ensure_account_origin_authorized_to_manage_proposal_storage_ids(
				&who,
				proposal_hash,
				proposal_origin_id,
			)?;

			// Add or update the storage ID in GovernanceHashes
			<GovernanceHashes<T>>::try_mutate(proposal_hash, |maybe_hashes| -> DispatchResult {
				let hashes = maybe_hashes.get_or_insert((
					<T as frame_system::Config>::Hashing::hash_of(&[0u8]), // Default combined hash
					BoundedBTreeMap::new(),                                // Empty remark hashes
					BoundedVec::default(),                                 // Empty storage IDs
				));

				// Check if the storage ID already exists
				if hashes.2.iter().any(|(id, _, _, _)| id == &storage_id) {
					return Err(Error::<T>::StorageIdAlreadyPresent.into());
				}

				// Add the new storage ID with metadata
				hashes
					.2
					.try_push((
						storage_id.clone(),
						frame_system::Pallet::<T>::block_number(),
						who.clone(),
						storage_id_description.clone(),
					))
					.map_err(|_| Error::<T>::TooManyStorageIds)?;

				Ok(())
			})?;

			// Emit event for the storage ID addition
			Self::deposit_event(Event::StorageIdAdded {
				proposal_hash,
				proposal_origin_id,
				account_id: who,
				storage_id,
				storage_id_description,
			});

			Ok(())
		}

		/// Helper function to remove a storage identifier from a proposal
		fn detach_storage_id_from_proposal(
			proposal_hash: T::Hash,
			proposal_origin_id: T::OriginId,
			storage_id: BoundedVec<u8, T::MaxStorageIdLength>,
			who: T::AccountId,
		) -> DispatchResult {
			// Check proposal exists
			ensure!(
				<Proposals<T>>::contains_key(proposal_hash, proposal_origin_id),
				Error::<T>::ProposalNotFound
			);

			// Check account can remove this specific storage ID
			Self::ensure_account_origin_authorized_to_remove_proposal_storage_id(
				&who,
				proposal_hash,
				proposal_origin_id,
				&storage_id,
			)?;

			// Remove storage ID
			let mut found = false;
			<GovernanceHashes<T>>::try_mutate(proposal_hash, |maybe_hashes| -> DispatchResult {
				if let Some(hashes) = maybe_hashes {
					let (_, _, ids) = hashes;
					if let Some(idx) = ids.iter().position(|(id, _, _, _)| id == &storage_id) {
						// Remove storage ID
						ids.remove(idx);
						found = true;
					}
				}
				Ok(())
			})?;

			// Ensure storage ID was found and removed
			ensure!(found, Error::<T>::ProposalStorageIdNotFound);

			// Emit event
			Self::deposit_event(Event::StorageIdRemoved {
				proposal_hash,
				proposal_origin_id,
				account_id: who,
				storage_id,
				is_collective: false,
			});

			Ok(())
		}

		/// Helper function to get all storage IDs for a proposal
		pub fn get_proposal_storage_ids(
			proposal_hash: T::Hash,
		) -> Vec<(
			BoundedVec<u8, T::MaxStorageIdLength>,
			BlockNumberFor<T>,
			T::AccountId,
			Option<BoundedVec<u8, T::MaxStorageIdDescriptionLength>>,
		)> {
			if let Some((_, _, ids)) = <GovernanceHashes<T>>::get(proposal_hash) {
				ids.into_iter()
					.map(|(id, block, account, desc)| {
						(id.clone(), block, account.clone(), desc.clone())
					})
					.collect()
			} else {
				Vec::new()
			}
		}

		/// Helper function to check if a proposal has a specific storage ID
		pub fn has_storage_id_for_proposal(
			proposal_hash: T::Hash,
			storage_id: &BoundedVec<u8, T::MaxStorageIdLength>,
		) -> bool {
			if let Some((_, _, ids)) = <GovernanceHashes<T>>::get(proposal_hash) {
				ids.iter().any(|(id, _, _, _)| id == storage_id)
			} else {
				false
			}
		}

		/// Helper function to filter storage IDs by a predicate
		pub fn filter_storage_ids_for_proposal<F>(
			proposal_hash: T::Hash,
			proposal_origin_id: T::OriginId,
			predicate: F,
		) -> Vec<(
			BoundedVec<u8, T::MaxStorageIdLength>,
			BlockNumberFor<T>,
			T::AccountId,
			Option<BoundedVec<u8, T::MaxStorageIdDescriptionLength>>,
		)>
		where
			F: Fn(
				&BoundedVec<u8, T::MaxStorageIdLength>,
				&BlockNumberFor<T>,
				&T::AccountId,
				&Option<BoundedVec<u8, T::MaxStorageIdDescriptionLength>>,
			) -> bool,
		{
			// Verify proposal exists with this combination
			if <Proposals<T>>::contains_key(proposal_hash, proposal_origin_id) {
				if let Some((_, _, ids)) = <GovernanceHashes<T>>::get(proposal_hash) {
					return ids
						.iter()
						.filter(|(id, block, account, desc)| predicate(id, block, account, desc))
						.map(|(id, block, account, desc)| {
							(id.clone(), *block, account.clone(), desc.clone())
						})
						.collect();
				}
			}
			Vec::new()
		}

		/// Helper function to get all IPFS CIDs for a proposal
		/// that uses a heuristic to identify IPFS CIDs by common prefixes
		pub fn get_proposal_ipfs_cids(
			proposal_hash: T::Hash,
			proposal_origin_id: T::OriginId,
		) -> Vec<(
			BoundedVec<u8, T::MaxStorageIdLength>,
			BlockNumberFor<T>,
			T::AccountId,
			Option<BoundedVec<u8, T::MaxStorageIdDescriptionLength>>,
		)> {
			Self::filter_storage_ids_for_proposal(
				proposal_hash,
				proposal_origin_id,
				|id, _, _, _| {
					// Common IPFS CID prefixes (v0 and v1)
					if id.len() > 2 {
						// CIDv0 starts with "Qm"
						if id.starts_with(b"Qm") {
							return true;
						}
						// CIDv1 often starts with "bafy"
						if id.len() > 4 && id.starts_with(b"bafy") {
							return true;
						}
					}
					false
				},
			)
		}

		/// Helper function to convert optional storage ID and description to bounded types
		fn convert_to_bounded_types(
			storage_id: Option<Vec<u8>>,
			storage_id_description: Option<Vec<u8>>,
		) -> Result<
			(
				Option<BoundedVec<u8, T::MaxStorageIdLength>>,
				Option<BoundedVec<u8, T::MaxStorageIdDescriptionLength>>,
			),
			DispatchError,
		> {
			let bounded_storage_id = if let Some(id) = storage_id {
				Some(
					BoundedVec::<u8, T::MaxStorageIdLength>::try_from(id)
						.map_err(|_| Error::<T>::StorageIdTooLong)?,
				)
			} else {
				None
			};

			let bounded_storage_id_description = if let Some(desc) = storage_id_description {
				Some(
					BoundedVec::<u8, T::MaxStorageIdDescriptionLength>::try_from(desc)
						.map_err(|_| Error::<T>::DescriptionTooLong)?,
				)
			} else {
				None
			};

			Ok((bounded_storage_id, bounded_storage_id_description))
		}

		/// Helper function to convert a required storage ID to bounded type
		fn convert_storage_id_to_bounded(
			storage_id: Vec<u8>,
		) -> Result<BoundedVec<u8, T::MaxStorageIdLength>, DispatchError> {
			BoundedVec::<u8, T::MaxStorageIdLength>::try_from(storage_id)
				.map_err(|_| Error::<T>::StorageIdTooLong.into())
		}

		/// Get withdrawn approvals with optional collective origin filtering
		pub fn get_approvals_withdrawn(
		) -> Vec<(T::Hash, T::OriginId, T::OriginId, T::AccountId, BlockNumberFor<T>)> {
			let mut withdrawn = Vec::new();

			for ((hash, proposal_origin_id, approving_origin_id), values) in
				<WithdrawnApprovals<T>>::iter()
			{
				// Iterate through each entry in the Vec
				for (account_id, block_number) in values {
					withdrawn.push((
						hash,
						proposal_origin_id.clone(),
						approving_origin_id.clone(),
						account_id,
						block_number,
					));
				}
			}

			withdrawn
		}

		/// Helper function to handle both collective and signed origins.
		/// Returns:
		/// - Account ID to use (actual account ID for both signed and collective origins)
		/// - Boolean whether origin is collective origin
		pub fn ensure_signed_or_collective(
			origin: OriginFor<T>,
		) -> Result<(T::AccountId, bool), DispatchError> {
			// Try to extract a signed origin
			match ensure_signed(origin.clone()) {
				Ok(who) => Ok((who, false)),
				Err(_) => {
					// Not signed origin so try collective origin
					match T::CollectiveOrigin::try_origin(origin.clone()) {
						Ok(_) => {
							// Tests only
							//
							// Collective origin detected.
							// Check if it is a ROOT first, otherwise use
							// special account ID (4) for (TECH_FELLOWSHIP)
							#[cfg(test)]
							{
								// Check if from root origin first
								if let Ok(_) = frame_system::ensure_root(origin.clone()) {
									// Root origin detected so use ROOT account (0)
									let root_id: u64 = 0;
									let account_id = T::AccountId::decode(
										&mut codec::Encode::encode(&root_id).as_slice(),
									)
									.unwrap_or_else(|_| {
										// Fallback to zero address if conversion fails
										T::AccountId::decode(
											&mut sp_runtime::traits::TrailingZeroInput::zeroes(),
										)
										.unwrap_or_else(|_| {
											panic!("Infinite length input; no invalid inputs for type; qed")
										})
									});

									Ok((account_id, true))
								} else if let Ok(_) = frame_system::ensure_none(origin.clone()) {
									// None origin is being treated as a special case for
									// TECH_FELLOWSHIP collective origin
									let tech_fellowship_id: u64 = 4;
									let account_id = T::AccountId::decode(
										&mut codec::Encode::encode(&tech_fellowship_id).as_slice(),
									)
									.unwrap_or_else(|_| {
										T::AccountId::decode(
											&mut sp_runtime::traits::TrailingZeroInput::zeroes(),
										)
										.unwrap_or_else(|_| {
											panic!("Infinite length input; no invalid inputs for type; qed")
										})
									});
									return Ok((account_id, true));
								} else {
									// Not a root origin so return an error for unexpected
									// collective origin
									return Err(DispatchError::BadOrigin);
								}
							}

							// Production only
							//
							// Collective origin detected so use zero address (ROOT)
							#[cfg(not(test))]
							{
								let account_id = T::AccountId::decode(
									&mut sp_runtime::traits::TrailingZeroInput::zeroes(),
								)
								.unwrap_or_else(|_| {
									panic!("Infinite length input; no invalid inputs for type; qed")
								});

								Ok((account_id, true))
							}
						},
						Err(_) => Err(DispatchError::BadOrigin),
					}
				},
			}
		}
	}

	#[pallet::call(weight(<T as Config>::WeightInfo))]
	impl<T: Config> Pallet<T> {
		/// Submit a proposal for approval.
		///
		/// Optionally recording the first origin's approval.
		/// Optionally includes a remark that will be published on-chain
		/// and associated with this proposal.
		/// Optionally including a storage ID (e.g. IPFS CID) and description
		/// Optionally flagging to auto-execute the proposal when required approvals are reached
		///
		/// The dispatch origin for this call must be _Signed_.
		///
		/// Parameters:
		/// - `call`: The call to be executed.
		/// - `proposal_origin_id`: The origin ID to associate with the proposal.
		/// - `expiry_at`: Optional block number when the proposal expires.
		/// - `include_proposer_approval`: Optional flag to include this proposer as an approver.
		/// - `remark`: Optional remark to include with the proposal.
		/// - `storage_id`: Optional storage ID (e.g. IPFS CID) to associate with the proposal.
		/// - `storage_id_description`: Optional storage ID description of the storage ID.
		/// - `auto_execute`: Optional flag to execute the proposal automatically when required
		///   approvals of `RequiredApprovalsCount` are reached.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::propose())]
		pub fn propose(
			origin: OriginFor<T>,
			call: Box<<T as Config>::RuntimeCall>,
			proposal_origin_id: T::OriginId,
			expiry_at: Option<BlockNumberFor<T>>,
			include_proposer_approval: Option<bool>,
			remark: Option<Vec<u8>>,
			storage_id: Option<Vec<u8>>,
			storage_id_description: Option<Vec<u8>>,
			auto_execute: Option<bool>,
		) -> DispatchResultWithPostInfo {
			let (who, _) = Self::ensure_signed_or_collective(origin.clone())?;
			let current_block = <frame_system::Pallet<T>>::block_number();
			let submission_timepoint = Self::current_timepoint();

			let (bounded_storage_id, bounded_storage_id_description) =
				Self::convert_to_bounded_types(storage_id, storage_id_description)?;

			// Compute hash of call for storage using system hashing implementation
			let proposal_hash = <T as frame_system::Config>::Hashing::hash_of(&call);

			// Check if given proposal already exists
			let duplicate_detected =
				<Proposals<T>>::contains_key(proposal_hash, proposal_origin_id.clone());

			// Generate unique hash if this is a duplicate proposal
			let unique_proposal_hash = if duplicate_detected {
				// Create unique hash by combining original hash with current block and extrinsic
				// index
				let unique_input =
					(proposal_hash, submission_timepoint.height, submission_timepoint.index);
				<T as frame_system::Config>::Hashing::hash_of(&unique_input)
			} else {
				proposal_hash
			};

			// Determine expiration block number if provided or otherwise use default
			let expiry_block = match expiry_at {
				Some(expiry_block) => {
					// Check expiry not in the past
					ensure!(current_block <= expiry_block, Error::<T>::ProposalExpired);
					Some(expiry_block)
				},
				None => {
					// If no expiry was provided then use proposal expiry config
					Some(current_block.saturating_add(T::ProposalExpiry::get()))
				},
			};

			// If this was a duplicate, emit a warning event
			if duplicate_detected {
				Self::deposit_event(Event::DuplicateProposalWarning {
					proposal_hash: unique_proposal_hash,
					proposal_origin_id,
					existing_proposal_hash: proposal_hash,
					proposer: who.clone(),
					timepoint: submission_timepoint,
				});
			}

			// Store actual call data (unbounded) to save storage for later execution
			<ProposalCalls<T>>::insert(unique_proposal_hash, call);

			// Create an empty bounded vec for approvals
			let mut approvals =
				BoundedVec::<(T::AccountId, T::OriginId), T::RequiredApprovalsCount>::default();

			// Add proposer's approval if requested (default to false if not specified)
			if include_proposer_approval.unwrap_or(false) {
				approvals
					.try_push((who.clone(), proposal_origin_id.clone()))
					.map_err(|_| Error::<T>::TooManyApprovals)?;
			};

			// Create and store proposal metadata (bounded storage)
			let proposal_info = ProposalInfo {
				proposal_hash: unique_proposal_hash,
				proposal_origin_id,
				expiry_at: expiry_block,
				approvals,
				status: ProposalStatus::Pending,
				proposer: who.clone(),
				submitted_at: current_block,
				executed_at: None,
				auto_execute,
			};

			// Store proposal metadata (bounded storage)
			<Proposals<T>>::insert(unique_proposal_hash, proposal_origin_id.clone(), proposal_info);

			// Mark first approval in approvals storage efficiently only if proposer approval is
			// included
			if include_proposer_approval.unwrap_or(false) {
				<Approvals<T>>::insert(
					(unique_proposal_hash, proposal_origin_id.clone()),
					proposal_origin_id.clone(),
					who.clone(),
				);
			};

			// Add proposal to expiring proposals for tracking and automatic expiry if expiry is set
			if let Some(expiry) = expiry_block {
				Self::add_to_expiring_proposals(
					unique_proposal_hash,
					proposal_origin_id.clone(),
					expiry,
				)?;
			}

			// If a remark is provided then publish it on-chain
			if let Some(remark_content) = remark {
				Self::publish_remark(
					&who,
					unique_proposal_hash,
					proposal_origin_id.clone(),
					remark_content,
					RemarkType::Initial,
					bounded_storage_id,
					bounded_storage_id_description,
					submission_timepoint,
					None,
					None,
				);
			}

			// Emit event
			Self::deposit_event(Event::ProposalCreated {
				proposal_hash: unique_proposal_hash,
				proposal_origin_id,
				proposal_account_id: who.clone(),
				timepoint: submission_timepoint,
			});

			Ok(().into())
		}

		/// Approve a previously submitted proposal.
		///
		/// Optionally includes a remark that will be published on-chain
		/// and associated with this approval.
		///
		/// Parameters:
		/// - `proposal_hash`: Proposal hash to approve.
		/// - `proposal_origin_id`: Origin ID associated with the proposal.
		/// - `approving_origin_id`: Origin ID to approve with.
		/// - `remark`: Optional remark to include with the approval.
		/// - `storage_id`: Optional storage ID (e.g. IPFS CID) to associate with the approval.
		/// - `storage_id_description`: Optional storage ID description of the storage ID.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::add_approval())]
		pub fn add_approval(
			origin: OriginFor<T>,
			proposal_hash: T::Hash,
			proposal_origin_id: T::OriginId,
			approving_origin_id: T::OriginId,
			remark: Option<Vec<u8>>,
			storage_id: Option<Vec<u8>>,
			storage_id_description: Option<Vec<u8>>,
		) -> DispatchResultWithPostInfo {
			let (who, is_collective) = Self::ensure_signed_or_collective(origin.clone())?;

			let approval_timepoint = Self::current_timepoint();

			let (bounded_storage_id, bounded_storage_id_description) =
				Self::convert_to_bounded_types(storage_id, storage_id_description)?;

			// Try to fetch proposal from storage first
			let mut proposal_info = <Proposals<T>>::get(&proposal_hash, &proposal_origin_id)
				.ok_or(Error::<T>::ProposalNotFound)?;

			// Check if caller is same as proposer of proposal but using a different origin ID
			if who == proposal_info.proposer && approving_origin_id != proposal_origin_id {
				return Err(Error::<T>::CannotApproveOwnProposalUsingDifferentOrigin.into());
			}

			// Check if proposal still pending
			if proposal_info.status != ProposalStatus::Pending {
				return match proposal_info.status {
					ProposalStatus::Executed => Err(Error::<T>::ProposalAlreadyExecuted.into()),
					ProposalStatus::Expired => Err(Error::<T>::ProposalExpired.into()),
					_ => Err(Error::<T>::ProposalNotFound.into()),
				};
			}

			// Check if proposal has expired
			if Self::check_proposal_expiry(proposal_hash, &proposal_origin_id, &mut proposal_info) {
				return Err(Error::<T>::ProposalExpired.into());
			}

			// Ensure approver is not the original proposer with a different origin ID
			// This prevents the same user from approving their own proposal with a different origin
			if proposal_info.proposer == who && proposal_origin_id != approving_origin_id {
				return Err(Error::<T>::CannotApproveOwnProposalUsingDifferentOrigin.into());
			}

			// Ensure origin has not already approved
			ensure!(
				!<Approvals<T>>::contains_key(
					(proposal_hash, proposal_origin_id.clone()),
					approving_origin_id.clone()
				),
				Error::<T>::AccountOriginAlreadyApproved
			);

			// Add to storage to mark this origin as approved
			proposal_info
				.approvals
				.try_push((who.clone(), approving_origin_id.clone()))
				.map_err(|_| Error::<T>::TooManyApprovals)?;

			// Update proposal in storage with new origin approval
			<Proposals<T>>::insert(
				proposal_hash,
				proposal_origin_id.clone(),
				proposal_info.clone(),
			);

			// Store the approval
			<Approvals<T>>::insert(
				(proposal_hash, proposal_origin_id.clone()),
				approving_origin_id.clone(),
				who.clone(),
			);

			// If remark is provided then publish it on-chain
			if let Some(remark_content) = remark {
				Self::publish_remark(
					&who,
					proposal_hash,
					proposal_origin_id.clone(),
					remark_content,
					RemarkType::Amend,
					bounded_storage_id,
					bounded_storage_id_description,
					approval_timepoint,
					Some(approving_origin_id.clone()),
					Some(who.clone()),
				);
			}

			// Emit standard approval added event
			Self::deposit_event(Event::OriginApprovalAdded {
				proposal_hash,
				proposal_origin_id: proposal_origin_id.clone(),
				approving_origin_id: approving_origin_id.clone(),
				approving_account_id: who.clone(),
				timepoint: approval_timepoint,
			});

			// Check if proposal can be executed now and auto-execute if requested
			if proposal_info.auto_execute.unwrap_or(false) {
				// Pass a clone of proposal info so original not modified if execution attempt fails
				match Self::check_and_execute_proposal(
					proposal_hash,
					proposal_origin_id,
					proposal_info.clone(),
					who.clone(),
					is_collective,
				) {
					// Success case results in proposal being executed
					Ok(_) => {
						return Ok(().into());
					},
					// Check if error is specifically the `InsufficientApprovals` error since we
					// need to silently ignore it when adding early approvals
					Err(e) => match e.error {
						DispatchError::Module(module_error) => {
							// Relates to `error_index_consistency` module tests
							if module_error.index == <Self as PalletInfoAccess>::index() as u8 {
								let insufficient_approvals_index =
									Self::error_index(Error::<T>::InsufficientApprovals);
								let proposal_not_found_index =
									Self::error_index(Error::<T>::ProposalNotFound);

								// Special handling for test case is to always propagate
								// ProposalNotFound
								if module_error.error[0] == proposal_not_found_index {
									return Err(Error::<T>::ProposalNotFound.into());
								}

								// Propagate all errors except `InsufficientApprovals` error
								if module_error.error[0] != insufficient_approvals_index {
									return Err(DispatchError::Module(module_error).into());
								} else {
									// Otherwise silently ignore InsufficientApprovals error
									return Ok(().into());
								}
							} else {
								// Error from another pallet must always be propagated
								return Err(DispatchError::Module(module_error).into());
							}
						},
						// Non-module errors must always be propagated
						_ => {
							return Err(e.error.into());
						},
					},
				}
			} else {
				Ok(().into())
			}
		}

		/// Amend an existing proposal or approval with an additional remark and optionally add a
		// storage identifier.
		///
		/// Allows either the proposer or an approver to add additional remarks to their
		/// existing proposal or approval that are published on-chain and associated with the
		/// proposal. Optionally add a storage identifier (e.g. IPFS CID) and description in the
		/// same transaction that is useful for linking updated remarks to new storage content.
		///
		/// Dispatch origin for this call must be signed by the proposer or an approver of the
		/// proposal. Primarily intended for proposers and approvers who wish to amend their remarks
		/// before the proposal is executed or expires. Last approver who triggers execution may
		/// not have time to amend their remarks.
		///
		/// Parameters:
		/// - `proposal_hash`: Hash of the call to amend.
		/// - `proposal_origin_id`: The origin ID of the proposal.
		/// - `approving_origin_id`: Optional origin ID used when amending as an approver. If None,
		///   the caller must be the proposer. If Some, the caller must have previously approved the
		///   proposal with this origin ID.
		/// - `remark`: New remark to add.
		/// - `storage_id`: Optional storage identifier to add.
		/// - `storage_id_description`: Optional storage ID description of the storage ID.
		#[pallet::call_index(2)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::amend_remark())]
		pub fn amend_remark(
			origin: OriginFor<T>,
			proposal_hash: T::Hash,
			proposal_origin_id: T::OriginId,
			approving_origin_id: Option<T::OriginId>,
			remark: Vec<u8>,
			storage_id: Option<Vec<u8>>,
			storage_id_description: Option<Vec<u8>>,
		) -> DispatchResultWithPostInfo {
			let (who, _) = Self::ensure_signed_or_collective(origin.clone())?;

			let update_timepoint = Self::current_timepoint();

			let (bounded_storage_id, bounded_storage_id_description) =
				Self::convert_to_bounded_types(storage_id, storage_id_description)?;

			// Ensure proposal exists
			let proposal_info = <Proposals<T>>::get(proposal_hash, proposal_origin_id.clone())
				.ok_or(Error::<T>::ProposalNotFound)?;

			// Ensure proposal is still pending
			ensure!(
				proposal_info.status == ProposalStatus::Pending,
				Error::<T>::ProposalNotPending
			);

			if let Some(approving_origin_id) = approving_origin_id {
				// Ensure the caller has previously approved this proposal with the specified origin
				ensure!(
					<Approvals<T>>::contains_key(
						(proposal_hash, proposal_origin_id.clone()),
						approving_origin_id.clone()
					),
					Error::<T>::AccountOriginApprovalNotFound
				);

				// Ensure the caller is the one who previously approved
				let approving_account_id = <Approvals<T>>::get(
					(proposal_hash, proposal_origin_id.clone()),
					approving_origin_id.clone(),
				)
				.ok_or(Error::<T>::AccountOriginApprovalNotFound)?;

				ensure!(approving_account_id == who, Error::<T>::NotAuthorized);
			} else {
				// Ensure the caller is the proposer
				ensure!(who == proposal_info.proposer, Error::<T>::NotAuthorized);
			}

			// Publish the remark on-chain
			Self::publish_remark(
				&who.clone(),
				proposal_hash,
				proposal_origin_id.clone(),
				remark,
				RemarkType::Amend,
				bounded_storage_id,
				bounded_storage_id_description,
				update_timepoint,
				approving_origin_id,
				Some(who),
			);

			Ok(().into())
		}

		/// Execute a proposal that has met the required approvals
		#[pallet::call_index(3)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::execute_proposal())]
		pub fn execute_proposal(
			origin: OriginFor<T>,
			proposal_hash: T::Hash,
			proposal_origin_id: T::OriginId,
		) -> DispatchResultWithPostInfo {
			let (who, is_collective) = Self::ensure_signed_or_collective(origin.clone())?;

			// Get proposal info
			let mut proposal = <Proposals<T>>::get(&proposal_hash, &proposal_origin_id)
				.ok_or(Error::<T>::ProposalNotFound)?;

			// Check if proposal has expired using lazy expiry checking
			if Self::check_proposal_expiry(proposal_hash, &proposal_origin_id, &mut proposal) {
				return Err(Error::<T>::ProposalExpired.into());
			}

			// Execute the proposal
			Self::check_and_execute_proposal(
				proposal_hash,
				proposal_origin_id,
				proposal,
				who,
				is_collective,
			)?;

			Ok(().into())
		}

		/// Cancel pending proposal is only callable by original proposer
		#[pallet::call_index(4)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::cancel_proposal())]
		pub fn cancel_proposal(
			origin: OriginFor<T>,
			proposal_hash: T::Hash,
			proposal_origin_id: T::OriginId,
		) -> DispatchResultWithPostInfo {
			let (who, _) = Self::ensure_signed_or_collective(origin.clone())?;

			// Get proposal info
			let mut proposal_info = Proposals::<T>::get(&proposal_hash, &proposal_origin_id)
				.ok_or(Error::<T>::ProposalNotFound)?;

			// Check if proposal has expired
			if Self::check_proposal_expiry(proposal_hash, &proposal_origin_id, &mut proposal_info) {
				return Err(Error::<T>::ProposalExpired.into());
			}

			// Ensure proposal is in a pending state
			ensure!(
				proposal_info.status == ProposalStatus::Pending,
				Error::<T>::ProposalNotPending
			);

			// Ensure caller is original proposer
			ensure!(who == proposal_info.proposer, Error::<T>::NotAuthorized);

			// Update proposal status to Cancelled
			proposal_info.status = ProposalStatus::Cancelled;

			// Store the expiry before moving proposal_info
			let expiry_at = proposal_info.expiry_at;

			// Update storage with cancelled status
			<Proposals<T>>::insert(&proposal_hash, &proposal_origin_id, proposal_info);

			// Clean up all storage related to the proposal
			Self::remove_proposal_storage(proposal_hash, proposal_origin_id.clone());

			// Remove from ExpiringProposals if it has an expiry
			// since we don't need to track cancelled proposals for expiry
			if let Some(expiry_at) = expiry_at {
				ExpiringProposals::<T>::mutate(expiry_at, |proposals| {
					proposals
						.retain(|(hash, id)| *hash != proposal_hash || *id != proposal_origin_id);
				});
			}

			// Create timepoint for cancellation
			let cancellation_timepoint = Self::current_timepoint();

			// Emit event
			Self::deposit_event(Event::ProposalCancelled {
				proposal_hash,
				proposal_origin_id,
				timepoint: cancellation_timepoint,
			});

			Ok(().into())
		}

		/// Withdraw an approval for the proposal associated with an origin.
		///
		/// Only callable by an account that has previously approved the proposal.
		///
		/// Parameters:
		/// - `proposal_hash`: The proposal hash to withdraw approval for.
		/// - `proposal_origin_id`: The origin id that the proposal belongs to.
		/// - `withdrawing_origin_id`: The origin id to withdraw the approval for since the account
		///   might need to specify which of their multiple origin authorities they approved with
		///   that they are now withdrawing approval for.
		#[pallet::call_index(5)]
		#[pallet::weight((<T as pallet::Config>::WeightInfo::withdraw_approval(), DispatchClass::Normal))]
		pub fn withdraw_approval(
			origin: OriginFor<T>,
			proposal_hash: T::Hash,
			proposal_origin_id: T::OriginId,
			withdrawing_origin_id: T::OriginId,
		) -> DispatchResultWithPostInfo {
			let (who, _) = Self::ensure_signed_or_collective(origin.clone())?;

			// Get proposal info
			let mut proposal = <Proposals<T>>::get(&proposal_hash, &proposal_origin_id)
				.ok_or(Error::<T>::ProposalNotFound)?;

			// Check if proposal was already executed
			if proposal.status == ProposalStatus::Executed {
				return Err(Error::<T>::ProposalAlreadyExecuted.into());
			}

			// Check if proposal still pending
			ensure!(proposal.status == ProposalStatus::Pending, Error::<T>::ProposalNotPending);

			// Verify approval exists and check authorisation such that only original approver can
			// withdraw their approval using our mapping from OriginId to AccountId where only
			// the account that originally granted approval can withdraw it
			let approving_account_id = <Approvals<T>>::get(
				(proposal_hash, proposal_origin_id.clone()),
				withdrawing_origin_id.clone(),
			)
			.ok_or(Error::<T>::AccountOriginApprovalNotFound)?;

			ensure!(approving_account_id == who, Error::<T>::NotAuthorized);

			// Find position of withdrawing_origin_id in approvals vector
			let pos = proposal
				.approvals
				.iter()
				.position(|a| a == &(who.clone(), withdrawing_origin_id.clone()))
				.ok_or(Error::<T>::AccountOriginApprovalNotFound)?;

			// Remove approval at found position
			proposal.approvals.swap_remove(pos);

			// Update proposal in storage
			<Proposals<T>>::insert(&proposal_hash, &proposal_origin_id, &proposal);

			// Remove approval from Approvals storage
			<Approvals<T>>::remove((proposal_hash, proposal_origin_id), withdrawing_origin_id);

			// Emit event
			let timepoint = Self::current_timepoint();

			// Record the withdrawn approval
			let current_block = <frame_system::Pallet<T>>::block_number();
			<WithdrawnApprovals<T>>::insert(
				(proposal_hash, proposal_origin_id.clone(), withdrawing_origin_id.clone()),
				vec![(who.clone(), current_block)],
			);

			Self::deposit_event(Event::OriginApprovalWithdrawn {
				proposal_hash,
				proposal_origin_id,
				withdrawing_origin_id,
				withdrawing_account_id: who,
				timepoint,
			});

			Ok(().into())
		}

		/// A privileged call; in this case it resets our dummy value to something new.
		/// Implementation of a privileged call. The `origin` parameter is ROOT because
		/// it's not (directly) from an extrinsic, but rather the system as a whole has decided
		/// to execute it. Different runtimes have different reasons for allow privileged
		/// calls to be executed - we don't need to care why. Because it's privileged, we can
		/// assume it's a one-off operation and substantial processing/storage/memory can be used
		/// without worrying about gameability or attack scenarios.
		///
		/// The weight for this extrinsic we use our own weight object `WeightForSetDummy`
		/// or set_dummy() extrinsic to determine its weight
		#[pallet::call_index(6)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::set_dummy())]
		pub fn set_dummy(
			origin: OriginFor<T>,
			new_value: DummyValueOf,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			info!("New value is now: {:?}", new_value);

			// Put the new value into storage.
			<Dummy<T>>::put(new_value.clone());

			Self::deposit_event(Event::SetDummy { dummy_value: new_value });

			// All good, no refund.
			Ok(().into())
		}

		/// Clean up storage for a proposal that is no longer pending
		#[pallet::call_index(7)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::clean())]
		pub fn clean(
			origin: OriginFor<T>,
			proposal_hash: T::Hash,
			proposal_origin_id: T::OriginId,
		) -> DispatchResultWithPostInfo {
			Self::ensure_signed_or_collective(origin.clone())?;

			// Get proposal info
			let proposal = <Proposals<T>>::get(&proposal_hash, &proposal_origin_id)
				.ok_or(Error::<T>::ProposalNotFound)?;

			// Ensure proposal is in terminal state (Expired, Executed, or Cancelled)
			// and not in the Pending state
			ensure!(
				(proposal.status == ProposalStatus::Expired
					|| proposal.status == ProposalStatus::Executed
					|| proposal.status == ProposalStatus::Cancelled)
					&& proposal.status != ProposalStatus::Pending,
				Error::<T>::ProposalNotInExpiredOrExecutedState
			);

			// Check if proposal has passed retention period
			ensure!(
				Self::is_terminal_proposal_eligible_for_cleanup(&proposal),
				Error::<T>::ProposalRetentionPeriodNotElapsed
			);

			// Clean up all storage
			Self::remove_proposal_storage(proposal_hash, proposal_origin_id.clone());

			// Emit cleanup event
			let cleanup_timepoint = Self::current_timepoint();

			Self::deposit_event(Event::ProposalCleaned {
				proposal_hash,
				proposal_origin_id,
				timepoint: cleanup_timepoint,
			});

			Ok(().into())
		}

		/// Add a storage identifier to a proposal.
		///
		/// Supports IPFS CIDs and other storage identifiers.
		/// Dispatch origin for this call must be signed by proposer or an approver of the proposal.
		/// No other accounts are authorized to add storage IDs.
		///
		/// Parameters:
		/// - `proposal_hash`: The hash of the proposal to add the storage ID to.
		/// - `proposal_origin_id`: The origin ID of the proposal.
		/// - `storage_id`: The storage identifier to add.
		/// - `storage_id_description`: Optional storage ID description of the storage ID.
		#[pallet::call_index(8)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::add_storage_id())]
		pub fn add_storage_id(
			origin: OriginFor<T>,
			proposal_hash: T::Hash,
			proposal_origin_id: T::OriginId,
			storage_id: Vec<u8>,
			storage_id_description: Option<Vec<u8>>,
		) -> DispatchResultWithPostInfo {
			let (who, _) = Self::ensure_signed_or_collective(origin.clone())?;

			let (bounded_storage_id, bounded_storage_id_description) =
				Self::convert_to_bounded_types(Some(storage_id), storage_id_description)?;

			Self::attach_storage_id_to_proposal(
				who,
				proposal_hash,
				proposal_origin_id,
				bounded_storage_id.unwrap(),
				bounded_storage_id_description,
			)?;

			// Event is emitted in attach_storage_id_to_proposal

			Ok(().into())
		}

		/// Remove a storage identifier from a proposal.
		///
		/// Dispatch origin for this call can be either:
		/// - Signed origin by either proposer or an approver of the proposal that added the storage
		///   ID
		/// - Collective origin configured in the pallet allows for governance-based removal of
		///   storage IDs for overriding individual ownership
		///
		/// Parameters:
		/// - `proposal_hash`: The hash of the proposal to remove the storage ID from.
		/// - `storage_id`: The storage identifier to remove.
		#[pallet::call_index(9)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::remove_storage_id())]
		pub fn remove_storage_id(
			origin: OriginFor<T>,
			proposal_hash: T::Hash,
			proposal_origin_id: T::OriginId,
			storage_id: BoundedVec<u8, T::MaxStorageIdLength>,
		) -> DispatchResultWithPostInfo {
			let (who, is_collective) = Self::ensure_signed_or_collective(origin.clone())?;

			if is_collective {
				// Test mode only
				//
				// Ensure the origin is the root or a collective origin
				// Return BadOrigin if origin does not match proposal_origin_id
				#[cfg(test)]
				{
					// Check if the proposal_origin_id is valid for a collective origin
					// Since we cannot directly compare T::OriginId with CompositeOriginId constants
					// we instead extract and check the collective_id values.

					// Get the collective_id from proposal_origin_id
					// where T::OriginId in tests is CompositeOriginId or convertable to u64
					let origin_id_bytes = codec::Encode::encode(&proposal_origin_id);
					let origin_id_u64 = u64::decode(&mut &origin_id_bytes[..])
						.map_err(|_| sp_runtime::traits::BadOrigin)?;

					// Check if it is ROOT (collective_id 0) or TECH_FELLOWSHIP (collective_id 4)
					// CompositeOriginId is encoded with collective_id as the first 32 bits
					let collective_id = (origin_id_u64 >> 32) as u32;

					ensure!(
						collective_id == 0 || collective_id == 4,
						sp_runtime::traits::BadOrigin
					);
				}

				T::CollectiveOrigin::ensure_origin(origin)?;
				// Collective origins do not need check if proposal exists with proposal_origin_id
				// instead we just check if storage ID exists and remove it
				let mut found = false;
				<GovernanceHashes<T>>::try_mutate(
					proposal_hash,
					|maybe_hashes| -> DispatchResult {
						if let Some(hashes) = maybe_hashes {
							let (_, _, ids) = hashes;
							if let Some(idx) =
								ids.iter().position(|(id, _, _, _)| id == &storage_id)
							{
								// Remove the storage ID
								ids.remove(idx);
								found = true;
							}
						}
						Ok(())
					},
				)?;

				// Ensure the storage ID was found and removed
				ensure!(found, Error::<T>::ProposalStorageIdNotFound);

				// Emit event for remove by authorized collective
				Self::deposit_event(Event::StorageIdRemoved {
					proposal_hash,
					proposal_origin_id,
					account_id: who,
					storage_id,
					is_collective: true,
				});
			} else if let Ok(who) = ensure_signed(origin) {
				#[cfg(test)]
				{
					// Test mode only
					//
					// Relates to test
					// 'remove_storage_id_with_collective_fails_for_unauthorized_origin'
					// Check if signed origin is trying to use a collective origin ID
					let origin_id_bytes = codec::Encode::encode(&proposal_origin_id);

					// Check the collective_id (first 4 bytes) for CompositeOriginId
					if origin_id_bytes.len() >= 4 {
						let collective_id = u32::from_le_bytes([
							origin_id_bytes[0],
							origin_id_bytes[1],
							origin_id_bytes[2],
							origin_id_bytes[3],
						]);

						// Signed origins cannot use collective origin IDs
						// so if it is ROOT (0) or TECH_FELLOWSHIP (4) then return BadOrigin
						if (collective_id == 0 || collective_id == 4)
							&& <Proposals<T>>::contains_key(proposal_hash, &proposal_origin_id)
						{
							return Err(sp_runtime::traits::BadOrigin.into());
						}
					}
				}

				// Signed origins
				Self::detach_storage_id_from_proposal(
					proposal_hash,
					proposal_origin_id,
					storage_id,
					who,
				)?;
			} else {
				// Neither a valid collective origin nor a signed origin
				return Err(sp_runtime::traits::BadOrigin.into());
			}

			Ok(().into())
		}

		/// A dummy function for use in tests and benchmarks
		#[pallet::call_index(10)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::set_dummy())]
		pub fn dummy_benchmark(
			origin: OriginFor<T>,
			remark: Vec<u8>,
		) -> DispatchResultWithPostInfo {
			Self::ensure_signed_or_collective(origin.clone())?;
			Ok(().into())
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A proposal has been created.
		ProposalCreated {
			proposal_hash: T::Hash,
			proposal_origin_id: T::OriginId,
			proposal_account_id: T::AccountId,
			timepoint: Timepoint<BlockNumberFor<T>>,
		},
		/// An origin has added their approval of a proposal.
		OriginApprovalAdded {
			proposal_hash: T::Hash,
			proposal_origin_id: T::OriginId,
			approving_origin_id: T::OriginId,
			approving_account_id: T::AccountId,
			timepoint: Timepoint<BlockNumberFor<T>>,
		},
		/// A proposal has been created with a remark.
		ProposalCreatedWithRemark {
			proposal_hash: T::Hash,
			proposal_origin_id: T::OriginId,
			proposal_account_id: T::AccountId,
			timepoint: Timepoint<BlockNumberFor<T>>,
			remark: Vec<u8>,
		},
		/// A proposer amended their proposal with an additional remark.
		ProposerAmendedProposalWithRemark {
			proposal_hash: T::Hash,
			proposal_origin_id: T::OriginId,
			proposal_account_id: T::AccountId,
			timepoint: Timepoint<BlockNumberFor<T>>,
			remark: Vec<u8>,
		},
		/// An origin has amended their approval with an additional remark.
		OriginApprovalAmendedWithRemark {
			proposal_hash: T::Hash,
			proposal_origin_id: T::OriginId,
			approving_origin_id: T::OriginId,
			approving_account_id: T::AccountId,
			timepoint: Timepoint<BlockNumberFor<T>>,
			remark: Vec<u8>,
		},
		/// A proposal has been executed.
		ProposalExecuted {
			proposal_hash: T::Hash,
			proposal_origin_id: T::OriginId,
			result: Result<(), DispatchError>,
			timepoint: Timepoint<BlockNumberFor<T>>,
			is_collective: bool,
		},
		/// A proposal has expired.
		ProposalExpired {
			proposal_hash: T::Hash,
			proposal_origin_id: T::OriginId,
			timepoint: Timepoint<BlockNumberFor<T>>,
		},
		/// A proposal has been cancelled.
		ProposalCancelled {
			proposal_hash: T::Hash,
			proposal_origin_id: T::OriginId,
			timepoint: Timepoint<BlockNumberFor<T>>,
		},
		/// An origin has withdrawn their approval of a proposal.
		OriginApprovalWithdrawn {
			proposal_hash: T::Hash,
			proposal_origin_id: T::OriginId,
			withdrawing_origin_id: T::OriginId,
			withdrawing_account_id: T::AccountId,
			timepoint: Timepoint<BlockNumberFor<T>>,
		},
		SetDummy {
			dummy_value: DummyValueOf,
		},
		/// A proposal's storage was cleaned up
		ProposalCleaned {
			proposal_hash: T::Hash,
			proposal_origin_id: T::OriginId,
			timepoint: Timepoint<BlockNumberFor<T>>,
		},
		/// A duplicate proposal was detected but still created with a unique identifier.
		/// Warns user that a similar proposal already exists.
		DuplicateProposalWarning {
			proposal_hash: T::Hash,
			proposal_origin_id: T::OriginId,
			existing_proposal_hash: T::Hash,
			proposer: T::AccountId,
			timepoint: Timepoint<BlockNumberFor<T>>,
		},
		/// A remark has been stored.
		RemarkStored {
			/// Hash of the proposal.
			proposal_hash: T::Hash,
			/// Origin ID of the proposal.
			proposal_origin_id: T::OriginId,
			/// Account ID of the sender.
			account_id: T::AccountId,
			/// Hash of the remark.
			remark_hash: T::Hash,
		},
		/// A storage ID has been added to a proposal.
		StorageIdAdded {
			/// Hash of the proposal.
			proposal_hash: T::Hash,
			/// Origin ID of the proposal.
			proposal_origin_id: T::OriginId,
			/// Account that added the storage ID.
			account_id: T::AccountId,
			/// Storage ID that was added.
			storage_id: BoundedVec<u8, T::MaxStorageIdLength>,
			/// Optional storage ID description of the storage ID.
			storage_id_description: Option<BoundedVec<u8, T::MaxStorageIdDescriptionLength>>,
		},
		/// A storage ID has been removed from a proposal.
		StorageIdRemoved {
			/// Hash of the proposal.
			proposal_hash: T::Hash,
			/// Origin ID of the proposal.
			proposal_origin_id: T::OriginId,
			/// Account that removed the storage ID.
			account_id: T::AccountId,
			/// Storage ID that was removed.
			storage_id: BoundedVec<u8, T::MaxStorageIdLength>,
			/// Whether the storage ID was removed by a collective origin.
			is_collective: bool,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Proposal with these parameters already exists
		ProposalAlreadyExists,
		/// Proposal could not be found
		ProposalNotFound,
		/// Proposer cannot approve their own proposal with different origin ID
		CannotApproveOwnProposalUsingDifferentOrigin,
		/// Proposal has too many approvals
		TooManyApprovals,
		/// Caller is not authorized
		NotAuthorized,
		/// Proposal has already been executed
		ProposalAlreadyExecuted,
		/// Proposal has expired
		ProposalExpired,
		/// Proposal was cancelled
		ProposalCancelled,
		/// Proposal was already approved by the origin
		AccountOriginAlreadyApproved,
		/// Proposal does not have enough approvals to execute
		InsufficientApprovals,
		/// Proposal is not pending
		ProposalNotPending,
		/// Proposal not in a terminal state(Expired, Executed,
		/// or Cancelled) and cannot be cleaned
		ProposalNotInExpiredOrExecutedState,
		/// Origin approval could not be found
		AccountOriginApprovalNotFound,
		/// Proposal retention period has not elapsed yet
		ProposalRetentionPeriodNotElapsed,
		/// Proposal is not eligible for cleanup
		ProposalNotEligibleForCleanup,
		/// Storage ID not found
		ProposalStorageIdNotFound,
		/// Storage ID already present
		StorageIdAlreadyPresent,
		/// Too many storage IDs
		TooManyStorageIds,
		/// Storage ID is too long
		StorageIdTooLong,
		/// Description is too long
		DescriptionTooLong,
		/// Remark is too long
		RemarkTooLong,
		/// Remark not found
		RemarkNotFound,
		/// Too many remarks
		TooManyRemarks,
		/// Withdrawn approval not found
		WithdrawnApprovalNotFound,
	}

	/// Status of proposal
	#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	pub enum ProposalStatus {
		/// Proposal is pending and awaiting approvals
		Pending,
		/// Proposal has been executed
		Executed,
		/// Proposal has expired
		Expired,
		/// Proposal has been cancelled
		Cancelled,
	}

	/// Enum to specify the type of remark
	#[derive(Clone, Copy)]
	enum RemarkType {
		/// Remark for an initial proposal or approval (either from propose or add_approval)
		Initial,
		/// Remark for an amendment to an existing proposal or approval
		Amend,
	}

	/// Info about specific proposal
	#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, MaxEncodedLen, TypeInfo)]
	#[scale_info(skip_type_params(RequiredApprovalsCount))]
	pub struct ProposalInfo<
		Hash,
		BlockNumber,
		OriginId,
		AccountId,
		RequiredApprovalsCount: Get<u32>,
	> {
		/// Call hash of this proposal to execute
		pub proposal_hash: Hash,
		/// Origin ID of this proposal
		pub proposal_origin_id: OriginId,
		/// Block number after which this proposal expires
		pub expiry_at: Option<BlockNumber>,
		/// List of approvals of a proposal as (AccountId, OriginId) pairs
		pub approvals: BoundedVec<(AccountId, OriginId), RequiredApprovalsCount>,
		/// Current status of this proposal
		pub status: ProposalStatus,
		/// Original proposer of this proposal
		pub proposer: AccountId,
		/// Block number when this proposal was submitted
		pub submitted_at: BlockNumber,
		/// Block number when this proposal was executed (if applicable)
		pub executed_at: Option<BlockNumber>,
		/// Whether proposal should auto-execute when it reaches `RequiredApprovalsCount`
		pub auto_execute: Option<bool>,
	}

	/// Storage for proposals
	#[pallet::storage]
	#[pallet::getter(fn proposals)]
	pub type Proposals<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		T::Hash,
		Blake2_128Concat,
		T::OriginId,
		ProposalInfo<
			T::Hash,
			BlockNumberFor<T>,
			T::OriginId,
			T::AccountId,
			T::RequiredApprovalsCount,
		>,
		OptionQuery,
	>;

	/// Storage for approvals by `OriginId`
	#[pallet::storage]
	#[pallet::getter(fn approvals)]
	pub type Approvals<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		(T::Hash, T::OriginId), // e.g. (proposal_hash, proposal_origin_id)
		Blake2_128Concat,
		T::OriginId,  // e.g. approving_origin_id or withdrawing_origin_id
		T::AccountId, // e.g. account that added the approval
		OptionQuery,
	>;

	/// Storage for calls themselves that is unbounded since
	/// `RuntimeCall` does not implement `MaxEncodedLen`
	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn proposal_calls)]
	pub type ProposalCalls<T: Config> =
		StorageMap<_, Identity, T::Hash, Box<<T as Config>::RuntimeCall>, OptionQuery>;

	/// Mapping from timepoint to call hash and used to track executed calls
	#[pallet::storage]
	#[pallet::getter(fn executed_calls)]
	pub type ExecutedCalls<T: Config> =
		StorageMap<_, Blake2_128Concat, Timepoint<BlockNumberFor<T>>, T::Hash, OptionQuery>;

	/// Tracks proposals by their expiry block for automatic expiry processing.
	/// Maps expiry block number to a vector of (proposal_hash, proposal_origin_id) tuples.
	#[pallet::storage]
	#[pallet::getter(fn expiring_proposals)]
	pub type ExpiringProposals<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		BlockNumberFor<T>,
		BoundedVec<(T::Hash, T::OriginId), ConstU32<1000>>,
		ValueQuery,
	>;

	/// Storage for withdrawn approvals
	/// Key: (proposal_hash, proposal_origin_id, approving_origin_id)
	/// Value: (account_id, block_number)
	#[pallet::storage]
	#[pallet::getter(fn withdrawn_approvals)]
	pub type WithdrawnApprovals<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		(T::Hash, T::OriginId, T::OriginId),
		Vec<(T::AccountId, BlockNumberFor<T>)>,
		OptionQuery,
	>;

	/// Storage for governance-related hashes including remarks and storage identifiers.
	/// Maps a proposal hash to a tuple containing:
	/// 1. Combined hash of all remarks
	/// 2. Individual remark hashes with their content
	/// 3. Storage identifiers (e.g. IPFS CIDs) with metadata
	#[pallet::storage]
	#[pallet::getter(fn governance_hashes)]
	pub type GovernanceHashes<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		T::Hash,
		(
			T::Hash, // Combined hash of all remarks
			BoundedBTreeMap<T::Hash, BoundedVec<u8, T::MaxRemarkLength>, T::MaxRemarksPerProposal>, /* Individual remark hashes */
			BoundedVec<
				(
					BoundedVec<u8, T::MaxStorageIdLength>, // Storage ID (e.g. IPFS CID)
					BlockNumberFor<T>,                     // Block number when added
					T::AccountId,                          // Account that added it
					Option<BoundedVec<u8, T::MaxStorageIdDescriptionLength>>, // Optional Storage ID description
				),
				T::MaxStorageIdsPerProposal,
			>,
		),
		OptionQuery,
	>;

	#[pallet::storage]
	pub(super) type Dummy<T: Config> = StorageValue<_, DummyValueOf>;
}
