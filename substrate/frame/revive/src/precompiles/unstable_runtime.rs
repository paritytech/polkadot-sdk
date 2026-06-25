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
//! - `dispatch`: dispatch an arbitrary SCALE-encoded `RuntimeCall` as the calling contract's
//!   account (a `Signed` origin).
//! - `getStorage`: read the raw bytes of any runtime storage item by its full storage key.
//!
//! # Warning
//!
//! This interface is **unstable**:
//! - The runtime organization of pallets, indices, and storage keys might change between runtime
//!   upgrades.
//! - The encoding format might change between runtime upgrades.
//!
//! Contracts relying on it can break across upgrades. It is opt-in per runtime
//! and intended for experimentation, not production use.

use crate::{
	exec::Origin,
	limits,
	precompiles::{AddressMatcher, Error, Ext, Precompile, alloy::sol},
	vm::RuntimeCosts,
	weights::WeightInfo,
};
use alloc::vec::Vec;
use alloy_core::sol_types::SolValue;
use codec::DecodeLimit;
use core::{marker::PhantomData, num::NonZero};
use frame_support::{
	MAX_EXTRINSIC_DEPTH,
	dispatch::{GetDispatchInfo, extract_actual_weight},
	traits::{Contains, Everything, IsType, OriginTrait},
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
		function getStorage(bytes key, uint32 max_len) external returns (bytes);
	}
}

/// Precompile that provides access to unstable runtime functionality.
///
/// `Filter` lets a runtime restrict which `RuntimeCall`s may be dispatched
/// through this precompile. Defaults to [`Everything`] (no restriction).
///
/// The filter is attached to the dispatch origin, so it is enforced
/// transitively on calls nested inside synchronous wrappers such as
/// `Utility::batch` and `Proxy::proxy` — not only on the top-level call. It does
/// **not** cover calls that are stored and re-dispatched later by
/// deferred-execution pallets (e.g. `Scheduler`), which run with the runtime's
/// `BaseCallFilter` only; to bar those, deny the deferral call itself (e.g.
/// `Scheduler::schedule`) in the filter, or rely on `BaseCallFilter` for hard
/// restrictions.
pub struct UnstableRuntime<T, Filter = Everything>(PhantomData<(T, Filter)>);

impl<T: crate::Config, Filter> Precompile for UnstableRuntime<T, Filter>
where
	Filter: Contains<<T as crate::Config>::RuntimeCall>,
{
	type T = T;
	type Interface = IUnstableRuntime::IUnstableRuntimeCalls;
	// Fixed external precompile address. The `u16` occupies bytes [16,17] of the
	// 20-byte address, resolving to
	// `0x0000000000000000000000000000000009030000`. Chosen adjacent to the vesting
	// precompile (`0x0902`) and clear of the asset/XCM precompiles (`0x0120`-`0x0420`,
	// `0x000A`) so it does not collide in revive runtimes.
	const MATCHER: AddressMatcher = AddressMatcher::Fixed(NonZero::new(0x0903).unwrap());
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
				// Strict decode: reject trailing bytes and bound the nesting depth so
				// deeply nested calls cannot overflow the stack during decoding.
				// here i am using MAX_EXTRINSIC_DEPTH because That's the same depth limit the
				// runtime applies to normal extrinsics. Using it means this precompile accepts
				// exactly the calls that would be valid if a user submitted them as a
				// transaction
				let call = <T as crate::Config>::RuntimeCall::decode_all_with_depth_limit(
					MAX_EXTRINSIC_DEPTH,
					&mut &encoded_call[..],
				)
				.map_err(|_| Error::Revert("invalid RuntimeCall encoding".into()))?;

				// Let the runtime restrict which calls may be dispatched through
				// this precompile (defaults to allowing everything). Redundant with
				// the origin-attached filter below for *blocking*, but kept so a
				// filtered top-level call reverts with this clear message instead of
				// the generic `frame_system` "call filtered" error.
				if !Filter::contains(&call) {
					return Err(Error::Revert("call not allowed by filter".into()));
				}

				// Charge the fixed overhead of the dispatch wrapper (dispatch-info
				// computation and origin construction). The dispatched call's own
				// weight is charged separately below.
				env.charge(<T as crate::Config>::WeightInfo::unstable_runtime_dispatch())?;

				// Dispatch as the calling contract's account. Reject a Root caller
				// rather than silently dispatching with Root privileges.
				let mut origin: <T as frame_system::Config>::RuntimeOrigin = match env.caller() {
					Origin::Signed(account_id) => RawOrigin::Signed(account_id).into(),
					Origin::Root => {
						return Err(Error::Revert("root origin cannot dispatch".into()));
					},
				};

				// Attach the filter to the origin so it is enforced transitively on
				// calls nested inside synchronous wrappers (`Utility::batch`,
				// `Proxy::proxy`, ...), not only on the top-level call checked above.
				origin.add_filter(|c: &<T as frame_system::Config>::RuntimeCall| {
					let c = <T as crate::Config>::RuntimeCall::from_ref(c);
					Filter::contains(c)
				});

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
			IUnstableRuntimeCalls::getStorage(IUnstableRuntime::getStorageCall {
				key,
				max_len,
			}) => {
				let max_len = *max_len;

				// Bound the declared read length. This caps the return size and,
				// together with the upfront charge below, gates the `max_len`-sized
				// allocation against the gas budget.
				if max_len > limits::CALLDATA_BYTES {
					return Err(Error::Revert("max_len too large".into()));
				}

				// Two-dimensional read weight: keyed on the key length (trie
				// traversal) and the value length `v` (bytes pulled into the PoV).
				let key_len = key.as_ref().len() as u32;
				let weight = |v: u32| {
					<T as crate::Config>::WeightInfo::unstable_runtime_get_storage(key_len, v)
				};

				// Charge the benchmarked read weight for the caller-declared bound
				// up front. This also gates the `max_len`-sized allocation: an
				// oversized `max_len` runs out of gas here, before we allocate.
				let charged = env.charge(weight(max_len))?;

				// Read against the main trie (including the in-block overlay).
				let mut buf = alloc::vec![0u8; max_len as usize];
				let len = match sp_io::storage::read(key.as_ref(), &mut buf, 0) {
					None => {
						env.adjust_gas(charged, weight(0));
						return Ok(Vec::<u8>::new().abi_encode());
					},
					Some(len) => len,
				};

				// The whole value entered the PoV regardless of `max_len`, so the
				// proof must be charged for its actual length on every path —
				// including the revert below — otherwise an oversized value could be
				// read into the proof for the price of a tiny `max_len`.
				if len > max_len {
					env.charge(weight(len).saturating_sub(weight(max_len)))?;
					return Err(Error::Revert("value exceeds max_len".into()));
				}

				buf.truncate(len as usize);
				env.adjust_gas(charged, weight(len));
				Ok(buf.abi_encode())
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
			Error, Ext, Precompile,
			alloy::sol_types::{SolType, sol_data::Bytes},
		},
		test_utils::BOB,
		tests::{ExtBuilder, RuntimeCall, Test},
		weights::WeightInfo,
	};
	use codec::Encode;
	use frame_support::traits::fungible::Inspect;

	/// Build the `dispatch(encoded_call)` precompile input for a `RuntimeCall`.
	fn dispatch_input(call: RuntimeCall) -> IUnstableRuntime::IUnstableRuntimeCalls {
		IUnstableRuntime::IUnstableRuntimeCalls::dispatch(IUnstableRuntime::dispatchCall {
			encoded_call: call.encode().into(),
		})
	}

	/// Build the `getStorage(key, max_len)` precompile input.
	fn storage_input(key: &[u8], max_len: u32) -> IUnstableRuntime::IUnstableRuntimeCalls {
		IUnstableRuntime::IUnstableRuntimeCalls::getStorage(IUnstableRuntime::getStorageCall {
			key: key.to_vec().into(),
			max_len,
		})
	}

	fn address() -> [u8; 20] {
		<UnstableRuntime<Test> as Precompile>::MATCHER.base_address()
	}

	/// A call filter that forbids all `Balances` calls. Used to exercise the
	/// configurable `Filter` parameter of the precompile.
	pub struct DenyBalances;
	impl frame_support::traits::Contains<RuntimeCall> for DenyBalances {
		fn contains(call: &RuntimeCall) -> bool {
			!matches!(call, RuntimeCall::Balances(_))
		}
	}

	#[test]
	fn dispatch_respects_call_filter() {
		ExtBuilder::default().build().execute_with(|| {
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			// A balances transfer that the `DenyBalances` filter forbids.
			let call = RuntimeCall::Balances(pallet_balances::Call::transfer_keep_alive {
				dest: BOB,
				value: 1,
			});
			let input = dispatch_input(call);

			let result = <UnstableRuntime<Test, DenyBalances> as Precompile>::call(
				&address(),
				&input,
				&mut ext,
			);

			assert_eq!(result.unwrap_err(), Error::Revert("call not allowed by filter".into()));
		});
	}

	#[test]
	fn dispatch_filter_applies_to_nested_batch_calls() {
		ExtBuilder::default().build().execute_with(|| {
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			let value = 1_000u128;
			let before = <Test as crate::Config>::Currency::balance(&BOB);

			// A denied `Balances` transfer wrapped in `Utility::batch`. A top-level
			// filter lets the batch through and the inner transfer would execute; an
			// origin-attached filter must reject the nested call so BOB is unpaid.
			let inner = RuntimeCall::Balances(pallet_balances::Call::transfer_keep_alive {
				dest: BOB,
				value,
			});
			let batch =
				RuntimeCall::Utility(pallet_utility::Call::batch { calls: alloc::vec![inner] });
			let input = dispatch_input(batch);

			let _ = <UnstableRuntime<Test, DenyBalances> as Precompile>::call(
				&address(),
				&input,
				&mut ext,
			);

			assert_eq!(
				<Test as crate::Config>::Currency::balance(&BOB),
				before,
				"filter must apply to calls nested inside Utility::batch",
			);
		});
	}

	#[test]
	fn dispatch_allows_permitted_nested_batch_calls() {
		ExtBuilder::default().build().execute_with(|| {
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			let value = 1_000u128;
			let before = <Test as crate::Config>::Currency::balance(&BOB);

			// Positive control: with the default `Everything` filter, a transfer
			// nested in `Utility::batch` must still execute (the origin-attached
			// filter does not over-block permitted calls).
			let inner = RuntimeCall::Balances(pallet_balances::Call::transfer_keep_alive {
				dest: BOB,
				value,
			});
			let batch =
				RuntimeCall::Utility(pallet_utility::Call::batch { calls: alloc::vec![inner] });
			let input = dispatch_input(batch);

			<UnstableRuntime<Test> as Precompile>::call(&address(), &input, &mut ext)
				.expect("dispatch should succeed");

			assert_eq!(
				<Test as crate::Config>::Currency::balance(&BOB) - before,
				value,
				"permitted nested batch call should execute",
			);
		});
	}

	#[test]
	fn dispatch_rejects_root_caller() {
		ExtBuilder::default().build().execute_with(|| {
			let mut call_setup = CallSetup::<Test>::default();
			// Make the precompile's caller a Root origin.
			call_setup.set_origin(crate::exec::Origin::Root);
			let (mut ext, _) = call_setup.ext();

			let call = RuntimeCall::System(frame_system::Call::remark { remark: Vec::new() });
			let input = dispatch_input(call);

			let result = <UnstableRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);

			assert_eq!(result.unwrap_err(), Error::Revert("root origin cannot dispatch".into()));
		});
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

			let result = <UnstableRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);

			assert_eq!(result.unwrap_err(), Error::from(crate::Error::<Test>::StateChangeDenied),);
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

			let result = <UnstableRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);

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

			let input =
				IUnstableRuntime::IUnstableRuntimeCalls::dispatch(IUnstableRuntime::dispatchCall {
					encoded_call: vec![0xff, 0xff, 0xff].into(),
				});

			let result = <UnstableRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);

			assert_eq!(result.unwrap_err(), Error::Revert("invalid RuntimeCall encoding".into()),);
		});
	}

	#[test]
	fn dispatch_rejects_trailing_bytes() {
		ExtBuilder::default().build().execute_with(|| {
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			// A valid call with extra bytes appended. A strict (`decode_all`) decode
			// must reject this rather than silently dispatch the leading call.
			let call = RuntimeCall::System(frame_system::Call::remark { remark: Vec::new() });
			let mut bytes = call.encode();
			bytes.push(0xff);
			let input =
				IUnstableRuntime::IUnstableRuntimeCalls::dispatch(IUnstableRuntime::dispatchCall {
					encoded_call: bytes.into(),
				});

			let result = <UnstableRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);

			assert_eq!(result.unwrap_err(), Error::Revert("invalid RuntimeCall encoding".into()));
		});
	}

	#[test]
	fn dispatch_rejects_deeply_nested_calls() {
		ExtBuilder::default().build().execute_with(|| {
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			// Nest `Utility::batch` far past `MAX_EXTRINSIC_DEPTH` so the bounded
			// decode rejects it before it can recurse the native stack. Built
			// inside-out with a loop (no native recursion during construction).
			let mut call = RuntimeCall::System(frame_system::Call::remark { remark: Vec::new() });
			for _ in 0..512 {
				call =
					RuntimeCall::Utility(pallet_utility::Call::batch { calls: alloc::vec![call] });
			}
			let input = dispatch_input(call);

			let result = <UnstableRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);

			assert_eq!(result.unwrap_err(), Error::Revert("invalid RuntimeCall encoding".into()));
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
				let _ = <UnstableRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);
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
			let result = <UnstableRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);

			assert_eq!(result.unwrap_err(), Error::Revert("value exceeds max_len".into()));
		});
	}

	#[test]
	fn storage_charges_more_proof_size_for_larger_value() {
		// proof_size (PoV) is the dominant cost of a read and scales with the
		// actual value length that enters the proof. The charge (after adjusting
		// the upfront `max_len` reservation down to the real length) must grow
		// with the value size. Each measurement runs in its own externalities.
		let measure = |value_len: usize| {
			ExtBuilder::default().build().execute_with(|| {
				sp_io::storage::set(b"some_key", &alloc::vec![0u8; value_len]);
				let mut call_setup = CallSetup::<Test>::default();
				let (mut ext, _) = call_setup.ext();
				let before = ext.frame_meter().weight_consumed();
				let input = storage_input(b"some_key", value_len as u32);
				let _ = <UnstableRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);
				(ext.frame_meter().weight_consumed() - before).proof_size()
			})
		};

		let small = measure(16);
		let large = measure(400);
		assert!(
			large > small,
			"a larger value must charge more proof_size (small={small}, large={large})",
		);
	}

	#[test]
	fn storage_charges_more_for_longer_key() {
		// The read weight is two-dimensional: a longer key traverses more trie
		// nodes, so the charge must grow with the key length, not only the value
		// length. Value is held constant (1 byte) so only the key dimension varies.
		let measure = |key: &[u8]| {
			ExtBuilder::default().build().execute_with(|| {
				sp_io::storage::set(key, b"v");
				let mut call_setup = CallSetup::<Test>::default();
				let (mut ext, _) = call_setup.ext();
				let before = ext.frame_meter().weight_consumed();
				let input = storage_input(key, 64);
				let _ = <UnstableRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);
				(ext.frame_meter().weight_consumed() - before).ref_time()
			})
		};

		let short = measure(&[1u8; 4]);
		let long = measure(&[1u8; 128]);
		assert!(long > short, "a longer key must charge more weight (short={short}, long={long})");
	}

	#[test]
	fn storage_reverts_when_max_len_too_large() {
		ExtBuilder::default().build().execute_with(|| {
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			let input = storage_input(b"k", crate::limits::CALLDATA_BYTES + 1);
			let result = <UnstableRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);

			assert_eq!(result.unwrap_err(), Error::Revert("max_len too large".into()));
		});
	}

	#[test]
	fn storage_charges_for_actual_proof_on_revert() {
		ExtBuilder::default().build().execute_with(|| {
			// A value much larger than the declared `max_len`. The whole value
			// enters the PoV when read, so even though we revert, proof_size must
			// be charged for the actual length — not the small `max_len`.
			let value_len = 400usize;
			sp_io::storage::set(b"big_value", &alloc::vec![0u8; value_len]);

			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			let before = ext.frame_meter().weight_consumed();
			let input = storage_input(b"big_value", 16);
			let result = <UnstableRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);
			let consumed = ext.frame_meter().weight_consumed() - before;

			assert_eq!(result.unwrap_err(), Error::Revert("value exceeds max_len".into()));

			let expected = <Test as crate::Config>::WeightInfo::unstable_runtime_get_storage(
				b"big_value".len() as u32,
				value_len as u32,
			)
			.proof_size();
			assert!(
				consumed.proof_size() >= expected,
				"revert must charge proof_size for the actual value length \
				 (consumed={}, expected>={expected})",
				consumed.proof_size(),
			);
		});
	}

	#[test]
	fn dispatch_propagates_inner_call_error() {
		ExtBuilder::default().build().execute_with(|| {
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			let before = <Test as crate::Config>::Currency::balance(&BOB);

			// Transfer far more than the caller (ALICE) holds: the call dispatches
			// but fails, and the failure must propagate (not silently succeed).
			let call = RuntimeCall::Balances(pallet_balances::Call::transfer_keep_alive {
				dest: BOB,
				value: u128::MAX,
			});
			let input = dispatch_input(call);

			let result = <UnstableRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);

			// Mapped to an `Error::Error` (the dispatch error), not a revert or `Ok`.
			assert!(
				matches!(result, Err(Error::Error(_))),
				"a failing dispatched call must propagate its error, got {result:?}",
			);
			assert_eq!(
				<Test as crate::Config>::Currency::balance(&BOB),
				before,
				"a failed transfer must not move funds",
			);
		});
	}

	#[test]
	fn storage_read_works_in_read_only_context() {
		ExtBuilder::default().build().execute_with(|| {
			sp_io::storage::set(b"ro_key", b"value");

			let mut call_setup = CallSetup::<Test>::default();
			// A static (read-only) context: reads are still permitted (unlike
			// `dispatch`, which reverts here).
			call_setup.set_read_only(true);
			let (mut ext, _) = call_setup.ext();

			let raw = <UnstableRuntime<Test> as Precompile>::call(
				&address(),
				&storage_input(b"ro_key", 64),
				&mut ext,
			)
			.expect("getStorage must succeed in a read-only context");

			let decoded = Bytes::abi_decode(&raw).expect("decode");
			assert_eq!(decoded.as_ref(), b"value");
		});
	}

	#[test]
	fn storage_returns_value_exactly_at_max_len() {
		ExtBuilder::default().build().execute_with(|| {
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			// Boundary: the value length equals `max_len` exactly (`len > max_len`
			// is false), so it is returned rather than reverted.
			let value = alloc::vec![7u8; 32];
			sp_io::storage::set(b"exact", &value);
			let raw = <UnstableRuntime<Test> as Precompile>::call(
				&address(),
				&storage_input(b"exact", 32),
				&mut ext,
			)
			.expect("a value exactly at max_len must be returned");

			let decoded = Bytes::abi_decode(&raw).expect("decode");
			assert_eq!(decoded.as_ref(), value.as_slice());
		});
	}

	#[test]
	fn storage_allows_max_len_equal_to_calldata_bytes() {
		ExtBuilder::default().build().execute_with(|| {
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			// Boundary: the cap is strictly `>`, so `max_len == CALLDATA_BYTES` is
			// allowed (absent key → empty bytes, not a "too large" revert).
			let raw = <UnstableRuntime<Test> as Precompile>::call(
				&address(),
				&storage_input(b"absent", crate::limits::CALLDATA_BYTES),
				&mut ext,
			)
			.expect("max_len == CALLDATA_BYTES must be allowed");

			let decoded = Bytes::abi_decode(&raw).expect("decode");
			assert!(decoded.as_ref().is_empty());
		});
	}

	#[test]
	fn storage_max_len_zero_reverts_for_non_empty_value() {
		ExtBuilder::default().build().execute_with(|| {
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			// `max_len == 0` against a present, non-empty value: the value length
			// exceeds the bound, so it reverts.
			sp_io::storage::set(b"nonempty", b"x");
			let result = <UnstableRuntime<Test> as Precompile>::call(
				&address(),
				&storage_input(b"nonempty", 0),
				&mut ext,
			);
			assert_eq!(result.unwrap_err(), Error::Revert("value exceeds max_len".into()));
		});
	}

	#[test]
	fn storage_empty_value_and_absent_key_both_return_empty() {
		ExtBuilder::default().build().execute_with(|| {
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			// A key present with an EMPTY value and an absent key both decode to
			// empty bytes — the precompile does not distinguish them.
			sp_io::storage::set(b"empty_val", b"");
			let present = <UnstableRuntime<Test> as Precompile>::call(
				&address(),
				&storage_input(b"empty_val", 64),
				&mut ext,
			)
			.expect("read");
			let absent = <UnstableRuntime<Test> as Precompile>::call(
				&address(),
				&storage_input(b"never_set", 64),
				&mut ext,
			)
			.expect("read");

			let present = Bytes::abi_decode(&present).expect("decode");
			let absent = Bytes::abi_decode(&absent).expect("decode");
			assert!(present.as_ref().is_empty());
			assert_eq!(present.as_ref(), absent.as_ref());
		});
	}
}
