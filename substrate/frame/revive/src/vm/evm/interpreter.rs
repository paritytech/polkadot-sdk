//! Custom EVM interpreter implementation using sp_core types

use super::ExtBytecode;
use crate::{
	primitives::ExecReturnValue,
	vm::{
		evm::{memory::Memory, stack::Stack},
		ExecResult, Ext,
	},
	Error, ExecError,
};
use alloc::vec::Vec;
use codec::{Decode, Encode};
use core::ops::ControlFlow;
use pallet_revive_uapi::ReturnFlags;
use scale_info::TypeInfo;

/// Reason for EVM execution halt
#[derive(Debug, PartialEq, Clone, Encode, Decode, codec::DecodeWithMemTracking, TypeInfo)]
pub enum HaltReason {
	// Gas and resource errors
	OutOfGas,
	MemoryOOG,
	InvalidOperandOOG,

	// Stack errors
	StackOverflow,
	StackUnderflow,

	// Jump and opcode errors
	InvalidJump,
	OpcodeNotFound,
	InvalidFEOpcode,
	NotActivated,
	EOFOpcodeDisabledInLegacy,

	// State change errors
	StateChangeDuringStaticCall,
	CallNotAllowedInsideStatic,

	// Call depth and contract creation
	CallDepthExceeded,
	CreateInitCodeSizeLimit,

	// External and system errors
	FatalExternalError,
	ReentrancyGuard,
	OutOfOffset,
	OutOfFund,
}

impl frame_support::traits::PalletError for HaltReason {
	const MAX_ENCODED_SIZE: usize = 1; // Single byte discriminant for enum variants
}

/// EVM execution halt - either successful termination or error
#[derive(Debug, PartialEq)]
pub enum Halt {
	Stop,
	Return(Vec<u8>),
	Revert(Vec<u8>),
	Err(HaltReason),
}

impl From<HaltReason> for Halt {
	fn from(reason: HaltReason) -> Self {
		Halt::Err(reason)
	}
}

/// Convert ExecError to ControlFlow<Halt>
///
/// This function checks if the error should result in a successful execution (with error code)
/// or if it should halt the execution. VM-triggered errors now use the Halt(HaltReason) pattern.
pub fn exec_error_into_halt<E: Ext>(from: crate::ExecError) -> ControlFlow<Halt> {
	use crate::{DispatchError, Error};

	// First check if this should be a non-halting success case
	if crate::vm::exec_error_into_return_code::<E>(from).is_ok() {
		return ControlFlow::Continue(());
	}

	// Map specific errors to halt reasons or fallback
	let out_of_gas: DispatchError = Error::<E::T>::OutOfGas.into();
	let invalid_instruction: DispatchError = Error::<E::T>::InvalidInstruction.into();
	let state_change_denied: DispatchError = Error::<E::T>::StateChangeDenied.into();
	let out_of_bounds: DispatchError = Error::<E::T>::OutOfBounds.into();
	let max_call_depth_reached: DispatchError = Error::<E::T>::MaxCallDepthReached.into();
	let static_memory_too_large: DispatchError = Error::<E::T>::StaticMemoryTooLarge.into();
	let value_too_large: DispatchError = Error::<E::T>::ValueTooLarge.into();
	let reentranced_pallet: DispatchError = Error::<E::T>::ReenteredPallet.into();
	let basic_block_too_large: DispatchError = Error::<E::T>::BasicBlockTooLarge.into();
	let transfer_failed: DispatchError = Error::<E::T>::TransferFailed.into();

	let halt_reason = match from.error {
		err if err == out_of_gas => HaltReason::OutOfGas,
		err if err == invalid_instruction => HaltReason::OpcodeNotFound,
		err if err == state_change_denied => HaltReason::StateChangeDuringStaticCall,
		err if err == out_of_bounds => HaltReason::OutOfOffset,
		err if err == max_call_depth_reached => HaltReason::CallDepthExceeded,
		err if err == static_memory_too_large => HaltReason::MemoryOOG,
		err if err == value_too_large => HaltReason::MemoryOOG,
		err if err == reentranced_pallet => HaltReason::ReentrancyGuard,
		err if err == basic_block_too_large => HaltReason::CreateInitCodeSizeLimit,
		err if err == transfer_failed => HaltReason::OutOfFund,
		_ => HaltReason::FatalExternalError,
	};

	ControlFlow::Break(Halt::Err(halt_reason))
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
		match halt {
			Halt::Stop => Ok(ExecReturnValue::default()),
			Halt::Return(data) => Ok(ExecReturnValue { flags: ReturnFlags::empty(), data }),
			Halt::Revert(data) => Ok(ExecReturnValue { flags: ReturnFlags::REVERT, data }),
			Halt::Err(reason) => Err(ExecError::from(Error::<E::T>::Halt(reason))),
		}
	}
}
