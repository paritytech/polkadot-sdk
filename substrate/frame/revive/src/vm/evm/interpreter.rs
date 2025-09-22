//! Custom EVM interpreter implementation using sp_core types

use super::ExtBytecode;
use crate::{
	vm::{
		evm::{memory::Memory, stack::Stack},
		Ext,
	},
	DispatchError, Error,
};
use alloc::vec::Vec;
use core::ops::ControlFlow;

#[derive(Debug, PartialEq)]
pub enum Halt {
	// Successful termination with output
	Return(Vec<u8>), // InstructionResult::Return
	Revert(Vec<u8>), // InstructionResult::Revert

	// Successful termination without output
	Stop,         // InstructionResult::Stop
	SelfDestruct, // InstructionResult::SelfDestruct

	// Resource limit errors
	OutOfGas,       // InstructionResult::OutOfGas
	StackOverflow,  // InstructionResult::StackOverflow
	StackUnderflow, // InstructionResult::StackUnderflow
	MemoryOOG,      // InstructionResult::MemoryOOG
	InvalidOperandOOG,

	// Invalid operation errors
	InvalidJump,     // InstructionResult::InvalidJump
	OpcodeNotFound,  // InstructionResult::OpcodeNotFound
	InvalidFEOpcode, // InstructionResult::InvalidFEOpcode

	// EVM rule violations
	StateChangeDuringStaticCall, // InstructionResult::StateChangeDuringStaticCall
	CallDepthExceeded,           // InstructionResult::CallTooDeep
	CreateInitCodeSizeLimit,     // InstructionResult::CreateInitCodeSizeLimit
	CallNotAllowedInsideStatic,  // InstructionResult::CallNotAllowedInsideStatic

	// External/system errors
	FatalExternalError, // InstructionResult::FatalExternalError
	ReentrancyGuard,    // InstructionResult::ReentrancySentryOOG

	// Additional REVM errors you might want
	OutOfOffset,               // InstructionResult::OutOfOffset
	NotActivated,              // InstructionResult::NotActivated (for EIP activation)
	EOFOpcodeDisabledInLegacy, // InstructionResult::EOFOpcodeDisabledInLegacy
}

impl From<crate::ExecError> for ControlFlow<Halt> {
	fn from(err: crate::ExecError) -> Self {
		todo!()
	}
}

/// EVM interpreter state using sp_core types
#[derive(Debug)]
pub struct Interpreter<'a, E: Ext> {
	/// Access to the environment
	pub ext: &'a mut E,
	/// The bytecode being executed
	pub bytecode: ExtBytecode,
	/// Input data for the current call
	pub input: Vec<u8>,
	/// The execution stack
	pub stack: Stack,
	/// EVM memory
	pub memory: Memory,
}

impl<'a, E: Ext> Interpreter<'a, E> {
	/// Create a new interpreter instance
	pub fn new(bytecode: ExtBytecode, input: Vec<u8>, ext: &'a mut E) -> Self {
		Self { ext, bytecode, input, stack: Stack::new(), memory: Memory::new() }
	}
}

pub type InstructionTable<E> = [fn(Interpreter<'_, E>); 256];
