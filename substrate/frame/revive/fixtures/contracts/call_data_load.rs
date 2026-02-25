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

//! This uses the call data load API to first the first input byte.
//! This single input byte is used as the offset for a second call
//! to the call data load API.
//! The output of the second API call is returned.

#![no_std]
#![no_main]
include!("../panic_handler.rs");

use uapi::{HostFn, HostFnImpl as api, ReturnFlags};

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	let mut buf = [0; 32];
	api::call_data_load(&mut buf, 0);

	let offset = buf[31] as u32;
	let mut buf = [0; 32];
	api::call_data_load(&mut buf, offset);

	api::return_value(ReturnFlags::empty(), &buf);
}
