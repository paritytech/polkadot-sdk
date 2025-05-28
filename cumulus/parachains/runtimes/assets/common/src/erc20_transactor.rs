// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! The ERC20 Asset Transactor.

use core::marker::PhantomData;
use ethereum_standards::IERC20;
use frame_support::{
	pallet_prelude::Zero,
	traits::{fungible::Inspect, OriginTrait},
};
use pallet_revive::{
	precompiles::alloy::{
		primitives::{Address, U256 as EU256},
		sol_types::SolCall,
	},
	AddressMapper, ContractResult, DepositLimit, MomentOf,
};
use sp_core::{Get, H160, H256, U256};
use sp_runtime::Weight;
use xcm::latest::prelude::*;
use xcm_executor::{
	traits::{ConvertLocation, Error as MatchError, MatchesFungibles, TransactAsset},
	AssetsInHolding,
};

type BalanceOf<T> = <<T as pallet_revive::Config>::Currency as Inspect<
	<T as frame_system::Config>::AccountId,
>>::Balance;

/// An Asset Transactor that deals with ERC20 tokens.
pub struct ERC20Transactor<
	T,
	Matcher,
	AccountIdConverter,
	GasLimit,
	StorageDepositLimit,
	AccountId,
	TransfersCheckingAccount,
>(
	PhantomData<(
		T,
		Matcher,
		AccountIdConverter,
		GasLimit,
		StorageDepositLimit,
		AccountId,
		TransfersCheckingAccount,
	)>,
);

impl<
		AccountId: Eq + Clone,
		T: pallet_revive::Config<AccountId = AccountId>,
		AccountIdConverter: ConvertLocation<AccountId>,
		Matcher: MatchesFungibles<H160, u128>,
		GasLimit: Get<Weight>,
		StorageDepositLimit: Get<BalanceOf<T>>,
		TransfersCheckingAccount: Get<AccountId>,
	> TransactAsset
	for ERC20Transactor<
		T,
		Matcher,
		AccountIdConverter,
		GasLimit,
		StorageDepositLimit,
		AccountId,
		TransfersCheckingAccount,
	>
where
	BalanceOf<T>: Into<U256> + TryFrom<U256>,
	MomentOf<T>: Into<U256>,
	T::Hash: frame_support::traits::IsType<H256>,
{
	fn can_check_in(_origin: &Location, _what: &Asset, _context: &XcmContext) -> XcmResult {
		// We don't support teleports.
		Err(XcmError::Unimplemented)
	}

	fn check_in(_origin: &Location, _what: &Asset, _context: &XcmContext) {
		// We don't support teleports.
	}

	fn can_check_out(_destination: &Location, _what: &Asset, _context: &XcmContext) -> XcmResult {
		// We don't support teleports.
		Err(XcmError::Unimplemented)
	}

	fn check_out(_destination: &Location, _what: &Asset, _context: &XcmContext) {
		// We don't support teleports.
	}

	fn withdraw_asset_with_surplus(
		what: &Asset,
		who: &Location,
		_context: Option<&XcmContext>,
	) -> Result<(AssetsInHolding, Weight), XcmError> {
		tracing::trace!(
			target: "xcm::transactor::erc20::withdraw",
			?what, ?who,
		);
		let (asset_id, amount) = Matcher::matches_fungibles(what)?;
		let who = AccountIdConverter::convert_location(who)
			.ok_or(MatchError::AccountIdConversionFailed)?;
		// We need to map the 32 byte checking account to a 20 byte account.
		let checking_account_eth = T::AddressMapper::to_address(&TransfersCheckingAccount::get());
		let checking_address = Address::from(Into::<[u8; 20]>::into(checking_account_eth));
		let gas_limit = GasLimit::get();
		// To withdraw, we actually transfer to the checking account.
		// We do this using the solidity ERC20 interface.
		let data =
			IERC20::transferCall { to: checking_address, value: EU256::from(amount) }.abi_encode();
		let ContractResult { result, gas_consumed, storage_deposit, .. } =
			pallet_revive::Pallet::<T>::bare_call(
				T::RuntimeOrigin::signed(who.clone()),
				asset_id,
				BalanceOf::<T>::zero(),
				gas_limit,
				DepositLimit::Balance(StorageDepositLimit::get()),
				data,
			);
		// We need to return this surplus for the executor to allow refunding it.
		let surplus = gas_limit.saturating_sub(gas_consumed);
		tracing::trace!(target: "xcm::transactor::erc20::withdraw", ?gas_consumed, ?surplus, ?storage_deposit);
		if let Ok(return_value) = result {
			tracing::trace!(target: "xcm::transactor::erc20::withdraw", ?return_value, "Return value by withdraw_asset");
			if return_value.did_revert() {
				tracing::debug!(target: "xcm::transactor::erc20::withdraw", "ERC20 contract reverted");
				Err(XcmError::FailedToTransactAsset("ERC20 contract reverted"))
			} else {
				let is_success = IERC20::transferCall::abi_decode_returns_validate(&return_value.data).map_err(|error| {
					tracing::debug!(target: "xcm::transactor::erc20::withdraw", ?error, "ERC20 contract result couldn't decode");
					XcmError::FailedToTransactAsset("ERC20 contract result couldn't decode")
				})?;
				if is_success {
					tracing::trace!(target: "xcm::transactor::erc20::withdraw", "ERC20 contract was successful");
					Ok((what.clone().into(), surplus))
				} else {
					tracing::debug!(target: "xcm::transactor::erc20::withdraw", "contract transfer failed");
					Err(XcmError::FailedToTransactAsset("ERC20 contract transfer failed"))
				}
			}
		} else {
			tracing::debug!(target: "xcm::transactor::erc20::withdraw", ?result, "Error");
			// This error could've been duplicate smart contract, out of gas, etc.
			// If the issue is gas, there's nothing the user can change in the XCM
			// that will make this work since there's a hardcoded gas limit.
			Err(XcmError::FailedToTransactAsset("ERC20 contract execution errored"))
		}
	}

	fn deposit_asset_with_surplus(
		what: &Asset,
		who: &Location,
		_context: Option<&XcmContext>,
	) -> Result<Weight, XcmError> {
		tracing::trace!(
			target: "xcm::transactor::erc20::deposit",
			?what, ?who,
		);
		let (asset_id, amount) = Matcher::matches_fungibles(what)?;
		let who = AccountIdConverter::convert_location(who)
			.ok_or(MatchError::AccountIdConversionFailed)?;
		// We need to map the 32 byte beneficiary account to a 20 byte account.
		let eth_address = T::AddressMapper::to_address(&who);
		let address = Address::from(Into::<[u8; 20]>::into(eth_address));
		// To deposit, we actually transfer from the checking account to the beneficiary.
		// We do this using the solidity ERC20 interface.
		let data = IERC20::transferCall { to: address, value: EU256::from(amount) }.abi_encode();
		let gas_limit = GasLimit::get();
		let ContractResult { result, gas_consumed, storage_deposit, .. } =
			pallet_revive::Pallet::<T>::bare_call(
				T::RuntimeOrigin::signed(TransfersCheckingAccount::get()),
				asset_id,
				BalanceOf::<T>::zero(),
				gas_limit,
				DepositLimit::Balance(StorageDepositLimit::get()),
				data,
			);
		// We need to return this surplus for the executor to allow refunding it.
		let surplus = gas_limit.saturating_sub(gas_consumed);
		tracing::trace!(target: "xcm::transactor::erc20::deposit", ?gas_consumed, ?surplus, ?storage_deposit);
		if let Ok(return_value) = result {
			tracing::trace!(target: "xcm::transactor::erc20::deposit", ?return_value, "Return value");
			if return_value.did_revert() {
				tracing::debug!(target: "xcm::transactor::erc20::deposit", "Contract reverted");
				Err(XcmError::FailedToTransactAsset("ERC20 contract reverted"))
			} else {
				let is_success = IERC20::transferCall::abi_decode_returns_validate(&return_value.data).map_err(|error| {
					tracing::debug!(target: "xcm::transactor::erc20::deposit", ?error, "ERC20 contract result couldn't decode");
					XcmError::FailedToTransactAsset("ERC20 contract result couldn't decode")
				})?;
				if is_success {
					tracing::trace!(target: "xcm::transactor::erc20::deposit", "ERC20 contract was successful");
					Ok(surplus)
				} else {
					tracing::debug!(target: "xcm::transactor::erc20::deposit", "contract transfer failed");
					Err(XcmError::FailedToTransactAsset("ERC20 contract transfer failed"))
				}
			}
		} else {
			tracing::debug!(target: "xcm::transactor::erc20::deposit", ?result, "Error");
			// This error could've been duplicate smart contract, out of gas, etc.
			// If the issue is gas, there's nothing the user can change in the XCM
			// that will make this work since there's a hardcoded gas limit.
			Err(XcmError::FailedToTransactAsset("ERC20 contract execution errored"))
		}
	}
}
