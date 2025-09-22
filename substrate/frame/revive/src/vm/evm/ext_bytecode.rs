use core::{ops::Deref, ptr};
use revm::{
	bytecode::{utils::read_u16, Bytecode},
	interpreter::interpreter_types::{Immediates, Jumps},
};

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

	fn is_end(&self) -> bool {
		self.instruction_pointer.is_null()
	}
}

impl Jumps for ExtBytecode {
	fn relative_jump(&mut self, offset: isize) {
		self.instruction_pointer = unsafe { self.instruction_pointer.offset(offset) };
	}

	fn absolute_jump(&mut self, offset: usize) {
		self.instruction_pointer = unsafe { self.base.bytes_ref().as_ptr().add(offset) };
	}

	fn is_valid_legacy_jump(&mut self, offset: usize) -> bool {
		self.base.legacy_jump_table().expect("Panic if not legacy").is_valid(offset)
	}

	fn opcode(&self) -> u8 {
		// SAFETY: `instruction_pointer` always point to bytecode.
		unsafe { *self.instruction_pointer }
	}

	fn pc(&self) -> usize {
		// SAFETY: `instruction_pointer` should be at an offset from the start of the bytes.
		// In practice this is always true unless a caller modifies the `instruction_pointer` field
		// manually.
		unsafe { self.instruction_pointer.offset_from(self.base.bytes_ref().as_ptr()) as usize }
	}
}

impl Immediates for ExtBytecode {
	fn read_u16(&self) -> u16 {
		unsafe { read_u16(self.instruction_pointer) }
	}

	fn read_u8(&self) -> u8 {
		unsafe { *self.instruction_pointer }
	}

	fn read_slice(&self, len: usize) -> &[u8] {
		unsafe { core::slice::from_raw_parts(self.instruction_pointer, len) }
	}

	fn read_offset_u16(&self, offset: isize) -> u16 {
		unsafe {
			read_u16(
				self.instruction_pointer
					// Offset for max_index that is one byte
					.offset(offset),
			)
		}
	}
}
