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
use frame_support::{
	defensive,
	storage_alias,
	traits::OnRuntimeUpgrade,
};

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
		Encode,
		Decode,
		DecodeWithMemTracking,
		Clone,
		PartialEq,
		Eq,
		MaxEncodedLen,
		Debug,
		TypeInfo,
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
	pub type Approvals<T: Config<I>, I: 'static> =
		StorageValue<Pallet<T, I>, BoundedVec<ProposalIndex, <T as Config<I>>::MaxApprovals>, ValueQuery>;
}

/// Called from [`Pallet::try_state`] so that try-runtime and tests can still verify the
/// consistency of any remaining on-chain legacy state before the migration fires.
///
/// ### Invariants
/// 1. [`legacy::ProposalCount`] >= number of entries in [`legacy::Proposals`].
/// 2. Every key in [`legacy::Proposals`] is strictly less than [`legacy::ProposalCount`].
/// 3. Every index in [`legacy::Approvals`] exists as a key in [`legacy::Proposals`].
#[cfg(any(feature = "try-runtime", test))]
pub fn try_state_proposals<T: Config<I>, I: 'static>(
) -> Result<(), sp_runtime::TryRuntimeError> {
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

	legacy::Approvals::<T, I>::get()
		.iter()
		.try_for_each(|proposal_index| -> Result<(), sp_runtime::TryRuntimeError> {
			ensure!(
				legacy::Proposals::<T, I>::contains_key(proposal_index),
				"Proposal indices in `Approvals` must also be contained in `Proposals`."
			);
			Ok(())
		})?;

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


/// Converts a deprecated native-token legacy proposal into the typed fields needed for a
/// new [`SpendStatus`] entry.
///
/// The runtime **must** provide this implementation because only the runtime knows:
/// - the representation of its native asset kind (`T::AssetKind`);
/// - how a local `AccountId` maps to `T::Beneficiary`;
/// - whether native `BalanceOf<T,I>` maps 1:1 to `AssetBalanceOf<T,I>`.
///
/// # Example (simple native-token runtime)
/// ```ignore
/// pub struct NativeTreasuryConverter;
/// impl pallet_treasury::migration::LegacyProposalConverter<Runtime, ()>
///     for NativeTreasuryConverter
/// {
///     fn convert(
///         proposal: pallet_treasury::migration::legacy::Proposal<AccountId, Balance>,
///     ) -> (RuntimeAssetKind, Balance, AccountId) {
///         (RuntimeAssetKind::Native, proposal.value, proposal.beneficiary)
///     }
/// }
/// ```
pub trait LegacyProposalConverter<T: Config<I>, I: 'static> {
	/// Convert a legacy proposal into (asset_kind, amount, beneficiary) for a [`SpendStatus`].
	fn convert(
		proposal: legacy::Proposal<T::AccountId, BalanceOf<T, I>>,
	) -> (T::AssetKind, AssetBalanceOf<T, I>, T::Beneficiary);
}

pub mod migrate_legacy_proposals {
	use super::*;

	/// Migrates all remaining legacy treasury proposals and removes the legacy storage entirely.
	///
	/// For each entry in [`legacy::Proposals`]:
	/// - The proposer's bond is unreserved (for both approved and unapproved proposals).
	/// - If the proposal index is listed in [`legacy::Approvals`], it is converted into a
	///   [`SpendStatus`] entry in [`Spends`] with `status = Pending`, using the runtime-supplied
	///   [`LegacyProposalConverter`]. This preserves the intent of approved spends without
	///   forcing a potentially-unfunded payout at upgrade time.
	/// - Unapproved proposals are dropped after bond refund (their spend authority was never
	///   granted).
	///
	/// After draining all proposals, [`legacy::Approvals`] and [`legacy::ProposalCount`] are
	/// killed.
	///
	/// # Weight
	/// Per-proposal cost is `1 read + 1 write` for the proposal map entry, plus
	/// `UnreserveWeight` for the bond unreserve. Approved proposals additionally cost `1 write`
	/// for the new [`Spends`] entry and `1 write` for [`SpendCount`].
	///
	/// # Panics / defensive
	/// If `unreserve` returns a non-zero remainder (bond was partially slashed or the account
	/// was reaped since the proposal was created), a `defensive!` warning is emitted and the
	/// migration continues, the stranded amount cannot be recovered automatically.
	///
	pub struct Migration<T, I, Converter, UnreserveWeight>(
		PhantomData<(T, I, Converter, UnreserveWeight)>,
	);

	impl<T, I, Converter, UnreserveWeight> OnRuntimeUpgrade
		for Migration<T, I, Converter, UnreserveWeight>
	where
		T: Config<I>,
		I: 'static,
		Converter: LegacyProposalConverter<T, I>,
		UnreserveWeight: Get<Weight>,
	{
		fn on_runtime_upgrade() -> Weight {
			let approved: BTreeSet<ProposalIndex> =
				legacy::Approvals::<T, I>::get().into_iter().collect();

			let now = T::BlockNumberProvider::current_block_number();
			let expire_at = now.saturating_add(T::PayoutPeriod::get());

			let mut next_spend = SpendCount::<T, I>::get();
			let mut proposals_processed: u64 = 0;
			let mut spends_created: u64 = 0;

			for (proposal_index, proposal) in legacy::Proposals::<T, I>::drain() {
				proposals_processed = proposals_processed.saturating_add(1);

				let remainder = T::Currency::unreserve(&proposal.proposer, proposal.bond);
				if !remainder.is_zero() {
					defensive!(
						"legacy treasury proposal bond not fully unreserved",
						(proposal_index, remainder),
					);
				}

				if approved.contains(&proposal_index) {
					let (asset_kind, amount, beneficiary) = Converter::convert(proposal);

					let spend_index = match next_spend.checked_add(1) {
						Some(n) => {
							let idx = next_spend;
							next_spend = n;
							idx
						},
						None => {
							defensive!(
								"SpendIndex overflow during legacy proposal migration",
								proposal_index,
							);
							continue;
						},
					};

					Spends::<T, I>::insert(
						spend_index,
						SpendStatus {
							asset_kind,
							amount,
							beneficiary,
							valid_from: now,
							expire_at,
							status: PaymentState::Pending,
						},
					);
					spends_created = spends_created.saturating_add(1);
				}
			}

			legacy::Approvals::<T, I>::kill();
			legacy::ProposalCount::<T, I>::kill();

			if spends_created > 0 {
				SpendCount::<T, I>::put(next_spend);
			}

			log::info!(
				target: LOG_TARGET,
				"migrate_legacy_proposals: processed {} proposals, created {} spends.",
				proposals_processed,
				spends_created,
			);

			let reads = proposals_processed.saturating_add(3);
			let writes = proposals_processed
				.saturating_add(spends_created)
				.saturating_add(if spends_created > 0 { 3 } else { 2 });

			T::DbWeight::get().reads_writes(reads, writes)
				+ UnreserveWeight::get().saturating_mul(proposals_processed)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			let proposals_count = legacy::Proposals::<T, I>::iter_values().count() as u32;
			let approvals = legacy::Approvals::<T, I>::get();
			let approvals_count = approvals.len() as u32;
			let old_spend_count = SpendCount::<T, I>::get();

			let mut valid_approvals: u32 = 0;
			for idx in approvals.iter() {
				if legacy::Proposals::<T, I>::contains_key(idx) {
					valid_approvals = valid_approvals.saturating_add(1);
				} else {
					log::warn!(
						target: LOG_TARGET,
						"pre_upgrade: orphaned approval index {:?} has no matching proposal; \
						 it will be dropped without creating a Spend.",
						idx,
					);
				}
			}

			log::info!(
				target: LOG_TARGET,
				"pre_upgrade migrate_legacy_proposals: proposals={}, approvals={}, \
				 valid_approvals={}, spend_count={}",
				proposals_count,
				approvals_count,
				valid_approvals,
				old_spend_count,
			);

			Ok((proposals_count, valid_approvals, old_spend_count).encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			let (old_proposals, valid_approvals, old_spend_count) =
				<(u32, u32, SpendIndex)>::decode(&mut &state[..]).expect("Known good");

			ensure!(
				legacy::Proposals::<T, I>::iter().next().is_none(),
				"post_upgrade: legacy Proposals storage is not empty after migration"
			);
			ensure!(
				legacy::Approvals::<T, I>::get().is_empty(),
				"post_upgrade: legacy Approvals storage is not empty after migration"
			);

			let new_spend_count = SpendCount::<T, I>::get();
			ensure!(
				new_spend_count == old_spend_count.saturating_add(valid_approvals),
				"post_upgrade: SpendCount did not advance by the expected number of valid approvals"
			);

			log::info!(
				target: LOG_TARGET,
				"post_upgrade migrate_legacy_proposals: migrated {} proposals, created {} spends. \
				 SpendCount: {} -> {}",
				old_proposals,
				valid_approvals,
				old_spend_count,
				new_spend_count,
			);

			Ok(())
		}
	}
}
