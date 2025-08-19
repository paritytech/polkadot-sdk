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
use alloy_core::sol;
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
		function callerIsOrigin() external pure returns (bool);

		/// Checks whether the caller of the current contract is root.
		///
		/// Note that only the origin of the call stack can be root. Hence this
		/// function returning `true` implies that the contract is being called by the origin.
		///
		/// A return value of `true` indicates that this contract is being called by a root origin,
		/// and `false` indicates that the caller is a signed origin.
		function callerIsRoot() external pure returns (bool);

		/// Returns the minimum balance that is required for creating an account
		/// (the existential deposit).
		function minimumBalance() external pure returns (uint);

		/// Returns the code hash of the currently executing contract.
		function ownCodeHash() external pure returns (bytes32);

		/// Returns the amount of weight left.
		/// The data is encoded as `Weight`.
		function weightLeft() external pure returns (uint);
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
				Ok(output.to_vec())
			},
			ISystemCalls::hashBlake128(ISystem::hashBlake128Call { input }) => {
				env.gas_meter_mut().charge(RuntimeCosts::HashBlake128(input.len() as u32))?;
				let output = sp_io::hashing::blake2_128(input.as_bytes_ref());
				Ok(output.to_vec())
			},
			ISystemCalls::toAccountId(ISystem::toAccountIdCall { input }) => {
				use crate::address::AddressMapper;
				use codec::Encode;
				env.gas_meter_mut().charge(RuntimeCosts::ToAccountId)?;
				let account_id =
					T::AddressMapper::to_account_id(&crate::H160::from_slice(input.as_slice()));
				Ok(account_id.encode())
			},
			ISystemCalls::callerIsOrigin(ISystem::callerIsOriginCall {}) => {
				env.gas_meter_mut().charge(RuntimeCosts::CallerIsOrigin)?;
				let is_origin = match env.origin() {
					Origin::Root => {
						// `Root` does not have an address
						false
					},
					Origin::Signed(origin_account_id) => {
						let origin_address = T::AddressMapper::to_address(&origin_account_id).0;
						match env.caller() {
							Origin::Signed(caller_account_id) => {
								let caller_address =
									T::AddressMapper::to_address(&caller_account_id).0;
								origin_address == caller_address
							},
							Origin::Root => false,
						}
					},
				};
				Ok(vec![is_origin as u8])
			},
			ISystemCalls::callerIsRoot(ISystem::callerIsRootCall {}) => {
				env.gas_meter_mut().charge(RuntimeCosts::CallerIsRoot)?;
				let caller_is_root = match env.caller() {
					Origin::Root => true,
					Origin::Signed(_) => false,
				};
				Ok(vec![caller_is_root as u8])
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
				let output = minimum_balance.encode();
				Ok(output)
			},
			ISystemCalls::weightLeft(ISystem::weightLeftCall {}) => {
				env.gas_meter_mut().charge(RuntimeCosts::WeightLeft)?;
				let gas_left = env.gas_meter().gas_left().encode();
				Ok(gas_left)
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
		pallet,
		precompiles::{tests::run_test_vectors, BuiltinPrecompile},
		tests::{ExtBuilder, Test},
		H160,
	};
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
			let expected_fallback_account_id =
				<System<Test>>::call(&<System<Test>>::MATCHER.base_address(), &input, &mut ext)
					.unwrap();

			// then
			assert_eq!(
				expected_fallback_account_id[20..32],
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
			let data =
				<System<Test>>::call(&<System<Test>>::MATCHER.base_address(), &input, &mut ext)
					.unwrap();

			// then
			assert_ne!(
				data.as_slice()[20..32],
				[0xEE; 12],
				"fallback suffix found where none should be"
			);
			assert_eq!(
				<Test as frame_system::Config>::AccountId::decode(&mut data.as_slice()),
				Ok(EVE),
			);
		})
	}
}
