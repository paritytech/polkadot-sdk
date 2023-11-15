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
    Ptr32,
    Ptr32Mut,
    Result,
    ReturnCode,
};
use crate::ReturnFlags;
use scale::Encode;

// TODO: Remove the constant and use the real func ids.
const FUNC_ID: u32 = 0;

mod sys {
    use super::{
        Ptr32,
        ReturnCode,
    };
    use core::arch::asm;

    fn ecall(mut a0: u32, a1: u32) -> u32 {
        unsafe {
            asm!(
                "ecall",
                inout("a0") a0,
                in("a1") a1,
            );
        }
        a0
    }

    fn ecall0(mut a0: u32) -> u32 {
        unsafe {
            asm!(
                "ecall",
                inout("a0") a0,
            );
        }
        a0
    }

    pub fn call(func_id: u32, in_ptr: Ptr32<[u8]>) -> ReturnCode {
        ReturnCode(ecall(func_id, in_ptr._value))
    }

    pub fn call0(func_id: u32) -> ReturnCode {
        ReturnCode(ecall0(func_id))
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
    let ret_code = (
        Ptr32::from_slice(code_hash),
        gas_limit,
        Ptr32::from_slice(endowment),
        Ptr32::from_slice(input),
        input.len() as u32,
        Ptr32Mut::from_slice(out_address),
        Ptr32Mut::from_ref(&mut address_len),
        Ptr32Mut::from_slice(out_return_value),
        Ptr32Mut::from_ref(&mut return_value_len),
        Ptr32::from_slice(salt),
        salt.len() as u32,
    )
        .using_encoded(|in_data| sys::call(FUNC_ID, Ptr32::from_slice(in_data)));
    extract_from_slice(out_address, address_len as usize);
    extract_from_slice(out_return_value, return_value_len as usize);
    ret_code.into()
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
    let ret_code = (
        flags,
        Ptr32::from_slice(callee),
        gas_limit,
        Ptr32::from_slice(value),
        Ptr32::from_slice(input),
        input.len() as u32,
        Ptr32Mut::from_slice(output),
        Ptr32Mut::from_ref(&mut output_len),
    )
        .using_encoded(|in_data| sys::call(FUNC_ID, Ptr32::from_slice(in_data)));
    extract_from_slice(output, output_len as usize);
    ret_code.into()
}

pub fn delegate_call(
    flags: u32,
    code_hash: &[u8],
    input: &[u8],
    output: &mut &mut [u8],
) -> Result {
    let mut output_len = output.len() as u32;
    let ret_code = (
        flags,
        Ptr32::from_slice(code_hash),
        Ptr32::from_slice(input),
        input.len() as u32,
        Ptr32Mut::from_slice(output),
        Ptr32Mut::from_ref(&mut output_len),
    )
        .using_encoded(|in_data| sys::call(FUNC_ID, Ptr32::from_slice(in_data)));
    extract_from_slice(output, output_len as usize);
    ret_code.into()
}

pub fn transfer(account_id: &[u8], value: &[u8]) -> Result {
    let ret_code = (
        Ptr32::from_slice(account_id),
        account_id.len() as u32,
        Ptr32::from_slice(value),
        value.len() as u32,
    )
        .using_encoded(|in_data| sys::call(FUNC_ID, Ptr32::from_slice(in_data)));
    ret_code.into()
}

pub fn deposit_event(topics: &[u8], data: &[u8]) {
    (
        Ptr32::from_slice(topics),
        topics.len() as u32,
        Ptr32::from_slice(data),
        data.len() as u32,
    )
        .using_encoded(|in_data| sys::call(FUNC_ID, Ptr32::from_slice(in_data)));
}

pub fn set_storage(key: &[u8], encoded_value: &[u8]) -> Option<u32> {
    let ret_code = (
        Ptr32::from_slice(key),
        key.len() as u32,
        Ptr32::from_slice(encoded_value),
        encoded_value.len() as u32,
    )
        .using_encoded(|in_data| sys::call(FUNC_ID, Ptr32::from_slice(in_data)));
    ret_code.into()
}

pub fn clear_storage(key: &[u8]) -> Option<u32> {
    let ret_code = (Ptr32::from_slice(key), key.len() as u32)
        .using_encoded(|in_data| sys::call(FUNC_ID, Ptr32::from_slice(in_data)));
    ret_code.into()
}

pub fn get_storage(key: &[u8], output: &mut &mut [u8]) -> Result {
    let mut output_len = output.len() as u32;
    let ret_code = (
        Ptr32::from_slice(key),
        key.len() as u32,
        Ptr32Mut::from_slice(output),
        Ptr32Mut::from_ref(&mut output_len),
    )
        .using_encoded(|in_data| sys::call(FUNC_ID, Ptr32::from_slice(in_data)));
    extract_from_slice(output, output_len as usize);
    ret_code.into()
}

pub fn take_storage(key: &[u8], output: &mut &mut [u8]) -> Result {
    let mut output_len = output.len() as u32;
    let ret_code = (
        Ptr32::from_slice(key),
        key.len() as u32,
        Ptr32Mut::from_slice(output),
        Ptr32Mut::from_ref(&mut output_len),
    )
        .using_encoded(|in_data| sys::call(FUNC_ID, Ptr32::from_slice(in_data)));
    extract_from_slice(output, output_len as usize);
    ret_code.into()
}

pub fn storage_contains(key: &[u8]) -> Option<u32> {
    let ret_code = (Ptr32::from_slice(key), key.len() as u32)
        .using_encoded(|in_data| sys::call(FUNC_ID, Ptr32::from_slice(in_data)));
    ret_code.into()
}

pub fn terminate(beneficiary: &[u8]) -> ! {
    (Ptr32::from_slice(beneficiary))
        .using_encoded(|in_data| sys::call(FUNC_ID, Ptr32::from_slice(in_data)));
    unsafe {
        core::hint::unreachable_unchecked();
    }
}

pub fn call_chain_extension(func_id: u32, input: &[u8], output: &mut &mut [u8]) -> u32 {
    let mut output_len = output.len() as u32;
    let ret_code = (
        func_id,
        Ptr32::from_slice(input),
        input.len() as u32,
        Ptr32Mut::from_slice(output),
        Ptr32Mut::from_ref(&mut output_len),
    )
        .using_encoded(|in_data| sys::call(FUNC_ID, Ptr32::from_slice(in_data)));
    extract_from_slice(output, output_len as usize);
    ret_code.into_u32()
}

pub fn input(output: &mut &mut [u8]) {
    let mut output_len = output.len() as u32;
    (
        Ptr32Mut::from_slice(output),
        Ptr32Mut::from_ref(&mut output_len),
    )
        .using_encoded(|in_data| sys::call(FUNC_ID, Ptr32::from_slice(in_data)));
    extract_from_slice(output, output_len as usize);
}

pub fn return_value(flags: ReturnFlags, return_value: &[u8]) -> ! {
    (
        flags.into_u32(),
        Ptr32::from_slice(return_value),
        return_value.len() as u32,
    )
        .using_encoded(|in_data| sys::call(FUNC_ID, Ptr32::from_slice(in_data)));
    unsafe {
        core::hint::unreachable_unchecked();
    }
}

pub fn call_runtime(call: &[u8]) -> Result {
    let ret_code = (Ptr32::from_slice(call), call.len() as u32)
        .using_encoded(|in_data| sys::call(FUNC_ID, Ptr32::from_slice(in_data)));
    ret_code.into()
}

macro_rules! impl_wrapper_for {
    ( $( $name:ident, )* ) => {
        $(

            pub fn $name(output: &mut &mut [u8]) {
                let mut output_len = output.len() as u32;
                (
                    Ptr32Mut::from_slice(output),
                    Ptr32Mut::from_ref(&mut output_len),
                ).using_encoded(|in_data| sys::call(FUNC_ID, Ptr32::from_slice(in_data)));
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
    {
        (
            gas,
            Ptr32Mut::from_slice(output),
            Ptr32Mut::from_ref(&mut output_len),
        )
            .using_encoded(|in_data| sys::call(FUNC_ID, Ptr32::from_slice(in_data)));
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
        let ret_code = (Ptr32::from_slice(bytes), bytes.len() as u32)
            .using_encoded(|in_data| sys::call(FUNC_ID, Ptr32::from_slice(in_data)));
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
                (
                    Ptr32::from_slice(input),
                    input.len() as u32,
                    Ptr32Mut::from_slice(output),
                ).using_encoded(|in_data| sys::call(FUNC_ID, Ptr32::from_slice(in_data)));
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
    let ret_code = (
        Ptr32::from_slice(signature),
        Ptr32::from_slice(message_hash),
        Ptr32Mut::from_slice(output),
    )
        .using_encoded(|in_data| sys::call(FUNC_ID, Ptr32::from_slice(in_data)));
    ret_code.into()
}

pub fn ecdsa_to_eth_address(pubkey: &[u8; 33], output: &mut [u8; 20]) -> Result {
    let ret_code = (Ptr32::from_slice(pubkey), Ptr32Mut::from_slice(output))
        .using_encoded(|in_data| sys::call(FUNC_ID, Ptr32::from_slice(in_data)));
    ret_code.into()
}

/// **WARNING**: this function is from the [unstable interface](https://github.com/paritytech/substrate/tree/master/frame/contracts#unstable-interfaces),
/// which is unsafe and normally is not available on production chains.
pub fn sr25519_verify(
    signature: &[u8; 64],
    message: &[u8],
    pub_key: &[u8; 32],
) -> Result {
    let ret_code = (
        Ptr32::from_slice(signature),
        Ptr32::from_slice(pub_key),
        message.len() as u32,
        Ptr32::from_slice(message),
    )
        .using_encoded(|in_data| sys::call(FUNC_ID, Ptr32::from_slice(in_data)));
    ret_code.into()
}

pub fn is_contract(account_id: &[u8]) -> bool {
    let ret_val = sys::call(FUNC_ID, Ptr32::from_slice(account_id));
    ret_val.into_bool()
}

pub fn caller_is_origin() -> bool {
    let ret_val = sys::call0(FUNC_ID);
    ret_val.into_bool()
}

pub fn set_code_hash(code_hash: &[u8]) -> Result {
    let ret_val = sys::call(FUNC_ID, Ptr32::from_slice(code_hash));
    ret_val.into()
}

pub fn code_hash(account_id: &[u8], output: &mut [u8]) -> Result {
    let mut output_len = output.len() as u32;
    let ret_val = (
        Ptr32::from_slice(account_id),
        Ptr32Mut::from_slice(output),
        Ptr32Mut::from_ref(&mut output_len),
    )
        .using_encoded(|in_data| sys::call(FUNC_ID, Ptr32::from_slice(in_data)));
    ret_val.into()
}

pub fn own_code_hash(output: &mut [u8]) {
    let mut output_len = output.len() as u32;
    (
        Ptr32Mut::from_slice(output),
        Ptr32Mut::from_ref(&mut output_len),
    )
        .using_encoded(|in_data| sys::call(FUNC_ID, Ptr32::from_slice(in_data)));
}
