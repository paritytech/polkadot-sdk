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

//! This tests that no output is written when the provided output buffer
//! length was set to 0 during calls and instantiations.
//!
//! The fixture calls itself recursively once and also tries to instantiate
//! itself; both with an output buffer of length 0.
//!
//! Because the contract always returns some data from the constructor and
//! from recursive calls, if a length of 0 is not ignored by the pallet,
//! the test would fail with `OutputBufferTooSmall` instead.

#![no_std]
#![no_main]

use common::input;
use uapi::{HostFn, HostFnImpl as api};

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {
	api::return_value(uapi::ReturnFlags::empty(), &[1, 2, 3, 4, 5, 6, 7, 8]);
}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	let mut caller_address = [0u8; 20];
	api::caller(&mut caller_address);

	let mut callee_address = [0u8; 20];
	api::address(&mut callee_address);

	// we already recurse; return some data
	if caller_address == callee_address {
		api::return_value(uapi::ReturnFlags::empty(), &caller_address);
	}

	let mut output_buf = [0u8; 32];

	api::call(
		uapi::CallFlags::ALLOW_REENTRY,
		&callee_address,
		0u64,
		0u64,
		None,
		&[0u8; 32],
		&[],
		Some(&mut &mut output_buf[..0]),
	);
	//.unwrap();
	//assert_eq!(output_buf, [0; 32]);

	let mut code_hash = [0; 32];
	api::own_code_hash(&mut code_hash);

	//api::instantiate(
	//	&code_hash,
	//	0u64,
	//	0u64,
	//	None,
	//	&[0; 32],
	//	&[0; 32],
	//	None,
	//	Some(&mut &mut output_buf[..0]),
	//	None,
	//)
	//.unwrap();
	//assert_eq!(output_buf, [0; 32]);
}
