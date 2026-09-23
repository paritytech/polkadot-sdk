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
use crate::{
	AccountIdOf, BalanceOf, CodeInfo, Config, ContractBlob, DispatchError, Error, H256, LOG_TARGET,
	Weight,
	debug::DebugSettings,
	precompiles::Token,
	tracing,
	vm::{
		BytecodeType, ExecResult, Ext, evm::instructions::exec_instruction,
		runtime_costs::cost_args,
	},
	weights::WeightInfo,
};
use alloc::vec::Vec;
use core::{convert::Infallible, ops::ControlFlow};
use revm::{bytecode::Bytecode, primitives::Bytes};

#[cfg(feature = "runtime-benchmarks")]
pub mod instructions;
#[cfg(not(feature = "runtime-benchmarks"))]
mod instructions;

mod interpreter;
pub use interpreter::{Halt, Interpreter};

mod ext_bytecode;
pub(crate) use ext_bytecode::ExtBytecode;

mod memory;
mod stack;
mod util;

/// Hard-coded value returned by the EVM `DIFFICULTY` opcode.
///
/// After Ethereum's Merge (Sept 2022), the `DIFFICULTY` opcode was redefined to return
/// `prevrandao`, a randomness value from the beacon chain. In Substrate pallet-revive
/// a fixed constant is returned instead for compatibility with contracts that still read this
/// opcode. The value is aligned with the difficulty hardcoded for PVM contracts.
pub(crate) const DIFFICULTY: u64 = 2500000000000000_u64;

/// Cost  for a single unit of EVM gas.
#[derive(Eq, PartialEq, Debug, Clone, Copy)]
pub struct EVMGas(pub u64);

impl<T: Config> Token<T> for EVMGas {
	fn weight(&self) -> Weight {
		let base_cost = T::WeightInfo::evm_jumpdest_opcode(1)
			.saturating_sub(T::WeightInfo::evm_jumpdest_opcode(0));
		base_cost.saturating_mul(self.0)
	}
}

/// Weight costs for EVM opcodes.
#[derive(Eq, PartialEq, Debug, Clone, Copy)]
pub(crate) enum EvmOpcodeCosts {
	JUMP,
	JUMPI,
	JUMPDEST,
	PUSH,
	POP,
	DUP,
	SWAP,
	PC,
	CHAINID,
	PREVRANDAO,
	CODESIZE,
	CALLDATALOAD,
	CALLDATASIZE,
	RETURNDATASIZE,
	ADD,
	MUL,
	SUB,
	DIV,
	SDIV,
	MOD,
	SMOD,
	ADDMOD,
	MULMOD,
	EXP { exponent_bits: u32 },
	SIGNEXTEND,
	LT,
	GT,
	CLZ,
	SLT,
	SGT,
	EQ,
	ISZERO,
	AND,
	OR,
	XOR,
	NOT,
	BYTE,
	SHL,
	SHR,
	SAR,
	MLOAD,
	MSTORE,
	MSTORE8,
	MSIZE,
	MCOPY { len: u32 },
}

impl<T: Config> Token<T> for EvmOpcodeCosts {
	fn weight(&self) -> Weight {
		use EvmOpcodeCosts::*;

		let weight_of = |token: EvmOpcodeCosts| Token::<T>::weight(&token);

		match self {
			JUMP => cost_args!(evm_jump_opcode, 1).saturating_sub(weight_of(JUMPDEST)),
			JUMPI => cost_args!(evm_jumpi_opcode, 1).saturating_sub(weight_of(JUMPDEST)),
			JUMPDEST => cost_args!(evm_jumpdest_opcode, 1),
			PUSH => cost_args!(evm_push_opcode, 1),
			POP => cost_args!(evm_pop_opcode, 1),
			DUP => cost_args!(evm_dup_opcode, 1),
			SWAP => cost_args!(evm_swap_opcode, 1),
			PC => cost_args!(evm_pc_opcode, 1),
			CHAINID => cost_args!(evm_chainid_opcode, 1),
			PREVRANDAO => cost_args!(evm_prevrandao_opcode, 1),
			CODESIZE => cost_args!(evm_codesize_opcode, 1),
			CALLDATALOAD => cost_args!(evm_calldataload_opcode, 1),
			CALLDATASIZE => cost_args!(evm_calldatasize_opcode, 1),
			RETURNDATASIZE => cost_args!(evm_returndatasize_opcode, 1),
			ADD => cost_args!(evm_add_opcode, 1),
			MUL => cost_args!(evm_mul_opcode, 1),
			SUB => cost_args!(evm_sub_opcode, 1),
			DIV => cost_args!(evm_div_opcode, 1).saturating_sub(weight_of(POP)),
			SDIV => cost_args!(evm_sdiv_opcode, 1).saturating_sub(weight_of(POP)),
			MOD => cost_args!(evm_mod_opcode, 1).saturating_sub(weight_of(POP)),
			SMOD => cost_args!(evm_smod_opcode, 1).saturating_sub(weight_of(POP)),
			ADDMOD => cost_args!(evm_addmod_opcode, 1).saturating_sub(weight_of(POP)),
			MULMOD => cost_args!(evm_mulmod_opcode, 1).saturating_sub(weight_of(POP)),
			EXP { exponent_bits: 0 } => cost_args!(evm_exp_zero_opcode, 1),
			EXP { exponent_bits } => cost_args!(evm_exp_opcode, 1)
				.saturating_add(cost_args!(evm_exp_per_bit, exponent_bits.saturating_sub(1))),
			SIGNEXTEND => cost_args!(evm_signextend_opcode, 1).saturating_sub(weight_of(POP)),
			LT => cost_args!(evm_lt_opcode, 1),
			GT => cost_args!(evm_gt_opcode, 1),
			CLZ => cost_args!(evm_clz_opcode, 1),
			SLT => cost_args!(evm_slt_opcode, 1),
			SGT => cost_args!(evm_sgt_opcode, 1),
			EQ => cost_args!(evm_eq_opcode, 1),
			ISZERO => cost_args!(evm_iszero_opcode, 1).saturating_sub(weight_of(POP)),
			AND => cost_args!(evm_and_opcode, 1),
			OR => cost_args!(evm_or_opcode, 1),
			XOR => cost_args!(evm_xor_opcode, 1),
			NOT => cost_args!(evm_not_opcode, 1),
			BYTE => cost_args!(evm_byte_opcode, 1),
			SHL => cost_args!(evm_shl_opcode, 1).saturating_sub(weight_of(POP)),
			SHR => cost_args!(evm_shr_opcode, 1).saturating_sub(weight_of(POP)),
			SAR => cost_args!(evm_sar_opcode, 1).saturating_sub(weight_of(POP)),
			MLOAD => cost_args!(evm_mload_opcode, 1),
			MSTORE => cost_args!(evm_mstore_opcode, 1),
			MSTORE8 => cost_args!(evm_mstore8_opcode, 1),
			MSIZE => cost_args!(evm_msize_opcode, 1),
			MCOPY { len } => {
				// The fixed cost includes copying 64 bytes; shorter copies pay no variable cost.
				cost_args!(evm_mcopy_opcode, 1)
					.saturating_add(cost_args!(evm_mcopy_per_byte, len.saturating_sub(64)))
			},
		}
	}
}

impl<T: Config> ContractBlob<T> {
	/// Create a new contract from EVM init code.
	pub fn from_evm_init_code(code: Vec<u8>, owner: AccountIdOf<T>) -> Result<Self, DispatchError> {
		if code.len() > revm::primitives::eip3860::MAX_INITCODE_SIZE &&
			!DebugSettings::is_unlimited_contract_size_allowed::<T>()
		{
			return Err(<Error<T>>::BlobTooLarge.into());
		}

		let code_len = code.len() as u32;
		let code_info = CodeInfo {
			owner,
			deposit: Default::default(),
			refcount: 0,
			code_len,
			code_type: BytecodeType::Evm,
			behaviour_version: Default::default(),
		};

		Bytecode::new_raw_checked(Bytes::from(code.to_vec())).map_err(|err| {
			log::debug!(target: LOG_TARGET, "failed to create evm bytecode from init code: {err:?}" );
			<Error<T>>::CodeRejected
		})?;

		// Code hash is not relevant for init code, since it is not stored on-chain.
		let code_hash = H256::default();
		Ok(ContractBlob { code, code_info, code_hash })
	}

	/// Create a new contract from EVM runtime code.
	pub fn from_evm_runtime_code(
		code: Vec<u8>,
		owner: AccountIdOf<T>,
	) -> Result<Self, DispatchError> {
		let code_len = code.len() as u32;
		let deposit = super::calculate_code_deposit::<T>(code_len);
		Self::from_evm_runtime_code_with_deposit(code, owner, deposit)
	}

	/// Create a new contract from EVM runtime code with an explicit owner and
	/// deposit amount.
	///
	/// Used for `Origin::Root` uploads: there is no origin account to attribute
	/// the deposit to, so the caller passes the pallet's own account as a
	/// sentinel owner (no user can sign as it, so the code can't be removed via
	/// the owner-gated path) and a zero deposit (both `charge_deposit` and
	/// `refund_deposit` short-circuit at amount 0).
	pub fn from_evm_runtime_code_with_deposit(
		code: Vec<u8>,
		owner: AccountIdOf<T>,
		deposit: BalanceOf<T>,
	) -> Result<Self, DispatchError> {
		if code.len() > revm::primitives::eip170::MAX_CODE_SIZE &&
			!DebugSettings::is_unlimited_contract_size_allowed::<T>()
		{
			return Err(<Error<T>>::BlobTooLarge.into());
		}

		// EIP-3541: reject new contract code (runtime code) starting with the 0xEF byte.
		// Reserved for EIP-7702 delegation indicators; clashing here would let a
		// constructor return bytes that subsequent calls would misinterpret as a
		// delegation pointer.
		if code.first() == Some(&0xEF) {
			return Err(<Error<T>>::CodeRejected.into());
		}

		let code_len = code.len() as u32;

		let code_info = CodeInfo {
			owner,
			deposit,
			refcount: 0,
			code_len,
			code_type: BytecodeType::Evm,
			behaviour_version: Default::default(),
		};

		Bytecode::new_raw_checked(Bytes::from(code.to_vec())).map_err(|err| {
			log::debug!(target: LOG_TARGET, "failed to create evm bytecode from code: {err:?}" );
			<Error<T>>::CodeRejected
		})?;

		let code_hash = H256(sp_io::hashing::keccak_256(&code));
		Ok(ContractBlob { code, code_info, code_hash })
	}
}

/// Calls the EVM interpreter with the provided bytecode and inputs.
pub fn call<E: Ext>(bytecode: Bytecode, ext: &mut E, input: Vec<u8>) -> ExecResult {
	let mut interpreter = Interpreter::new(ExtBytecode::new(bytecode), input, ext);
	let tracing_enabled = tracing::if_tracing(|t| t.is_execution_tracer()).unwrap_or(false);

	let ControlFlow::Break(halt) = if tracing_enabled {
		run_plain_with_tracing(&mut interpreter)
	} else {
		run_plain(&mut interpreter)
	};
	halt.into()
}

pub(crate) fn run_plain<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt, Infallible> {
	loop {
		let opcode = interpreter.bytecode.opcode();
		interpreter.bytecode.relative_jump(1);
		exec_instruction(interpreter, opcode)?;
	}
}

fn run_plain_with_tracing<E: Ext>(
	interpreter: &mut Interpreter<E>,
) -> ControlFlow<Halt, Infallible> {
	loop {
		let opcode = interpreter.bytecode.opcode();
		tracing::if_tracing(|tracer| {
			let pc = interpreter.bytecode.pc() as u64;
			tracer.enter_opcode(pc, opcode, interpreter)
		});

		interpreter.bytecode.relative_jump(1);
		let res = exec_instruction(interpreter, opcode);

		tracing::if_tracing(|tracer| tracer.exit_step(interpreter, None));

		res?;
	}
}
