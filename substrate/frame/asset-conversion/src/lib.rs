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

//! # Substrate Asset Conversion pallet
//!
//! Substrate Asset Conversion pallet based on the [Uniswap V2](https://github.com/Uniswap/v2-core) logic.
//!
//! ## Overview
//!
//! This pallet allows you to:
//!
//!  - [create a liquidity pool](`Pallet::create_pool()`) for 2 assets
//!  - [provide the liquidity](`Pallet::add_liquidity()`) and receive back an LP token
//!  - [exchange the LP token back to assets](`Pallet::remove_liquidity()`)
//!  - [swap a specific amount of assets for another](`Pallet::swap_exact_tokens_for_tokens()`) if
//!    there is a pool created, or
//!  - [swap some assets for a specific amount of
//!    another](`Pallet::swap_tokens_for_exact_tokens()`).
//!  - [query for an exchange price](`AssetConversionApi::quote_price_exact_tokens_for_tokens`) via
//!    a runtime call endpoint
//!  - [query the size of a liquidity pool](`AssetConversionApi::get_reserves`) via a runtime api
//!    endpoint.
//!
//! The `quote_price_exact_tokens_for_tokens` and `quote_price_tokens_for_exact_tokens` functions
//! both take a path parameter of the route to take. If you want to swap from native asset to
//! non-native asset 1, you would pass in a path of `[DOT, 1]` or `[1, DOT]`. If you want to swap
//! from non-native asset 1 to non-native asset 2, you would pass in a path of `[1, DOT, 2]`.
//!
//! (For an example of configuring this pallet to use `MultiLocation` as an asset id, see the
//! cumulus repo).
//!
//! Here is an example `state_call` that asks for a quote of a pool of native versus asset 1:
//!
//! ```text
//! curl -sS -H "Content-Type: application/json" -d \
//! '{"id":1, "jsonrpc":"2.0", "method": "state_call", "params": ["AssetConversionApi_quote_price_tokens_for_exact_tokens", "0x0101000000000000000000000011000000000000000000"]}' \
//! http://localhost:9933/
//! ```
//! (This can be run against the kitchen sync node in the `node` folder of this repo.)
#![deny(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
mod swap;
#[cfg(test)]
mod tests;
mod types;
pub mod weights;

pub use pallet::*;
pub use swap::*;
pub use types::*;
pub use weights::WeightInfo;

use codec::Codec;
use frame_support::{
	storage::{with_storage_layer, with_transaction},
	traits::{
		fungible::{
			Balanced as BalancedFungible, Credit as CreditFungible, Inspect as InspectFungible,
			Mutate as MutateFungible,
		},
		fungibles::{Balanced, Create, Credit as CreditFungibles, Inspect, Mutate},
		tokens::{
			AssetId, Balance,
			Fortitude::Polite,
			Precision::Exact,
			Preservation::{Expendable, Preserve},
		},
		AccountTouch, ContainsPair, Imbalance, Incrementable,
	},
	PalletId,
};
use sp_core::Get;
use sp_runtime::{
	traits::{
		CheckedAdd, CheckedDiv, CheckedMul, CheckedSub, Ensure, IntegerSquareRoot, MaybeDisplay,
		One, TrailingZeroInput, Zero,
	},
	DispatchError, RuntimeDebug, Saturating, TokenError, TransactionOutcome,
};
use sp_std::{boxed::Box, collections::btree_set::BTreeSet, vec::Vec};

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	use sp_arithmetic::{traits::Unsigned, Permill};

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Currency type that this works on.
		type Currency: InspectFungible<Self::AccountId, Balance = Self::Balance>
			+ MutateFungible<Self::AccountId>
			+ BalancedFungible<Self::AccountId>;

		/// The type in which the assets for swapping are measured.
		type Balance: Balance;

		/// A type used for calculations concerning the `Balance` type to avoid possible overflows.
		type HigherPrecisionBalance: IntegerSquareRoot
			+ One
			+ Ensure
			+ Unsigned
			+ From<u32>
			+ From<Self::Balance>
			+ TryInto<Self::Balance>;

		/// Identifier for the class of non-native asset.
		/// Note: A `From<u32>` bound here would prevent `MultiLocation` from being used as an
		/// `AssetId`.
		type AssetId: AssetId;

		/// Type that identifies either the native currency or a token class from `Assets`.
		/// `Ord` is added because of `get_pool_id`.
		///
		/// The pool's `AccountId` is derived from this type. Any changes to the type may
		/// necessitate a migration.
		type MultiAssetId: AssetId + Ord + From<Self::AssetId>;

		/// Type to convert an `AssetId` into `MultiAssetId`.
		type MultiAssetIdConverter: MultiAssetIdConverter<Self::MultiAssetId, Self::AssetId>;

		/// `AssetId` to address the lp tokens by.
		type PoolAssetId: AssetId + PartialOrd + Incrementable + From<u32>;

		/// Registry for the assets.
		type Assets: Inspect<Self::AccountId, AssetId = Self::AssetId, Balance = Self::Balance>
			+ Mutate<Self::AccountId>
			+ AccountTouch<Self::AssetId, Self::AccountId>
			+ ContainsPair<Self::AssetId, Self::AccountId>
			+ Balanced<Self::AccountId>;

		/// Registry for the lp tokens. Ideally only this pallet should have create permissions on
		/// the assets.
		type PoolAssets: Inspect<Self::AccountId, AssetId = Self::PoolAssetId, Balance = Self::Balance>
			+ Create<Self::AccountId>
			+ Mutate<Self::AccountId>
			+ AccountTouch<Self::PoolAssetId, Self::AccountId>;

		/// A % the liquidity providers will take of every swap. Represents 10ths of a percent.
		#[pallet::constant]
		type LPFee: Get<u32>;

		/// A one-time fee to setup the pool.
		#[pallet::constant]
		type PoolSetupFee: Get<Self::Balance>;

		/// An account that receives the pool setup fee.
		type PoolSetupFeeReceiver: Get<Self::AccountId>;

		/// A fee to withdraw the liquidity.
		#[pallet::constant]
		type LiquidityWithdrawalFee: Get<Permill>;

		/// The minimum LP token amount that could be minted. Ameliorates rounding errors.
		#[pallet::constant]
		type MintMinLiquidity: Get<Self::Balance>;

		/// The max number of hops in a swap.
		#[pallet::constant]
		type MaxSwapPathLength: Get<u32>;

		/// The pallet's id, used for deriving its sovereign account ID.
		#[pallet::constant]
		type PalletId: Get<PalletId>;

		/// A setting to allow creating pools with both non-native assets.
		#[pallet::constant]
		type AllowMultiAssetPools: Get<bool>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;

		/// The benchmarks need a way to create asset ids from u32s.
		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: BenchmarkHelper<Self::AssetId, Self::MultiAssetId>;
	}

	/// Map from `PoolAssetId` to `PoolInfo`. This establishes whether a pool has been officially
	/// created rather than people sending tokens directly to a pool's public account.
	#[pallet::storage]
	pub type Pools<T: Config> =
		StorageMap<_, Blake2_128Concat, PoolIdOf<T>, PoolInfo<T::PoolAssetId>, OptionQuery>;

	/// Stores the `PoolAssetId` that is going to be used for the next lp token.
	/// This gets incremented whenever a new lp pool is created.
	#[pallet::storage]
	pub type NextPoolAssetId<T: Config> = StorageValue<_, T::PoolAssetId, OptionQuery>;

	// Pallet's events.
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A successful call of the `CretaPool` extrinsic will create this event.
		PoolCreated {
			/// The account that created the pool.
			creator: T::AccountId,
			/// The pool id associated with the pool. Note that the order of the assets may not be
			/// the same as the order specified in the create pool extrinsic.
			pool_id: PoolIdOf<T>,
			/// The account ID of the pool.
			pool_account: T::AccountId,
			/// The id of the liquidity tokens that will be minted when assets are added to this
			/// pool.
			lp_token: T::PoolAssetId,
		},

		/// A successful call of the `AddLiquidity` extrinsic will create this event.
		LiquidityAdded {
			/// The account that the liquidity was taken from.
			who: T::AccountId,
			/// The account that the liquidity tokens were minted to.
			mint_to: T::AccountId,
			/// The pool id of the pool that the liquidity was added to.
			pool_id: PoolIdOf<T>,
			/// The amount of the first asset that was added to the pool.
			amount1_provided: T::Balance,
			/// The amount of the second asset that was added to the pool.
			amount2_provided: T::Balance,
			/// The id of the lp token that was minted.
			lp_token: T::PoolAssetId,
			/// The amount of lp tokens that were minted of that id.
			lp_token_minted: T::Balance,
		},

		/// A successful call of the `RemoveLiquidity` extrinsic will create this event.
		LiquidityRemoved {
			/// The account that the liquidity tokens were burned from.
			who: T::AccountId,
			/// The account that the assets were transferred to.
			withdraw_to: T::AccountId,
			/// The pool id that the liquidity was removed from.
			pool_id: PoolIdOf<T>,
			/// The amount of the first asset that was removed from the pool.
			amount1: T::Balance,
			/// The amount of the second asset that was removed from the pool.
			amount2: T::Balance,
			/// The id of the lp token that was burned.
			lp_token: T::PoolAssetId,
			/// The amount of lp tokens that were burned of that id.
			lp_token_burned: T::Balance,
			/// Liquidity withdrawal fee (%).
			withdrawal_fee: Permill,
		},
		/// Assets have been converted from one to another. Both `SwapExactTokenForToken`
		/// and `SwapTokenForExactToken` will generate this event.
		SwapExecuted {
			/// Which account was the instigator of the swap.
			who: T::AccountId,
			/// The account that the assets were transferred to.
			send_to: T::AccountId,
			/// The amount of the first asset that was swapped.
			amount_in: T::Balance,
			/// The amount of the second asset that was received.
			amount_out: T::Balance,
			/// The route of asset IDs with amounts that the swap went through.
			/// E.g. (A, amount_in) -> (Dot, amount_out) -> (B, amount_out)
			path: BalancePath<T>,
		},
		/// Assets have been converted from one to another.
		SwapCreditExecuted {
			/// The amount of the first asset that was swapped.
			amount_in: T::Balance,
			/// The amount of the second asset that was received.
			amount_out: T::Balance,
			/// The route of asset IDs with amounts that the swap went through.
			/// E.g. (A, amount_in) -> (Dot, amount_out) -> (B, amount_out)
			path: BalancePath<T>,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Provided assets are equal.
		EqualAssets,
		/// Provided asset is not supported for pool.
		UnsupportedAsset,
		/// Pool already exists.
		PoolExists,
		/// Desired amount can't be zero.
		WrongDesiredAmount,
		/// Provided amount should be greater than or equal to the existential deposit/asset's
		/// minimal amount.
		AmountOneLessThanMinimal,
		/// Provided amount should be greater than or equal to the existential deposit/asset's
		/// minimal amount.
		AmountTwoLessThanMinimal,
		/// Reserve needs to always be greater than or equal to the existential deposit/asset's
		/// minimal amount.
		ReserveLeftLessThanMinimal,
		/// Desired amount can't be equal to the pool reserve.
		AmountOutTooHigh,
		/// The pool doesn't exist.
		PoolNotFound,
		/// An overflow happened.
		Overflow,
		/// The minimal amount requirement for the first token in the pair wasn't met.
		AssetOneDepositDidNotMeetMinimum,
		/// The minimal amount requirement for the second token in the pair wasn't met.
		AssetTwoDepositDidNotMeetMinimum,
		/// The minimal amount requirement for the first token in the pair wasn't met.
		AssetOneWithdrawalDidNotMeetMinimum,
		/// The minimal amount requirement for the second token in the pair wasn't met.
		AssetTwoWithdrawalDidNotMeetMinimum,
		/// Optimal calculated amount is less than desired.
		OptimalAmountLessThanDesired,
		/// Insufficient liquidity minted.
		InsufficientLiquidityMinted,
		/// Requested liquidity can't be zero.
		ZeroLiquidity,
		/// Amount can't be zero.
		ZeroAmount,
		/// Insufficient liquidity in the pool.
		InsufficientLiquidity,
		/// Calculated amount out is less than provided minimum amount.
		ProvidedMinimumNotSufficientForSwap,
		/// Provided maximum amount is not sufficient for swap.
		ProvidedMaximumNotSufficientForSwap,
		/// Only pools with native on one side are valid.
		PoolMustContainNativeCurrency,
		/// The provided path must consists of 2 assets at least.
		InvalidPath,
		/// It was not possible to calculate path data.
		PathError,
		/// The provided path must consists of unique assets.
		NonUniquePath,
		/// It was not possible to get or increment the Id of the pool.
		IncorrectPoolAssetId,
		/// The destination account cannot exist with the swapped funds.
		BelowMinimum,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn integrity_test() {
			assert!(
				T::MaxSwapPathLength::get() > 1,
				"the `MaxSwapPathLength` should be greater than 1",
			);
		}
	}

	/// Pallet's callable functions.
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Creates an empty liquidity pool and an associated new `lp_token` asset
		/// (the id of which is returned in the `Event::PoolCreated` event).
		///
		/// Once a pool is created, someone may [`Pallet::add_liquidity`] to it.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::create_pool())]
		pub fn create_pool(
			origin: OriginFor<T>,
			asset1: Box<T::MultiAssetId>,
			asset2: Box<T::MultiAssetId>,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;
			ensure!(asset1 != asset2, Error::<T>::EqualAssets);

			// prepare pool_id
			let pool_id = Self::get_pool_id(*asset1, *asset2);
			ensure!(!Pools::<T>::contains_key(&pool_id), Error::<T>::PoolExists);
			let (asset1, asset2) = &pool_id;
			if !T::AllowMultiAssetPools::get() && !T::MultiAssetIdConverter::is_native(asset1) {
				Err(Error::<T>::PoolMustContainNativeCurrency)?;
			}

			let pool_account = Self::get_pool_account(&pool_id);
			frame_system::Pallet::<T>::inc_providers(&pool_account);

			// pay the setup fee
			T::Currency::transfer(
				&sender,
				&T::PoolSetupFeeReceiver::get(),
				T::PoolSetupFee::get(),
				Preserve,
			)?;

			// try to convert both assets
			match T::MultiAssetIdConverter::try_convert(asset1) {
				MultiAssetIdConversionResult::Converted(asset) =>
					if !T::Assets::contains(&asset, &pool_account) {
						T::Assets::touch(asset, &pool_account, &sender)?
					},
				MultiAssetIdConversionResult::Unsupported(_) => Err(Error::<T>::UnsupportedAsset)?,
				MultiAssetIdConversionResult::Native => (),
			}
			match T::MultiAssetIdConverter::try_convert(asset2) {
				MultiAssetIdConversionResult::Converted(asset) =>
					if !T::Assets::contains(&asset, &pool_account) {
						T::Assets::touch(asset, &pool_account, &sender)?
					},
				MultiAssetIdConversionResult::Unsupported(_) => Err(Error::<T>::UnsupportedAsset)?,
				MultiAssetIdConversionResult::Native => (),
			}

			let lp_token = NextPoolAssetId::<T>::get()
				.or(T::PoolAssetId::initial_value())
				.ok_or(Error::<T>::IncorrectPoolAssetId)?;
			let next_lp_token_id = lp_token.increment().ok_or(Error::<T>::IncorrectPoolAssetId)?;
			NextPoolAssetId::<T>::set(Some(next_lp_token_id));

			T::PoolAssets::create(lp_token.clone(), pool_account.clone(), false, 1u32.into())?;
			T::PoolAssets::touch(lp_token.clone(), &pool_account, &sender)?;

			let pool_info = PoolInfo { lp_token: lp_token.clone() };
			Pools::<T>::insert(pool_id.clone(), pool_info);

			Self::deposit_event(Event::PoolCreated {
				creator: sender,
				pool_id,
				pool_account,
				lp_token,
			});

			Ok(())
		}

		/// Provide liquidity into the pool of `asset1` and `asset2`.
		/// NOTE: an optimal amount of asset1 and asset2 will be calculated and
		/// might be different than the provided `amount1_desired`/`amount2_desired`
		/// thus you should provide the min amount you're happy to provide.
		/// Params `amount1_min`/`amount2_min` represent that.
		/// `mint_to` will be sent the liquidity tokens that represent this share of the pool.
		///
		/// Once liquidity is added, someone may successfully call
		/// [`Pallet::swap_exact_tokens_for_tokens`] successfully.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::add_liquidity())]
		pub fn add_liquidity(
			origin: OriginFor<T>,
			asset1: Box<T::MultiAssetId>,
			asset2: Box<T::MultiAssetId>,
			amount1_desired: T::Balance,
			amount2_desired: T::Balance,
			amount1_min: T::Balance,
			amount2_min: T::Balance,
			mint_to: T::AccountId,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;

			let pool_id = Self::get_pool_id(*asset1.clone(), *asset2);
			// swap params if needed
			let (amount1_desired, amount2_desired, amount1_min, amount2_min) =
				if pool_id.0 == *asset1 {
					(amount1_desired, amount2_desired, amount1_min, amount2_min)
				} else {
					(amount2_desired, amount1_desired, amount2_min, amount1_min)
				};
			ensure!(
				amount1_desired > Zero::zero() && amount2_desired > Zero::zero(),
				Error::<T>::WrongDesiredAmount
			);

			let maybe_pool = Pools::<T>::get(&pool_id);
			let pool = maybe_pool.as_ref().ok_or(Error::<T>::PoolNotFound)?;
			let pool_account = Self::get_pool_account(&pool_id);

			let (asset1, asset2) = &pool_id;
			let reserve1 = Self::get_balance(&pool_account, asset1)?;
			let reserve2 = Self::get_balance(&pool_account, asset2)?;

			let amount1: T::Balance;
			let amount2: T::Balance;
			if reserve1.is_zero() || reserve2.is_zero() {
				amount1 = amount1_desired;
				amount2 = amount2_desired;
			} else {
				let amount2_optimal = Self::quote(&amount1_desired, &reserve1, &reserve2)?;

				if amount2_optimal <= amount2_desired {
					ensure!(
						amount2_optimal >= amount2_min,
						Error::<T>::AssetTwoDepositDidNotMeetMinimum
					);
					amount1 = amount1_desired;
					amount2 = amount2_optimal;
				} else {
					let amount1_optimal = Self::quote(&amount2_desired, &reserve2, &reserve1)?;
					ensure!(
						amount1_optimal <= amount1_desired,
						Error::<T>::OptimalAmountLessThanDesired
					);
					ensure!(
						amount1_optimal >= amount1_min,
						Error::<T>::AssetOneDepositDidNotMeetMinimum
					);
					amount1 = amount1_optimal;
					amount2 = amount2_desired;
				}
			}

			Self::validate_minimal_amount(amount1.saturating_add(reserve1), asset1)
				.map_err(|_| Error::<T>::AmountOneLessThanMinimal)?;
			Self::validate_minimal_amount(amount2.saturating_add(reserve2), asset2)
				.map_err(|_| Error::<T>::AmountTwoLessThanMinimal)?;

			Self::transfer(asset1, &sender, &pool_account, amount1, true)?;
			Self::transfer(asset2, &sender, &pool_account, amount2, true)?;

			let total_supply = T::PoolAssets::total_issuance(pool.lp_token.clone());

			let lp_token_amount: T::Balance;
			if total_supply.is_zero() {
				lp_token_amount = Self::calc_lp_amount_for_zero_supply(&amount1, &amount2)?;
				T::PoolAssets::mint_into(
					pool.lp_token.clone(),
					&pool_account,
					T::MintMinLiquidity::get(),
				)?;
			} else {
				let side1 = Self::mul_div(&amount1, &total_supply, &reserve1)?;
				let side2 = Self::mul_div(&amount2, &total_supply, &reserve2)?;
				lp_token_amount = side1.min(side2);
			}

			ensure!(
				lp_token_amount > T::MintMinLiquidity::get(),
				Error::<T>::InsufficientLiquidityMinted
			);

			T::PoolAssets::mint_into(pool.lp_token.clone(), &mint_to, lp_token_amount)?;

			Self::deposit_event(Event::LiquidityAdded {
				who: sender,
				mint_to,
				pool_id,
				amount1_provided: amount1,
				amount2_provided: amount2,
				lp_token: pool.lp_token.clone(),
				lp_token_minted: lp_token_amount,
			});

			Ok(())
		}

		/// Allows you to remove liquidity by providing the `lp_token_burn` tokens that will be
		/// burned in the process. With the usage of `amount1_min_receive`/`amount2_min_receive`
		/// it's possible to control the min amount of returned tokens you're happy with.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::remove_liquidity())]
		pub fn remove_liquidity(
			origin: OriginFor<T>,
			asset1: Box<T::MultiAssetId>,
			asset2: Box<T::MultiAssetId>,
			lp_token_burn: T::Balance,
			amount1_min_receive: T::Balance,
			amount2_min_receive: T::Balance,
			withdraw_to: T::AccountId,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;

			let pool_id = Self::get_pool_id(*asset1.clone(), *asset2);
			// swap params if needed
			let (amount1_min_receive, amount2_min_receive) = if pool_id.0 == *asset1 {
				(amount1_min_receive, amount2_min_receive)
			} else {
				(amount2_min_receive, amount1_min_receive)
			};
			let (asset1, asset2) = pool_id.clone();

			ensure!(lp_token_burn > Zero::zero(), Error::<T>::ZeroLiquidity);

			let maybe_pool = Pools::<T>::get(&pool_id);
			let pool = maybe_pool.as_ref().ok_or(Error::<T>::PoolNotFound)?;

			let pool_account = Self::get_pool_account(&pool_id);
			let reserve1 = Self::get_balance(&pool_account, &asset1)?;
			let reserve2 = Self::get_balance(&pool_account, &asset2)?;

			let total_supply = T::PoolAssets::total_issuance(pool.lp_token.clone());
			let withdrawal_fee_amount = T::LiquidityWithdrawalFee::get() * lp_token_burn;
			let lp_redeem_amount = lp_token_burn.saturating_sub(withdrawal_fee_amount);

			let amount1 = Self::mul_div(&lp_redeem_amount, &reserve1, &total_supply)?;
			let amount2 = Self::mul_div(&lp_redeem_amount, &reserve2, &total_supply)?;

			ensure!(
				!amount1.is_zero() && amount1 >= amount1_min_receive,
				Error::<T>::AssetOneWithdrawalDidNotMeetMinimum
			);
			ensure!(
				!amount2.is_zero() && amount2 >= amount2_min_receive,
				Error::<T>::AssetTwoWithdrawalDidNotMeetMinimum
			);
			let reserve1_left = reserve1.saturating_sub(amount1);
			let reserve2_left = reserve2.saturating_sub(amount2);
			Self::validate_minimal_amount(reserve1_left, &asset1)
				.map_err(|_| Error::<T>::ReserveLeftLessThanMinimal)?;
			Self::validate_minimal_amount(reserve2_left, &asset2)
				.map_err(|_| Error::<T>::ReserveLeftLessThanMinimal)?;

			// burn the provided lp token amount that includes the fee
			T::PoolAssets::burn_from(pool.lp_token.clone(), &sender, lp_token_burn, Exact, Polite)?;

			Self::transfer(&asset1, &pool_account, &withdraw_to, amount1, false)?;
			Self::transfer(&asset2, &pool_account, &withdraw_to, amount2, false)?;

			Self::deposit_event(Event::LiquidityRemoved {
				who: sender,
				withdraw_to,
				pool_id,
				amount1,
				amount2,
				lp_token: pool.lp_token.clone(),
				lp_token_burned: lp_token_burn,
				withdrawal_fee: T::LiquidityWithdrawalFee::get(),
			});

			Ok(())
		}

		/// Swap the exact amount of `asset1` into `asset2`.
		/// `amount_out_min` param allows you to specify the min amount of the `asset2`
		/// you're happy to receive.
		///
		/// [`AssetConversionApi::quote_price_exact_tokens_for_tokens`] runtime call can be called
		/// for a quote.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::swap_exact_tokens_for_tokens())]
		pub fn swap_exact_tokens_for_tokens(
			origin: OriginFor<T>,
			path: Vec<Box<T::MultiAssetId>>,
			amount_in: T::Balance,
			amount_out_min: T::Balance,
			send_to: T::AccountId,
			keep_alive: bool,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;
			Self::do_swap_exact_tokens_for_tokens(
				sender,
				path.into_iter().map(|a| *a).collect(),
				amount_in,
				Some(amount_out_min),
				send_to,
				keep_alive,
			)?;
			Ok(())
		}

		/// Swap any amount of `asset1` to get the exact amount of `asset2`.
		/// `amount_in_max` param allows to specify the max amount of the `asset1`
		/// you're happy to provide.
		///
		/// [`AssetConversionApi::quote_price_tokens_for_exact_tokens`] runtime call can be called
		/// for a quote.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::swap_tokens_for_exact_tokens())]
		pub fn swap_tokens_for_exact_tokens(
			origin: OriginFor<T>,
			path: Vec<Box<T::MultiAssetId>>,
			amount_out: T::Balance,
			amount_in_max: T::Balance,
			send_to: T::AccountId,
			keep_alive: bool,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;
			Self::do_swap_tokens_for_exact_tokens(
				sender,
				path.into_iter().map(|a| *a).collect(),
				amount_out,
				Some(amount_in_max),
				send_to,
				keep_alive,
			)?;
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Swap exactly `amount_in` of asset `path[0]` for asset `path[1]`.
		/// If an `amount_out_min` is specified, it will return an error if it is unable to acquire
		/// the amount desired.
		///
		/// Withdraws the `path[0]` asset from `sender`, deposits the `path[1]` asset to `send_to`,
		/// respecting `keep_alive`.
		///
		/// If successful, returns the amount of `path[1]` acquired for the `amount_in`.
		///
		/// WARNING: This may return an error after a partial storage mutation. It should be used
		/// only inside a transactional storage context and an Err result must imply a storage
		/// rollback.
		pub(crate) fn do_swap_exact_tokens_for_tokens(
			sender: T::AccountId,
			path: Vec<T::MultiAssetId>,
			amount_in: T::Balance,
			amount_out_min: Option<T::Balance>,
			send_to: T::AccountId,
			keep_alive: bool,
		) -> Result<T::Balance, DispatchError> {
			ensure!(amount_in > Zero::zero(), Error::<T>::ZeroAmount);
			if let Some(amount_out_min) = amount_out_min {
				ensure!(amount_out_min > Zero::zero(), Error::<T>::ZeroAmount);
			}

			Self::validate_swap_path(&path)?;
			let path = Self::balance_path_from_amount_in(amount_in, path)?;

			let amount_out = path.last().map(|(_, a)| *a).ok_or(Error::<T>::InvalidPath)?;
			if let Some(amount_out_min) = amount_out_min {
				ensure!(
					amount_out >= amount_out_min,
					Error::<T>::ProvidedMinimumNotSufficientForSwap
				);
			}

			Self::swap(&sender, &path, &send_to, keep_alive)?;

			Self::deposit_event(Event::SwapExecuted {
				who: sender,
				send_to,
				amount_in,
				amount_out,
				path,
			});
			Ok(amount_out)
		}

		/// Take the `path[0]` asset and swap some amount for `amount_out` of the `path[1]`. If an
		/// `amount_in_max` is specified, it will return an error if acquiring `amount_out` would be
		/// too costly.
		///
		/// Withdraws `path[0]` asset from `sender`, deposits the `path[1]` asset to `send_to`,
		/// respecting `keep_alive`.
		///
		/// If successful returns the amount of the `path[0]` taken to provide `path[1]`.
		///
		/// WARNING: This may return an error after a partial storage mutation. It should be used
		/// only inside a transactional storage context and an Err result must imply a storage
		/// rollback.
		pub(crate) fn do_swap_tokens_for_exact_tokens(
			sender: T::AccountId,
			path: Vec<T::MultiAssetId>,
			amount_out: T::Balance,
			amount_in_max: Option<T::Balance>,
			send_to: T::AccountId,
			keep_alive: bool,
		) -> Result<T::Balance, DispatchError> {
			ensure!(amount_out > Zero::zero(), Error::<T>::ZeroAmount);
			if let Some(amount_in_max) = amount_in_max {
				ensure!(amount_in_max > Zero::zero(), Error::<T>::ZeroAmount);
			}

			Self::validate_swap_path(&path)?;
			let path = Self::balance_path_from_amount_out(amount_out, path)?;

			let amount_in = path.first().map(|(_, a)| *a).ok_or(Error::<T>::InvalidPath)?;
			if let Some(amount_in_max) = amount_in_max {
				ensure!(
					amount_in <= amount_in_max,
					Error::<T>::ProvidedMaximumNotSufficientForSwap
				);
			}

			Self::swap(&sender, &path, &send_to, keep_alive)?;

			Self::deposit_event(Event::SwapExecuted {
				who: sender,
				send_to,
				amount_in,
				amount_out,
				path,
			});

			Ok(amount_in)
		}

		/// Swap exactly `credit_in` of asset `path[0]` for asset `path[last]`.  If `amount_out_min`
		/// is provided and the swap can't achieve at least this amount, an error is returned.
		///
		/// On a successful swap, the function returns the `credit_out` of `path[last]` obtained
		/// from the `credit_in`. On failure, it returns an `Err` containing the original
		/// `credit_in` and the associated error code.
		///
		/// WARNING: This may return an error after a partial storage mutation. It should be used
		/// only inside a transactional storage context and an Err result must imply a storage
		/// rollback.
		pub(crate) fn do_swap_exact_credit_tokens_for_tokens(
			path: Vec<T::MultiAssetId>,
			credit_in: Credit<T>,
			amount_out_min: Option<T::Balance>,
		) -> Result<Credit<T>, (Credit<T>, DispatchError)> {
			let amount_in = credit_in.peek();
			let inspect_path = |credit_asset| {
				ensure!(path.get(0).map_or(false, |a| *a == credit_asset), Error::<T>::InvalidPath);
				ensure!(!amount_in.is_zero(), Error::<T>::ZeroAmount);
				ensure!(amount_out_min.map_or(true, |a| !a.is_zero()), Error::<T>::ZeroAmount);

				Self::validate_swap_path(&path)?;
				let path = Self::balance_path_from_amount_in(amount_in, path)?;

				let amount_out = path.last().map(|(_, a)| *a).ok_or(Error::<T>::InvalidPath)?;
				ensure!(
					amount_out_min.map_or(true, |a| amount_out >= a),
					Error::<T>::ProvidedMinimumNotSufficientForSwap
				);
				Ok((path, amount_out))
			};
			let (path, amount_out) = match inspect_path(credit_in.asset()) {
				Ok((p, a)) => (p, a),
				Err(e) => return Err((credit_in, e)),
			};

			let credit_out = Self::credit_swap(credit_in, &path)?;

			Self::deposit_event(Event::SwapCreditExecuted { amount_in, amount_out, path });

			Ok(credit_out)
		}

		/// Swaps a portion of `credit_in` of `path[0]` asset to obtain the desired `amount_out` of
		/// the `path[last]` asset. The provided `credit_in` must be adequate to achieve the target
		/// `amount_out`, or an error will occur.
		///
		/// On success, the function returns a (`credit_out`, `credit_change`) tuple, where
		/// `credit_out` represents the acquired amount of the `path[last]` asset, and
		/// `credit_change` is the remaining portion from the `credit_in`. On failure, an `Err` with
		/// the initial `credit_in` and error code is returned.
		///
		/// WARNING: This may return an error after a partial storage mutation. It should be used
		/// only inside a transactional storage context and an Err result must imply a storage
		/// rollback.
		pub(crate) fn do_swap_credit_tokens_for_exact_tokens(
			path: Vec<T::MultiAssetId>,
			credit_in: Credit<T>,
			amount_out: T::Balance,
		) -> Result<(Credit<T>, Credit<T>), (Credit<T>, DispatchError)> {
			let amount_in_max = credit_in.peek();
			let inspect_path = |credit_asset| {
				ensure!(path.get(0).map_or(false, |a| a == &credit_asset), Error::<T>::InvalidPath);
				ensure!(amount_in_max > Zero::zero(), Error::<T>::ZeroAmount);
				ensure!(amount_out > Zero::zero(), Error::<T>::ZeroAmount);

				Self::validate_swap_path(&path)?;
				let path = Self::balance_path_from_amount_out(amount_out, path)?;

				let amount_in = path.first().map(|(_, a)| *a).ok_or(Error::<T>::InvalidPath)?;
				ensure!(
					amount_in <= amount_in_max,
					Error::<T>::ProvidedMaximumNotSufficientForSwap
				);

				Ok((path, amount_in))
			};
			let (path, amount_in) = match inspect_path(credit_in.asset()) {
				Ok((p, a)) => (p, a),
				Err(e) => return Err((credit_in, e)),
			};

			let (credit_in, credit_change) = credit_in.split(amount_in);
			let credit_out = Self::credit_swap(credit_in, &path)?;

			Self::deposit_event(Event::SwapCreditExecuted { amount_in, amount_out, path });

			Ok((credit_out, credit_change))
		}

		/// Transfer an `amount` of `asset_id`, respecting the `keep_alive` requirements.
		fn transfer(
			asset_id: &T::MultiAssetId,
			from: &T::AccountId,
			to: &T::AccountId,
			amount: T::Balance,
			keep_alive: bool,
		) -> Result<T::Balance, DispatchError> {
			let preservation = match keep_alive {
				true => Preserve,
				false => Expendable,
			};
			match T::MultiAssetIdConverter::try_convert(asset_id) {
				MultiAssetIdConversionResult::Converted(asset_id) =>
					T::Assets::transfer(asset_id, from, to, amount, preservation),
				MultiAssetIdConversionResult::Native =>
					Ok(T::Currency::transfer(from, to, amount, preservation)?),
				MultiAssetIdConversionResult::Unsupported(_) =>
					Err(Error::<T>::UnsupportedAsset.into()),
			}
		}

		/// The balance of `who` is increased in order to counter `credit`. If the whole of `credit`
		/// cannot be countered, then nothing is changed and the original `credit` is returned in an
		/// `Err`.
		fn resolve(who: &T::AccountId, credit: Credit<T>) -> Result<(), Credit<T>> {
			match credit {
				Credit::Native(c) => T::Currency::resolve(who, c).map_err(|c| c.into()),
				Credit::Asset(c) => T::Assets::resolve(who, c).map_err(|c| c.into()),
			}
		}

		/// Removes `value` balance of `asset` from `who` account if possible.
		fn withdraw(
			asset: &T::MultiAssetId,
			who: &T::AccountId,
			value: T::Balance,
			keep_alive: bool,
		) -> Result<Credit<T>, DispatchError> {
			let preservation = match keep_alive {
				true => Preserve,
				false => Expendable,
			};
			match T::MultiAssetIdConverter::try_convert(asset) {
				MultiAssetIdConversionResult::Converted(asset) => {
					if preservation == Preserve {
						// TODO drop the ensure! when this issue addressed
						// https://github.com/paritytech/polkadot-sdk/issues/1698
						let free =
							T::Assets::reducible_balance(asset.clone(), who, preservation, Polite);
						ensure!(free >= value, TokenError::NotExpendable);
					}
					T::Assets::withdraw(asset, who, value, Exact, preservation, Polite)
						.map(|c| c.into())
				},
				MultiAssetIdConversionResult::Native => {
					if preservation == Preserve {
						// TODO drop the ensure! when this issue addressed
						// https://github.com/paritytech/polkadot-sdk/issues/1698
						let free = T::Currency::reducible_balance(who, preservation, Polite);
						ensure!(free >= value, TokenError::NotExpendable);
					}
					T::Currency::withdraw(who, value, Exact, preservation, Polite).map(|c| c.into())
				},
				MultiAssetIdConversionResult::Unsupported(_) =>
					Err(Error::<T>::UnsupportedAsset.into()),
			}
		}

		/// Swap assets along the `path`, withdrawing from `sender` and depositing in `send_to`.
		///
		/// Note: It's assumed that the provided `path` is valid.
		///
		/// WARNING: This may return an error after a partial storage mutation. It should be used
		/// only inside a transactional storage context and an Err result must imply a storage
		/// rollback.
		fn swap(
			sender: &T::AccountId,
			path: &BalancePath<T>,
			send_to: &T::AccountId,
			keep_alive: bool,
		) -> Result<(), DispatchError> {
			let (asset_in, amount_in) = path.first().ok_or(Error::<T>::InvalidPath)?;
			let credit_in = Self::withdraw(asset_in, sender, *amount_in, keep_alive)?;

			let credit_out = Self::credit_swap(credit_in, path).map_err(|(_, e)| e)?;
			Self::resolve(send_to, credit_out).map_err(|_| Error::<T>::BelowMinimum)?;

			Ok(())
		}

		/// Swap assets along the specified `path`, consuming `credit_in` and producing
		/// `credit_out`.
		///
		/// If an error occurs, `credit_in` is returned back.
		///
		/// Note: It's assumed that the provided `path` is valid and `credit_in` corresponds to the
		/// first asset in the `path`.
		///
		/// WARNING: This may return an error after a partial storage mutation. It should be used
		/// only inside a transactional storage context and an Err result must imply a storage
		/// rollback.
		fn credit_swap(
			credit_in: Credit<T>,
			path: &BalancePath<T>,
		) -> Result<Credit<T>, (Credit<T>, DispatchError)> {
			let resolve_path = || -> Result<Credit<T>, DispatchError> {
				for pos in 0..=path.len() {
					if let Some([(asset1, _), (asset2, amount_out)]) = path.get(pos..=pos + 1) {
						let pool_from = Self::get_pool_account(&Self::get_pool_id(
							asset1.clone(),
							asset2.clone(),
						));
						if let Some((asset3, _)) = path.get(pos + 2) {
							let pool_to = Self::get_pool_account(&Self::get_pool_id(
								asset2.clone(),
								asset3.clone(),
							));
							Self::transfer(asset2, &pool_from, &pool_to, *amount_out, true)?;
						} else {
							let credit_out = Self::withdraw(asset2, &pool_from, *amount_out, true)?;
							return Ok(credit_out)
						}
					}
				}
				return Err(Error::<T>::InvalidPath.into())
			};

			let credit_out = match resolve_path() {
				Ok(c) => c,
				Err(e) => return Err((credit_in, e)),
			};

			let pool_to = if let Some([(asset1, _), (asset2, _)]) = path.get(0..2) {
				Self::get_pool_account(&Self::get_pool_id(asset1.clone(), asset2.clone()))
			} else {
				return Err((credit_in, Error::<T>::InvalidPath.into()))
			};

			Self::resolve(&pool_to, credit_in).map_err(|c| (c, Error::<T>::BelowMinimum.into()))?;

			Ok(credit_out)
		}

		/// The account ID of the pool.
		///
		/// This actually does computation. If you need to keep using it, then make sure you cache
		/// the value and only call this once.
		pub fn get_pool_account(pool_id: &PoolIdOf<T>) -> T::AccountId {
			let encoded_pool_id = sp_io::hashing::blake2_256(&Encode::encode(pool_id)[..]);

			Decode::decode(&mut TrailingZeroInput::new(encoded_pool_id.as_ref()))
				.expect("infinite length input; no invalid inputs for type; qed")
		}

		/// Get the `owner`'s balance of `asset`, which could be the chain's native asset or another
		/// fungible. Returns a value in the form of an `Balance`.
		fn get_balance(
			owner: &T::AccountId,
			asset: &T::MultiAssetId,
		) -> Result<T::Balance, Error<T>> {
			match T::MultiAssetIdConverter::try_convert(asset) {
				MultiAssetIdConversionResult::Converted(asset_id) => Ok(
					<<T as Config>::Assets>::reducible_balance(asset_id, owner, Expendable, Polite),
				),
				MultiAssetIdConversionResult::Native =>
					Ok(<<T as Config>::Currency>::reducible_balance(owner, Expendable, Polite)),
				MultiAssetIdConversionResult::Unsupported(_) =>
					Err(Error::<T>::UnsupportedAsset.into()),
			}
		}

		/// Returns a pool id constructed from 2 assets.
		/// 1. Native asset should be lower than the other asset ids.
		/// 2. Two native or two non-native assets are compared by their `Ord` implementation.
		///
		/// We expect deterministic order, so (asset1, asset2) or (asset2, asset1) returns the same
		/// result.
		pub fn get_pool_id(asset1: T::MultiAssetId, asset2: T::MultiAssetId) -> PoolIdOf<T> {
			match (
				T::MultiAssetIdConverter::is_native(&asset1),
				T::MultiAssetIdConverter::is_native(&asset2),
			) {
				(true, false) => return (asset1, asset2),
				(false, true) => return (asset2, asset1),
				_ => {
					// else we want to be deterministic based on `Ord` implementation
					if asset1 <= asset2 {
						(asset1, asset2)
					} else {
						(asset2, asset1)
					}
				},
			}
		}

		/// Returns the balance of each asset in the pool.
		/// The tuple result is in the order requested (not necessarily the same as pool order).
		pub fn get_reserves(
			asset1: &T::MultiAssetId,
			asset2: &T::MultiAssetId,
		) -> Result<(T::Balance, T::Balance), Error<T>> {
			let pool_id = Self::get_pool_id(asset1.clone(), asset2.clone());
			let pool_account = Self::get_pool_account(&pool_id);

			let balance1 = Self::get_balance(&pool_account, asset1)?;
			let balance2 = Self::get_balance(&pool_account, asset2)?;

			if balance1.is_zero() || balance2.is_zero() {
				Err(Error::<T>::PoolNotFound)?;
			}

			Ok((balance1, balance2))
		}

		/// Leading to an amount at the end of a `path`, get the required amounts in.
		pub(crate) fn balance_path_from_amount_out(
			amount_out: T::Balance,
			path: Vec<T::MultiAssetId>,
		) -> Result<BalancePath<T>, DispatchError> {
			let mut balance_path: BalancePath<T> = Vec::with_capacity(path.len());
			let mut amount_in: T::Balance = amount_out;

			let mut iter = path.into_iter().rev().peekable();
			while let Some(asset2) = iter.next() {
				let asset1 = match iter.peek() {
					Some(a) => a,
					None => {
						balance_path.push((asset2, amount_in));
						break
					},
				};
				let (reserve_in, reserve_out) = Self::get_reserves(asset1, &asset2)?;
				balance_path.push((asset2, amount_in));
				amount_in = Self::get_amount_in(&amount_in, &reserve_in, &reserve_out)?;
			}
			balance_path.reverse();

			Ok(balance_path)
		}

		/// Following an amount into a `path`, get the corresponding amounts out.
		pub(crate) fn balance_path_from_amount_in(
			amount_in: T::Balance,
			path: Vec<T::MultiAssetId>,
		) -> Result<BalancePath<T>, DispatchError> {
			let mut balance_path: BalancePath<T> = Vec::with_capacity(path.len());
			let mut amount_out: T::Balance = amount_in;

			let mut iter = path.into_iter().peekable();
			while let Some(asset1) = iter.next() {
				let asset2 = match iter.peek() {
					Some(a) => a,
					None => {
						balance_path.push((asset1, amount_out));
						break
					},
				};
				let (reserve_in, reserve_out) = Self::get_reserves(&asset1, asset2)?;
				balance_path.push((asset1, amount_out));
				amount_out = Self::get_amount_out(&amount_out, &reserve_in, &reserve_out)?;
			}
			Ok(balance_path)
		}

		/// Used by the RPC service to provide current prices.
		pub fn quote_price_exact_tokens_for_tokens(
			asset1: T::MultiAssetId,
			asset2: T::MultiAssetId,
			amount: T::Balance,
			include_fee: bool,
		) -> Option<T::Balance> {
			let pool_id = Self::get_pool_id(asset1.clone(), asset2.clone());
			let pool_account = Self::get_pool_account(&pool_id);

			let balance1 = Self::get_balance(&pool_account, &asset1).ok()?;
			let balance2 = Self::get_balance(&pool_account, &asset2).ok()?;
			if !balance1.is_zero() {
				if include_fee {
					Self::get_amount_out(&amount, &balance1, &balance2).ok()
				} else {
					Self::quote(&amount, &balance1, &balance2).ok()
				}
			} else {
				None
			}
		}

		/// Used by the RPC service to provide current prices.
		pub fn quote_price_tokens_for_exact_tokens(
			asset1: T::MultiAssetId,
			asset2: T::MultiAssetId,
			amount: T::Balance,
			include_fee: bool,
		) -> Option<T::Balance> {
			let pool_id = Self::get_pool_id(asset1.clone(), asset2.clone());
			let pool_account = Self::get_pool_account(&pool_id);

			let balance1 = Self::get_balance(&pool_account, &asset1).ok()?;
			let balance2 = Self::get_balance(&pool_account, &asset2).ok()?;
			if !balance1.is_zero() {
				if include_fee {
					Self::get_amount_in(&amount, &balance1, &balance2).ok()
				} else {
					Self::quote(&amount, &balance2, &balance1).ok()
				}
			} else {
				None
			}
		}

		/// Calculates the optimal amount from the reserves.
		pub fn quote(
			amount: &T::Balance,
			reserve1: &T::Balance,
			reserve2: &T::Balance,
		) -> Result<T::Balance, Error<T>> {
			// (amount * reserve2) / reserve1
			Self::mul_div(amount, reserve2, reserve1)
		}

		pub(super) fn calc_lp_amount_for_zero_supply(
			amount1: &T::Balance,
			amount2: &T::Balance,
		) -> Result<T::Balance, Error<T>> {
			let amount1 = T::HigherPrecisionBalance::from(*amount1);
			let amount2 = T::HigherPrecisionBalance::from(*amount2);

			let result = amount1
				.checked_mul(&amount2)
				.ok_or(Error::<T>::Overflow)?
				.integer_sqrt()
				.checked_sub(&T::MintMinLiquidity::get().into())
				.ok_or(Error::<T>::InsufficientLiquidityMinted)?;

			result.try_into().map_err(|_| Error::<T>::Overflow)
		}

		fn mul_div(a: &T::Balance, b: &T::Balance, c: &T::Balance) -> Result<T::Balance, Error<T>> {
			let a = T::HigherPrecisionBalance::from(*a);
			let b = T::HigherPrecisionBalance::from(*b);
			let c = T::HigherPrecisionBalance::from(*c);

			let result = a
				.checked_mul(&b)
				.ok_or(Error::<T>::Overflow)?
				.checked_div(&c)
				.ok_or(Error::<T>::Overflow)?;

			result.try_into().map_err(|_| Error::<T>::Overflow)
		}

		/// Calculates amount out.
		///
		/// Given an input amount of an asset and pair reserves, returns the maximum output amount
		/// of the other asset.
		pub fn get_amount_out(
			amount_in: &T::Balance,
			reserve_in: &T::Balance,
			reserve_out: &T::Balance,
		) -> Result<T::Balance, Error<T>> {
			let amount_in = T::HigherPrecisionBalance::from(*amount_in);
			let reserve_in = T::HigherPrecisionBalance::from(*reserve_in);
			let reserve_out = T::HigherPrecisionBalance::from(*reserve_out);

			if reserve_in.is_zero() || reserve_out.is_zero() {
				return Err(Error::<T>::ZeroLiquidity.into())
			}

			let amount_in_with_fee = amount_in
				.checked_mul(&(T::HigherPrecisionBalance::from(1000u32) - (T::LPFee::get().into())))
				.ok_or(Error::<T>::Overflow)?;

			let numerator =
				amount_in_with_fee.checked_mul(&reserve_out).ok_or(Error::<T>::Overflow)?;

			let denominator = reserve_in
				.checked_mul(&1000u32.into())
				.ok_or(Error::<T>::Overflow)?
				.checked_add(&amount_in_with_fee)
				.ok_or(Error::<T>::Overflow)?;

			let result = numerator.checked_div(&denominator).ok_or(Error::<T>::Overflow)?;

			result.try_into().map_err(|_| Error::<T>::Overflow)
		}

		/// Calculates amount in.
		///
		/// Given an output amount of an asset and pair reserves, returns a required input amount
		/// of the other asset.
		pub fn get_amount_in(
			amount_out: &T::Balance,
			reserve_in: &T::Balance,
			reserve_out: &T::Balance,
		) -> Result<T::Balance, Error<T>> {
			let amount_out = T::HigherPrecisionBalance::from(*amount_out);
			let reserve_in = T::HigherPrecisionBalance::from(*reserve_in);
			let reserve_out = T::HigherPrecisionBalance::from(*reserve_out);

			if reserve_in.is_zero() || reserve_out.is_zero() {
				Err(Error::<T>::ZeroLiquidity.into())?
			}

			if amount_out >= reserve_out {
				Err(Error::<T>::AmountOutTooHigh.into())?
			}

			let numerator = reserve_in
				.checked_mul(&amount_out)
				.ok_or(Error::<T>::Overflow)?
				.checked_mul(&1000u32.into())
				.ok_or(Error::<T>::Overflow)?;

			let denominator = reserve_out
				.checked_sub(&amount_out)
				.ok_or(Error::<T>::Overflow)?
				.checked_mul(&(T::HigherPrecisionBalance::from(1000u32) - T::LPFee::get().into()))
				.ok_or(Error::<T>::Overflow)?;

			let result = numerator
				.checked_div(&denominator)
				.ok_or(Error::<T>::Overflow)?
				.checked_add(&One::one())
				.ok_or(Error::<T>::Overflow)?;

			result.try_into().map_err(|_| Error::<T>::Overflow)
		}

		/// Ensure that a `value` meets the minimum balance requirements of an `asset` class.
		fn validate_minimal_amount(value: T::Balance, asset: &T::MultiAssetId) -> Result<(), ()> {
			if T::MultiAssetIdConverter::is_native(asset) {
				let ed = T::Currency::minimum_balance();
				ensure!(
					T::HigherPrecisionBalance::from(value) >= T::HigherPrecisionBalance::from(ed),
					()
				);
			} else {
				let MultiAssetIdConversionResult::Converted(asset_id) =
					T::MultiAssetIdConverter::try_convert(asset)
				else {
					return Err(())
				};
				let minimal = T::Assets::minimum_balance(asset_id);
				ensure!(value >= minimal, ());
			}
			Ok(())
		}

		/// Ensure that a path is valid.
		fn validate_swap_path(path: &Vec<T::MultiAssetId>) -> Result<(), DispatchError> {
			ensure!(path.len() >= 2, Error::<T>::InvalidPath);
			ensure!(path.len() as u32 <= T::MaxSwapPathLength::get(), Error::<T>::InvalidPath);

			// validate all the pools in the path are unique
			let mut pools = BTreeSet::<PoolIdOf<T>>::new();
			for assets_pair in path.windows(2) {
				if let [asset1, asset2] = assets_pair {
					let pool_id = Self::get_pool_id(asset1.clone(), asset2.clone());
					let new_element = pools.insert(pool_id);
					if !new_element {
						return Err(Error::<T>::NonUniquePath.into())
					}
				}
			}
			Ok(())
		}

		/// Returns the next pool asset id for benchmark purposes only.
		#[cfg(any(test, feature = "runtime-benchmarks"))]
		pub fn get_next_pool_asset_id() -> T::PoolAssetId {
			NextPoolAssetId::<T>::get()
				.or(T::PoolAssetId::initial_value())
				.expect("Next pool asset ID can not be None")
		}
	}
}

sp_api::decl_runtime_apis! {
	/// This runtime api allows people to query the size of the liquidity pools
	/// and quote prices for swaps.
	pub trait AssetConversionApi<Balance, AssetId> where
		Balance: frame_support::traits::tokens::Balance + MaybeDisplay,
		AssetId: Codec
	{
		/// Provides a quote for [`Pallet::swap_tokens_for_exact_tokens`].
		///
		/// Note that the price may have changed by the time the transaction is executed.
		/// (Use `amount_in_max` to control slippage.)
		fn quote_price_tokens_for_exact_tokens(asset1: AssetId, asset2: AssetId, amount: Balance, include_fee: bool) -> Option<Balance>;

		/// Provides a quote for [`Pallet::swap_exact_tokens_for_tokens`].
		///
		/// Note that the price may have changed by the time the transaction is executed.
		/// (Use `amount_out_min` to control slippage.)
		fn quote_price_exact_tokens_for_tokens(asset1: AssetId, asset2: AssetId, amount: Balance, include_fee: bool) -> Option<Balance>;

		/// Returns the size of the liquidity pool for the given asset pair.
		fn get_reserves(asset1: AssetId, asset2: AssetId) -> Option<(Balance, Balance)>;
	}
}

sp_core::generate_feature_enabled_macro!(runtime_benchmarks_enabled, feature = "runtime-benchmarks", $);
