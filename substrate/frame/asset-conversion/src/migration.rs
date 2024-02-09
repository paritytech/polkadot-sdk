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
		fungible::Mutate as FungibleMutate,
		fungibles::{Inspect, Refund as RefundT, ResetTeam as ResetTeamT},
		tokens::{Fortitude, Precision, Preservation, WithdrawConsequence},
	};

	/// Type facilitating the migration of existing pools to new account ids when the type deriving
	/// a pool's account id from pool id has changed.
	///
	/// ### Parameters:
	/// - `T`: The [`Config`] implementation for the target asset conversion instance with a new
	///   account derivation method defined by [`PoolLocator`].
	/// - `OldLocator`: The previously used type for account derivation.
	/// - `ResetTeam`: A type used for resetting the team configuration of an LP token.
	/// - `Refund`: A type used to perform a refund for assets from `T::Assets` registry if the
	///   previous pool account holds a deposit.
	/// - `PoolRefund`: A type used to perform a refund for assets from `T::PoolAssets` registry if
	///   the previous pool account holds a deposit.
	/// - `DepositAssets`: asset registry used for deposits for assets from T::Assets` and
	///   `T::PoolAssets`.
	/// - `WeightPerItem`: A getter returning the weight required for the migration of a single pool
	///   account ID. It should include: 2 * weight_of(T::Assets::balance(..)) + 2 *
	///   weight_of(T::Assets::transfer(..)) + 2 * weight_of(Refund::deposit_held(..)) + 2 *
	///   weight_of(Refund::refund(..)) + weight_of(ResetTeam::reset_team(..));
	pub struct Migrate<T, OldLocator, ResetTeam, Refund, PoolRefund, DepositAssets, WeightPerItem>(
		PhantomData<(T, OldLocator, ResetTeam, Refund, PoolRefund, DepositAssets, WeightPerItem)>,
	);
	impl<T, OldLocator, ResetTeam, Refund, PoolRefund, DepositAssets, WeightPerItem>
		OnRuntimeUpgrade
		for Migrate<T, OldLocator, ResetTeam, Refund, PoolRefund, DepositAssets, WeightPerItem>
	where
		T: Config<PoolId = (<T as Config>::AssetKind, <T as Config>::AssetKind)>,
		OldLocator: PoolLocator<T::AccountId, T::AssetKind, T::PoolId>,
		ResetTeam: ResetTeamT<T::AccountId, AssetId = T::PoolAssetId>,
		Refund: RefundT<T::AccountId, AssetId = T::AssetKind, Balance = DepositAssets::Balance>,
		PoolRefund:
			RefundT<T::AccountId, AssetId = T::PoolAssetId, Balance = DepositAssets::Balance>,
		DepositAssets: FungibleMutate<T::AccountId>,
		WeightPerItem: Get<Weight>,
	{
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();
			for (pool_id, info) in Pools::<T>::iter() {
				weight.saturating_accrue(WeightPerItem::get());
				let (account_id, new_account_id) =
					match (OldLocator::address(&pool_id), T::PoolLocator::address(&pool_id)) {
						(Ok(a), Ok(b)) if a != b => (a, b),
						_ => continue,
					};

				let (asset1, asset2) = pool_id;

				log::info!(
					target: LOG_TARGET,
					"migrating an asset pair (`{:?}`, `{:?}`).",
					asset1.clone(),
					asset2.clone(),
				);

				// assets that must be transferred to the new account id.
				let balance1 = T::Assets::balance(asset1.clone(), &account_id);
				let balance2 = T::Assets::balance(asset2.clone(), &account_id);
				let balance3 = T::PoolAssets::balance(info.lp_token.clone(), &account_id);

				// check if it possible to withdraw the assets from old account id.

				let withdraw_result1 =
					T::Assets::can_withdraw(asset1.clone(), &account_id, balance1);
				if withdraw_result1 != WithdrawConsequence::<_>::Success {
					log::error!(
						target: LOG_TARGET,
						"total balance cannot be withdrawn for asset1 from pair (`{:?}`,`{:?}`), account id `{:?}` with result `{:?}`.",
						asset1,
						asset2,
						account_id,
						withdraw_result1,
					);
					continue;
				}

				let withdraw_result2 =
					T::Assets::can_withdraw(asset2.clone(), &account_id, balance2);
				if withdraw_result2 != WithdrawConsequence::<_>::Success {
					log::error!(
						target: LOG_TARGET,
						"total balance cannot be withdrawn for asset2 from pair (`{:?}`,`{:?}`), account id `{:?}` with result `{:?}`.",
						asset1,
						asset2,
						account_id,
						withdraw_result2,
					);
					continue;
				}

				let withdraw_result3 =
					T::PoolAssets::can_withdraw(info.lp_token.clone(), &account_id, balance3);
				if withdraw_result3 != WithdrawConsequence::<_>::Success {
					log::error!(
						target: LOG_TARGET,
						"total balance cannot be withdrawn for lp token `{:?}`, from pair (`{:?}`,`{:?}`), account id `{:?}` with result `{:?}`.",
						info.lp_token,
						asset1,
						asset2,
						account_id,
						withdraw_result3,
					);
					continue;
				}

				// check if deposit has to be placed for the new account.
				// if deposit required mint a deposit amount to the depositor account to ensure the
				// deposit can be provided. after the deposit from the old account will be returned,
				// the minted assets will be burned.

				if let Some((d, b)) = Refund::deposit_held(asset1.clone(), account_id.clone()) {
					let ed = DepositAssets::minimum_balance();
					if let Err(e) = DepositAssets::mint_into(&d, b + ed) {
						log::error!(
							target: LOG_TARGET,
							"failed to mint deposit for asset1 `{:?}`, into account id `{:?}` with error `{:?}`.",
							asset1,
							d,
							e,
						);
						continue;
					}
					if let Err(e) = T::Assets::touch(asset1.clone(), &new_account_id, &d) {
						let burn_res = DepositAssets::burn_from(
							&d,
							b + ed,
							Precision::Exact,
							Fortitude::Force,
						);
						log::error!(
							target: LOG_TARGET,
							"failed to touch account `{:?}`, `{:?}`, from pair (`{:?}`,`{:?}`), with error `{:?}` and burn result `{:?}`.",
							asset1.clone(),
							new_account_id,
							asset1,
							asset2,
							e,
							burn_res,
						);
						continue;
					}
				}

				if let Some((d, b)) = Refund::deposit_held(asset2.clone(), account_id.clone()) {
					let ed = DepositAssets::minimum_balance();
					if let Err(e) = DepositAssets::mint_into(&d, b + ed) {
						log::error!(
							target: LOG_TARGET,
							"failed to mint deposit for asset2 `{:?}`, into account id `{:?}` with error `{:?}`.",
							asset2,
							d,
							e,
						);
						continue;
					}
					if let Err(e) = T::Assets::touch(asset2.clone(), &new_account_id, &d) {
						let burn_res = DepositAssets::burn_from(
							&d,
							b + ed,
							Precision::Exact,
							Fortitude::Force,
						);
						log::error!(
							target: LOG_TARGET,
							"failed to touch account `{:?}`, `{:?}`, from pair (`{:?}`,`{:?}`), with error `{:?}` and burn result `{:?}`.",
							asset2.clone(),
							new_account_id,
							asset1,
							asset2,
							e,
							burn_res,
						);
						continue;
					}
				}

				if let Some((d, b)) =
					PoolRefund::deposit_held(info.lp_token.clone(), account_id.clone())
				{
					let ed = DepositAssets::minimum_balance();
					if let Err(e) = DepositAssets::mint_into(&d, b + ed) {
						log::error!(
							target: LOG_TARGET,
							"failed to mint deposit for lp token `{:?}`, into account id `{:?}` with error `{:?}`.",
							info.lp_token.clone(),
							d,
							e,
						);
						continue;
					}
					if let Err(e) = T::PoolAssets::touch(info.lp_token.clone(), &new_account_id, &d)
					{
						let burn_res = DepositAssets::burn_from(
							&d,
							b + ed,
							Precision::Exact,
							Fortitude::Force,
						);
						log::error!(
							target: LOG_TARGET,
							"failed to touch account `{:?}`, `{:?}`, from pair (`{:?}`,`{:?}`), with error `{:?}` with a burn result `{:?}`.",
							info.lp_token,
							new_account_id,
							asset1,
							asset2,
							e,
							burn_res,
						);
						continue;
					}
				}

				// transfer all pool related assets to the new account.

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

				if let Err(e) = T::Assets::transfer(
					asset2.clone(),
					&account_id,
					&new_account_id,
					balance2,
					Preservation::Expendable,
				) {
					let transfer_res = T::Assets::transfer(
						asset1,
						&new_account_id,
						&account_id,
						balance1,
						Preservation::Expendable,
					);
					log::error!(
						target: LOG_TARGET,
						"transfer all, of `{:?}` from `{:?}` to `{:?}` failed with error `{:?}` with rollback transfer result `{:?}`",
						asset2,
						account_id,
						new_account_id,
						e,
						transfer_res,
					);
					continue;
				}

				if let Err(e) = T::PoolAssets::transfer(
					info.lp_token.clone(),
					&account_id,
					&new_account_id,
					balance3,
					Preservation::Expendable,
				) {
					let transfer_res1 = T::Assets::transfer(
						asset1,
						&new_account_id,
						&account_id,
						balance1,
						Preservation::Expendable,
					);
					let transfer_res2 = T::Assets::transfer(
						asset2,
						&new_account_id,
						&account_id,
						balance2,
						Preservation::Expendable,
					);
					log::error!(
						target: LOG_TARGET,
						"transfer all, of `{:?}` from `{:?}` to `{:?}` failed with error `{:?}` with rollback transfer result `{:?}` and `{:?}`",
						info.lp_token,
						account_id,
						new_account_id,
						e,
						transfer_res1,
						transfer_res2,
					);
					continue;
				}

				// refund deposits from old accounts and burn previously minted assets.

				if let Some((d, b)) = Refund::deposit_held(asset1.clone(), account_id.clone()) {
					if let Err(e) = Refund::refund(asset1.clone(), account_id.clone()) {
						log::error!(
							target: LOG_TARGET,
							"refund for asset1 `{:?}` to account `{:?}` failed with error `{:?}`",
							asset1,
							account_id,
							e,
						);
					}
					let ed = DepositAssets::minimum_balance();
					if let Err(e) =
						DepositAssets::burn_from(&d, b + ed, Precision::Exact, Fortitude::Force)
					{
						log::error!(
							target: LOG_TARGET,
							"failed to burn deposit for asset1 from `{:?}` with error `{:?}`.",
							d,
							e,
						);
						continue;
					}
				}

				if let Some((d, b)) = Refund::deposit_held(asset2.clone(), account_id.clone()) {
					if let Err(e) = Refund::refund(asset2.clone(), account_id.clone()) {
						log::error!(
							target: LOG_TARGET,
							"refund for asset2 `{:?}` to account `{:?}` failed with error `{:?}`",
							asset2.clone(),
							account_id,
							e,
						);
					}
					let ed = DepositAssets::minimum_balance();
					if let Err(e) =
						DepositAssets::burn_from(&d, b + ed, Precision::Exact, Fortitude::Force)
					{
						log::error!(
							target: LOG_TARGET,
							"failed to burn deposit for asset2 from `{:?}` with error `{:?}`.",
							d,
							e,
						);
						continue;
					}
				}

				if let Some((d, b)) =
					PoolRefund::deposit_held(info.lp_token.clone(), account_id.clone())
				{
					if let Err(e) = PoolRefund::refund(info.lp_token.clone(), account_id.clone()) {
						log::error!(
							target: LOG_TARGET,
							"refund for lp token `{:?}` to account `{:?}` failed with error `{:?}`",
							info.lp_token.clone(),
							account_id,
							e,
						);
					}
					let ed = DepositAssets::minimum_balance();
					if let Err(e) =
						DepositAssets::burn_from(&d, b + ed, Precision::Exact, Fortitude::Force)
					{
						log::error!(
							target: LOG_TARGET,
							"failed to burn deposit for lp token from `{:?}` with error `{:?}`.",
							d,
							e,
						);
						continue;
					}
				}

				if let Err(e) = ResetTeam::reset_team(
					info.lp_token.clone(),
					new_account_id.clone(),
					new_account_id.clone(),
					new_account_id.clone(),
					new_account_id,
				) {
					log::error!(
						target: LOG_TARGET,
						"team reset for asset `{:?}` failed with error `{:?}`",
						info.lp_token,
						e,
					);
				}
			}
			weight
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			let mut expected: Vec<(
				T::PoolId,
				// asset1 balance
				<T as Config>::Balance,
				// asset2 balance
				<T as Config>::Balance,
				// lp token balance
				<T as Config>::Balance,
				// total issuance
				DepositAssets::Balance,
			)> = vec![];
			for (pool_id, info) in Pools::<T>::iter() {
				let account_id = OldLocator::address(&pool_id)
					.expect("pool ids must be convertible with old account id conversion type");
				let (asset1, asset2) = pool_id;
				let balance1 = T::Assets::total_balance(asset1.clone(), &account_id);
				let balance2 = T::Assets::total_balance(asset2.clone(), &account_id);
				let balance3 = T::PoolAssets::total_balance(info.lp_token.clone(), &account_id);
				let total_issuance = DepositAssets::total_issuance();
				let withdraw_success = WithdrawConsequence::<<T as Config>::Balance>::Success;
				if T::Assets::can_withdraw(asset1.clone(), &account_id, balance1) ==
					withdraw_success && T::Assets::can_withdraw(
					asset2.clone(),
					&account_id,
					balance2,
				) == withdraw_success &&
					T::PoolAssets::can_withdraw(info.lp_token, &account_id, balance3) ==
						withdraw_success
				{
					expected.push(((asset1, asset2), balance1, balance2, balance3, total_issuance));
				}
			}
			Ok(expected.encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
			let expected: Vec<(
				T::PoolId,
				T::Balance,
				T::Balance,
				T::Balance,
				DepositAssets::Balance,
			)> = Decode::decode(&mut state.as_slice()).expect(
				"the state parameter should be something that was generated by pre_upgrade",
			);
			for (pool_id, balance1, balance2, balance3, total_issuance) in expected {
				let new_account_id = T::PoolLocator::address(&pool_id)
					.expect("pool ids must be convertible with new account id conversion type");
				let info = Pools::<T>::get(&pool_id).expect("pool info must be present");
				let (asset1, asset2) = pool_id;
				assert_eq!(balance1, T::Assets::total_balance(asset1, &new_account_id));
				assert_eq!(balance2, T::Assets::total_balance(asset2, &new_account_id));
				assert_eq!(balance3, T::PoolAssets::total_balance(info.lp_token, &new_account_id));
				assert_eq!(total_issuance, DepositAssets::total_issuance());
			}
			Ok(())
		}
	}
}
