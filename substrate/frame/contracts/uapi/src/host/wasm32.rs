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
use super::{
	extract_from_slice, ptr_len_or_sentinel, ptr_or_sentinel, CallFlags, HostFn, HostFnImpl, Result,
};
use crate::{ReturnCode, ReturnFlags};

mod sys {
	use super::ReturnCode;

	#[link(wasm_import_module = "seal0")]
	extern "C" {
		pub fn account_reentrance_count(account_ptr: *const u8) -> u32;

		pub fn lock_delegate_dependency(code_hash_ptr: *const u8);

		pub fn address(output_ptr: *mut u8, output_len_ptr: *mut u32);

		pub fn balance(output_ptr: *mut u8, output_len_ptr: *mut u32);

		pub fn block_number(output_ptr: *mut u8, output_len_ptr: *mut u32);

		pub fn call(
			callee_ptr: *const u8,
			callee_len: u32,
			gas: u64,
			value_ptr: *const u8,
			value_len: u32,
			input_data_ptr: *const u8,
			input_data_len: u32,
			output_ptr: *mut u8,
			output_len_ptr: *mut u32,
		) -> ReturnCode;

		pub fn call_chain_extension(
			func_id: u32,
			input_ptr: *const u8,
			input_len: u32,
			output_ptr: *mut u8,
			output_len_ptr: *mut u32,
		) -> ReturnCode;

		pub fn call_runtime(call_ptr: *const u8, call_len: u32) -> ReturnCode;

		pub fn caller(output_ptr: *mut u8, output_len_ptr: *mut u32);

		pub fn caller_is_origin() -> ReturnCode;

		pub fn caller_is_root() -> ReturnCode;

		pub fn clear_storage(key_ptr: *const u8, key_len: u32) -> ReturnCode;

		pub fn code_hash(
			account_id_ptr: *const u8,
			output_ptr: *mut u8,
			output_len_ptr: *mut u32,
		) -> ReturnCode;

		pub fn contains_storage(key_ptr: *const u8, key_len: u32) -> ReturnCode;

		pub fn debug_message(str_ptr: *const u8, str_len: u32) -> ReturnCode;

		pub fn delegate_call(
			flags: u32,
			code_hash_ptr: *const u8,
			input_data_ptr: *const u8,
			input_data_len: u32,
			output_ptr: *mut u8,
			output_len_ptr: *mut u32,
		) -> ReturnCode;

		pub fn deposit_event(
			topics_ptr: *const u8,
			topics_len: u32,
			data_ptr: *const u8,
			data_len: u32,
		);

		pub fn ecdsa_recover(
			signature_ptr: *const u8,
			message_hash_ptr: *const u8,
			output_ptr: *mut u8,
		) -> ReturnCode;

		pub fn ecdsa_to_eth_address(public_key_ptr: *const u8, output_ptr: *mut u8) -> ReturnCode;

		pub fn gas_left(output_ptr: *mut u8, output_len_ptr: *mut u32);

		pub fn get_storage(
			key_ptr: *const u8,
			out_ptr: *mut u8,
			out_len_ptr: *mut u32,
		) -> ReturnCode;

		pub fn hash_blake2_128(input_ptr: *const u8, input_len: u32, output_ptr: *mut u8);

		pub fn hash_blake2_256(input_ptr: *const u8, input_len: u32, output_ptr: *mut u8);

		pub fn hash_keccak_256(input_ptr: *const u8, input_len: u32, output_ptr: *mut u8);

		pub fn hash_sha2_256(input_ptr: *const u8, input_len: u32, output_ptr: *mut u8);

		pub fn input(buf_ptr: *mut u8, buf_len_ptr: *mut u32);

		pub fn instantiation_nonce() -> u64;

		pub fn is_contract(account_id_ptr: *const u8) -> ReturnCode;

		pub fn minimum_balance(output_ptr: *mut u8, output_len_ptr: *mut u32);

		pub fn now(output_ptr: *mut u8, output_len_ptr: *mut u32);

		pub fn own_code_hash(output_ptr: *mut u8, output_len_ptr: *mut u32);

		pub fn reentrance_count() -> u32;

		pub fn unlock_delegate_dependency(code_hash_ptr: *const u8);

		pub fn seal_return(flags: u32, data_ptr: *const u8, data_len: u32) -> !;

		pub fn set_code_hash(code_hash_ptr: *const u8) -> ReturnCode;

		pub fn set_storage(key_ptr: *const u8, value_ptr: *const u8, value_len: u32);

		pub fn sr25519_verify(
			signature_ptr: *const u8,
			public_key_ptr: *const u8,
			message_len: u32,
			message_ptr: *const u8,
		) -> ReturnCode;

		pub fn take_storage(
			key_ptr: *const u8,
			key_len: u32,
			out_ptr: *mut u8,
			out_len_ptr: *mut u32,
		) -> ReturnCode;

		pub fn terminate(beneficiary_ptr: *const u8) -> !;

		pub fn transfer(
			account_id_ptr: *const u8,
			account_id_len: u32,
			transferred_value_ptr: *const u8,
			transferred_value_len: u32,
		) -> ReturnCode;

		pub fn value_transferred(output_ptr: *mut u8, output_len_ptr: *mut u32);

		pub fn weight_to_fee(gas: u64, output_ptr: *mut u8, output_len_ptr: *mut u32);

		pub fn xcm_execute(msg_ptr: *const u8, msg_len: u32) -> ReturnCode;

		pub fn xcm_send(
			dest_ptr: *const u8,
			msg_ptr: *const u8,
			msg_len: u32,
			output_ptr: *mut u8,
		) -> ReturnCode;
	}

	pub mod v1 {
		use crate::ReturnCode;

		#[link(wasm_import_module = "seal1")]
		extern "C" {
			pub fn call(
				flags: u32,
				callee_ptr: *const u8,
				gas: u64,
				transferred_value_ptr: *const u8,
				input_data_ptr: *const u8,
				input_data_len: u32,
				output_ptr: *mut u8,
				output_len_ptr: *mut u32,
			) -> ReturnCode;

			pub fn clear_storage(key_ptr: *const u8, key_len: u32) -> ReturnCode;

			pub fn contains_storage(key_ptr: *const u8, key_len: u32) -> ReturnCode;

			pub fn gas_left(output_ptr: *mut u8, output_len_ptr: *mut u32);

			pub fn get_storage(
				key_ptr: *const u8,
				key_len: u32,
				out_ptr: *mut u8,
				out_len_ptr: *mut u32,
			) -> ReturnCode;

			pub fn instantiate(
				code_hash_ptr: *const u8,
				gas: u64,
				value_ptr: *const u8,
				input_ptr: *const u8,
				input_len: u32,
				address_ptr: *mut u8,
				address_len_ptr: *mut u32,
				output_ptr: *mut u8,
				output_len_ptr: *mut u32,
				salt_ptr: *const u8,
				salt_len: u32,
			) -> ReturnCode;

			pub fn set_storage(
				key_ptr: *const u8,
				value_ptr: *const u8,
				value_len: u32,
			) -> ReturnCode;

			pub fn terminate(beneficiary_ptr: *const u8) -> !;

			pub fn weight_to_fee(
				ref_time_limit: u64,
				proof_size_limit: u64,
				output_ptr: *mut u8,
				output_len_ptr: *mut u32,
			);
		}
	}

	pub mod v2 {
		use crate::ReturnCode;

		#[link(wasm_import_module = "seal2")]
		extern "C" {
			pub fn call(
				flags: u32,
				callee_ptr: *const u8,
				ref_time_limit: u64,
				proof_size_limit: u64,
				deposit_ptr: *const u8,
				transferred_value_ptr: *const u8,
				input_data_ptr: *const u8,
				input_data_len: u32,
				output_ptr: *mut u8,
				output_len_ptr: *mut u32,
			) -> ReturnCode;

			pub fn instantiate(
				code_hash_ptr: *const u8,
				ref_time_limit: u64,
				proof_size_limit: u64,
				deposit_ptr: *const u8,
				value_ptr: *const u8,
				input_ptr: *const u8,
				input_len: u32,
				address_ptr: *mut u8,
				address_len_ptr: *mut u32,
				output_ptr: *mut u8,
				output_len_ptr: *mut u32,
				salt_ptr: *const u8,
				salt_len: u32,
			) -> ReturnCode;

			pub fn set_storage(
				key_ptr: *const u8,
				key_len: u32,
				value_ptr: *const u8,
				value_len: u32,
			) -> ReturnCode;
		}
	}
}

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
				let mut output_len = output.len() as u32;
				unsafe {
					$( $mod )::*::$name(output.as_mut_ptr(), &mut output_len);
				}
				extract_from_slice(output, output_len as usize)
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
				unsafe {
					sys::[<hash_ $name>](
						input.as_ptr(),
						input.len() as u32,
						output.as_mut_ptr(),
					)
				}
			}
		}
	};
}

impl HostFn for HostFnImpl {
	#[inline(always)]
	fn instantiate_v1(
		code_hash: &[u8],
		gas: u64,
		value: &[u8],
		input: &[u8],
		mut address: Option<&mut &mut [u8]>,
		mut output: Option<&mut &mut [u8]>,
		salt: &[u8],
	) -> Result {
		let (address_ptr, mut address_len) = ptr_len_or_sentinel(&mut address);
		let (output_ptr, mut output_len) = ptr_len_or_sentinel(&mut output);
		let ret_code = unsafe {
			sys::v1::instantiate(
				code_hash.as_ptr(),
				gas,
				value.as_ptr(),
				input.as_ptr(),
				input.len() as u32,
				address_ptr,
				&mut address_len,
				output_ptr,
				&mut output_len,
				salt.as_ptr(),
				salt.len() as u32,
			)
		};

		if let Some(ref mut address) = address {
			extract_from_slice(address, address_len as usize);
		}
		if let Some(ref mut output) = output {
			extract_from_slice(output, output_len as usize);
		}
		ret_code.into()
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
		let (address_ptr, mut address_len) = ptr_len_or_sentinel(&mut address);
		let (output_ptr, mut output_len) = ptr_len_or_sentinel(&mut output);
		let deposit_ptr = ptr_or_sentinel(&deposit);

		let ret_code = {
			unsafe {
				sys::v2::instantiate(
					code_hash.as_ptr(),
					ref_time_limit,
					proof_size_limit,
					deposit_ptr,
					value.as_ptr(),
					input.as_ptr(),
					input.len() as u32,
					address_ptr,
					&mut address_len,
					output_ptr,
					&mut output_len,
					salt.as_ptr(),
					salt.len() as u32,
				)
			}
		};

		if let Some(ref mut address) = address {
			extract_from_slice(address, address_len as usize);
		}

		if let Some(ref mut output) = output {
			extract_from_slice(output, output_len as usize);
		}

		ret_code.into()
	}

	#[inline(always)]
	fn call(
		callee: &[u8],
		gas: u64,
		value: &[u8],
		input_data: &[u8],
		mut output: Option<&mut &mut [u8]>,
	) -> Result {
		let (output_ptr, mut output_len) = ptr_len_or_sentinel(&mut output);
		let ret_code = {
			unsafe {
				sys::call(
					callee.as_ptr(),
					callee.len() as u32,
					gas,
					value.as_ptr(),
					value.len() as u32,
					input_data.as_ptr(),
					input_data.len() as u32,
					output_ptr,
					&mut output_len,
				)
			}
		};

		if let Some(ref mut output) = output {
			extract_from_slice(output, output_len as usize);
		}

		ret_code.into()
	}

	#[inline(always)]
	fn call_v1(
		flags: CallFlags,
		callee: &[u8],
		gas: u64,
		value: &[u8],
		input_data: &[u8],
		mut output: Option<&mut &mut [u8]>,
	) -> Result {
		let (output_ptr, mut output_len) = ptr_len_or_sentinel(&mut output);
		let ret_code = {
			unsafe {
				sys::v1::call(
					flags.bits(),
					callee.as_ptr(),
					gas,
					value.as_ptr(),
					input_data.as_ptr(),
					input_data.len() as u32,
					output_ptr,
					&mut output_len,
				)
			}
		};

		if let Some(ref mut output) = output {
			extract_from_slice(output, output_len as usize);
		}

		ret_code.into()
	}

	fn call_v2(
		flags: CallFlags,
		callee: &[u8],
		ref_time_limit: u64,
		proof_size_limit: u64,
		deposit: Option<&[u8]>,
		value: &[u8],
		input_data: &[u8],
		mut output: Option<&mut &mut [u8]>,
	) -> Result {
		let (output_ptr, mut output_len) = ptr_len_or_sentinel(&mut output);
		let deposit_ptr = ptr_or_sentinel(&deposit);
		let ret_code = {
			unsafe {
				sys::v2::call(
					flags.bits(),
					callee.as_ptr(),
					ref_time_limit,
					proof_size_limit,
					deposit_ptr,
					value.as_ptr(),
					input_data.as_ptr(),
					input_data.len() as u32,
					output_ptr,
					&mut output_len,
				)
			}
		};

		if let Some(ref mut output) = output {
			extract_from_slice(output, output_len as usize);
		}

		ret_code.into()
	}

	fn caller_is_root() -> u32 {
		unsafe { sys::caller_is_root() }.into_u32()
	}

	#[inline(always)]
	fn delegate_call(
		flags: CallFlags,
		code_hash: &[u8],
		input: &[u8],
		mut output: Option<&mut &mut [u8]>,
	) -> Result {
		let (output_ptr, mut output_len) = ptr_len_or_sentinel(&mut output);
		let ret_code = {
			unsafe {
				sys::delegate_call(
					flags.bits(),
					code_hash.as_ptr(),
					input.as_ptr(),
					input.len() as u32,
					output_ptr,
					&mut output_len,
				)
			}
		};

		if let Some(ref mut output) = output {
			extract_from_slice(output, output_len as usize);
		}

		ret_code.into()
	}

	fn transfer(account_id: &[u8], value: &[u8]) -> Result {
		let ret_code = unsafe {
			sys::transfer(
				account_id.as_ptr(),
				account_id.len() as u32,
				value.as_ptr(),
				value.len() as u32,
			)
		};
		ret_code.into()
	}

	fn deposit_event(topics: &[u8], data: &[u8]) {
		unsafe {
			sys::deposit_event(
				topics.as_ptr(),
				topics.len() as u32,
				data.as_ptr(),
				data.len() as u32,
			)
		}
	}

	fn set_storage(key: &[u8], value: &[u8]) {
		unsafe { sys::set_storage(key.as_ptr(), value.as_ptr(), value.len() as u32) };
	}

	fn set_storage_v1(key: &[u8], encoded_value: &[u8]) -> Option<u32> {
		let ret_code = unsafe {
			sys::v1::set_storage(key.as_ptr(), encoded_value.as_ptr(), encoded_value.len() as u32)
		};
		ret_code.into()
	}

	fn set_storage_v2(key: &[u8], encoded_value: &[u8]) -> Option<u32> {
		let ret_code = unsafe {
			sys::v2::set_storage(
				key.as_ptr(),
				key.len() as u32,
				encoded_value.as_ptr(),
				encoded_value.len() as u32,
			)
		};
		ret_code.into()
	}

	fn clear_storage(key: &[u8]) {
		unsafe { sys::clear_storage(key.as_ptr(), key.len() as u32) };
	}

	fn clear_storage_v1(key: &[u8]) -> Option<u32> {
		let ret_code = unsafe { sys::v1::clear_storage(key.as_ptr(), key.len() as u32) };
		ret_code.into()
	}

	#[inline(always)]
	fn get_storage(key: &[u8], output: &mut &mut [u8]) -> Result {
		let mut output_len = output.len() as u32;
		let ret_code =
			{ unsafe { sys::get_storage(key.as_ptr(), output.as_mut_ptr(), &mut output_len) } };
		extract_from_slice(output, output_len as usize);
		ret_code.into()
	}

	#[inline(always)]
	fn get_storage_v1(key: &[u8], output: &mut &mut [u8]) -> Result {
		let mut output_len = output.len() as u32;
		let ret_code = {
			unsafe {
				sys::v1::get_storage(
					key.as_ptr(),
					key.len() as u32,
					output.as_mut_ptr(),
					&mut output_len,
				)
			}
		};
		extract_from_slice(output, output_len as usize);
		ret_code.into()
	}

	#[inline(always)]
	fn take_storage(key: &[u8], output: &mut &mut [u8]) -> Result {
		let mut output_len = output.len() as u32;
		let ret_code = {
			unsafe {
				sys::take_storage(
					key.as_ptr(),
					key.len() as u32,
					output.as_mut_ptr(),
					&mut output_len,
				)
			}
		};
		extract_from_slice(output, output_len as usize);
		ret_code.into()
	}

	fn debug_message(str: &[u8]) -> Result {
		let ret_code = unsafe { sys::debug_message(str.as_ptr(), str.len() as u32) };
		ret_code.into()
	}

	fn contains_storage(key: &[u8]) -> Option<u32> {
		let ret_code = unsafe { sys::contains_storage(key.as_ptr(), key.len() as u32) };
		ret_code.into()
	}

	fn contains_storage_v1(key: &[u8]) -> Option<u32> {
		let ret_code = unsafe { sys::v1::contains_storage(key.as_ptr(), key.len() as u32) };
		ret_code.into()
	}

	fn terminate(beneficiary: &[u8]) -> ! {
		unsafe { sys::terminate(beneficiary.as_ptr()) }
	}

	fn terminate_v1(beneficiary: &[u8]) -> ! {
		unsafe { sys::v1::terminate(beneficiary.as_ptr()) }
	}

	fn call_chain_extension(func_id: u32, input: &[u8], mut output: Option<&mut &mut [u8]>) -> u32 {
		let (output_ptr, mut output_len) = ptr_len_or_sentinel(&mut output);
		let ret_code = {
			unsafe {
				sys::call_chain_extension(
					func_id,
					input.as_ptr(),
					input.len() as u32,
					output_ptr,
					&mut output_len,
				)
			}
		};

		if let Some(ref mut output) = output {
			extract_from_slice(output, output_len as usize);
		}
		ret_code.into_u32()
	}

	#[inline(always)]
	fn input(output: &mut &mut [u8]) {
		let mut output_len = output.len() as u32;
		{
			unsafe { sys::input(output.as_mut_ptr(), &mut output_len) };
		}
		extract_from_slice(output, output_len as usize);
	}

	fn return_value(flags: ReturnFlags, return_value: &[u8]) -> ! {
		unsafe { sys::seal_return(flags.bits(), return_value.as_ptr(), return_value.len() as u32) }
	}

	fn call_runtime(call: &[u8]) -> Result {
		let ret_code = unsafe { sys::call_runtime(call.as_ptr(), call.len() as u32) };
		ret_code.into()
	}

	impl_wrapper_for! {
		() => [caller, block_number, address, balance, gas_left, value_transferred, now, minimum_balance],
		(v1) => [gas_left],
	}

	#[inline(always)]
	fn weight_to_fee(gas: u64, output: &mut &mut [u8]) {
		let mut output_len = output.len() as u32;
		{
			unsafe { sys::weight_to_fee(gas, output.as_mut_ptr(), &mut output_len) };
		}
		extract_from_slice(output, output_len as usize);
	}

	fn weight_to_fee_v1(ref_time_limit: u64, proof_size_limit: u64, output: &mut &mut [u8]) {
		let mut output_len = output.len() as u32;
		{
			unsafe {
				sys::v1::weight_to_fee(
					ref_time_limit,
					proof_size_limit,
					output.as_mut_ptr(),
					&mut output_len,
				)
			};
		}
		extract_from_slice(output, output_len as usize);
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
		let ret_code = unsafe {
			sys::ecdsa_recover(signature.as_ptr(), message_hash.as_ptr(), output.as_mut_ptr())
		};
		ret_code.into()
	}

	fn ecdsa_to_eth_address(pubkey: &[u8; 33], output: &mut [u8; 20]) -> Result {
		let ret_code = unsafe { sys::ecdsa_to_eth_address(pubkey.as_ptr(), output.as_mut_ptr()) };
		ret_code.into()
	}

	fn sr25519_verify(signature: &[u8; 64], message: &[u8], pub_key: &[u8; 32]) -> Result {
		let ret_code = unsafe {
			sys::sr25519_verify(
				signature.as_ptr(),
				pub_key.as_ptr(),
				message.len() as u32,
				message.as_ptr(),
			)
		};
		ret_code.into()
	}

	fn is_contract(account_id: &[u8]) -> bool {
		let ret_val = unsafe { sys::is_contract(account_id.as_ptr()) };
		ret_val.into_bool()
	}

	fn caller_is_origin() -> bool {
		let ret_val = unsafe { sys::caller_is_origin() };
		ret_val.into_bool()
	}

	fn set_code_hash(code_hash: &[u8]) -> Result {
		let ret_val = unsafe { sys::set_code_hash(code_hash.as_ptr()) };
		ret_val.into()
	}

	fn code_hash(account_id: &[u8], output: &mut [u8]) -> Result {
		let mut output_len = output.len() as u32;
		let ret_val =
			unsafe { sys::code_hash(account_id.as_ptr(), output.as_mut_ptr(), &mut output_len) };
		ret_val.into()
	}

	fn own_code_hash(output: &mut [u8]) {
		let mut output_len = output.len() as u32;
		unsafe { sys::own_code_hash(output.as_mut_ptr(), &mut output_len) }
	}

	fn account_reentrance_count(account: &[u8]) -> u32 {
		unsafe { sys::account_reentrance_count(account.as_ptr()) }
	}

	fn lock_delegate_dependency(code_hash: &[u8]) {
		unsafe { sys::lock_delegate_dependency(code_hash.as_ptr()) }
	}

	fn unlock_delegate_dependency(code_hash: &[u8]) {
		unsafe { sys::unlock_delegate_dependency(code_hash.as_ptr()) }
	}

	fn instantiation_nonce() -> u64 {
		unsafe { sys::instantiation_nonce() }
	}

	fn reentrance_count() -> u32 {
		unsafe { sys::reentrance_count() }
	}

	fn xcm_execute(msg: &[u8]) -> Result {
		let ret_code = unsafe { sys::xcm_execute(msg.as_ptr(), msg.len() as _) };
		ret_code.into()
	}

	fn xcm_send(dest: &[u8], msg: &[u8], output: &mut [u8; 32]) -> Result {
		let ret_code = unsafe {
			sys::xcm_send(dest.as_ptr(), msg.as_ptr(), msg.len() as _, output.as_mut_ptr())
		};
		ret_code.into()
	}
}
