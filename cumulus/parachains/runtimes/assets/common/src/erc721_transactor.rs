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
//!
//! Enables XCM reserve transfers of ERC-721 non-fungible tokens held in smart contracts
//! deployed via `pallet-revive`. Teleportation is not supported.
//!
//! # Transfer Flow
//!
//! **Withdraw (source chain):** The XCM executor calls `withdraw_asset`, which calls
//! `transferFrom(owner, checking_account, tokenId)` on the ERC-721 contract, moving the
//! NFT to a sovereign checking account while the asset is in transit.
//!
//! **Deposit (destination chain):** The XCM executor calls `deposit_asset`, which calls
//! `transferFrom(checking_account, beneficiary, tokenId)` on the ERC-721 contract,
//! releasing the NFT from the checking account to the final recipient.
//!
//! # Asset Identification
//!
//! ERC-721 assets are identified in XCM using:
//! - `AssetId`: A `Location` ending with `AccountKey20 { key: contract_address }`, which points to
//!   the deployed ERC-721 contract.
//! - `AssetInstance::Index(token_id)`: The unique token ID within the collection.
//!
//! Example XCM asset for an ERC-721 token on Asset Hub:
//! ```text
//! Asset {
//!     id: Location(0, [AccountKey20 { key: 0xabcd...1234 }]),
//!     fun: NonFungible(AssetInstance::Index(42)),
//! }
//! ```

use core::marker::PhantomData;
use ethereum_standards::IERC721;
use frame_support::{
	defensive_assert,
	traits::{fungible::Inspect, OriginTrait},
};
use frame_system::pallet_prelude::OriginFor;
use pallet_revive::{
	precompiles::alloy::{
		primitives::{Address, U256 as EU256},
		sol_types::SolCall,
	},
	AddressMapper, ContractResult, ExecConfig, MomentOf, TransactionLimits,
};
use sp_core::{Get, H160, H256, U256};
use sp_runtime::Weight;
use xcm::latest::prelude::*;
use xcm_executor::{
	traits::{ConvertLocation, Error as MatchError, MatchesNonFungibles, TransactAsset},
	AssetsInHolding,
};

type BalanceOf<T> = <<T as pallet_revive::Config>::Currency as Inspect<
	<T as frame_system::Config>::AccountId,
>>::Balance;

/// An Asset Transactor that deals with ERC-721 non-fungible tokens.
pub struct ERC721Transactor<
	T,
	Matcher,
	AccountIdConverter,
	WeightLimit,
	StorageDepositLimit,
	AccountId,
	TransfersCheckingAccount,
>(
	PhantomData<(
		T,
		Matcher,
		AccountIdConverter,
		WeightLimit,
		StorageDepositLimit,
		AccountId,
		TransfersCheckingAccount,
	)>,
);

impl<
		AccountId: Eq + Clone,
		T: pallet_revive::Config<AccountId = AccountId>,
		AccountIdConverter: ConvertLocation<AccountId>,
		Matcher: MatchesNonFungibles<H160, u128>,
		WeightLimit: Get<Weight>,
		StorageDepositLimit: Get<BalanceOf<T>>,
		TransfersCheckingAccount: Get<AccountId>,
	> TransactAsset
	for ERC721Transactor<
		T,
		Matcher,
		AccountIdConverter,
		WeightLimit,
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
		// Teleportation is not supported for ERC-721 tokens: the contract state cannot be
		// burned on the source chain and minted on the destination.
		Err(XcmError::Unimplemented)
	}

	fn check_in(_origin: &Location, _what: &Asset, _context: &XcmContext) {
		// No-op: teleportation not supported.
	}

	fn can_check_out(_destination: &Location, _what: &Asset, _context: &XcmContext) -> XcmResult {
		// Teleportation is not supported for ERC-721 tokens.
		Err(XcmError::Unimplemented)
	}

	fn check_out(_destination: &Location, _what: &Asset, _context: &XcmContext) {
		// No-op: teleportation not supported.
	}

	/// Withdraws an ERC-721 token from the sender by transferring it to the checking account.
	fn withdraw_asset_with_surplus(
		what: &Asset,
		who: &Location,
		_context: Option<&XcmContext>,
	) -> Result<(AssetsInHolding, Weight), XcmError> {
		tracing::trace!(
			target: "xcm::transactor::erc721::withdraw",
			?what, ?who,
		);
		let (contract_id, token_id) = Matcher::matches_nonfungibles(what)?;
		let who = AccountIdConverter::convert_location(who)
			.ok_or(MatchError::AccountIdConversionFailed)?;
		// Map the 32-byte owner account to a 20-byte Ethereum address (msg.sender of the call).
		let owner_eth = T::AddressMapper::to_address(&who);
		let from = Address::from(Into::<[u8; 20]>::into(owner_eth));
		// Map the 32-byte checking account to a 20-byte Ethereum address (recipient of the NFT).
		let checking_account_eth = T::AddressMapper::to_address(&TransfersCheckingAccount::get());
		let to = Address::from(Into::<[u8; 20]>::into(checking_account_eth));
		let weight_limit = WeightLimit::get();
		// Call ERC-721 transferFrom(owner, checking_account, tokenId) from the owner's origin.
		// Since msg.sender == from == owner, the ERC-721 contract permits this transfer.
		let data =
			IERC721::transferFromCall { from, to, tokenId: EU256::from(token_id) }.abi_encode();
		let ContractResult { result, weight_consumed, storage_deposit, .. } =
			pallet_revive::Pallet::<T>::bare_call(
				OriginFor::<T>::signed(who.clone()),
				contract_id,
				U256::zero(),
				TransactionLimits::WeightAndDeposit {
					weight_limit,
					deposit_limit: StorageDepositLimit::get(),
				},
				data,
				&ExecConfig::new_substrate_tx(),
			);
		// Return unused weight so the XCM executor can refund it.
		let surplus = weight_limit.saturating_sub(weight_consumed);
		tracing::trace!(
			target: "xcm::transactor::erc721::withdraw",
			?weight_consumed, ?surplus, ?storage_deposit,
		);
		match result {
			Ok(return_value) => {
				tracing::trace!(
					target: "xcm::transactor::erc721::withdraw",
					?return_value,
					"Return value by withdraw_asset",
				);
				if return_value.did_revert() {
					tracing::debug!(
						target: "xcm::transactor::erc721::withdraw",
						"ERC721 contract reverted",
					);
					Err(XcmError::FailedToTransactAsset("ERC721 contract reverted"))
				} else {
					// transferFrom returns void; a non-reverting call means success.
					tracing::trace!(
						target: "xcm::transactor::erc721::withdraw",
						"ERC721 transferFrom successful",
					);
					Ok((
						AssetsInHolding::new_from_non_fungible(
							what.id.clone(),
							AssetInstance::Index(token_id),
						),
						surplus,
					))
				}
			},
			Err(error) => {
				tracing::debug!(
					target: "xcm::transactor::erc721::withdraw",
					?error,
					"ERC721 contract execution errored",
				);
				// Could be out-of-gas, duplicate contract call, etc.
				// A hardcoded gas limit means the user cannot fix this by changing the XCM.
				Err(XcmError::FailedToTransactAsset("ERC721 contract execution errored"))
			},
		}
	}

	/// Deposits an ERC-721 token to the beneficiary by transferring it from the checking account.
	fn deposit_asset_with_surplus(
		what: AssetsInHolding,
		who: &Location,
		_context: Option<&XcmContext>,
	) -> Result<Weight, (AssetsInHolding, XcmError)> {
		tracing::trace!(
			target: "xcm::transactor::erc721::deposit",
			?what, ?who,
		);
		defensive_assert!(what.len() == 1, "Trying to deposit more than one asset!");
		// Retrieve the single non-fungible asset and match it.
		let maybe = what
			.non_fungible_assets_iter()
			.next()
			.and_then(|asset| Matcher::matches_nonfungibles(&asset).ok());
		let (contract_id, token_id) = match maybe {
			Some(inner) => inner,
			None => return Err((what, MatchError::AssetNotHandled.into())),
		};
		let who = match AccountIdConverter::convert_location(who) {
			Some(inner) => inner,
			None => return Err((what, MatchError::AccountIdConversionFailed.into())),
		};
		// Map the 32-byte beneficiary account to a 20-byte Ethereum address.
		let eth_address = T::AddressMapper::to_address(&who);
		let to = Address::from(Into::<[u8; 20]>::into(eth_address));
		// Map the 32-byte checking account to a 20-byte Ethereum address (current NFT holder).
		let checking_account_eth = T::AddressMapper::to_address(&TransfersCheckingAccount::get());
		let from = Address::from(Into::<[u8; 20]>::into(checking_account_eth));
		let weight_limit = WeightLimit::get();
		// Call ERC-721 transferFrom(checking_account, beneficiary, tokenId) from the checking
		// account's origin. Since msg.sender == from == checking_account, the ERC-721 contract
		// permits this transfer.
		let data =
			IERC721::transferFromCall { from, to, tokenId: EU256::from(token_id) }.abi_encode();
		let ContractResult { result, weight_consumed, storage_deposit, .. } =
			pallet_revive::Pallet::<T>::bare_call(
				OriginFor::<T>::signed(TransfersCheckingAccount::get()),
				contract_id,
				U256::zero(),
				TransactionLimits::WeightAndDeposit {
					weight_limit,
					deposit_limit: StorageDepositLimit::get(),
				},
				data,
				&ExecConfig::new_substrate_tx(),
			);
		// Return unused weight so the XCM executor can refund it.
		let surplus = weight_limit.saturating_sub(weight_consumed);
		tracing::trace!(
			target: "xcm::transactor::erc721::deposit",
			?weight_consumed, ?surplus, ?storage_deposit,
		);
		match result {
			Ok(return_value) => {
				tracing::trace!(
					target: "xcm::transactor::erc721::deposit",
					?return_value,
					"Return value",
				);
				if return_value.did_revert() {
					tracing::debug!(
						target: "xcm::transactor::erc721::deposit",
						"ERC721 contract reverted",
					);
					Err((what, XcmError::FailedToTransactAsset("ERC721 contract reverted")))
				} else {
					// transferFrom returns void; a non-reverting call means success.
					tracing::trace!(
						target: "xcm::transactor::erc721::deposit",
						"ERC721 transferFrom successful",
					);
					Ok(surplus)
				}
			},
			Err(error) => {
				tracing::debug!(
					target: "xcm::transactor::erc721::deposit",
					?error,
					"ERC721 contract execution errored",
				);
				Err((what, XcmError::FailedToTransactAsset("ERC721 contract execution errored")))
			},
		}
	}
}
