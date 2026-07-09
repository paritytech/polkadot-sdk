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

//! [`UncheckedRuntime`] precompile implementation.
//!
//! Provides low-level, **unchecked** access to runtime functionality from within a
//! contract:
//! - `dispatch`: dispatch an arbitrary SCALE-encoded `RuntimeCall` as the calling contract's
//!   account (a `Signed` origin).
//! - `getStorage`: read the raw bytes of any runtime storage item by its full storage key.
//!
//! # Why "unchecked"
//!
//! The precompile operates on **raw, positionally-encoded** input: a `dispatch`
//! payload is `[pallet_index][call_index][SCALE-encoded args]` and a `getStorage`
//! key is a raw storage key. Those are *implementation details* of the runtime, not
//! a stable ABI — a runtime upgrade can change what a given byte sequence means. The
//! precompile does **not** (and cannot, on-chain) verify that the bytes a contract
//! submits still carry the semantics the author intended. Calling it with stale
//! assumptions is undefined behaviour — hence "unchecked". It is opt-in per runtime
//! (the `unchecked-precompiles` cargo feature plus explicit inclusion in
//! `Config::Precompiles`) and intended for experimentation, not production use.
//!
//! # What a runtime upgrade can change, and what `dispatch` does about it
//!
//! `dispatch` decodes the input strictly (`decode_all_with_depth_limit`), which
//! catches *structural* drift but is blind to *semantic* drift:
//!
//! | Upgrade change | Outcome |
//! |---|---|
//! | Field added/removed/reordered; width-changing type swap (`u64`→`u128`); compact toggled; vacant pallet/call index | **decode fails → reverts** (safe) |
//! | A pallet/call index is **reassigned** to a *different* call whose argument layout still fits | **silently dispatches the wrong call** ⚠️ |
//! | Same pallet/call/index, **same-width** argument-type change (e.g. `Permill`→`Perbill`, both `u32`) | **silently dispatches with different semantics** ⚠️ |
//! | Same encoding, changed call *behaviour* (logic/units) | **silently dispatches with different semantics** ⚠️ |
//! | Origin requirement tightened (`Signed`→`Root`), or call newly blocked by `BaseCallFilter` | dispatch reverts (`BadOrigin` / filtered) |
//!
//! The ⚠️ rows **cannot** be detected on-chain: distinguishing them requires
//! verifying the call's full *type signature* against runtime metadata, which is not
//! available during execution (and a name/index check would be duck-typing — it
//! misses the `Permill`→`Perbill` class entirely). Contracts that need that
//! guarantee must establish it themselves (e.g. pin and verify the encoding
//! off-chain against the runtime metadata). This precompile deliberately offers only
//! the raw primitive, not the safety layer.

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
	traits::{ConstU32, Contains, Everything, Get, IsType, OriginTrait},
};
use frame_system::RawOrigin;
use sp_runtime::traits::Dispatchable;

/// The largest storage value `getStorage` can return.
///
/// The value is ABI-encoded as `bytes` for the return trip (32-byte offset word,
/// 32-byte length word, data padded to a 32-byte boundary), and that envelope
/// counts against the `limits::CALLDATA_BYTES` return-data cap enforced on every
/// call frame — so the raw value must stay 64 bytes below the cap.
/// `MaxValueLen` configurations above this are clamped to it.
pub const MAX_RETURNABLE_VALUE_LEN: u32 = limits::CALLDATA_BYTES - 64;

sol! {
	/// Unchecked, raw access to the runtime. The pallet/call indices and storage
	/// keys are positional implementation details, not a stable ABI: a runtime
	/// upgrade can change what a given byte sequence means (see the module docs).
	interface IUncheckedRuntime {
		/// Dispatch a SCALE-encoded runtime `encoded_call` as the calling
		/// contract's account (a `Signed` origin).
		function dispatch(bytes encoded_call) external;

		/// Read the raw bytes of the runtime storage value at `key`.
		///
		/// Returns empty bytes if the key is absent. The returned value is bounded by
		/// the runtime's configured length limit, and the key by
		/// `limits::UNCHECKED_RUNTIME_KEY_BYTES`.
		function getStorage(bytes key) external returns (bytes);
	}
}

/// Precompile that provides unchecked, low-level access to runtime functionality.
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
///
/// `StorageFilter` restricts which storage keys `getStorage` may read (defaults to
/// [`Everything`]). Reading a key materialises its whole value into the PoV before
/// the size is known, so an unrestricted reader can pull a large value (e.g.
/// `:code:`) into the proof. Deny such keys here to bound that exposure; the runtime
/// that opts in is responsible for scoping the readable key space.
///
/// `MaxValueLen` is the value length `getStorage` charges for **before** the read,
/// regardless of the actual value (it is refunded down to the real length after).
/// This is what makes the read sound: a caller that cannot afford the worst case
/// runs out of gas before the value is pulled into the PoV. Set it together with
/// `StorageFilter` so that no readable key can hold a value larger than the limit.
/// The effective limit is clamped to [`MAX_RETURNABLE_VALUE_LEN`], since nothing
/// larger can be returned once the ABI envelope is added.
pub struct UncheckedRuntime<
	T,
	Filter = Everything,
	StorageFilter = Everything,
	MaxValueLen = ConstU32<{ MAX_RETURNABLE_VALUE_LEN }>,
>(PhantomData<(T, Filter, StorageFilter, MaxValueLen)>);

impl<T: crate::Config, Filter, StorageFilter, MaxValueLen> Precompile
	for UncheckedRuntime<T, Filter, StorageFilter, MaxValueLen>
where
	Filter: Contains<<T as crate::Config>::RuntimeCall>,
	StorageFilter: Contains<Vec<u8>>,
	MaxValueLen: Get<u32>,
{
	type T = T;
	type Interface = IUncheckedRuntime::IUncheckedRuntimeCalls;
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
		use IUncheckedRuntime::IUncheckedRuntimeCalls;

		match input {
			IUncheckedRuntimeCalls::dispatch(IUncheckedRuntime::dispatchCall { encoded_call }) => {
				// Mutates state and acts with the contract's origin — unsafe under
				// STATICCALL or DELEGATECALL.
				if env.is_read_only() {
					return Err(crate::Error::<T>::StateChangeDenied.into());
				}
				if env.is_delegate_call() {
					return Err(crate::Error::<T>::PrecompileDelegateDenied.into());
				}

				// Charge before decoding so oversized input is paid for even when it
				// fails to decode, and depth-bound the decode (as extrinsics are) to
				// prevent stack overflow.
				env.frame_meter_mut().charge_weight_token(RuntimeCosts::PrecompileDecode(
					encoded_call.len() as u32,
				))?;
				let call = <T as crate::Config>::RuntimeCall::decode_all_with_depth_limit(
					MAX_EXTRINSIC_DEPTH,
					&mut &encoded_call[..],
				)
				.map_err(|_| Error::Revert("invalid RuntimeCall encoding".into()))?;

				// Redundant with the origin filter below, but gives a clear revert
				// instead of the generic `frame_system` "call filtered" error.
				if !Filter::contains(&call) {
					return Err(Error::Revert("call not allowed by filter".into()));
				}

				env.charge(<T as crate::Config>::WeightInfo::unchecked_runtime_dispatch())?;

				// Reject a Root caller rather than dispatch with Root's privileges.
				let mut origin: <T as frame_system::Config>::RuntimeOrigin = match env.caller() {
					Origin::Signed(account_id) => RawOrigin::Signed(account_id).into(),
					Origin::Root => {
						return Err(Error::Revert("root origin cannot dispatch".into()));
					},
				};

				// Enforce the filter transitively on nested calls (`Utility::batch`,
				// `Proxy::proxy`, ...), not just the top-level call checked above.
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
			IUncheckedRuntimeCalls::getStorage(IUncheckedRuntime::getStorageCall { key }) => {
				// Keep the key length inside the benchmarked weight domain.
				let key_len = key.as_ref().len() as u32;
				if key_len > limits::UNCHECKED_RUNTIME_KEY_BYTES {
					return Err(Error::Revert("storage key too long".into()));
				}

				// A read materialises the whole value into the PoV before its size is
				// known, so gate the key before reading to bound that exposure.
				if !StorageFilter::contains(&key.as_ref().to_vec()) {
					return Err(Error::Revert("storage key not allowed by filter".into()));
				}

				// The clamp keeps a misconfigured `MaxValueLen` from producing return
				// data the frame would reject (and from evaluating the weight function
				// past its benchmarked range).
				let limit = MaxValueLen::get().min(MAX_RETURNABLE_VALUE_LEN);
				// Read weight is 2-D: key length (trie traversal) and value length
				// (bytes pulled into the PoV).
				let weight = |v: u32| {
					<T as crate::Config>::WeightInfo::unchecked_runtime_get_storage(key_len, v)
				};

				// Charge the worst case before the read, so a caller that cannot afford
				// the limit runs out of gas before the value is pulled into the PoV.
				// Refunded down to the actual length below.
				let charged = env.charge(weight(limit))?;

				let mut buf = alloc::vec![0u8; limit as usize];
				let len = match sp_io::storage::read(key.as_ref(), &mut buf, 0) {
					None => {
						env.adjust_gas(charged, weight(0));
						return Ok(Vec::<u8>::new().abi_encode());
					},
					Some(len) => len,
				};

				// A value above the limit means the `StorageFilter` and `MaxValueLen`
				// are misaligned (a config bug). The whole value is already in the PoV,
				// so charge its actual length; revert rather than hand back a silently
				// truncated value. Warn first, so the misconfiguration is visible even
				// when the charge below runs out of gas.
				if len > limit {
					log::warn!(
						target: crate::LOG_TARGET,
						"getStorage read a {len}-byte value over the {limit}-byte MaxValueLen; \
						 align the StorageFilter and MaxValueLen",
					);
					env.charge(weight(len).saturating_sub(weight(limit)))?;
					return Err(Error::Revert("value exceeds storage limit".into()));
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
	use super::{IUncheckedRuntime, UncheckedRuntime};
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
	use frame_support::traits::{ConstU32, Everything, fungible::Inspect};

	/// Build the `dispatch(encoded_call)` precompile input for a `RuntimeCall`.
	fn dispatch_input(call: RuntimeCall) -> IUncheckedRuntime::IUncheckedRuntimeCalls {
		IUncheckedRuntime::IUncheckedRuntimeCalls::dispatch(IUncheckedRuntime::dispatchCall {
			encoded_call: call.encode().into(),
		})
	}

	/// Build the `getStorage(key)` precompile input.
	fn storage_input(key: &[u8]) -> IUncheckedRuntime::IUncheckedRuntimeCalls {
		IUncheckedRuntime::IUncheckedRuntimeCalls::getStorage(IUncheckedRuntime::getStorageCall {
			key: key.to_vec().into(),
		})
	}

	fn address() -> [u8; 20] {
		<UncheckedRuntime<Test> as Precompile>::MATCHER.base_address()
	}

	/// Documents a limitation: `dispatch` cannot detect a same-width argument-type
	/// change (the ⚠️ row in the module docs). A contract that encoded a `Permill`
	/// has its bytes silently re-interpreted as a `Perbill` after an upgrade — the
	/// call dispatches with completely different semantics and nothing reverts.
	#[test]
	fn dispatch_cannot_detect_same_width_type_change() {
		use crate::tests::pallet_dummy;
		use sp_runtime::{Perbill, Permill};

		ExtBuilder::default().build().execute_with(|| {
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			// A 50% `Permill` and a 0.05% `Perbill` encode to the SAME 4 bytes (both
			// are the u32 `500_000`), so a strict decode cannot tell them apart.
			let intended = Permill::from_percent(50);
			assert_eq!(intended.encode(), Perbill::from_parts(500_000).encode());

			// Emulate a contract built against an OLDER runtime where `record_rate`
			// took a `Permill`: it ships `[pallet][call][Permill(50%) bytes]`. The
			// CURRENT `record_rate` takes a `Perbill` — same width, so it still decodes.
			let probe =
				RuntimeCall::Dummy(pallet_dummy::Call::record_rate { rate: Perbill::zero() });
			let mut stale_bytes = probe.encode()[..2].to_vec(); // pallet + call index
			stale_bytes.extend(intended.encode()); // the Permill-intended argument

			let input = IUncheckedRuntime::IUncheckedRuntimeCalls::dispatch(
				IUncheckedRuntime::dispatchCall { encoded_call: stale_bytes.into() },
			);

			// Dispatch SUCCEEDS — there is no protection against the semantic drift.
			<UncheckedRuntime<Test> as Precompile>::call(&address(), &input, &mut ext)
				.expect("stale bytes decode cleanly and dispatch");

			// The call silently ran with the WRONG meaning: 0.05% (Perbill) instead of
			// the 50% the contract intended.
			let recorded = pallet_dummy::RecordedRate::<Test>::get().expect("record_rate ran");
			assert_eq!(recorded, Perbill::from_parts(500_000)); // 0.05%
			assert_ne!(recorded, Perbill::from_percent(50)); // NOT the intended 50%
		});
	}

	/// A call filter that forbids all `Balances` calls. Used to exercise the
	/// configurable `Filter` parameter of the precompile.
	pub struct DenyBalances;
	impl frame_support::traits::Contains<RuntimeCall> for DenyBalances {
		fn contains(call: &RuntimeCall) -> bool {
			!matches!(call, RuntimeCall::Balances(_))
		}
	}

	/// A storage-key filter that forbids reading the `:code:` key. Used to exercise
	/// the configurable `StorageFilter` parameter of the precompile.
	pub struct DenyCodeKey;
	impl frame_support::traits::Contains<Vec<u8>> for DenyCodeKey {
		fn contains(key: &Vec<u8>) -> bool {
			!key.starts_with(b":code:")
		}
	}

	#[test]
	fn storage_respects_key_filter() {
		ExtBuilder::default().build().execute_with(|| {
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();
			sp_io::storage::set(b":code:", b"wasm-blob");

			let result = <UncheckedRuntime<Test, Everything, DenyCodeKey> as Precompile>::call(
				&address(),
				&storage_input(b":code:"),
				&mut ext,
			);

			assert_eq!(
				result.unwrap_err(),
				Error::Revert("storage key not allowed by filter".into()),
			);
		});
	}

	#[test]
	fn storage_value_len_config_is_clamped() {
		use super::MAX_RETURNABLE_VALUE_LEN;

		ExtBuilder::default().build().execute_with(|| {
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();
			sp_io::storage::set(b"small", b"v");

			// Without the clamp a `MaxValueLen` of `u32::MAX` would charge and
			// allocate for a 4 GiB value up front. Clamped, the read behaves as
			// with the default limit.
			let raw = <UncheckedRuntime<Test, Everything, Everything, ConstU32<{ u32::MAX }>>
				as Precompile>::call(&address(), &storage_input(b"small"), &mut ext)
				.expect("an oversized MaxValueLen must be clamped");
			let decoded = Bytes::abi_decode(&raw).expect("decode");
			assert_eq!(decoded.as_ref(), b"v");

			// The clamp bounds what is readable: one byte past the returnable
			// maximum reverts even under the u32::MAX configuration.
			sp_io::storage::set(
				b"over_max",
				&alloc::vec![7u8; MAX_RETURNABLE_VALUE_LEN as usize + 1],
			);
			let result = <UncheckedRuntime<Test, Everything, Everything, ConstU32<{ u32::MAX }>>
				as Precompile>::call(&address(), &storage_input(b"over_max"), &mut ext);
			assert_eq!(result.unwrap_err(), Error::Revert("value exceeds storage limit".into()));
		});
	}

	#[test]
	fn storage_key_length_is_capped() {
		ExtBuilder::default().build().execute_with(|| {
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			// A key exactly at the cap is readable (absent -> empty bytes)...
			let max = crate::limits::UNCHECKED_RUNTIME_KEY_BYTES as usize;
			<UncheckedRuntime<Test> as Precompile>::call(
				&address(),
				&storage_input(&vec![1u8; max]),
				&mut ext,
			)
			.expect("a key at the cap must be readable");

			// ...one byte past it reverts, keeping the charged weight inside the
			// benchmarked key-length range.
			let result = <UncheckedRuntime<Test> as Precompile>::call(
				&address(),
				&storage_input(&vec![1u8; max + 1]),
				&mut ext,
			);
			assert_eq!(result.unwrap_err(), Error::Revert("storage key too long".into()));
		});
	}

	#[test]
	fn storage_filter_allows_permitted_keys() {
		ExtBuilder::default().build().execute_with(|| {
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();
			sp_io::storage::set(b"allowed", b"value");

			let raw = <UncheckedRuntime<Test, Everything, DenyCodeKey> as Precompile>::call(
				&address(),
				&storage_input(b"allowed"),
				&mut ext,
			)
			.expect("a permitted key is readable");

			let decoded = Bytes::abi_decode(&raw).expect("decode");
			assert_eq!(decoded.as_ref(), b"value");
		});
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

			let result = <UncheckedRuntime<Test, DenyBalances> as Precompile>::call(
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

			let _ = <UncheckedRuntime<Test, DenyBalances> as Precompile>::call(
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

			<UncheckedRuntime<Test> as Precompile>::call(&address(), &input, &mut ext)
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

			let result = <UncheckedRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);

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

			<UncheckedRuntime<Test> as Precompile>::call(&address(), &input, &mut ext)
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

			let result = <UncheckedRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);

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

			let result = <UncheckedRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);

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

			let input = IUncheckedRuntime::IUncheckedRuntimeCalls::dispatch(
				IUncheckedRuntime::dispatchCall { encoded_call: vec![0xff, 0xff, 0xff].into() },
			);

			let result = <UncheckedRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);

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
			let input = IUncheckedRuntime::IUncheckedRuntimeCalls::dispatch(
				IUncheckedRuntime::dispatchCall { encoded_call: bytes.into() },
			);

			let result = <UncheckedRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);

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

			let result = <UncheckedRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);

			assert_eq!(result.unwrap_err(), Error::Revert("invalid RuntimeCall encoding".into()));
		});
	}

	#[test]
	fn dispatch_rejects_empty_call() {
		ExtBuilder::default().build().execute_with(|| {
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			// Empty input is not a valid `RuntimeCall`.
			let input = IUncheckedRuntime::IUncheckedRuntimeCalls::dispatch(
				IUncheckedRuntime::dispatchCall { encoded_call: Vec::new().into() },
			);

			let result = <UncheckedRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);

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
				let input = IUncheckedRuntime::IUncheckedRuntimeCalls::dispatch(
					IUncheckedRuntime::dispatchCall { encoded_call: vec![0xff; len].into() },
				);
				let _ = <UncheckedRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);
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

			let input = storage_input(b"my_key");
			let raw = <UncheckedRuntime<Test> as Precompile>::call(&address(), &input, &mut ext)
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

			let input = storage_input(b"does_not_exist");
			let raw = <UncheckedRuntime<Test> as Precompile>::call(&address(), &input, &mut ext)
				.expect("storage read should succeed");

			let decoded = Bytes::abi_decode(&raw).expect("return should abi-decode as bytes");
			assert!(decoded.as_ref().is_empty(), "absent key should return empty bytes");
		});
	}

	#[test]
	fn storage_reverts_when_value_exceeds_limit() {
		ExtBuilder::default().build().execute_with(|| {
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			// The runtime limit is 8 bytes; the stored value is larger.
			sp_io::storage::set(b"big_key", &[0u8; 64]);

			let result =
				<UncheckedRuntime<Test, Everything, Everything, ConstU32<8>> as Precompile>::call(
					&address(),
					&storage_input(b"big_key"),
					&mut ext,
				);

			assert_eq!(result.unwrap_err(), Error::Revert("value exceeds storage limit".into()));
		});
	}

	#[test]
	fn storage_charges_more_proof_size_for_larger_value() {
		// proof_size (PoV) is the dominant cost of a read and scales with the
		// actual value length that enters the proof. The charge (after adjusting
		// the upfront limit reservation down to the real length) must grow with
		// the value size. Each measurement runs in its own externalities.
		let measure = |value_len: usize| {
			ExtBuilder::default().build().execute_with(|| {
				sp_io::storage::set(b"some_key", &alloc::vec![0u8; value_len]);
				let mut call_setup = CallSetup::<Test>::default();
				let (mut ext, _) = call_setup.ext();
				let before = ext.frame_meter().weight_consumed();
				let input = storage_input(b"some_key");
				let _ = <UncheckedRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);
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
				let input = storage_input(key);
				let _ = <UncheckedRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);
				(ext.frame_meter().weight_consumed() - before).ref_time()
			})
		};

		let short = measure(&[1u8; 4]);
		let long = measure(&[1u8; 128]);
		assert!(long > short, "a longer key must charge more weight (short={short}, long={long})");
	}

	#[test]
	fn storage_charges_for_actual_proof_on_revert() {
		ExtBuilder::default().build().execute_with(|| {
			// A value much larger than the limit. The whole value enters the PoV when
			// read, so even though we revert, proof_size must be charged for the actual
			// length — not the (smaller) limit.
			let value_len = 400usize;
			sp_io::storage::set(b"big_value", &alloc::vec![0u8; value_len]);

			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			let before = ext.frame_meter().weight_consumed();
			let result =
				<UncheckedRuntime<Test, Everything, Everything, ConstU32<16>> as Precompile>::call(
					&address(),
					&storage_input(b"big_value"),
					&mut ext,
				);
			let consumed = ext.frame_meter().weight_consumed() - before;

			assert_eq!(result.unwrap_err(), Error::Revert("value exceeds storage limit".into()));

			let expected = <Test as crate::Config>::WeightInfo::unchecked_runtime_get_storage(
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

			let result = <UncheckedRuntime<Test> as Precompile>::call(&address(), &input, &mut ext);

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

			let raw = <UncheckedRuntime<Test> as Precompile>::call(
				&address(),
				&storage_input(b"ro_key"),
				&mut ext,
			)
			.expect("getStorage must succeed in a read-only context");

			let decoded = Bytes::abi_decode(&raw).expect("decode");
			assert_eq!(decoded.as_ref(), b"value");
		});
	}

	#[test]
	fn storage_returns_value_exactly_at_limit() {
		ExtBuilder::default().build().execute_with(|| {
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			// Boundary: value length equals the limit exactly (`len > limit` is false),
			// so it is returned rather than reverted.
			let value = alloc::vec![7u8; 32];
			sp_io::storage::set(b"exact", &value);
			let raw =
				<UncheckedRuntime<Test, Everything, Everything, ConstU32<32>> as Precompile>::call(
					&address(),
					&storage_input(b"exact"),
					&mut ext,
				)
				.expect("a value exactly at the limit must be returned");

			let decoded = Bytes::abi_decode(&raw).expect("decode");
			assert_eq!(decoded.as_ref(), value.as_slice());
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
			let present = <UncheckedRuntime<Test> as Precompile>::call(
				&address(),
				&storage_input(b"empty_val"),
				&mut ext,
			)
			.expect("read");
			let absent = <UncheckedRuntime<Test> as Precompile>::call(
				&address(),
				&storage_input(b"never_set"),
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
