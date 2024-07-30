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

use common::{input, output};
use uapi::{HostFn, HostFnImpl as api};

const DJANGO: [u8; 32] = [4u8; 32];

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	// If the input data is not empty, then recursively call self with empty input data.
	// This should trap instead of self-destructing since a contract cannot be removed, while it's
	// in the execution stack. If the recursive call traps, then trap here as well.
	input!(input, 4,);

	if !input.is_empty() {
		output!(addr, [0u8; 32], api::address,);
		api::call_v2(
			uapi::CallFlags::ALLOW_REENTRY,
			addr,
			0u64,                // How much ref_time to devote for the execution. 0 = all.
			0u64,                // How much proof_size to devote for the execution. 0 = all.
			None,                // No deposit limit.
			&0u64.to_le_bytes(), // Value to transfer.
			&[0u8; 0],
			None,
		)
		.unwrap();
	} else {
		// Try to terminate and give balance to django.
		api::terminate_v1(&DJANGO);
	}
}
