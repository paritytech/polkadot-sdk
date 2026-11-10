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

//! Types to build an environment that can be used to test and benchmark host function /
//! pre-compiles.

#![cfg(any(feature = "runtime-benchmarks", test))]
// A lot of this code is only used in benchmarking but we also want to use the code for testing
// pre-compiles eventually. For that we will probably export some of those types from the crate.
// Until then we simply ignore the warnings that arise when compiling tests without runtime
// benchmarks.
#![cfg_attr(test, allow(dead_code))]

#[cfg(all(test, feature = "runtime-benchmarks"))]
mod tests;

use crate::{
	AccountInfo, BalanceOf, BalanceWithDust, Code, CodeInfoOf, Config, ContractBlob, ContractInfo,
	Error, ExecConfig, ExecOrigin as Origin, OriginFor, Pallet as Contracts, PristineCode, Weight,
	address::AddressMapper,
	exec::{ExportedFunction, Key, PrecompileExt, Stack},
	limits,
	metering::{TransactionLimits, TransactionMeter},
	transient_storage::MeterEntry,
	vm::pvm::{PreparedCall, Runtime},
};
use alloc::{vec, vec::Vec};
use frame_support::{storage::child, traits::fungible::Mutate};
use frame_system::RawOrigin;
use pallet_revive_fixtures::bench as bench_fixtures;
use sp_core::{H160, H256, U256};
use sp_io::hashing::keccak_256;
use sp_runtime::traits::{Bounded, Hash};

type StackExt<'a, T> = Stack<'a, T, ContractBlob<T>>;

/// A builder used to prepare a contract call.
pub struct CallSetup<T: Config> {
	contract: Contract<T>,
	dest: T::AccountId,
	origin: Origin<T>,
	transaction_meter: TransactionMeter<T>,
	value: BalanceOf<T>,
	data: Vec<u8>,
	transient_storage_size: u32,
	exec_config: ExecConfig<T>,
	read_only: bool,
	delegate_call: bool,
}

impl<T> Default for CallSetup<T>
where
	T: Config,
{
	fn default() -> Self {
		Self::new(VmBinaryModule::dummy())
	}
}

impl<T> CallSetup<T>
where
	T: Config,
{
	/// Setup a new call for the given module.
	pub fn new(module: VmBinaryModule) -> Self {
		let contract = Contract::<T>::new(module, vec![]).unwrap();
		let dest = contract.account_id.clone();
		let origin = Origin::from_account_id(contract.caller.clone());

		// The call that creates the frame already pays for the info.
		#[cfg(feature = "runtime-benchmarks")]
		frame_benchmarking::benchmarking::add_to_whitelist(
			crate::AccountInfoOf::<T>::hashed_key_for(&T::AddressMapper::to_address(
				&contract.account_id,
			))
			.into(),
		);

		Self {
			contract,
			dest,
			origin,
			transaction_meter: TransactionMeter::new(TransactionLimits::WeightAndDeposit {
				weight_limit: Weight::MAX,
				deposit_limit: default_deposit_limit::<T>(),
			})
			.unwrap(),
			value: 0u32.into(),
			data: vec![],
			transient_storage_size: 0,
			exec_config: ExecConfig::new_substrate_tx(),
			read_only: false,
			delegate_call: false,
		}
	}

	/// Set the meter's storage deposit limit.
	pub fn set_storage_deposit_limit(&mut self, balance: BalanceOf<T>) {
		self.transaction_meter = TransactionMeter::new(TransactionLimits::WeightAndDeposit {
			weight_limit: Weight::MAX,
			deposit_limit: balance,
		})
		.unwrap();
	}

	/// Set the call's origin.
	pub fn set_origin(&mut self, origin: Origin<T>) {
		self.origin = origin;
	}

	/// Set the contract's balance.
	pub fn set_balance(&mut self, value: impl Into<BalanceWithDust<BalanceOf<T>>>) {
		self.contract.set_balance(value);
	}

	/// Set the call's input data.
	pub fn set_data(&mut self, value: Vec<u8>) {
		self.data = value;
	}

	/// Set the transient storage size.
	pub fn set_transient_storage_size(&mut self, size: u32) {
		self.transient_storage_size = size;
	}

	/// Set the read-only flag on the call stack frame.
	pub fn set_read_only(&mut self, read_only: bool) {
		self.read_only = read_only;
	}

	/// Mark the call as a delegate call.
	pub fn set_delegate_call(&mut self, delegate: bool) {
		self.delegate_call = delegate;
	}

	/// Get the call's input data.
	pub fn data(&self) -> Vec<u8> {
		self.data.clone()
	}

	/// Get the call's contract.
	pub fn contract(&self) -> Contract<T> {
		self.contract.clone()
	}

	/// Build the call stack.
	pub fn ext(&mut self) -> (StackExt<'_, T>, ContractBlob<T>) {
		let mut ext = StackExt::bench_new_call(
			T::AddressMapper::to_address(&self.dest),
			self.origin.clone(),
			&mut self.transaction_meter,
			self.value,
			&self.exec_config,
			self.read_only,
			self.delegate_call,
		);
		if self.transient_storage_size > 0 {
			Self::with_transient_storage(&mut ext.0, self.transient_storage_size).unwrap();
		}
		ext
	}

	/// Prepare a call to the module.
	pub fn prepare_call<'a>(
		ext: &'a mut StackExt<'a, T>,
		module: ContractBlob<T>,
		input: Vec<u8>,
	) -> PreparedCall<'a, StackExt<'a, T>> {
		module.prepare_call(Runtime::new(ext, input), ExportedFunction::Call).unwrap()
	}

	/// Add transient_storage
	fn with_transient_storage(ext: &mut StackExt<T>, size: u32) -> Result<(), &'static str> {
		let &MeterEntry { amount, limit } = ext.transient_storage().meter().current();
		ext.transient_storage().meter().current_mut().limit = size;
		for i in 1u32.. {
			let mut key_data = i.to_le_bytes().to_vec();
			while key_data.last() == Some(&0) {
				key_data.pop();
			}
			let key = Key::try_from_var(key_data).unwrap();
			if let Err(e) = ext.set_transient_storage(&key, Some(Vec::new()), false) {
				// Restore previous settings.
				ext.transient_storage().meter().current_mut().limit = limit;
				ext.transient_storage().meter().current_mut().amount = amount;
				if e == Error::<T>::OutOfTransientStorage.into() {
					break;
				} else {
					return Err("Initialization of the transient storage failed");
				}
			}
		}
		Ok(())
	}
}

/// The deposit limit we use for benchmarks.
pub fn default_deposit_limit<T: Config>() -> BalanceOf<T> {
	<BalanceOf<T>>::max_value()
}

/// The funding that each account that either calls or instantiates contracts is funded with.
pub fn caller_funding<T: Config>() -> BalanceOf<T> {
	// Minting can overflow, so we can't abuse of the funding. This value happens to be big enough,
	// but not too big to make the total supply overflow.
	BalanceOf::<T>::max_value() / 10_000u32.into()
}

/// An instantiated and deployed contract.
#[derive(Clone)]
pub struct Contract<T: Config> {
	pub caller: T::AccountId,
	pub account_id: T::AccountId,
	pub address: H160,
}

impl<T> Contract<T>
where
	T: Config,
{
	/// Create new contract and use a default account id as instantiator.
	pub fn new(module: VmBinaryModule, data: Vec<u8>) -> Result<Contract<T>, &'static str> {
		let caller = T::AddressMapper::to_fallback_account_id(&crate::test_utils::ALICE_ADDR);
		Self::with_caller(caller, module, data)
	}

	/// Create new contract and use an account id derived from the supplied index as instantiator.
	#[cfg(feature = "runtime-benchmarks")]
	pub fn with_index(
		index: u32,
		module: VmBinaryModule,
		data: Vec<u8>,
	) -> Result<Contract<T>, &'static str> {
		Self::with_caller(frame_benchmarking::account("instantiator", index, 0), module, data)
	}

	/// Create new contract and use the supplied `caller` as instantiator.
	pub fn with_caller(
		caller: T::AccountId,
		module: VmBinaryModule,
		data: Vec<u8>,
	) -> Result<Contract<T>, &'static str> {
		T::Currency::set_balance(&caller, caller_funding::<T>());
		let salt = Some([0xffu8; 32]);
		let origin: OriginFor<T> = RawOrigin::Signed(caller.clone()).into();

		// We ignore the error since we might also pass an already mapped account here.
		Contracts::<T>::map_account(origin.clone()).ok();

		#[cfg(feature = "runtime-benchmarks")]
		frame_benchmarking::benchmarking::add_to_whitelist(
			frame_system::Account::<T>::hashed_key_for(&caller).into(),
		);

		let outcome = Contracts::<T>::bare_instantiate(
			origin,
			U256::zero(),
			TransactionLimits::WeightAndDeposit {
				weight_limit: Weight::MAX,
				deposit_limit: default_deposit_limit::<T>(),
			},
			Code::Upload(module.code),
			data,
			salt,
			&ExecConfig::new_substrate_tx(),
		);

		let address = outcome.result?.addr;
		let account_id = T::AddressMapper::to_fallback_account_id(&address);
		let result = Contract { caller, address, account_id };

		AccountInfo::<T>::insert_contract(&address, result.info()?);
		Ok(result)
	}

	/// Create a new contract with the supplied storage item count and size each.
	pub fn with_storage(
		code: VmBinaryModule,
		stor_num: u32,
		stor_size: u32,
	) -> Result<Self, &'static str> {
		let contract = Contract::<T>::new(code, vec![])?;
		let storage_items = (0..stor_num)
			.map(|i| {
				let hash = T::Hashing::hash_of(&i)
					.as_ref()
					.try_into()
					.map_err(|_| "Hash too big for storage key")?;
				Ok((hash, vec![42u8; stor_size as usize]))
			})
			.collect::<Result<Vec<_>, &'static str>>()?;
		contract.store(&storage_items)?;
		Ok(contract)
	}

	/// Store the supplied storage items into this contracts storage.
	pub fn store(&self, items: &Vec<([u8; 32], Vec<u8>)>) -> Result<(), &'static str> {
		let info = self.info()?;
		for item in items {
			info.write(&Key::Fix(item.0), Some(item.1.clone()), None, false)
				.map_err(|_| "Failed to write storage to restoration dest")?;
		}

		AccountInfo::<T>::insert_contract(&self.address, info);
		Ok(())
	}

	/// Create a new contract with the specified unbalanced storage trie.
	pub fn with_unbalanced_storage_trie(
		code: VmBinaryModule,
		key: &[u8],
	) -> Result<Self, &'static str> {
		/// Number of layers in a Radix16 unbalanced trie.
		const UNBALANCED_TRIE_LAYERS: u32 = 20;

		if (key.len() as u32) < UNBALANCED_TRIE_LAYERS.div_ceil(2) {
			return Err("Key size too small to create the specified trie");
		}

		let value = vec![16u8; limits::STORAGE_BYTES as usize];
		let contract = Contract::<T>::new(code, vec![])?;
		let info = contract.info()?;
		let child_trie_info = info.child_trie_info();
		child::put_raw(&child_trie_info, &key, &value);
		for l in 0..UNBALANCED_TRIE_LAYERS {
			let pos = l as usize / 2;
			let mut key_new = key.to_vec();
			for i in 0u8..16 {
				key_new[pos] = if l % 2 == 0 {
					(key_new[pos] & 0xF0) | i
				} else {
					(key_new[pos] & 0x0F) | (i << 4)
				};

				if key == &key_new {
					continue;
				}
				child::put_raw(&child_trie_info, &key_new, &value);
			}
		}
		Ok(contract)
	}

	/// Get the `ContractInfo` of the `addr` or an error if it no longer exists.
	pub fn address_info(addr: &T::AccountId) -> Result<ContractInfo<T>, &'static str> {
		<AccountInfo<T>>::load_contract(&T::AddressMapper::to_address(addr))
			.ok_or("Expected contract to exist at this point.")
	}

	/// Get the `ContractInfo` of this contract or an error if it no longer exists.
	pub fn info(&self) -> Result<ContractInfo<T>, &'static str> {
		Self::address_info(&self.account_id)
	}

	/// Set the balance of the contract to the supplied amount.
	pub fn set_balance(&self, value: impl Into<BalanceWithDust<BalanceOf<T>>>) {
		let (value, dust) = value.into().deconstruct();
		T::Currency::set_balance(&self.account_id, value);
		crate::AccountInfoOf::<T>::mutate(&self.address, |account| {
			account.as_mut().map(|a| a.dust = dust);
		});
	}

	/// Returns `true` iff all storage entries related to code storage exist.
	pub fn code_exists(hash: &sp_core::H256) -> bool {
		<PristineCode<T>>::contains_key(hash) && <CodeInfoOf<T>>::contains_key(&hash)
	}

	/// Returns `true` iff no storage entry related to code storage exist.
	pub fn code_removed(hash: &sp_core::H256) -> bool {
		!<PristineCode<T>>::contains_key(hash) && !<CodeInfoOf<T>>::contains_key(&hash)
	}
}

/// A vm binary module ready to be put on chain.
#[derive(Clone)]
pub struct VmBinaryModule {
	pub code: Vec<u8>,
	pub hash: H256,
}

impl VmBinaryModule {
	/// Return a contract code that does nothing.
	pub fn dummy() -> Self {
		Self::new(bench_fixtures::dummy().to_vec())
	}

	fn new(code: Vec<u8>) -> Self {
		let hash = keccak_256(&code);
		Self { code, hash: H256(hash) }
	}
}

#[cfg(any(test, feature = "runtime-benchmarks"))]
impl VmBinaryModule {
	// Creates EVM init code that deploys `size` bytes of runtime code (all STOP opcodes).
	// EVM memory is zero-initialized, so RETURN(0, size) produces `size` bytes of 0x00.
	// The runtime code is what gets stored in PristineCode and loaded on every call.
	pub fn evm_init_code_for_runtime_size(size: u32) -> Self {
		use revm::bytecode::opcode::{PUSH1, PUSH3, RETURN};
		assert!(size <= 0x00FF_FFFF, "size {size} exceeds PUSH3 max (16MiB - 1)");
		let [_, b1, b2, b3] = size.to_be_bytes();
		let code = vec![
			PUSH3, b1, b2, b3, // push runtime code size
			PUSH1, 0,      // push memory offset 0
			RETURN, // return `size` bytes from memory as runtime code
		];
		Self::new(code)
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl VmBinaryModule {
	/// Same as [`Self::dummy`] but uses `replace_with` to make the code unique.
	pub fn dummy_unique(replace_with: u32) -> Self {
		Self::new(bench_fixtures::dummy_unique(replace_with))
	}

	/// Same as as `with_num_instructions` but based on the blob size.
	///
	/// This is needed when we weigh a blob without knowing how much instructions it
	/// contains.
	pub fn sized(size: u32) -> Self {
		// Due to variable length encoding of instructions this is not precise. But we only
		// need rough numbers for our benchmarks.
		Self::with_num_instructions(size / 3)
	}

	/// A contract code of specified number of instructions that uses all its bytes for instructions
	/// but will return immediately.
	///
	/// All the basic blocks are maximum sized (only the first is important though). This is to
	/// account for the fact that the interpreter will compile one basic block at a time even
	/// when no code is executed. Hence this contract will trigger the compilation of a maximum
	/// sized basic block and then return with its first instruction.
	///
	/// All the code will be put into the "call" export. Hence this code can be safely used for the
	/// `instantiate_with_code` benchmark where no compilation of any block should be measured.
	pub fn with_num_instructions(num_instructions: u32) -> Self {
		use alloc::{fmt::Write, string::ToString};
		let mut text = "
		pub @deploy:
		ret
		pub @call:
		"
		.to_string();
		for i in 0..num_instructions {
			match i {
				// return execution right away without breaking up basic block
				// SENTINEL is a hard coded syscall that terminates execution
				0 => writeln!(text, "ecalli {}", crate::SENTINEL).unwrap(),
				i if i % (limits::code::BASIC_BLOCK_SIZE - 1) == 0 => {
					text.push_str("fallthrough\n")
				},
				_ => text.push_str("a0 = a1 + a2\n"),
			}
		}
		text.push_str("ret\n");
		Self::from_assembly(&text)
	}

	/// The number of branches in [`Self::with_branch_blocks`].
	///
	/// All blocks the contract enters must fit into the interpreter cache at the same time, or
	/// the interpreter would reset the cache and compile blocks again. The cache holds
	/// `INTERPRETER_CACHE_BYTES / 24` entries and keeps room for one block of
	/// `BASIC_BLOCK_SIZE` instructions. A block of one instruction takes two entries: its gas
	/// charge and the instruction. 16000 such blocks and the final block of two instructions
	/// take 32003 entries, below the 42689 usable.
	///
	/// The cache entries are kept in vectors that double their capacity as they grow, up to
	/// the cache limit. 32003 entries stay below a capacity of 32768. Above it the vectors would
	/// be reallocated once more, which adds a copy of the whole cache to the largest values of
	/// `b` and skews the linear fit.
	pub const BRANCH_BLOCKS: u32 = 16_000;

	/// A contract whose `call` export enters `b` basic blocks of one instruction and returns.
	///
	/// The code holds [`Self::BRANCH_BLOCKS`] branches, one `ecalli SENTINEL` and one `ret`.
	/// Each branch ends a basic block, so each forms a block of its own, and it is always taken
	/// to the next block. The `ecalli SENTINEL` comes after `b` of them: the call compiles and
	/// runs `b` blocks of one branch each, then compiles the block of the `ecalli SENTINEL` and
	/// the instruction after it, and stops at the `ecalli SENTINEL`. The code holds the same
	/// instructions for every `b`, so its size does not depend on `b`.
	///
	/// The branch is a `branch_eq` of two registers. Of the instructions that end a basic block
	/// and continue at another one, branches took the longest to compile and run per block in a
	/// local measurement: 40 to 60% longer than a `fallthrough` and 25% longer than a `jump`, a
	/// `load_imm_and_jump` or a `load_imm_and_jump_indirect`. Branches with an immediate, taken
	/// or not, were within the noise of it.
	///
	/// `b` is within `0..=BRANCH_BLOCKS`.
	pub fn with_branch_blocks(b: u32) -> Self {
		let mut text = alloc::string::String::from("pub @deploy:\nret\npub @call:\n");
		for i in 0..Self::BRANCH_BLOCKS {
			if i == b {
				text.push_str(&alloc::format!("ecalli {}\n", crate::SENTINEL));
			}
			text.push_str(&alloc::format!("jump @next{i} if a0 == a0\n@next{i}:\n"));
		}
		if b == Self::BRANCH_BLOCKS {
			text.push_str(&alloc::format!("ecalli {}\n", crate::SENTINEL));
		}
		text.push_str("ret\n");
		Self::from_assembly(&text)
	}

	/// A contract whose `call` export compiles an entry block of `n` instructions and returns.
	///
	/// The entry block starts with an `ecalli SENTINEL`, which ends the execution but not the
	/// basic block, so the interpreter compiles the whole block before running it. `n - 2`
	/// fillers and a `fallthrough` follow. The remaining `BASIC_BLOCK_SIZE - n` fillers and a
	/// `ret` form a second block, which is never entered. The code holds the same instructions
	/// for every `n`, so its size does not depend on `n`.
	///
	/// The filler is a `store_imm_indirect_u64` with two 4 byte immediates. It never executes.
	/// Of the instructions that do not end a basic block, the `store_imm_indirect` kinds took
	/// the longest to compile per instruction in a local measurement of arithmetic with
	/// registers and immediates, 32 and 64 bit immediate loads, register moves, and loads and
	/// stores with absolute and indirect addresses. The others took up to 13% less. With short
	/// immediates it is still among the slowest, so the difference is not only the size of the
	/// instruction.
	///
	/// `n` is within `2..=BASIC_BLOCK_SIZE`.
	pub fn with_entry_block(n: u32) -> Self {
		const FILLER: &str = "u64 [a0 + 0x12345678] = 0x12345678\n";
		let mut text =
			alloc::format!("pub @deploy:\nret\npub @call:\necalli {}\n", crate::SENTINEL);
		for _ in 2..n {
			text.push_str(FILLER);
		}
		text.push_str("fallthrough\n");
		for _ in n..limits::code::BASIC_BLOCK_SIZE {
			text.push_str(FILLER);
		}
		text.push_str("ret\n");
		Self::from_assembly(&text)
	}

	/// The number of slots in the chain of addresses of [`Self::with_load_chain`].
	///
	/// With slots [`Self::LOAD_CHAIN_SLOT_BYTES`] apart, the chain stays in the L2 but not in the
	/// L1 data cache of the reference hardware: Intel Ice Lake (48 KiB L1D of 12 ways, 1.25 MiB
	/// L2 of 20 ways) and AMD Zen 3 (32 KiB L1D of 8 ways, 512 KiB L2 of 8 ways), all with 64
	/// byte lines, 64 L1D sets and 1024 L2 sets. A load touches one line, or two in the
	/// benchmark variant where every load spans two lines. With slots two lines apart, the 1024
	/// lines (64 KiB) of the other variants map to every other set and the 2048 lines (128 KiB)
	/// of the spanning variant to every set. Either way each L1D set in use receives 32 lines
	/// against at most 12 ways, and each L2 set 2 lines, a quarter of the Zen 3 ways. The L2 is
	/// physically indexed, so 2 lines per set is an average that depends on where the 32 4 KiB
	/// pages of the chain land. Every load also takes a DTLB lookup; the 32 pages fit the 64 entry
	/// L1 DTLB of both.
	pub const LOAD_CHAIN_SLOTS: u32 = 1024;

	/// The distance between two slots of [`Self::with_load_chain`] in bytes.
	///
	/// Two 64 byte cache lines; see [`Self::LOAD_CHAIN_SLOTS`].
	pub const LOAD_CHAIN_SLOT_BYTES: u32 = 128;

	/// A contract whose `call` export runs `a1` iterations of `LOAD_CHAIN_GAS - 3` dependent
	/// loads.
	///
	/// An iteration is the branch `jump @done if t0 == a1`, then one basic block of loads
	/// `a0 = u64 [a0]`, `t0 = t0 + 1` and `jump @loop`. Each load reads the address of the next
	/// one, so it cannot start before the previous load has finished. The contract declares
	/// [`Self::LOAD_CHAIN_SLOTS`] slots of [`Self::LOAD_CHAIN_SLOT_BYTES`] as read-write data.
	/// The caller writes the chain of addresses into it and sets `a0` to the first address.
	pub fn with_load_chain() -> Self {
		let mut text = alloc::format!(
			"%rw_data_size = {}\npub @deploy:\nret\npub @call:\n@loop:\njump @done if t0 == a1\n",
			Self::LOAD_CHAIN_SLOTS * Self::LOAD_CHAIN_SLOT_BYTES,
		);
		for _ in 3..crate::metering::LOAD_CHAIN_GAS {
			text.push_str("a0 = u64 [a0]\n");
		}
		text.push_str("t0 = t0 + 1\njump @loop\n@done:\nret\n");
		Self::from_assembly(&text)
	}

	/// A contract whose `call` export runs `a1` laps of a random cycle of indirect jumps.
	///
	/// The cycle has `INDIRECT_JUMP_GAS - 3` basic blocks of one instruction each. Every block
	/// is a `load_imm_and_jump_indirect` that jumps to the address in `a0` and loads the address
	/// of the block after the next one into `a0`. A lap starts and ends at a block with the
	/// branch `jump @done if t0 == a1`, followed by `t0 = t0 + 1` and the jump into the cycle.
	/// The order of the blocks in the cycle is a fixed random permutation, so consecutive
	/// jumps go to unrelated code addresses.
	///
	/// Before the first lap, a warm-up loop enters every cycle block once in code order, with
	/// `a0` holding the loop's own address so that the block jumps back to it. The interpreter
	/// compiles a block when execution first enters it and appends its handlers to its cache,
	/// so the cache holds the cycle blocks in code order. A lap visits them in the random order
	/// of the cycle, so its reads of handlers and arguments are as scattered as its reads of the
	/// jump table and the code map. The warm-up runs once per call whatever `a1` is.
	///
	/// The number of blocks keeps the interpreter data a lap reads in the L2 but not in the L1
	/// data cache of the reference hardware (see [`Self::LOAD_CHAIN_SLOTS`]). A jump into a
	/// block reads the block's 2 byte jump table entry, its 4 byte entry in the code map, and
	/// the 8 byte handler and 16 byte arguments of its two compiled instructions: the gas charge
	/// and the jump. The code map has one entry per code byte and a block is 5 bytes of code, so
	/// the entries of consecutive blocks are 20 bytes apart and a lap reads every cache line of
	/// the map: 20 bytes per block. Every block thus adds 2 + 20 + 2 * 24 = 70 bytes to the data
	/// of a lap. The code itself is read only when a block is compiled. The
	/// `INDIRECT_JUMP_GAS - 3 = 3000` blocks take 205 KiB, 4.3 times the Ice Lake L1D: each of
	/// the 64 L1D sets receives about 52 lines against at most 12 ways. Each of the 1024 L2 sets
	/// receives 3.2 lines, 40% of the Zen 3 ways. The L2 is physically indexed, so 3.2 lines per
	/// set is an average that depends on where the pages land. The data spans four arrays of
	/// 4 KiB pages, jump table 2 to 3, code map 15 to 16, handlers 12 to 13 and arguments 24 to
	/// 25, so 53 to 57 pages plus the interpreter's own state. They fit the fully associative 64
	/// entry L1 DTLB of Zen 3. The 64 entry L1 DTLB of Ice Lake is 4-way set associative, so some
	/// of them may miss it and hit the second level TLB.
	pub fn with_indirect_jump_cycle() -> Self {
		use polkavm_common::{
			abi::VM_CODE_ADDRESS_ALIGNMENT,
			program::{Instruction, Reg},
			writer::ProgramBlobBuilder,
		};
		use rand::{SeedableRng, seq::SliceRandom};

		// Basic block indices. The cycle starts at `CYCLE`.
		const DEPLOY: u32 = 0;
		const ENTRY: u32 = 1;
		const WARM_UP: u32 = 2;
		const WARM_UP_DONE: u32 = 4;
		const HEAD: u32 = 5;
		const DONE: u32 = 7;
		const CYCLE: u32 = 8;

		let blocks = crate::metering::INDIRECT_JUMP_GAS - 3;
		// Jump table entry 0 is `HEAD`, entry `1 + i` is cycle block `i` and entry `1 + blocks`
		// is `WARM_UP`. A jump table entry `k` has the code address
		// `(k + 1) * VM_CODE_ADDRESS_ALIGNMENT`.
		let address = |entry: u32| {
			i32::try_from((entry + 1) * VM_CODE_ADDRESS_ALIGNMENT)
				.expect("the jump table has fewer than 2^30 entries; qed")
		};
		let head = address(0);
		let block = |i: u32| address(1 + i);
		let warm_up = address(1 + blocks);
		// The distance between the addresses of consecutive jump table entries.
		let step = block(0) - head;

		let mut order: Vec<u32> = (0..blocks).collect();
		order.shuffle(&mut rand_pcg::Pcg64::seed_from_u64(1337));
		// The lap visits `HEAD`, then the cycle blocks in `order`. Each block loads the address
		// of the block two steps ahead.
		let lap: Vec<i32> = core::iter::once(head).chain(order.iter().map(|&i| block(i))).collect();
		let two_ahead = |position: usize| lap[(position + 2) % lap.len()];

		let a0 = Reg::A0.into();
		let a1 = Reg::A1.into();
		let t0 = Reg::T0.into();
		let t1 = Reg::T1.into();
		let ra = Reg::RA.into();
		let mut code = vec![
			Instruction::jump_indirect(ra, 0),
			// `t1` walks the addresses of the cycle blocks in code order. Each block is entered
			// with `a0` holding the address of `WARM_UP`, so it jumps back.
			Instruction::load_imm(t1, head),
			Instruction::fallthrough,
			Instruction::add_imm_64(t1, t1, step),
			Instruction::branch_eq_imm(t1, warm_up, WARM_UP_DONE),
			Instruction::load_imm_and_jump_indirect(a0, t1, warm_up, 0),
			Instruction::load_imm(a0, lap[1]),
			Instruction::fallthrough,
			Instruction::branch_eq(t0, a1, DONE),
			Instruction::add_imm_64(t0, t0, 1),
			Instruction::load_imm_and_jump_indirect(a0, a0, two_ahead(0), 0),
			Instruction::jump_indirect(ra, 0),
		];
		// Cycle block `i` sits at lap position `1 + k` where `order[k] == i`.
		let mut cycle: Vec<(u32, Instruction)> = order
			.iter()
			.enumerate()
			.map(|(k, &i)| {
				(i, Instruction::load_imm_and_jump_indirect(a0, a0, two_ahead(1 + k), 0))
			})
			.collect();
		cycle.sort_by_key(|(i, _)| *i);
		code.extend(cycle.into_iter().map(|(_, instruction)| instruction));
		let jump_table: Vec<u32> = core::iter::once(HEAD)
			.chain((0..blocks).map(|i| CYCLE + i))
			.chain(core::iter::once(WARM_UP))
			.collect();

		let mut builder =
			ProgramBlobBuilder::new(polkavm_common::program::InstructionSetKind::ReviveV1);
		builder.set_code(&code, &jump_table);
		builder.add_export_by_basic_block(DEPLOY, b"deploy");
		builder.add_export_by_basic_block(ENTRY, b"call");
		Self::new(builder.into_vec().expect("the program built here is valid; qed"))
	}

	/// The largest code section [`Self::with_code_bytes`] builds.
	///
	/// The interpreter keeps a map with one entry per byte of the code section. It counts
	/// towards [`limits::code::PURGABLE_MEMORY_LIMIT`] next to the interpreter cache, which a
	/// contract of this size fills. That allows at most 512 KiB of code. One page is left as a
	/// margin for the code that [`Self::with_code_bytes`] adds.
	pub const MAX_CODE_BYTES: u32 = (limits::code::PURGABLE_MEMORY_LIMIT -
		limits::code::INTERPRETER_CACHE_BYTES) /
		polkavm_common::program::INTERPRETER_FLATMAP_ENTRY_SIZE -
		limits::PAGE_SIZE;

	/// A contract with a code section of about `code_bytes` bytes whose `call` export returns
	/// at once.
	///
	/// The `call` export is a basic block of `ecalli SENTINEL` and `fallthrough`, which stops
	/// the call. Blocks of `a0 = a1 + a2`, 3 bytes each, fill the rest of the code section and
	/// never run. Only the size of the code section depends on `code_bytes`.
	pub fn with_code_bytes(code_bytes: u32) -> Self {
		let mut text = alloc::format!(
			"pub @deploy:\nret\npub @call:\necalli {}\nfallthrough\n",
			crate::SENTINEL
		);
		for i in 0..code_bytes / 3 {
			if i % limits::code::BASIC_BLOCK_SIZE == limits::code::BASIC_BLOCK_SIZE - 1 {
				text.push_str("fallthrough\n");
			} else {
				text.push_str("a0 = a1 + a2\n");
			}
		}
		text.push_str("ret\n");
		Self::from_assembly(&text)
	}

	/// Assembles `text` into a contract for the instruction set contracts use.
	fn from_assembly(text: &str) -> Self {
		let code = polkavm_common::assembler::assemble(
			Some(polkavm_common::program::InstructionSetKind::ReviveV1),
			text,
		)
		.expect("callers pass assembly text built in this module, which is valid; qed");
		Self::new(code)
	}

	/// A contract code that calls the "noop" host function in a loop depending in the input.
	pub fn noop() -> Self {
		Self::new(bench_fixtures::noop().to_vec())
	}

	/// An evm contract that executes `size` JUMPDEST instructions.
	pub fn evm_noop(size: u32) -> Self {
		use revm::bytecode::opcode::JUMPDEST;

		let code = vec![JUMPDEST; size as usize];
		Self::new(code)
	}
}
