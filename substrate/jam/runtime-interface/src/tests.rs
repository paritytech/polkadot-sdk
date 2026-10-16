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

use crate::{FunctionContext, HostFunction, HostFunctions as _, Registers};
use alloc::{string::String, vec, vec::Vec};

crate::host_calls! {
	/// Reverses the `len` bytes at `input` into `out`, returning `len`.
	#[index = 10]
	pub fn ext_test_reverse(
		ctx: &mut dyn FunctionContext,
		input: *const u8,
		len: u64,
		out: *mut u8,
	) -> u64 {
		let mut data = ctx.read_memory(input, len)?;
		data.reverse();
		ctx.write_memory(out, &data)?;
		Ok(len)
	}

	/// Writes `value` into the first `out_len` bytes of `out`.
	#[index = 11]
	pub fn ext_test_fill(ctx: &mut dyn FunctionContext, value: u32, out: *mut u8, out_len: u64) {
		ctx.with_buffer(out, out_len, |buffer| buffer.fill(value as u8))
	}

	/// Runtime-only host call.
	#[index = 0]
	pub fn ext_test_runtime_only(value: i64) -> i32;
}

/// A guest memory of 64 bytes.
struct Memory(Vec<u8>);

impl Memory {
	fn range(&self, ptr: *const u8, len: usize) -> Result<core::ops::Range<usize>, String> {
		let start = ptr as usize;
		let end = start.checked_add(len).filter(|end| *end <= self.0.len());
		end.map(|end| start..end).ok_or_else(|| String::from("out of bounds"))
	}
}

impl FunctionContext for Memory {
	fn read_memory_into(&mut self, ptr: *const u8, dest: &mut [u8]) -> Result<(), String> {
		let range = self.range(ptr, dest.len())?;
		dest.copy_from_slice(&self.0[range]);
		Ok(())
	}

	fn write_memory(&mut self, ptr: *mut u8, data: &[u8]) -> Result<(), String> {
		let range = self.range(ptr.cast_const(), data.len())?;
		self.0[range].copy_from_slice(data);
		Ok(())
	}
}

struct Args<'a>(&'a [u64]);

impl Registers for Args<'_> {
	fn arg(&self, index: usize) -> u64 {
		self.0[index]
	}
}

/// The name of every host call with a body.
fn host_functions() -> Vec<&'static str> {
	HostFunctions::host_functions().iter().map(|f| f.name()).collect()
}

#[test]
fn indices_list_every_host_call() {
	assert_eq!(
		HOST_CALL_INDICES,
		&[(10, "ext_test_reverse"), (11, "ext_test_fill"), (0, "ext_test_runtime_only")][..]
	);
}

#[test]
fn host_functions_only_contain_host_calls_with_body() {
	assert_eq!(host_functions(), ["ext_test_reverse", "ext_test_fill"]);
}

#[test]
fn execute_reads_and_writes_guest_memory() {
	let mut memory = Memory(vec![0; 64]);
	memory.0[8..12].copy_from_slice(b"abcd");

	let result = ext_test_reverse.execute(&mut memory, &Args(&[8, 4, 32]));

	assert_eq!(result, Ok(Some(4)));
	assert_eq!(&memory.0[32..36], b"dcba");

	let result = ext_test_fill.execute(&mut memory, &Args(&[7, 40, 3]));

	assert_eq!(result, Ok(None));
	assert_eq!(&memory.0[40..44], &[7, 7, 7, 0]);
}

#[test]
fn execute_rejects_invalid_arguments() {
	let mut memory = Memory(vec![0; 64]);

	// Memory access out of bounds.
	let result = ext_test_reverse.execute(&mut memory, &Args(&[60, 8, 0]));
	assert_eq!(result, Err(String::from("out of bounds")));

	// Length outside of the 32-bit guest address space.
	let result = ext_test_fill.execute(&mut memory, &Args(&[7, 0, 1 << 32]));
	assert_eq!(result, Err(String::from("length 4294967296 exceeds the guest address space")));
}
