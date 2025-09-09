// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::{
	address::AddressMapper,
	precompiles::{BuiltinAddressMatcher, BuiltinPrecompile, Error, Ext},
	vm::RuntimeCosts,
	Config, Origin, H160,
};
use alloc::vec::Vec;
use alloy_core::{sol, sol_types::SolValue};
use codec::Encode;
use core::{marker::PhantomData, num::NonZero};
use frame_support::traits::fungible::Inspect;
use sp_core::hexdisplay::AsBytesRef;

pub struct System<T>(PhantomData<T>);

sol! {
	interface ISystem {
		/// Computes the BLAKE2 256-bit hash on the given input.
		function hashBlake256(bytes memory input) external pure returns (bytes32 digest);

		/// Computes the BLAKE2 128-bit hash on the given input.
		function hashBlake128(bytes memory input) external pure returns (bytes32 digest);

		/// Retrieve the account id for a specified `H160` address.
		///
		/// Calling this function on a native `H160` chain (`type AccountId = H160`)
		/// does not make sense, as it would just return the `address` that it was
		/// called with.
		///
		/// # Note
		///
		/// If no mapping exists for `addr`, the fallback account id will be returned.
		function toAccountId(address input) external view returns (bytes memory account_id);

		/// Checks whether the contract caller is the origin of the whole call stack.
		function callerIsOrigin() external view returns (bool);

		/// Checks whether the caller of the current contract is root.
		///
		/// Note that only the origin of the call stack can be root. Hence this
		/// function returning `true` implies that the contract is being called by the origin.
		///
		/// A return value of `true` indicates that this contract is being called by a root origin,
		/// and `false` indicates that the caller is a signed origin.
		function callerIsRoot() external view returns (bool);

		/// Returns the minimum balance that is required for creating an account
		/// (the existential deposit).
		function minimumBalance() external view returns (uint);

		/// Returns the code hash of the currently executing contract.
		function ownCodeHash() external view returns (bytes32);

		/// Returns the amount of weight left.
		/// The data is encoded as `Weight`.
		function weightLeft() external view returns (uint64 refTime, uint64 proofSize);
	}
}

impl<T: Config> BuiltinPrecompile for System<T> {
	type T = T;
	type Interface = ISystem::ISystemCalls;
	const MATCHER: BuiltinAddressMatcher =
		BuiltinAddressMatcher::Fixed(NonZero::new(0x900).unwrap());
	const HAS_CONTRACT_INFO: bool = false;

	fn call(
		_address: &[u8; 20],
		input: &Self::Interface,
		env: &mut impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		use ISystem::ISystemCalls;
		match input {
			ISystemCalls::hashBlake256(ISystem::hashBlake256Call { input }) => {
				env.gas_meter_mut().charge(RuntimeCosts::HashBlake256(input.len() as u32))?;
				let output = sp_io::hashing::blake2_256(input.as_bytes_ref());
				Ok(output.abi_encode())
			},
			ISystemCalls::hashBlake128(ISystem::hashBlake128Call { input }) => {
				env.gas_meter_mut().charge(RuntimeCosts::HashBlake128(input.len() as u32))?;
				let output = sp_io::hashing::blake2_128(input.as_bytes_ref());
				Ok(output.abi_encode())
			},
			ISystemCalls::toAccountId(ISystem::toAccountIdCall { input }) => {
				use crate::address::AddressMapper;
				use codec::Encode;
				env.gas_meter_mut().charge(RuntimeCosts::ToAccountId)?;
				let account_id =
					T::AddressMapper::to_account_id(&H160::from_slice(input.as_slice()));
				Ok(account_id.encode().abi_encode())
			},
			ISystemCalls::callerIsOrigin(ISystem::callerIsOriginCall {}) => {
				env.gas_meter_mut().charge(RuntimeCosts::CallerIsOrigin)?;
				let is_origin = env.caller_is_origin();
				Ok(is_origin.abi_encode())
			},
			ISystemCalls::callerIsRoot(ISystem::callerIsRootCall {}) => {
				env.gas_meter_mut().charge(RuntimeCosts::CallerIsRoot)?;
				let is_root = env.caller_is_root();
				Ok(is_root.abi_encode())
			},
			ISystemCalls::ownCodeHash(ISystem::ownCodeHashCall {}) => {
				env.gas_meter_mut().charge(RuntimeCosts::OwnCodeHash)?;
				let address = env.address();
				let output = env.code_hash(&address).encode();
				Ok(output)
			},
			ISystemCalls::minimumBalance(ISystem::minimumBalanceCall {}) => {
				env.gas_meter_mut().charge(RuntimeCosts::MinimumBalance)?;
				let minimum_balance = T::Currency::minimum_balance();
				let minimum_balance_as_evm_value = env.convert_native_to_evm(minimum_balance);
				Ok(minimum_balance_as_evm_value.to_big_endian().abi_encode())
			},
			ISystemCalls::weightLeft(ISystem::weightLeftCall {}) => {
				env.gas_meter_mut().charge(RuntimeCosts::WeightLeft)?;
				let ref_time = env.gas_meter().gas_left().ref_time();
				let proof_size = env.gas_meter().gas_left().proof_size();
				let res = (ref_time, proof_size);
				Ok(res.abi_encode())
			},
		}
	}
}

#[cfg(test)]
mod tests {
	use super::{ISystem, *};
	use crate::{
		address::AddressMapper,
		call_builder::{caller_funding, CallSetup},
		exec::ExportedFunction::Call,
		gas::GasMeter,
		pallet,
		precompiles::{
			alloy::sol_types::{sol_data::Bytes, SolType},
			tests::run_test_vectors,
			BuiltinPrecompile,
		},
		storage,
		test_utils::{BOB, BOB_ADDR, GAS_LIMIT},
		tests::{test_utils::place_contract, ExtBuilder, Test},
		U256,
	};
	use assert_matches::assert_matches;
	use codec::Decode;
	use frame_support::traits::fungible::Mutate;

	#[test]
	fn test_system_precompile() {
		run_test_vectors::<System<Test>>(include_str!("testdata/900-blake2_256.json"));
		run_test_vectors::<System<Test>>(include_str!("testdata/900-blake2_128.json"));
		run_test_vectors::<System<Test>>(include_str!("testdata/900-to_account_id.json"));
	}

	#[test]
	fn test_system_precompile_unmapped_account() {
		ExtBuilder::default().build().execute_with(|| {
			// given
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();
			let unmapped_address = H160::zero();

			// when
			let input = ISystem::ISystemCalls::toAccountId(ISystem::toAccountIdCall {
				input: unmapped_address.0.into(),
			});
			let raw_data =
				<System<Test>>::call(&<System<Test>>::MATCHER.base_address(), &input, &mut ext)
					.unwrap();

			// then
			let expected_fallback_account_id =
				Bytes::abi_decode(&raw_data).expect("decoding failed");
			assert_eq!(
				expected_fallback_account_id.0.as_ref()[20..32],
				[0xEE; 12],
				"no fallback suffix found where one should be"
			);
		})
	}

	#[test]
	fn test_system_precompile_mapped_account() {
		use crate::test_utils::EVE;
		ExtBuilder::default().build().execute_with(|| {
			// given
			let mapped_address = {
				<Test as pallet::Config>::Currency::set_balance(&EVE, caller_funding::<Test>());
				let _ = <Test as pallet::Config>::AddressMapper::map(&EVE);
				<Test as pallet::Config>::AddressMapper::to_address(&EVE)
			};

			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			// when
			let input = ISystem::ISystemCalls::toAccountId(ISystem::toAccountIdCall {
				input: mapped_address.0.into(),
			});
			let raw_data =
				<System<Test>>::call(&<System<Test>>::MATCHER.base_address(), &input, &mut ext)
					.unwrap();

			// then
			let data = Bytes::abi_decode(&raw_data).expect("decoding failed");
			assert_ne!(
				data.0.as_ref()[20..32],
				[0xEE; 12],
				"fallback suffix found where none should be"
			);
			assert_eq!(
				<Test as frame_system::Config>::AccountId::decode(&mut data.as_ref()),
				Ok(EVE),
			);
		})
	}

	/*
	use crate::Weight;
	use alloy_core::sol_types::{sol_data::Bool};
	use crate::exec::Ext;
	use crate::exec::PrecompileExt;

	#[test]
	fn root_caller_succeeds() {
		let code_bob = crate::exec::tests::MockLoader::insert(Call, |ctx, _| {
			let ret = ctx.ext
			.delegate_call(
											   Weight::MAX,
											   U256::zero(),
											   H160::from(pallet_revive_uapi::SYSTEM_PRECOMPILE_ADDR),
											   pallet_revive_uapi::solidity_selector("callerIsRoot()").to_vec(),
									   )
									   .map(|_| ctx.ext.last_frame_output().clone());
						   let caller_is_root = Bool::abi_decode(&ret.unwrap().data).expect("decoding to bool failed");
						   assert!(caller_is_root);
			crate::exec::tests::exec_success()
		});

		ExtBuilder::default().build().execute_with(|| {
			place_contract(&BOB, code_bob);
			let result = crate::exec::tests::MockStack::run_call(
				Origin::Root,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage::meter::Meter::new(0),
				U256::zero(),
				vec![0],
				false,
			);
			assert_matches!(result, Ok(_));
		});
	}

	#[test]
	fn root_caller_fails() {
		let code_bob = crate::exec::tests::MockLoader::insert(Call, |ctx, _| {
			let ret = ctx.ext
				.delegate_call(
					Weight::MAX,
					U256::zero(),
					H160::from(pallet_revive_uapi::SYSTEM_PRECOMPILE_ADDR),
					pallet_revive_uapi::solidity_selector("callerIsRoot()").to_vec(),
				)
				.map(|_| ctx.ext.last_frame_output().clone());
			let caller_is_root = Bool::abi_decode(&ret.unwrap().data).expect("decoding to bool failed");
			assert!(!caller_is_root);
			crate::exec::tests::exec_success()
		});

		ExtBuilder::default().build().execute_with(|| {
			place_contract(&BOB, code_bob);
			let result = crate::exec::tests::MockStack::run_call(
				Origin::Signed(BOB),
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage::meter::Meter::new(0),
				U256::zero(),
				vec![0],
				false,
			);
			assert_matches!(result, Ok(_));
		});
	}
	*/
}
