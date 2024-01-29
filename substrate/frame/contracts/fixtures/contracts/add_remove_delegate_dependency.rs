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

//! This contract tests the behavior of adding / removing delegate_dependencies when delegate
//! calling into a contract.
#![no_std]
#![no_main]

use common::input;
use uapi::{HostFn, HostFnImpl as api};

const ALICE: [u8; 32] = [1u8; 32];

/// Load input data and perform the action specified by the input.
/// If `delegate_call` is true, then delegate call into the contract.
fn load_input(delegate_call: bool) {
	input!(
		action: u32,
		code_hash: [u8; 32],
	);

	match action {
		// 1 = Add delegate dependency
		1 => {
			#[allow(deprecated)]
			api::add_delegate_dependency(code_hash);
		},
		// 2 = Remove delegate dependency
		2 => {
			#[allow(deprecated)]
			api::remove_delegate_dependency(code_hash);
		},
		// 3 = Terminate
		3 => {
			api::terminate_v1(&ALICE);
		},
		// Everything else is a noop
		_ => {},
	}

	if delegate_call {
		api::delegate_call(uapi::CallFlags::empty(), code_hash, &[], None).unwrap();
	}
}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {
	load_input(false);
}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	load_input(true);
}
