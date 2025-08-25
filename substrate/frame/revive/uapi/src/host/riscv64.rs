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
	pack_hi_lo, ReturnFlags,
};
use pallet_revive_proc_macro::unstable_hostfn;

mod sys {
	use crate::ReturnCode;

	#[polkavm_derive::polkavm_define_abi]
	mod abi {}

	impl abi::FromHost for ReturnCode {
		type Regs = (u64,);

		fn from_host((a0,): Self::Regs) -> Self {
			ReturnCode(a0 as _)
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
		pub fn set_storage_or_clear(
			flags: u32,
			key_ptr: *const u8,
			value_ptr: *const u8,
		) -> ReturnCode;
		pub fn clear_storage(flags: u32, key_ptr: *const u8, key_len: u32) -> ReturnCode;
		pub fn get_storage(
			flags: u32,
			key_ptr: *const u8,
			key_len: u32,
			out_ptr: *mut u8,
			out_len_ptr: *mut u32,
		) -> ReturnCode;
		pub fn get_storage_or_zero(flags: u32, key_ptr: *const u8, out_ptr: *mut u8);
		pub fn contains_storage(flags: u32, key_ptr: *const u8, key_len: u32) -> ReturnCode;
		pub fn take_storage(
			flags: u32,
			key_ptr: *const u8,
			key_len: u32,
			out_ptr: *mut u8,
			out_len_ptr: *mut u32,
		) -> ReturnCode;
		pub fn call(
			flags_and_callee: u64,
			ref_time_limit: u64,
			proof_size_limit: u64,
			deposit_and_value: u64,
			input_data: u64,
			output_data: u64,
		) -> ReturnCode;
		pub fn delegate_call(
			flags_and_callee: u64,
			ref_time_limit: u64,
			proof_size_limit: u64,
			deposit_ptr: *const u8,
			input_data: u64,
			output_data: u64,
		) -> ReturnCode;
		pub fn instantiate(
			ref_time_limit: u64,
			proof_size_limit: u64,
			deposit_and_value: u64,
			input_data: u64,
			output_data: u64,
			address_and_salt: u64,
		) -> ReturnCode;
		pub fn terminate(beneficiary_ptr: *const u8);
		pub fn call_data_copy(out_ptr: *mut u8, out_len: u32, offset: u32);
		pub fn call_data_load(out_ptr: *mut u8, offset: u32);
		pub fn seal_return(flags: u32, data_ptr: *const u8, data_len: u32);
		pub fn caller(out_ptr: *mut u8);
		pub fn origin(out_ptr: *mut u8);
		pub fn code_hash(address_ptr: *const u8, out_ptr: *mut u8);
		pub fn code_size(address_ptr: *const u8) -> u64;
		pub fn address(out_ptr: *mut u8);
		pub fn weight_to_fee(ref_time: u64, proof_size: u64, out_ptr: *mut u8);
		pub fn ref_time_left() -> u64;
		pub fn get_immutable_data(out_ptr: *mut u8, out_len_ptr: *mut u32);
		pub fn set_immutable_data(ptr: *const u8, len: u32);
		pub fn balance(out_ptr: *mut u8);
		pub fn balance_of(addr_ptr: *const u8, out_ptr: *mut u8);
		pub fn chain_id(out_ptr: *mut u8);
		pub fn value_transferred(out_ptr: *mut u8);
		pub fn now(out_ptr: *mut u8);
		pub fn gas_limit() -> u64;
		pub fn deposit_event(
			topics_ptr: *const [u8; 32],
			num_topic: u32,
			data_ptr: *const u8,
			data_len: u32,
		);
		pub fn gas_price() -> u64;
		pub fn base_fee(out_ptr: *mut u8);
		pub fn call_data_size() -> u64;
		pub fn block_number(out_ptr: *mut u8);
		pub fn block_hash(block_number_ptr: *const u8, out_ptr: *mut u8);
		pub fn block_author(out_ptr: *mut u8);
		pub fn hash_keccak_256(input_ptr: *const u8, input_len: u32, out_ptr: *mut u8);
		pub fn sr25519_verify(
			signature_ptr: *const u8,
			pub_key_ptr: *const u8,
			message_len: u32,
			message_ptr: *const u8,
		) -> ReturnCode;
		pub fn set_code_hash(code_hash_ptr: *const u8);
		pub fn ecdsa_to_eth_address(key_ptr: *const u8, out_ptr: *mut u8) -> ReturnCode;
		pub fn instantiation_nonce() -> u64;
		pub fn return_data_size() -> u64;
		pub fn return_data_copy(out_ptr: *mut u8, out_len_ptr: *mut u32, offset: u32);
	}
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
		ref_time_limit: u64,
		proof_size_limit: u64,
		deposit_limit: &[u8; 32],
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
		let (output_ptr, mut output_len_ptr) = ptr_len_or_sentinel(&mut output);
		let deposit_limit_ptr = deposit_limit.as_ptr();
		let salt_ptr = ptr_or_sentinel(&salt);

		let deposit_and_value = pack_hi_lo(deposit_limit_ptr as _, value.as_ptr() as _);
		let address_and_salt = pack_hi_lo(address as _, salt_ptr as _);
		let input_data = pack_hi_lo(input.len() as _, input.as_ptr() as _);
		let output_data = pack_hi_lo(&mut output_len_ptr as *mut _ as _, output_ptr as _);

		let ret_code = unsafe {
			sys::instantiate(
				ref_time_limit,
				proof_size_limit,
				deposit_and_value,
				input_data,
				output_data,
				address_and_salt,
			)
		};

		if let Some(ref mut output) = output {
			extract_from_slice(output, output_len_ptr as usize);
		}

		ret_code.into()
	}

	fn call(
		flags: CallFlags,
		callee: &[u8; 20],
		ref_time_limit: u64,
		proof_size_limit: u64,
		deposit_limit: &[u8; 32],
		value: &[u8; 32],
		input: &[u8],
		mut output: Option<&mut &mut [u8]>,
	) -> Result {
		let (output_ptr, mut output_len) = ptr_len_or_sentinel(&mut output);
		let deposit_limit_ptr = deposit_limit.as_ptr();

		let flags_and_callee = pack_hi_lo(flags.bits(), callee.as_ptr() as _);
		let deposit_and_value = pack_hi_lo(deposit_limit_ptr as _, value.as_ptr() as _);
		let input_data = pack_hi_lo(input.len() as _, input.as_ptr() as _);
		let output_data = pack_hi_lo(&mut output_len as *mut _ as _, output_ptr as _);

		let ret_code = unsafe {
			sys::call(
				flags_and_callee,
				ref_time_limit,
				proof_size_limit,
				deposit_and_value,
				input_data,
				output_data,
			)
		};

		if let Some(ref mut output) = output {
			extract_from_slice(output, output_len as usize);
		}

		ret_code.into()
	}

	fn delegate_call(
		flags: CallFlags,
		address: &[u8; 20],
		ref_time_limit: u64,
		proof_size_limit: u64,
		deposit_limit: &[u8; 32],
		input: &[u8],
		mut output: Option<&mut &mut [u8]>,
	) -> Result {
		let (output_ptr, mut output_len) = ptr_len_or_sentinel(&mut output);
		let deposit_limit_ptr = deposit_limit.as_ptr();

		let flags_and_callee = pack_hi_lo(flags.bits(), address.as_ptr() as u32);
		let input_data = pack_hi_lo(input.len() as u32, input.as_ptr() as u32);
		let output_data = pack_hi_lo(&mut output_len as *mut _ as u32, output_ptr as u32);

		let ret_code = unsafe {
			sys::delegate_call(
				flags_and_callee,
				ref_time_limit,
				proof_size_limit,
				deposit_limit_ptr as _,
				input_data,
				output_data,
			)
		};

		if let Some(ref mut output) = output {
			extract_from_slice(output, output_len as usize);
		}

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

	fn set_storage_or_clear(
		flags: StorageFlags,
		key: &[u8; 32],
		encoded_value: &[u8; 32],
	) -> Option<u32> {
		let ret_code = unsafe {
			sys::set_storage_or_clear(flags.bits(), key.as_ptr(), encoded_value.as_ptr())
		};
		ret_code.into()
	}

	fn get_storage_or_zero(flags: StorageFlags, key: &[u8; 32], output: &mut [u8; 32]) {
		unsafe { sys::get_storage_or_zero(flags.bits(), key.as_ptr(), output.as_mut_ptr()) };
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

	fn call_data_load(out_ptr: &mut [u8; 32], offset: u32) {
		unsafe { sys::call_data_load(out_ptr.as_mut_ptr(), offset) };
	}

	fn gas_limit() -> u64 {
		unsafe { sys::gas_limit() }
	}

	fn call_data_size() -> u64 {
		unsafe { sys::call_data_size() }
	}

	fn return_value(flags: ReturnFlags, return_value: &[u8]) -> ! {
		unsafe { sys::seal_return(flags.bits(), return_value.as_ptr(), return_value.len() as u32) }
		panic!("seal_return does not return");
	}

	fn gas_price() -> u64 {
		unsafe { sys::gas_price() }
	}

	fn base_fee(output: &mut [u8; 32]) {
		unsafe { sys::base_fee(output.as_mut_ptr()) }
	}

	fn balance(output: &mut [u8; 32]) {
		unsafe { sys::balance(output.as_mut_ptr()) }
	}

	fn value_transferred(output: &mut [u8; 32]) {
		unsafe { sys::value_transferred(output.as_mut_ptr()) }
	}

	fn now(output: &mut [u8; 32]) {
		unsafe { sys::now(output.as_mut_ptr()) }
	}

	fn chain_id(output: &mut [u8; 32]) {
		unsafe { sys::chain_id(output.as_mut_ptr()) }
	}

	fn address(output: &mut [u8; 20]) {
		unsafe { sys::address(output.as_mut_ptr()) }
	}

	fn caller(output: &mut [u8; 20]) {
		unsafe { sys::caller(output.as_mut_ptr()) }
	}

	fn origin(output: &mut [u8; 20]) {
		unsafe { sys::origin(output.as_mut_ptr()) }
	}

	fn block_number(output: &mut [u8; 32]) {
		unsafe { sys::block_number(output.as_mut_ptr()) }
	}

	fn block_author(output: &mut [u8; 20]) {
		unsafe { sys::block_author(output.as_mut_ptr()) }
	}

	fn weight_to_fee(ref_time_limit: u64, proof_size_limit: u64, output: &mut [u8; 32]) {
		unsafe { sys::weight_to_fee(ref_time_limit, proof_size_limit, output.as_mut_ptr()) };
	}

	fn hash_keccak_256(input: &[u8], output: &mut [u8; 32]) {
		unsafe { sys::hash_keccak_256(input.as_ptr(), input.len() as u32, output.as_mut_ptr()) }
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

	fn code_hash(address: &[u8; 20], output: &mut [u8; 32]) {
		unsafe { sys::code_hash(address.as_ptr(), output.as_mut_ptr()) }
	}

	fn code_size(address: &[u8; 20]) -> u64 {
		unsafe { sys::code_size(address.as_ptr()) }
	}

	fn return_data_size() -> u64 {
		unsafe { sys::return_data_size() }
	}

	fn return_data_copy(output: &mut &mut [u8], offset: u32) {
		let mut output_len = output.len() as u32;
		{
			unsafe { sys::return_data_copy(output.as_mut_ptr(), &mut output_len, offset) };
		}
		extract_from_slice(output, output_len as usize);
	}

	fn ref_time_left() -> u64 {
		unsafe { sys::ref_time_left() }
	}

	fn block_hash(block_number_ptr: &[u8; 32], output: &mut [u8; 32]) {
		unsafe { sys::block_hash(block_number_ptr.as_ptr(), output.as_mut_ptr()) };
	}

	fn call_data_copy(output: &mut [u8], offset: u32) {
		let len = output.len() as u32;
		unsafe { sys::call_data_copy(output.as_mut_ptr(), len, offset) };
	}

	#[unstable_hostfn]
	fn clear_storage(flags: StorageFlags, key: &[u8]) -> Option<u32> {
		let ret_code = unsafe { sys::clear_storage(flags.bits(), key.as_ptr(), key.len() as u32) };
		ret_code.into()
	}

	#[unstable_hostfn]
	fn contains_storage(flags: StorageFlags, key: &[u8]) -> Option<u32> {
		let ret_code =
			unsafe { sys::contains_storage(flags.bits(), key.as_ptr(), key.len() as u32) };
		ret_code.into()
	}

	#[unstable_hostfn]
	fn ecdsa_to_eth_address(pubkey: &[u8; 33], output: &mut [u8; 20]) -> Result {
		let ret_code = unsafe { sys::ecdsa_to_eth_address(pubkey.as_ptr(), output.as_mut_ptr()) };
		ret_code.into()
	}

	#[unstable_hostfn]
	fn set_code_hash(code_hash: &[u8; 32]) {
		unsafe { sys::set_code_hash(code_hash.as_ptr()) }
	}

	#[unstable_hostfn]
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

	#[unstable_hostfn]
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

	#[unstable_hostfn]
	fn terminate(beneficiary: &[u8; 20]) -> ! {
		unsafe { sys::terminate(beneficiary.as_ptr()) }
		panic!("terminate does not return");
	}
}
