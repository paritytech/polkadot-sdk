//! Custom EVM interpreter implementation using sp_core types

use crate::{vm::{
	evm::{memory::Memory, stack::Stack},
	Ext,
}, DispatchError, Error};
use alloc::vec::Vec;
use core::marker::PhantomData;
use revm::interpreter::interpreter::ExtBytecode;

/// Out of gas error variants from revm
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutOfGasError {
	/// Basic out of gas error
	Basic,
	/// Out of gas during memory operations
	Memory,
	/// Out of gas due to memory limit exceeded
	MemoryLimit,
	/// Out of gas in precompile execution
	Precompile,
	/// Out of gas with invalid operand
	InvalidOperand,
	/// Out of gas in reentrancy sentry
	ReentrancySentry,
}

/// HaltReason enum from revm indicating that the EVM has experienced an exceptional halt
/// This causes execution to immediately end with all gas being consumed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HaltReason {
	/// Out of gas error with specific variant
	OutOfGas(OutOfGasError),
	/// Opcode not found error
	OpcodeNotFound,
	/// Invalid FE opcode error
	InvalidFEOpcode,
	/// Invalid jump destination
	InvalidJump,
	/// The feature or opcode is not activated in hardfork
	NotActivated,
	/// Attempting to pop a value from an empty stack
	StackUnderflow,
	/// Attempting to push a value onto a full stack
	StackOverflow,
	/// Invalid memory or storage offset
	OutOfOffset,
	/// Address collision during contract creation
	CreateCollision,
	/// Error in precompile execution
	PrecompileError,
	/// Nonce overflow error
	NonceOverflow,
	/// Contract size limit exceeded during creation
	CreateContractSizeLimit,
	/// Contract creation starting with EF prefix
	CreateContractStartingWithEF,
	/// Init code size limit exceeded
	CreateInitCodeSizeLimit,
	/// Payment overflow error
	OverflowPayment,
	/// State change attempted during static call
	StateChangeDuringStaticCall,
	/// Call not allowed inside static context
	CallNotAllowedInsideStatic,
	/// Insufficient funds for operation
	OutOfFunds,
	/// Maximum call depth exceeded
	CallTooDeep,
}

impl HaltReason {
	/// Returns `true` if this is an out of gas error
	pub const fn is_out_of_gas(&self) -> bool {
		matches!(self, Self::OutOfGas(_))
	}

	/// Returns the inner OutOfGasError if this is an OutOfGas variant
	pub const fn as_out_of_gas(&self) -> Option<OutOfGasError> {
		match self {
			Self::OutOfGas(err) => Some(*err),
			_ => None,
		}
	}
}

/// Convert from Error to HaltReason
/// This reverses the mapping logic from the commented `instruction_result_into_exec_error` function
impl<T> From<Error<T>> for HaltReason {
	fn from(error: Error<T>) -> Self {
		match error {
			Error::OutOfGas => Self::OutOfGas(OutOfGasError::Basic),
			Error::StaticMemoryTooLarge => Self::OutOfGas(OutOfGasError::MemoryLimit),
			Error::InvalidInstruction => Self::OpcodeNotFound,
			Error::StateChangeDenied => Self::StateChangeDuringStaticCall,
			Error::ContractTrapped => Self::PrecompileError,
			Error::OutOfBounds => Self::OutOfOffset,
			Error::DuplicateContract => Self::CreateCollision,
			Error::BalanceConversionFailed => Self::OverflowPayment,
			Error::MaxCallDepthReached => Self::CallTooDeep,
			Error::TransferFailed => Self::OutOfFunds,
			// Map other common error cases
			Error::CodeRejected => Self::PrecompileError,
			Error::ValueTooLarge => Self::OverflowPayment,
			Error::StorageDepositLimitExhausted => Self::OutOfGas(OutOfGasError::Basic),
			Error::ExecutionFailed => Self::PrecompileError,
			Error::InvalidCallFlags => Self::InvalidFEOpcode,
			Error::DecodingFailed => Self::PrecompileError,
			Error::InvalidSyscall => Self::OpcodeNotFound,
			// Map specific error cases that weren't in the original commented logic
			Error::ContractNotFound => Self::PrecompileError,
			Error::CodeNotFound => Self::PrecompileError,
			Error::CodeInfoNotFound => Self::PrecompileError,
			Error::TerminatedWhileReentrant => Self::PrecompileError,
			Error::InputForwarded => Self::PrecompileError,
			Error::TooManyTopics => Self::PrecompileError,
			Error::TerminatedInConstructor => Self::PrecompileError,
			Error::ReentranceDenied => Self::PrecompileError,
			Error::ReenteredPallet => Self::PrecompileError,
			Error::StorageDepositNotEnoughFunds => Self::OutOfFunds,
			Error::CodeInUse => Self::PrecompileError,
			Error::ContractReverted => Self::PrecompileError,
			Error::BlobTooLarge => Self::CreateContractSizeLimit,
			Error::BasicBlockTooLarge => Self::PrecompileError,
			Error::MaxDelegateDependenciesReached => Self::PrecompileError,
			Error::DelegateDependencyNotFound => Self::PrecompileError,
			Error::DelegateDependencyAlreadyExists => Self::CreateCollision,
			Error::CannotAddSelfAsDelegateDependency => Self::PrecompileError,
			Error::OutOfTransientStorage => Self::OutOfGas(OutOfGasError::Memory),
			Error::InvalidStorageFlags => Self::InvalidFEOpcode,
			// Default fallback for any unhandled errors - use PrecompileError as a general catch-all
			_ => Self::PrecompileError,
		}
	}
}

/// Convert from HaltReason to DispatchError
/// This implements the mapping logic from the commented `instruction_result_into_exec_error` function
impl From<HaltReason> for DispatchError {
	fn from(halt_reason: HaltReason) -> Self {
		match halt_reason {
			HaltReason::OutOfGas(OutOfGasError::Basic) |
			HaltReason::OutOfGas(OutOfGasError::InvalidOperand) |
			HaltReason::OutOfGas(OutOfGasError::ReentrancySentry) |
			HaltReason::OutOfGas(OutOfGasError::Precompile) |
			HaltReason::OutOfGas(OutOfGasError::Memory) => DispatchError::Other("OutOfGas"),
			HaltReason::OutOfGas(OutOfGasError::MemoryLimit) => DispatchError::Other("StaticMemoryTooLarge"),
			HaltReason::OpcodeNotFound |
			HaltReason::InvalidJump |
			HaltReason::NotActivated |
			HaltReason::InvalidFEOpcode |
			HaltReason::CreateContractStartingWithEF => DispatchError::Other("InvalidInstruction"),
			HaltReason::CallNotAllowedInsideStatic |
			HaltReason::StateChangeDuringStaticCall => DispatchError::Other("StateChangeDenied"),
			HaltReason::StackUnderflow |
			HaltReason::StackOverflow |
			HaltReason::NonceOverflow |
			HaltReason::PrecompileError => DispatchError::Other("ContractTrapped"),
			HaltReason::OutOfOffset => DispatchError::Other("OutOfBounds"),
			HaltReason::CreateCollision => DispatchError::Other("DuplicateContract"),
			HaltReason::OverflowPayment => DispatchError::Other("BalanceConversionFailed"),
			HaltReason::CreateContractSizeLimit |
			HaltReason::CreateInitCodeSizeLimit => DispatchError::Other("StaticMemoryTooLarge"),
			HaltReason::CallTooDeep => DispatchError::Other("MaxCallDepthReached"),
			HaltReason::OutOfFunds => DispatchError::Other("TransferFailed"),
		}
	}
}

/// EVM interpreter state using sp_core types
#[derive(Debug)]
pub struct Interpreter<'a, E: Ext> {
	pub ext: &'a mut E,
	/// The bytecode being executed
	pub bytecode: ExtBytecode,
	/// Input data for the current call
	pub input: Vec<u8>,
	/// The execution stack
	pub stack: Stack,
	/// Return data from the last call
	pub return_data: Vec<u8>,
	/// EVM memory
	pub memory: Memory,
}

impl<'a, E: Ext> Interpreter<'a, E> {
	/// Create a new interpreter instance
	pub fn new(bytecode: ExtBytecode, input: Vec<u8>, ext: &'a mut E) -> Self {
		Self {
			ext,
			bytecode,
			input,
			stack: Stack::new(),
			return_data: Vec::new(),
			memory: Memory::new(),
		}
	}
}

pub type InstructionTable<E> = [fn(Interpreter<'_, E>); 256];
