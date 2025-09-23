//! Custom EVM interpreter implementation using sp_core types

use super::ExtBytecode;
use crate::vm::{
	evm::{memory::Memory, stack::Stack},
	ExecResult, Ext,
};
use alloc::vec::Vec;
use core::ops::ControlFlow;

#[derive(Debug, PartialEq)]
pub enum Halt {
	Return(Vec<u8>),
	Revert(Vec<u8>),

	Stop,
	SelfDestruct,

	OutOfGas,
	StackOverflow,
	StackUnderflow,
	MemoryOOG,
	InvalidOperandOOG,

	InvalidJump,
	OpcodeNotFound,
	InvalidFEOpcode,

	StateChangeDuringStaticCall,
	CallDepthExceeded,
	CreateInitCodeSizeLimit,
	CallNotAllowedInsideStatic,

	FatalExternalError,
	ReentrancyGuard,

	OutOfOffset,
	NotActivated,
	EOFOpcodeDisabledInLegacy,
}

/// Convert ExecError to ControlFlow<Halt> using proper error type comparison
pub fn exec_error_into_halt<E: Ext>(from: crate::ExecError) -> ControlFlow<Halt> {
	use crate::{DispatchError, Error};

	let static_memory_too_large: DispatchError = Error::<E::T>::StaticMemoryTooLarge.into();
	let code_rejected: DispatchError = Error::<E::T>::CodeRejected.into();
	let transfer_failed: DispatchError = Error::<E::T>::TransferFailed.into();
	let duplicate_contract: DispatchError = Error::<E::T>::DuplicateContract.into();
	let value_too_large: DispatchError = Error::<E::T>::ValueTooLarge.into();
	let out_of_gas: DispatchError = Error::<E::T>::OutOfGas.into();
	let out_of_deposit: DispatchError = Error::<E::T>::StorageDepositLimitExhausted.into();
	let invalid_instruction: DispatchError = Error::<E::T>::InvalidInstruction.into();
	let state_change_denied: DispatchError = Error::<E::T>::StateChangeDenied.into();
	let contract_trapped: DispatchError = Error::<E::T>::ContractTrapped.into();
	let out_of_bounds: DispatchError = Error::<E::T>::OutOfBounds.into();
	let max_call_depth_reached: DispatchError = Error::<E::T>::MaxCallDepthReached.into();

	let halt = match from.error {
		err if err == static_memory_too_large => Halt::MemoryOOG,
		err if err == code_rejected => Halt::OpcodeNotFound,
		err if err == transfer_failed => Halt::OutOfGas,
		err if err == duplicate_contract => Halt::OutOfGas,
		err if err == value_too_large => Halt::OutOfGas,
		err if err == out_of_deposit => Halt::OutOfGas,
		err if err == out_of_gas => Halt::OutOfGas,
		err if err == invalid_instruction => Halt::OpcodeNotFound,
		err if err == state_change_denied => Halt::StateChangeDuringStaticCall,
		err if err == contract_trapped => Halt::FatalExternalError,
		err if err == out_of_bounds => Halt::OutOfOffset,
		err if err == max_call_depth_reached => Halt::CallDepthExceeded,
		_ => Halt::FatalExternalError,
	};

	ControlFlow::Break(halt)
}

/// EVM interpreter state using sp_core types
#[derive(Debug)]
pub struct Interpreter<'a, E: Ext> {
	/// Access to the environment
	pub ext: &'a mut E,
	/// The bytecode being executed
	pub bytecode: ExtBytecode,
	/// Input data for the current call
	pub input: Vec<u8>, // TODO maybe just &'a[u8]
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

	/// Convert a Halt reason into an ExecResult
	pub fn into_exec_result(&self, halt: Halt) -> ExecResult {
		use crate::{primitives::ExecReturnValue, Error, ExecError};
		use pallet_revive_uapi::ReturnFlags;

		match halt {
			Halt::Return(data) => Ok(ExecReturnValue { flags: ReturnFlags::empty(), data }),
			Halt::Revert(data) => Ok(ExecReturnValue { flags: ReturnFlags::REVERT, data }),
			Halt::Stop => Ok(ExecReturnValue { flags: ReturnFlags::empty(), data: Vec::new() }),
			Halt::SelfDestruct =>
				Ok(ExecReturnValue { flags: ReturnFlags::empty(), data: Vec::new() }),

			Halt::OutOfGas => Err(ExecError::from(Error::<E::T>::OutOfGas)),
			Halt::StackOverflow => Err(ExecError::from(Error::<E::T>::ContractTrapped)),
			Halt::StackUnderflow => Err(ExecError::from(Error::<E::T>::ContractTrapped)),
			Halt::MemoryOOG => Err(ExecError::from(Error::<E::T>::StaticMemoryTooLarge)),
			Halt::InvalidOperandOOG => Err(ExecError::from(Error::<E::T>::OutOfGas)),

			Halt::InvalidJump => Err(ExecError::from(Error::<E::T>::InvalidInstruction)),
			Halt::OpcodeNotFound => Err(ExecError::from(Error::<E::T>::InvalidInstruction)),
			Halt::InvalidFEOpcode => Err(ExecError::from(Error::<E::T>::InvalidInstruction)),

			Halt::StateChangeDuringStaticCall =>
				Err(ExecError::from(Error::<E::T>::StateChangeDenied)),
			Halt::CallDepthExceeded => Err(ExecError::from(Error::<E::T>::MaxCallDepthReached)),
			Halt::CreateInitCodeSizeLimit =>
				Err(ExecError::from(Error::<E::T>::StaticMemoryTooLarge)),
			Halt::CallNotAllowedInsideStatic =>
				Err(ExecError::from(Error::<E::T>::StateChangeDenied)),

			Halt::FatalExternalError => Err(ExecError::from(Error::<E::T>::ContractTrapped)),
			Halt::ReentrancyGuard => Err(ExecError::from(Error::<E::T>::ContractTrapped)),

			Halt::OutOfOffset => Err(ExecError::from(Error::<E::T>::OutOfBounds)),
			Halt::NotActivated => Err(ExecError::from(Error::<E::T>::InvalidInstruction)),
			Halt::EOFOpcodeDisabledInLegacy =>
				Err(ExecError::from(Error::<E::T>::InvalidInstruction)),
		}
	}
}
