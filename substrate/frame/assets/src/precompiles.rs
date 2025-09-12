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

use crate::{weights::WeightInfo, Call, Config, PhantomData, TransferFlags};
use alloc::vec::Vec;
use ethereum_standards::{
	IERC20,
	IERC20::{IERC20Calls, IERC20Events},
};
use pallet_revive::precompiles::{
	alloy::{
		self,
		primitives::IntoLogData,
		sol_types::{Revert, SolCall},
	},
	AddressMapper, AddressMatcher, Error, Ext, Precompile, RuntimeCosts, H160, H256,
};

/// Mean of extracting the asset id from the precompile address.
pub trait AssetIdExtractor {
	type AssetId;
	/// Extracts the asset id from the address.
	fn asset_id_from_address(address: &[u8; 20]) -> Result<Self::AssetId, Error>;
}

/// The configuration of a pallet-assets precompile.
pub trait AssetPrecompileConfig {
	/// The Address matcher used by the precompile.
	const MATCHER: AddressMatcher;

	/// The [`AssetIdExtractor`] used by the precompile.
	type AssetIdExtractor: AssetIdExtractor;
}

/// An `AssetIdExtractor` that stores the asset id directly inside the address.
pub struct InlineAssetIdExtractor;

impl AssetIdExtractor for InlineAssetIdExtractor {
	type AssetId = u32;
	fn asset_id_from_address(addr: &[u8; 20]) -> Result<Self::AssetId, Error> {
		let bytes: [u8; 4] = addr[0..4].try_into().expect("slice is 4 bytes; qed");
		let index = u32::from_be_bytes(bytes);
		return Ok(index.into());
	}
}

/// A precompile configuration that uses a prefix [`AddressMatcher`].
pub struct InlineIdConfig<const PREFIX: u16>;

impl<const P: u16> AssetPrecompileConfig for InlineIdConfig<P> {
	const MATCHER: AddressMatcher = AddressMatcher::Prefix(core::num::NonZero::new(P).unwrap());
	type AssetIdExtractor = InlineAssetIdExtractor;
}

/// An ERC20 precompile.
pub struct ERC20<Runtime, PrecompileConfig, Instance = ()> {
	_phantom: PhantomData<(Runtime, PrecompileConfig, Instance)>,
}

impl<Runtime, PrecompileConfig, Instance: 'static> Precompile
	for ERC20<Runtime, PrecompileConfig, Instance>
where
	PrecompileConfig: AssetPrecompileConfig,
	Runtime: crate::Config<Instance> + pallet_revive::Config,
	<<PrecompileConfig as AssetPrecompileConfig>::AssetIdExtractor as AssetIdExtractor>::AssetId:
		Into<<Runtime as Config<Instance>>::AssetId>,
	Call<Runtime, Instance>: Into<<Runtime as pallet_revive::Config>::RuntimeCall>,
	alloy::primitives::U256: TryInto<<Runtime as Config<Instance>>::Balance>,

	// Note can't use From as it's not implemented for alloy::primitives::U256 for unsigned types
	alloy::primitives::U256: TryFrom<<Runtime as Config<Instance>>::Balance>,
{
	type T = Runtime;
	type Interface = IERC20::IERC20Calls;
	const MATCHER: AddressMatcher = PrecompileConfig::MATCHER;
	const HAS_CONTRACT_INFO: bool = false;

	fn call(
		address: &[u8; 20],
		input: &Self::Interface,
		env: &mut impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		let asset_id = PrecompileConfig::AssetIdExtractor::asset_id_from_address(address)?.into();

		match input {
			IERC20Calls::transfer(call) => Self::transfer(asset_id, call, env),
			IERC20Calls::totalSupply(_) => Self::total_supply(asset_id, env),
			IERC20Calls::balanceOf(call) => Self::balance_of(asset_id, call, env),
			IERC20Calls::allowance(call) => Self::allowance(asset_id, call, env),
			IERC20Calls::approve(call) => Self::approve(asset_id, call, env),
			IERC20Calls::transferFrom(call) => Self::transfer_from(asset_id, call, env),
		}
	}
}

const ERR_INVALID_CALLER: &str = "Invalid caller";
const ERR_BALANCE_CONVERSION_FAILED: &str = "Balance conversion failed";

impl<Runtime, PrecompileConfig, Instance: 'static> ERC20<Runtime, PrecompileConfig, Instance>
where
	PrecompileConfig: AssetPrecompileConfig,
	Runtime: crate::Config<Instance> + pallet_revive::Config,
	<<PrecompileConfig as AssetPrecompileConfig>::AssetIdExtractor as AssetIdExtractor>::AssetId:
		Into<<Runtime as Config<Instance>>::AssetId>,
	Call<Runtime, Instance>: Into<<Runtime as pallet_revive::Config>::RuntimeCall>,
	alloy::primitives::U256: TryInto<<Runtime as Config<Instance>>::Balance>,

	// Note can't use From as it's not implemented for alloy::primitives::U256 for unsigned types
	alloy::primitives::U256: TryFrom<<Runtime as Config<Instance>>::Balance>,
{
	/// Get the caller as an `H160` address.
	fn caller(env: &mut impl Ext<T = Runtime>) -> Result<H160, Error> {
		env.caller()
			.account_id()
			.map(<Runtime as pallet_revive::Config>::AddressMapper::to_address)
			.map_err(|_| Error::Revert(Revert { reason: ERR_INVALID_CALLER.into() }))
	}

	/// Convert a `U256` value to the balance type of the pallet.
	fn to_balance(
		value: alloy::primitives::U256,
	) -> Result<<Runtime as Config<Instance>>::Balance, Error> {
		value
			.try_into()
			.map_err(|_| Error::Revert(Revert { reason: ERR_BALANCE_CONVERSION_FAILED.into() }))
	}

	/// Convert a balance to a `U256` value.
	/// Note this is needed cause From is not implemented for unsigned integer types
	fn to_u256(
		value: <Runtime as Config<Instance>>::Balance,
	) -> Result<alloy::primitives::U256, Error> {
		alloy::primitives::U256::try_from(value)
			.map_err(|_| Error::Revert(Revert { reason: ERR_BALANCE_CONVERSION_FAILED.into() }))
	}

	/// Deposit an event to the runtime.
	fn deposit_event(env: &mut impl Ext<T = Runtime>, event: IERC20Events) -> Result<(), Error> {
		let (topics, data) = event.into_log_data().split();
		let topics = topics.into_iter().map(|v| H256(v.0)).collect::<Vec<_>>();
		env.gas_meter_mut().charge(RuntimeCosts::DepositEvent {
			num_topic: topics.len() as u32,
			len: topics.len() as u32,
		})?;
		env.deposit_event(topics, data.to_vec());
		Ok(())
	}

	/// Execute the transfer call.
	fn transfer(
		asset_id: <Runtime as Config<Instance>>::AssetId,
		call: &IERC20::transferCall,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		env.charge(<Runtime as Config<Instance>>::WeightInfo::transfer())?;

		let from = Self::caller(env)?;
		let dest = <Runtime as pallet_revive::Config>::AddressMapper::to_account_id(
			&call.to.into_array().into(),
		);

		let f = TransferFlags { keep_alive: false, best_effort: false, burn_dust: false };
		crate::Pallet::<Runtime, Instance>::do_transfer(
			asset_id,
			&<Runtime as pallet_revive::Config>::AddressMapper::to_account_id(&from),
			&dest,
			Self::to_balance(call.value)?,
			None,
			f,
		)?;

		Self::deposit_event(
			env,
			IERC20Events::Transfer(IERC20::Transfer {
				from: from.0.into(),
				to: call.to,
				value: call.value,
			}),
		)?;

		return Ok(IERC20::transferCall::abi_encode_returns(&true));
	}

	/// Execute the total supply call.
	fn total_supply(
		asset_id: <Runtime as Config<Instance>>::AssetId,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		use frame_support::traits::fungibles::Inspect;
		env.charge(<Runtime as Config<Instance>>::WeightInfo::total_issuance())?;

		let value = Self::to_u256(crate::Pallet::<Runtime, Instance>::total_issuance(asset_id))?;
		return Ok(IERC20::totalSupplyCall::abi_encode_returns(&value));
	}

	/// Execute the balance_of call.
	fn balance_of(
		asset_id: <Runtime as Config<Instance>>::AssetId,
		call: &IERC20::balanceOfCall,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		env.charge(<Runtime as Config<Instance>>::WeightInfo::balance())?;
		let account = call.account.into_array().into();
		let account = <Runtime as pallet_revive::Config>::AddressMapper::to_account_id(&account);
		let value = Self::to_u256(crate::Pallet::<Runtime, Instance>::balance(asset_id, account))?;
		return Ok(IERC20::balanceOfCall::abi_encode_returns(&value));
	}

	/// Execute the allowance call.
	fn allowance(
		asset_id: <Runtime as Config<Instance>>::AssetId,
		call: &IERC20::allowanceCall,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		env.charge(<Runtime as Config<Instance>>::WeightInfo::allowance())?;
		use frame_support::traits::fungibles::approvals::Inspect;
		let owner = call.owner.into_array().into();
		let owner = <Runtime as pallet_revive::Config>::AddressMapper::to_account_id(&owner);

		let spender = call.spender.into_array().into();
		let spender = <Runtime as pallet_revive::Config>::AddressMapper::to_account_id(&spender);
		let value = Self::to_u256(crate::Pallet::<Runtime, Instance>::allowance(
			asset_id, &owner, &spender,
		))?;

		return Ok(IERC20::balanceOfCall::abi_encode_returns(&value));
	}

	/// Execute the approve call.
	fn approve(
		asset_id: <Runtime as Config<Instance>>::AssetId,
		call: &IERC20::approveCall,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		env.charge(<Runtime as Config<Instance>>::WeightInfo::approve_transfer())?;
		let owner = Self::caller(env)?;
		let spender = call.spender.into_array().into();
		let spender = <Runtime as pallet_revive::Config>::AddressMapper::to_account_id(&spender);

		crate::Pallet::<Runtime, Instance>::do_approve_transfer(
			asset_id,
			&<Runtime as pallet_revive::Config>::AddressMapper::to_account_id(&owner),
			&spender,
			Self::to_balance(call.value)?,
		)?;

		Self::deposit_event(
			env,
			IERC20Events::Approval(IERC20::Approval {
				owner: owner.0.into(),
				spender: call.spender,
				value: call.value,
			}),
		)?;

		return Ok(IERC20::approveCall::abi_encode_returns(&true));
	}

	/// Execute the transfer_from call.
	fn transfer_from(
		asset_id: <Runtime as Config<Instance>>::AssetId,
		call: &IERC20::transferFromCall,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		env.charge(<Runtime as Config<Instance>>::WeightInfo::transfer_approved())?;
		let spender = Self::caller(env)?;
		let spender = <Runtime as pallet_revive::Config>::AddressMapper::to_account_id(&spender);

		let from = call.from.into_array().into();
		let from = <Runtime as pallet_revive::Config>::AddressMapper::to_account_id(&from);

		let to = call.to.into_array().into();
		let to = <Runtime as pallet_revive::Config>::AddressMapper::to_account_id(&to);

		crate::Pallet::<Runtime, Instance>::do_transfer_approved(
			asset_id,
			&from,
			&spender,
			&to,
			Self::to_balance(call.value)?,
		)?;

		Self::deposit_event(
			env,
			IERC20Events::Transfer(IERC20::Transfer {
				from: call.from,
				to: call.to,
				value: call.value,
			}),
		)?;

		return Ok(IERC20::transferFromCall::abi_encode_returns(&true));
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::{
		mock::{new_test_ext, Assets, Balances, RuntimeEvent, RuntimeOrigin, System, Test},
		precompiles::alloy::hex,
	};
	use alloy::primitives::U256;
	use frame_support::{assert_ok, traits::Currency};
	use pallet_revive::DepositLimit;
	use sp_core::H160;
	use sp_runtime::Weight;

	fn assert_contract_event(contract: H160, event: IERC20Events) {
		let (topics, data) = event.into_log_data().split();
		let topics = topics.into_iter().map(|v| H256(v.0)).collect::<Vec<_>>();
		System::assert_has_event(RuntimeEvent::Revive(pallet_revive::Event::ContractEmitted {
			contract,
			data: data.to_vec(),
			topics,
		}));
	}

	#[test]
	fn asset_id_extractor_works() {
		let address: [u8; 20] =
			hex::const_decode_to_array(b"0000053900000000000000000000000001200000").unwrap();
		assert!(InlineIdConfig::<0x0120>::MATCHER.matches(&address));
		assert_eq!(
			<InlineIdConfig<0x0120> as AssetPrecompileConfig>::AssetIdExtractor::asset_id_from_address(
				&address
			)
			.unwrap(),
			1337u32
		);
	}

	#[test]
	fn precompile_transfer_works() {
		new_test_ext().execute_with(|| {
			let asset_id = 0u32;
			let asset_addr = H160::from(
				hex::const_decode_to_array(b"0000000000000000000000000000000001200000").unwrap(),
			);

			let from = 1;
			let to = 2;

			Balances::make_free_balance_be(&from, 100);
			Balances::make_free_balance_be(&to, 100);

			let from_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&from);
			let to_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&to);
			assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, from, true, 1));
			assert_ok!(Assets::mint(RuntimeOrigin::signed(from), asset_id, from, 100));

			let data =
				IERC20::transferCall { to: to_addr.0.into(), value: U256::from(10) }.abi_encode();

			pallet_revive::Pallet::<Test>::bare_call(
				RuntimeOrigin::signed(1),
				H160::from(asset_addr),
				0u32.into(),
				Weight::MAX,
				DepositLimit::UnsafeOnlyForDryRun,
				data,
			);

			assert_contract_event(
				asset_addr,
				IERC20Events::Transfer(IERC20::Transfer {
					from: from_addr.0.into(),
					to: to_addr.0.into(),
					value: U256::from(10),
				}),
			);

			assert_eq!(Assets::balance(asset_id, from), 90);
			assert_eq!(Assets::balance(asset_id, to), 10);
		});
	}

	#[test]
	fn total_supply_works() {
		new_test_ext().execute_with(|| {
			let asset_id = 0u32;
			let asset_addr =
				hex::const_decode_to_array(b"0000000000000000000000000000000001200000").unwrap();

			Balances::make_free_balance_be(&1, 100);
			assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, 1, true, 1));
			assert_ok!(Assets::mint(RuntimeOrigin::signed(1), asset_id, 1, 1000));

			let data = IERC20::totalSupplyCall {}.abi_encode();

			let data = pallet_revive::Pallet::<Test>::bare_call(
				RuntimeOrigin::signed(1),
				H160::from(asset_addr),
				0u32.into(),
				Weight::MAX,
				DepositLimit::UnsafeOnlyForDryRun,
				data,
			)
			.result
			.unwrap()
			.data;

			let ret = IERC20::totalSupplyCall::abi_decode_returns(&data).unwrap();
			assert_eq!(ret, U256::from(1000));
		});
	}

	#[test]
	fn balance_of_works() {
		new_test_ext().execute_with(|| {
			let asset_id = 0u32;
			let asset_addr =
				hex::const_decode_to_array(b"0000000000000000000000000000000001200000").unwrap();

			Balances::make_free_balance_be(&1, 100);
			assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, 1, true, 1));
			assert_ok!(Assets::mint(RuntimeOrigin::signed(1), asset_id, 1, 1000));

			let account = <Test as pallet_revive::Config>::AddressMapper::to_address(&1).0.into();
			let data = IERC20::balanceOfCall { account }.abi_encode();

			let data = pallet_revive::Pallet::<Test>::bare_call(
				RuntimeOrigin::signed(1),
				H160::from(asset_addr),
				0u32.into(),
				Weight::MAX,
				DepositLimit::UnsafeOnlyForDryRun,
				data,
			)
			.result
			.unwrap()
			.data;

			let ret = IERC20::balanceOfCall::abi_decode_returns(&data).unwrap();
			assert_eq!(ret, U256::from(1000));
		});
	}

	#[test]
	fn approval_works() {
		use frame_support::traits::fungibles::approvals::Inspect;

		new_test_ext().execute_with(|| {
			let asset_id = 0u32;
			let asset_addr = H160::from(
				hex::const_decode_to_array(b"0000000000000000000000000000000001200000").unwrap(),
			);

			let owner = 1;
			let spender = 2;
			let other = 3;

			Balances::make_free_balance_be(&owner, 100);
			Balances::make_free_balance_be(&spender, 100);
			Balances::make_free_balance_be(&other, 100);

			let owner_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&owner);
			let spender_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&spender);
			let other_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&other);

			assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner, true, 1));
			assert_ok!(Assets::mint(RuntimeOrigin::signed(owner), asset_id, owner, 100));

			let data =
				IERC20::approveCall { spender: spender_addr.0.into(), value: U256::from(25) }
					.abi_encode();

			pallet_revive::Pallet::<Test>::bare_call(
				RuntimeOrigin::signed(owner),
				H160::from(asset_addr),
				0u32.into(),
				Weight::MAX,
				DepositLimit::UnsafeOnlyForDryRun,
				data,
			);

			assert_contract_event(
				asset_addr,
				IERC20Events::Approval(IERC20::Approval {
					owner: owner_addr.0.into(),
					spender: spender_addr.0.into(),
					value: U256::from(25),
				}),
			);

			let data = IERC20::allowanceCall {
				owner: owner_addr.0.into(),
				spender: spender_addr.0.into(),
			}
			.abi_encode();

			let data = pallet_revive::Pallet::<Test>::bare_call(
				RuntimeOrigin::signed(owner),
				H160::from(asset_addr),
				0u32.into(),
				Weight::MAX,
				DepositLimit::UnsafeOnlyForDryRun,
				data,
			)
			.result
			.unwrap()
			.data;

			let ret = IERC20::allowanceCall::abi_decode_returns(&data).unwrap();
			assert_eq!(ret, U256::from(25));

			let data = IERC20::transferFromCall {
				from: owner_addr.0.into(),
				to: other_addr.0.into(),
				value: U256::from(10),
			}
			.abi_encode();

			pallet_revive::Pallet::<Test>::bare_call(
				RuntimeOrigin::signed(spender),
				H160::from(asset_addr),
				0u32.into(),
				Weight::MAX,
				DepositLimit::UnsafeOnlyForDryRun,
				data,
			);
			assert_eq!(Assets::balance(asset_id, owner), 90);
			assert_eq!(Assets::allowance(asset_id, &owner, &spender), 15);
			assert_eq!(Assets::balance(asset_id, other), 10);

			assert_contract_event(
				asset_addr,
				IERC20Events::Transfer(IERC20::Transfer {
					from: owner_addr.0.into(),
					to: other_addr.0.into(),
					value: U256::from(10),
				}),
			);
		});
	}
}
