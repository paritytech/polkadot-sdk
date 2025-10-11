// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

#![no_std]
#![no_main]
include!("../panic_handler.rs");

use uapi::{input, HostFn, HostFnImpl as api};

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
    input!(36, code_hash: [u8],);
    
    let mut addr = [0u8; 20];
    let salt = [1u8; 32];
    // Send 100,000 units when creating the contract (100_000 plank is 100_000_000_000 in balance (wei))
    let mut value_bytes = [0u8; 32];
    value_bytes[..8].copy_from_slice(&100_000_000_000u64.to_le_bytes()[..8]);
    api::instantiate(
        u64::MAX,
        u64::MAX,
        &[u8::MAX; 32],
        &value_bytes,
        code_hash,
        Some(&mut addr),
        None,
        Some(&salt),
    ).unwrap();
    
    api::call(
        uapi::CallFlags::empty(),
        &addr,
        u64::MAX,
        u64::MAX,
        &[0u8; 32],
        &[0u8; 32],
        &[],
        None,
    ).unwrap();
    
    // Return the address of the created (and destroyed) contract
    api::return_value(uapi::ReturnFlags::empty(), &addr);
}
