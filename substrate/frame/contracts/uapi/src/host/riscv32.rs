#![allow(unused_variables, unused_mut)]
// Copyright (C) Parity Technologies (UK) Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
// TODO: bring up to date with wasm32.rs

use super::{CallFlags, HostFn, HostFnImpl, Result};
use crate::ReturnFlags;

/// A macro to implement all Host functions with a signature of `fn(&mut &mut [u8])`.
///
/// Example:
/// ```nocompile
// impl_wrapper_for! {
//     () => [gas_left],
//     (v1) => [gas_left],
// }
// ```
// 
// Expands to:
// ```nocompile
// fn gas_left(output: &mut &mut [u8]) {
//     unsafe { sys::gas_left(...); }
// }
// fn gas_left_v1(output: &mut &mut [u8]) {
//     unsafe { sys::v1::gas_left(...); }
// }
// ```
macro_rules! impl_wrapper_for {
	(@impl_fn $( $mod:ident )::*, $suffix_sep: literal, $suffix:tt, $name:ident) => {
		paste::paste! {
			fn [<$name $suffix_sep $suffix>](output: &mut &mut [u8]) {
				todo!()
			}
		}
	};

	() => {};

	(($mod:ident) => [$( $name:ident),*], $($tail:tt)*) => {
		$(impl_wrapper_for!(@impl_fn sys::$mod, "_", $mod, $name);)*
		impl_wrapper_for!($($tail)*);
	};

	(() =>	[$( $name:ident),*], $($tail:tt)*) => {
		$(impl_wrapper_for!(@impl_fn sys, "", "", $name);)*
		impl_wrapper_for!($($tail)*);
	};
}

/// A macro to implement all the hash functions Apis.
macro_rules! impl_hash_fn {
	( $name:ident, $bytes_result:literal ) => {
		paste::item! {
			fn [<hash_ $name>](input: &[u8], output: &mut [u8; $bytes_result]) {
				todo!()
			}
		}
	};
}

/// A macro to implement the get_storage functions.
macro_rules! impl_get_storage {
	($fn_name:ident, $sys_get_storage:path) => {
		fn $fn_name(key: &[u8], output: &mut &mut [u8]) -> Result {
			todo!()
		}
	};
}

impl HostFn for HostFnImpl {
	fn instantiate_v1(
		code_hash: &[u8],
		gas: u64,
		value: &[u8],
		input: &[u8],
		mut address: Option<&mut &mut [u8]>,
		mut output: Option<&mut &mut [u8]>,
		salt: &[u8],
	) -> Result {
		todo!()
	}

	fn instantiate_v2(
		code_hash: &[u8],
		ref_time_limit: u64,
		proof_size_limit: u64,
		deposit: Option<&[u8]>,
		value: &[u8],
		input: &[u8],
		mut address: Option<&mut &mut [u8]>,
		mut output: Option<&mut &mut [u8]>,
		salt: &[u8],
	) -> Result {
		todo!()
	}

	fn call(
		callee: &[u8],
		gas: u64,
		value: &[u8],
		input_data: &[u8],
		mut output: Option<&mut &mut [u8]>,
	) -> Result {
		todo!()
	}

	fn call_v1(
		flags: CallFlags,
		callee: &[u8],
		gas: u64,
		value: &[u8],
		input_data: &[u8],
		mut output: Option<&mut &mut [u8]>,
	) -> Result {
		todo!()
	}

	fn call_v2(
		flags: CallFlags,
		callee: &[u8],
		ref_time_limit: u64,
		proof_time_limit: u64,
		deposit: Option<&[u8]>,
		value: &[u8],
		input_data: &[u8],
		mut output: Option<&mut &mut [u8]>,
	) -> Result {
		todo!()
	}

	fn caller_is_root() -> u32 {
		todo!()
	}

	fn delegate_call(
		flags: CallFlags,
		code_hash: &[u8],
		input: &[u8],
		mut output: Option<&mut &mut [u8]>,
	) -> Result {
		todo!()
	}

	fn transfer(account_id: &[u8], value: &[u8]) -> Result {
		todo!()
	}

	fn deposit_event(topics: &[u8], data: &[u8]) {
		todo!()
	}

	fn set_storage(key: &[u8], value: &[u8]) {
		todo!()
	}

	fn set_storage_v1(key: &[u8], encoded_value: &[u8]) -> Option<u32> {
		todo!()
	}

	fn set_storage_v2(key: &[u8], encoded_value: &[u8]) -> Option<u32> {
		todo!()
	}

	fn clear_storage(key: &[u8]) {
		todo!()
	}

	fn clear_storage_v1(key: &[u8]) -> Option<u32> {
		todo!()
	}

	impl_get_storage!(get_storage, sys::get_storage);
	impl_get_storage!(get_storage_v1, sys::v1::get_storage);

	fn take_storage(key: &[u8], output: &mut &mut [u8]) -> Result {
		todo!()
	}

	fn contains_storage(key: &[u8]) -> Option<u32> {
		todo!()
	}

	fn contains_storage_v1(key: &[u8]) -> Option<u32> {
		todo!()
	}

	fn terminate(beneficiary: &[u8]) -> ! {
		todo!()
	}

	fn terminate_v1(beneficiary: &[u8]) -> ! {
		todo!()
	}

	fn call_chain_extension(func_id: u32, input: &[u8], output: Option<&mut &mut [u8]>) -> u32 {
		todo!()
	}

	fn input(output: &mut &mut [u8]) {
		todo!()
	}

	fn return_value(flags: ReturnFlags, return_value: &[u8]) -> ! {
		todo!()
	}

	fn call_runtime(call: &[u8]) -> Result {
		todo!()
	}

	fn debug_message(str: &[u8]) -> Result {
		todo!()
	}

	impl_wrapper_for! {
		() => [caller, block_number, address, balance, gas_left, value_transferred, now, minimum_balance],
		(v1) => [gas_left],
	}

	fn weight_to_fee(gas: u64, output: &mut &mut [u8]) {
		todo!()
	}

	fn weight_to_fee_v1(ref_time_limit: u64, proof_size_limit: u64, output: &mut &mut [u8]) {
		todo!()
	}

	impl_hash_fn!(sha2_256, 32);
	impl_hash_fn!(keccak_256, 32);
	impl_hash_fn!(blake2_256, 32);
	impl_hash_fn!(blake2_128, 16);

	fn ecdsa_recover(
		signature: &[u8; 65],
		message_hash: &[u8; 32],
		output: &mut [u8; 33],
	) -> Result {
		todo!()
	}

	fn ecdsa_to_eth_address(pubkey: &[u8; 33], output: &mut [u8; 20]) -> Result {
		todo!()
	}

	fn sr25519_verify(signature: &[u8; 64], message: &[u8], pub_key: &[u8; 32]) -> Result {
		todo!()
	}

	fn is_contract(account_id: &[u8]) -> bool {
		todo!()
	}

	fn caller_is_origin() -> bool {
		todo!()
	}

	fn set_code_hash(code_hash: &[u8]) -> Result {
		todo!()
	}

	fn code_hash(account_id: &[u8], output: &mut [u8]) -> Result {
		todo!()
	}

	fn own_code_hash(output: &mut [u8]) {
		todo!()
	}

	fn account_reentrance_count(account: &[u8]) -> u32 {
		todo!()
	}

	fn lock_delegate_dependency(code_hash: &[u8]) {
		todo!()
	}

	fn unlock_delegate_dependency(code_hash: &[u8]) {
		todo!()
	}

	fn instantiation_nonce() -> u64 {
		todo!()
	}

	fn reentrance_count() -> u32 {
		todo!()
	}

	fn xcm_execute(msg: &[u8]) -> Result {
		todo!()
	}

	fn xcm_send(dest: &[u8], msg: &[u8], output: &mut [u8; 32]) -> Result {
		todo!()
	}
}
