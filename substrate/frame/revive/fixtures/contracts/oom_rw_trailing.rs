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

//! This creates a large rw section but the trailing zeroes
//! are removed by the linker. It should be rejected even
//! though the blob is small enough.

#![no_std]
#![no_main]

extern crate common;

use uapi::{HostFn, HostFnImpl as api, ReturnFlags};

static mut BUFFER: [u8; 2 * 1025 * 1024] = [0; 2 * 1025 * 1024];

unsafe fn buffer() -> &'static [u8; 2 * 1025 * 1024] {
	let ptr = core::ptr::addr_of!(BUFFER);
	&*ptr
}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub unsafe extern "C" fn call_never() {
	// make sure the buffer is not optimized away
	api::return_value(ReturnFlags::empty(), buffer());
}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {}
