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
use frame_support::{defensive, traits::OnRuntimeUpgrade};

/// The log target for this pallet.
const LOG_TARGET: &str = "runtime::treasury";

pub mod cleanup_proposals {
	use super::*;

	/// Migration to cleanup unapproved proposals to return the bonds back to the proposers.
	/// Proposals can no longer be created and the `Proposal` storage item will be removed in the
	/// future.
	///
	/// `UnreserveWeight` returns `Weight` of `unreserve_balance` operation which is perfomed during
	/// this migration.
	pub struct Migration<T, I, UnreserveWeight>(PhantomData<(T, I, UnreserveWeight)>);

	impl<T: Config<I>, I: 'static, UnreserveWeight: Get<Weight>> OnRuntimeUpgrade
		for Migration<T, I, UnreserveWeight>
	{
		fn on_runtime_upgrade() -> frame_support::weights::Weight {
			let mut approval_index = BTreeSet::new();
			for approval in Approvals::<T, I>::get().iter() {
				approval_index.insert(*approval);
			}

			let mut proposals_processed = 0;
			for (proposal_index, p) in Proposals::<T, I>::iter() {
				if !approval_index.contains(&proposal_index) {
					let err_amount = T::Currency::unreserve(&p.proposer, p.bond);
					if err_amount.is_zero() {
						Proposals::<T, I>::remove(proposal_index);
						log::info!(
							target: LOG_TARGET,
							"Released bond amount of {:?} to proposer {:?}",
							p.bond,
							p.proposer,
						);
					} else {
						defensive!(
							"err_amount is non zero for proposal {:?}",
							(proposal_index, err_amount)
						);
						Proposals::<T, I>::mutate_extant(proposal_index, |proposal| {
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
				Proposals::<T, I>::iter_values().count() as u32,
				Approvals::<T, I>::get().len() as u32,
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
			let new_proposals_count = Proposals::<T, I>::iter_values().count() as u32;
			let new_approvals_count = Approvals::<T, I>::get().len() as u32;

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
