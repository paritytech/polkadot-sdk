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

	/// Series of checks on the Pot, to ensure that we have enough funds
	/// before executing a Spend
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

	/// Funds transfer from the Pot to a project account
	pub fn spend(amount: BalanceOf<T>, beneficiary: AccountIdOf<T>) -> DispatchResult {
		// Get Pot account
		let pot_account: AccountIdOf<T> = Self::pot_account();

		//Operate the transfer
		T::NativeBalance::transfer(&pot_account, &beneficiary, amount, Preservation::Preserve)?;

		Ok(())
	}

	// Done in begin_block
	// At the beginning of every Epoch, populate the `Spends` storage from the `Projects` storage
	// (populated by an external process/pallet) make sure that there is enough funds before
	// creating a new `SpendInfo`, and `ProjectInfo` corresponding to a created `SpendInfo`
	// should be removed from the `Projects` storage. This is also a good place to Reserve the
	// funds for created `SpendInfos`. the function will be use in a hook.

	pub fn begin_block(now: BlockNumberFor<T>) -> Weight {
		let max_block_weight = T::BlockWeights::get().max_block;
		let epoch = T::EpochDurationBlocks::get();

		//We reach the check period
		if (now % epoch).is_zero() {
			let mut projects = Projects::<T>::get();

			if projects.len() > 0 {
				// Reserve funds for the project
				let pot = Self::pot_account();

				for project in projects.clone() {
					// check if the pot has enough fund for the Spend
					let check = Self::pot_check(project.amount);
					if check.is_ok() {
						// Create a new Spend
						let new_spend = SpendInfo::<T>::new(&project);						
						match T::NativeBalance::hold(
							&HoldReason::FundsReserved.into(),
							&pot,
							project.amount,
						){
							Ok(_x) => println!("Hold operation succeded!"),
							Err(e) => println!("{:?}", e),
						};

						// Remove project from project_list
						projects.retain(|value| *value != project);

						// Emmit an event
						let now = T::BlockNumberProvider::current_block_number();
						Self::deposit_event(Event::SpendCreated {
							when: now,
							amount: new_spend.amount,
							project_id: project.project_id,
						});
					}
				}
			}

			// Update project storage
			Projects::<T>::mutate(|val| {
				*val = projects;
			});
		}
		max_block_weight
	}
}
