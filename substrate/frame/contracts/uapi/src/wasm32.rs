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

		#[cfg(feature = "ink-debug")]
		pub fn debug_message(str_ptr: *const u8, str_len: u32) -> ReturnCode;

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

#[inline(always)]
pub fn instantiate(
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
pub fn call(
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
pub fn delegate_call(flags: u32, code_hash: &[u8], input: &[u8], output: &mut &mut [u8]) -> Result {
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

pub fn transfer(account_id: &[u8], value: &[u8]) -> Result {
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

pub fn deposit_event(topics: &[u8], data: &[u8]) {
	unsafe {
		sys::deposit_event(topics.as_ptr(), topics.len() as u32, data.as_ptr(), data.len() as u32)
	}
}

pub fn set_storage(key: &[u8], encoded_value: &[u8]) -> Option<u32> {
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

pub fn clear_storage(key: &[u8]) -> Option<u32> {
	let ret_code = unsafe { sys::clear_storage(key.as_ptr(), key.len() as u32) };
	ret_code.into()
}

#[inline(always)]
pub fn get_storage(key: &[u8], output: &mut &mut [u8]) -> Result {
	let mut output_len = output.len() as u32;
	let ret_code = {
		unsafe {
			sys::get_storage(key.as_ptr(), key.len() as u32, output.as_mut_ptr(), &mut output_len)
		}
	};
	extract_from_slice(output, output_len as usize);
	ret_code.into()
}

#[inline(always)]
pub fn take_storage(key: &[u8], output: &mut &mut [u8]) -> Result {
	let mut output_len = output.len() as u32;
	let ret_code = {
		unsafe {
			sys::take_storage(key.as_ptr(), key.len() as u32, output.as_mut_ptr(), &mut output_len)
		}
	};
	extract_from_slice(output, output_len as usize);
	ret_code.into()
}

pub fn storage_contains(key: &[u8]) -> Option<u32> {
	let ret_code = unsafe { sys::contains_storage(key.as_ptr(), key.len() as u32) };
	ret_code.into()
}

pub fn terminate(beneficiary: &[u8]) -> ! {
	unsafe { sys::terminate(beneficiary.as_ptr()) }
}

#[inline(always)]
pub fn call_chain_extension(func_id: u32, input: &[u8], output: &mut &mut [u8]) -> u32 {
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
pub fn input(output: &mut &mut [u8]) {
	let mut output_len = output.len() as u32;
	{
		unsafe { sys::input(output.as_mut_ptr(), &mut output_len) };
	}
	extract_from_slice(output, output_len as usize);
}

pub fn return_value(flags: ReturnFlags, return_value: &[u8]) -> ! {
	unsafe { sys::seal_return(flags.into_u32(), return_value.as_ptr(), return_value.len() as u32) }
}

pub fn call_runtime(call: &[u8]) -> Result {
	let ret_code = unsafe { sys::call_runtime(call.as_ptr(), call.len() as u32) };
	ret_code.into()
}

macro_rules! impl_wrapper_for {
    ( $( $name:ident, )* ) => {
        $(
            #[inline(always)]
            pub fn $name(output: &mut &mut [u8]) {
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
pub fn weight_to_fee(gas: u64, output: &mut &mut [u8]) {
	let mut output_len = output.len() as u32;
	{
		unsafe { sys::weight_to_fee(gas, output.as_mut_ptr(), &mut output_len) };
	}
	extract_from_slice(output, output_len as usize);
}

#[cfg(feature = "ink-debug")]
/// Call `debug_message` with the supplied UTF-8 encoded message.
///
/// If debug message recording is disabled in the contracts pallet, the first call will
/// return a `LoggingDisabled` error, and further calls will be a no-op to avoid the cost
/// of calling into the supervisor.
///
/// # Note
///
/// This depends on the `debug_message` interface which requires the
/// `"pallet-contracts/unstable-interface"` feature to be enabled in the target runtime.
pub fn debug_message(message: &str) {
	static mut DEBUG_ENABLED: bool = false;
	static mut FIRST_RUN: bool = true;

	// SAFETY: safe because executing in a single threaded context
	// We need those two variables in order to make sure that the assignment is performed
	// in the "logging enabled" case. This is because during RPC execution logging might
	// be enabled while it is disabled during the actual execution as part of a
	// transaction. The gas estimation takes place during RPC execution. We want to
	// overestimate instead of underestimate gas usage. Otherwise using this estimate
	// could lead to a out of gas error.
	if unsafe { DEBUG_ENABLED || FIRST_RUN } {
		let bytes = message.as_bytes();
		let ret_code = unsafe { sys::debug_message(bytes.as_ptr(), bytes.len() as u32) };
		if !matches!(ret_code.into(), Err(super::Error::LoggingDisabled)) {
			// SAFETY: safe because executing in a single threaded context
			unsafe { DEBUG_ENABLED = true }
		}
		// SAFETY: safe because executing in a single threaded context
		unsafe { FIRST_RUN = false }
	}
}

#[cfg(not(feature = "ink-debug"))]
/// A no-op. Enable the `ink-debug` feature for debug messages.
pub fn debug_message(_message: &str) {}

macro_rules! impl_hash_fn {
	( $name:ident, $bytes_result:literal ) => {
		paste::item! {
			pub fn [<hash_ $name>](input: &[u8], output: &mut [u8; $bytes_result]) {
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
impl_hash_fn!(sha2_256, 32);
impl_hash_fn!(keccak_256, 32);
impl_hash_fn!(blake2_256, 32);
impl_hash_fn!(blake2_128, 16);

pub fn ecdsa_recover(
	signature: &[u8; 65],
	message_hash: &[u8; 32],
	output: &mut [u8; 33],
) -> Result {
	let ret_code = unsafe {
		sys::ecdsa_recover(signature.as_ptr(), message_hash.as_ptr(), output.as_mut_ptr())
	};
	ret_code.into()
}

pub fn ecdsa_to_eth_address(pubkey: &[u8; 33], output: &mut [u8; 20]) -> Result {
	let ret_code = unsafe { sys::ecdsa_to_eth_address(pubkey.as_ptr(), output.as_mut_ptr()) };
	ret_code.into()
}

pub fn sr25519_verify(signature: &[u8; 64], message: &[u8], pub_key: &[u8; 32]) -> Result {
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

pub fn is_contract(account_id: &[u8]) -> bool {
	let ret_val = unsafe { sys::is_contract(account_id.as_ptr()) };
	ret_val.into_bool()
}

pub fn caller_is_origin() -> bool {
	let ret_val = unsafe { sys::caller_is_origin() };
	ret_val.into_bool()
}

pub fn set_code_hash(code_hash: &[u8]) -> Result {
	let ret_val = unsafe { sys::set_code_hash(code_hash.as_ptr()) };
	ret_val.into()
}

pub fn code_hash(account_id: &[u8], output: &mut [u8]) -> Result {
	let mut output_len = output.len() as u32;
	let ret_val =
		unsafe { sys::code_hash(account_id.as_ptr(), output.as_mut_ptr(), &mut output_len) };
	ret_val.into()
}

pub fn own_code_hash(output: &mut [u8]) {
	let mut output_len = output.len() as u32;
	unsafe { sys::own_code_hash(output.as_mut_ptr(), &mut output_len) }
}
