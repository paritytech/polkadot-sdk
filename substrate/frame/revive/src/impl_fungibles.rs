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

use alloy_core::{sol, sol_types::*, primitives::{Address, U256 as EU256}};
use frame_support::{
    traits::tokens::{
        fungibles,
        DepositConsequence,
        WithdrawConsequence,
        Fortitude,
        Precision,
        Preservation,
        Provenance,
    },
};
use sp_core::U256;

use super::*;

// ERC20 interface.
sol! {
    function totalSupply() public view virtual returns (uint256);
    function balanceOf(address account) public view virtual returns (uint256);
    function transfer(address to, uint256 value) public virtual returns (bool);
    function mint(uint256 amount) public;
}

impl<T: Config> fungibles::Inspect<<T as frame_system::Config>::AccountId> for Pallet<T>
where
	BalanceOf<T>: Into<U256> + TryFrom<U256> + Bounded,
	MomentOf<T>: Into<U256>,
    T::Hash: frame_support::traits::IsType<H256>
{
    // The asset id of an ERC20 is its origin contract's address.
    type AssetId = H160;
    // The balance is always u128.
    type Balance = u128;

    // Need to call a view function here.
    fn total_issuance(_: Self::AssetId) -> Self::Balance {
        1
    }

    fn minimum_balance(_: Self::AssetId) -> Self::Balance {
        1
    }

    fn total_balance(_: Self::AssetId, _: &T::AccountId) -> Self::Balance {
        1
    }

    fn balance(asset_id: Self::AssetId, account_id: &T::AccountId) -> Self::Balance {
        let eth_address = T::AddressMapper::to_address(account_id);
        let address = Address::from(Into::<[u8; 20]>::into(eth_address));
        let data = balanceOfCall { account: address }.abi_encode();
        let ContractResult { result, .. } = Self::bare_call(
            T::RuntimeOrigin::signed(account_id.clone()),
            asset_id,
            BalanceOf::<T>::zero(),
            Weight::from_parts(1_000_000_000, 100_000),
            DepositLimit::Unchecked,
            data
        );
        EU256::abi_decode(&result.unwrap().data, true).expect("Failed to ABI decode").to::<u128>()
    }

    fn reducible_balance(_: Self::AssetId, _: &T::AccountId, _: Preservation, _: Fortitude) -> Self::Balance {
        1
    }

    fn can_deposit(_: Self::AssetId, _: &T::AccountId, _: Self::Balance, _: Provenance) -> DepositConsequence {
        DepositConsequence::Success
    }

    fn can_withdraw(_: Self::AssetId, _: &T::AccountId, _: Self::Balance) -> WithdrawConsequence<Self::Balance> {
        WithdrawConsequence::Success
    }

    fn asset_exists(_: Self::AssetId) -> bool {
        false
    }
}

// We implement `fungibles::Mutate` to override `burn_from` and `mint_to`.
//
// These functions are used in [`xcm_builder::FungiblesAdapter`].
impl<T: Config> fungibles::Mutate<<T as frame_system::Config>::AccountId> for Pallet<T>
where
	BalanceOf<T>: Into<U256> + TryFrom<U256> + Bounded,
	MomentOf<T>: Into<U256>,
    T::Hash: frame_support::traits::IsType<H256>
{
    fn burn_from(
        asset: Self::AssetId,
        who: &T::AccountId,
        amount: Self::Balance,
        preservation: Preservation,
        precision: Precision,
        force: Fortitude,
    ) -> Result<Self::Balance, DispatchError> {
        let eth_address = T::AddressMapper::to_address(who);
        let address = Address::from(Into::<[u8; 20]>::into(eth_address));
        let checking_account_eth = T::AddressMapper::to_address(&T::CheckingAccount::get());
        let checking_address = Address::from(Into::<[u8; 20]>::into(checking_account_eth));
        let data = transferCall { to: checking_address, value: EU256::from(amount) }.abi_encode();
        let ContractResult { result, .. } = Self::bare_call(
            T::RuntimeOrigin::signed(who.clone()),
            asset,
            BalanceOf::<T>::zero(),
            Weight::from_parts(1_000_000_000, 100_000),
            DepositLimit::Unchecked,
            data
        );
        log::trace!(target: "whatiwant", "Result: {:?}", &result);
        if let Ok(return_value) = result {
	        let is_success = bool::abi_decode(&return_value.data, false).expect("Failed to ABI decode");
	        if is_success {
	            // TODO: Should return the balance left in `who`.
	            Ok(0)
	        } else {
	            // TODO: Can actually match errors from contract call
	            // to provide better errors here.
	            Err(DispatchError::Unavailable)
	        }
        } else {
            Err(DispatchError::Unavailable)
        }
    }

    fn mint_into(
        asset: Self::AssetId,
        who: &T::AccountId,
        amount: Self::Balance,
    ) -> Result<Self::Balance, DispatchError> {
        let eth_address = T::AddressMapper::to_address(who);
        let address = Address::from(Into::<[u8; 20]>::into(eth_address));
        let data = transferCall { to: address, value: EU256::from(amount) }.abi_encode();
        let ContractResult { result, .. } = Self::bare_call(
            T::RuntimeOrigin::signed(T::CheckingAccount::get()),
            asset,
            BalanceOf::<T>::zero(),
            Weight::from_parts(1_000_000_000, 100_000),
            DepositLimit::Unchecked,
            data
        );
        if let Ok(return_value) = result {
	        let is_success = bool::abi_decode(&return_value.data, false).expect("Failed to ABI decode");
	        log::trace!(target: "whatiwant", "Is success: {:?}", &is_success);
	        if is_success {
	            // TODO: Should return the balance left in `who`.
	            Ok(0)
	        } else {
	            // TODO: Can actually match errors from contract call
	            // to provide better errors here.
	            Err(DispatchError::Unavailable)
	        }
        } else {
        	Err(DispatchError::Unavailable)
        }
    }
}

// This impl is needed for implementing `fungibles::Mutate`.
// However, we don't have this type of access to smart contracts.
// Withdraw and deposit happen via the custom `fungibles::Mutate` impl above.
// Because of this, all functions here return an error, when possible.
impl<T: Config> fungibles::Unbalanced<<T as frame_system::Config>::AccountId> for Pallet<T>
where
	BalanceOf<T>: Into<U256> + TryFrom<U256> + Bounded,
	MomentOf<T>: Into<U256>,
    T::Hash: frame_support::traits::IsType<H256>
{
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
        asset: Self::AssetId,
        who: &T::AccountId,
        amount: Self::Balance,
        precision: Precision,
        preservation: Preservation,
        _: Fortitude,
    ) -> Result<Self::Balance, DispatchError> {
        Err(DispatchError::Unavailable)
    }

    fn increase_balance(
        asset: Self::AssetId,
        who: &T::AccountId,
        amount: Self::Balance,
        _: Precision,
    ) -> Result<Self::Balance, DispatchError> {
        Err(DispatchError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        test_utils::{builder::*, ALICE, BOB, deposit_limit},
        tests::{Contracts, RuntimeOrigin, Test, ExtBuilder},
        Code,
    };
    use frame_support::assert_ok;
    use pallet_revive_fixtures::compile_module;

    #[test]
    fn call_erc20_contract() {
        ExtBuilder::default().existential_deposit(1).build().execute_with(|| {
    		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
    		let (code, _) = compile_module("erc20").unwrap();
            let Contract { addr, account_id } = BareInstantiateBuilder::<Test>::bare_instantiate(RuntimeOrigin::signed(ALICE), Code::Upload(code))
                .build_and_unwrap_contract();
            let amount = 1000;
            let _ = BareCallBuilder::<Test>::bare_call(RuntimeOrigin::signed(ALICE), addr)
                .data(mintCall { amount: EU256::from(amount) }.abi_encode())
                .build_and_unwrap_result();
            let result = BareCallBuilder::<Test>::bare_call(RuntimeOrigin::signed(ALICE), addr)
                .data(totalSupplyCall {}.abi_encode())
                .build_and_unwrap_result();
            let balance = EU256::abi_decode(&result.data, true)
                .expect("Failed to decode ABI response");
            assert_eq!(balance, EU256::from(amount));
            // Contract is uploaded.
            assert_eq!(ContractInfoOf::<Test>::contains_key(&addr), true);
        });
    }

    #[test]
    fn get_balance_of_erc20() {
        ExtBuilder::default().existential_deposit(1).build().execute_with(|| {
    		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
    		let (code, _) = compile_module("erc20").unwrap();
            let Contract { addr, account_id } = BareInstantiateBuilder::<Test>::bare_instantiate(RuntimeOrigin::signed(ALICE), Code::Upload(code))
                .build_and_unwrap_contract();
            assert_eq!(<Contracts as fungibles::Inspect<_>>::balance(addr, &ALICE), 0);
            let amount = 1000;
            let _ = BareCallBuilder::<Test>::bare_call(RuntimeOrigin::signed(ALICE), addr)
                .data(mintCall { amount: EU256::from(amount) }.abi_encode())
                .build_and_unwrap_result();
            assert_eq!(<Contracts as fungibles::Inspect<_>>::balance(addr, &ALICE), amount);
        });
    }

    #[test]
    fn burn_from_impl_works() {
        ExtBuilder::default().existential_deposit(1).build().execute_with(|| {
            let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
            let (code, _) = compile_module("erc20").unwrap();
            let Contract { addr, account_id } = BareInstantiateBuilder::<Test>::bare_instantiate(RuntimeOrigin::signed(ALICE), Code::Upload(code))
                .build_and_unwrap_contract();
            let amount = 1000;
            let _ = BareCallBuilder::<Test>::bare_call(RuntimeOrigin::signed(ALICE), addr)
                .data(mintCall { amount: EU256::from(amount * 2) }.abi_encode())
                .build_and_unwrap_result();
            assert_eq!(<Contracts as fungibles::Inspect<_>>::balance(addr, &ALICE), amount * 2);

            // Use `fungibles::Mutate<_>::burn_from`.
            assert_ok!(<Contracts as fungibles::Mutate<_>>::burn_from(addr, &ALICE, amount, Preservation::Expendable, Precision::Exact, Fortitude::Polite));
            assert_eq!(<Contracts as fungibles::Inspect<_>>::balance(addr, &ALICE), amount);
        });
    }

    #[test]
    fn mint_into_impl_works() {
        ExtBuilder::default().existential_deposit(1).build().execute_with(|| {
            let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
            let (code, _) = compile_module("erc20").unwrap();
            let Contract { addr, account_id } = BareInstantiateBuilder::<Test>::bare_instantiate(RuntimeOrigin::signed(ALICE), Code::Upload(code))
                .storage_deposit_limit((1_000_000_000_000).into())
                .build_and_unwrap_contract();
            let amount = 1000;
            // BOB is the checking account, we're putting `amount` in it.
            let _ = Contracts::bare_call(
                RuntimeOrigin::signed(BOB),
                addr,
                BalanceOf::<Test>::zero(),
                Weight::from_parts(1_000_000_000, 100_000),
                DepositLimit::Unchecked,
                mintCall { amount: EU256::from(amount) }.abi_encode(),
            );
            assert_eq!(<Contracts as fungibles::Inspect<_>>::balance(addr, &BOB), amount);
            let _ = BareCallBuilder::<Test>::bare_call(RuntimeOrigin::signed(ALICE), addr)
                .data(mintCall { amount: EU256::from(amount) }.abi_encode())
                .build_and_unwrap_result();
            assert_eq!(<Contracts as fungibles::Inspect<_>>::balance(addr, &ALICE), amount);

            // We use `mint_into` to transfer assets from the checking account to `ALICE`.
            assert_ok!(<Contracts as fungibles::Mutate<_>>::mint_into(addr, &ALICE, amount));
            // Balances changed accordingly.
            assert_eq!(<Contracts as fungibles::Inspect<_>>::balance(addr, &BOB), 0);
            assert_eq!(<Contracts as fungibles::Inspect<_>>::balance(addr, &ALICE), amount * 2);
        });
    }
}
