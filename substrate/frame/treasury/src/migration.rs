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
use core::marker::PhantomData;
use frame_support::{defensive, traits::OnRuntimeUpgrade};
use pallet_balances::WeightInfo;

/// The log target for this pallet.
const LOG_TARGET: &str = "runtime::treasury";

pub struct ReleaseHeldProposals<T, I>(PhantomData<(T, I)>);

impl<T: Config<I> + pallet_balances::Config, I: 'static> OnRuntimeUpgrade
	for ReleaseHeldProposals<T, I>
{
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let mut approval_index = BTreeSet::new();
		for approval in Approvals::<T, I>::get().iter() {
			approval_index.insert(*approval);
		}

		let mut proposals_released = 0;
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
				proposals_released += 1;
			}
		}

		log::info!(
			target: LOG_TARGET,
			"Migration for pallet-treasury finished, released {} proposal bonds.",
			proposals_released,
		);

		// calculate and return migration weights
		let approvals_read = 1;
		T::DbWeight::get()
			.reads_writes(proposals_released as u64 + approvals_read, proposals_released as u64) +
			<T as pallet_balances::Config>::WeightInfo::force_unreserve() * proposals_released
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		Ok((Proposals::<T, I>::iter_values().count() as u32, Approvals::<T, I>::get().len() as u32)
			.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		let (old_proposals_count, old_approvals_count) =
			<(u32, u32)>::decode(&mut &state[..]).expect("Known good");
		let new_proposals_count = Proposals::<T, I>::iter_values().count() as u32;
		let new_approvals_count = Approvals::<T, I>::get().len() as u32;

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
