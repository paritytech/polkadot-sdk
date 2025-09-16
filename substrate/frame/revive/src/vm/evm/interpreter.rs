//! Custom EVM interpreter implementation using sp_core types

use crate::vm::{
	evm::{memory::Memory, stack::Stack},
	Ext,
};
use alloc::vec::Vec;
use core::marker::PhantomData;
use revm::interpreter::interpreter::ExtBytecode;

/// EVM interpreter state using sp_core types
#[derive(Debug)]
pub struct Interpreter<'a, E: Ext> {
	/// The bytecode being executed
	pub bytecode: ExtBytecode,
	/// The execution stack
	pub stack: Stack,
	/// Return data from the last call
	pub return_data: Vec<u8>,
	/// EVM memory
	pub memory: Memory,
	/// Input data for the current call
	pub input: Vec<u8>,
	/// Phantom data for the Ext type
	pub _phantom: PhantomData<&'a E>,
}

impl<'a, E: Ext> Interpreter<'a, E> {
	/// Create a new interpreter instance
	pub fn new(bytecode: ExtBytecode, input: Vec<u8>) -> Self {
		Self {
			bytecode,
			stack: Stack::new(),
			return_data: Vec::new(),
			memory: Memory::new(),
			input,
			_phantom: Default::default(),
		}
	}
}
