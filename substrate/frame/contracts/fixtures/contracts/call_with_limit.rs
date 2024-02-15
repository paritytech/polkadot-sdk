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

//! This fixture calls the account_id with the 2D Weight limit.
//! It returns the result of the call as output data.
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
		256,
		callee_addr: [u8; 32],
		ref_time: u64,
		proof_size: u64,
		forwarded_input: [u8],
	);

	#[allow(deprecated)]
	api::call_v2(
		uapi::CallFlags::empty(),
		callee_addr,
		ref_time,
		proof_size,
		None,                // No deposit limit.
		&0u64.to_le_bytes(), // value transferred to the contract.
		forwarded_input,
		None,
	)
	.unwrap();
}
