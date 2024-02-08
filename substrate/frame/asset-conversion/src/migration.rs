// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//  http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Storage migrations.

use super::*;
#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use log;
#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;
use sp_std::marker::PhantomData;
#[cfg(feature = "try-runtime")]
use sp_std::vec;

const LOG_TARGET: &'static str = "runtime::asset-conversion::migration";

/// Module providing migration functionality for updating account ids when the type deriving a
/// pool's account id from pool id has changed.
pub mod new_pool_account_id {
	use super::*;
	use frame_support::traits::{
		fungibles::{Refund as RefundT, ResetTeam as ResetTeamT},
		tokens::Preservation,
	};

	/// Type facilitating the migration of existing pools to new account ids when the type deriving
	/// a pool's account id from pool id has changed.
	///
	/// ### Parameters:
	/// - `T`: The [`Config`] implementation for the target asset conversion instance with a new
	///   account derivation method defined by [`PoolLocator`].
	/// - `OldLocator`: The previously used type for account derivation.
	/// - `ResetTeam`: A type used for resetting the team configuration of an LP token.
	/// - `Refund`: A type used to perform a refund if the previous pool account holds a deposit.
	/// - `WeightPerItem`: A getter returning the weight required for the migration of a single pool
	///   account ID. It should include: 2 * weight_of(T::Assets::balance(..)) + 2 *
	///   weight_of(T::Assets::transfer(..)) + 2 * weight_of(Refund::deposit(..)) + 2 *
	///   weight_of(Refund::refund(..)) + weight_of(ResetTeam::reset_team(..));
	pub struct Migrate<T, OldLocator, ResetTeam, Refund, WeightPerItem>(
		PhantomData<(T, OldLocator, ResetTeam, Refund, WeightPerItem)>,
	);
	impl<T, OldLocator, ResetTeam, Refund, WeightPerItem> OnRuntimeUpgrade
		for Migrate<T, OldLocator, ResetTeam, Refund, WeightPerItem>
	where
		T: Config<PoolId = (<T as Config>::AssetKind, <T as Config>::AssetKind)>,
		OldLocator: PoolLocator<T::AccountId, T::AssetKind, T::PoolId>,
		ResetTeam: ResetTeamT<T::AccountId, AssetId = T::PoolAssetId>,
		Refund: RefundT<T::AccountId, AssetId = T::AssetKind>,
		WeightPerItem: Get<Weight>,
	{
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();
			for (pool_id, pool_info) in Pools::<T>::iter() {
				weight.saturating_accrue(WeightPerItem::get());
				let (account_id, new_account_id) =
					match (OldLocator::address(&pool_id), T::PoolLocator::address(&pool_id)) {
						(Ok(a), Ok(b)) if a != b => (a, b),
						_ => continue,
					};

				let (asset1, asset2) = pool_id;
				let balance1 = T::Assets::balance(asset1.clone(), &account_id);
				if let Err(e) = T::Assets::transfer(
					asset1.clone(),
					&account_id,
					&new_account_id,
					balance1,
					Preservation::Expendable,
				) {
					log::error!(
						target: LOG_TARGET,
						"transfer all, of `{:?}` from `{:?}` to `{:?}` failed with error `{:?}`",
						asset1,
						account_id,
						new_account_id,
						e,
					);
					continue;
				}

				let balance2 = T::Assets::balance(asset2.clone(), &account_id);
				if let Err(e) = T::Assets::transfer(
					asset2.clone(),
					&account_id,
					&new_account_id,
					balance2,
					Preservation::Expendable,
				) {
					log::error!(
						target: LOG_TARGET,
						"transfer all, of `{:?}` from `{:?}` to `{:?}` failed with error `{:?}`",
						asset2,
						account_id,
						new_account_id,
						e,
					);
					let _ = T::Assets::transfer(
						asset1,
						&new_account_id,
						&account_id,
						balance1,
						Preservation::Expendable,
					);
					continue;
				}

				if Refund::deposit(asset1.clone(), account_id.clone()).is_some() {
					if let Err(e) = Refund::refund(asset1.clone(), account_id.clone()) {
						log::error!(
							target: LOG_TARGET,
							"refund for asset1 `{:?}` to account `{:?}` failed with error `{:?}`",
							asset1,
							account_id,
							e,
						);
					}
				}

				if Refund::deposit(asset2.clone(), account_id.clone()).is_some() {
					if let Err(e) = Refund::refund(asset2.clone(), account_id.clone()) {
						log::error!(
							target: LOG_TARGET,
							"refund for asset2 `{:?}` to account `{:?}` failed with error `{:?}`",
							asset2.clone(),
							account_id,
							e,
						);
					}
				}

				if let Err(e) = ResetTeam::reset_team(
					pool_info.lp_token.clone(),
					new_account_id.clone(),
					new_account_id.clone(),
					new_account_id.clone(),
					new_account_id,
				) {
					log::error!(
						target: LOG_TARGET,
						"team reset for asset `{:?}` failed with error `{:?}`",
						pool_info.lp_token,
						e,
					);
				}
			}
			weight
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			let mut expected: Vec<(T::PoolId, <T as Config>::Balance, <T as Config>::Balance)> =
				vec![];
			for (pool_id, _) in Pools::<T>::iter() {
				let account_id = OldLocator::address(&pool_id)
					.expect("pool ids must be convertible with old account id conversion type");
				let (asset1, asset2) = pool_id;
				let balance1 = T::Assets::total_balance(asset1.clone(), &account_id);
				let balance2 = T::Assets::total_balance(asset2.clone(), &account_id);
				expected.push(((asset1, asset2), balance1, balance2));
			}
			Ok(expected.encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
			let expected: Vec<(T::PoolId, T::Balance, T::Balance)> = Decode::decode(
				&mut state.as_slice(),
			)
			.expect("the state parameter should be something that was generated by pre_upgrade");
			for (pool_id, balance1, balance2) in expected {
				let new_account_id = T::PoolLocator::address(&pool_id)
					.expect("pool ids must be convertible with new account id conversion type");
				let (asset1, asset2) = pool_id;
				assert_eq!(balance1, T::Assets::total_balance(asset1, &new_account_id));
				assert_eq!(balance2, T::Assets::total_balance(asset2, &new_account_id));
			}
			Ok(())
		}
	}
}
