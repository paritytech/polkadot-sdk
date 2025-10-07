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

//! Implementation of the `fungibles::*` family of traits for `pallet-revive`.
//!
//! This is meant to allow ERC20 tokens stored on this pallet to be used with
//! the fungibles traits.
//! This is only meant for tests since gas limits are not taken into account,
//! the feature flags make sure of that.

#![cfg(any(feature = "std", feature = "runtime-benchmarks", test))]

use crate::OriginFor;
use alloy_core::{
	primitives::{Address, U256 as EU256},
	sol_types::*,
};
use frame_support::{
	traits::{
		tokens::{
			fungible, fungibles, DepositConsequence, Fortitude, Precision, Preservation,
			Provenance, WithdrawConsequence,
		},
		OriginTrait,
	},
	PalletId,
};
use sp_core::{H160, U256};
use sp_runtime::{traits::AccountIdConversion, DispatchError};

use super::{address::AddressMapper, pallet, Config, ContractResult, ExecConfig, Pallet, Weight};
use ethereum_standards::IERC20;

const GAS_LIMIT: Weight = Weight::from_parts(1_000_000_000, 100_000);

impl<T: Config> Pallet<T> {
	// Test checking account for the `fungibles::*` implementation.
	//
	// Still needs to be mapped in tests for it to be usable.
	pub fn checking_account() -> <T as frame_system::Config>::AccountId {
		PalletId(*b"py/revch").into_account_truncating()
	}
}

impl<T: Config> fungibles::Inspect<<T as frame_system::Config>::AccountId> for Pallet<T> {
	// The asset id of an ERC20 is its origin contract's address.
	type AssetId = H160;
	// The balance is always u128.
	type Balance = u128;

	// Need to call a view function here.
	fn total_issuance(asset_id: Self::AssetId) -> Self::Balance {
		let data = IERC20::totalSupplyCall {}.abi_encode();
		let ContractResult { result, .. } = Self::bare_call(
			OriginFor::<T>::signed(Self::checking_account()),
			asset_id,
			U256::zero(),
			GAS_LIMIT,
			<<T as pallet::Config>::Currency as fungible::Inspect<_>>::total_issuance(),
			data,
			ExecConfig::new_substrate_tx(),
		);
		if let Ok(return_value) = result {
			if let Ok(eu256) = EU256::abi_decode_validate(&return_value.data) {
				eu256.to::<u128>()
			} else {
				0
			}
		} else {
			0
		}
	}

	fn minimum_balance(_: Self::AssetId) -> Self::Balance {
		// ERC20s don't have this concept.
		1
	}

	fn total_balance(asset_id: Self::AssetId, account_id: &T::AccountId) -> Self::Balance {
		// Since ERC20s don't have the concept of freezes and locks,
		// total balance is the same as balance.
		Self::balance(asset_id, account_id)
	}

	fn balance(asset_id: Self::AssetId, account_id: &T::AccountId) -> Self::Balance {
		let eth_address = T::AddressMapper::to_address(account_id);
		let address = Address::from(Into::<[u8; 20]>::into(eth_address));
		let data = IERC20::balanceOfCall { account: address }.abi_encode();
		let ContractResult { result, .. } = Self::bare_call(
			OriginFor::<T>::signed(account_id.clone()),
			asset_id,
			U256::zero(),
			GAS_LIMIT,
			<<T as pallet::Config>::Currency as fungible::Inspect<_>>::total_issuance(),
			data,
			ExecConfig::new_substrate_tx(),
		);
		if let Ok(return_value) = result {
			if let Ok(eu256) = EU256::abi_decode_validate(&return_value.data) {
				eu256.to::<u128>()
			} else {
				0
			}
		} else {
			0
		}
	}

	fn reducible_balance(
		asset_id: Self::AssetId,
		account_id: &T::AccountId,
		_: Preservation,
		_: Fortitude,
	) -> Self::Balance {
		// Since ERC20s don't have minimum amounts, this is the same
		// as balance.
		Self::balance(asset_id, account_id)
	}

	fn can_deposit(
		_: Self::AssetId,
		_: &T::AccountId,
		_: Self::Balance,
		_: Provenance,
	) -> DepositConsequence {
		DepositConsequence::Success
	}

	fn can_withdraw(
		_: Self::AssetId,
		_: &T::AccountId,
		_: Self::Balance,
	) -> WithdrawConsequence<Self::Balance> {
		WithdrawConsequence::Success
	}

	fn asset_exists(_: Self::AssetId) -> bool {
		false
	}
}

// We implement `fungibles::Mutate` to override `burn_from` and `mint_to`.
//
// These functions are used in [`xcm_builder::FungiblesAdapter`].
impl<T: Config> fungibles::Mutate<<T as frame_system::Config>::AccountId> for Pallet<T> {
	fn burn_from(
		asset_id: Self::AssetId,
		who: &T::AccountId,
		amount: Self::Balance,
		_: Preservation,
		_: Precision,
		_: Fortitude,
	) -> Result<Self::Balance, DispatchError> {
		let checking_account_eth = T::AddressMapper::to_address(&Self::checking_account());
		let checking_address = Address::from(Into::<[u8; 20]>::into(checking_account_eth));
		let data =
			IERC20::transferCall { to: checking_address, value: EU256::from(amount) }.abi_encode();
		let ContractResult { result, gas_consumed, .. } = Self::bare_call(
			OriginFor::<T>::signed(who.clone()),
			asset_id,
			U256::zero(),
			GAS_LIMIT,
			<<T as pallet::Config>::Currency as fungible::Inspect<_>>::total_issuance(),
			data,
			ExecConfig::new_substrate_tx(),
		);
		log::trace!(target: "whatiwant", "{gas_consumed}");
		if let Ok(return_value) = result {
			if return_value.did_revert() {
				Err("Contract reverted".into())
			} else {
				let is_success =
					bool::abi_decode_validate(&return_value.data).expect("Failed to ABI decode");
				if is_success {
					let balance = <Self as fungibles::Inspect<_>>::balance(asset_id, who);
					Ok(balance)
				} else {
					Err("Contract transfer failed".into())
				}
			}
		} else {
			Err("Contract out of gas".into())
		}
	}

	fn mint_into(
		asset_id: Self::AssetId,
		who: &T::AccountId,
		amount: Self::Balance,
	) -> Result<Self::Balance, DispatchError> {
		let eth_address = T::AddressMapper::to_address(who);
		let address = Address::from(Into::<[u8; 20]>::into(eth_address));
		let data = IERC20::transferCall { to: address, value: EU256::from(amount) }.abi_encode();
		let ContractResult { result, .. } = Self::bare_call(
			OriginFor::<T>::signed(Self::checking_account()),
			asset_id,
			U256::zero(),
			GAS_LIMIT,
			<<T as pallet::Config>::Currency as fungible::Inspect<_>>::total_issuance(),
			data,
			ExecConfig::new_substrate_tx(),
		);
		if let Ok(return_value) = result {
			if return_value.did_revert() {
				Err("Contract reverted".into())
			} else {
				let is_success =
					bool::abi_decode_validate(&return_value.data).expect("Failed to ABI decode");
				if is_success {
					let balance = <Self as fungibles::Inspect<_>>::balance(asset_id, who);
					Ok(balance)
				} else {
					Err("Contract transfer failed".into())
				}
			}
		} else {
			Err("Contract out of gas".into())
		}
	}
}

// This impl is needed for implementing `fungibles::Mutate`.
// However, we don't have this type of access to smart contracts.
// Withdraw and deposit happen via the custom `fungibles::Mutate` impl above.
// Because of this, all functions here return an error, when possible.
impl<T: Config> fungibles::Unbalanced<<T as frame_system::Config>::AccountId> for Pallet<T> {
	fn handle_raw_dust(_: Self::AssetId, _: Self::Balance) {}
	fn handle_dust(_: fungibles::Dust<T::AccountId, Self>) {}
	fn write_balance(
		_: Self::AssetId,
		_: &T::AccountId,
		_: Self::Balance,
	) -> Result<Option<Self::Balance>, DispatchError> {
		Err(DispatchError::Unavailable)
	}
	fn set_total_issuance(_id: Self::AssetId, _amount: Self::Balance) {
		// Empty.
	}

	fn decrease_balance(
		_: Self::AssetId,
		_: &T::AccountId,
		_: Self::Balance,
		_: Precision,
		_: Preservation,
		_: Fortitude,
	) -> Result<Self::Balance, DispatchError> {
		Err(DispatchError::Unavailable)
	}

	fn increase_balance(
		_: Self::AssetId,
		_: &T::AccountId,
		_: Self::Balance,
		_: Precision,
	) -> Result<Self::Balance, DispatchError> {
		Err(DispatchError::Unavailable)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		test_utils::{builder::*, ALICE},
		tests::{Contracts, ExtBuilder, RuntimeOrigin, Test},
		AccountInfoOf, Code,
	};
	use frame_support::assert_ok;
	const ERC20_PVM_CODE: &[u8] = include_bytes!("../fixtures/erc20/erc20.polkavm");

	#[test]
	fn call_erc20_contract() {
		ExtBuilder::default().existential_deposit(1).build().execute_with(|| {
			let _ =
				<<Test as Config>::Currency as fungible::Mutate<_>>::set_balance(&ALICE, 1_000_000);
			let code = ERC20_PVM_CODE.to_vec();
			let amount = EU256::from(1000);
			let constructor_data = sol_data::Uint::<256>::abi_encode(&amount);
			let Contract { addr, .. } = BareInstantiateBuilder::<Test>::bare_instantiate(
				RuntimeOrigin::signed(ALICE),
				Code::Upload(code),
			)
			.data(constructor_data)
			.build_and_unwrap_contract();
			let result = BareCallBuilder::<Test>::bare_call(RuntimeOrigin::signed(ALICE), addr)
				.data(IERC20::totalSupplyCall {}.abi_encode())
				.build_and_unwrap_result();
			let balance =
				EU256::abi_decode_validate(&result.data).expect("Failed to decode ABI response");
			assert_eq!(balance, EU256::from(amount));
			// Contract is uploaded.
			assert_eq!(AccountInfoOf::<Test>::contains_key(&addr), true);
		});
	}

	#[test]
	fn total_issuance_works() {
		ExtBuilder::default().existential_deposit(1).build().execute_with(|| {
			let _ =
				<<Test as Config>::Currency as fungible::Mutate<_>>::set_balance(&ALICE, 1_000_000);
			let code = ERC20_PVM_CODE.to_vec();
			let amount = 1000;
			let constructor_data = sol_data::Uint::<256>::abi_encode(&EU256::from(amount));
			let Contract { addr, .. } = BareInstantiateBuilder::<Test>::bare_instantiate(
				RuntimeOrigin::signed(ALICE),
				Code::Upload(code),
			)
			.data(constructor_data)
			.build_and_unwrap_contract();

			let total_issuance = <Contracts as fungibles::Inspect<_>>::total_issuance(addr);
			assert_eq!(total_issuance, amount);
		});
	}

	#[test]
	fn get_balance_of_erc20() {
		ExtBuilder::default().existential_deposit(1).build().execute_with(|| {
			let _ =
				<<Test as Config>::Currency as fungible::Mutate<_>>::set_balance(&ALICE, 1_000_000);
			let code = ERC20_PVM_CODE.to_vec();
			let amount = 1000;
			let constructor_data = sol_data::Uint::<256>::abi_encode(&EU256::from(amount));
			let Contract { addr, .. } = BareInstantiateBuilder::<Test>::bare_instantiate(
				RuntimeOrigin::signed(ALICE),
				Code::Upload(code),
			)
			.data(constructor_data)
			.build_and_unwrap_contract();
			assert_eq!(<Contracts as fungibles::Inspect<_>>::balance(addr, &ALICE), amount);
		});
	}

	#[test]
	fn burn_from_impl_works() {
		ExtBuilder::default().existential_deposit(1).build().execute_with(|| {
			let _ =
				<<Test as Config>::Currency as fungible::Mutate<_>>::set_balance(&ALICE, 1_000_000);
			let code = ERC20_PVM_CODE.to_vec();
			let amount = 1000;
			let constructor_data = sol_data::Uint::<256>::abi_encode(&(EU256::from(amount * 2)));
			let Contract { addr, .. } = BareInstantiateBuilder::<Test>::bare_instantiate(
				RuntimeOrigin::signed(ALICE),
				Code::Upload(code),
			)
			.data(constructor_data)
			.build_and_unwrap_contract();
			let _ = BareCallBuilder::<Test>::bare_call(RuntimeOrigin::signed(ALICE), addr)
				.build_and_unwrap_result();
			assert_eq!(<Contracts as fungibles::Inspect<_>>::balance(addr, &ALICE), amount * 2);

			// Use `fungibles::Mutate<_>::burn_from`.
			assert_ok!(<Contracts as fungibles::Mutate<_>>::burn_from(
				addr,
				&ALICE,
				amount,
				Preservation::Expendable,
				Precision::Exact,
				Fortitude::Polite
			));
			assert_eq!(<Contracts as fungibles::Inspect<_>>::balance(addr, &ALICE), amount);
		});
	}

	#[test]
	fn mint_into_impl_works() {
		ExtBuilder::default().existential_deposit(1).build().execute_with(|| {
			let checking_account = Pallet::<Test>::checking_account();
			let _ =
				<<Test as Config>::Currency as fungible::Mutate<_>>::set_balance(&ALICE, 1_000_000);
			let _ = <<Test as Config>::Currency as fungible::Mutate<_>>::set_balance(
				&checking_account,
				1_000_000,
			);
			let code = ERC20_PVM_CODE.to_vec();
			let amount = 1000;
			let constructor_data = sol_data::Uint::<256>::abi_encode(&EU256::from(amount));
			// We're instantiating the contract with the `CheckingAccount` so it has `amount` in it.
			let Contract { addr, .. } = BareInstantiateBuilder::<Test>::bare_instantiate(
				RuntimeOrigin::signed(checking_account.clone()),
				Code::Upload(code),
			)
			.storage_deposit_limit(1_000_000_000_000)
			.data(constructor_data)
			.build_and_unwrap_contract();
			assert_eq!(
				<Contracts as fungibles::Inspect<_>>::balance(addr, &checking_account),
				amount
			);
			assert_eq!(<Contracts as fungibles::Inspect<_>>::balance(addr, &ALICE), 0);

			// We use `mint_into` to transfer assets from the checking account to `ALICE`.
			assert_ok!(<Contracts as fungibles::Mutate<_>>::mint_into(addr, &ALICE, amount));
			// Balances changed accordingly.
			assert_eq!(<Contracts as fungibles::Inspect<_>>::balance(addr, &checking_account), 0);
			assert_eq!(<Contracts as fungibles::Inspect<_>>::balance(addr, &ALICE), amount);
		});
	}
}
