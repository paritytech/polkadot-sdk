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

//! [`UnstableRuntime`] precompile implementation.
//!
//! Provides low-level access to runtime functionality from within a contract:
//! - [`IUnstableRuntime::dispatch`]: dispatch an arbitrary SCALE-encoded
//!   `RuntimeCall` as the calling contract's account (a `Signed` origin).
//! - [`IUnstableRuntime::storage`]: read the raw bytes of any runtime storage
//!   item by its full storage key.
//!
//! # Warning
//!
//! This interface is **unstable**:
//! - The runtime organization of pallets, indices, and storage keys might change
//!   between runtime upgrades.
//! - The encoding format might change between runtime upgrades.
//!
//! Contracts relying on it can break across upgrades. It is opt-in per runtime
//! and intended for experimentation, not production use.

use crate::precompiles::{alloy::sol, AddressMatcher, Error, Ext, Precompile};
use alloc::vec::Vec;
use core::{marker::PhantomData, num::NonZero};

sol! {
	/// Everything here is unstable; the runtime organization of pallets,
	/// indices, and storage keys might change between runtime upgrades.
	interface IUnstableRuntime {
		/// Dispatch a SCALE-encoded runtime `encoded_call` as the calling
		/// contract's account (a `Signed` origin).
		function dispatch(bytes encoded_call) external;

		/// Read the raw bytes of the runtime storage value at `key`.
		///
		/// Returns empty bytes if the key is absent. `max_len` is the caller's
		/// declared upper bound on the value length, used for metering.
		function storage(bytes key, uint32 max_len) external returns (bytes);
	}
}

/// Precompile that provides access to unstable runtime functionality.
pub struct UnstableRuntime<T>(PhantomData<T>);

impl<T: crate::Config> Precompile for UnstableRuntime<T> {
	type T = T;
	type Interface = IUnstableRuntime::IUnstableRuntimeCalls;
	// TODO: provisional address (see design doc O6); confirm before stabilising.
	const MATCHER: AddressMatcher = AddressMatcher::Fixed(NonZero::new(0x0100).unwrap());
	const HAS_CONTRACT_INFO: bool = false;

	fn call(
		_address: &[u8; 20],
		input: &Self::Interface,
		_env: &mut impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		use IUnstableRuntime::IUnstableRuntimeCalls;

		match input {
			IUnstableRuntimeCalls::dispatch(IUnstableRuntime::dispatchCall { encoded_call }) => {
				let _ = encoded_call;
				// TODO: implement dispatch (decode RuntimeCall, guards, charge,
				// dispatch as Signed(caller), adjust weight).
				Ok(Default::default())
			},
			IUnstableRuntimeCalls::storage(IUnstableRuntime::storageCall { key, max_len }) => {
				let _ = (key, max_len);
				// TODO: implement storage read (charge by max_len, length-aware
				// read against the main trie, ABI-encode the bytes return).
				Ok(Default::default())
			},
		}
	}
}

#[cfg(test)]
mod tests {
	use super::{IUnstableRuntime, UnstableRuntime};
	use crate::{
		call_builder::CallSetup,
		precompiles::{
			alloy::sol_types::{sol_data::Bytes, SolType},
			Error, Precompile,
		},
		test_utils::BOB,
		tests::{ExtBuilder, RuntimeCall, Test},
	};
	use codec::Encode;
	use frame_support::traits::fungible::Inspect;

	/// Build the `dispatch(encoded_call)` precompile input for a `RuntimeCall`.
	fn dispatch_input(call: RuntimeCall) -> IUnstableRuntime::IUnstableRuntimeCalls {
		IUnstableRuntime::IUnstableRuntimeCalls::dispatch(IUnstableRuntime::dispatchCall {
			encoded_call: call.encode().into(),
		})
	}

	/// Build the `storage(key, max_len)` precompile input.
	fn storage_input(key: &[u8], max_len: u32) -> IUnstableRuntime::IUnstableRuntimeCalls {
		IUnstableRuntime::IUnstableRuntimeCalls::storage(IUnstableRuntime::storageCall {
			key: key.to_vec().into(),
			max_len,
		})
	}

	fn address() -> [u8; 20] {
		<UnstableRuntime<Test> as Precompile>::MATCHER.base_address()
	}

	#[test]
	fn dispatch_executes_call_as_contract_account() {
		ExtBuilder::default().build().execute_with(|| {
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			let value = 1_000_000u128;
			let before = <Test as crate::Config>::Currency::balance(&BOB);

			// The caller account (ALICE) is funded by `CallSetup`. Dispatching a
			// transfer should execute as ALICE and move funds to BOB.
			let call = RuntimeCall::Balances(pallet_balances::Call::transfer_keep_alive {
				dest: BOB,
				value,
			});
			let input = dispatch_input(call);

			<UnstableRuntime<Test> as Precompile>::call(&address(), &input, &mut ext)
				.expect("dispatch should succeed");

			let after = <Test as crate::Config>::Currency::balance(&BOB);
			assert_eq!(after - before, value, "dispatch did not execute the transfer");
		});
	}

	#[test]
	fn dispatch_reverts_in_read_only() {
		ExtBuilder::default().build().execute_with(|| {
			let mut call_setup = CallSetup::<Test>::default();
			call_setup.set_read_only(true);
			let (mut ext, _) = call_setup.ext();

			let call = RuntimeCall::System(frame_system::Call::remark { remark: Vec::new() });
			let input = dispatch_input(call);

			let result =
				<UnstableRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);

			assert_eq!(
				result.unwrap_err(),
				Error::from(crate::Error::<Test>::StateChangeDenied),
			);
		});
	}

	#[test]
	fn dispatch_reverts_on_delegate_call() {
		ExtBuilder::default().build().execute_with(|| {
			let mut call_setup = CallSetup::<Test>::default();
			call_setup.set_delegate_call(true);
			let (mut ext, _) = call_setup.ext();

			let call = RuntimeCall::System(frame_system::Call::remark { remark: Vec::new() });
			let input = dispatch_input(call);

			let result =
				<UnstableRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);

			assert_eq!(
				result.unwrap_err(),
				Error::from(crate::Error::<Test>::PrecompileDelegateDenied),
			);
		});
	}

	#[test]
	fn dispatch_reverts_on_invalid_encoding() {
		ExtBuilder::default().build().execute_with(|| {
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			let input = IUnstableRuntime::IUnstableRuntimeCalls::dispatch(
				IUnstableRuntime::dispatchCall { encoded_call: vec![0xff, 0xff, 0xff].into() },
			);

			let result =
				<UnstableRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);

			assert_eq!(
				result.unwrap_err(),
				Error::Revert("invalid RuntimeCall encoding".into()),
			);
		});
	}

	#[test]
	fn storage_reads_raw_value() {
		ExtBuilder::default().build().execute_with(|| {
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			sp_io::storage::set(b"my_key", b"hello world");

			let input = storage_input(b"my_key", 64);
			let raw = <UnstableRuntime<Test> as Precompile>::call(&address(), &input, &mut ext)
				.expect("storage read should succeed");

			let decoded = Bytes::abi_decode(&raw).expect("return should abi-decode as bytes");
			assert_eq!(decoded.as_ref(), b"hello world");
		});
	}

	#[test]
	fn storage_absent_key_returns_empty() {
		ExtBuilder::default().build().execute_with(|| {
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			let input = storage_input(b"does_not_exist", 64);
			let raw = <UnstableRuntime<Test> as Precompile>::call(&address(), &input, &mut ext)
				.expect("storage read should succeed");

			let decoded = Bytes::abi_decode(&raw).expect("return should abi-decode as bytes");
			assert!(decoded.as_ref().is_empty(), "absent key should return empty bytes");
		});
	}
}
