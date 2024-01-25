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

// This fixture tests if account_reentrance_count works as expected.
#![no_std]
#![no_main]

use common::{input, output};
use uapi::{HostFn, HostFnImpl as api};

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	input!(expected_reentrance_count: u32,);

	// Read the contract address.
	output!(addr, [0u8; 32], api::address,);

	#[allow(deprecated)]
	let reentrance_count = api::reentrance_count();
	assert_eq!(reentrance_count, expected_reentrance_count);

	// Re-enter 5 times in a row and assert that the reentrant counter works as expected.
	if expected_reentrance_count != 5 {
		let count = (expected_reentrance_count + 1).to_le_bytes();

		api::call_v1(
			uapi::CallFlags::ALLOW_REENTRY,
			addr,
			0u64,                // How much gas to devote for the execution. 0 = all.
			&0u64.to_le_bytes(), // value transferred to the contract.
			&count,
			None,
		)
		.unwrap();
	}
}
