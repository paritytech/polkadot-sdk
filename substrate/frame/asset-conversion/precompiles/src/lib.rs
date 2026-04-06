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
use codec::Decode;
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

		// --- Location-based variants (SCALE-encoded AssetKind) ---
		// These accept assets as SCALE-encoded byte blobs instead of ERC20 precompile
		// addresses. Useful for Rust/PolkaVM contracts that work with Locations natively,
		// or when the caller already knows the encoded asset and wants to skip the
		// address-to-asset-kind lookup.

		/// Like swapExactTokensForTokens, but assets are SCALE-encoded AssetKind.
		function swapExactTokensForTokensByLocation(
			bytes[] calldata path,
			uint256 amountIn,
			uint256 amountOutMin,
			address sendTo,
			bool keepAlive
		) external returns (uint256 amountOut);

		/// Like swapTokensForExactTokens, but assets are SCALE-encoded AssetKind.
		function swapTokensForExactTokensByLocation(
			bytes[] calldata path,
			uint256 amountOut,
			uint256 amountInMax,
			address sendTo,
			bool keepAlive
		) external returns (uint256 amountIn);

		/// Like quoteExactTokensForTokens, but assets are SCALE-encoded AssetKind.
		function quoteExactTokensForTokensByLocation(
			bytes calldata asset1,
			bytes calldata asset2,
			uint256 amount,
			bool includeFee
		) external view returns (uint256);

		/// Like quoteTokensForExactTokens, but assets are SCALE-encoded AssetKind.
		function quoteTokensForExactTokensByLocation(
			bytes calldata asset1,
			bytes calldata asset2,
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
			IAssetConversionCalls::swapTokensForExactTokens(_) |
			IAssetConversionCalls::swapExactTokensForTokensByLocation(_) |
			IAssetConversionCalls::swapTokensForExactTokensByLocation(_)
				if env.is_read_only() =>
			{
				Err(Error::Error(pallet_revive::Error::<Self::T>::StateChangeDenied.into()))
			},
			// Address-based variants
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
			// Location-based variants
			IAssetConversionCalls::swapExactTokensForTokensByLocation(call) => {
				Self::swap_exact_tokens_for_tokens_by_location(call, env)
			},
			IAssetConversionCalls::swapTokensForExactTokensByLocation(call) => {
				Self::swap_tokens_for_exact_tokens_by_location(call, env)
			},
			IAssetConversionCalls::quoteExactTokensForTokensByLocation(call) => {
				Self::quote_exact_tokens_for_tokens_by_location(call, env)
			},
			IAssetConversionCalls::quoteTokensForExactTokensByLocation(call) => {
				Self::quote_tokens_for_exact_tokens_by_location(call, env)
			},
		}
	}
}

const ERR_INVALID_CALLER: &str = "Invalid caller";
const ERR_BALANCE_CONVERSION_FAILED: &str = "Balance conversion failed";
const ERR_POOL_NOT_FOUND: &str = "Pool does not exist or has no liquidity";
const ERR_PATH_TOO_LONG: &str = "Swap path exceeds MaxSwapPathLength";
const ERR_DELEGATE_CALL: &str = "Cannot be called via delegate call";
const ERR_INVALID_ASSET_ENCODING: &str = "Failed to SCALE-decode asset kind";

impl<const ADDRESS: u16, Runtime, Converter> AssetConversion<ADDRESS, Runtime, Converter>
where
	Runtime: pallet_asset_conversion::Config + pallet_revive::Config,
	Converter:
		AddressToAssetKind<AssetKind = <Runtime as pallet_asset_conversion::Config>::AssetKind>,
	alloy::primitives::U256: TryInto<<Runtime as pallet_asset_conversion::Config>::Balance>,
	alloy::primitives::U256: TryFrom<<Runtime as pallet_asset_conversion::Config>::Balance>,
{
	/// Convert ERC20 precompile addresses to asset kinds.
	fn resolve_address_path(
		path: &[alloy::primitives::Address],
	) -> Result<Vec<<Runtime as pallet_asset_conversion::Config>::AssetKind>, Error> {
		path.iter().map(|&a| Converter::address_to_asset_kind(&H160(a.0 .0))).collect()
	}

	/// SCALE-decode asset kinds from raw bytes.
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

	// --- Core swap/quote logic ---

	fn do_swap_exact_tokens_for_tokens(
		path: Vec<<Runtime as pallet_asset_conversion::Config>::AssetKind>,
		amount_in: alloy::primitives::U256,
		amount_out_min: alloy::primitives::U256,
		send_to: alloy::primitives::Address,
		keep_alive: bool,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<alloy::primitives::U256, Error> {
		let path_len = Self::validated_path_len(&path)?;
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

		let send_to = env.to_account_id(&H160(send_to.0 .0));

		use pallet_asset_conversion::Swap;
		let amount_out = <pallet_asset_conversion::Pallet<Runtime> as Swap<
			<Runtime as frame_system::Config>::AccountId,
		>>::swap_exact_tokens_for_tokens(
			sender,
			path,
			Self::to_balance(amount_in)?,
			Some(Self::to_balance(amount_out_min)?),
			send_to,
			keep_alive,
		)?;

		Self::to_u256(amount_out)
	}

	fn do_swap_tokens_for_exact_tokens(
		path: Vec<<Runtime as pallet_asset_conversion::Config>::AssetKind>,
		amount_out: alloy::primitives::U256,
		amount_in_max: alloy::primitives::U256,
		send_to: alloy::primitives::Address,
		keep_alive: bool,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<alloy::primitives::U256, Error> {
		let path_len = Self::validated_path_len(&path)?;
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

		let send_to = env.to_account_id(&H160(send_to.0 .0));

		use pallet_asset_conversion::Swap;
		let amount_in = <pallet_asset_conversion::Pallet<Runtime> as Swap<
			<Runtime as frame_system::Config>::AccountId,
		>>::swap_tokens_for_exact_tokens(
			sender,
			path,
			Self::to_balance(amount_out)?,
			Some(Self::to_balance(amount_in_max)?),
			send_to,
			keep_alive,
		)?;

		Self::to_u256(amount_in)
	}

	fn do_quote_exact_tokens_for_tokens(
		asset1: <Runtime as pallet_asset_conversion::Config>::AssetKind,
		asset2: <Runtime as pallet_asset_conversion::Config>::AssetKind,
		amount: alloy::primitives::U256,
		include_fee: bool,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<alloy::primitives::U256, Error> {
		// Quote is always a single-pair operation (the Solidity interface takes two assets,
		// not a path). The actual cost is just reserve reads + arithmetic, but no dedicated
		// benchmark exists yet. Charging the swap weight for path length 2 is a safe
		// overestimate since swaps include transfer costs that quotes do not.
		env.charge(
			<Runtime as pallet_asset_conversion::Config>::WeightInfo::swap_exact_tokens_for_tokens(
				2,
			),
		)?;

		use pallet_asset_conversion::QuotePrice;
		let quoted =
			<pallet_asset_conversion::Pallet<Runtime> as QuotePrice>::quote_price_exact_tokens_for_tokens(
				asset1,
				asset2,
				Self::to_balance(amount)?,
				include_fee,
			)
			.ok_or(Error::Revert(Revert { reason: ERR_POOL_NOT_FOUND.into() }))?;

		Self::to_u256(quoted)
	}

	fn do_quote_tokens_for_exact_tokens(
		asset1: <Runtime as pallet_asset_conversion::Config>::AssetKind,
		asset2: <Runtime as pallet_asset_conversion::Config>::AssetKind,
		amount: alloy::primitives::U256,
		include_fee: bool,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<alloy::primitives::U256, Error> {
		// See comment in do_quote_exact_tokens_for_tokens for weight rationale.
		env.charge(
			<Runtime as pallet_asset_conversion::Config>::WeightInfo::swap_tokens_for_exact_tokens(
				2,
			),
		)?;

		use pallet_asset_conversion::QuotePrice;
		let quoted =
			<pallet_asset_conversion::Pallet<Runtime> as QuotePrice>::quote_price_tokens_for_exact_tokens(
				asset1,
				asset2,
				Self::to_balance(amount)?,
				include_fee,
			)
			.ok_or(Error::Revert(Revert { reason: ERR_POOL_NOT_FOUND.into() }))?;

		Self::to_u256(quoted)
	}

	// --- Address-based entry points ---

	fn swap_exact_tokens_for_tokens(
		call: &IAssetConversion::swapExactTokensForTokensCall,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		let r = Self::do_swap_exact_tokens_for_tokens(
			Self::resolve_address_path(&call.path)?,
			call.amountIn,
			call.amountOutMin,
			call.sendTo,
			call.keepAlive,
			env,
		)?;
		Ok(IAssetConversion::swapExactTokensForTokensCall::abi_encode_returns(&r))
	}

	fn swap_tokens_for_exact_tokens(
		call: &IAssetConversion::swapTokensForExactTokensCall,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		let r = Self::do_swap_tokens_for_exact_tokens(
			Self::resolve_address_path(&call.path)?,
			call.amountOut,
			call.amountInMax,
			call.sendTo,
			call.keepAlive,
			env,
		)?;
		Ok(IAssetConversion::swapTokensForExactTokensCall::abi_encode_returns(&r))
	}

	fn quote_exact_tokens_for_tokens(
		call: &IAssetConversion::quoteExactTokensForTokensCall,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		let a1 = Converter::address_to_asset_kind(&H160(call.asset1.0 .0))?;
		let a2 = Converter::address_to_asset_kind(&H160(call.asset2.0 .0))?;
		let r = Self::do_quote_exact_tokens_for_tokens(a1, a2, call.amount, call.includeFee, env)?;
		Ok(IAssetConversion::quoteExactTokensForTokensCall::abi_encode_returns(&r))
	}

	fn quote_tokens_for_exact_tokens(
		call: &IAssetConversion::quoteTokensForExactTokensCall,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		let a1 = Converter::address_to_asset_kind(&H160(call.asset1.0 .0))?;
		let a2 = Converter::address_to_asset_kind(&H160(call.asset2.0 .0))?;
		let r = Self::do_quote_tokens_for_exact_tokens(a1, a2, call.amount, call.includeFee, env)?;
		Ok(IAssetConversion::quoteTokensForExactTokensCall::abi_encode_returns(&r))
	}

	// --- Location-based entry points ---

	fn swap_exact_tokens_for_tokens_by_location(
		call: &IAssetConversion::swapExactTokensForTokensByLocationCall,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		let r = Self::do_swap_exact_tokens_for_tokens(
			call.path.iter().map(|e| Self::decode_asset_kind(e)).collect::<Result<_, _>>()?,
			call.amountIn,
			call.amountOutMin,
			call.sendTo,
			call.keepAlive,
			env,
		)?;
		Ok(IAssetConversion::swapExactTokensForTokensByLocationCall::abi_encode_returns(&r))
	}

	fn swap_tokens_for_exact_tokens_by_location(
		call: &IAssetConversion::swapTokensForExactTokensByLocationCall,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		let r = Self::do_swap_tokens_for_exact_tokens(
			call.path.iter().map(|e| Self::decode_asset_kind(e)).collect::<Result<_, _>>()?,
			call.amountOut,
			call.amountInMax,
			call.sendTo,
			call.keepAlive,
			env,
		)?;
		Ok(IAssetConversion::swapTokensForExactTokensByLocationCall::abi_encode_returns(&r))
	}

	fn quote_exact_tokens_for_tokens_by_location(
		call: &IAssetConversion::quoteExactTokensForTokensByLocationCall,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		let a1 = Self::decode_asset_kind(&call.asset1)?;
		let a2 = Self::decode_asset_kind(&call.asset2)?;
		let r = Self::do_quote_exact_tokens_for_tokens(a1, a2, call.amount, call.includeFee, env)?;
		Ok(IAssetConversion::quoteExactTokensForTokensByLocationCall::abi_encode_returns(&r))
	}

	fn quote_tokens_for_exact_tokens_by_location(
		call: &IAssetConversion::quoteTokensForExactTokensByLocationCall,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		let a1 = Self::decode_asset_kind(&call.asset1)?;
		let a2 = Self::decode_asset_kind(&call.asset2)?;
		let r = Self::do_quote_tokens_for_exact_tokens(a1, a2, call.amount, call.includeFee, env)?;
		Ok(IAssetConversion::quoteTokensForExactTokensByLocationCall::abi_encode_returns(&r))
	}
}
