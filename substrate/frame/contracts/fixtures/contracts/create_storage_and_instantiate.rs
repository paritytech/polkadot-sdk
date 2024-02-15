// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! This instantiates another contract and passes some input to its constructor.
#![no_std]
#![no_main]

use common::input;
use uapi::{HostFn, HostFnImpl as api};

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	input!(
		input: [u8; 4],
		code_hash: [u8; 32],
		deposit_limit: [u8; 8],
	);

	let value = 10_000u64.to_le_bytes();
	let salt = [0u8; 0];
	let mut address = [0u8; 32];
	let address = &mut &mut address[..];

	#[allow(deprecated)]
	api::instantiate_v2(
		code_hash,
		0u64, // How much ref_time weight to devote for the execution. 0 = all.
		0u64, // How much proof_size weight to devote for the execution. 0 = all.
		Some(deposit_limit),
		&value,
		input,
		Some(address),
		None,
		&salt,
	)
	.unwrap();

	// Return the deployed contract address.
	api::return_value(uapi::ReturnFlags::empty(), address);
}
