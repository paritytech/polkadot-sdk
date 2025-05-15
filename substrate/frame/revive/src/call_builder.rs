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

use crate::{
	address::AddressMapper,
	exec::{ExportedFunction, Key, PrecompileExt, Stack},
	limits,
	storage::meter::Meter,
	transient_storage::MeterEntry,
	wasm::{PreparedCall, Runtime},
	BalanceOf, Code, CodeInfoOf, Config, ContractInfo, ContractInfoOf, DepositLimit, Error,
	GasMeter, MomentOf, NonceAlreadyIncremented, Origin, Pallet as Contracts, PristineCode,
	WasmBlob, Weight,
};
use alloc::{vec, vec::Vec};
use frame_support::{storage::child, traits::fungible::Mutate};
use frame_system::RawOrigin;
use pallet_revive_fixtures::bench as bench_fixtures;
use sp_core::{Get, H160, H256, U256};
use sp_io::hashing::keccak_256;
use sp_runtime::traits::{Bounded, Hash};

type StackExt<'a, T> = Stack<'a, T, WasmBlob<T>>;

/// A builder used to prepare a contract call.
pub struct CallSetup<T: Config> {
	contract: Contract<T>,
	dest: T::AccountId,
	origin: Origin<T>,
	gas_meter: GasMeter<T>,
	storage_meter: Meter<T>,
	value: BalanceOf<T>,
	data: Vec<u8>,
	transient_storage_size: u32,
}

impl<T> Default for CallSetup<T>
where
	T: Config,
	BalanceOf<T>: Into<U256> + TryFrom<U256>,
	MomentOf<T>: Into<U256>,
	T::Hash: frame_support::traits::IsType<H256>,
{
	fn default() -> Self {
		Self::new(WasmModule::dummy())
	}
}

impl<T> CallSetup<T>
where
	T: Config,
	BalanceOf<T>: Into<U256> + TryFrom<U256>,
	MomentOf<T>: Into<U256>,
	T::Hash: frame_support::traits::IsType<H256>,
{
	/// Setup a new call for the given module.
	pub fn new(module: WasmModule) -> Self {
		let contract = Contract::<T>::new(module, vec![]).unwrap();
		let dest = contract.account_id.clone();
		let origin = Origin::from_account_id(contract.caller.clone());

		let storage_meter = Meter::new(&origin, default_deposit_limit::<T>(), 0u32.into()).unwrap();

		#[cfg(feature = "runtime-benchmarks")]
		{
			// Whitelist contract account, as it is already accounted for in the call benchmark
			frame_benchmarking::benchmarking::add_to_whitelist(
				frame_system::Account::<T>::hashed_key_for(&contract.account_id).into(),
			);

			// Whitelist the contract's contractInfo as it is already accounted for in the call
			// benchmark
			frame_benchmarking::benchmarking::add_to_whitelist(
				crate::ContractInfoOf::<T>::hashed_key_for(&T::AddressMapper::to_address(
					&contract.account_id,
				))
				.into(),
			);
		}

		Self {
			contract,
			dest,
			origin,
			gas_meter: GasMeter::new(Weight::MAX),
			storage_meter,
			value: 0u32.into(),
			data: vec![],
			transient_storage_size: 0,
		}
	}

	/// Set the meter's storage deposit limit.
	pub fn set_storage_deposit_limit(&mut self, balance: BalanceOf<T>) {
		self.storage_meter = Meter::new(&self.origin, balance, 0u32.into()).unwrap();
	}

	/// Set the call's origin.
	pub fn set_origin(&mut self, origin: Origin<T>) {
		self.origin = origin;
	}

	/// Set the contract's balance.
	pub fn set_balance(&mut self, value: BalanceOf<T>) {
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

	/// Get the call's input data.
	pub fn data(&self) -> Vec<u8> {
		self.data.clone()
	}

	/// Get the call's contract.
	pub fn contract(&self) -> Contract<T> {
		self.contract.clone()
	}

	/// Build the call stack.
	pub fn ext(&mut self) -> (StackExt<'_, T>, WasmBlob<T>) {
		let mut ext = StackExt::bench_new_call(
			T::AddressMapper::to_address(&self.dest),
			self.origin.clone(),
			&mut self.gas_meter,
			&mut self.storage_meter,
			self.value,
		);
		if self.transient_storage_size > 0 {
			Self::with_transient_storage(&mut ext.0, self.transient_storage_size).unwrap();
		}
		ext
	}

	/// Prepare a call to the module.
	pub fn prepare_call<'a>(
		ext: &'a mut StackExt<'a, T>,
		module: WasmBlob<T>,
		input: Vec<u8>,
		aux_data_size: u32,
	) -> PreparedCall<'a, StackExt<'a, T>> {
		module
			.prepare_call(Runtime::new(ext, input), ExportedFunction::Call, aux_data_size)
			.unwrap()
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
	(T::DepositPerByte::get() * 1024u32.into() * 1024u32.into()) +
		T::DepositPerItem::get() * 1024u32.into()
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
	BalanceOf<T>: Into<U256> + TryFrom<U256>,
	MomentOf<T>: Into<U256>,
	T::Hash: frame_support::traits::IsType<H256>,
{
	/// Create new contract and use a default account id as instantiator.
	pub fn new(module: WasmModule, data: Vec<u8>) -> Result<Contract<T>, &'static str> {
		let caller = T::AddressMapper::to_fallback_account_id(&crate::test_utils::ALICE_ADDR);
		Self::with_caller(caller, module, data)
	}

	/// Create new contract and use an account id derived from the supplied index as instantiator.
	#[cfg(feature = "runtime-benchmarks")]
	pub fn with_index(
		index: u32,
		module: WasmModule,
		data: Vec<u8>,
	) -> Result<Contract<T>, &'static str> {
		Self::with_caller(frame_benchmarking::account("instantiator", index, 0), module, data)
	}

	/// Create new contract and use the supplied `caller` as instantiator.
	pub fn with_caller(
		caller: T::AccountId,
		module: WasmModule,
		data: Vec<u8>,
	) -> Result<Contract<T>, &'static str> {
		T::Currency::set_balance(&caller, caller_funding::<T>());
		let salt = Some([0xffu8; 32]);
		let origin: T::RuntimeOrigin = RawOrigin::Signed(caller.clone()).into();

		// We ignore the error since we might also pass an already mapped account here.
		Contracts::<T>::map_account(origin.clone()).ok();

		#[cfg(feature = "runtime-benchmarks")]
		frame_benchmarking::benchmarking::add_to_whitelist(
			frame_system::Account::<T>::hashed_key_for(&caller).into(),
		);

		let outcome = Contracts::<T>::bare_instantiate(
			origin,
			0u32.into(),
			Weight::MAX,
			DepositLimit::Balance(default_deposit_limit::<T>()),
			Code::Upload(module.code),
			data,
			salt,
			NonceAlreadyIncremented::No,
		);

		let address = outcome.result?.addr;
		let account_id = T::AddressMapper::to_fallback_account_id(&address);
		let result = Contract { caller, address, account_id };

		ContractInfoOf::<T>::insert(&address, result.info()?);

		Ok(result)
	}

	/// Create a new contract with the supplied storage item count and size each.
	pub fn with_storage(
		code: WasmModule,
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
		<ContractInfoOf<T>>::insert(&self.address, info);
		Ok(())
	}

	/// Create a new contract with the specified unbalanced storage trie.
	pub fn with_unbalanced_storage_trie(
		code: WasmModule,
		key: &[u8],
	) -> Result<Self, &'static str> {
		/// Number of layers in a Radix16 unbalanced trie.
		const UNBALANCED_TRIE_LAYERS: u32 = 20;

		if (key.len() as u32) < (UNBALANCED_TRIE_LAYERS + 1) / 2 {
			return Err("Key size too small to create the specified trie");
		}

		let value = vec![16u8; limits::PAYLOAD_BYTES as usize];
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
		ContractInfoOf::<T>::get(T::AddressMapper::to_address(addr))
			.ok_or("Expected contract to exist at this point.")
	}

	/// Get the `ContractInfo` of this contract or an error if it no longer exists.
	pub fn info(&self) -> Result<ContractInfo<T>, &'static str> {
		Self::address_info(&self.account_id)
	}

	/// Set the balance of the contract to the supplied amount.
	pub fn set_balance(&self, balance: BalanceOf<T>) {
		T::Currency::set_balance(&self.account_id, balance);
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

/// A wasm module ready to be put on chain.
#[derive(Clone)]
pub struct WasmModule {
	pub code: Vec<u8>,
	pub hash: H256,
}

impl WasmModule {
	/// Return a contract code that does nothing.
	pub fn dummy() -> Self {
		Self::new(bench_fixtures::DUMMY.to_vec())
	}

	fn new(code: Vec<u8>) -> Self {
		let hash = keccak_256(&code);
		Self { code, hash: H256(hash) }
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl WasmModule {
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
				i if i % (limits::code::BASIC_BLOCK_SIZE - 1) == 0 =>
					text.push_str("fallthrough\n"),
				_ => text.push_str("a0 = a1 + a2\n"),
			}
		}
		text.push_str("ret\n");
		let code = polkavm_common::assembler::assemble(&text).unwrap();
		Self::new(code)
	}

	/// A contract code that calls the "noop" host function in a loop depending in the input.
	pub fn noop() -> Self {
		Self::new(bench_fixtures::NOOP.to_vec())
	}

	/// A contract code that does unaligned memory accessed in a loop.
	pub fn instr(do_load: bool) -> Self {
		let load = match do_load {
			false => "",
			true => "a0 = u64 [a0]",
		};
		let text = alloc::format!(
			"
		pub @deploy:
		ret
		pub @call:
			@loop:
				jump @done if t0 == a1
				{load}
				t0 = t0 + 1
				jump @loop
			@done:
		ret
		"
		);
		let code = polkavm_common::assembler::assemble(&text).unwrap();
		Self::new(code)
	}
}
