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
    extract_from_slice,
    Result,
    ReturnCode,
};
use crate::{
    engine::on_chain::EncodeScope,
    ReturnFlags,
};
use scale::Encode;

mod sys {
    #[polkavm_derive::polkavm_import]
    extern "C" {
        #[polkavm_import(index = 1)]
        pub fn set_storage(
            key_ptr: *const u8,
            key_len: u32,
            value_ptr: *const u8,
            value_len: u32,
        ) -> u32;

        #[polkavm_import(index = 2)]
        pub fn clear_storage(key_ptr: *const u8, key_len: u32) -> u32;

        #[polkavm_import(index = 3)]
        pub fn get_storage(
            key_ptr: *const u8,
            key_len: u32,
            out_ptr: *mut u8,
            out_len_ptr: *mut u32,
        ) -> u32;

        #[polkavm_import(index = 4)]
        pub fn contains_storage(key_ptr: *const u8, key_len: u32) -> u32;

        #[polkavm_import(index = 5)]
        pub fn take_storage(
            key_ptr: *const u8,
            key_len: u32,
            out_ptr: *mut u8,
            out_len_ptr: *mut u32,
        ) -> u32;

        #[polkavm_import(index = 48)]
        pub fn transfer(account_ptr: *const u8, value_ptr: *const u8) -> u32;

        #[polkavm_import(index = 7)]
        pub fn call(ptr: *const u8) -> u32;

        #[polkavm_import(index = 9)]
        pub fn delegate_call(
            flags: u32,
            code_hash_ptr: *const u8,
            input_data_ptr: *const u8,
            input_data_len: u32,
            out_ptr: *mut u8,
            out_len_ptr: *mut u32,
        ) -> u32;

        #[polkavm_import(index = 10)]
        pub fn instantiate(ptr: *const u8) -> u32;

        #[polkavm_import(index = 12)]
        pub fn terminate(beneficiary_ptr: *const u8);

        #[polkavm_import(index = 13)]
        pub fn input(out_ptr: *mut u8, out_len_ptr: *mut u32);

        #[polkavm_import(index = 14)]
        pub fn seal_return(flags: u32, data_ptr: *const u8, data_len: u32);

        #[polkavm_import(index = 15)]
        pub fn caller(out_ptr: *mut u8, out_len_ptr: *mut u32);

        #[polkavm_import(index = 16)]
        pub fn is_contract(account_ptr: *const u8) -> u32;

        #[polkavm_import(index = 17)]
        pub fn code_hash(
            account_ptr: *const u8,
            out_ptr: *mut u8,
            out_len_ptr: *mut u32,
        ) -> u32;

        #[polkavm_import(index = 18)]
        pub fn own_code_hash(out_ptr: *mut u8, out_len_ptr: *mut u32);

        #[polkavm_import(index = 19)]
        pub fn caller_is_origin() -> u32;

        #[polkavm_import(index = 20)]
        pub fn caller_is_root() -> u32;

        #[polkavm_import(index = 21)]
        pub fn address(out_ptr: *mut u8, out_len_ptr: *mut u32);

        #[polkavm_import(index = 22)]
        pub fn weight_to_fee(gas: u64, out_ptr: *mut u8, out_len_ptr: *mut u32);

        #[polkavm_import(index = 24)]
        pub fn gas_left(out_ptr: *mut u8, out_len_ptr: *mut u32);

        #[polkavm_import(index = 26)]
        pub fn balance(out_ptr: *mut u8, out_len_ptr: *mut u32);

        #[polkavm_import(index = 27)]
        pub fn value_transferred(out_ptr: *mut u8, out_len_ptr: *mut u32);

        #[polkavm_import(index = 28)]
        pub fn now(out_ptr: *mut u8, out_len_ptr: *mut u32);

        #[polkavm_import(index = 29)]
        pub fn minimum_balance(out_ptr: *mut u8, out_len_ptr: *mut u32);

        #[polkavm_import(index = 30)]
        pub fn deposit_event(
            topics_ptr: *const u8,
            topics_len: u32,
            data_ptr: *const u8,
            data_len: u32,
        );

        #[polkavm_import(index = 31)]
        pub fn block_number(out_ptr: *mut u8, out_len_ptr: *mut u32);

        #[polkavm_import(index = 32)]
        pub fn hash_sha2_256(input_ptr: *const u8, input_len: u32, out_ptr: *mut u8);

        #[polkavm_import(index = 33)]
        pub fn hash_keccak_256(input_ptr: *const u8, input_len: u32, out_ptr: *mut u8);

        #[polkavm_import(index = 34)]
        pub fn hash_blake2_256(input_ptr: *const u8, input_len: u32, out_ptr: *mut u8);

        #[polkavm_import(index = 35)]
        pub fn hash_blake2_128(input_ptr: *const u8, input_len: u32, out_ptr: *mut u8);

        #[polkavm_import(index = 36)]
        pub fn call_chain_extension(
            id: u32,
            input_ptr: *const u8,
            input_len: u32,
            out_ptr: *mut u8,
            out_len_ptr: *mut u32,
        ) -> u32;

        #[polkavm_import(index = 37)]
        pub fn debug_message(str_ptr: *const u8, str_len: u32) -> u32;

        #[polkavm_import(index = 38)]
        pub fn call_runtime(call_ptr: *const u8, call_len: u32) -> u32;

        #[polkavm_import(index = 39)]
        pub fn ecdsa_recover(
            signature_ptr: *const u8,
            message_hash_ptr: *const u8,
            out_ptr: *mut u8,
        ) -> u32;

        #[polkavm_import(index = 40)]
        pub fn sr25519_verify(
            signature_ptr: *const u8,
            pub_key_ptr: *const u8,
            message_len: u32,
            message_ptr: *const u8,
        ) -> u32;

        #[polkavm_import(index = 41)]
        pub fn set_code_hash(code_hash_ptr: *const u8) -> u32;

        #[polkavm_import(index = 42)]
        pub fn ecdsa_to_eth_address(key_ptr: *const u8, out_ptr: *mut u8) -> u32;
    }
}

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
    let mut in_data = [0u8; 64];
    #[allow(trivial_casts)]
    (
        code_hash.as_ptr() as u32,
        gas_limit,
        endowment.as_ptr() as u32,
        input.as_ptr() as u32,
        input.len() as u32,
        out_address.as_mut_ptr() as u32,
        &mut address_len as *mut _ as u32,
        out_return_value.as_mut_ptr() as u32,
        &mut return_value_len as *mut _ as u32,
        salt.as_ptr() as u32,
        salt.len() as u32,
    )
        .encode_to(&mut EncodeScope::from(in_data.as_mut()));
    let ret_val = unsafe { sys::instantiate(in_data.as_ptr()) };
    extract_from_slice(out_address, address_len as usize);
    extract_from_slice(out_return_value, return_value_len as usize);
    ReturnCode(ret_val).into()
}

pub fn call(
    flags: u32,
    callee: &[u8],
    gas_limit: u64,
    value: &[u8],
    input: &[u8],
    output: &mut &mut [u8],
) -> Result {
    let mut output_len = output.len() as u32;
    let mut in_data = [0u8; 64];
    #[allow(trivial_casts)]
    (
        flags,
        callee.as_ptr() as u32,
        gas_limit,
        value.as_ptr() as u32,
        input.as_ptr() as u32,
        input.len() as u32,
        output.as_mut_ptr() as u32,
        &mut output_len as *mut _ as u32,
    )
        .encode_to(&mut EncodeScope::from(in_data.as_mut()));
    let ret_val = unsafe { sys::call(in_data.as_ptr()) };
    extract_from_slice(output, output_len as usize);
    ReturnCode(ret_val).into()
}

pub fn delegate_call(
    flags: u32,
    code_hash: &[u8],
    input: &[u8],
    output: &mut &mut [u8],
) -> Result {
    let mut output_len = output.len() as u32;
    let ret_val = unsafe {
        sys::delegate_call(
            flags,
            code_hash.as_ptr(),
            input.as_ptr(),
            input.len() as u32,
            output.as_mut_ptr(),
            &mut output_len,
        )
    };
    extract_from_slice(output, output_len as usize);
    ReturnCode(ret_val).into()
}

pub fn transfer(account_id: &[u8], value: &[u8]) -> Result {
    let ret_val = unsafe { sys::transfer(account_id.as_ptr(), value.as_ptr()) };
    ReturnCode(ret_val).into()
}

pub fn deposit_event(topics: &[u8], data: &[u8]) {
    unsafe {
        sys::deposit_event(
            topics.as_ptr(),
            topics.len() as u32,
            data.as_ptr(),
            data.len() as u32,
        )
    }
}

pub fn set_storage(key: &[u8], encoded_value: &[u8]) -> Option<u32> {
    let ret_val = unsafe {
        sys::set_storage(
            key.as_ptr(),
            key.len() as u32,
            encoded_value.as_ptr(),
            encoded_value.len() as u32,
        )
    };
    ReturnCode(ret_val).into()
}

pub fn clear_storage(key: &[u8]) -> Option<u32> {
    let ret_val = unsafe { sys::clear_storage(key.as_ptr(), key.len() as u32) };
    ret_val.into()
}

pub fn get_storage(key: &[u8], output: &mut &mut [u8]) -> Result {
    let mut output_len = output.len() as u32;
    let ret_val = unsafe {
        sys::get_storage(
            key.as_ptr(),
            key.len() as u32,
            output.as_mut_ptr(),
            &mut output_len,
        )
    };
    extract_from_slice(output, output_len as usize);
    ReturnCode(ret_val).into()
}

pub fn take_storage(key: &[u8], output: &mut &mut [u8]) -> Result {
    let mut output_len = output.len() as u32;
    let ret_val = unsafe {
        sys::take_storage(
            key.as_ptr(),
            key.len() as u32,
            output.as_mut_ptr(),
            &mut output_len,
        )
    };
    extract_from_slice(output, output_len as usize);
    ReturnCode(ret_val).into()
}

pub fn storage_contains(key: &[u8]) -> Option<u32> {
    let ret_val = unsafe { sys::contains_storage(key.as_ptr(), key.len() as u32) };
    ReturnCode(ret_val).into()
}

pub fn terminate(beneficiary: &[u8]) -> ! {
    unsafe {
        sys::terminate(beneficiary.as_ptr());
        core::hint::unreachable_unchecked();
    }
}

pub fn call_chain_extension(func_id: u32, input: &[u8], output: &mut &mut [u8]) -> u32 {
    let mut output_len = output.len() as u32;
    let ret_val = unsafe {
        sys::call_chain_extension(
            func_id,
            input.as_ptr(),
            input.len() as u32,
            output.as_mut_ptr(),
            &mut output_len,
        )
    };
    extract_from_slice(output, output_len as usize);
    ret_val
}

pub fn input(output: &mut &mut [u8]) {
    let mut output_len = output.len() as u32;
    unsafe { sys::input(output.as_mut_ptr(), &mut output_len) }
    extract_from_slice(output, output_len as usize);
}

pub fn return_value(flags: ReturnFlags, return_value: &[u8]) -> ! {
    unsafe {
        sys::seal_return(
            flags.into_u32(),
            return_value.as_ptr(),
            return_value.len() as u32,
        );
        core::hint::unreachable_unchecked();
    }
}

pub fn call_runtime(call: &[u8]) -> Result {
    let ret_val = unsafe { sys::call_runtime(call.as_ptr(), call.len() as u32) };
    ReturnCode(ret_val).into()
}

macro_rules! impl_wrapper_for {
    ( $( $name:ident, )* ) => {
        $(
            pub fn $name(output: &mut &mut [u8]) {
                let mut output_len = output.len() as u32;
                unsafe {
                    sys::$name(
                        output.as_mut_ptr(),
                        &mut output_len,
                    )
                }
                extract_from_slice(output, output_len as usize)
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

pub fn weight_to_fee(gas: u64, output: &mut &mut [u8]) {
    let mut output_len = output.len() as u32;
    unsafe { sys::weight_to_fee(gas, output.as_mut_ptr(), &mut output_len) }
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
        let ret_val = unsafe { sys::debug_message(bytes.as_ptr(), bytes.len() as u32) };
        if !matches!(
            ReturnCode(ret_val).into(),
            Err(super::Error::LoggingDisabled)
        ) {
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
    let ret_val = unsafe {
        sys::ecdsa_recover(
            signature.as_ptr(),
            message_hash.as_ptr(),
            output.as_mut_ptr(),
        )
    };
    ReturnCode(ret_val).into()
}

pub fn ecdsa_to_eth_address(pubkey: &[u8; 33], output: &mut [u8; 20]) -> Result {
    let ret_val =
        unsafe { sys::ecdsa_to_eth_address(pubkey.as_ptr(), output.as_mut_ptr()) };
    ReturnCode(ret_val).into()
}

/// **WARNING**: this function is from the [unstable interface](https://github.com/paritytech/substrate/tree/master/frame/contracts#unstable-interfaces),
/// which is unsafe and normally is not available on production chains.
pub fn sr25519_verify(
    signature: &[u8; 64],
    message: &[u8],
    pub_key: &[u8; 32],
) -> Result {
    let ret_val = unsafe {
        sys::sr25519_verify(
            signature.as_ptr(),
            pub_key.as_ptr(),
            message.len() as u32,
            message.as_ptr(),
        )
    };
    ReturnCode(ret_val).into()
}

pub fn is_contract(account_id: &[u8]) -> bool {
    let ret_val = unsafe { sys::is_contract(account_id.as_ptr()) };
    ReturnCode(ret_val).into_bool()
}

pub fn caller_is_origin() -> bool {
    let ret_val = unsafe { sys::caller_is_origin() };
    ReturnCode(ret_val).into_bool()
}

pub fn set_code_hash(code_hash: &[u8]) -> Result {
    let ret_val = unsafe { sys::set_code_hash(code_hash.as_ptr()) };
    ReturnCode(ret_val).into()
}

pub fn code_hash(account_id: &[u8], output: &mut [u8]) -> Result {
    let mut output_len = output.len() as u32;
    let ret_val = unsafe {
        sys::code_hash(account_id.as_ptr(), output.as_mut_ptr(), &mut output_len)
    };
    ReturnCode(ret_val).into()
}

pub fn own_code_hash(output: &mut [u8]) {
    let mut output_len = output.len() as u32;
    unsafe { sys::own_code_hash(output.as_mut_ptr(), &mut output_len) }
}

