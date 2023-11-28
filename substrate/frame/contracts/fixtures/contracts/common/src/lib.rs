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
#![cfg(any(target_arch = "wasm32", target_arch = "riscv32"))]

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
	#[cfg(target_arch = "wasm32")]
	core::arch::wasm32::unreachable();

	#[cfg(target_arch = "riscv32")]
	// Safety: The unimp instruction is guaranteed to trap
	unsafe {
		core::arch::asm!("unimp");
		core::hint::unreachable_unchecked();
	}
}
