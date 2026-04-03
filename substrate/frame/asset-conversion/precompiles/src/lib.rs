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

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use core::marker::PhantomData;
use frame_support::traits::Get;
use pallet_asset_conversion::weights::WeightInfo as _;
use pallet_revive::precompiles::{
	alloy::{
		self,
		sol_types::{Revert, SolCall},
	},
	AddressMatcher, Error, Ext, Precompile, H160,
};

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

alloy::sol! {
	/// Precompile interface for asset-conversion (DEX) operations.
	interface IAssetConversion {
		/// Swap an exact amount of input tokens for as many output tokens as possible.
		/// @param path Ordered list of asset addresses defining the swap route.
		/// @param amountIn Exact amount of the first asset to swap.
		/// @param amountOutMin Minimum acceptable amount of the last asset to receive.
		/// @param sendTo Address to receive the output tokens.
		/// @param keepAlive If true, ensures the sender account stays above existential deposit.
		/// @return amountOut The amount of output tokens received.
		function swapExactTokensForTokens(
			address[] calldata path,
			uint256 amountIn,
			uint256 amountOutMin,
			address sendTo,
			bool keepAlive
		) external returns (uint256 amountOut);

		/// Swap tokens to receive an exact amount of output tokens.
		/// @param path Ordered list of asset addresses defining the swap route.
		/// @param amountOut Exact amount of the last asset to receive.
		/// @param amountInMax Maximum acceptable amount of the first asset to spend.
		/// @param sendTo Address to receive the output tokens.
		/// @param keepAlive If true, ensures the sender account stays above existential deposit.
		/// @return amountIn The amount of input tokens spent.
		function swapTokensForExactTokens(
			address[] calldata path,
			uint256 amountOut,
			uint256 amountInMax,
			address sendTo,
			bool keepAlive
		) external returns (uint256 amountIn);

		/// Quote the expected output for a given exact input swap.
		/// @param asset1 Address of the input asset.
		/// @param asset2 Address of the output asset.
		/// @param amount The input amount to quote for.
		/// @param includeFee Whether to include the pool's LP fee in the quote.
		/// @return The expected output amount.
		function quoteExactTokensForTokens(
			address asset1,
			address asset2,
			uint256 amount,
			bool includeFee
		) external view returns (uint256);

		/// Quote the required input for a given exact output swap.
		/// @param asset1 Address of the input asset.
		/// @param asset2 Address of the output asset.
		/// @param amount The desired output amount to quote for.
		/// @param includeFee Whether to include the pool's LP fee in the quote.
		/// @return The required input amount.
		function quoteTokensForExactTokens(
			address asset1,
			address asset2,
			uint256 amount,
			bool includeFee
		) external view returns (uint256);
	}
}

/// Trait for converting an EVM address (H160) to the runtime's asset kind.
///
/// Runtimes must implement this to define how precompile addresses map to asset
/// identifiers used by `pallet-asset-conversion`. On Asset Hub, this would resolve
/// ERC20 precompile addresses (with prefixes like 0x0120, 0x0220) to `xcm::v5::Location`.
pub trait AddressToAssetKind {
	type AssetKind;
	fn address_to_asset_kind(address: &H160) -> Result<Self::AssetKind, Error>;
}

/// Asset conversion precompile exposing DEX swap and quote operations.
///
/// `ADDRESS` is the `u16` identifier embedded at bytes [16..18] of the precompile's H160 address.
/// `Converter` maps EVM addresses to the runtime's `AssetKind`.
pub struct AssetConversion<const ADDRESS: u16, Runtime, Converter> {
	_phantom: PhantomData<(Runtime, Converter)>,
}

impl<const ADDRESS: u16, Runtime, Converter> Precompile
	for AssetConversion<ADDRESS, Runtime, Converter>
where
	Runtime: pallet_asset_conversion::Config + pallet_revive::Config,
	Converter:
		AddressToAssetKind<AssetKind = <Runtime as pallet_asset_conversion::Config>::AssetKind>,
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

		match input {
			_ if env.is_delegate_call() => Err(Error::Revert(ERR_DELEGATE_CALL.into())),
			IAssetConversionCalls::swapExactTokensForTokens(_) |
			IAssetConversionCalls::swapTokensForExactTokens(_)
				if env.is_read_only() =>
			{
				Err(Error::Error(pallet_revive::Error::<Self::T>::StateChangeDenied.into()))
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
		}
	}
}

const ERR_INVALID_CALLER: &str = "Invalid caller";
const ERR_BALANCE_CONVERSION_FAILED: &str = "Balance conversion failed";
const ERR_POOL_NOT_FOUND: &str = "Pool does not exist or has no liquidity";
const ERR_PATH_TOO_LONG: &str = "Swap path exceeds MaxSwapPathLength";
const ERR_DELEGATE_CALL: &str = "Cannot be called via delegate call";

impl<const ADDRESS: u16, Runtime, Converter> AssetConversion<ADDRESS, Runtime, Converter>
where
	Runtime: pallet_asset_conversion::Config + pallet_revive::Config,
	Converter:
		AddressToAssetKind<AssetKind = <Runtime as pallet_asset_conversion::Config>::AssetKind>,
	alloy::primitives::U256: TryInto<<Runtime as pallet_asset_conversion::Config>::Balance>,
	alloy::primitives::U256: TryFrom<<Runtime as pallet_asset_conversion::Config>::Balance>,
{
	/// Validates that the path length does not exceed `MaxSwapPathLength` and returns it as u32.
	fn validated_path_len(path: &[alloy::primitives::Address]) -> Result<u32, Error> {
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

		let sender = env
			.caller()
			.account_id()
			.map_err(|_| Error::Revert(Revert { reason: ERR_INVALID_CALLER.into() }))?
			.clone();

		let path = call
			.path
			.iter()
			.map(|&addr| Converter::address_to_asset_kind(&H160(addr.0 .0)))
			.collect::<Result<Vec<_>, _>>()?;

		let amount_in = Self::to_balance(call.amountIn)?;
		let amount_out_min = Self::to_balance(call.amountOutMin)?;

		let send_to = env.to_account_id(&H160(call.sendTo.0 .0));

		use pallet_asset_conversion::Swap;
		let amount_out = <pallet_asset_conversion::Pallet<Runtime> as Swap<
			<Runtime as frame_system::Config>::AccountId,
		>>::swap_exact_tokens_for_tokens(
			sender,
			path,
			amount_in,
			Some(amount_out_min),
			send_to,
			call.keepAlive,
		)?;

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

		let sender = env
			.caller()
			.account_id()
			.map_err(|_| Error::Revert(Revert { reason: ERR_INVALID_CALLER.into() }))?
			.clone();

		let path = call
			.path
			.iter()
			.map(|&addr| Converter::address_to_asset_kind(&H160(addr.0 .0)))
			.collect::<Result<Vec<_>, _>>()?;

		let amount_out = Self::to_balance(call.amountOut)?;
		let amount_in_max = Self::to_balance(call.amountInMax)?;

		let send_to = env.to_account_id(&H160(call.sendTo.0 .0));

		use pallet_asset_conversion::Swap;
		let amount_in = <pallet_asset_conversion::Pallet<Runtime> as Swap<
			<Runtime as frame_system::Config>::AccountId,
		>>::swap_tokens_for_exact_tokens(
			sender,
			path,
			amount_out,
			Some(amount_in_max),
			send_to,
			call.keepAlive,
		)?;

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

		let asset1 = Converter::address_to_asset_kind(&H160(call.asset1.0 .0))?;
		let asset2 = Converter::address_to_asset_kind(&H160(call.asset2.0 .0))?;
		let amount = Self::to_balance(call.amount)?;

		use pallet_asset_conversion::QuotePrice;
		let quoted =
			<pallet_asset_conversion::Pallet<Runtime> as QuotePrice>::quote_price_exact_tokens_for_tokens(
				asset1,
				asset2,
				amount,
				call.includeFee,
			)
			.ok_or(Error::Revert(Revert {
				reason: ERR_POOL_NOT_FOUND.into(),
			}))?;

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

		let asset1 = Converter::address_to_asset_kind(&H160(call.asset1.0 .0))?;
		let asset2 = Converter::address_to_asset_kind(&H160(call.asset2.0 .0))?;
		let amount = Self::to_balance(call.amount)?;

		use pallet_asset_conversion::QuotePrice;
		let quoted = <pallet_asset_conversion::Pallet<Runtime> as QuotePrice>::quote_price_tokens_for_exact_tokens(
			asset1,
			asset2,
			amount,
			call.includeFee,
		)
		.ok_or(Error::Revert(Revert {
			reason: ERR_POOL_NOT_FOUND.into(),
		}))?;

		Ok(IAssetConversion::quoteTokensForExactTokensCall::abi_encode_returns(&Self::to_u256(
			quoted,
		)?))
	}
}
