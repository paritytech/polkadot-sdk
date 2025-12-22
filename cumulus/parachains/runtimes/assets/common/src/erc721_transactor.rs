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

//! The ERC721 Asset Transactor.

use core::marker::PhantomData;
use ethereum_standards::IERC721;
use frame_support::traits::{fungible::Inspect, OriginTrait};
use pallet_revive::{
	precompiles::alloy::{primitives::Address, sol_types::SolCall},
	AddressMapper, ContractResult, DepositLimit, MomentOf,
};
use sp_core::{Get, H160, H256, U256}; // keep this for U256::zero()

use sp_runtime::Weight;
use xcm::latest::prelude::*;
use xcm_executor::{
	traits::{ConvertLocation, Error as MatchError, MatchesNonFungibles, TransactAsset},
	AssetsInHolding,
};
type BalanceOf<T> = <<T as pallet_revive::Config>::Currency as Inspect<
	<T as frame_system::Config>::AccountId,
>>::Balance;

use pallet_revive::precompiles::alloy::primitives::U256 as AlloyU256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpAlloyU256(pub U256);

impl From<U256> for SpAlloyU256 {
	fn from(value: U256) -> Self {
		SpAlloyU256(value)
	}
}

impl From<SpAlloyU256> for AlloyU256 {
	fn from(value: SpAlloyU256) -> Self {
		let bytes: [u8; 32] = value.0.to_big_endian();
		Self::from_be_bytes(bytes)
	}
}

impl From<AlloyU256> for SpAlloyU256 {
	fn from(value: AlloyU256) -> Self {
		let bytes: [u8; 32] = value.to_be_bytes();
		SpAlloyU256(U256::from_big_endian(&bytes))
	}
}

fn asset_instance_to_u256(inst: &AssetInstance) -> Result<U256, XcmError> {
	match inst {
		AssetInstance::Undefined => Err(XcmError::FailedToTransactAsset("Undefined AssetInstance")),
		AssetInstance::Index(index) => Ok(U256::from(*index)),
		AssetInstance::Array4(arr) => Ok(U256::from_big_endian(&arr[..])),
		AssetInstance::Array8(arr) => Ok(U256::from_big_endian(&arr[..])),
		AssetInstance::Array16(arr) => Ok(U256::from_big_endian(&arr[..])),
		AssetInstance::Array32(arr) => Ok(U256::from_big_endian(&arr[..])),
	}
}

/// An Asset Transactor that deals with ERC721 tokens.
pub struct ERC721Transactor<
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
		Matcher: MatchesNonFungibles<H160, AssetInstance>,
		GasLimit: Get<Weight>,
		StorageDepositLimit: Get<BalanceOf<T>>,
		TransfersCheckingAccount: Get<AccountId>,
	> TransactAsset
	for ERC721Transactor<
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
		Err(XcmError::Unimplemented)
	}

	fn check_in(_origin: &Location, _what: &Asset, _context: &XcmContext) {}

	fn can_check_out(_destination: &Location, _what: &Asset, _context: &XcmContext) -> XcmResult {
		Err(XcmError::Unimplemented)
	}

	fn check_out(_destination: &Location, _what: &Asset, _context: &XcmContext) {}

	fn withdraw_asset_with_surplus(
		what: &Asset,
		who: &Location,
		_context: Option<&XcmContext>,
	) -> Result<(AssetsInHolding, Weight), XcmError> {
		tracing::trace!(
				target: "xcm::transactor::erc721::withdraw",
			?what, ?who
		);

		let (asset_id, instance) = Matcher::matches_nonfungibles(what)?;
		let token_id = asset_instance_to_u256(&instance)?;

		let who = AccountIdConverter::convert_location(who)
			.ok_or(MatchError::AccountIdConversionFailed)?;

		// We need to map the 32 byte checking account to a 20 byte account.
		let checking_account_eth = T::AddressMapper::to_address(&TransfersCheckingAccount::get());
		let checking_address = Address::from(Into::<[u8; 20]>::into(checking_account_eth));

		let caller_eth = T::AddressMapper::to_address(&who);
		let caller_address = Address::from(Into::<[u8; 20]>::into(caller_eth));

		let token_id_alloy: AlloyU256 = SpAlloyU256(token_id).into();
		let data = IERC721::transferFromCall {
			from: caller_address,
			to: checking_address,
			tokenId: token_id_alloy,
		}
		.abi_encode();

		let gas_limit = GasLimit::get();
		let ContractResult { result, gas_consumed, storage_deposit, .. } =
			pallet_revive::Pallet::<T>::bare_call(
				T::RuntimeOrigin::signed(who.clone()),
				asset_id,
				U256::zero(),
				gas_limit,
				DepositLimit::Balance(StorageDepositLimit::get()),
				data,
			);

		// We need to return this surplus for the executor to allow refunding it.
		let surplus = gas_limit.saturating_sub(gas_consumed);
		tracing::trace!(target: "xcm::transactor::erc721::withdraw", ?gas_consumed, ?surplus, ?storage_deposit);

		if let Ok(return_value) = result {
			tracing::trace!(target: "xcm::transactor::erc721::withdraw", ?return_value, "Return value by withdraw_asset");
			if return_value.did_revert() {
				tracing::debug!(target: "xcm::transactor::erc721::withdraw", "ERC721 contract reverted");
				Err(XcmError::FailedToTransactAsset("ERC721 contract reverted"))
			} else if !return_value.data.is_empty() {
				tracing::debug!(target: "xcm::transactor::erc721::withdraw", ?return_value, "Unexpected data");
				Err(XcmError::FailedToTransactAsset("ERC721 returned unexpected data"))
			} else {
				// ERC721 transferFrom does not return a value.
				// Success is determined by the absence of a revert.
				tracing::trace!(
					target: "xcm::transactor::erc721::withdraw",
					"ERC721 transferFrom executed successfully"
				);
				Ok((what.clone().into(), surplus))
			}
		} else {
			tracing::debug!(target: "xcm::transactor::erc721::withdraw", ?result, "Error");
			Err(XcmError::FailedToTransactAsset("ERC721 contract execution errored"))
		}
	}
	fn deposit_asset_with_surplus(
		what: &Asset,
		who: &Location,
		_context: Option<&XcmContext>,
	) -> Result<Weight, XcmError> {
		tracing::trace!(
			target: "xcm::transactor::erc721::deposit",
			?what, ?who,
		);

		let (asset_id, instance) = Matcher::matches_nonfungibles(what)?;
		let token_id = asset_instance_to_u256(&instance)?;

		let who = AccountIdConverter::convert_location(who)
			.ok_or(MatchError::AccountIdConversionFailed)?;

		// We need to map the 32 byte beneficiary account to a 20 byte account.
		let beneficiary_eth = T::AddressMapper::to_address(&who);
		let beneficiary_address = Address::from(Into::<[u8; 20]>::into(beneficiary_eth));

		let checking_eth = T::AddressMapper::to_address(&TransfersCheckingAccount::get());
		let checking_address = Address::from(Into::<[u8; 20]>::into(checking_eth));

		let token_id_alloy: AlloyU256 = SpAlloyU256(token_id).into();

		let data = IERC721::transferFromCall {
			from: checking_address,
			to: beneficiary_address,
			tokenId: token_id_alloy,
		}
		.abi_encode();

		let gas_limit = GasLimit::get();
		let ContractResult { result, gas_consumed, storage_deposit, .. } =
			pallet_revive::Pallet::<T>::bare_call(
				T::RuntimeOrigin::signed(TransfersCheckingAccount::get()),
				asset_id,
				U256::zero(),
				gas_limit,
				DepositLimit::Balance(StorageDepositLimit::get()),
				data,
			);

		let surplus = gas_limit.saturating_sub(gas_consumed);
		tracing::trace!(
			target: "xcm::transactor::erc721::deposit",
			?gas_consumed, ?surplus, ?storage_deposit
		);

		if let Ok(return_value) = result {
			tracing::trace!(
				target: "xcm::transactor::erc721::deposit",
				?return_value,
				"Return value"
			);
			if return_value.did_revert() {
				tracing::debug!(
					target: "xcm::transactor::erc721::deposit",
					"Contract reverted"
				);
				Err(XcmError::FailedToTransactAsset("ERC721 contract reverted"))
			} else if !return_value.data.is_empty() {
				tracing::debug!(target: "xcm::transactor::erc721::withdraw", ?return_value, "Unexpected data");
				Err(XcmError::FailedToTransactAsset("ERC721 returned unexpected data"))
			} else {
				// ERC721 transferFrom does not return a value.
				// Success is determined by the absence of a revert.
				tracing::trace!(
					target: "xcm::transactor::erc721::deposit",
					"ERC721 contract was successful"
				);
				Ok(surplus)
			}
		} else {
			tracing::debug!(
				target: "xcm::transactor::erc721::deposit",
				?result,
				"Error"
			);
			Err(XcmError::FailedToTransactAsset("ERC721 contract execution errored"))
		}
	}
}
