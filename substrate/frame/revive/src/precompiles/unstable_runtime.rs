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

use crate::{
	exec::Origin,
	precompiles::{alloy::sol, AddressMatcher, Error, Ext, Precompile},
	vm::RuntimeCosts,
};
use alloc::vec::Vec;
use alloy_core::sol_types::SolValue;
use codec::Decode;
use core::{marker::PhantomData, num::NonZero};
use frame_support::{
	dispatch::{extract_actual_weight, GetDispatchInfo},
	traits::Get,
	weights::Weight,
};
use frame_system::RawOrigin;
use sp_runtime::traits::Dispatchable;

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
		env: &mut impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		use IUnstableRuntime::IUnstableRuntimeCalls;

		match input {
			IUnstableRuntimeCalls::dispatch(IUnstableRuntime::dispatchCall { encoded_call }) => {
				// Dispatching mutates state and acts with the contract's origin,
				// so it is forbidden in a static context or via delegate call.
				if env.is_read_only() {
					return Err(crate::Error::<T>::StateChangeDenied.into());
				}
				if env.is_delegate_call() {
					return Err(crate::Error::<T>::PrecompileDelegateDenied.into());
				}

				// Charge for decoding the (arbitrary length) call bytes up front,
				// so oversized input is metered even when it fails to decode.
				env.frame_meter_mut().charge_weight_token(RuntimeCosts::PrecompileDecode(
					encoded_call.len() as u32,
				))?;
				let call = <T as crate::Config>::RuntimeCall::decode(&mut &encoded_call[..])
					.map_err(|_| Error::Revert("invalid RuntimeCall encoding".into()))?;

				// Dispatch as the calling contract's account. Reject a Root caller
				// rather than silently dispatching with Root privileges.
				let origin = match env.caller() {
					Origin::Signed(account_id) => RawOrigin::Signed(account_id).into(),
					Origin::Root =>
						return Err(Error::Revert("root origin cannot dispatch".into())),
				};

				let info = call.get_dispatch_info();
				let charged = env.charge(info.call_weight)?;
				let result = call.dispatch(origin);
				let actual = extract_actual_weight(&result, &info);
				env.adjust_gas(charged, actual);

				match result {
					Ok(_) => Ok(Default::default()),
					Err(e) => Err(Error::from(e.error)),
				}
			},
			IUnstableRuntimeCalls::storage(IUnstableRuntime::storageCall { key, max_len }) => {
				let max_len = *max_len;

				// Charge for the read up front, bounded by the caller-declared
				// `max_len`. proof_size is proportional to the maximum number of
				// value bytes that could enter the PoV and cannot be refunded once
				// the node is read, so we bill the declared bound rather than the
				// actual length.
				// TODO(G2): replace with a benchmark that also accounts for the
				// worst-case trie node overhead of reading an item of this length.
				let read_weight = <T as frame_system::Config>::DbWeight::get()
					.reads(1)
					.saturating_add(Weight::from_parts(0, max_len as u64));
				env.charge(read_weight)?;

				// Read against the main trie (including the in-block overlay). The
				// returned length is the value's full length; if it exceeds the
				// declared bound we revert rather than return a truncated value.
				let mut buf = alloc::vec![0u8; max_len as usize];
				match sp_io::storage::read(key.as_ref(), &mut buf, 0) {
					None => Ok(Vec::<u8>::new().abi_encode()),
					Some(len) if len > max_len =>
						Err(Error::Revert("value exceeds max_len".into())),
					Some(len) => {
						buf.truncate(len as usize);
						Ok(buf.abi_encode())
					},
				}
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
			Error, Ext, Precompile,
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
	fn dispatch_charges_for_decoding_call_bytes() {
		// Measure the weight consumed when dispatching `len` bytes of (invalid)
		// call data. Invalid input reverts at decode time, so no dispatch
		// `call_weight` is charged and any difference is solely the decode cost —
		// which must be metered before the decode is attempted. Each measurement
		// runs in its own externalities to avoid duplicate-contract setup.
		let measure = |len: usize| {
			ExtBuilder::default().build().execute_with(|| {
				let mut call_setup = CallSetup::<Test>::default();
				let (mut ext, _) = call_setup.ext();
				let before = ext.frame_meter().weight_consumed();
				let input = IUnstableRuntime::IUnstableRuntimeCalls::dispatch(
					IUnstableRuntime::dispatchCall { encoded_call: vec![0xff; len].into() },
				);
				let _ =
					<UnstableRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);
				(ext.frame_meter().weight_consumed() - before).ref_time()
			})
		};

		let small = measure(8);
		let large = measure(8_192);
		assert!(
			large > small,
			"decoding a larger call must charge more weight (small={small}, large={large})",
		);
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

	#[test]
	fn storage_reverts_when_value_exceeds_max_len() {
		ExtBuilder::default().build().execute_with(|| {
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			sp_io::storage::set(b"big_key", &[0u8; 64]);

			// Declared bound is smaller than the actual value length.
			let input = storage_input(b"big_key", 16);
			let result =
				<UnstableRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);

			assert_eq!(result.unwrap_err(), Error::Revert("value exceeds max_len".into()));
		});
	}

	#[test]
	fn storage_charges_more_proof_size_for_larger_max_len() {
		// proof_size (PoV) is the dominant, non-refundable cost of a read and is
		// bounded by the caller-declared `max_len`, so the charge must grow with
		// it. Each measurement runs in its own externalities.
		let measure = |max_len: u32| {
			ExtBuilder::default().build().execute_with(|| {
				let mut call_setup = CallSetup::<Test>::default();
				let (mut ext, _) = call_setup.ext();
				let before = ext.frame_meter().weight_consumed();
				let input = storage_input(b"some_key", max_len);
				let _ =
					<UnstableRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);
				(ext.frame_meter().weight_consumed() - before).proof_size()
			})
		};

		let small = measure(16);
		let large = measure(16_384);
		assert!(
			large > small,
			"a larger max_len must charge more proof_size (small={small}, large={large})",
		);
	}
}
