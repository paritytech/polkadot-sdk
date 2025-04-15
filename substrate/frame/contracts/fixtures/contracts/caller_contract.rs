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

#![no_std]
#![no_main]

use common::input;
use uapi::{HostFn, HostFnImpl as api, ReturnErrorCode};

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	input!(code_hash: [u8; 32],);

	// The value to transfer on instantiation and calls. Chosen to be greater than existential
	// deposit.
	let value = 32768u64.to_le_bytes();
	let salt = [0u8; 0];

	// Callee will use the first 4 bytes of the input to return an exit status.
	let input = [0u8, 1, 34, 51, 68, 85, 102, 119];
	let reverted_input = [1u8, 34, 51, 68, 85, 102, 119];

	// Fail to deploy the contract since it returns a non-zero exit status.
	let res = api::instantiate_v2(
		code_hash,
		0u64, // How much ref_time weight to devote for the execution. 0 = all.
		0u64, // How much proof_size weight to devote for the execution. 0 = all.
		None, // No deposit limit.
		&value,
		&reverted_input,
		None,
		None,
		&salt,
	);
	assert!(matches!(res, Err(ReturnErrorCode::CalleeReverted)));

	// Fail to deploy the contract due to insufficient ref_time weight.
	let res = api::instantiate_v2(
		code_hash, 1u64, // too little ref_time weight
		0u64, // How much proof_size weight to devote for the execution. 0 = all.
		None, // No deposit limit.
		&value, &input, None, None, &salt,
	);
	assert!(matches!(res, Err(ReturnErrorCode::CalleeTrapped)));

	// Fail to deploy the contract due to insufficient proof_size weight.
	let res = api::instantiate_v2(
		code_hash, 0u64, // How much ref_time weight to devote for the execution. 0 = all.
		1u64, // Too little proof_size weight
		None, // No deposit limit.
		&value, &input, None, None, &salt,
	);
	assert!(matches!(res, Err(ReturnErrorCode::CalleeTrapped)));

	// Deploy the contract successfully.
	let mut callee = [0u8; 32];
	let callee = &mut &mut callee[..];

	api::instantiate_v2(
		code_hash,
		0u64, // How much ref_time weight to devote for the execution. 0 = all.
		0u64, // How much proof_size weight to devote for the execution. 0 = all.
		None, // No deposit limit.
		&value,
		&input,
		Some(callee),
		None,
		&salt,
	)
	.unwrap();
	assert_eq!(callee.len(), 32);

	// Call the new contract and expect it to return failing exit code.
	let res = api::call_v2(
		uapi::CallFlags::empty(),
		callee,
		0u64, // How much ref_time weight to devote for the execution. 0 = all.
		0u64, // How much proof_size weight to devote for the execution. 0 = all.
		None, // No deposit limit.
		&value,
		&reverted_input,
		None,
	);
	assert!(matches!(res, Err(ReturnErrorCode::CalleeReverted)));

	// Fail to call the contract due to insufficient ref_time weight.
	let res = api::call_v2(
		uapi::CallFlags::empty(),
		callee,
		1u64, // Too little ref_time weight.
		0u64, // How much proof_size weight to devote for the execution. 0 = all.
		None, // No deposit limit.
		&value,
		&input,
		None,
	);
	assert!(matches!(res, Err(ReturnErrorCode::CalleeTrapped)));

	// Fail to call the contract due to insufficient proof_size weight.
	let res = api::call_v2(
		uapi::CallFlags::empty(),
		callee,
		0u64, // How much ref_time weight to devote for the execution. 0 = all.
		1u64, // too little proof_size weight
		None, // No deposit limit.
		&value,
		&input,
		None,
	);
	assert!(matches!(res, Err(ReturnErrorCode::CalleeTrapped)));

	// Call the contract successfully.
	let mut output = [0u8; 4];
	api::call_v2(
		uapi::CallFlags::empty(),
		callee,
		0u64, // How much ref_time weight to devote for the execution. 0 = all.
		0u64, // How much proof_size weight to devote for the execution. 0 = all.
		None, // No deposit limit.
		&value,
		&input,
		Some(&mut &mut output[..]),
	)
	.unwrap();
	assert_eq!(&output, &input[4..])
}
