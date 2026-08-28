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

use sp_runtime_interface::{runtime_interface, Pointer};

#[allow(unused_imports)]
use crate::*;

/// Wasm only interface that provides functions for calling into the allocator.
#[runtime_interface(wasm_only)]
pub trait Allocator {
	/// Malloc the given number of bytes and return the pointer to the allocated memory location.
	fn malloc(&mut self, size: u32) -> Pointer<u8> {
		self.allocate_memory(size).expect("Failed to allocate memory")
	}

	/// Free the given pointer.
	fn free(&mut self, ptr: Pointer<u8>) {
		self.deallocate_memory(ptr).expect("Failed to deallocate memory")
	}
}
