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
		tokens::{
			DepositConsequence, Fortitude, Precision, Preservation, Provenance, WithdrawConsequence,
		},
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
	///   weight_of(T::Assets::transfer(..)) + weight_of(T::PoolAssets::balance(..)) +
	///   weight_of(T::PoolAssets::transfer(..))  + 2 * weight_of(Refund::deposit_held(..)) + 2 *
	///   weight_of(Refund::refund(..)) + weight_of(PoolRefund::deposit_held(..)) +
	///   weight_of(PoolRefund::refund(..)) + 3 * weight_of(DepositAssets::minimum_balance(..)) + 3
	///   * weight_of(DepositAssets::mint_into(..)) + 3 * weight_of(DepositAssets::burn_from(..)) +
	///   weight_of(ResetTeam::reset_team(..));
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

				// Assets that must be transferred to the new account id.
				let balance1 = T::Assets::balance(asset1.clone(), &account_id);
				let balance2 = T::Assets::balance(asset2.clone(), &account_id);
				let balance3 = T::PoolAssets::balance(info.lp_token.clone(), &account_id);

				log::info!(
					target: LOG_TARGET,
					"migrating an asset pair (`{:?}`, `{:?}`) with lp token `{:?}` from old account id `{:?}` to the new account id `{:?}` with balances `{:?}`, `{:?}`, `{:?}`.",
					asset1.clone(),
					asset2.clone(),
					info.lp_token.clone(),
					account_id.clone(), new_account_id.clone(),
					balance1,
					balance2,
					balance3,
				);

				// Check if it's possible to withdraw the assets from old account id.
				// It might fail if asset is not live.

				let withdraw_result1 =
					T::Assets::can_withdraw(asset1.clone(), &account_id, balance1);
				if !matches!(
					withdraw_result1,
					WithdrawConsequence::<_>::Success | WithdrawConsequence::<_>::ReducedToZero(_)
				) {
					log::error!(
						target: LOG_TARGET,
						"total balance of asset1 cannot be withdrawn from the old account with result `{:?}`.",
						withdraw_result1,
					);
					continue;
				}

				let withdraw_result2 =
					T::Assets::can_withdraw(asset2.clone(), &account_id, balance2);
				if !matches!(
					withdraw_result2,
					WithdrawConsequence::<_>::Success | WithdrawConsequence::<_>::ReducedToZero(_)
				) {
					log::error!(
						target: LOG_TARGET,
						"total balance of asset2 cannot be withdrawn from the old account with result `{:?}`.",
						withdraw_result2,
					);
					continue;
				}

				let withdraw_result3 =
					T::PoolAssets::can_withdraw(info.lp_token.clone(), &account_id, balance3);
				if !matches!(
					withdraw_result3,
					WithdrawConsequence::<_>::Success | WithdrawConsequence::<_>::ReducedToZero(_)
				) {
					log::error!(
						target: LOG_TARGET,
						"total balance of lp token cannot be withdrawn from the old account with result `{:?}`.",
						withdraw_result3,
					);
					continue;
				}

				// Check if it's possible to deposit the assets to new account id.
				// It might fail if asset is not live or minimum balance has changed.

				let deposit_result1 = T::Assets::can_deposit(
					asset1.clone(),
					&new_account_id,
					balance1,
					Provenance::Extant,
				);
				if !matches!(
					deposit_result1,
					DepositConsequence::Success | DepositConsequence::CannotCreate
				) {
					log::error!(
						target: LOG_TARGET,
						"total balance of asset1 cannot be deposited to the new account with result `{:?}`.",
						deposit_result1,
					);
					continue;
				}

				let deposit_result2 = T::Assets::can_deposit(
					asset2.clone(),
					&new_account_id,
					balance2,
					Provenance::Extant,
				);
				if !matches!(
					deposit_result2,
					DepositConsequence::Success | DepositConsequence::CannotCreate
				) {
					log::error!(
						target: LOG_TARGET,
						"total balance of asset2 cannot be deposited to the new account with result `{:?}`.",
						deposit_result2,
					);
					continue;
				}

				let deposit_result3 = T::PoolAssets::can_deposit(
					info.lp_token.clone(),
					&new_account_id,
					balance3,
					Provenance::Extant,
				);
				if !matches!(
					deposit_result3,
					DepositConsequence::Success | DepositConsequence::CannotCreate
				) {
					log::error!(
						target: LOG_TARGET,
						"total balance of lp token cannot be deposited to the new account with result `{:?}`.",
						deposit_result3,
					);
					continue;
				}

				// Check if a deposit needs to be placed for the new account. If so, mint the
				// required deposit amount to the depositor's account to ensure it can be provided.
				// Once the deposit from the old account is returned, the minted assets will be
				// burned. Minting assets is necessary because it's not possible to transfer assets
				// to the new account if a deposit is required but not provided. Additionally, the
				// deposit cannot be refunded from the old account until its balance is zero.

				if let Some((d, b)) = Refund::deposit_held(asset1.clone(), account_id.clone()) {
					let ed = DepositAssets::minimum_balance();
					if let Err(e) = DepositAssets::mint_into(&d, b + ed) {
						log::error!(
							target: LOG_TARGET,
							"failed to mint deposit for asset1 into depositor account id `{:?}` with error `{:?}`.",
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
							"failed to touch the new account for asset1 with error `{:?}` and burn result `{:?}`.",
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
							"failed to mint deposit for asset2 into depositor account id `{:?}` with error `{:?}`.",
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
							"failed to touch the new account for asset2 with error `{:?}` and burn result `{:?}`.",
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
							"failed to mint deposit for lp token into depositor account id `{:?}` with error `{:?}`.",
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
							"failed to touch the new account for lp token with error `{:?}` with a burn result `{:?}`.",
							e,
							burn_res,
						);
						continue;
					}
				}

				// Transfer all pool related assets to the new account.

				if let Err(e) = T::Assets::transfer(
					asset1.clone(),
					&account_id,
					&new_account_id,
					balance1,
					Preservation::Expendable,
				) {
					log::error!(
						target: LOG_TARGET,
						"transfer of asset1 to the new account failed with error `{:?}`",
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
						"transfer of asset2 failed with error `{:?}` and rollback transfer result `{:?}`",
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
						"transfer of lp tokens failed with error `{:?}` and rollback transfer results `{:?}` and `{:?}`",
						e,
						transfer_res1,
						transfer_res2,
					);
					continue;
				}

				// Refund deposits from old accounts and burn previously minted assets.

				if let Some((d, b)) = Refund::deposit_held(asset1.clone(), account_id.clone()) {
					if let Err(e) = Refund::refund(asset1.clone(), account_id.clone()) {
						log::error!(
							target: LOG_TARGET,
							"refund of asset1 account deposit failed with error `{:?}`",
							e,
						);
					}
					let ed = DepositAssets::minimum_balance();
					if let Err(e) =
						DepositAssets::burn_from(&d, b + ed, Precision::Exact, Fortitude::Force)
					{
						log::error!(
							target: LOG_TARGET,
							"burn of asset1 from depositor account id `{:?}` failed with error `{:?}`.",
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
							"refund of asset2 account deposit failed with error `{:?}`",
							e,
						);
					}
					let ed = DepositAssets::minimum_balance();
					if let Err(e) =
						DepositAssets::burn_from(&d, b + ed, Precision::Exact, Fortitude::Force)
					{
						log::error!(
							target: LOG_TARGET,
							"burn of asset2 from depositor account id `{:?}` failed with error `{:?}`.",
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
							"refund of lp token account deposit failed with error `{:?}`",
							e,
						);
					}
					let ed = DepositAssets::minimum_balance();
					if let Err(e) =
						DepositAssets::burn_from(&d, b + ed, Precision::Exact, Fortitude::Force)
					{
						log::error!(
							target: LOG_TARGET,
							"burn of lp tokens from depositor account id `{:?}` failed with error `{:?}`.",
							d,
							e,
						);
						continue;
					}
				}

				if let Err(e) = ResetTeam::reset_team(
					info.lp_token,
					new_account_id.clone(),
					new_account_id.clone(),
					new_account_id.clone(),
					new_account_id,
				) {
					log::error!(
						target: LOG_TARGET,
						"team reset for lp tone failed with error `{:?}`",
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
					.expect("account id must be derivable with old pool locator");
				let new_account_id = T::PoolLocator::address(&pool_id)
					.expect("account id must be derivable with new pool locator");
				let (asset1, asset2) = pool_id;
				let balance1 = T::Assets::balance(asset1.clone(), &account_id);
				let balance2 = T::Assets::balance(asset2.clone(), &account_id);
				let balance3 = T::PoolAssets::balance(info.lp_token.clone(), &account_id);
				let total_issuance = DepositAssets::total_issuance();

				assert_eq!(T::Balance::zero(), T::Assets::balance(asset1.clone(), &new_account_id));
				assert_eq!(T::Balance::zero(), T::Assets::balance(asset2.clone(), &new_account_id));
				assert_eq!(
					T::Balance::zero(),
					T::PoolAssets::balance(info.lp_token.clone(), &new_account_id)
				);

				let withdraw_result1 =
					T::Assets::can_withdraw(asset1.clone(), &account_id, balance1);
				let withdraw_result2 =
					T::Assets::can_withdraw(asset2.clone(), &account_id, balance2);
				let withdraw_result3 =
					T::PoolAssets::can_withdraw(info.lp_token.clone(), &account_id, balance3);

				if !matches!(
					withdraw_result1,
					WithdrawConsequence::<_>::Success | WithdrawConsequence::<_>::ReducedToZero(_)
				) || !matches!(
					withdraw_result2,
					WithdrawConsequence::<_>::Success | WithdrawConsequence::<_>::ReducedToZero(_)
				) || !matches!(
					withdraw_result3,
					WithdrawConsequence::<_>::Success | WithdrawConsequence::<_>::ReducedToZero(_)
				) {
					log::warn!(
						target: LOG_TARGET,
						"cannot withdraw, migration for asset pair (`{:?}`,`{:?}`) will be skipped, with results: `{:?}`, `{:?}`, `{:?}`",
						asset1,
						asset2,
						withdraw_result1,
						withdraw_result2,
						withdraw_result3,
					);
					continue;
				}

				let increase_result1 = T::Assets::can_deposit(
					asset1.clone(),
					&new_account_id,
					balance1,
					Provenance::Extant,
				);
				let increase_result2 = T::Assets::can_deposit(
					asset2.clone(),
					&new_account_id,
					balance2,
					Provenance::Extant,
				);
				let increase_result3 = T::PoolAssets::can_deposit(
					info.lp_token,
					&new_account_id,
					balance3,
					Provenance::Extant,
				);

				if !matches!(
					increase_result1,
					DepositConsequence::Success | DepositConsequence::CannotCreate
				) || !matches!(
					increase_result2,
					DepositConsequence::Success | DepositConsequence::CannotCreate
				) || !matches!(
					increase_result3,
					DepositConsequence::Success | DepositConsequence::CannotCreate
				) {
					log::warn!(
						target: LOG_TARGET,
						"cannot deposit, migration for asset pair (`{:?}`,`{:?}`) will be skipped, with results: `{:?}`, `{:?}`, `{:?}`", 		asset1,
						asset2,
						increase_result1,
						increase_result2,
						increase_result3,
					);
					continue;
				}

				log::info!(
					target: LOG_TARGET,
					"asset pair (`{:?}`,`{:?}`) will be migrated with balance1 `{:?}`, balance2 `{:?}` and balance3 `{:?}`.",
					asset1.clone(), asset2.clone(), balance1, balance2, balance3
				);

				expected.push(((asset1, asset2), balance1, balance2, balance3, total_issuance));
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

				log::info!(
					target: LOG_TARGET,
					"assert migration results for asset pair (`{:?}`, `{:?}`).",
					asset1.clone(),
					asset2.clone(),
				);

				assert_eq!(balance1, T::Assets::total_balance(asset1, &new_account_id));
				assert_eq!(balance2, T::Assets::total_balance(asset2, &new_account_id));
				assert_eq!(balance3, T::PoolAssets::total_balance(info.lp_token, &new_account_id));
				assert_eq!(total_issuance, DepositAssets::total_issuance());
			}
			Ok(())
		}
	}
}
