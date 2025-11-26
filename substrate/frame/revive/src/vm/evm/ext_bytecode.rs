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
use core::ops::Deref;
use revm::bytecode::Bytecode;

/// Extended bytecode structure that wraps base bytecode with additional execution metadata.
#[derive(Debug)]
pub struct ExtBytecode {
	/// The base bytecode.
	base: Bytecode,
	/// The current instruction pointer.
	instruction_pointer: *const u8,
}

impl Deref for ExtBytecode {
	type Target = Bytecode;

	fn deref(&self) -> &Self::Target {
		&self.base
	}
}

impl Default for ExtBytecode {
	fn default() -> Self {
		Self::new(Bytecode::default())
	}
}

impl ExtBytecode {
	/// Create new extended bytecode and set the instruction pointer to the start of the bytecode.
	pub fn new(base: Bytecode) -> Self {
		let instruction_pointer = base.bytecode_ptr();
		Self { base, instruction_pointer }
	}

	pub fn bytecode_slice(&self) -> &[u8] {
		self.base.original_byte_slice()
	}

	/// Relative jumps does not require checking for overflow.
	pub fn relative_jump(&mut self, offset: isize) {
		// SAFETY: The offset is validated by the caller to ensure it points within the bytecode
		self.instruction_pointer = unsafe { self.instruction_pointer.offset(offset) };
	}

	/// Absolute jumps require checking for overflow and if target is a jump destination
	/// from jump table.
	pub fn absolute_jump(&mut self, offset: usize) {
		// SAFETY: The offset is validated by the caller to ensure it points within the bytecode
		self.instruction_pointer = unsafe { self.base.bytes_ref().as_ptr().add(offset) };
	}

	/// Check legacy jump destination from jump table.
	pub fn is_valid_legacy_jump(&mut self, offset: usize) -> bool {
		self.base.legacy_jump_table().expect("Panic if not legacy").is_valid(offset)
	}

	/// Returns current program counter.
	pub fn pc(&self) -> usize {
		// SAFETY: `instruction_pointer` should be at an offset from the start of the bytes.
		// In practice this is always true unless a caller modifies the `instruction_pointer` field
		// manually.
		unsafe { self.instruction_pointer.offset_from_unsigned(self.base.bytes_ref().as_ptr()) }
	}

	/// Returns instruction opcode.
	pub fn opcode(&self) -> u8 {
		// SAFETY: `instruction_pointer` always point to bytecode.
		unsafe { *self.instruction_pointer }
	}

	/// Reads next `len` bytes from the bytecode.
	///
	/// Used by PUSH opcode.
	pub fn read_slice(&self, len: usize) -> &[u8] {
		// SAFETY: The caller ensures that `len` bytes are available from the current instruction
		// pointer position.
		unsafe { core::slice::from_raw_parts(self.instruction_pointer, len) }
	}
}
