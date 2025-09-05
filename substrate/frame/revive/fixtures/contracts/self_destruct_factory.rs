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
    
    api::instantiate(
        u64::MAX,
        u64::MAX,
        &[u8::MAX; 32],
        &[0u8; 32],
        code_hash,
        Some(&mut addr),
        None,
        Some(&salt),
    ).unwrap();
    let mut contract_info_buffer = [0u8; 128];
    let mut contract_info_slice = &mut contract_info_buffer[..];
    api::contract_info(&addr, &mut contract_info_slice).unwrap();
    
    // // Extract trie_id from the returned data (first 32 bytes)
    // let mut trie_id = [0u8; 32];
    // trie_id.copy_from_slice(&contract_info_slice[0..32]);
    
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
