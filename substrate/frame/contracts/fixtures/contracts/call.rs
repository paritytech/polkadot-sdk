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

//! This calls another contract as passed as its account id.
#![no_std]
#![no_main]

extern crate common;
extern crate uapi;
use uapi::{Api, CallFlags, ApiImpl as api};

#[no_mangle]
pub fn deploy() {}

#[no_mangle]
pub fn call() {
	let mut buffer = [0u8; 36]; // 4 bytes for the callee input data, 32 bytes for the callee address.
	let mut out = [0u8; 0]; // No output data.

	// Read the input data.
    api::input(&mut &mut buffer[..]);

	// xx Call the callee.
    api::call(
        CallFlags::empty(),
        &buffer[4..36],     // callee address.
        0u64,               // How much gas to devote for the execution. 0 = all.
        &buffer[36..],      // Pointer to value to transfer.
        &buffer[0..4],      // Pointer to input data buffer address.
        Some(&mut out[..]), // Pointer to output data buffer address.
    );
}
