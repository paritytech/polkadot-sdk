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
use frame_support::{defensive, storage_alias, traits::UncheckedOnRuntimeUpgrade};

const LOG_TARGET: &str = "runtime::treasury";

/// Storage as it existed at pallet storage version 0, before [`crate::migration::MigrateV0ToV1`].
///
/// These aliases deliberately preserve the **on-chain storage keys** of the old pallet storage
/// declarations that have been removed from `lib.rs`.
pub mod v0 {
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

	/// Proposal indices that have been approved but not yet awarded.
	///
	/// `MaxApprovals` is the same getter the runtime used to pass as
	/// `pallet_treasury::Config::MaxApprovals`. It bounds the decoded queue and is not part of the
	/// storage key.
	#[allow(invalid_type_param_default)]
	#[storage_alias]
	pub type Approvals<T: Config<I>, I: 'static, MaxApprovals: Get<u32> + 'static> =
		StorageValue<Pallet<T, I>, BoundedVec<ProposalIndex, MaxApprovals>, ValueQuery>;
}

/// Invariants of the v0 proposal storage.
///
/// Called once, from the storage v0-to-v1 migration's `pre_upgrade` hook, when storage moves from
/// version 0 to 1. It is not part of the pallet's per-block `try_state` hook.
///
/// ### Invariants
/// 1. [`v0::ProposalCount`] >= number of entries in [`v0::Proposals`].
/// 2. Every key in [`v0::Proposals`] is strictly less than [`v0::ProposalCount`].
/// 3. Every index in [`v0::Approvals`] exists as a key in [`v0::Proposals`].
#[cfg(any(feature = "try-runtime", test))]
pub fn try_state_proposals<T: Config<I>, I: 'static, MaxApprovals: Get<u32> + 'static>(
) -> Result<(), sp_runtime::TryRuntimeError> {
	use frame_support::ensure;

	let current_proposal_count = v0::ProposalCount::<T, I>::get();
	ensure!(
		current_proposal_count as usize >= v0::Proposals::<T, I>::iter().count(),
		"Actual number of proposals exceeds `ProposalCount`."
	);

	v0::Proposals::<T, I>::iter_keys().try_for_each(
		|proposal_index| -> Result<(), sp_runtime::TryRuntimeError> {
			ensure!(
				(current_proposal_count as u32) > proposal_index,
				"`ProposalCount` should be strictly greater than any ProposalIndex used as a key \
				 for `Proposals`."
			);
			Ok(())
		},
	)?;

	v0::Approvals::<T, I, MaxApprovals>::get().iter().try_for_each(
		|proposal_index| -> Result<(), sp_runtime::TryRuntimeError> {
			ensure!(
				v0::Proposals::<T, I>::contains_key(proposal_index),
				"Proposal indices in `Approvals` must also be contained in `Proposals`."
			);
			Ok(())
		},
	)?;

	Ok(())
}

mod v1 {
	use super::*;

	/// Pays out and removes every remaining v0 treasury proposal, then deletes that storage.
	///
	/// Wrapped by [`super::MigrateV0ToV1`], which runs this only when the on-chain storage
	/// version is 0 and then writes version 1.
	///
	/// This does the same work [`Pallet::spend_funds`] used to do for the legacy queue, but once
	/// at upgrade time instead of every spend period:
	/// - Proposals listed in [`v0::Approvals`] are paid from the pot, their bond is unreserved, and
	///   an [`Event::Awarded`] is emitted.
	/// - Unapproved proposals only get their bond refunded; their spend was never authorised.
	///
	/// If the pot cannot cover an approved payout, that proposal is left in place and its approval
	/// is kept. The migration logs a warning naming each deferred index and amount. Because the
	/// storage version is then 1, a later upgrade does not retry those entries.
	///
	/// **Warning:** before enacting the upgrade, a chain whose pot cannot cover an approved
	/// proposal must pay it out manually, fund the pot, or remove the approval. After this
	/// migration has run, deferred entries are orphaned.
	///
	/// # Weight
	/// One read per legacy proposal visited, plus fixed reads for the pot, `Approvals` and
	/// settlement. Each processed proposal also reads the proposer account, and each payout reads
	/// the beneficiary account. Up to three writes per proposal actually processed, plus the
	/// treasury `settle` write when something was paid. One extra write when pruning `Approvals`
	/// for deferred payouts, two when deleting all legacy storage.
	///
	/// # Defensive
	/// A non-zero `unreserve` remainder (bond partially slashed, or the proposer reaped since the
	/// proposal was created) emits a `defensive!` and the migration continues; the stranded amount
	/// cannot be recovered automatically.
	pub struct UncheckedMigrateToV1<T, I, MaxApprovals>(PhantomData<(T, I, MaxApprovals)>);

	impl<T: Config<I>, I: 'static, MaxApprovals: Get<u32> + 'static> UncheckedOnRuntimeUpgrade
		for UncheckedMigrateToV1<T, I, MaxApprovals>
	{
		fn on_runtime_upgrade() -> Weight {
			let approved: BTreeSet<ProposalIndex> =
				v0::Approvals::<T, I, MaxApprovals>::get().into_iter().collect();

			let mut budget_remaining = Pallet::<T, I>::pot();
			let mut imbalance = PositiveImbalanceOf::<T, I>::zero();
			let mut iterations: u64 = 0;
			let mut processed: u64 = 0;
			let mut paid: u64 = 0;
			let mut deferred = false;
			let mut deferred_payouts: alloc::vec::Vec<(ProposalIndex, BalanceOf<T, I>)> =
				alloc::vec::Vec::new();

			for (proposal_index, proposal) in v0::Proposals::<T, I>::iter() {
				iterations = iterations.saturating_add(1);
				if approved.contains(&proposal_index) {
					if proposal.value > budget_remaining {
						// The pot cannot cover this payout. The version bump means it will not be
						// retried, so the chain must resolve it before enacting the upgrade.
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

				v0::Proposals::<T, I>::remove(proposal_index);
				processed = processed.saturating_add(1);
			}

			if deferred {
				for (proposal_index, amount) in deferred_payouts {
					log::warn!(
						target: LOG_TARGET,
						"deferred legacy treasury payout: proposal {proposal_index} requires {amount:?} \
						 but the pot is insufficient; fund the pot, pay it out manually or remove the \
						 approval before enacting this upgrade",
					);
				}
				v0::Approvals::<T, I, MaxApprovals>::mutate(|approvals| {
					approvals.retain(|index| v0::Proposals::<T, I>::contains_key(index))
				});
			} else {
				v0::Approvals::<T, I, MaxApprovals>::kill();
				v0::ProposalCount::<T, I>::kill();
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
				"MigrateV0ToV1: removed {} proposals, paid out {}. Legacy storage {}.",
				processed,
				paid,
				if deferred { "kept, some payouts exceed the pot" } else { "deleted" },
			);

			// One read per proposal visited, plus pot, `Approvals` and settlement. Each processed
			// proposal reads the proposer account, and each payout reads the beneficiary account.
			// Up to three writes per processed proposal, plus the treasury `settle` write when
			// something was paid. One extra write when pruning `Approvals` for deferred payouts,
			// two when deleting all legacy storage.
			let fixed_reads = if deferred { 4 } else { 3 };
			let fixed_writes = if deferred { 1 } else { 2 };
			let settle_write = u64::from(paid > 0);
			T::DbWeight::get().reads_writes(
				iterations
					.saturating_add(fixed_reads)
					.saturating_add(processed)
					.saturating_add(paid),
				processed
					.saturating_mul(3)
					.saturating_add(fixed_writes)
					.saturating_add(settle_write),
			)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			super::try_state_proposals::<T, I, MaxApprovals>()?;

			let proposals_count = v0::Proposals::<T, I>::iter_values().count() as u32;
			let approvals_count = v0::Approvals::<T, I, MaxApprovals>::get().len() as u32;

			log::info!(
				target: LOG_TARGET,
				"pre_upgrade MigrateV0ToV1: proposals={}, approvals={}",
				proposals_count,
				approvals_count,
			);

			Ok((proposals_count, approvals_count).encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			let (old_proposals, old_approvals) =
				<(u32, u32)>::decode(&mut &state[..]).expect("Known good");

			let remaining = v0::Proposals::<T, I>::iter().count() as u32;
			ensure!(
				remaining <= old_proposals,
				"post_upgrade: legacy Proposals grew during the migration"
			);

			// Whatever survived must be an approved payout the pot could not cover; everything
			// else has to be gone.
			let approvals = v0::Approvals::<T, I, MaxApprovals>::get();
			for (index, _) in v0::Proposals::<T, I>::iter() {
				ensure!(
					approvals.contains(&index),
					"post_upgrade: an unapproved legacy proposal survived the migration"
				);
			}

			log::info!(
				target: LOG_TARGET,
				"post_upgrade MigrateV0ToV1: {} of {} proposals removed \
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

/// Migrate treasury storage from version 0 to 1 by paying out and deleting legacy proposals.
///
/// `MaxApprovals` bounds the decoded v0 approvals queue. Pass the same getter the runtime used for
/// `Config::MaxApprovals` before that constant moved to the bounties pallet.
pub type MigrateV0ToV1<T, I, MaxApprovals> = frame_support::migrations::VersionedMigration<
	0,
	1,
	v1::UncheckedMigrateToV1<T, I, MaxApprovals>,
	Pallet<T, I>,
	<T as frame_system::Config>::DbWeight,
>;
