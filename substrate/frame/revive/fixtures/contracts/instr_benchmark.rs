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

extern crate common;

use common::input;
use uapi::{HostFn, HostFnImpl as api, ReturnFlags};

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	input!(rounds: u32, start: u32, div: u32, mult: u32, add: u32, );

	let mut acc = start;

	for _ in 0..rounds {
		acc = acc / div * mult + add;
	}

	api::return_value(ReturnFlags::empty(), start.to_le_bytes().as_ref());
}
