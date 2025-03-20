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
    function mint(uint256 amount) public;
    function burn(uint256 amount) public;
    function balanceOf(address account) public view virtual returns (uint256);
    function totalSupply() public view virtual returns (uint256);
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

// We implement `fungibles::Mutate` but we don't override anything.
impl<T: Config> fungibles::Mutate<<T as frame_system::Config>::AccountId> for Pallet<T>
where
	BalanceOf<T>: Into<U256> + TryFrom<U256> + Bounded,
	MomentOf<T>: Into<U256>,
    T::Hash: frame_support::traits::IsType<H256>
{}

// The magic happens in `fungibles::Unbalanced`.
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
        // TODO
        Err(DispatchError::Unavailable)
    }
    fn set_total_issuance(_id: Self::AssetId, _amount: Self::Balance) {
        // TODO
    }

    fn decrease_balance(
        asset: Self::AssetId,
        who: &T::AccountId,
        amount: Self::Balance,
        precision: Precision,
        preservation: Preservation,
        _: Fortitude,
    ) -> Result<Self::Balance, DispatchError> {
        // TODO
        Ok(amount)
    }

    fn increase_balance(
        asset: Self::AssetId,
        who: &T::AccountId,
        amount: Self::Balance,
        _: Precision,
    ) -> Result<Self::Balance, DispatchError> {
        // TODO
        Ok(amount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        test_utils::{builder::*, ALICE, deposit_limit},
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
}
