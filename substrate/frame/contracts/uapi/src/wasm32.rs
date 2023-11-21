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

use super::{extract_from_slice, Result, ReturnCode};
use crate::ReturnFlags;

mod sys {
	use super::ReturnCode;

	#[link(wasm_import_module = "seal0")]
	extern "C" {
		pub fn transfer(
			account_id_ptr: *const u8,
			account_id_len: u32,
			transferred_value_ptr: *const u8,
			transferred_value_len: u32,
		) -> ReturnCode;

		pub fn deposit_event(
			topics_ptr: *const u8,
			topics_len: u32,
			data_ptr: *const u8,
			data_len: u32,
		);

		pub fn call_chain_extension(
			func_id: u32,
			input_ptr: *const u8,
			input_len: u32,
			output_ptr: *mut u8,
			output_len_ptr: *mut u32,
		) -> ReturnCode;

		pub fn input(buf_ptr: *mut u8, buf_len_ptr: *mut u32);
		pub fn seal_return(flags: u32, data_ptr: *const u8, data_len: u32) -> !;

		pub fn caller(output_ptr: *mut u8, output_len_ptr: *mut u32);
		pub fn block_number(output_ptr: *mut u8, output_len_ptr: *mut u32);
		pub fn address(output_ptr: *mut u8, output_len_ptr: *mut u32);
		pub fn balance(output_ptr: *mut u8, output_len_ptr: *mut u32);
		pub fn weight_to_fee(gas: u64, output_ptr: *mut u8, output_len_ptr: *mut u32);
		pub fn gas_left(output_ptr: *mut u8, output_len_ptr: *mut u32);
		pub fn value_transferred(output_ptr: *mut u8, output_len_ptr: *mut u32);
		pub fn now(output_ptr: *mut u8, output_len_ptr: *mut u32);
		pub fn minimum_balance(output_ptr: *mut u8, output_len_ptr: *mut u32);

		pub fn hash_keccak_256(input_ptr: *const u8, input_len: u32, output_ptr: *mut u8);
		pub fn hash_blake2_256(input_ptr: *const u8, input_len: u32, output_ptr: *mut u8);
		pub fn hash_blake2_128(input_ptr: *const u8, input_len: u32, output_ptr: *mut u8);
		pub fn hash_sha2_256(input_ptr: *const u8, input_len: u32, output_ptr: *mut u8);

		pub fn is_contract(account_id_ptr: *const u8) -> ReturnCode;

		pub fn caller_is_origin() -> ReturnCode;

		pub fn set_code_hash(code_hash_ptr: *const u8) -> ReturnCode;

		pub fn code_hash(
			account_id_ptr: *const u8,
			output_ptr: *mut u8,
			output_len_ptr: *mut u32,
		) -> ReturnCode;

		pub fn own_code_hash(output_ptr: *mut u8, output_len_ptr: *mut u32);

		pub fn delegate_call(
			flags: u32,
			code_hash_ptr: *const u8,
			input_data_ptr: *const u8,
			input_data_len: u32,
			output_ptr: *mut u8,
			output_len_ptr: *mut u32,
		) -> ReturnCode;

		pub fn ecdsa_recover(
			// 65 bytes of ecdsa signature
			signature_ptr: *const u8,
			// 32 bytes hash of the message
			message_hash_ptr: *const u8,
			output_ptr: *mut u8,
		) -> ReturnCode;

		pub fn ecdsa_to_eth_address(public_key_ptr: *const u8, output_ptr: *mut u8) -> ReturnCode;

		/// **WARNING**: this function is from the [unstable interface](https://github.com/paritytech/substrate/tree/master/frame/contracts#unstable-interfaces),
		/// which is unsafe and normally is not available on production chains.
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

		pub fn call_runtime(call_ptr: *const u8, call_len: u32) -> ReturnCode;
	}

	#[link(wasm_import_module = "seal1")]
	extern "C" {
		pub fn instantiate(
			init_code_ptr: *const u8,
			gas: u64,
			endowment_ptr: *const u8,
			input_ptr: *const u8,
			input_len: u32,
			address_ptr: *mut u8,
			address_len_ptr: *mut u32,
			output_ptr: *mut u8,
			output_len_ptr: *mut u32,
			salt_ptr: *const u8,
			salt_len: u32,
		) -> ReturnCode;

		pub fn terminate(beneficiary_ptr: *const u8) -> !;

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

		// # Parameters
		//
		// - `key_ptr`: pointer into the linear memory where the key is placed.
		// - `key_len`: the length of the key in bytes.
		//
		// # Return Value
		//
		// Returns the size of the pre-existing value at the specified key if any.
		// Otherwise `SENTINEL` is returned as a sentinel value.
		pub fn clear_storage(key_ptr: *const u8, key_len: u32) -> ReturnCode;

		// # Parameters
		//
		// - `key_ptr`: pointer into the linear memory where the key of the requested value is
		//   placed.
		// - `key_len`: the length of the key in bytes.
		//
		// # Return Value
		//
		// Returns the size of the pre-existing value at the specified key if any.
		// Otherwise `SENTINEL` is returned as a sentinel value.
		pub fn contains_storage(key_ptr: *const u8, key_len: u32) -> ReturnCode;

		// # Parameters
		//
		// - `key_ptr`: pointer into the linear memory where the key of the requested value is
		//   placed.
		// - `key_len`: the length of the key in bytes.
		// - `out_ptr`: pointer to the linear memory where the value is written to.
		// - `out_len_ptr`: in-out pointer into linear memory where the buffer length is read from
		//   and the value length is written to.
		//
		// # Errors
		//
		// `ReturnCode::KeyNotFound`
		pub fn get_storage(
			key_ptr: *const u8,
			key_len: u32,
			out_ptr: *mut u8,
			out_len_ptr: *mut u32,
		) -> ReturnCode;
	}

	#[link(wasm_import_module = "seal2")]
	extern "C" {
		// # Parameters
		//
		// - `key_ptr`: pointer into the linear memory where the location to store the value is
		//   placed.
		// - `key_len`: the length of the key in bytes.
		// - `value_ptr`: pointer into the linear memory where the value to set is placed.
		// - `value_len`: the length of the value in bytes.
		//
		// # Return Value
		//
		// Returns the size of the pre-existing value at the specified key if any.
		// Otherwise `SENTINEL` is returned as a sentinel value.
		pub fn set_storage(
			key_ptr: *const u8,
			key_len: u32,
			value_ptr: *const u8,
			value_len: u32,
		) -> ReturnCode;
	}
}

macro_rules! impl_wrapper_for {
	( $( $name:ident, )* ) => {
		$(
			#[inline(always)]
			fn $name(output: &mut &mut [u8]) {
				let mut output_len = output.len() as u32;
				{
					unsafe {
						sys::$name(
							output.as_mut_ptr(),
							&mut output_len,
						)
					};
				}
			}
		)*
	}
}

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

pub enum ApiImpl {}

impl super::Api for ApiImpl {
	#[inline(always)]
	fn instantiate(
		code_hash: &[u8],
		gas_limit: u64,
		endowment: &[u8],
		input: &[u8],
		out_address: &mut &mut [u8],
		out_return_value: &mut &mut [u8],
		salt: &[u8],
	) -> Result {
		let mut address_len = out_address.len() as u32;
		let mut return_value_len = out_return_value.len() as u32;
		let ret_code = {
			unsafe {
				sys::instantiate(
					code_hash.as_ptr(),
					gas_limit,
					endowment.as_ptr(),
					input.as_ptr(),
					input.len() as u32,
					out_address.as_mut_ptr(),
					&mut address_len,
					out_return_value.as_mut_ptr(),
					&mut return_value_len,
					salt.as_ptr(),
					salt.len() as u32,
				)
			}
		};
		extract_from_slice(out_address, address_len as usize);
		extract_from_slice(out_return_value, return_value_len as usize);
		ret_code.into()
	}

	#[inline(always)]
	fn call(
		flags: u32,
		callee: &[u8],
		gas_limit: u64,
		value: &[u8],
		input: &[u8],
		output: &mut &mut [u8],
	) -> Result {
		let mut output_len = output.len() as u32;
		let ret_code = {
			unsafe {
				sys::call(
					flags,
					callee.as_ptr(),
					gas_limit,
					value.as_ptr(),
					input.as_ptr(),
					input.len() as u32,
					output.as_mut_ptr(),
					&mut output_len,
				)
			}
		};
		extract_from_slice(output, output_len as usize);
		ret_code.into()
	}

	#[inline(always)]
	fn delegate_call(flags: u32, code_hash: &[u8], input: &[u8], output: &mut &mut [u8]) -> Result {
		let mut output_len = output.len() as u32;
		let ret_code = {
			unsafe {
				sys::delegate_call(
					flags,
					code_hash.as_ptr(),
					input.as_ptr(),
					input.len() as u32,
					output.as_mut_ptr(),
					&mut output_len,
				)
			}
		};
		extract_from_slice(output, output_len as usize);
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

	fn set_storage(key: &[u8], encoded_value: &[u8]) -> Option<u32> {
		let ret_code = unsafe {
			sys::set_storage(
				key.as_ptr(),
				key.len() as u32,
				encoded_value.as_ptr(),
				encoded_value.len() as u32,
			)
		};
		ret_code.into()
	}

	fn clear_storage(key: &[u8]) -> Option<u32> {
		let ret_code = unsafe { sys::clear_storage(key.as_ptr(), key.len() as u32) };
		ret_code.into()
	}

	#[inline(always)]
	fn get_storage(key: &[u8], output: &mut &mut [u8]) -> Result {
		let mut output_len = output.len() as u32;
		let ret_code = {
			unsafe {
				sys::get_storage(
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

	fn storage_contains(key: &[u8]) -> Option<u32> {
		let ret_code = unsafe { sys::contains_storage(key.as_ptr(), key.len() as u32) };
		ret_code.into()
	}

	fn terminate(beneficiary: &[u8]) -> ! {
		unsafe { sys::terminate(beneficiary.as_ptr()) }
	}

	#[inline(always)]
	fn call_chain_extension(func_id: u32, input: &[u8], output: &mut &mut [u8]) -> u32 {
		let mut output_len = output.len() as u32;
		let ret_code = {
			unsafe {
				sys::call_chain_extension(
					func_id,
					input.as_ptr(),
					input.len() as u32,
					output.as_mut_ptr(),
					&mut output_len,
				)
			}
		};
		extract_from_slice(output, output_len as usize);
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
		unsafe {
			sys::seal_return(flags.into_u32(), return_value.as_ptr(), return_value.len() as u32)
		}
	}

	fn call_runtime(call: &[u8]) -> Result {
		let ret_code = unsafe { sys::call_runtime(call.as_ptr(), call.len() as u32) };
		ret_code.into()
	}

	impl_wrapper_for! {
		caller,
		block_number,
		address,
		balance,
		gas_left,
		value_transferred,
		now,
		minimum_balance,
	}

	#[inline(always)]
	fn weight_to_fee(gas: u64, output: &mut &mut [u8]) {
		let mut output_len = output.len() as u32;
		{
			unsafe { sys::weight_to_fee(gas, output.as_mut_ptr(), &mut output_len) };
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
}
