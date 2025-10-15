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
	precompiles::{BuiltinAddressMatcher, BuiltinPrecompile, Error, Ext},
	storage::WriteOutcome,
	vm::RuntimeCosts,
	Config, Key,
};
use alloc::vec::Vec;
use alloy_core::sol_types::SolValue;
use core::{marker::PhantomData, num::NonZero};
use pallet_revive_uapi::{precompiles::storage::IStorage, StorageFlags};
use sp_core::hexdisplay::AsBytesRef;

pub struct Storage<T>(PhantomData<T>);

impl<T: Config> BuiltinPrecompile for Storage<T> {
	type T = T;
	type Interface = IStorage::IStorageCalls;
	const MATCHER: BuiltinAddressMatcher =
		BuiltinAddressMatcher::Fixed(NonZero::new(0x901).unwrap());
	const HAS_CONTRACT_INFO: bool = false;

	fn call(
		_address: &[u8; 20],
		input: &Self::Interface,
		env: &mut impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		// Benchmarks call the pre-compile functions directly, without the delegate
		// call overhead. The `delegate_call` overhead is benchmarked individually.
		#[cfg(not(feature = "runtime-benchmarks"))]
		if !env.is_delegate_call() {
			return Err(Error::Revert(
				"Storage precompile can only be called via delegate call".into(),
			));
		}

		use IStorage::IStorageCalls;
		let max_size = env.max_value_size();
		match input {
			IStorageCalls::clearStorage(IStorage::clearStorageCall { flags, key, isFixedKey }) => {
				let transient = is_transient(*flags)
					.map_err(|_| Error::Revert("invalid storage flag".into()))?;
				let costs = |len| {
					if transient {
						RuntimeCosts::ClearTransientStorage(len)
					} else {
						RuntimeCosts::ClearStorage(len)
					}
				};
				let charged = env.gas_meter_mut().charge(costs(max_size))?;
				let key = decode_key(key.as_bytes_ref(), *isFixedKey)
					.map_err(|_| Error::Revert("failed decoding key".into()))?;
				let outcome = if transient {
					env.set_transient_storage(&key, None, false)
						.map_err(|_| Error::Revert("failed setting transient storage".into()))?
				} else {
					env.set_storage(&key, None, false)
						.map_err(|_| Error::Revert("failed setting storage".into()))?
				};
				let contained_key = outcome != WriteOutcome::New;
				let ret = (contained_key, outcome.old_len());
				env.gas_meter_mut().adjust_gas(charged, costs(outcome.old_len()));
				Ok(ret.abi_encode())
			},
			IStorageCalls::containsStorage(IStorage::containsStorageCall {
				flags,
				key,
				isFixedKey,
			}) => {
				let transient = is_transient(*flags)
					.map_err(|_| Error::Revert("invalid storage flag".into()))?;
				let costs = |len| {
					if transient {
						RuntimeCosts::ContainsTransientStorage(len)
					} else {
						RuntimeCosts::ContainsStorage(len)
					}
				};
				let charged = env.gas_meter_mut().charge(costs(max_size))?;
				let key = decode_key(key.as_bytes_ref(), *isFixedKey)
					.map_err(|_| Error::Revert("failed decoding key".into()))?;
				let outcome = if transient {
					env.get_transient_storage_size(&key)
				} else {
					env.get_storage_size(&key)
				};
				let value_len = outcome.unwrap_or(0);
				let ret = (outcome.is_some(), value_len);
				env.gas_meter_mut().adjust_gas(charged, costs(value_len));
				Ok(ret.abi_encode())
			},
			IStorageCalls::takeStorage(IStorage::takeStorageCall { flags, key, isFixedKey }) => {
				let transient = is_transient(*flags)
					.map_err(|_| Error::Revert("invalid storage flag".into()))?;
				let costs = |len| {
					if transient {
						RuntimeCosts::TakeTransientStorage(len)
					} else {
						RuntimeCosts::TakeStorage(len)
					}
				};
				let charged = env.gas_meter_mut().charge(costs(max_size))?;
				let key = decode_key(key.as_bytes_ref(), *isFixedKey)
					.map_err(|_| Error::Revert("failed decoding key".into()))?;
				let outcome = if transient {
					env.set_transient_storage(&key, None, true)?
				} else {
					env.set_storage(&key, None, true)?
				};

				if let crate::storage::WriteOutcome::Taken(value) = outcome {
					env.gas_meter_mut().adjust_gas(charged, costs(value.len() as u32));
					Ok(value.abi_encode())
				} else {
					env.gas_meter_mut().adjust_gas(charged, costs(0));
					Ok(Vec::<u8>::new().abi_encode())
				}
			},
		}
	}
}

struct InvalidStorageFlag();
fn is_transient(flags: u32) -> Result<bool, InvalidStorageFlag> {
	StorageFlags::from_bits(flags)
		.ok_or_else(InvalidStorageFlag)
		.map(|flags| flags.contains(StorageFlags::TRANSIENT))
}

fn decode_key(key_bytes: &[u8], is_fixed_key: bool) -> Result<Key, ()> {
	match is_fixed_key {
		true => {
			if key_bytes.len() != 32 {
				return Err(());
			}
			let mut decode_buf = [0u8; 32];
			decode_buf[..32].copy_from_slice(&key_bytes[..32]);
			Ok(Key::from_fixed(decode_buf))
		},
		false => {
			if key_bytes.len() as u32 > crate::limits::STORAGE_KEY_BYTES {
				return Err(());
			}
			Key::try_from_var(key_bytes.to_vec())
		},
	}
}

#[cfg(not(feature = "runtime-benchmarks"))]
#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		call_builder::CallSetup,
		precompiles::BuiltinPrecompile,
		tests::{ExtBuilder, Test},
	};

	#[test]
	fn revert_if_called_without_delegate_call() {
		ExtBuilder::default().build().execute_with(|| {
			// given
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			// when
			let input = IStorage::IStorageCalls::clearStorage(IStorage::clearStorageCall {
				flags: StorageFlags::empty().bits().into(),
				key: [0u8; 32].into(),
				isFixedKey: true,
			});
			let raw_data =
				<Storage<Test>>::call(&<Storage<Test>>::MATCHER.base_address(), &input, &mut ext);
			assert_eq!(
				raw_data.unwrap_err(),
				Error::Revert("Storage precompile can only be called via delegate call".into(),)
			);
		})
	}
}
