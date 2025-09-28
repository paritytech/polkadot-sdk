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

//! Implementation of the `nonfungibles::*` family of traits for `pallet-revive`.
//!
//! This allows ERC721 tokens stored on this pallet to be used with
//! the nonfungibles traits.
//!
//! As with the fungibles implementation, this is only meant for tests
//! since gas limits and contract execution costs are not taken into account,
//! and the feature flags ensure that this code is excluded in production runtimes.

#![cfg(any(feature = "std", feature = "runtime-benchmarks", test))]

use alloy_core::{
	primitives::{Address, U256 as EU256},
	sol_types::*,
};

use frame_support::{
	pallet_prelude::DispatchResult,
	traits::{nonfungibles, tokens::fungible, OriginTrait},
};
use sp_core::{H160, H256, U256};

use super::{
	address::AddressMapper, BalanceOf, Bounded, Config, ContractResult, DepositLimit, MomentOf,
	Pallet, Weight,
};
use ethereum_standards::IERC721;

const GAS_LIMIT: Weight = Weight::from_parts(1_000_000_000, 100_000);

impl<T: Config> nonfungibles::Inspect<<T as frame_system::Config>::AccountId> for Pallet<T>
where
	BalanceOf<T>: Into<U256> + TryFrom<U256> + Bounded,
	MomentOf<T>: Into<U256>,
	T::Hash: frame_support::traits::IsType<H256>,
{
	type CollectionId = H160; // indirizzo contratto ERC721
	type ItemId = u128; // tokenId

	fn owner(
		collection: &Self::CollectionId,
		item: &Self::ItemId,
	) -> Option<<T as frame_system::Config>::AccountId> {
		let data = IERC721::ownerOfCall { tokenId: EU256::from(*item) }.abi_encode();
		let ContractResult { result, .. } = Self::bare_call(
			T::RuntimeOrigin::signed(Self::checking_account()),
			*collection,
			U256::zero(),
			GAS_LIMIT,
			DepositLimit::Balance(<<T as super::pallet::Config>::Currency as fungible::Inspect<
				_,
			>>::total_issuance()),
			data,
		);
		if let Ok(return_value) = result {
			if let Ok(addr) = Address::abi_decode_validate(&return_value.data) {
				let owner_eth = H160::from_slice(addr.as_slice());

				Some(T::AddressMapper::to_account_id(&owner_eth))
			} else {
				None
			}
		} else {
			None
		}
	}

	fn can_transfer(collection: &Self::CollectionId, item: &Self::ItemId) -> bool {
		Self::owner(collection, item).is_some()
	}
}
impl<T: Config> nonfungibles::Mutate<<T as frame_system::Config>::AccountId> for Pallet<T>
where
	BalanceOf<T>: Into<U256> + TryFrom<U256> + Bounded,
	MomentOf<T>: Into<U256>,
	T::Hash: frame_support::traits::IsType<H256>,
{
	fn mint_into(
		collection: &Self::CollectionId,
		item: &Self::ItemId,
		who: &T::AccountId,
	) -> DispatchResult {
		// Mint simulato: trasferiamo l’NFT dal checking account → `who`.
		let from = Self::checking_account();
		let eth_from = T::AddressMapper::to_address(&from);
		let from_addr = Address::from(Into::<[u8; 20]>::into(eth_from));

		let eth_to = T::AddressMapper::to_address(who);
		let to_addr = Address::from(Into::<[u8; 20]>::into(eth_to));

		let token_id = EU256::from(*item);

		let data = IERC721::transferFromCall { from: from_addr, to: to_addr, tokenId: token_id }
			.abi_encode();

		let ContractResult { result, .. } = Self::bare_call(
			T::RuntimeOrigin::signed(from),
			*collection,
			U256::zero(),
			GAS_LIMIT,
			DepositLimit::Balance(<<T as super::pallet::Config>::Currency as fungible::Inspect<
				_,
			>>::total_issuance()),
			data,
		);

		if let Ok(rv) = result {
			if rv.did_revert() {
				Err("Contract reverted".into())
			} else {
				Ok(())
			}
		} else {
			Err("Contract out of gas".into())
		}
	}

	fn burn(
		collection: &Self::CollectionId,
		item: &Self::ItemId,
		maybe_check_owner: Option<&T::AccountId>,
	) -> DispatchResult {
		// Burn simulato: trasferiamo l’NFT al checking account (o a 0x0 se preferisci).
		let owner = if let Some(acc) = maybe_check_owner {
			if let Some(current) = <Self as nonfungibles::Inspect<_>>::owner(collection, item) {
				if &current != acc {
					return Err("Wrong owner".into());
				}
				acc.clone()
			} else {
				return Err("Owner not found".into());
			}
		} else {
			<Self as nonfungibles::Inspect<_>>::owner(collection, item).ok_or("Owner not found")?
		};

		let eth_from = T::AddressMapper::to_address(&owner);
		let from_addr = Address::from(Into::<[u8; 20]>::into(eth_from));

		let checking = Self::checking_account();
		let eth_checking = T::AddressMapper::to_address(&checking);
		let to_addr = Address::from(Into::<[u8; 20]>::into(eth_checking));

		let token_id = EU256::from(*item);

		let data = IERC721::transferFromCall { from: from_addr, to: to_addr, tokenId: token_id }
			.abi_encode();

		let ContractResult { result, .. } = Self::bare_call(
			T::RuntimeOrigin::signed(owner),
			*collection,
			U256::zero(),
			GAS_LIMIT,
			DepositLimit::Balance(<<T as super::pallet::Config>::Currency as fungible::Inspect<
				_,
			>>::total_issuance()),
			data,
		);

		if let Ok(rv) = result {
			if rv.did_revert() {
				Err("Contract reverted".into())
			} else {
				Ok(())
			}
		} else {
			Err("Contract out of gas".into())
		}
	}
}

#[cfg(test)]
mod erc721_contract_tests {
	use super::*;
	use crate::{
		test_utils::{builder::*, ALICE, BOB},
		tests::{ExtBuilder, RuntimeOrigin, Test},
		AccountInfoOf, Code,
	};
	const ERC721_PVM_CODE: &[u8] = include_bytes!("../fixtures/erc721/erc721.polkavm");

	#[test]
	fn deploy_erc721_contract() {
		ExtBuilder::default().existential_deposit(1).build().execute_with(|| {
			let _ =
				<<Test as Config>::Currency as fungible::Mutate<_>>::set_balance(&ALICE, 1_000_000);

			let code = ERC721_PVM_CODE.to_vec();

			let Contract { addr, .. } = BareInstantiateBuilder::<Test>::bare_instantiate(
				RuntimeOrigin::signed(ALICE),
				Code::Upload(code),
			)
			.build_and_unwrap_contract();

			assert!(AccountInfoOf::<Test>::contains_key(&addr));
		});
	}

	#[test]
	fn erc721_balance_of_and_owner_of() {
		ExtBuilder::default().existential_deposit(1).build().execute_with(|| {
			let _ =
				<<Test as Config>::Currency as fungible::Mutate<_>>::set_balance(&ALICE, 1_000_000);

			let code = ERC721_PVM_CODE.to_vec();

			let Contract { addr, .. } = BareInstantiateBuilder::<Test>::bare_instantiate(
				RuntimeOrigin::signed(ALICE),
				Code::Upload(code),
			)
			.build_and_unwrap_contract();

			// ownerOf(0) must be ALICE
			let result = BareCallBuilder::<Test>::bare_call(RuntimeOrigin::signed(ALICE), addr)
				.data(IERC721::ownerOfCall { tokenId: EU256::from(0) }.abi_encode())
				.build_and_unwrap_result();

			let owner = Address::abi_decode_validate(&result.data).expect("decode ownerOf");
			let owner_eth = H160::from_slice(owner.as_slice());

			assert_eq!(owner_eth, <Test as Config>::AddressMapper::to_address(&ALICE));

			// balanceOf(ALICE) must be 1
			let result = BareCallBuilder::<Test>::bare_call(RuntimeOrigin::signed(ALICE), addr)
				.data(
					IERC721::balanceOfCall {
						owner: <Test as Config>::AddressMapper::to_address(&ALICE).0.into(),
					}
					.abi_encode(),
				)
				.build_and_unwrap_result();

			let balance: EU256 = EU256::abi_decode_validate(&result.data).unwrap();
			assert_eq!(balance, EU256::from(1));
		});
	}

	#[test]
	fn erc721_transfer_from() {
		ExtBuilder::default().existential_deposit(1).build().execute_with(|| {
			let _ =
				<<Test as Config>::Currency as fungible::Mutate<_>>::set_balance(&ALICE, 1_000_000);

			let code = ERC721_PVM_CODE.to_vec();

			let Contract { addr, .. } = BareInstantiateBuilder::<Test>::bare_instantiate(
				RuntimeOrigin::signed(ALICE),
				Code::Upload(code),
			)
			.build_and_unwrap_contract();

			// Transfer tokenId=0 from ALICE to BOB
			let _ = BareCallBuilder::<Test>::bare_call(RuntimeOrigin::signed(ALICE), addr)
				.data(
					IERC721::transferFromCall {
						from: <Test as Config>::AddressMapper::to_address(&ALICE).0.into(),
						to: <Test as Config>::AddressMapper::to_address(&BOB).0.into(),
						tokenId: EU256::from(0),
					}
					.abi_encode(),
				)
				.build_and_unwrap_result();

			// now ownerOf(0) must be BOB
			let result = BareCallBuilder::<Test>::bare_call(RuntimeOrigin::signed(ALICE), addr)
				.data(IERC721::ownerOfCall { tokenId: EU256::from(0) }.abi_encode())
				.build_and_unwrap_result();

			let owner = Address::abi_decode_validate(&result.data).expect("decode ownerOf");
			let owner_eth = H160::from_slice(owner.as_slice());

			assert_eq!(owner_eth, <Test as Config>::AddressMapper::to_address(&BOB));
		});
	}
}
