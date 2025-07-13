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
use codec::{Decode, DecodeWithMemTracking, Encode, Error as CodecError};
use core::marker::PhantomData;
use frame_support::{
	dispatch::{
		ClassifyDispatch, DispatchClass, DispatchResult, DispatchResultWithPostInfo,
		GetDispatchInfo, Pays, PaysFee, WeighData,
	},
	pallet_prelude::*,
	traits::{EnsureOrigin, Get, IsSubType, IsType},
	weights::Weight,
	BoundedVec, Parameter,
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
			+ Dispatchable<RuntimeOrigin = Self::RuntimeOrigin>
			+ GetDispatchInfo
			+ From<frame_system::Call<Self>>;

		/// The hashing implementation.
		type Hashing: sp_runtime::traits::Hash;

		/// Identifier type for different origins that must maintain uniqueness and comparability.
		type OriginId: Parameter + Member + TypeInfo + Copy + Ord + MaxEncodedLen;

		/// The maximum number of approvals for a single proposal.
		/// The original specification by Dr Gavin Wood requires exactly two approvals to satisfy
		/// the "AND Gate" pattern for two origins.
		#[pallet::constant]
		type MaxApprovals: Get<u32> + Clone;

		/// How long a proposal is valid for measured in blocks before it expires.
		#[pallet::constant]
		type ProposalExpiry: Get<BlockNumberFor<Self>>;

		/// How long to retain proposal data after it reaches a terminal state of (Executed,
		/// Expired).
		///
		/// Inclusions:
		///   - Retention period starts after the proposal's expiry time, or after the current block
		///     after
		/// termination through Execution if no expiry is set, and allows on-chain queries before
		/// final cleanup.
		///
		/// Exclusions:
		///   - Retention does not occur for proposals with Cancelled status that are cleaned up
		///     immediately as
		///   they have not completed their normal lifecycle.
		///
		/// Benefits:
		///   - On-chain Queryability: Providing the option to retain proposal data in storage to
		///     provide on-chain queryability allows other pallets, smart contracts, or runtime
		///     logic to query the
		///    status and details of recently executed or expired proposals. Whilst events are
		/// emitted and    can be found in block explorers, or off-chain indexing could be used,
		/// they are not directly    queryable on-chain. Chains with storage constraints may opt
		/// to disable retention to save on    storage space.
		///   - UX: User interfaces can easily show recently executed or expired proposals without
		///     needing to scan through event logs, providing better UX for governance participants.
		///   - Dispute Resolution: For dispute about a proposal outcome the data may be readily
		///     available for a period allowing for easier verification and resolution.
		///   - Governance Analytics: Retention allows for easier on-chain analytics about proposal
		///     outcomes, success rates, and participation metrics.
		///   - Potential Recovery Actions: Governance systems may opt to challenge or reverse
		///     decisions within a certain timeframe after execution.
		#[pallet::constant]
		type NonCancelledProposalRetentionPeriod: Get<BlockNumberFor<Self>>;

		/// Maximum number of proposals to check for expiry per block.
		/// This limits the on_initialize processing to prevent excessive resource usage.
		#[pallet::constant]
		type MaxProposalsToExpirePerBlock: Get<u32>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
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
			for (proposal_hash, origin_id) in expiring.into_iter().take(to_process) {
				if let Some(mut proposal_info) = Proposals::<T>::get(&proposal_hash, &origin_id) {
					// Only update status if the proposal is still pending
					if proposal_info.status == ProposalStatus::Pending {
						proposal_info.status = ProposalStatus::Expired;
						Proposals::<T>::insert(proposal_hash, origin_id.clone(), proposal_info);

						// Emit an event
						Self::deposit_event(Event::ProposalExpired {
							proposal_hash,
							origin_id,
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
			// TODO
		}

		fn offchain_worker(_n: BlockNumberFor<T>) {
			// TODO
		}
	}

	impl<T: Config> Pallet<T> {
		/// Helper function to get error index of specific error variant
		fn error_index(error: Error<T>) -> u8 {
			match error {
				Error::ProposalAlreadyExists => 0,
				Error::ProposalNotFound => 1,
				Error::CannotApproveOwnProposalUsingDifferentOrigin => 2,
				Error::TooManyApprovals => 3,
				Error::NotAuthorized => 4,
				Error::ProposalAlreadyExecuted => 5,
				Error::ProposalExpired => 6,
				Error::ProposalCancelled => 7,
				Error::OriginAlreadyApproved => 8,
				Error::InsufficientApprovals => 9,
				Error::ProposalNotPending => 10,
				Error::ProposalNotInExpiredOrExecutedState => 11,
				Error::OriginApprovalNotFound => 12,
				Error::ProposalRetentionPeriodNotElapsed => 13,
				Error::ProposalNotEligibleForCleanup => 14,
			}
		}

		/// Helper function to check if a proposal has sufficient approvals and execute it
		fn check_and_execute_proposal(
			proposal_hash: T::Hash,
			origin_id: T::OriginId,
			mut proposal_info: ProposalInfo<
				T::Hash,
				BlockNumberFor<T>,
				T::OriginId,
				T::AccountId,
				T::MaxApprovals,
			>,
		) -> DispatchResult {
			// Ensure proposal is in Pending state
			ensure!(
				proposal_info.status == ProposalStatus::Pending,
				Error::<T>::ProposalNotPending
			);

			// Check for number of approvals required from MaxApprovals
			if proposal_info.approvals.len() >= T::MaxApprovals::get() as usize {
				// Retrieve the actual call from storage
				if let Some(call) = <ProposalCalls<T>>::get(proposal_hash) {
					// Execute the call with root origin
					let result = call.dispatch(frame_system::RawOrigin::Root.into());

					// Update proposal status
					proposal_info.status = ProposalStatus::Executed;
					proposal_info.executed_at = Some(frame_system::Pallet::<T>::block_number());

					// Remove from ExpiringProposals if has expiry since don't need to
					// track executed proposals for expiry
					if let Some(expiry) = proposal_info.expiry {
						ExpiringProposals::<T>::mutate(expiry, |proposals| {
							proposals
								.retain(|(hash, id)| *hash != proposal_hash || *id != origin_id);
						});
					}

					<Proposals<T>>::insert(proposal_hash, origin_id.clone(), proposal_info);

					// Create timepoint for execution
					let execution_timepoint = Self::current_timepoint();

					// Store in ExecutedCalls mapping
					<ExecutedCalls<T>>::insert(execution_timepoint.clone(), proposal_hash);

					Self::deposit_event(Event::ProposalExecuted {
						proposal_hash,
						origin_id,
						result: result.map(|_| ()).map_err(|e| e.error),
						timepoint: execution_timepoint,
					});

					return Ok(());
				}

				return Err(Error::<T>::ProposalNotFound.into());
			}

			// Return an error when there aren't enough approvals
			Err(Error::<T>::InsufficientApprovals.into())
		}

		/// Helper function to check if a proposal expired and update its status if necessary
		fn check_proposal_expiry(
			proposal_hash: T::Hash,
			origin_id: &T::OriginId,
			proposal_info: &mut ProposalInfo<
				T::Hash,
				BlockNumberFor<T>,
				T::OriginId,
				T::AccountId,
				T::MaxApprovals,
			>,
		) -> bool {
			// Only check expiry for pending proposals
			if proposal_info.status == ProposalStatus::Pending {
				let current_block = frame_system::Pallet::<T>::block_number();

				// Check if proposal has expired
				if let Some(expiry) = proposal_info.expiry {
					if current_block > expiry {
						// Update proposal status to expired
						proposal_info.status = ProposalStatus::Expired;

						// Update storage
						<Proposals<T>>::insert(
							proposal_hash,
							origin_id.clone(),
							proposal_info.clone(),
						);

						// Emit event
						Self::deposit_event(Event::ProposalExpired {
							proposal_hash,
							origin_id: origin_id.clone(),
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
		fn remove_proposal_storage(proposal_hash: T::Hash, origin_id: T::OriginId) {
			<ProposalCalls<T>>::remove(proposal_hash);
			<Approvals<T>>::remove_prefix((proposal_hash, origin_id.clone()), None);
			<Proposals<T>>::remove(proposal_hash, origin_id);
		}

		/// Helper to check if terminal proposal (Executed, Expired, Cancelled) is eligible for
		/// cleanup based on retention period
		fn is_terminal_proposal_eligible_for_cleanup(
			proposal: &ProposalInfo<
				T::Hash,
				BlockNumberFor<T>,
				T::OriginId,
				T::AccountId,
				T::MaxApprovals,
			>,
		) -> bool {
			// Get current block number
			let current_block = frame_system::Pallet::<T>::block_number();

			// Check if proposal in terminal state and not pending
			if (proposal.status == ProposalStatus::Executed ||
				proposal.status == ProposalStatus::Expired ||
				proposal.status == ProposalStatus::Cancelled) &&
				proposal.status != ProposalStatus::Pending
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
						.saturating_add(T::NonCancelledProposalRetentionPeriod::get()),
					// Expired proposals use expiry as base
					ProposalStatus::Expired => match proposal.expiry {
						Some(expiry) =>
							expiry.saturating_add(T::NonCancelledProposalRetentionPeriod::get()),
						None => current_block
							.saturating_add(T::NonCancelledProposalRetentionPeriod::get()),
					},
					// This should never be reached due to the earlier check for Cancelled status
					_ =>
						current_block.saturating_add(T::NonCancelledProposalRetentionPeriod::get()),
				};

				// Proposal is eligible for cleanup if we passed cleanup_eligible_block
				current_block >= cleanup_eligible_block
			} else {
				// Non-terminal proposals are not eligible for cleanup
				false
			}
		}

		/// Helper function to get the current timepoint
		fn current_timepoint() -> Timepoint<BlockNumberFor<T>> {
			Timepoint {
				height: frame_system::Pallet::<T>::block_number(),
				index: frame_system::Pallet::<T>::extrinsic_index().unwrap_or_default(),
			}
		}
	}

	#[pallet::call(weight(<T as Config>::WeightInfo))]
	impl<T: Config> Pallet<T> {
		/// Submit a proposal for approval, recording the first origin's approval.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::propose())]
		pub fn propose(
			origin: OriginFor<T>,
			call: Box<<T as Config>::RuntimeCall>,
			origin_id: T::OriginId,
			expiry: Option<BlockNumberFor<T>>,
		) -> DispatchResultWithPostInfo {
			// Check extrinsic was signed
			let who = ensure_signed(origin)?;

			// Compute hash of call for storage using system hashing implementation
			let proposal_hash = <T as frame_system::Config>::Hashing::hash_of(&call);

			// Check if given proposal already exists
			ensure!(
				!<Proposals<T>>::contains_key(proposal_hash, origin_id),
				Error::<T>::ProposalAlreadyExists
			);

			// Get current block number
			let current_block = frame_system::Pallet::<T>::block_number();

			// Determine expiration block number if provided or otherwise use default
			let expiry_block = match expiry {
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

			// Store actual call data (unbounded) to save storage
			<ProposalCalls<T>>::insert(proposal_hash, call);

			// Create an empty bounded vec for approvals
			let mut approvals =
				BoundedVec::<(T::AccountId, T::OriginId), T::MaxApprovals>::default();

			// Add proposer as first approval
			if let Err(_) = approvals.try_push((who.clone(), origin_id.clone())) {
				return Err(Error::<T>::TooManyApprovals.into());
			}

			// Create and store proposal metadata (bounded storage)
			let proposal_info = ProposalInfo {
				call_hash: proposal_hash,
				expiry: expiry_block,
				approvals,
				status: ProposalStatus::Pending,
				proposer: who.clone(),
				submitted_at: current_block,
				executed_at: None,
			};

			// Store proposal metadata (bounded storage)
			<Proposals<T>>::insert(proposal_hash, origin_id.clone(), proposal_info);

			// Mark first approval in approvals storage efficiently
			<Approvals<T>>::insert(
				(proposal_hash, origin_id.clone()),
				origin_id.clone(),
				who.clone(),
			);

			// Add proposal to expiry tracking for automatic expiry
			ExpiringProposals::<T>::mutate(expiry_block.unwrap(), |proposals| {
				if let Err(_) = proposals.try_push((proposal_hash, origin_id.clone())) {
					log::warn!("Too many proposals expiring in the same block. Some proposals may not be automatically expired.");
				}
			});

			// Create timepoint for submission
			let submission_timepoint = Self::current_timepoint();

			// Emit event
			Self::deposit_event(Event::ProposalCreated {
				proposal_hash,
				origin_id,
				timepoint: submission_timepoint,
			});

			Ok(().into())
		}

		/// Approve a previously submitted proposal.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::add_approval())]
		pub fn add_approval(
			origin: OriginFor<T>,
			call_hash: T::Hash,
			origin_id: T::OriginId,
			approving_origin_id: T::OriginId,
			auto_execute: bool,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			// Try to fetch proposal from storage first
			let mut proposal_info =
				<Proposals<T>>::get(&call_hash, &origin_id).ok_or(Error::<T>::ProposalNotFound)?;

			// Check if caller is same as proposer of proposal but using a different origin ID
			if who == proposal_info.proposer && approving_origin_id != origin_id {
				return Err(Error::<T>::CannotApproveOwnProposalUsingDifferentOrigin.into());
			}

			// Check if proposal has expired
			if Self::check_proposal_expiry(call_hash, &origin_id, &mut proposal_info) {
				return Err(Error::<T>::ProposalExpired.into());
			}

			// Check if proposal still pending
			if proposal_info.status != ProposalStatus::Pending {
				return match proposal_info.status {
					ProposalStatus::Executed => Err(Error::<T>::ProposalAlreadyExecuted.into()),
					ProposalStatus::Expired => Err(Error::<T>::ProposalExpired.into()),
					_ => Err(Error::<T>::ProposalNotFound.into()),
				};
			}

			// Check if origin_id already approved
			if <Approvals<T>>::contains_key(
				(call_hash, origin_id.clone()),
				approving_origin_id.clone(),
			) {
				return Err(Error::<T>::OriginAlreadyApproved.into());
			}

			// Add to storage to mark this origin as approved
			<Approvals<T>>::insert(
				(call_hash, origin_id.clone()),
				approving_origin_id.clone(),
				who.clone(),
			);

			// Add to proposal's approvals list if not yet present
			if !proposal_info.approvals.contains(&(who.clone(), approving_origin_id.clone())) {
				if proposal_info
					.approvals
					.try_push((who.clone(), approving_origin_id.clone()))
					.is_err()
				{
					return Err(Error::<T>::TooManyApprovals.into());
				}
			}

			// Update proposal in storage with new origin approval
			<Proposals<T>>::insert(call_hash, origin_id.clone(), &proposal_info);

			// Emit approval event
			let approval_timepoint = Self::current_timepoint();
			Self::deposit_event(Event::OriginApprovalAdded {
				proposal_hash: call_hash,
				origin_id: origin_id.clone(),
				approving_origin_id,
				timepoint: approval_timepoint,
			});

			// Pass a clone of proposal info so original does not get modified if execution attempt
			// fails
			if auto_execute {
				match Self::check_and_execute_proposal(call_hash, origin_id, proposal_info.clone())
				{
					// Success case results in proposal being executed
					Ok(_) => {},
					// Check if error is specifically the `InsufficientApprovals` error since we
					// need to silently ignore it when adding early approvals
					Err(e) => match e {
						DispatchError::Module(module_error) => {
							if module_error.index == <Self as PalletInfoAccess>::index() as u8 {
								let insufficient_approvals_index =
									Self::error_index(Error::<T>::InsufficientApprovals);

								// Propagate all errors except `InsufficientApprovals` error
								if module_error.error[0] != insufficient_approvals_index {
									return Err(DispatchError::Module(module_error).into());
								}
								// Otherwise silently ignore InsufficientApprovals error
							} else {
								// Error from another pallet must always be propagated
								return Err(DispatchError::Module(module_error).into());
							}
						},
						// Non-module errors must always be propagated
						_ => return Err(e.into()),
					},
				}
			}

			Ok(().into())
		}

		/// Execute a proposal that has met the required approvals
		#[pallet::call_index(2)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::execute_proposal())]
		pub fn execute_proposal(
			origin: OriginFor<T>,
			proposal_hash: T::Hash,
			origin_id: T::OriginId,
		) -> DispatchResultWithPostInfo {
			ensure_signed(origin)?;

			// Get proposal info
			let mut proposal = <Proposals<T>>::get(&proposal_hash, &origin_id)
				.ok_or(Error::<T>::ProposalNotFound)?;

			// Check if proposal has expired using lazy expiry checking
			if Self::check_proposal_expiry(proposal_hash, &origin_id, &mut proposal) {
				return Err(Error::<T>::ProposalExpired.into());
			}

			// Execute the proposal
			Self::check_and_execute_proposal(proposal_hash, origin_id, proposal)?;

			Ok(().into())
		}

		/// Cancel pending proposal is only callable by original proposer
		#[pallet::call_index(3)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::cancel_proposal())]
		pub fn cancel_proposal(
			origin: OriginFor<T>,
			proposal_hash: T::Hash,
			origin_id: T::OriginId,
		) -> DispatchResultWithPostInfo {
			// Check extrinsic was signed
			let who = ensure_signed(origin)?;

			// Get proposal info
			let mut proposal_info = Proposals::<T>::get(&proposal_hash, &origin_id)
				.ok_or(Error::<T>::ProposalNotFound)?;

			// Check if proposal has expired
			if Self::check_proposal_expiry(proposal_hash, &origin_id, &mut proposal_info) {
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
			// TODO: Potentially remove since this is just for completeness
			// and potentially unnecessary since status will be removed from storage
			// shortly after and proposal cancelled event will be emitted
			// at lower cost
			proposal_info.status = ProposalStatus::Cancelled;

			// Store the expiry before moving proposal_info
			let expiry = proposal_info.expiry;

			// Update storage with cancelled status
			<Proposals<T>>::insert(&proposal_hash, &origin_id, proposal_info);

			// Clean up all storage related to the proposal
			Self::remove_proposal_storage(proposal_hash, origin_id.clone());

			// Remove from ExpiringProposals if it has an expiry
			// since we don't need to track cancelled proposals for expiry
			if let Some(expiry) = expiry {
				ExpiringProposals::<T>::mutate(expiry, |proposals| {
					proposals.retain(|(hash, id)| *hash != proposal_hash || *id != origin_id);
				});
			}

			// Create timepoint for cancellation
			let cancellation_timepoint = Self::current_timepoint();

			// Emit event
			Self::deposit_event(Event::ProposalCancelled {
				proposal_hash,
				origin_id,
				timepoint: cancellation_timepoint,
			});

			Ok(().into())
		}

		/// Withdraw an approval for the proposal associated with an origin.
		///
		/// Only callable by an origin that has approved the proposal.
		///
		/// - `origin`: Must be a valid authority (i.e. entity) that approves the proposal.
		/// - `proposal_hash`: The proposal hash to withdraw approval for.
		/// - `origin_id`: The origin id that the proposal belongs to.
		/// - `withdrawing_origin_id`: The origin id to withdraw the approval for since the account
		///   might need to specify which of their multiple origin authorities they approved with
		///   that they are now withdrawing approval for.
		#[pallet::call_index(4)]
		#[pallet::weight((<T as pallet::Config>::WeightInfo::withdraw_approval(), DispatchClass::Normal))]
		pub fn withdraw_approval(
			origin: OriginFor<T>,
			proposal_hash: T::Hash,
			origin_id: T::OriginId,
			withdrawing_origin_id: T::OriginId,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			// Get proposal info
			let mut proposal = <Proposals<T>>::get(&proposal_hash, &origin_id)
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
			let approval_account = <Approvals<T>>::get(
				(proposal_hash, origin_id.clone()),
				withdrawing_origin_id.clone(),
			)
			.ok_or(Error::<T>::OriginApprovalNotFound)?;

			ensure!(approval_account == who, Error::<T>::NotAuthorized);

			// Find position of withdrawing_origin_id in approvals vector
			let pos = proposal
				.approvals
				.iter()
				.position(|a| a == &(who.clone(), withdrawing_origin_id.clone()))
				.ok_or(Error::<T>::OriginApprovalNotFound)?;

			// Remove approval at found position
			proposal.approvals.swap_remove(pos);

			// Update proposal in storage
			<Proposals<T>>::insert(&proposal_hash, &origin_id, &proposal);

			// Remove approval from Approvals storage
			<Approvals<T>>::remove((proposal_hash, origin_id), withdrawing_origin_id);

			// Emit event
			let withdrawal_timepoint = Self::current_timepoint();
			Self::deposit_event(Event::OriginApprovalWithdrawn {
				proposal_hash,
				origin_id,
				withdrawing_origin_id,
				timepoint: withdrawal_timepoint,
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
		#[pallet::call_index(5)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::set_dummy())]
		pub fn set_dummy(origin: OriginFor<T>, new_value: DummyValueOf) -> DispatchResult {
			ensure_root(origin)?;

			info!("New value is now: {:?}", new_value);

			// Put the new value into storage.
			<Dummy<T>>::put(new_value.clone());

			Self::deposit_event(Event::SetDummy { dummy_value: new_value });

			// All good, no refund.
			Ok(())
		}

		/// Clean up storage for a proposal that is no longer pending
		#[pallet::call_index(6)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::clean())]
		pub fn clean(
			origin: OriginFor<T>,
			proposal_hash: T::Hash,
			origin_id: T::OriginId,
		) -> DispatchResultWithPostInfo {
			ensure_signed(origin)?;

			// Get proposal info
			let proposal = <Proposals<T>>::get(&proposal_hash, &origin_id)
				.ok_or(Error::<T>::ProposalNotFound)?;

			// Ensure proposal is in terminal state (Expired, Executed, or Cancelled)
			// and not in the Pending state
			ensure!(
				(proposal.status == ProposalStatus::Expired ||
					proposal.status == ProposalStatus::Executed ||
					proposal.status == ProposalStatus::Cancelled) &&
					proposal.status != ProposalStatus::Pending,
				Error::<T>::ProposalNotInExpiredOrExecutedState
			);

			// Check if proposal has passed retention period
			ensure!(
				Self::is_terminal_proposal_eligible_for_cleanup(&proposal),
				Error::<T>::ProposalRetentionPeriodNotElapsed
			);

			// Clean up all storage
			Self::remove_proposal_storage(proposal_hash, origin_id.clone());

			// Emit cleanup event
			let cleanup_timepoint = Self::current_timepoint();

			Self::deposit_event(Event::ProposalCleaned {
				proposal_hash,
				origin_id,
				timepoint: cleanup_timepoint,
			});

			Ok(().into())
		}

		/// A dummy function for use in tests and benchmarks
		#[pallet::call_index(7)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::set_dummy())]
		pub fn dummy_benchmark(
			origin: OriginFor<T>,
			remark: Vec<u8>,
		) -> DispatchResultWithPostInfo {
			ensure_signed(origin)?;
			Ok(().into())
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A proposal has been created.
		ProposalCreated {
			proposal_hash: T::Hash,
			origin_id: T::OriginId,
			timepoint: Timepoint<BlockNumberFor<T>>,
		},
		/// An origin has added their approval of a proposal.
		OriginApprovalAdded {
			proposal_hash: T::Hash,
			origin_id: T::OriginId,
			approving_origin_id: T::OriginId,
			timepoint: Timepoint<BlockNumberFor<T>>,
		},
		/// A proposal has been executed.
		ProposalExecuted {
			proposal_hash: T::Hash,
			origin_id: T::OriginId,
			result: Result<(), DispatchError>,
			timepoint: Timepoint<BlockNumberFor<T>>,
		},
		/// A proposal has expired.
		ProposalExpired {
			proposal_hash: T::Hash,
			origin_id: T::OriginId,
			timepoint: Timepoint<BlockNumberFor<T>>,
		},
		/// A proposal has been cancelled.
		ProposalCancelled {
			proposal_hash: T::Hash,
			origin_id: T::OriginId,
			timepoint: Timepoint<BlockNumberFor<T>>,
		},
		/// An origin has withdrawn their approval of a proposal.
		OriginApprovalWithdrawn {
			proposal_hash: T::Hash,
			origin_id: T::OriginId,
			withdrawing_origin_id: T::OriginId,
			timepoint: Timepoint<BlockNumberFor<T>>,
		},
		SetDummy {
			dummy_value: DummyValueOf,
		},
		/// A proposal's storage was cleaned up
		ProposalCleaned {
			proposal_hash: T::Hash,
			origin_id: T::OriginId,
			timepoint: Timepoint<BlockNumberFor<T>>,
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
		/// Caller is not authorized to approve
		NotAuthorized,
		/// Proposal is not pending
		ProposalNotPending,
		/// Proposal has already been executed
		ProposalAlreadyExecuted,
		/// Proposal has expired
		ProposalExpired,
		/// Proposal was cancelled
		ProposalCancelled,
		/// Proposal was already approved by the origin
		OriginAlreadyApproved,
		/// Proposal does not have enough approvals to execute
		InsufficientApprovals,
		/// Origin approval could not be found
		OriginApprovalNotFound,
		/// Proposal not in a terminal state(Expired, Executed,
		/// or Cancelled) and cannot be cleaned
		ProposalNotInExpiredOrExecutedState,
		/// Proposal retention period has not elapsed yet
		ProposalRetentionPeriodNotElapsed,
		/// Proposal is not eligible for cleanup
		ProposalNotEligibleForCleanup,
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

	/// Info about specific proposal
	#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, MaxEncodedLen, TypeInfo)]
	#[scale_info(skip_type_params(MaxApprovals))]
	pub struct ProposalInfo<Hash, BlockNumber, OriginId, AccountId, MaxApprovals: Get<u32>> {
		/// Call hash of this proposal to execute
		pub call_hash: Hash,
		/// Block number after which this proposal expires
		pub expiry: Option<BlockNumber>,
		/// List of approvals of a proposal as (AccountId, OriginId) pairs
		pub approvals: BoundedVec<(AccountId, OriginId), MaxApprovals>,
		/// Current status of this proposal
		pub status: ProposalStatus,
		/// Original proposer of this proposal
		pub proposer: AccountId,
		/// Block number when this proposal was submitted
		pub submitted_at: BlockNumber,
		/// Block number when this proposal was executed (if applicable)
		pub executed_at: Option<BlockNumber>,
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
		ProposalInfo<T::Hash, BlockNumberFor<T>, T::OriginId, T::AccountId, T::MaxApprovals>,
		OptionQuery,
	>;

	/// Storage for approvals by `OriginId`
	#[pallet::storage]
	#[pallet::getter(fn approvals)]
	pub type Approvals<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		(T::Hash, T::OriginId), // e.g. (proposal_hash, origin_id)
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
	/// Maps expiry block number to a vector of (proposal_hash, origin_id) tuples.
	#[pallet::storage]
	#[pallet::getter(fn expiring_proposals)]
	pub type ExpiringProposals<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		BlockNumberFor<T>,
		BoundedVec<(T::Hash, T::OriginId), ConstU32<1000>>,
		ValueQuery,
	>;

	#[pallet::storage]
	pub(super) type Dummy<T: Config> = StorageValue<_, DummyValueOf>;
}
