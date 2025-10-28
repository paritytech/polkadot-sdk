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

/// EVM opcode implementations.
use super::interpreter::Interpreter;
use crate::vm::{evm::Halt, Ext};
use revm::bytecode::opcode::*;

/// Arithmetic operations (ADD, SUB, MUL, DIV, etc.).
mod arithmetic;
/// Bitwise operations (AND, OR, XOR, NOT, etc.).
mod bitwise;
/// Block information instructions (COINBASE, TIMESTAMP, etc.).
mod block_info;
/// Contract operations (CALL, CREATE, DELEGATECALL, etc.).
mod contract;
/// Control flow instructions (JUMP, JUMPI, REVERT, etc.).
mod control;
/// Host environment interactions (SLOAD, SSTORE, LOG, etc.).
#[cfg(feature = "runtime-benchmarks")]
pub mod host;
#[cfg(not(feature = "runtime-benchmarks"))]
mod host;
/// Memory operations (MLOAD, MSTORE, MSIZE, etc.).
mod memory;
/// Stack operations (PUSH, POP, DUP, SWAP, etc.).
mod stack;
/// System information instructions (ADDRESS, CALLER, etc.).
mod system;
/// Transaction information instructions (ORIGIN, GASPRICE, etc.).
mod tx_info;
/// Utility functions and helpers for instruction implementation.
mod utility;

pub fn exec_instruction<E: Ext>(
	interpreter: &mut Interpreter<E>,
	opcode: u8,
) -> core::ops::ControlFlow<Halt> {
	match opcode {
		STOP => control::stop(interpreter),
		ADD => arithmetic::add(interpreter),
		MUL => arithmetic::mul(interpreter),
		SUB => arithmetic::sub(interpreter),
		DIV => arithmetic::div(interpreter),
		SDIV => arithmetic::sdiv(interpreter),
		MOD => arithmetic::rem(interpreter),
		SMOD => arithmetic::smod(interpreter),
		ADDMOD => arithmetic::addmod(interpreter),
		MULMOD => arithmetic::mulmod(interpreter),
		EXP => arithmetic::exp(interpreter),
		SIGNEXTEND => arithmetic::signextend(interpreter),

		LT => bitwise::lt(interpreter),
		GT => bitwise::gt(interpreter),
		SLT => bitwise::slt(interpreter),
		SGT => bitwise::sgt(interpreter),
		EQ => bitwise::eq(interpreter),
		ISZERO => bitwise::iszero(interpreter),
		AND => bitwise::bitand(interpreter),
		OR => bitwise::bitor(interpreter),
		XOR => bitwise::bitxor(interpreter),
		NOT => bitwise::not(interpreter),
		BYTE => bitwise::byte(interpreter),
		SHL => bitwise::shl(interpreter),
		SHR => bitwise::shr(interpreter),
		SAR => bitwise::sar(interpreter),
		CLZ => bitwise::clz(interpreter),

		KECCAK256 => system::keccak256(interpreter),

		ADDRESS => system::address(interpreter),
		BALANCE => host::balance(interpreter),
		ORIGIN => tx_info::origin(interpreter),
		CALLER => system::caller(interpreter),
		CALLVALUE => system::callvalue(interpreter),
		CALLDATALOAD => system::calldataload(interpreter),
		CALLDATASIZE => system::calldatasize(interpreter),
		CALLDATACOPY => system::calldatacopy(interpreter),
		CODESIZE => system::codesize(interpreter),
		CODECOPY => system::codecopy(interpreter),

		GASPRICE => tx_info::gasprice(interpreter),
		EXTCODESIZE => host::extcodesize(interpreter),
		EXTCODECOPY => host::extcodecopy(interpreter),
		RETURNDATASIZE => system::returndatasize(interpreter),
		RETURNDATACOPY => system::returndatacopy(interpreter),
		EXTCODEHASH => host::extcodehash(interpreter),
		BLOCKHASH => host::blockhash(interpreter),
		COINBASE => block_info::coinbase(interpreter),
		TIMESTAMP => block_info::timestamp(interpreter),
		NUMBER => block_info::block_number(interpreter),
		DIFFICULTY => block_info::difficulty(interpreter),
		GASLIMIT => block_info::gaslimit(interpreter),
		CHAINID => block_info::chainid(interpreter),
		SELFBALANCE => host::selfbalance(interpreter),
		BASEFEE => block_info::basefee(interpreter),
		BLOBHASH => tx_info::blob_hash(interpreter),
		BLOBBASEFEE => block_info::blob_basefee(interpreter),

		POP => stack::pop(interpreter),
		MLOAD => memory::mload(interpreter),
		MSTORE => memory::mstore(interpreter),
		MSTORE8 => memory::mstore8(interpreter),
		SLOAD => host::sload(interpreter),
		SSTORE => host::sstore(interpreter),
		JUMP => control::jump(interpreter),
		JUMPI => control::jumpi(interpreter),
		PC => control::pc(interpreter),
		MSIZE => memory::msize(interpreter),
		GAS => system::gas(interpreter),
		JUMPDEST => control::jumpdest(interpreter),
		TLOAD => host::tload(interpreter),
		TSTORE => host::tstore(interpreter),
		MCOPY => memory::mcopy(interpreter),

		PUSH0 => stack::push0(interpreter),
		PUSH1 => stack::push::<1, _>(interpreter),
		PUSH2 => stack::push::<2, _>(interpreter),
		PUSH3 => stack::push::<3, _>(interpreter),
		PUSH4 => stack::push::<4, _>(interpreter),
		PUSH5 => stack::push::<5, _>(interpreter),
		PUSH6 => stack::push::<6, _>(interpreter),
		PUSH7 => stack::push::<7, _>(interpreter),
		PUSH8 => stack::push::<8, _>(interpreter),
		PUSH9 => stack::push::<9, _>(interpreter),
		PUSH10 => stack::push::<10, _>(interpreter),
		PUSH11 => stack::push::<11, _>(interpreter),
		PUSH12 => stack::push::<12, _>(interpreter),
		PUSH13 => stack::push::<13, _>(interpreter),
		PUSH14 => stack::push::<14, _>(interpreter),
		PUSH15 => stack::push::<15, _>(interpreter),
		PUSH16 => stack::push::<16, _>(interpreter),
		PUSH17 => stack::push::<17, _>(interpreter),
		PUSH18 => stack::push::<18, _>(interpreter),
		PUSH19 => stack::push::<19, _>(interpreter),
		PUSH20 => stack::push::<20, _>(interpreter),
		PUSH21 => stack::push::<21, _>(interpreter),
		PUSH22 => stack::push::<22, _>(interpreter),
		PUSH23 => stack::push::<23, _>(interpreter),
		PUSH24 => stack::push::<24, _>(interpreter),
		PUSH25 => stack::push::<25, _>(interpreter),
		PUSH26 => stack::push::<26, _>(interpreter),
		PUSH27 => stack::push::<27, _>(interpreter),
		PUSH28 => stack::push::<28, _>(interpreter),
		PUSH29 => stack::push::<29, _>(interpreter),
		PUSH30 => stack::push::<30, _>(interpreter),
		PUSH31 => stack::push::<31, _>(interpreter),
		PUSH32 => stack::push::<32, _>(interpreter),

		DUP1 => stack::dup::<1, _>(interpreter),
		DUP2 => stack::dup::<2, _>(interpreter),
		DUP3 => stack::dup::<3, _>(interpreter),
		DUP4 => stack::dup::<4, _>(interpreter),
		DUP5 => stack::dup::<5, _>(interpreter),
		DUP6 => stack::dup::<6, _>(interpreter),
		DUP7 => stack::dup::<7, _>(interpreter),
		DUP8 => stack::dup::<8, _>(interpreter),
		DUP9 => stack::dup::<9, _>(interpreter),
		DUP10 => stack::dup::<10, _>(interpreter),
		DUP11 => stack::dup::<11, _>(interpreter),
		DUP12 => stack::dup::<12, _>(interpreter),
		DUP13 => stack::dup::<13, _>(interpreter),
		DUP14 => stack::dup::<14, _>(interpreter),
		DUP15 => stack::dup::<15, _>(interpreter),
		DUP16 => stack::dup::<16, _>(interpreter),

		SWAP1 => stack::swap::<1, _>(interpreter),
		SWAP2 => stack::swap::<2, _>(interpreter),
		SWAP3 => stack::swap::<3, _>(interpreter),
		SWAP4 => stack::swap::<4, _>(interpreter),
		SWAP5 => stack::swap::<5, _>(interpreter),
		SWAP6 => stack::swap::<6, _>(interpreter),
		SWAP7 => stack::swap::<7, _>(interpreter),
		SWAP8 => stack::swap::<8, _>(interpreter),
		SWAP9 => stack::swap::<9, _>(interpreter),
		SWAP10 => stack::swap::<10, _>(interpreter),
		SWAP11 => stack::swap::<11, _>(interpreter),
		SWAP12 => stack::swap::<12, _>(interpreter),
		SWAP13 => stack::swap::<13, _>(interpreter),
		SWAP14 => stack::swap::<14, _>(interpreter),
		SWAP15 => stack::swap::<15, _>(interpreter),
		SWAP16 => stack::swap::<16, _>(interpreter),

		LOG0 => host::log::<0, _>(interpreter),
		LOG1 => host::log::<1, _>(interpreter),
		LOG2 => host::log::<2, _>(interpreter),
		LOG3 => host::log::<3, _>(interpreter),
		LOG4 => host::log::<4, _>(interpreter),

		CREATE => contract::create::<false, _>(interpreter),
		CREATE2 => contract::create::<true, _>(interpreter),
		CALL => contract::call(interpreter),
		STATICCALL => contract::static_call(interpreter),
		DELEGATECALL => contract::delegate_call(interpreter),
		CALLCODE => contract::call_code(interpreter),

		RETURN => control::ret(interpreter),
		REVERT => control::revert(interpreter),
		SELFDESTRUCT => host::selfdestruct(interpreter),

		_ => control::invalid(interpreter),
	}
}
