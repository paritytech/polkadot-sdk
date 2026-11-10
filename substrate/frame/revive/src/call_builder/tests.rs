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

use super::VmBinaryModule;
use crate::{
	SENTINEL,
	limits::code::{BASIC_BLOCK_SIZE, INTERPRETER_CACHE_BYTES, check_pvm_code},
	metering::{INDIRECT_JUMP_GAS, LOAD_CHAIN_GAS},
};
use polkavm_common::program::{
	INTERPRETER_CACHE_ENTRY_SIZE, ISA_ReviveV1, Instruction, ParsedInstruction, ProgramBlob,
};

/// The instructions of the `call` export, from its start to the end of the code.
fn call_instructions(module: &VmBinaryModule) -> Vec<ParsedInstruction> {
	let blob = ProgramBlob::parse(module.code[..].into()).unwrap();
	let call = blob.exports().find(|export| export.symbol().as_bytes() == b"call").unwrap();
	blob.instructions_bounded_at_with_isa(ISA_ReviveV1, call.program_counter())
		.collect()
}

/// The contracts of `interpreter_compile_per_instruction` have the same size, hold the same
/// opcodes and differ only in the size of the entry block.
#[test]
fn interpreter_compile_per_instruction_benchmark_contracts() {
	let sorted_opcodes = |instructions: &[ParsedInstruction]| {
		let mut opcodes: Vec<_> = instructions.iter().map(|inst| inst.kind.opcode()).collect();
		opcodes.sort_unstable();
		opcodes
	};
	let reference = VmBinaryModule::with_entry_block(2);
	let code_len = reference.code.len();
	let opcodes = sorted_opcodes(&call_instructions(&reference));
	for n in [2, 3, BASIC_BLOCK_SIZE / 2, BASIC_BLOCK_SIZE - 1, BASIC_BLOCK_SIZE] {
		let module = VmBinaryModule::with_entry_block(n);
		let instructions = call_instructions(&module);
		let entry_block_size = instructions
			.iter()
			.position(|inst| inst.kind.opcode().starts_new_basic_block())
			.unwrap() + 1;

		assert_eq!(module.code.len(), code_len, "n = {n}");
		assert_eq!(sorted_opcodes(&instructions), opcodes, "n = {n}");
		assert_eq!(instructions[0].kind, Instruction::ecalli(SENTINEL.cast_signed()), "n = {n}");
		assert_eq!(u32::try_from(entry_block_size).unwrap(), n, "n = {n}");
		assert!(
			instructions[1..entry_block_size - 1]
				.iter()
				.all(|inst| matches!(inst.kind, Instruction::store_imm_indirect_u64(..))),
			"n = {n}"
		);
		assert_eq!(check_pvm_code(&module.code), Ok(()), "n = {n}");
	}
}

/// The contracts of `interpreter_compile_per_block` have the same size and enter `b`
/// blocks of one instruction before they stop.
///
/// All blocks fit into the interpreter cache at once. The cache reserves room for one
/// block of `BASIC_BLOCK_SIZE` instructions and resets when it would grow into it. Every
/// compiled block takes one entry for its gas charge and one per instruction. The entries
/// also stay below 32768, see [`VmBinaryModule::BRANCH_BLOCKS`].
#[test]
fn interpreter_compile_per_block_benchmark_contracts() {
	let max = VmBinaryModule::BRANCH_BLOCKS;
	let code_len = VmBinaryModule::with_branch_blocks(0).code.len();
	for b in [0, 1, max / 2, max - 1, max] {
		let module = VmBinaryModule::with_branch_blocks(b);
		let instructions = call_instructions(&module);
		let sentinel = instructions
			.iter()
			.position(|inst| inst.kind == Instruction::ecalli(SENTINEL.cast_signed()))
			.unwrap();

		assert_eq!(module.code.len(), code_len, "b = {b}");
		assert_eq!(u32::try_from(sentinel).unwrap(), b, "b = {b}");
		for inst in &instructions[..sentinel] {
			let Instruction::branch_eq(s1, s2, target) = inst.kind else {
				panic!("b = {b}: {inst:?}")
			};
			assert!(s1 == s2 && target == inst.next_offset.0, "b = {b}: {inst:?}");
		}
		assert!(instructions[sentinel + 1].kind.opcode().starts_new_basic_block(), "b = {b}");
		assert_eq!(check_pvm_code(&module.code), Ok(()), "b = {b}");
	}

	let cache_entries = INTERPRETER_CACHE_BYTES / INTERPRETER_CACHE_ENTRY_SIZE;
	let usable_entries = cache_entries - (BASIC_BLOCK_SIZE + 1);
	// `max` blocks of one instruction and the block of `ecalli` and its terminator.
	let compiled_entries = 2 * max + 3;
	assert!(compiled_entries <= usable_entries, "{compiled_entries} > {usable_entries}");
	assert!(compiled_entries <= 32 * 1024, "{compiled_entries} > {}", 32 * 1024);
}

/// An iteration of the `instr_load_chain_<m>` contract costs `LOAD_CHAIN_GAS` and its loads
/// fill a maximum-sized basic block.
#[test]
fn instr_load_chain_benchmark_contract() {
	let module = VmBinaryModule::with_load_chain();
	let instructions = call_instructions(&module);
	let ends: Vec<usize> = instructions
		.iter()
		.enumerate()
		.filter(|(_, inst)| inst.kind.opcode().starts_new_basic_block())
		.map(|(i, _)| i)
		.collect();
	let loads = instructions
		.iter()
		.filter(|inst| matches!(inst.kind, Instruction::load_indirect_u64(..)))
		.count();

	// The loop condition, the loop body, `ret`.
	assert_eq!(ends.len(), 3);
	assert_eq!(ends[0], 0);
	assert_eq!(u32::try_from(ends[1] - ends[0]).unwrap(), BASIC_BLOCK_SIZE);
	assert_eq!(u32::try_from(ends[1] + 1).unwrap(), LOAD_CHAIN_GAS);
	assert_eq!(u32::try_from(loads).unwrap(), LOAD_CHAIN_GAS - 3);
	assert_eq!(check_pvm_code(&module.code), Ok(()));
}

/// A lap of the `instr_indirect_jump` contract visits every block of the cycle once and
/// costs `INDIRECT_JUMP_GAS`.
///
/// The warm-up before the first lap enters every cycle block in code order, which is
/// unrelated to the order of the lap. All blocks fit into the interpreter cache, and the
/// blocks are as large as the footprint in the docs of `with_indirect_jump_cycle` assumes.
#[test]
fn instr_indirect_jump_benchmark_contract() {
	let module = VmBinaryModule::with_indirect_jump_cycle();
	assert_eq!(check_pvm_code(&module.code), Ok(()));

	let blob = ProgramBlob::parse(module.code[..].into()).unwrap();
	let jump_table = blob.jump_table();
	let kind = |pc| blob.instructions_bounded_at_with_isa(ISA_ReviveV1, pc).next().unwrap().kind;
	let blocks = INDIRECT_JUMP_GAS - 3;
	assert_eq!(jump_table.len(), blocks + 2);
	let head = jump_table.get_by_index(0).unwrap();
	let warm_up = jump_table.get_by_index(blocks + 1).unwrap();
	let entry = call_instructions(&module);

	// The warm-up adds `step` to `t1` and jumps to it until `t1` is the address of the
	// warm-up loop. It loads the loop's address into `link`, the register every cycle block
	// jumps to (checked below), so every block it jumps to returns to the loop.
	let Instruction::load_imm(t1, start) = entry[0].kind else { panic!("{:?}", entry[0]) };
	assert_eq!(kind(warm_up), entry[2].kind);
	let Instruction::add_imm_64(sum, addend, step) = entry[2].kind else {
		panic!("{:?}", entry[2])
	};
	let Instruction::branch_eq_imm(compared, end, exit) = entry[3].kind else {
		panic!("{:?}", entry[3])
	};
	let Instruction::load_imm_and_jump_indirect(link, base, back, 0) = entry[4].kind else {
		panic!("{:?}", entry[4])
	};
	assert!([sum, addend, compared, base].iter().all(|&reg| reg == t1), "{:?}", &entry[..5]);
	assert_ne!(link, t1);
	assert_eq!(exit, entry[5].offset.0);
	assert_eq!(jump_table.get_by_address(back.cast_unsigned()), Some(warm_up));
	assert_eq!(jump_table.get_by_address(end.cast_unsigned()), Some(warm_up));
	let warm_up_steps = i32::try_from(blocks).unwrap();
	assert_eq!(start + (warm_up_steps + 1) * step, end);
	let compile_order: Vec<_> = (1..=warm_up_steps)
		.map(|k| jump_table.get_by_address((start + k * step).cast_unsigned()).unwrap())
		.collect();

	// Follow the jumps from the head into the cycle and back. The end of the warm-up, the
	// increment and every cycle block load and jump through `link`.
	let Instruction::load_imm(lap_reg, first) = entry[5].kind else { panic!("{:?}", entry[5]) };
	assert_eq!(lap_reg, link);
	let Instruction::load_imm_and_jump_indirect(r, b, mut next, 0) = entry[9].kind else {
		panic!("{:?}", entry[9])
	};
	assert!(r == link && b == link, "{:?}", entry[9]);
	assert!(matches!(entry[7].kind, Instruction::branch_eq(..)));
	assert_eq!(kind(head), entry[7].kind);
	let mut target = jump_table.get_by_address(first.cast_unsigned()).unwrap();
	let mut lap_order = Vec::new();
	while target != head {
		assert!(lap_order.len() < compile_order.len(), "the lap does not return to the head");
		lap_order.push(target);
		let Instruction::load_imm_and_jump_indirect(r, b, value, 0) = kind(target) else {
			panic!("{:?}", kind(target))
		};
		assert!(r == link && b == link, "block {target}: {:?}", kind(target));
		target = jump_table.get_by_address(next.cast_unsigned()).unwrap();
		next = value;
	}
	// The jump into the cycle and one jump out of every block.
	let jumps = 1 + lap_order.len();
	// The branch, the increment and the jumps.
	assert_eq!(2 + u32::try_from(jumps).unwrap(), INDIRECT_JUMP_GAS);

	// The warm-up compiles each block the lap visits exactly once, in code order. The lap
	// rarely jumps to the block compiled right after the previous one.
	let mut sorted = lap_order.clone();
	sorted.sort();
	assert_eq!(compile_order, sorted);
	let compiled_at = |pc| compile_order.binary_search(&pc).unwrap();
	let in_compile_order = lap_order
		.windows(2)
		.filter(|w| compiled_at(w[1]) == compiled_at(w[0]) + 1)
		.count();
	assert!(100 * in_compile_order < lap_order.len(), "{in_compile_order}");
	// Between 4 and 5 bytes of code per cycle block, and 2 byte jump table entries.
	let code_len = u32::try_from(blob.code().len()).unwrap();
	assert!(code_len >= 4 * blocks, "{code_len} < {}", 4 * blocks);
	assert!(code_len <= 5 * blocks, "{code_len} > {}", 5 * blocks);
	assert!(code_len <= u32::from(u16::MAX), "{code_len}");

	// Every block takes a gas charge and one entry per instruction. The cycle blocks take 2
	// each, the other blocks of `call` 18: the entry 3, the warm-up loop 5, the end of the
	// warm-up 3, the head 2, the increment 3 and the exit 2.
	let cache_entries = INTERPRETER_CACHE_BYTES / INTERPRETER_CACHE_ENTRY_SIZE;
	let usable_entries = cache_entries - (BASIC_BLOCK_SIZE + 1);
	let compiled_entries = 2 * blocks + 18;
	assert!(compiled_entries <= usable_entries, "{compiled_entries} > {usable_entries}");
}

/// The `interpreter_code_map_per_byte` contracts have a code section of about the requested
/// size and the same entry block.
#[test]
fn interpreter_code_map_per_byte_benchmark_contracts() {
	for code_bytes in [0, 3000, VmBinaryModule::MAX_CODE_BYTES / 2, VmBinaryModule::MAX_CODE_BYTES]
	{
		let module = VmBinaryModule::with_code_bytes(code_bytes);
		let blob = ProgramBlob::parse(module.code[..].into()).unwrap();
		let code_len = u32::try_from(blob.code().len()).unwrap();
		let instructions = call_instructions(&module);

		assert!(code_len.abs_diff(code_bytes) <= code_bytes / 100 + 16, "{code_len}");
		assert_eq!(instructions[0].kind, Instruction::ecalli(SENTINEL.cast_signed()));
		assert_eq!(instructions[1].kind, Instruction::fallthrough);
		assert_eq!(check_pvm_code(&module.code), Ok(()), "code_bytes = {code_bytes}");
	}
}
