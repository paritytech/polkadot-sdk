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

//! Create a basic block that is larger than we allow.

#![no_std]
#![no_main]

extern crate common;

use core::arch::asm;

// Export that is never called. We can put code here that should be in the binary
// but is never supposed to be run.
#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call_never() {
	// Stores cannot be optimized away because the optimizer cannot
	// know whether they have side effects.
	let value: u32 = 42;
	unsafe {
		// Repeat 1001 times to intentionally exceed the allowed basic block limit (1000)
		asm!(".rept 1001", "sw {x}, 0(sp)", ".endr", x = in(reg) value);
	}
}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {}
