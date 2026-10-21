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

//! Precompile exposing `pallet-asset-conversion` (Asset Hub DEX) to Solidity contracts.
//!
//! Allows smart contracts to swap tokens through Asset Hub's on-chain DEX and query
//! swap prices. The primary use case is contracts that accept payment in one asset
//! (e.g. USDC) and convert it to DOT or PUSD before using it.
//!
//! Assets are identified by their SCALE-encoded `AssetKind` passed as `bytes`.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use codec::Decode;
use core::marker::PhantomData;
use frame_support::traits::{Get, PalletInfoAccess};
use pallet_asset_conversion::{
	weights::WeightInfo as _, AddLiquidityAsset, MutateLiquidity, QuotePrice, Swap,
};
use pallet_revive::precompiles::{
	alloy::{
		self,
		sol_types::{Revert, SolCall},
	},
	AddressMatcher, Error, Ext, Precompile, H160,
};
use sp_runtime::{traits::Zero, DispatchError, TokenError};

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

alloy::sol! {
	/// Precompile interface for asset-conversion (DEX) operations.
	///
	/// Assets are identified by their SCALE-encoded AssetKind (e.g. xcm::v5::Location)
	/// passed as `bytes`. Contracts can hardcode these as constants or obtain them
	/// off-chain.
	interface IAssetConversion {
		/// Swap an exact amount of input tokens for as many output tokens as possible.
		/// @param path Ordered list of SCALE-encoded asset identifiers defining the swap route.
		/// @param amountIn Exact amount of the first asset to swap.
		/// @param amountOutMin Minimum acceptable amount of the last asset to receive.
		/// @param sendTo Address to receive the output tokens.
		/// @param keepAlive If true, ensures the sender account stays above existential deposit.
		/// @return amountOut The amount of output tokens received.
		function swapExactTokensForTokens(
			bytes[] calldata path,
			uint256 amountIn,
			uint256 amountOutMin,
			address sendTo,
			bool keepAlive
		) external returns (uint256 amountOut);

		/// Swap tokens to receive an exact amount of output tokens.
		/// @param path Ordered list of SCALE-encoded asset identifiers defining the swap route.
		/// @param amountOut Exact amount of the last asset to receive.
		/// @param amountInMax Maximum acceptable amount of the first asset to spend.
		/// @param sendTo Address to receive the output tokens.
		/// @param keepAlive If true, ensures the sender account stays above existential deposit.
		/// @return amountIn The amount of input tokens spent.
		function swapTokensForExactTokens(
			bytes[] calldata path,
			uint256 amountOut,
			uint256 amountInMax,
			address sendTo,
			bool keepAlive
		) external returns (uint256 amountIn);

		/// Quote the expected output for a given exact input swap.
		/// @param asset1 SCALE-encoded identifier of the input asset.
		/// @param asset2 SCALE-encoded identifier of the output asset.
		/// @param amount The input amount to quote for.
		/// @param includeFee Whether to include the pool's LP fee in the quote.
		/// @return The expected output amount.
		function quoteExactTokensForTokens(
			bytes calldata asset1,
			bytes calldata asset2,
			uint256 amount,
			bool includeFee
		) external view returns (uint256);

		/// Quote the required input for a given exact output swap.
		/// @param asset1 SCALE-encoded identifier of the input asset.
		/// @param asset2 SCALE-encoded identifier of the output asset.
		/// @param amount The desired output amount to quote for.
		/// @param includeFee Whether to include the pool's LP fee in the quote.
		/// @return The required input amount.
		function quoteTokensForExactTokens(
			bytes calldata asset1,
			bytes calldata asset2,
			uint256 amount,
			bool includeFee
		) external view returns (uint256);

		/// Create an empty liquidity pool for the given asset pair.
		/// @param asset1 SCALE-encoded identifier of the first asset.
		/// @param asset2 SCALE-encoded identifier of the second asset.
		function createPool(
			bytes calldata asset1,
			bytes calldata asset2
		) external;

		/// Add liquidity to an existing pool.
		/// @param asset1 SCALE-encoded identifier of the first asset.
		/// @param asset2 SCALE-encoded identifier of the second asset.
		/// @param amount1Desired Desired amount of the first asset to add.
		/// @param amount2Desired Desired amount of the second asset to add.
		/// @param amount1Min Minimum acceptable amount of the first asset.
		/// @param amount2Min Minimum acceptable amount of the second asset.
		/// @param mintTo Address to receive the LP tokens.
		/// @return lpTokensMinted The amount of LP tokens minted.
		function addLiquidity(
			bytes calldata asset1,
			bytes calldata asset2,
			uint256 amount1Desired,
			uint256 amount2Desired,
			uint256 amount1Min,
			uint256 amount2Min,
			address mintTo
		) external returns (uint256 lpTokensMinted);

		/// Remove liquidity from a pool.
		/// @param asset1 SCALE-encoded identifier of the first asset.
		/// @param asset2 SCALE-encoded identifier of the second asset.
		/// @param lpTokenBurn Amount of LP tokens to burn.
		/// @param amount1MinReceive Minimum amount of the first asset to receive.
		/// @param amount2MinReceive Minimum amount of the second asset to receive.
		/// @param withdrawTo Address to receive the withdrawn assets.
		/// @return amount1 The amount of the first asset withdrawn.
		/// @return amount2 The amount of the second asset withdrawn.
		function removeLiquidity(
			bytes calldata asset1,
			bytes calldata asset2,
			uint256 lpTokenBurn,
			uint256 amount1MinReceive,
			uint256 amount2MinReceive,
			address withdrawTo
		) external returns (uint256 amount1, uint256 amount2);

		/// Get the reserves (token balances) of a liquidity pool.
		/// @param asset1 SCALE-encoded identifier of the first asset.
		/// @param asset2 SCALE-encoded identifier of the second asset.
		/// @return reserve1 The balance of asset1 in the pool.
		/// @return reserve2 The balance of asset2 in the pool.
		function getReserves(
			bytes calldata asset1,
			bytes calldata asset2
		) external view returns (uint256 reserve1, uint256 reserve2);
	}
}

/// Asset conversion precompile exposing DEX swap and quote operations.
///
/// `ADDRESS` is the `u16` identifier embedded at bytes [16..18] of the precompile's H160 address.
pub struct AssetConversion<const ADDRESS: u16, Runtime> {
	_phantom: PhantomData<Runtime>,
}

impl<const ADDRESS: u16, Runtime> Precompile for AssetConversion<ADDRESS, Runtime>
where
	Runtime: pallet_asset_conversion::Config + pallet_revive::Config,
	alloy::primitives::U256: TryInto<<Runtime as pallet_asset_conversion::Config>::Balance>,
	alloy::primitives::U256: TryFrom<<Runtime as pallet_asset_conversion::Config>::Balance>,
{
	type T = Runtime;
	type Interface = IAssetConversion::IAssetConversionCalls;
	const MATCHER: AddressMatcher =
		AddressMatcher::Fixed(core::num::NonZero::new(ADDRESS).unwrap());
	const HAS_CONTRACT_INFO: bool = false;

	fn call(
		_address: &[u8; 20],
		input: &Self::Interface,
		env: &mut impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		use IAssetConversion::IAssetConversionCalls;

		if env.is_delegate_call() {
			return Err(Error::try_to_revert::<Self::T>(
				pallet_revive::Error::<Self::T>::PrecompileDelegateDenied.into(),
			));
		}

		match input {
			IAssetConversionCalls::swapExactTokensForTokens(_) |
			IAssetConversionCalls::swapTokensForExactTokens(_) |
			IAssetConversionCalls::createPool(_) |
			IAssetConversionCalls::addLiquidity(_) |
			IAssetConversionCalls::removeLiquidity(_)
				if env.is_read_only() =>
			{
				Err(Error::Revert(Revert { reason: ERR_STATE_CHANGE_DENIED.into() }))
			},
			IAssetConversionCalls::swapExactTokensForTokens(call) => {
				Self::swap_exact_tokens_for_tokens(call, env)
			},
			IAssetConversionCalls::swapTokensForExactTokens(call) => {
				Self::swap_tokens_for_exact_tokens(call, env)
			},
			IAssetConversionCalls::quoteExactTokensForTokens(call) => {
				Self::quote_exact_tokens_for_tokens(call, env)
			},
			IAssetConversionCalls::quoteTokensForExactTokens(call) => {
				Self::quote_tokens_for_exact_tokens(call, env)
			},
			IAssetConversionCalls::createPool(call) => Self::create_pool(call, env),
			IAssetConversionCalls::addLiquidity(call) => Self::add_liquidity(call, env),
			IAssetConversionCalls::removeLiquidity(call) => Self::remove_liquidity(call, env),
			IAssetConversionCalls::getReserves(call) => Self::get_reserves(call, env),
		}
	}
}

const ERR_INVALID_CALLER: &str = "Invalid caller";
const ERR_BALANCE_CONVERSION_FAILED: &str = "Balance conversion failed";
const ERR_INVALID_ASSET_PAIR: &str = "Invalid asset pair";
const ERR_POOL_EMPTY: &str = "Pool exists but has no liquidity";
const ERR_INSUFFICIENT_LIQUIDITY: &str = "Insufficient liquidity";
const ERR_STATE_CHANGE_DENIED: &str = "cannot modify state in a static call";
const ERR_UNEXPECTED: &str = "Unexpected error";
const ERR_PATH_TOO_LONG: &str = "Swap path exceeds MaxSwapPathLength";
const ERR_INVALID_ASSET_ENCODING: &str = "Failed to SCALE-decode asset kind";
/// In a swap, `Token(BelowMinimum)` comes from both the sender's and the pool's withdraw, so the
/// reason cannot name the account. Only swaps use it; see `revert_swap`.
const ERR_INEXACT_WITHDRAW: &str = "Swap amount cannot be withdrawn exactly";

impl<const ADDRESS: u16, Runtime> AssetConversion<ADDRESS, Runtime>
where
	Runtime: pallet_asset_conversion::Config + pallet_revive::Config,
	alloy::primitives::U256: TryInto<<Runtime as pallet_asset_conversion::Config>::Balance>,
	alloy::primitives::U256: TryFrom<<Runtime as pallet_asset_conversion::Config>::Balance>,
{
	/// Returns the caller's account ID.
	fn caller_account_id(
		env: &impl Ext<T = Runtime>,
	) -> Result<<Runtime as frame_system::Config>::AccountId, Error> {
		env.caller()
			.account_id()
			.map_err(|_| Error::Revert(Revert { reason: ERR_INVALID_CALLER.into() }))
			.cloned()
	}

	/// SCALE-decode a single asset kind from raw bytes.
	fn decode_asset_kind(
		data: &[u8],
	) -> Result<<Runtime as pallet_asset_conversion::Config>::AssetKind, Error> {
		<Runtime as pallet_asset_conversion::Config>::AssetKind::decode(&mut &data[..])
			.map_err(|_| Error::Revert(Revert { reason: ERR_INVALID_ASSET_ENCODING.into() }))
	}

	/// Validates that the path length does not exceed `MaxSwapPathLength` and returns it as u32.
	fn validated_path_len<T>(path: &[T]) -> Result<u32, Error> {
		let len = path.len() as u32;
		let max = <Runtime as pallet_asset_conversion::Config>::MaxSwapPathLength::get();
		if len > max {
			return Err(Error::Revert(Revert { reason: ERR_PATH_TOO_LONG.into() }));
		}
		Ok(len)
	}

	fn to_balance(
		value: alloy::primitives::U256,
	) -> Result<<Runtime as pallet_asset_conversion::Config>::Balance, Error> {
		value
			.try_into()
			.map_err(|_| Error::Revert(Revert { reason: ERR_BALANCE_CONVERSION_FAILED.into() }))
	}

	fn to_u256(
		value: <Runtime as pallet_asset_conversion::Config>::Balance,
	) -> Result<alloy::primitives::U256, Error> {
		alloy::primitives::U256::try_from(value)
			.map_err(|_| Error::Revert(Revert { reason: ERR_BALANCE_CONVERSION_FAILED.into() }))
	}

	/// Every pallet or token failure of a state-changing call is a Solidity revert.
	/// `Error::Error` comes from the blanket `From<DispatchError>`; only charging
	/// failures should reach it.
	fn revert_dispatch(e: DispatchError) -> Error {
		Error::Revert(Revert { reason: Self::dispatch_reason(e).into() })
	}

	/// Like `revert_dispatch`, but reports `Token(BelowMinimum)` as an inexact withdraw.
	/// Other calls hit that error for other reasons, e.g. `removeLiquidity` paying less than
	/// the minimum balance to a new account, so they keep the plain token reason.
	fn revert_swap(e: DispatchError) -> Error {
		let reason = match e {
			DispatchError::Token(TokenError::BelowMinimum) => ERR_INEXACT_WITHDRAW,
			e => Self::dispatch_reason(e),
		};
		Error::Revert(Revert { reason: reason.into() })
	}

	fn dispatch_reason(e: DispatchError) -> &'static str {
		match e {
			DispatchError::Token(token) => token.into(),
			DispatchError::Arithmetic(arith) => arith.into(),
			DispatchError::Other(msg) => msg,
			DispatchError::Module(module) => match Self::decode_pallet_error(e) {
				Some(err) => Self::pallet_reason(err),
				None => module.message.unwrap_or(ERR_UNEXPECTED),
			},
			_ => ERR_UNEXPECTED,
		}
	}

	fn decode_pallet_error(e: DispatchError) -> Option<pallet_asset_conversion::Error<Runtime>> {
		let DispatchError::Module(module) = e else { return None };
		let index = <pallet_asset_conversion::Pallet<Runtime> as PalletInfoAccess>::index() as u8;
		if module.index != index {
			return None;
		}
		pallet_asset_conversion::Error::<Runtime>::decode(&mut &module.error[..]).ok()
	}

	fn pallet_reason(err: pallet_asset_conversion::Error<Runtime>) -> &'static str {
		use pallet_asset_conversion::Error::*;
		match err {
			InvalidAssetPair => ERR_INVALID_ASSET_PAIR,
			PoolExists => "Pool already exists",
			WrongDesiredAmount => "Desired amount can't be zero",
			AmountOneLessThanMinimal => "Amount one is below the minimum",
			AmountTwoLessThanMinimal => "Amount two is below the minimum",
			ReserveLeftLessThanMinimal => "Reserves would fall below the minimum",
			AmountOutTooHigh => "Amount out equals the pool reserve",
			PoolNotFound => "Pool does not exist",
			Overflow => "Arithmetic overflow",
			AssetOneDepositDidNotMeetMinimum => "Asset one deposit is below the minimum",
			AssetTwoDepositDidNotMeetMinimum => "Asset two deposit is below the minimum",
			AssetOneWithdrawalDidNotMeetMinimum => "Asset one withdrawal is below the minimum",
			AssetTwoWithdrawalDidNotMeetMinimum => "Asset two withdrawal is below the minimum",
			OptimalAmountLessThanDesired => "Optimal amount is less than desired",
			InsufficientLiquidityMinted => "Insufficient liquidity minted",
			ZeroLiquidity => "Liquidity amount can't be zero",
			ZeroAmount => "Amount can't be zero",
			ProvidedMinimumNotSufficientForSwap => "Amount out is below the provided minimum",
			ProvidedMaximumNotSufficientForSwap => "Amount in exceeds the provided maximum",
			InvalidPath => "Swap path must contain at least two assets",
			NonUniquePath => "Swap path must contain unique assets",
			IncorrectPoolAssetId => "Could not allocate a pool asset id",
			BelowMinimum => "Destination account cannot exist with the swapped funds",
			PoolEmpty => ERR_POOL_EMPTY,
			FeeTooHigh => "Swap fee exceeds the maximum",
		}
	}

	fn swap_exact_tokens_for_tokens(
		call: &IAssetConversion::swapExactTokensForTokensCall,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		let path_len = Self::validated_path_len(&call.path)?;
		env.charge(
			<Runtime as pallet_asset_conversion::Config>::WeightInfo::swap_exact_tokens_for_tokens(
				path_len,
			),
		)?;
		let path: Vec<_> =
			call.path.iter().map(|e| Self::decode_asset_kind(e)).collect::<Result<_, _>>()?;

		let sender = Self::caller_account_id(env)?;
		let send_to = env.to_account_id(&H160(call.sendTo.0 .0));
		let amount_in = Self::to_balance(call.amountIn)?;

		let amount_out = <pallet_asset_conversion::Pallet<Runtime> as Swap<
			<Runtime as frame_system::Config>::AccountId,
		>>::swap_exact_tokens_for_tokens(
			sender,
			path,
			amount_in,
			Some(Self::to_balance(call.amountOutMin)?),
			send_to,
			call.keepAlive,
		)
		.map_err(Self::revert_swap)?;

		Ok(IAssetConversion::swapExactTokensForTokensCall::abi_encode_returns(&Self::to_u256(
			amount_out,
		)?))
	}

	fn swap_tokens_for_exact_tokens(
		call: &IAssetConversion::swapTokensForExactTokensCall,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		let path_len = Self::validated_path_len(&call.path)?;
		env.charge(
			<Runtime as pallet_asset_conversion::Config>::WeightInfo::swap_tokens_for_exact_tokens(
				path_len,
			),
		)?;
		let path: Vec<_> =
			call.path.iter().map(|e| Self::decode_asset_kind(e)).collect::<Result<_, _>>()?;

		let sender = Self::caller_account_id(env)?;
		let send_to = env.to_account_id(&H160(call.sendTo.0 .0));
		let amount_out = Self::to_balance(call.amountOut)?;

		let amount_in = <pallet_asset_conversion::Pallet<Runtime> as Swap<
			<Runtime as frame_system::Config>::AccountId,
		>>::swap_tokens_for_exact_tokens(
			sender,
			path,
			amount_out,
			Some(Self::to_balance(call.amountInMax)?),
			send_to,
			call.keepAlive,
		)
		.map_err(Self::revert_swap)?;

		Ok(IAssetConversion::swapTokensForExactTokensCall::abi_encode_returns(&Self::to_u256(
			amount_in,
		)?))
	}

	fn quote_exact_tokens_for_tokens(
		call: &IAssetConversion::quoteExactTokensForTokensCall,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		// Quote is always a single-pair operation (the Solidity interface takes two assets,
		// not a path). The actual cost is just reserve reads + arithmetic, but no dedicated
		// benchmark exists yet. Charging the swap weight for path length 2 is a safe
		// overestimate since swaps include transfer costs that quotes do not.
		env.charge(
			<Runtime as pallet_asset_conversion::Config>::WeightInfo::swap_exact_tokens_for_tokens(
				2,
			),
		)?;

		let asset1 = Self::decode_asset_kind(&call.asset1)?;
		let asset2 = Self::decode_asset_kind(&call.asset2)?;
		let amount = Self::to_balance(call.amount)?;

		let quoted =
			<pallet_asset_conversion::Pallet<Runtime> as QuotePrice>::quote_price_exact_tokens_for_tokens(
				asset1.clone(),
				asset2.clone(),
				amount,
				call.includeFee,
			)
			.ok_or_else(|| Self::quote_failure(&asset1, &asset2, amount, env))?;

		Ok(IAssetConversion::quoteExactTokensForTokensCall::abi_encode_returns(&Self::to_u256(
			quoted,
		)?))
	}

	fn quote_tokens_for_exact_tokens(
		call: &IAssetConversion::quoteTokensForExactTokensCall,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		// See comment in quote_exact_tokens_for_tokens for weight rationale.
		env.charge(
			<Runtime as pallet_asset_conversion::Config>::WeightInfo::swap_tokens_for_exact_tokens(
				2,
			),
		)?;

		let asset1 = Self::decode_asset_kind(&call.asset1)?;
		let asset2 = Self::decode_asset_kind(&call.asset2)?;
		let amount = Self::to_balance(call.amount)?;

		let quoted =
			<pallet_asset_conversion::Pallet<Runtime> as QuotePrice>::quote_price_tokens_for_exact_tokens(
				asset1.clone(),
				asset2.clone(),
				amount,
				call.includeFee,
			)
			.ok_or_else(|| Self::quote_failure(&asset1, &asset2, amount, env))?;

		Ok(IAssetConversion::quoteTokensForExactTokensCall::abi_encode_returns(&Self::to_u256(
			quoted,
		)?))
	}

	fn create_pool(
		call: &IAssetConversion::createPoolCall,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		env.charge(<Runtime as pallet_asset_conversion::Config>::WeightInfo::create_pool())?;

		let asset1 = Self::decode_asset_kind(&call.asset1)?;
		let asset2 = Self::decode_asset_kind(&call.asset2)?;

		let sender = Self::caller_account_id(env)?;

		<pallet_asset_conversion::Pallet<Runtime> as MutateLiquidity<
			<Runtime as frame_system::Config>::AccountId,
		>>::create_pool(&sender, asset1, asset2)
		.map_err(Self::revert_dispatch)?;

		Ok(Vec::new())
	}

	fn add_liquidity(
		call: &IAssetConversion::addLiquidityCall,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		env.charge(<Runtime as pallet_asset_conversion::Config>::WeightInfo::add_liquidity())?;

		let asset1 = Self::decode_asset_kind(&call.asset1)?;
		let asset2 = Self::decode_asset_kind(&call.asset2)?;

		let sender = Self::caller_account_id(env)?;
		let mint_to = env.to_account_id(&H160(call.mintTo.0 .0));

		let lp_tokens = <pallet_asset_conversion::Pallet<Runtime> as MutateLiquidity<
			<Runtime as frame_system::Config>::AccountId,
		>>::add_liquidity(
			&sender,
			AddLiquidityAsset {
				asset: asset1,
				amount_desired: Self::to_balance(call.amount1Desired)?,
				amount_min: Self::to_balance(call.amount1Min)?,
			},
			AddLiquidityAsset {
				asset: asset2,
				amount_desired: Self::to_balance(call.amount2Desired)?,
				amount_min: Self::to_balance(call.amount2Min)?,
			},
			&mint_to,
		)
		.map_err(Self::revert_dispatch)?;

		Ok(IAssetConversion::addLiquidityCall::abi_encode_returns(&Self::to_u256(lp_tokens)?))
	}

	fn remove_liquidity(
		call: &IAssetConversion::removeLiquidityCall,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		env.charge(<Runtime as pallet_asset_conversion::Config>::WeightInfo::remove_liquidity())?;

		let asset1 = Self::decode_asset_kind(&call.asset1)?;
		let asset2 = Self::decode_asset_kind(&call.asset2)?;

		let sender = Self::caller_account_id(env)?;
		let withdraw_to = env.to_account_id(&H160(call.withdrawTo.0 .0));

		let (amount1, amount2) = <pallet_asset_conversion::Pallet<Runtime> as MutateLiquidity<
			<Runtime as frame_system::Config>::AccountId,
		>>::remove_liquidity(
			&sender,
			asset1,
			asset2,
			Self::to_balance(call.lpTokenBurn)?,
			Self::to_balance(call.amount1MinReceive)?,
			Self::to_balance(call.amount2MinReceive)?,
			&withdraw_to,
		)
		.map_err(Self::revert_dispatch)?;

		Ok(IAssetConversion::removeLiquidityCall::abi_encode_returns(
			&IAssetConversion::removeLiquidityReturn {
				amount1: Self::to_u256(amount1)?,
				amount2: Self::to_u256(amount2)?,
			},
		))
	}

	fn get_reserves(
		call: &IAssetConversion::getReservesCall,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		env.charge(<Runtime as pallet_asset_conversion::Config>::WeightInfo::get_reserves())?;

		let asset1 = Self::decode_asset_kind(&call.asset1)?;
		let asset2 = Self::decode_asset_kind(&call.asset2)?;

		let (reserve1, reserve2) = Self::reserves(&asset1, &asset2, env)?;

		Ok(IAssetConversion::getReservesCall::abi_encode_returns(
			&IAssetConversion::getReservesReturn {
				reserve1: Self::to_u256(reserve1)?,
				reserve2: Self::to_u256(reserve2)?,
			},
		))
	}

	/// Read pool reserves, telling a missing pool apart from an empty one.
	///
	/// `Pallet::get_reserves` reports both as `PoolEmpty`, because both have a zero
	/// balance. `Pools` is only read in that case, and `pool_exists` is charged before it is.
	/// The caller has already paid for the reserve reads themselves.
	fn reserves(
		asset1: &<Runtime as pallet_asset_conversion::Config>::AssetKind,
		asset2: &<Runtime as pallet_asset_conversion::Config>::AssetKind,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<
		(
			<Runtime as pallet_asset_conversion::Config>::Balance,
			<Runtime as pallet_asset_conversion::Config>::Balance,
		),
		Error,
	> {
		let err = match pallet_asset_conversion::Pallet::<Runtime>::get_reserves(
			asset1.clone(),
			asset2.clone(),
		) {
			Ok(reserves) => return Ok(reserves),
			Err(pallet_asset_conversion::Error::PoolEmpty) => {
				env.charge(
					<Runtime as pallet_asset_conversion::Config>::WeightInfo::pool_exists(),
				)?;
				match pallet_asset_conversion::Pallet::<Runtime>::pool_exists(asset1, asset2) {
					Ok(true) => pallet_asset_conversion::Error::PoolEmpty,
					Ok(false) => pallet_asset_conversion::Error::PoolNotFound,
					Err(e) => e,
				}
			},
			Err(e) => e,
		};
		Err(Self::reserves_error(err))
	}

	/// Map a reserves failure to the same revert `getReserves` returns.
	///
	/// The expected variants are listed first. The rest stay exhaustive so a new
	/// pallet error fails compilation here instead of collapsing into one string.
	fn reserves_error(e: pallet_asset_conversion::Error<Runtime>) -> Error {
		use pallet_asset_conversion::Error::*;
		let reason = match e {
			InvalidAssetPair | PoolNotFound | PoolEmpty => Self::pallet_reason(e),
			PoolExists |
			WrongDesiredAmount |
			AmountOneLessThanMinimal |
			AmountTwoLessThanMinimal |
			ReserveLeftLessThanMinimal |
			AmountOutTooHigh |
			Overflow |
			AssetOneDepositDidNotMeetMinimum |
			AssetTwoDepositDidNotMeetMinimum |
			AssetOneWithdrawalDidNotMeetMinimum |
			AssetTwoWithdrawalDidNotMeetMinimum |
			OptimalAmountLessThanDesired |
			InsufficientLiquidityMinted |
			ZeroLiquidity |
			ZeroAmount |
			ProvidedMinimumNotSufficientForSwap |
			ProvidedMaximumNotSufficientForSwap |
			InvalidPath |
			NonUniquePath |
			IncorrectPoolAssetId |
			BelowMinimum |
			FeeTooHigh => {
				frame_support::defensive!("get_reserves returned unexpected error");
				ERR_UNEXPECTED
			},
		};
		Error::Revert(Revert { reason: reason.into() })
	}

	/// Why `QuotePrice` returned `None`.
	///
	/// A zero amount is rejected before the pool is read. Reserve failures reuse
	/// `reserves_error`, so a missing pool, an empty pool and an invalid pair report
	/// the same reasons as `getReserves`. Any other `None` means the reserves cannot
	/// fill the quote.
	fn quote_failure(
		asset1: &<Runtime as pallet_asset_conversion::Config>::AssetKind,
		asset2: &<Runtime as pallet_asset_conversion::Config>::AssetKind,
		amount: <Runtime as pallet_asset_conversion::Config>::Balance,
		env: &mut impl Ext<T = Runtime>,
	) -> Error {
		if amount.is_zero() {
			return Error::Revert(Revert {
				reason: Self::pallet_reason(pallet_asset_conversion::Error::ZeroAmount).into(),
			});
		}
		match Self::reserves(asset1, asset2, env) {
			Err(e) => e,
			Ok(_) => Error::Revert(Revert { reason: ERR_INSUFFICIENT_LIQUIDITY.into() }),
		}
	}
}
