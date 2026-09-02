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

//! Treasury pallet migrations.

use super::*;
use alloc::collections::BTreeSet;
#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;
use core::marker::PhantomData;
use frame_support::{defensive, storage_alias, traits::OnRuntimeUpgrade};

const LOG_TARGET: &str = "runtime::treasury";

/// These aliases deliberately preserve the **on-chain storage keys** of the old pallet storage
/// declarations that have been removed from `lib.rs`. They must only be used by the migrations
/// below and by `try_state_proposals` (which also lives here after the main module no longer
/// declares these items).
pub mod legacy {
	use super::*;
	use frame_support::pallet_prelude::*;

	/// A spending proposal identical to the removed `pallet::Proposal` struct.
	///
	/// Re-declared here so that `#[storage_alias]` can decode historic on-chain data without
	/// importing a type that no longer exists in the pallet's public API.
	#[derive(
		Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, MaxEncodedLen, Debug, TypeInfo,
	)]
	pub struct Proposal<AccountId, Balance> {
		/// The account that originally proposed this spend.
		pub proposer: AccountId,
		/// The amount to be transferred from the treasury to `beneficiary`.
		pub value: Balance,
		/// The destination account for the transfer.
		pub beneficiary: AccountId,
		/// The amount held on deposit (reserved) from `proposer`.
		pub bond: Balance,
	}

	/// Number of proposals that have been made (legacy counter).
	#[allow(invalid_type_param_default)]
	#[storage_alias]
	pub type ProposalCount<T: Config<I>, I: 'static> =
		StorageValue<Pallet<T, I>, ProposalIndex, ValueQuery>;

	/// Proposals that have been made (legacy map, keyed by [`ProposalIndex`]).
	#[allow(invalid_type_param_default)]
	#[storage_alias]
	pub type Proposals<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Twox64Concat,
		ProposalIndex,
		Proposal<<T as frame_system::Config>::AccountId, BalanceOf<T, I>>,
		OptionQuery,
	>;

	/// Proposal indices that have been approved but not yet awarded (legacy queue).
	#[allow(invalid_type_param_default)]
	#[storage_alias]
	pub type Approvals<T: Config<I>, I: 'static> = StorageValue<
		Pallet<T, I>,
		BoundedVec<ProposalIndex, <T as Config<I>>::MaxApprovals>,
		ValueQuery,
	>;
}

/// Called from the pallet's try-runtime hook so that try-runtime and tests can still verify
/// the consistency of any remaining on-chain legacy state before the migration fires.
///
/// ### Invariants
/// 1. [`legacy::ProposalCount`] >= number of entries in [`legacy::Proposals`].
/// 2. Every key in [`legacy::Proposals`] is strictly less than [`legacy::ProposalCount`].
/// 3. Every index in [`legacy::Approvals`] exists as a key in [`legacy::Proposals`].
#[cfg(any(feature = "try-runtime", test))]
pub fn try_state_proposals<T: Config<I>, I: 'static>() -> Result<(), sp_runtime::TryRuntimeError> {
	use frame_support::ensure;

	let current_proposal_count = legacy::ProposalCount::<T, I>::get();
	ensure!(
		current_proposal_count as usize >= legacy::Proposals::<T, I>::iter().count(),
		"Actual number of proposals exceeds `ProposalCount`."
	);

	legacy::Proposals::<T, I>::iter_keys().try_for_each(
		|proposal_index| -> Result<(), sp_runtime::TryRuntimeError> {
			ensure!(
				(current_proposal_count as u32) > proposal_index,
				"`ProposalCount` should be strictly greater than any ProposalIndex used as a key \
				 for `Proposals`."
			);
			Ok(())
		},
	)?;

	legacy::Approvals::<T, I>::get().iter().try_for_each(
		|proposal_index| -> Result<(), sp_runtime::TryRuntimeError> {
			ensure!(
				legacy::Proposals::<T, I>::contains_key(proposal_index),
				"Proposal indices in `Approvals` must also be contained in `Proposals`."
			);
			Ok(())
		},
	)?;

	Ok(())
}

pub mod cleanup_proposals {
	use super::*;

	/// Migration to cleanup unapproved proposals to return the bonds back to the proposers.
	/// Proposals can no longer be created and the `Proposal` storage item will be removed in the
	/// future.
	///
	/// `UnreserveWeight` returns `Weight` of `unreserve_balance` operation which is performed
	/// during this migration.
	pub struct Migration<T, I, UnreserveWeight>(PhantomData<(T, I, UnreserveWeight)>);

	impl<T: Config<I>, I: 'static, UnreserveWeight: Get<Weight>> OnRuntimeUpgrade
		for Migration<T, I, UnreserveWeight>
	{
		fn on_runtime_upgrade() -> frame_support::weights::Weight {
			let mut approval_index = BTreeSet::new();
			for approval in legacy::Approvals::<T, I>::get().iter() {
				approval_index.insert(*approval);
			}

			let mut proposals_processed = 0;
			for (proposal_index, p) in legacy::Proposals::<T, I>::iter() {
				if !approval_index.contains(&proposal_index) {
					let err_amount = T::Currency::unreserve(&p.proposer, p.bond);
					if err_amount.is_zero() {
						legacy::Proposals::<T, I>::remove(proposal_index);
						log::info!(
							target: LOG_TARGET,
							"Released bond amount of {:?} to proposer {:?}",
							p.bond,
							p.proposer,
						);
					} else {
						defensive!(
							"err_amount is non zero for proposal",
							(proposal_index, err_amount),
						);
						legacy::Proposals::<T, I>::mutate_extant(proposal_index, |proposal| {
							proposal.value = err_amount;
						});
						log::info!(
							target: LOG_TARGET,
							"Released partial bond amount of {:?} to proposer {:?}",
							p.bond - err_amount,
							p.proposer,
						);
					}
					proposals_processed += 1;
				}
			}

			log::info!(
				target: LOG_TARGET,
				"Migration for pallet-treasury finished, released {} proposal bonds.",
				proposals_processed,
			);

			// calculate and return migration weights
			let approvals_read = 1;
			T::DbWeight::get().reads_writes(
				proposals_processed as u64 + approvals_read,
				proposals_processed as u64,
			) + UnreserveWeight::get() * proposals_processed
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			let value = (
				legacy::Proposals::<T, I>::iter_values().count() as u32,
				legacy::Approvals::<T, I>::get().len() as u32,
			);
			log::info!(
				target: LOG_TARGET,
				"Proposals and Approvals count {:?}",
				value,
			);
			Ok(value.encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			let (old_proposals_count, old_approvals_count) =
				<(u32, u32)>::decode(&mut &state[..]).expect("Known good");
			let new_proposals_count = legacy::Proposals::<T, I>::iter_values().count() as u32;
			let new_approvals_count = legacy::Approvals::<T, I>::get().len() as u32;

			log::info!(
				target: LOG_TARGET,
				"Proposals and Approvals count {:?}",
				(new_proposals_count, new_approvals_count),
			);

			ensure!(
				new_proposals_count <= old_proposals_count,
				"Proposals after migration should be less or equal to old proposals"
			);
			ensure!(
				new_approvals_count == old_approvals_count,
				"Approvals after migration should remain the same"
			);
			Ok(())
		}
	}
}

pub mod migrate_legacy_proposals {
	use super::*;

	/// Pays out and removes every remaining legacy treasury proposal, then deletes the legacy
	/// storage.
	///
	/// This does the same work [`Pallet::spend_funds`] used to do for the legacy queue, but once
	/// at upgrade time instead of every spend period:
	/// - Proposals listed in [`legacy::Approvals`] are paid from the pot, their bond is unreserved,
	///   and an [`Event::Awarded`] is emitted.
	/// - Unapproved proposals only get their bond refunded; their spend was never authorised.
	///
	/// If the pot cannot cover an approved payout, that proposal is left in place and its approval
	/// is kept. The migration logs a warning naming each deferred index and amount.
	///
	/// **Warning:** once this migration is removed from a runtime's `Migrations` tuple, any
	/// deferred entries are orphaned: `spend_local`, `remove_approval`, the `spend_funds` drain
	/// loop and this migration are all gone, so there is no remaining code path to pay them out.
	/// Before enacting the upgrade, a chain whose pot cannot cover an approved proposal must pay
	/// it out manually, fund the pot, or remove the approval.
	///
	/// # Weight
	/// One read per legacy proposal visited, plus fixed reads for the pot, `Approvals` and
	/// settlement. Up to three reads and three writes per proposal actually processed.
	///
	/// # Defensive
	/// A non-zero `unreserve` remainder (bond partially slashed, or the proposer reaped since the
	/// proposal was created) emits a `defensive!` and the migration continues; the stranded amount
	/// cannot be recovered automatically.
	pub struct Migration<T, I = ()>(PhantomData<(T, I)>);

	impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
		fn on_runtime_upgrade() -> Weight {
			let approved: BTreeSet<ProposalIndex> =
				legacy::Approvals::<T, I>::get().into_iter().collect();

			let mut budget_remaining = Pallet::<T, I>::pot();
			let mut imbalance = PositiveImbalanceOf::<T, I>::zero();
			let mut iterations: u64 = 0;
			let mut processed: u64 = 0;
			let mut paid: u64 = 0;
			let mut deferred = false;
			let mut deferred_payouts: alloc::vec::Vec<(ProposalIndex, BalanceOf<T, I>)> =
				alloc::vec::Vec::new();

			for (proposal_index, proposal) in legacy::Proposals::<T, I>::iter() {
				iterations = iterations.saturating_add(1);
				if approved.contains(&proposal_index) {
					if proposal.value > budget_remaining {
						// The pot cannot cover this payout, so leave it for manual resolution
						// before this migration is removed from the runtime's `Migrations` tuple.
						deferred = true;
						deferred_payouts.push((proposal_index, proposal.value));
						continue;
					}
					budget_remaining -= proposal.value;
					imbalance.subsume(T::Currency::deposit_creating(
						&proposal.beneficiary,
						proposal.value,
					));
					Pallet::<T, I>::deposit_event(Event::Awarded {
						proposal_index,
						award: proposal.value,
						account: proposal.beneficiary.clone(),
					});
					paid = paid.saturating_add(1);
				}

				let remainder = T::Currency::unreserve(&proposal.proposer, proposal.bond);
				if !remainder.is_zero() {
					defensive!(
						"legacy treasury proposal bond not fully unreserved",
						(proposal_index, remainder),
					);
				}

				legacy::Proposals::<T, I>::remove(proposal_index);
				processed = processed.saturating_add(1);
			}

			if deferred {
				for (proposal_index, amount) in deferred_payouts {
					log::warn!(
						target: LOG_TARGET,
						"deferred legacy treasury payout: proposal {proposal_index} requires {amount:?} \
						 but the pot is insufficient; fund the pot, pay it out manually or remove the \
						 approval before this migration leaves the runtime Migrations tuple",
					);
				}
				legacy::Approvals::<T, I>::mutate(|approvals| {
					approvals.retain(|index| legacy::Proposals::<T, I>::contains_key(index))
				});
			} else {
				legacy::Approvals::<T, I>::kill();
				legacy::ProposalCount::<T, I>::kill();
			}

			// Balance the freshly created funds against the treasury account, as `spend_funds`
			// does. Skipping this would inflate total issuance by the amount paid out.
			if let Err(problem) = T::Currency::settle(
				&Pallet::<T, I>::account_id(),
				imbalance,
				WithdrawReasons::TRANSFER,
				KeepAlive,
			) {
				defensive!("treasury could not settle legacy proposal payouts");
				drop(problem);
			}

			log::info!(
				target: LOG_TARGET,
				"migrate_legacy_proposals: removed {} proposals, paid out {}. Legacy storage {}.",
				processed,
				paid,
				if deferred { "kept, some payouts exceed the pot" } else { "deleted" },
			);

			// One read per proposal visited, plus pot, `Approvals` and settlement. Up to three
			// reads and three writes per proposal actually processed; one extra write when pruning
			// `Approvals` for deferred payouts, two when deleting all legacy storage.
			let fixed_reads = if deferred { 4 } else { 3 };
			let fixed_writes = if deferred { 1 } else { 2 };
			T::DbWeight::get().reads_writes(
				iterations.saturating_add(fixed_reads),
				processed.saturating_mul(3).saturating_add(fixed_writes),
			)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			let proposals_count = legacy::Proposals::<T, I>::iter_values().count() as u32;
			let approvals_count = legacy::Approvals::<T, I>::get().len() as u32;

			log::info!(
				target: LOG_TARGET,
				"pre_upgrade migrate_legacy_proposals: proposals={}, approvals={}",
				proposals_count,
				approvals_count,
			);

			Ok((proposals_count, approvals_count).encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			let (old_proposals, old_approvals) =
				<(u32, u32)>::decode(&mut &state[..]).expect("Known good");

			let remaining = legacy::Proposals::<T, I>::iter().count() as u32;
			ensure!(
				remaining <= old_proposals,
				"post_upgrade: legacy Proposals grew during the migration"
			);

			// Whatever survived must be an approved payout the pot could not cover; everything
			// else has to be gone.
			let approvals = legacy::Approvals::<T, I>::get();
			for (index, _) in legacy::Proposals::<T, I>::iter() {
				ensure!(
					approvals.contains(&index),
					"post_upgrade: an unapproved legacy proposal survived the migration"
				);
			}

			log::info!(
				target: LOG_TARGET,
				"post_upgrade migrate_legacy_proposals: {} of {} proposals removed \
				 ({} approvals before, {} left unpaid).",
				old_proposals.saturating_sub(remaining),
				old_proposals,
				old_approvals,
				remaining,
			);

			Ok(())
		}
	}
}
