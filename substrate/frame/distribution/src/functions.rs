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

//! Helper functions for Distribution pallet.

pub use super::*;
impl<T: Config> Pallet<T> {
	pub fn pot_account() -> AccountIdOf<T> {
		// Get Pot account
		let pot_id = T::PotId::get();
		pot_id.into_account_truncating()
	}

	/// Funds transfer from the Pot to a project account
	pub fn spend(amount: BalanceOf<T>, beneficiary: AccountIdOf<T>) -> DispatchResult {
		// Get Pot account
		let pot_account: AccountIdOf<T> = Self::pot_account();

		//Operate the transfer
		T::NativeBalance::transfer(&pot_account, &beneficiary, amount, Preservation::Preserve)?;

		Ok(())
	}

	/// Series of checks on the Pot, to ensure that we have enough funds
	/// before executing a Spend --> used in tests.
	pub fn pot_check(spend: BalanceOf<T>) -> DispatchResult {
		// Get Pot account
		let pot_account = Self::pot_account();

		// Check that the Pot as enough funds for the transfer
		let balance = T::NativeBalance::balance(&pot_account);
		let minimum_balance = T::NativeBalance::minimum_balance();
		let remaining_balance = balance.saturating_sub(spend);

		ensure!(remaining_balance > minimum_balance, Error::<T>::InsufficientPotReserves);
		ensure!(balance > spend, Error::<T>::InsufficientPotReserves);
		Ok(())
	}

	// Done in begin_block
	// At the beginning of every Epoch, populate the `Spends` storage from the `Projects` storage
	// (populated by an external process/pallet) make sure that there is enough funds before
	// creating a new `SpendInfo`, and `ProjectInfo` corresponding to a created `SpendInfo`
	// should be removed from the `Projects` storage. This is also a good place to Reserve the
	// funds for created `SpendInfos`. the function will be use in a hook.

	pub fn begin_block(now: BlockNumberFor<T>) -> Weight {
		let max_block_weight = T::BlockWeights::get().max_block/10;
		let epoch = T::EpochDurationBlocks::get();

		//We reach the check period
		if (now % epoch).is_zero() {
			let mut projects = Projects::<T>::get().into_inner();

			if projects.len() > 0 {
				// Reserve funds for the project
				let pot = Self::pot_account();
				let balance = T::NativeBalance::balance(&pot);
				let minimum_balance = T::NativeBalance::minimum_balance();

				projects = projects
					.iter()
					.filter(|project| {
						// check if the pot has enough fund for the Spend
						// Check that the Pot as enough funds for the transfer
						let remaining_balance = balance.saturating_sub(project.amount);

						// we check that holding the necessary amount cannot fail
						if remaining_balance > minimum_balance && balance > project.amount {
							// Create a new Spend
							let new_spend = SpendInfo::<T>::new(&project);
							let _ = T::NativeBalance::hold(
								&HoldReason::FundsReserved.into(),
								&pot,
								project.amount,
							)
							.expect("Funds Reserve Failed");

							// Emmit an event
							let now = T::BlockNumberProvider::current_block_number();
							Self::deposit_event(Event::SpendCreated {
								when: now,
								amount: new_spend.amount,
								project_id: project.project_id.clone(),
							});
						}
						return false;
					})
					.map(|x| x.clone())
					.collect();
			}

			// Update project storage
			let mut bounded = BoundedVec::<ProjectInfo<T>, T::MaxProjects>::new();
			Projects::<T>::mutate(|val| {
				for p in projects {
					// The number of elements in projects is ALWAYS
					// egual or below T::MaxProjects at this point.
					let _ = bounded.try_push(p);
				}
				*val = bounded;
			});
		}
		max_block_weight
	}
}
