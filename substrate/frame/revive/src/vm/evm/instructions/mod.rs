//! EVM opcode implementations.

#[macro_use]
pub mod macros;
/// Arithmetic operations (ADD, SUB, MUL, DIV, etc.).
pub mod arithmetic;
/// Bitwise operations (AND, OR, XOR, NOT, etc.).
pub mod bitwise;
/// Block information instructions (COINBASE, TIMESTAMP, etc.).
pub mod block_info;
/// Contract operations (CALL, CREATE, DELEGATECALL, etc.).
pub mod contract;
/// Control flow instructions (JUMP, JUMPI, REVERT, etc.).
pub mod control;
/// Host environment interactions (SLOAD, SSTORE, LOG, etc.).
pub mod host;
/// Signed 256-bit integer operations.
pub mod i256;
/// Memory operations (MLOAD, MSTORE, MSIZE, etc.).
pub mod memory;
/// Stack operations (PUSH, POP, DUP, SWAP, etc.).
pub mod stack;
/// System information instructions (ADDRESS, CALLER, etc.).
pub mod system;
/// Transaction information instructions (ORIGIN, GASPRICE, etc.).
pub mod tx_info;
/// Utility functions and helpers for instruction implementation.
pub mod utility;

use crate::vm::{
	evm::{DummyHost, EVMInterpreter},
	Ext,
};
use revm::interpreter::Instruction;

/// Returns the instruction table for the given spec.
pub const fn instruction_table<'a, E: Ext>() -> [Instruction<EVMInterpreter<'a, E>, DummyHost>; 256]
{
	use revm::bytecode::opcode::*;
	let mut table = [control::unknown as Instruction<EVMInterpreter<'a, E>, DummyHost>; 256];

	table[STOP as usize] = control::stop;
	table[ADD as usize] = arithmetic::add;
	table[MUL as usize] = arithmetic::mul;
	table[SUB as usize] = arithmetic::sub;
	table[DIV as usize] = arithmetic::div;
	table[SDIV as usize] = arithmetic::sdiv;
	table[MOD as usize] = arithmetic::rem;
	table[SMOD as usize] = arithmetic::smod;
	table[ADDMOD as usize] = arithmetic::addmod;
	table[MULMOD as usize] = arithmetic::mulmod;
	table[EXP as usize] = arithmetic::exp;
	table[SIGNEXTEND as usize] = arithmetic::signextend;

	table[LT as usize] = bitwise::lt;
	table[GT as usize] = bitwise::gt;
	table[SLT as usize] = bitwise::slt;
	table[SGT as usize] = bitwise::sgt;
	table[EQ as usize] = bitwise::eq;
	table[ISZERO as usize] = bitwise::iszero;
	table[AND as usize] = bitwise::bitand;
	table[OR as usize] = bitwise::bitor;
	table[XOR as usize] = bitwise::bitxor;
	table[NOT as usize] = bitwise::not;
	table[BYTE as usize] = bitwise::byte;
	table[SHL as usize] = bitwise::shl;
	table[SHR as usize] = bitwise::shr;
	table[SAR as usize] = bitwise::sar;
	table[CLZ as usize] = bitwise::clz;

	table[KECCAK256 as usize] = system::keccak256;

	table[ADDRESS as usize] = system::address;
	table[BALANCE as usize] = host::balance;
	table[ORIGIN as usize] = tx_info::origin;
	table[CALLER as usize] = system::caller;
	table[CALLVALUE as usize] = system::callvalue;
	table[CALLDATALOAD as usize] = system::calldataload;
	table[CALLDATASIZE as usize] = system::calldatasize;
	table[CALLDATACOPY as usize] = system::calldatacopy;
	table[CODESIZE as usize] = system::codesize;
	table[CODECOPY as usize] = system::codecopy;

	table[GASPRICE as usize] = tx_info::gasprice;
	table[EXTCODESIZE as usize] = host::extcodesize;
	table[EXTCODECOPY as usize] = host::extcodecopy;
	table[RETURNDATASIZE as usize] = system::returndatasize;
	table[RETURNDATACOPY as usize] = system::returndatacopy;
	table[EXTCODEHASH as usize] = host::extcodehash;
	table[BLOCKHASH as usize] = host::blockhash;
	table[COINBASE as usize] = block_info::coinbase;
	table[TIMESTAMP as usize] = block_info::timestamp;
	table[NUMBER as usize] = block_info::block_number;
	table[DIFFICULTY as usize] = block_info::difficulty;
	table[GASLIMIT as usize] = block_info::gaslimit;
	table[CHAINID as usize] = block_info::chainid;
	table[SELFBALANCE as usize] = host::selfbalance;
	table[BASEFEE as usize] = block_info::basefee;
	table[BLOBHASH as usize] = tx_info::blob_hash;
	table[BLOBBASEFEE as usize] = block_info::blob_basefee;

	table[POP as usize] = stack::pop;
	table[MLOAD as usize] = memory::mload;
	table[MSTORE as usize] = memory::mstore;
	table[MSTORE8 as usize] = memory::mstore8;
	table[SLOAD as usize] = host::sload;
	table[SSTORE as usize] = host::sstore;
	table[JUMP as usize] = control::jump;
	table[JUMPI as usize] = control::jumpi;
	table[PC as usize] = control::pc;
	table[MSIZE as usize] = memory::msize;
	table[GAS as usize] = system::gas;
	table[JUMPDEST as usize] = control::jumpdest;
	table[TLOAD as usize] = host::tload;
	table[TSTORE as usize] = host::tstore;
	table[MCOPY as usize] = memory::mcopy;

	table[PUSH0 as usize] = stack::push0;
	table[PUSH1 as usize] = stack::push::<1, _, _>;
	table[PUSH2 as usize] = stack::push::<2, _, _>;
	table[PUSH3 as usize] = stack::push::<3, _, _>;
	table[PUSH4 as usize] = stack::push::<4, _, _>;
	table[PUSH5 as usize] = stack::push::<5, _, _>;
	table[PUSH6 as usize] = stack::push::<6, _, _>;
	table[PUSH7 as usize] = stack::push::<7, _, _>;
	table[PUSH8 as usize] = stack::push::<8, _, _>;
	table[PUSH9 as usize] = stack::push::<9, _, _>;
	table[PUSH10 as usize] = stack::push::<10, _, _>;
	table[PUSH11 as usize] = stack::push::<11, _, _>;
	table[PUSH12 as usize] = stack::push::<12, _, _>;
	table[PUSH13 as usize] = stack::push::<13, _, _>;
	table[PUSH14 as usize] = stack::push::<14, _, _>;
	table[PUSH15 as usize] = stack::push::<15, _, _>;
	table[PUSH16 as usize] = stack::push::<16, _, _>;
	table[PUSH17 as usize] = stack::push::<17, _, _>;
	table[PUSH18 as usize] = stack::push::<18, _, _>;
	table[PUSH19 as usize] = stack::push::<19, _, _>;
	table[PUSH20 as usize] = stack::push::<20, _, _>;
	table[PUSH21 as usize] = stack::push::<21, _, _>;
	table[PUSH22 as usize] = stack::push::<22, _, _>;
	table[PUSH23 as usize] = stack::push::<23, _, _>;
	table[PUSH24 as usize] = stack::push::<24, _, _>;
	table[PUSH25 as usize] = stack::push::<25, _, _>;
	table[PUSH26 as usize] = stack::push::<26, _, _>;
	table[PUSH27 as usize] = stack::push::<27, _, _>;
	table[PUSH28 as usize] = stack::push::<28, _, _>;
	table[PUSH29 as usize] = stack::push::<29, _, _>;
	table[PUSH30 as usize] = stack::push::<30, _, _>;
	table[PUSH31 as usize] = stack::push::<31, _, _>;
	table[PUSH32 as usize] = stack::push::<32, _, _>;

	table[DUP1 as usize] = stack::dup::<1, _, _>;
	table[DUP2 as usize] = stack::dup::<2, _, _>;
	table[DUP3 as usize] = stack::dup::<3, _, _>;
	table[DUP4 as usize] = stack::dup::<4, _, _>;
	table[DUP5 as usize] = stack::dup::<5, _, _>;
	table[DUP6 as usize] = stack::dup::<6, _, _>;
	table[DUP7 as usize] = stack::dup::<7, _, _>;
	table[DUP8 as usize] = stack::dup::<8, _, _>;
	table[DUP9 as usize] = stack::dup::<9, _, _>;
	table[DUP10 as usize] = stack::dup::<10, _, _>;
	table[DUP11 as usize] = stack::dup::<11, _, _>;
	table[DUP12 as usize] = stack::dup::<12, _, _>;
	table[DUP13 as usize] = stack::dup::<13, _, _>;
	table[DUP14 as usize] = stack::dup::<14, _, _>;
	table[DUP15 as usize] = stack::dup::<15, _, _>;
	table[DUP16 as usize] = stack::dup::<16, _, _>;

	table[SWAP1 as usize] = stack::swap::<1, _, _>;
	table[SWAP2 as usize] = stack::swap::<2, _, _>;
	table[SWAP3 as usize] = stack::swap::<3, _, _>;
	table[SWAP4 as usize] = stack::swap::<4, _, _>;
	table[SWAP5 as usize] = stack::swap::<5, _, _>;
	table[SWAP6 as usize] = stack::swap::<6, _, _>;
	table[SWAP7 as usize] = stack::swap::<7, _, _>;
	table[SWAP8 as usize] = stack::swap::<8, _, _>;
	table[SWAP9 as usize] = stack::swap::<9, _, _>;
	table[SWAP10 as usize] = stack::swap::<10, _, _>;
	table[SWAP11 as usize] = stack::swap::<11, _, _>;
	table[SWAP12 as usize] = stack::swap::<12, _, _>;
	table[SWAP13 as usize] = stack::swap::<13, _, _>;
	table[SWAP14 as usize] = stack::swap::<14, _, _>;
	table[SWAP15 as usize] = stack::swap::<15, _, _>;
	table[SWAP16 as usize] = stack::swap::<16, _, _>;

	table[LOG0 as usize] = host::log::<0, _>;
	table[LOG1 as usize] = host::log::<1, _>;
	table[LOG2 as usize] = host::log::<2, _>;
	table[LOG3 as usize] = host::log::<3, _>;
	table[LOG4 as usize] = host::log::<4, _>;

	table[CREATE as usize] = contract::create::<_, false, _>;
	table[CALL as usize] = contract::call;
	table[CALLCODE as usize] = contract::call_code;
	table[RETURN as usize] = control::ret;
	table[DELEGATECALL as usize] = contract::delegate_call;
	table[CREATE2 as usize] = contract::create::<_, true, _>;

	table[STATICCALL as usize] = contract::static_call;
	table[REVERT as usize] = control::revert;
	table[INVALID as usize] = control::invalid;
	table[SELFDESTRUCT as usize] = host::selfdestruct;
	table
}

#[cfg(test)]
mod tests {
	use super::instruction_table;
	use revm::bytecode::opcode::*;

	#[test]
	fn all_instructions_and_opcodes_used() {
		// known unknown instruction we compare it with other instructions from table.
		let unknown_instruction = 0x0C_usize;

		use crate::{exec::Stack, tests::Test, ContractBlob};
		let instr_table = instruction_table::<'static, Stack<'static, Test, ContractBlob<Test>>>();

		let unknown_istr = instr_table[unknown_instruction];
		for (i, instr) in instr_table.iter().enumerate() {
			let is_opcode_unknown = OpCode::new(i as u8).is_none();
			//
			let is_instr_unknown = std::ptr::fn_addr_eq(*instr, unknown_istr);
			assert_eq!(is_instr_unknown, is_opcode_unknown, "Opcode 0x{i:X?} is not handled",);
		}
	}
}
