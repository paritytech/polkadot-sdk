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
//!# Multisig Stateful Pallet
//!
//! //! ## WARNING
//!
//! NOT YET AUDITED. DO NOT USE IN PRODUCTION.
//!
//!A module to facilitate **stateful** multisig accounts. The statefulness of this means that we store a multisig account id in the state with  
//!related info (owners, threshold,..etc). The module affords enhanced control over administrative operations such as adding/removing owners, changing the threshold, account deletion, canceling an existing proposal. Each owner can approve/revoke a proposal.  
//!
//!We use `proposal` in this module to refer to an extrinsic that is to be dispatched from a multisig account after getting enough approvals.
//!
//!## Use Cases
//!
//!* Corporate Governance:
//!In a corporate setting, multisig accounts can be employed for decision-making processes. For example, a company may require the approval of multiple executives to initiate   significant financial transactions.
//!
//!* Joint Accounts:
//!Multisig accounts can be used for joint accounts where multiple individuals need to authorize transactions. This is particularly useful in family finances or shared  
//!business accounts.
//!
//!* Decentralized Autonomous Organizations (DAOs):
//!DAOs can utilize multisig accounts to ensure that decisions are made collectively. Multiple key holders can be required to approve changes to the organization's rules or  
//!the allocation of funds.
//!
//!... and much more.
//!
//!## Stateless Multisig vs Stateful Multisig
//!
//!### Overview
//!
//!All of the mentioned use cases -and more- are better served by a stateful multisig account. This is because a stateful multisig account is stored in the state and allows for more control over the account itself. For example, a stateful multisig account can be deleted, owners can be added/removed, threshold can be changed, proposals can be canceled,..etc.  
//!
//!A stateless multisig account is a multisig account that is not stored in the state. It is a simple call that is dispatched from a single account. This is useful for simple use cases where a multisig account is needed for a single purpose and no further control is needed over the account itself.
//!
//!### Extrensics (Frame/Multisig vs Stateful Multisig) -- Skip if not familiar with Frame/Multisig
//!
//!Main distinction in proposal approvals and execution between this implementation and the frame/multisig one is that this module  
//!has an extrinsic for each step of the process instead of having one entry point that can accept a `CallOrHash`:  
//!
//!1. Start Proposal
//!2. Approve (called N times based on the threshold needed)
//!3. Execute Proposal
//!
//!This is illustrated in the sequence diagram later in the README.
//!
//!### Technical Comparison
//!
//!Although a stateful multisig account might seem more expensive than a stateless one because it is stored in the state while stateless multisig is not, We see (on paper) that the stateless footprint is actually larger than the stateful one on the blockchain as for each extrinsic call in a stateless multisig, the caller needs to send all the owners and other parameters which are all stored on the blockchain itself.
//!
//!TODO: Add benchmark results for both stateless and stateful multisig. (main thing to measure is the storage of extrinsics cost) over one year with 1K multisig accounts,
//!each with 5-100 users and doing 50 proposals per day.
//!
//! - [`Config`]
//! - [`Call`]
//!
//! ### Dispatchable Functions
//! * [`create_multisig`](Call::create_multisig`) - Creates a new multisig account and attach owners with a threshold to it.
//! * [`start_proposal`](`Call::start_proposal`) - Start a multisig proposal.
//! * [`approve`](`Call::approve`) - Approve a multisig proposal.
//! * [`revoke`](`Call::revoke`) - Revoke a multisig approval from an existing proposal.
//! * [`execute_proposal`](`Call::execute_proposal`) - Execute a multisig proposal.
//!
//! Note: Next functions need to be called from the multisig account itself.
//!
//! * [`add_owner`](`Call::add_owner`) - Add a new owner to a multisig account.
//! * [`remove_owner`](`Call::remove_owner`) - Remove an owner from a multisig account.
//! * [`set_threshold`](`Call::set_threshold`) - Change the threshold of a multisig account.
//! * [`cancel_proposal`](`Call::cancel_proposal`) - Cancel a multisig proposal.
//! * [`delete_account`](`Call::delete_account`) - Delete a multisig account.

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

// TODO:
// Cleanup Proposals
// Deposits
// Beanchmarking

use frame_support::traits::fungible::MutateHold;
use frame_support::{
	pallet_prelude::*,
	storage::KeyLenOf,
	traits::{fungible, tokens::Precision},
};
use frame_system::pallet_prelude::BlockNumberFor;
pub use pallet::*;
use scale_info::prelude::collections::BTreeSet;
#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod types;
use crate::types::*;

use sp_io::hashing::blake2_256;
use sp_runtime::{
	traits::{Dispatchable, Hash, TrailingZeroInput},
	BoundedBTreeSet,
};
/// The log target of this pallet.
pub const LOG_TARGET: &'static str = "runtime::multisig_stateful";

// syntactic sugar for logging.
#[macro_export]
macro_rules! log {
	($level:tt, $patter:expr $(, $values:expr)* $(,)?) => {
		log::$level!(
			target: crate::LOG_TARGET,
			concat!("[{:?}] ✍️ ", $patter), <frame_system::Pallet<T>>::block_number() $(, $values)*
		)
	};
}

#[frame_support::pallet]
pub mod pallet {

	use crate::*;
	use frame_support::dispatch::{GetDispatchInfo, RawOrigin};
	use frame_support::traits::fungible::MutateHold;
	use frame_system::pallet_prelude::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);
	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	// #[pallet::config(with_default)]
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		type Currency: fungible::Inspect<Self::AccountId>
			+ fungible::hold::Mutate<Self::AccountId, Reason = Self::RuntimeHoldReason>;

		/// The hold reason when reserving funds.
		type RuntimeHoldReason: From<HoldReason>;

		/// A dispatchable call.
		// #[pallet::no_default_bounds]
		type RuntimeCall: Parameter
			+ Dispatchable<RuntimeOrigin = Self::RuntimeOrigin>
			+ GetDispatchInfo;

		/// The amount held on deposit for a created Multisig account.
		#[pallet::constant]
		type CreationDeposit: Get<BalanceOf<Self>>;

		/// The amount held on deposit for a new proposal.
		#[pallet::constant]
		type ProposalDeposit: Get<BalanceOf<Self>>;

		/// The maximum amount of signatories/owners allowed in the multisig.
		#[pallet::constant]
		type MaxSignatories: Get<u32>;

		/// The maximum amount of proposals to remove in one call when cleaning up the storage.
		/// Don't change the u8 as we don't want to allow a large number of proposals to be removed in one call
		/// to eliminate the possibility of a DoS attack.
		#[pallet::constant]
		type RemoveProposalsLimit: Get<u8>;
	}

	/// Each multisig account (key) has a set of current owners with a threshold.
	#[pallet::storage]
	pub type MultisigAccount<T: Config> =
		StorageMap<_, Twox64Concat, T::AccountId, MultisigAccountDetails<T>>;

	/// The set of open multisig proposals. A proposal is uniquely identified by the multisig account and the call hash.
	/// (maybe a nonce as well in the future)
	#[pallet::storage]
	pub type PendingProposals<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		T::AccountId,
		Blake2_128Concat,
		T::Hash, // Call Hash
		MultisigProposal<T>,
	>;

	/// Clear-cursor for pending proposals, map from MultisigAccountId -> (Maybe) Cursor.
	#[pallet::storage]
	pub(super) type ProposalsClearCursor<T: Config> =
		StorageMap<_, Twox64Concat, T::AccountId, BoundedVec<u8, KeyLenOf<PendingProposals<T>>>>;

	/// A reason for the pallet placing a hold on funds.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// Funds are held for creating a new multisig account.
		#[codec(index = 0)]
		MultisigCreation,
		/// Funds are held for creating a new proposal.
		#[codec(index = 1)]
		ProposalCreation,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// New multisig account created
		CreatedMultisig {
			multisig_account: T::AccountId,
			created_by: T::AccountId,
		},
		/// Multisig account deleted
		DeletedMultisig {
			multisig_account: T::AccountId,
		},
		/// A new owner added to multisig account
		AddedOwner {
			multisig_account: T::AccountId,
			added_owner: T::AccountId,
			threshold: u32,
		},
		/// An owner removed from  multisig account
		RemovedOwner {
			multisig_account: T::AccountId,
			removed_owner: T::AccountId,
			threshold: u32,
		},
		/// A multisig proposal has been approved.
		ApprovedProposal {
			approving_account: T::AccountId,
			multisig_account: T::AccountId,
			call_hash: T::Hash,
		},
		/// A multisig approval for a specific proposal has been revoked.
		RevokedApproval {
			revoking_account: T::AccountId,
			multisig_account: T::AccountId,
			call_hash: T::Hash,
		},
		/// A multisig proposal has started
		StartedProposal {
			proposer: T::AccountId,
			multisig_account: T::AccountId,
			call_hash: T::Hash,
		},
		/// A multisig proposal was completely approved and executed.
		ExecutedProposal {
			executor: T::AccountId,
			multisig_account: T::AccountId,
			call_hash: T::Hash, // Call Hash
			result: DispatchResult,
		},
		/// A multisig proposal has been cancelled.
		CanceledProposal {
			multisig_account: T::AccountId,
			call_hash: T::Hash,
		},
		/// New threshold set for multisig account.
		ChangedThreshold {
			multisig_account: T::AccountId,
			new_threshold: u32,
		},
		PendingProposalsCleared {
			multisig_account: T::AccountId,
		},
	}

	// Errors inform users that something went wrong.
	#[pallet::error]
	pub enum Error<T> {
		/// User already approved this multisig operation.
		AlreadyApproved,
		/// Threshold needs to be more than 0 and less than or equal to the number of owners.
		InvalidThreshold,
		/// There are too many signatories in approvers.
		TooManySignatories,
		/// Too many owners to create a multisig account or to add a new owner.
		TooManyOwners,
		/// Multisig account id not found.
		MultisigNotFound,
		/// Multisig account id still exists, can't remove related proposals.
		MultisigStillExists,
		/// Proposal not found.
		ProposalNotFound,
		/// Only accounts that are owners can do operations on multisig.
		UnAuthorizedOwner,
		/// Not enough approvers to execute the multisig operation.
		NotEnoughApprovers,
		/// The proposal for the call already exists.
		ProposalAlreadyExists,
		/// The owner already exists in the multisig account.
		OwnerAlreadyExists,
		/// Trying to do an operation concenring an owner that does not exist. (e.g. `remove_owner`` and `revoke` both needs the owner to exist to remove/revoke it.)
		OwnerNotFound,
		/// An error from the underlying `Currency`.
		CurrencyError,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Creates a new multisig account and attach owners with a threshold to it.
		///
		/// The dispatch origin for this call must be _Signed_. It is expected to be a nomral AccountId and not a Multisig AccountId.
		///
		/// - `owners`: Initial set of accounts to add to the multisig. These may be updated later via `add_owner` and `remove_owner`.
		/// - `threshold`: The threshold number of accounts required to approve an action. Must be greater than 0 and less than or equal to the total number of owners.
		///
		/// # Errors
		///
		/// * `TooManySignatories` - The number of signatories exceeds the maximum allowed.
		/// * `InvalidThreshold` - The threshold is greater than the total number of owners.
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::default())]
		pub fn create_multisig(
			origin: OriginFor<T>,
			owners: BoundedBTreeSet<T::AccountId, T::MaxSignatories>,
			threshold: u32,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			ensure!(owners.len() <= T::MaxSignatories::get() as usize, Error::<T>::TooManyOwners);
			// Inital check to make sure that the threshold is correct. We have another check later after making sure that the owners are unique.
			ensure!(
				threshold > 0 && threshold <= owners.len() as u32,
				Error::<T>::InvalidThreshold
			);

			T::Currency::hold(
				&HoldReason::MultisigCreation.into(),
				&who,
				T::CreationDeposit::get(),
			)
			.map_err(|_| Error::<T>::CurrencyError)?;

			let multisig_account = Self::get_multisig_account_id(&owners, Self::timepoint());

			let multisig_details: MultisigAccountDetails<T> = MultisigAccountDetails {
				owners: owners.clone(),
				threshold,
				creator: who.clone(),
				deposit: T::CreationDeposit::get(),
			};
			MultisigAccount::<T>::insert(&multisig_account, multisig_details);
			Self::deposit_event(Event::CreatedMultisig { multisig_account, created_by: who });
			Ok(())
		}

		/// Starts a new proposal for a dispatchable call for a multisig account.
		/// The caller must be one of the owners of the multisig account.
		///
		/// # Arguments
		///
		/// * `multisig_account` - The multisig account ID.
		/// * `call` - The dispatchable call to be executed.
		///
		/// # Errors
		///
		/// * `MultisigNotFound` - The multisig account does not exist.
		/// * `UnAuthorizedOwner` - The caller is not an owner of the multisig account.
		/// * `TooManySignatories` - The number of signatories exceeds the maximum allowed. (shouldn't really happen as it's the first approval)
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::default())]
		pub fn start_proposal(
			origin: OriginFor<T>,
			multisig_account: T::AccountId,
			call_hash: T::Hash,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let multisig_details =
				MultisigAccount::<T>::get(&multisig_account).ok_or(Error::<T>::MultisigNotFound)?;
			// Check that the caller is an owner of the multisig account.
			ensure!(multisig_details.has_owner(&who), Error::<T>::UnAuthorizedOwner);

			// Shouldn't start a new proposal if it already exists.
			ensure!(
				!PendingProposals::<T>::contains_key(&multisig_account, &call_hash),
				Error::<T>::ProposalAlreadyExists
			);

			// Hold the proposal deposit.
			T::Currency::hold(
				&HoldReason::ProposalCreation.into(),
				&who,
				T::ProposalDeposit::get(),
			)
			.map_err(|_| Error::<T>::CurrencyError)?;

			// Add the new approver to the list
			let mut approvers = BoundedBTreeSet::new();
			approvers.try_insert(who.clone()).map_err(|_| Error::<T>::TooManySignatories)?;

			// Update the proposal with the new approvers.
			PendingProposals::<T>::insert(
				&multisig_account,
				&call_hash,
				MultisigProposal {
					creator: who.clone(),
					creation_deposit: T::ProposalDeposit::get(),
					when: Self::timepoint(),
					approvers,
					expire_after: None,
				},
			);

			Self::deposit_event(Event::StartedProposal {
				proposer: who,
				multisig_account,
				call_hash,
			});
			Ok(())
		}

		/// Approves a proposal for a dispatchable call for a multisig account.
		/// The caller must be one of the owners of the multisig account.
		///
		/// # Arguments
		///
		/// * `multisig_account` - The multisig account ID.
		/// * `call_hash` - The hash of the call to be approved. (This will be the hash of the call that was used in `start_proposal`)
		///
		/// # Errors
		///
		/// * `MultisigNotFound` - The multisig account does not exist.
		/// * `UnAuthorizedOwner` - The caller is not an owner of the multisig account.
		/// * `TooManySignatories` - The number of signatories exceeds the maximum allowed.
		/// This shouldn't really happen as it's an approval, not an addition of a new owner.
		#[pallet::call_index(2)]
		#[pallet::weight(Weight::default())]
		pub fn approve(
			origin: OriginFor<T>,
			multisig_account: T::AccountId,
			call_hash: T::Hash,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let multisig_details =
				MultisigAccount::<T>::get(&multisig_account).ok_or(Error::<T>::MultisigNotFound)?;

			ensure!(multisig_details.has_owner(&who), Error::<T>::UnAuthorizedOwner);

			let multisig_proposal = PendingProposals::<T>::get(&multisig_account, &call_hash)
				.ok_or(Error::<T>::ProposalNotFound)?;
			let mut approvers = multisig_proposal.approvers;

			ensure!(!approvers.contains(&who), Error::<T>::AlreadyApproved);

			approvers.try_insert(who.clone()).map_err(|_| Error::<T>::TooManySignatories)?;

			PendingProposals::<T>::insert(
				&multisig_account,
				&call_hash,
				MultisigProposal { approvers, ..multisig_proposal },
			);

			Self::deposit_event(Event::ApprovedProposal {
				approving_account: who,
				multisig_account,
				call_hash,
			});
			Ok(())
		}

		/// Revokes an existing approval for a proposal for a multisig account.
		/// The caller must be one of the owners of the multisig account.
		///
		/// # Arguments
		///
		/// * `multisig_account` - The multisig account ID.
		/// * `call_hash` - The hash of the call to be approved. (This will be the hash of the call that was used in `start_proposal`)
		///
		/// # Errors
		///
		/// * `MultisigNotFound` - The multisig account does not exist.
		/// * `UnAuthorizedOwner` - The caller is not an owner of the multisig account.
		/// * `OwnerNotFound` - The caller has not approved the proposal.
		#[pallet::call_index(3)]
		#[pallet::weight(Weight::default())]
		pub fn revoke(
			origin: OriginFor<T>,
			multisig_account: T::AccountId,
			call_hash: T::Hash,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let multisig_details =
				MultisigAccount::<T>::get(&multisig_account).ok_or(Error::<T>::MultisigNotFound)?;

			ensure!(multisig_details.has_owner(&who), Error::<T>::UnAuthorizedOwner);

			let multisig_proposal = PendingProposals::<T>::get(&multisig_account, &call_hash)
				.ok_or(Error::<T>::ProposalNotFound)?;
			let mut approvers = multisig_proposal.approvers;
			ensure!(approvers.contains(&who), Error::<T>::OwnerNotFound);

			approvers.remove(&who);

			PendingProposals::<T>::insert(
				&multisig_account,
				&call_hash,
				MultisigProposal { approvers, ..multisig_proposal },
			);

			Self::deposit_event(Event::RevokedApproval {
				revoking_account: who,
				multisig_account,
				call_hash,
			});

			Ok(())
		}

		/// Executes a proposal for a dispatchable call for a multisig account.
		/// Poropsal needs to be approved by enough owners (exceeding multisig threshold) before it can be executed.
		/// The caller must be one of the owners of the multisig account.
		///
		/// This function does an extra check to make sure that all approvers still exist in the multisig account.
		/// That is to make sure that the multisig account is not compromised by removing an owner during an active proposal.
		///
		/// # Arguments
		///
		/// * `multisig_account` - The multisig account ID.
		/// * `call` - The call to be executed.
		///
		/// # Errors
		///
		/// * `MultisigNotFound` - The multisig account does not exist.
		/// * `UnAuthorizedOwner` - The caller is not an owner of the multisig account.
		/// * `NotEnoughApprovers` - approvers don't exceed the threshold.

		#[pallet::call_index(4)]
		#[pallet::weight(call.get_dispatch_info().weight)]
		pub fn execute_proposal(
			origin: OriginFor<T>,
			multisig_account: T::AccountId,
			call: Box<<T as Config>::RuntimeCall>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let multisig_details =
				MultisigAccount::<T>::get(&multisig_account).ok_or(Error::<T>::MultisigNotFound)?;

			ensure!(multisig_details.has_owner(&who), Error::<T>::UnAuthorizedOwner);

			let call_hash = Self::hash_of(&call);
			let multisig_proposal = PendingProposals::<T>::get(&multisig_account, &call_hash)
				.ok_or(Error::<T>::ProposalNotFound)?;

			ensure!(
				multisig_proposal.approvers.len() as u32 >= multisig_details.threshold,
				Error::<T>::NotEnoughApprovers
			);

			Self::check_approvers_still_exist(
				&multisig_account,
				&multisig_details,
				&multisig_proposal,
				&call_hash,
			)?;

			// Remove the proposal from the state.
			PendingProposals::<T>::remove(&multisig_account, &call_hash);

			// Make the call the last thing to prevent any possible re-entrancy attacks
			let result = call
				.dispatch(RawOrigin::Signed(multisig_account.clone()).into())
				.map(|_| ())
				.map_err(|e| e.error);

			Self::return_proposal_deposit(&multisig_proposal)?;

			Self::deposit_event(Event::ExecutedProposal {
				executor: who,
				multisig_account,
				call_hash,
				result,
			});

			result
		}

		/// Cancels an existing proposal for a multisig account Only if the proposal doesn't have approvers other than
		/// the proposer.
		///
		///	This function needs to be called from a the proposer of the proposal as the origin.
		///
		/// # Arguments
		///
		/// * `multisig_account` - The multisig account ID.
		/// * `call_hash` - The hash of the call to be canceled. (This will be the hash of the call that was used in `start_proposal`)
		///
		/// # Errors
		///
		/// * `MultisigNotFound` - The multisig account does not exist.
		/// * `ProposalNotFound` - The proposal does not exist.
		#[pallet::call_index(5)]
		#[pallet::weight(Weight::default())]
		pub fn cancel_own_proposal(
			origin: OriginFor<T>,
			multisig_account: T::AccountId,
			call_hash: T::Hash,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			ensure!(
				MultisigAccount::<T>::contains_key(&multisig_account),
				Error::<T>::MultisigNotFound
			);
			let multisig_proposal = PendingProposals::<T>::get(&multisig_account, &call_hash)
				.ok_or(Error::<T>::ProposalNotFound)?;

			ensure!(
				multisig_proposal.approvers.len() == 1
					&& multisig_proposal.approvers.contains(&who),
				Error::<T>::UnAuthorizedOwner
			);

			PendingProposals::<T>::remove(&multisig_account, &call_hash);
			Self::return_proposal_deposit(&multisig_proposal)?;
			Self::deposit_event(Event::CanceledProposal { multisig_account, call_hash });
			Ok(())
		}

		/// Remove up to `max` stale proposals for a deleted multisig account.
		///
		/// May be called by any Signed origin, but only after the multisig account is deleted.
		#[pallet::call_index(25)]
		#[pallet::weight(Weight::default())]
		pub fn cleanup_proposals(
			origin: OriginFor<T>,
			multisig_account: T::AccountId,
		) -> DispatchResultWithPostInfo {
			ensure_signed(origin)?;
			ensure!(
				!MultisigAccount::<T>::contains_key(&multisig_account),
				Error::<T>::MultisigStillExists
			);
			let maybe_cursor = ProposalsClearCursor::<T>::get(&multisig_account);
			let r = PendingProposals::<T>::clear_prefix(
				&multisig_account,
				T::RemoveProposalsLimit::get().into(), // RemoveProposalLimit is u8, no worries from a big loop.
				maybe_cursor.as_ref().map(|x| &x[..]),
			);
			if let Some(cursor) = r.maybe_cursor {
				ProposalsClearCursor::<T>::insert(
					&multisig_account,
					BoundedVec::truncate_from(cursor),
				);
			} else {
				// Clear the cursor if we're done.
				ProposalsClearCursor::<T>::remove(&multisig_account);
				Self::deposit_event(Event::PendingProposalsCleared { multisig_account });
			}
			Ok(if r.loops == 0 { Pays::Yes } else { Pays::No }.into())
		}

		//==============================================================================================================
		// Note: All the functions below needs to be called with a multisig account as the origin.
		// Starting code index from 20 to make it easier to group them and to add extrinsics before them in the future.
		//==============================================================================================================

		/// Adds a new owner to the multisig account.
		/// This function needs to be called from a Multisig account as the origin.
		/// Otherwise it will fail with MultisigNotFound error.
		///
		/// # Arguments
		///
		/// * `origin` - The origin multisig account who wants to add a new owner to the multisig account.
		/// * `new_owner` - The AccountId of the new owner to be added.
		/// * `new_threshold` - The new threshold for the multisig account after adding the new owner.
		///
		/// # Errors
		/// * `MultisigNotFound` - The multisig account does not exist.
		/// * `InvalidThreshold` - The threshold is greater than the total number of owners or is zero.
		/// * `TooManySignatories` - The number of signatories exceeds the maximum allowed.
		#[pallet::weight(Weight::default())]
		#[pallet::call_index(20)]
		pub fn add_owner(
			origin: OriginFor<T>,
			new_owner: T::AccountId,
			new_threshold: u32,
		) -> DispatchResult {
			let multisig_account = ensure_signed(origin)?;
			let multisig_details =
				MultisigAccount::<T>::get(&multisig_account).ok_or(Error::<T>::MultisigNotFound)?;
			let mut owners = multisig_details.owners;
			// If owner already exists in error.
			ensure!(!owners.contains(&new_owner), Error::<T>::OwnerAlreadyExists);

			let owners_len = owners.len() as u32;
			ensure!(new_threshold > 0, Error::<T>::InvalidThreshold);
			// Fail early if the threshold is greater than the total number of owners after adding the new owner.
			let owners_after_addition = owners_len
				.checked_add(1)
				.ok_or(DispatchError::Arithmetic(sp_runtime::ArithmeticError::Overflow))?;
			ensure!(new_threshold <= owners_after_addition, Error::<T>::InvalidThreshold);

			owners.try_insert(new_owner.clone()).map_err(|_| Error::<T>::TooManyOwners)?;

			let multisig_details: MultisigAccountDetails<T> =
				MultisigAccountDetails { owners, threshold: new_threshold, ..multisig_details };

			MultisigAccount::<T>::insert(&multisig_account, multisig_details);

			Self::deposit_event(Event::AddedOwner {
				multisig_account,
				added_owner: new_owner,
				threshold: new_threshold,
			});
			Ok(())
		}

		/// Removes an  owner from the multisig account.
		/// This function needs to be called from a Multisig account as the origin.
		/// Otherwise it will fail with MultisigNotFound error.
		/// If only one owner exists and is removed, the multisig account and any pending proposals for this account will be deleted from the state.
		///
		/// # Arguments
		///
		/// * `origin` - The origin multisig account who wants to remove an owner from the multisig account.
		/// * `owner_to_remove` - The AccountId of the owner to be removed.
		/// * `new_threshold` - The new threshold for the multisig account after removing the owner. Accepts zero if
		/// the owner is the only one left.kkk
		///
		/// # Errors
		///
		/// This function can return the following errors:
		///
		/// * `MultisigNotFound` - The multisig account does not exist.
		/// * `InvalidThreshold` - The new threshold is greater than the total number of owners or is zero.
		/// * `UnAuthorizedOwner` - The caller is not an owner of the multisig account.
		///
		#[pallet::call_index(21)]
		#[pallet::weight(Weight::default())]
		pub fn remove_owner(
			origin: OriginFor<T>,
			owner_to_remove: T::AccountId,
			new_threshold: u32,
		) -> DispatchResult {
			let multisig_account = ensure_signed(origin)?;
			let multisig_details =
				MultisigAccount::<T>::get(&multisig_account).ok_or(Error::<T>::MultisigNotFound)?;
			let mut owners = multisig_details.owners;
			let owners_len = owners.len() as u32;

			// Check Threshold first to fail early without a lookup for the owner
			// In case it's the only owner left, the threshold should be zero. Otherwise, it should be less than the
			// number of owners but never zero.
			if owners_len == 1 {
				ensure!(new_threshold == 0, Error::<T>::InvalidThreshold);
			} else {
				ensure!(new_threshold > 0, Error::<T>::InvalidThreshold);
			}
			// less than owners_len and not equal because we're removing one owner.
			ensure!(new_threshold < owners_len, Error::<T>::InvalidThreshold);

			ensure!(owners.contains(&owner_to_remove), Error::<T>::OwnerNotFound);
			owners.remove(&owner_to_remove);

			// Last owner was removed, remove the multisig account.
			if owners.len() == 0 {
				Self::purge_account(&multisig_account)?;
			} else {
				// Can't reach here with a zero threshold.
				let multisig_details: MultisigAccountDetails<T> =
					MultisigAccountDetails { owners, threshold: new_threshold, ..multisig_details };
				MultisigAccount::<T>::insert(&multisig_account, multisig_details);
			}

			Self::deposit_event(Event::RemovedOwner {
				multisig_account,
				removed_owner: owner_to_remove,
				threshold: new_threshold,
			});

			Ok(())
		}

		/// Sets a new threshold for a multisig account.
		///	This function needs to be called from a Multisig account as the origin.
		/// Otherwise it will fail with MultisigNotFound error.
		///
		/// # Arguments
		///
		/// * `origin` - The origin multisig account who wants to set the new threshold.
		/// * `new_threshold` - The new threshold to be set.
		/// # Errors
		///
		/// * `MultisigNotFound` - The multisig account does not exist.
		/// * `InvalidThreshold` - The new threshold is greater than the total number of owners or is zero.
		#[pallet::call_index(22)]
		#[pallet::weight(Weight::default())]
		pub fn set_threshold(origin: OriginFor<T>, new_threshold: u32) -> DispatchResult {
			let multisig_account = ensure_signed(origin)?;
			let multisig_details =
				MultisigAccount::<T>::get(&multisig_account).ok_or(Error::<T>::MultisigNotFound)?;
			let owners_len = multisig_details.owners.len() as u32;
			ensure!(new_threshold > 0 && new_threshold <= owners_len, Error::<T>::InvalidThreshold);

			MultisigAccount::<T>::insert(
				&multisig_account,
				MultisigAccountDetails {
					owners: multisig_details.owners,
					threshold: new_threshold,
					..multisig_details
				},
			);

			Self::deposit_event(Event::ChangedThreshold { multisig_account, new_threshold });
			Ok(())
		}

		/// Cancels an existing proposal for a multisig account.
		///
		///	This function needs to be called from a Multisig account as the origin.
		/// Otherwise it will fail with MultisigNotFound error.
		///
		/// # Arguments
		///
		/// * `origin` - The origin multisig account who wants to cancel the proposal.
		/// * `call_hash` - The hash of the call to be canceled. (This will be the hash of the call that was used in `start_proposal`)
		///
		/// # Errors
		///
		/// * `MultisigNotFound` - The multisig account does not exist.
		/// * `ProposalNotFound` - The proposal does not exist.
		#[pallet::call_index(23)]
		#[pallet::weight(Weight::default())]
		pub fn cancel_proposal(origin: OriginFor<T>, call_hash: T::Hash) -> DispatchResult {
			let multisig_account = ensure_signed(origin)?;
			ensure!(
				MultisigAccount::<T>::contains_key(&multisig_account),
				Error::<T>::MultisigNotFound
			);

			let proposal = PendingProposals::<T>::take(&multisig_account, &call_hash)
				.ok_or(Error::<T>::ProposalNotFound)?;

			Self::return_proposal_deposit(&proposal)?;
			
			Self::deposit_event(Event::CanceledProposal { multisig_account, call_hash });
			Ok(())
		}

		/// Deletes a multisig account and all related proposals.
		///
		///	This function needs to be called from a Multisig account as the origin.
		/// Otherwise it will fail with MultisigNotFound error.
		///
		/// # Arguments
		///
		/// * `origin` - The origin multisig account who wants to cancel the proposal.
		///
		/// # Errors
		///
		/// * `MultisigNotFound` - The multisig account does not exist.
		#[pallet::call_index(24)]
		#[pallet::weight(Weight::default())]
		pub fn delete_account(origin: OriginFor<T>) -> DispatchResult {
			let multisig_account = ensure_signed(origin)?;
			Self::purge_account(&multisig_account)?;
			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Derive a unique account id using the original account id and the timepoint.
	pub fn get_multisig_account_id(
		owners: &BoundedBTreeSet<T::AccountId, T::MaxSignatories>,
		timepoint: Timepoint<BlockNumberFor<T>>,
	) -> T::AccountId {
		let entropy = (b"pba/multisig_stateful", owners, timepoint).using_encoded(blake2_256);
		Decode::decode(&mut TrailingZeroInput::new(entropy.as_ref()))
			.expect("infinite length input; no invalid inputs for type; qed")
	}

	/// The current `Timepoint`.
	pub fn timepoint() -> Timepoint<BlockNumberFor<T>> {
		Timepoint {
			height: <frame_system::Pallet<T>>::block_number(),
			index: <frame_system::Pallet<T>>::extrinsic_index().unwrap_or_default(),
		}
	}

	pub fn hash_of(call: &<T as Config>::RuntimeCall) -> T::Hash {
		T::Hashing::hash_of(&call)
	}

	// Check if any approver has been removed from the multisig account.
	// We ensure that all approvers are still owners of the multisig account.
	// This is done by checking the intersection between the proposal approvers and the multisig owners.
	// If the number of final approvers after removing non-owners is still greater than the threshold, it is valid.
	// Otherwise, it is considered as an error.
	fn check_approvers_still_exist(
		multisig_account: &T::AccountId,
		multisig_details: &MultisigAccountDetails<T>,
		multisig_proposal: &MultisigProposal<T>,
		call_hash: &T::Hash,
	) -> DispatchResult {
		let current_approvers: BTreeSet<T::AccountId> = multisig_proposal
			.approvers
			.intersection(&multisig_details.owners)
			.cloned()
			.collect();

		ensure!(
			multisig_details.threshold <= current_approvers.len() as u32,
			Error::<T>::NotEnoughApprovers
		);

		// If the number of current approvers is equal to the number of approvers, then we don't need to update the
		// proposal.
		if current_approvers.len() == multisig_proposal.approvers.len() {
			return Ok(());
		}

		let updated_approvers_set: BoundedBTreeSet<T::AccountId, T::MaxSignatories> =
			current_approvers.try_into().map_err(|_| Error::<T>::TooManySignatories)?;

		let updated_proposal = MultisigProposal {
			approvers: updated_approvers_set, // Updated approvers
			// Rest are the same.
			creator: multisig_proposal.creator.clone(),
			..*multisig_proposal // when: multisig_proposal.when,
			                     // expire_after: multisig_proposal.expire_after,
			                     // creation_deposit: multisig_proposal.creation_deposit,
		};
		// Overwrites the proposal with the updated approvers.
		PendingProposals::<T>::insert(multisig_account, call_hash, updated_proposal);
		Ok(())
	}

	/// Deletes a multisig account
	fn purge_account(multisig_account: &T::AccountId) -> DispatchResult {
		let multisig_details =
			MultisigAccount::<T>::get(&multisig_account).ok_or(Error::<T>::MultisigNotFound)?;
		// Remove the multisig account.
		MultisigAccount::<T>::remove(&multisig_account);

		Self::return_multisig_creation_deposit(&multisig_details)?;

		Self::deposit_event(Event::DeletedMultisig { multisig_account: multisig_account.clone() });
		Ok(())
	}

	fn return_proposal_deposit(proposal: &MultisigProposal<T>) -> DispatchResult {
		T::Currency::release(
			&&HoldReason::ProposalCreation.into(),
			&proposal.creator,
			proposal.creation_deposit,
			Precision::BestEffort,
		)?;
		Ok(())
	}

	fn return_multisig_creation_deposit(
		multisig_details: &MultisigAccountDetails<T>,
	) -> DispatchResult {
		T::Currency::release(
			&&HoldReason::MultisigCreation.into(),
			&multisig_details.creator,
			multisig_details.deposit,
			Precision::BestEffort,
		)?;
		Ok(())
	}
}
