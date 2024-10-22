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

#![allow(unused_variables)]

use crate::{
	host::{CallFlags, HostFn, HostFnImpl, Result, StorageFlags},
	ReturnFlags,
};

mod sys {
	use crate::ReturnCode;

	#[polkavm_derive::polkavm_define_abi]
	mod abi {}

	impl abi::FromHost for ReturnCode {
		type Regs = (u32,);

		fn from_host((a0,): Self::Regs) -> Self {
			ReturnCode(a0)
		}
	}

	#[polkavm_derive::polkavm_import(abi = self::abi)]
	extern "C" {
		pub fn set_storage(
			flags: u32,
			key_ptr: *const u8,
			key_len: u32,
			value_ptr: *const u8,
			value_len: u32,
		) -> ReturnCode;
		pub fn clear_storage(flags: u32, key_ptr: *const u8, key_len: u32) -> ReturnCode;
		pub fn get_storage(
			flags: u32,
			key_ptr: *const u8,
			key_len: u32,
			out_ptr: *mut u8,
			out_len_ptr: *mut u32,
		) -> ReturnCode;
		pub fn contains_storage(flags: u32, key_ptr: *const u8, key_len: u32) -> ReturnCode;
		pub fn take_storage(
			flags: u32,
			key_ptr: *const u8,
			key_len: u32,
			out_ptr: *mut u8,
			out_len_ptr: *mut u32,
		) -> ReturnCode;
		pub fn transfer(address_ptr: *const u8, value_ptr: *const u8) -> ReturnCode;
		pub fn call(ptr: *const u8) -> ReturnCode;
		pub fn delegate_call(
			flags: u32,
			code_hash_ptr: *const u8,
			input_data_ptr: *const u8,
			input_data_len: u32,
			out_ptr: *mut u8,
			out_len_ptr: *mut u32,
		) -> ReturnCode;
		pub fn instantiate(ptr: *const u8) -> ReturnCode;
		pub fn terminate(beneficiary_ptr: *const u8);
		pub fn input(out_ptr: *mut u8, out_len_ptr: *mut u32);
		pub fn seal_return(flags: u32, data_ptr: *const u8, data_len: u32);
		pub fn caller(out_ptr: *mut u8);
		pub fn is_contract(account_ptr: *const u8) -> ReturnCode;
		pub fn code_hash(address_ptr: *const u8, out_ptr: *mut u8);
		pub fn own_code_hash(out_ptr: *mut u8);
		pub fn caller_is_origin() -> ReturnCode;
		pub fn caller_is_root() -> ReturnCode;
		pub fn address(out_ptr: *mut u8);
		pub fn weight_to_fee(ref_time: u64, proof_size: u64, out_ptr: *mut u8);
		pub fn weight_left(out_ptr: *mut u8, out_len_ptr: *mut u32);
		pub fn get_immutable_data(out_ptr: *mut u8, out_len_ptr: *mut u32);
		pub fn set_immutable_data(ptr: *const u8, len: u32);
		pub fn balance(out_ptr: *mut u8);
		pub fn balance_of(addr_ptr: *const u8, out_ptr: *mut u8);
		pub fn chain_id(out_ptr: *mut u8);
		pub fn value_transferred(out_ptr: *mut u8);
		pub fn now(out_ptr: *mut u8);
		pub fn minimum_balance(out_ptr: *mut u8);
		pub fn deposit_event(
			topics_ptr: *const [u8; 32],
			num_topic: u32,
			data_ptr: *const u8,
			data_len: u32,
		);
		pub fn block_number(out_ptr: *mut u8);
		pub fn hash_sha2_256(input_ptr: *const u8, input_len: u32, out_ptr: *mut u8);
		pub fn hash_keccak_256(input_ptr: *const u8, input_len: u32, out_ptr: *mut u8);
		pub fn hash_blake2_256(input_ptr: *const u8, input_len: u32, out_ptr: *mut u8);
		pub fn hash_blake2_128(input_ptr: *const u8, input_len: u32, out_ptr: *mut u8);
		pub fn call_chain_extension(
			id: u32,
			input_ptr: *const u8,
			input_len: u32,
			out_ptr: *mut u8,
			out_len_ptr: *mut u32,
		) -> ReturnCode;
		pub fn debug_message(str_ptr: *const u8, str_len: u32) -> ReturnCode;
		pub fn call_runtime(call_ptr: *const u8, call_len: u32) -> ReturnCode;
		pub fn ecdsa_recover(
			signature_ptr: *const u8,
			message_hash_ptr: *const u8,
			out_ptr: *mut u8,
		) -> ReturnCode;
		pub fn sr25519_verify(
			signature_ptr: *const u8,
			pub_key_ptr: *const u8,
			message_len: u32,
			message_ptr: *const u8,
		) -> ReturnCode;
		pub fn set_code_hash(code_hash_ptr: *const u8) -> ReturnCode;
		pub fn ecdsa_to_eth_address(key_ptr: *const u8, out_ptr: *mut u8) -> ReturnCode;
		pub fn instantiation_nonce() -> u64;
		pub fn lock_delegate_dependency(code_hash_ptr: *const u8);
		pub fn unlock_delegate_dependency(code_hash_ptr: *const u8);
		pub fn xcm_execute(msg_ptr: *const u8, msg_len: u32) -> ReturnCode;
		pub fn xcm_send(
			dest_ptr: *const u8,
			dest_len: *const u8,
			msg_ptr: *const u8,
			msg_len: u32,
			out_ptr: *mut u8,
		) -> ReturnCode;
		pub fn return_data_size(out_ptr: *mut u8);
		pub fn return_data_copy(out_ptr: *mut u8, out_len_ptr: *mut u32, offset: u32);
	}
}

/// A macro to implement all Host functions with a signature of `fn(&mut [u8; n])`.
macro_rules! impl_wrapper_for {
	(@impl_fn $name:ident, $n: literal) => {
		fn $name(output: &mut [u8; $n]) {
			unsafe { sys::$name(output.as_mut_ptr()) }
		}
	};

	() => {};

	([u8; $n: literal] => $($name:ident),*; $($tail:tt)*) => {
		$(impl_wrapper_for!(@impl_fn $name, $n);)*
		impl_wrapper_for!($($tail)*);
	};
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

#[inline(always)]
fn extract_from_slice(output: &mut &mut [u8], new_len: usize) {
	debug_assert!(new_len <= output.len());
	let tmp = core::mem::take(output);
	*output = &mut tmp[..new_len];
}

#[inline(always)]
fn ptr_len_or_sentinel(data: &mut Option<&mut &mut [u8]>) -> (*mut u8, u32) {
	match data {
		Some(ref mut data) => (data.as_mut_ptr(), data.len() as _),
		None => (crate::SENTINEL as _, 0),
	}
}

#[inline(always)]
fn ptr_or_sentinel(data: &Option<&[u8; 32]>) -> *const u8 {
	match data {
		Some(ref data) => data.as_ptr(),
		None => crate::SENTINEL as _,
	}
}

impl HostFn for HostFnImpl {
	fn instantiate(
		code_hash: &[u8; 32],
		ref_time_limit: u64,
		proof_size_limit: u64,
		deposit_limit: Option<&[u8; 32]>,
		value: &[u8; 32],
		input: &[u8],
		mut address: Option<&mut [u8; 20]>,
		mut output: Option<&mut &mut [u8]>,
		salt: Option<&[u8; 32]>,
	) -> Result {
		let address = match address {
			Some(ref mut data) => data.as_mut_ptr(),
			None => crate::SENTINEL as _,
		};
		let (output_ptr, mut output_len) = ptr_len_or_sentinel(&mut output);
		let deposit_limit_ptr = ptr_or_sentinel(&deposit_limit);
		let salt_ptr = ptr_or_sentinel(&salt);
		#[repr(packed)]
		#[allow(dead_code)]
		struct Args {
			code_hash: *const u8,
			ref_time_limit: u64,
			proof_size_limit: u64,
			deposit_limit: *const u8,
			value: *const u8,
			input: *const u8,
			input_len: u32,
			address: *const u8,
			output: *mut u8,
			output_len: *mut u32,
			salt: *const u8,
		}
		let args = Args {
			code_hash: code_hash.as_ptr(),
			ref_time_limit,
			proof_size_limit,
			deposit_limit: deposit_limit_ptr,
			value: value.as_ptr(),
			input: input.as_ptr(),
			input_len: input.len() as _,
			address,
			output: output_ptr,
			output_len: &mut output_len as *mut _,
			salt: salt_ptr,
		};

		let ret_code = { unsafe { sys::instantiate(&args as *const Args as *const _) } };

		if let Some(ref mut output) = output {
			extract_from_slice(output, output_len as usize);
		}

		ret_code.into()
	}

	fn call(
		flags: CallFlags,
		callee: &[u8; 20],
		ref_time_limit: u64,
		proof_size_limit: u64,
		deposit_limit: Option<&[u8; 32]>,
		value: &[u8; 32],
		input: &[u8],
		mut output: Option<&mut &mut [u8]>,
	) -> Result {
		let (output_ptr, mut output_len) = ptr_len_or_sentinel(&mut output);
		let deposit_limit_ptr = ptr_or_sentinel(&deposit_limit);
		#[repr(packed)]
		#[allow(dead_code)]
		struct Args {
			flags: u32,
			callee: *const u8,
			ref_time_limit: u64,
			proof_size_limit: u64,
			deposit_limit: *const u8,
			value: *const u8,
			input: *const u8,
			input_len: u32,
			output: *mut u8,
			output_len: *mut u32,
		}
		let args = Args {
			flags: flags.bits(),
			callee: callee.as_ptr(),
			ref_time_limit,
			proof_size_limit,
			deposit_limit: deposit_limit_ptr,
			value: value.as_ptr(),
			input: input.as_ptr(),
			input_len: input.len() as _,
			output: output_ptr,
			output_len: &mut output_len as *mut _,
		};

		let ret_code = { unsafe { sys::call(&args as *const Args as *const _) } };

		if let Some(ref mut output) = output {
			extract_from_slice(output, output_len as usize);
		}

		ret_code.into()
	}

	fn caller_is_root() -> u32 {
		unsafe { sys::caller_is_root() }.into_u32()
	}

	fn delegate_call(
		flags: CallFlags,
		code_hash: &[u8; 32],
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

	fn transfer(address: &[u8; 20], value: &[u8; 32]) -> Result {
		let ret_code = unsafe { sys::transfer(address.as_ptr(), value.as_ptr()) };
		ret_code.into()
	}

	fn deposit_event(topics: &[[u8; 32]], data: &[u8]) {
		unsafe {
			sys::deposit_event(
				topics.as_ptr(),
				topics.len() as u32,
				data.as_ptr(),
				data.len() as u32,
			)
		}
	}

	fn set_storage(flags: StorageFlags, key: &[u8], encoded_value: &[u8]) -> Option<u32> {
		let ret_code = unsafe {
			sys::set_storage(
				flags.bits(),
				key.as_ptr(),
				key.len() as u32,
				encoded_value.as_ptr(),
				encoded_value.len() as u32,
			)
		};
		ret_code.into()
	}

	fn clear_storage(flags: StorageFlags, key: &[u8]) -> Option<u32> {
		let ret_code = unsafe { sys::clear_storage(flags.bits(), key.as_ptr(), key.len() as u32) };
		ret_code.into()
	}

	fn contains_storage(flags: StorageFlags, key: &[u8]) -> Option<u32> {
		let ret_code =
			unsafe { sys::contains_storage(flags.bits(), key.as_ptr(), key.len() as u32) };
		ret_code.into()
	}

	fn get_storage(flags: StorageFlags, key: &[u8], output: &mut &mut [u8]) -> Result {
		let mut output_len = output.len() as u32;
		let ret_code = {
			unsafe {
				sys::get_storage(
					flags.bits(),
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

	fn take_storage(flags: StorageFlags, key: &[u8], output: &mut &mut [u8]) -> Result {
		let mut output_len = output.len() as u32;
		let ret_code = {
			unsafe {
				sys::take_storage(
					flags.bits(),
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

	fn terminate(beneficiary: &[u8; 20]) -> ! {
		unsafe { sys::terminate(beneficiary.as_ptr()) }
		panic!("terminate does not return");
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

	fn input(output: &mut &mut [u8]) {
		let mut output_len = output.len() as u32;
		{
			unsafe { sys::input(output.as_mut_ptr(), &mut output_len) };
		}
		extract_from_slice(output, output_len as usize);
	}

	fn return_value(flags: ReturnFlags, return_value: &[u8]) -> ! {
		unsafe { sys::seal_return(flags.bits(), return_value.as_ptr(), return_value.len() as u32) }
		panic!("seal_return does not return");
	}

	fn call_runtime(call: &[u8]) -> Result {
		let ret_code = unsafe { sys::call_runtime(call.as_ptr(), call.len() as u32) };
		ret_code.into()
	}

	impl_wrapper_for! {
		[u8; 32] => block_number, balance, value_transferred, now, minimum_balance, chain_id;
		[u8; 20] => address, caller;
	}

	fn weight_left(output: &mut &mut [u8]) {
		let mut output_len = output.len() as u32;
		unsafe { sys::weight_left(output.as_mut_ptr(), &mut output_len) }
		extract_from_slice(output, output_len as usize)
	}

	fn weight_to_fee(ref_time_limit: u64, proof_size_limit: u64, output: &mut [u8; 32]) {
		unsafe { sys::weight_to_fee(ref_time_limit, proof_size_limit, output.as_mut_ptr()) };
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

	fn is_contract(address: &[u8; 20]) -> bool {
		let ret_val = unsafe { sys::is_contract(address.as_ptr()) };
		ret_val.into_bool()
	}

	fn get_immutable_data(output: &mut &mut [u8]) {
		let mut output_len = output.len() as u32;
		unsafe { sys::get_immutable_data(output.as_mut_ptr(), &mut output_len) };
		extract_from_slice(output, output_len as usize);
	}

	fn set_immutable_data(data: &[u8]) {
		unsafe { sys::set_immutable_data(data.as_ptr(), data.len() as u32) }
	}

	fn balance_of(address: &[u8; 20], output: &mut [u8; 32]) {
		unsafe { sys::balance_of(address.as_ptr(), output.as_mut_ptr()) };
	}

	fn caller_is_origin() -> bool {
		let ret_val = unsafe { sys::caller_is_origin() };
		ret_val.into_bool()
	}

	fn set_code_hash(code_hash: &[u8; 32]) -> Result {
		let ret_val = unsafe { sys::set_code_hash(code_hash.as_ptr()) };
		ret_val.into()
	}

	fn code_hash(address: &[u8; 20], output: &mut [u8; 32]) {
		unsafe { sys::code_hash(address.as_ptr(), output.as_mut_ptr()) }
	}

	fn own_code_hash(output: &mut [u8; 32]) {
		unsafe { sys::own_code_hash(output.as_mut_ptr()) }
	}

	fn lock_delegate_dependency(code_hash: &[u8; 32]) {
		unsafe { sys::lock_delegate_dependency(code_hash.as_ptr()) }
	}

	fn unlock_delegate_dependency(code_hash: &[u8; 32]) {
		unsafe { sys::unlock_delegate_dependency(code_hash.as_ptr()) }
	}

	fn xcm_execute(msg: &[u8]) -> Result {
		let ret_code = unsafe { sys::xcm_execute(msg.as_ptr(), msg.len() as _) };
		ret_code.into()
	}

	fn xcm_send(dest: &[u8], msg: &[u8], output: &mut [u8; 32]) -> Result {
		let ret_code = unsafe {
			sys::xcm_send(
				dest.as_ptr(),
				dest.len() as _,
				msg.as_ptr(),
				msg.len() as _,
				output.as_mut_ptr(),
			)
		};
		ret_code.into()
	}

	fn return_data_size(output: &mut [u8; 32]) {
		unsafe { sys::return_data_size(output.as_mut_ptr()) };
	}

	fn return_data_copy(output: &mut &mut [u8], offset: u32) {
		let mut output_len = output.len() as u32;
		{
			unsafe { sys::return_data_copy(output.as_mut_ptr(), &mut output_len, offset) };
		}
		extract_from_slice(output, output_len as usize);
	}
}
