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

//! Benchmarks for the revive pallet

#![cfg(feature = "runtime-benchmarks")]

mod call_builder;
mod code;
use self::{call_builder::CallSetup, code::WasmModule};
use crate::{
	evm::runtime::GAS_PRICE,
	exec::{Ext, Key, MomentOf},
	limits,
	pure_precompiles::Precompile,
	storage::WriteOutcome,
	ConversionPrecision, Pallet as Contracts, *,
};
use alloc::{vec, vec::Vec};
use codec::{Encode, MaxEncodedLen};
use frame_benchmarking::v2::*;
use frame_support::{
	self, assert_ok,
	storage::child,
	traits::fungible::InspectHold,
	weights::{Weight, WeightMeter},
};
use frame_system::RawOrigin;
use pallet_revive_uapi::{pack_hi_lo, CallFlags, ReturnErrorCode, StorageFlags};
use sp_consensus_aura::AURA_ENGINE_ID;
use sp_consensus_babe::{
	digests::{PreDigest, PrimaryPreDigest},
	BABE_ENGINE_ID,
};
use sp_consensus_slots::Slot;
use sp_runtime::{
	generic::{Digest, DigestItem},
	traits::{Bounded, Hash},
};

/// How many runs we do per API benchmark.
///
/// This is picked more or less arbitrary. We experimented with different numbers until
/// the results appeared to be stable. Reducing the number would speed up the benchmarks
/// but might make the results less precise.
const API_BENCHMARK_RUNS: u32 = 1600;

/// Number of layers in a Radix16 unbalanced trie.
const UNBALANCED_TRIE_LAYERS: u32 = 20;

/// An instantiated and deployed contract.
#[derive(Clone)]
struct Contract<T: Config> {
	caller: T::AccountId,
	account_id: T::AccountId,
	address: H160,
}

impl<T> Contract<T>
where
	T: Config,
	BalanceOf<T>: Into<U256> + TryFrom<U256>,
	MomentOf<T>: Into<U256>,
	T::Hash: frame_support::traits::IsType<H256>,
{
	/// Create new contract and use a default account id as instantiator.
	fn new(module: WasmModule, data: Vec<u8>) -> Result<Contract<T>, &'static str> {
		Self::with_index(0, module, data)
	}

	/// Create new contract and use an account id derived from the supplied index as instantiator.
	fn with_index(
		index: u32,
		module: WasmModule,
		data: Vec<u8>,
	) -> Result<Contract<T>, &'static str> {
		Self::with_caller(account("instantiator", index, 0), module, data)
	}

	/// Create new contract and use the supplied `caller` as instantiator.
	fn with_caller(
		caller: T::AccountId,
		module: WasmModule,
		data: Vec<u8>,
	) -> Result<Contract<T>, &'static str> {
		T::Currency::set_balance(&caller, caller_funding::<T>());
		let salt = Some([0xffu8; 32]);
		let origin: T::RuntimeOrigin = RawOrigin::Signed(caller.clone()).into();

		Contracts::<T>::map_account(origin.clone()).unwrap();

		let outcome = Contracts::<T>::bare_instantiate(
			origin,
			0u32.into(),
			Weight::MAX,
			DepositLimit::Balance(default_deposit_limit::<T>()),
			Code::Upload(module.code),
			data,
			salt,
		);

		let address = outcome.result?.addr;
		let account_id = T::AddressMapper::to_fallback_account_id(&address);
		let result = Contract { caller, address, account_id };

		ContractInfoOf::<T>::insert(&address, result.info()?);

		Ok(result)
	}

	/// Create a new contract with the supplied storage item count and size each.
	fn with_storage(code: WasmModule, stor_num: u32, stor_size: u32) -> Result<Self, &'static str> {
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
	fn store(&self, items: &Vec<([u8; 32], Vec<u8>)>) -> Result<(), &'static str> {
		let info = self.info()?;
		for item in items {
			info.write(&Key::Fix(item.0), Some(item.1.clone()), None, false)
				.map_err(|_| "Failed to write storage to restoration dest")?;
		}
		<ContractInfoOf<T>>::insert(&self.address, info);
		Ok(())
	}

	/// Create a new contract with the specified unbalanced storage trie.
	fn with_unbalanced_storage_trie(code: WasmModule, key: &[u8]) -> Result<Self, &'static str> {
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
	fn address_info(addr: &T::AccountId) -> Result<ContractInfo<T>, &'static str> {
		ContractInfoOf::<T>::get(T::AddressMapper::to_address(addr))
			.ok_or("Expected contract to exist at this point.")
	}

	/// Get the `ContractInfo` of this contract or an error if it no longer exists.
	fn info(&self) -> Result<ContractInfo<T>, &'static str> {
		Self::address_info(&self.account_id)
	}

	/// Set the balance of the contract to the supplied amount.
	fn set_balance(&self, balance: BalanceOf<T>) {
		T::Currency::set_balance(&self.account_id, balance);
	}

	/// Returns `true` iff all storage entries related to code storage exist.
	fn code_exists(hash: &sp_core::H256) -> bool {
		<PristineCode<T>>::contains_key(hash) && <CodeInfoOf<T>>::contains_key(&hash)
	}

	/// Returns `true` iff no storage entry related to code storage exist.
	fn code_removed(hash: &sp_core::H256) -> bool {
		!<PristineCode<T>>::contains_key(hash) && !<CodeInfoOf<T>>::contains_key(&hash)
	}
}

/// The funding that each account that either calls or instantiates contracts is funded with.
fn caller_funding<T: Config>() -> BalanceOf<T> {
	// Minting can overflow, so we can't abuse of the funding. This value happens to be big enough,
	// but not too big to make the total supply overflow.
	BalanceOf::<T>::max_value() / 10_000u32.into()
}

/// The deposit limit we use for benchmarks.
fn default_deposit_limit<T: Config>() -> BalanceOf<T> {
	(T::DepositPerByte::get() * 1024u32.into() * 1024u32.into()) +
		T::DepositPerItem::get() * 1024u32.into()
}

#[benchmarks(
	where
		BalanceOf<T>: Into<U256> + TryFrom<U256>,
		T: Config,
		MomentOf<T>: Into<U256>,
		<T as frame_system::Config>::RuntimeEvent: From<pallet::Event<T>>,
		<T as Config>::RuntimeCall: From<frame_system::Call<T>>,
		<T as frame_system::Config>::Hash: frame_support::traits::IsType<H256>,
)]
mod benchmarks {
	use super::*;

	// The base weight consumed on processing contracts deletion queue.
	#[benchmark(pov_mode = Measured)]
	fn on_process_deletion_queue_batch() {
		#[block]
		{
			ContractInfo::<T>::process_deletion_queue_batch(&mut WeightMeter::new())
		}
	}

	#[benchmark(skip_meta, pov_mode = Measured)]
	fn on_initialize_per_trie_key(k: Linear<0, 1024>) -> Result<(), BenchmarkError> {
		let instance = Contract::<T>::with_storage(WasmModule::dummy(), k, limits::PAYLOAD_BYTES)?;
		instance.info()?.queue_trie_for_deletion();

		#[block]
		{
			ContractInfo::<T>::process_deletion_queue_batch(&mut WeightMeter::new())
		}

		Ok(())
	}

	// This benchmarks the overhead of loading a code of size `c` byte from storage and into
	// the execution engine.
	//
	// `call_with_code_per_byte(c) - call_with_code_per_byte(0)`
	//
	// This does **not** include the actual execution for which the gas meter
	// is responsible. The code used here will just return on call.
	//
	// We expect the influence of `c` to be none in this benchmark because every instruction that
	// is not in the first basic block is never read. We are primarily interested in the
	// `proof_size` result of this benchmark.
	#[benchmark(pov_mode = Measured)]
	fn call_with_code_per_byte(
		c: Linear<0, { limits::code::STATIC_MEMORY_BYTES / limits::code::BYTES_PER_INSTRUCTION }>,
	) -> Result<(), BenchmarkError> {
		let instance =
			Contract::<T>::with_caller(whitelisted_caller(), WasmModule::sized(c), vec![])?;
		let value = Pallet::<T>::min_balance();
		let storage_deposit = default_deposit_limit::<T>();

		#[extrinsic_call]
		call(
			RawOrigin::Signed(instance.caller.clone()),
			instance.address,
			value,
			Weight::MAX,
			storage_deposit,
			vec![],
		);

		Ok(())
	}

	// Measure the amount of time it takes to compile a single basic block.
	//
	// (basic_block_compilation(1) - basic_block_compilation(0)).ref_time()
	//
	// This is needed because the interpreter will always compile a whole basic block at
	// a time. To prevent a contract from triggering compilation without doing any execution
	// we will always charge one max sized block per contract call.
	//
	// We ignore the proof size component when using this benchmark as this is already accounted
	// for in `call_with_code_per_byte`.
	#[benchmark(pov_mode = Measured)]
	fn basic_block_compilation(b: Linear<0, 1>) -> Result<(), BenchmarkError> {
		let instance = Contract::<T>::with_caller(
			whitelisted_caller(),
			WasmModule::with_num_instructions(limits::code::BASIC_BLOCK_SIZE),
			vec![],
		)?;
		let value = Pallet::<T>::min_balance();
		let storage_deposit = default_deposit_limit::<T>();

		#[block]
		{
			Pallet::<T>::call(
				RawOrigin::Signed(instance.caller.clone()).into(),
				instance.address,
				value,
				Weight::MAX,
				storage_deposit,
				vec![],
			)?;
		}

		Ok(())
	}

	// `c`: Size of the code in bytes.
	// `i`: Size of the input in bytes.
	#[benchmark(pov_mode = Measured)]
	fn instantiate_with_code(
		c: Linear<0, { limits::code::STATIC_MEMORY_BYTES / limits::code::BYTES_PER_INSTRUCTION }>,
		i: Linear<0, { limits::code::BLOB_BYTES }>,
	) {
		let input = vec![42u8; i as usize];
		let salt = [42u8; 32];
		let value = Pallet::<T>::min_balance();
		let caller = whitelisted_caller();
		T::Currency::set_balance(&caller, caller_funding::<T>());
		let WasmModule { code, .. } = WasmModule::sized(c);
		let origin = RawOrigin::Signed(caller.clone());
		Contracts::<T>::map_account(origin.clone().into()).unwrap();
		let deployer = T::AddressMapper::to_address(&caller);
		let addr = crate::address::create2(&deployer, &code, &input, &salt);
		let account_id = T::AddressMapper::to_fallback_account_id(&addr);
		let storage_deposit = default_deposit_limit::<T>();
		#[extrinsic_call]
		_(origin, value, Weight::MAX, storage_deposit, code, input, Some(salt));

		let deposit =
			T::Currency::balance_on_hold(&HoldReason::StorageDepositReserve.into(), &account_id);
		// uploading the code reserves some balance in the callers account
		let code_deposit =
			T::Currency::balance_on_hold(&HoldReason::CodeUploadDepositReserve.into(), &caller);
		let mapping_deposit =
			T::Currency::balance_on_hold(&HoldReason::AddressMapping.into(), &caller);
		assert_eq!(
			T::Currency::balance(&caller),
			caller_funding::<T>() -
				value - deposit -
				code_deposit - mapping_deposit -
				Pallet::<T>::min_balance(),
		);
		// contract has the full value
		assert_eq!(T::Currency::balance(&account_id), value + Pallet::<T>::min_balance());
	}

	// `i`: Size of the input in bytes.
	// `s`: Size of e salt in bytes.
	#[benchmark(pov_mode = Measured)]
	fn instantiate(i: Linear<0, { limits::code::BLOB_BYTES }>) -> Result<(), BenchmarkError> {
		let input = vec![42u8; i as usize];
		let salt = [42u8; 32];
		let value = Pallet::<T>::min_balance();
		let caller = whitelisted_caller();
		T::Currency::set_balance(&caller, caller_funding::<T>());
		let origin = RawOrigin::Signed(caller.clone());
		Contracts::<T>::map_account(origin.clone().into()).unwrap();
		let WasmModule { code, .. } = WasmModule::dummy();
		let storage_deposit = default_deposit_limit::<T>();
		let deployer = T::AddressMapper::to_address(&caller);
		let addr = crate::address::create2(&deployer, &code, &input, &salt);
		let hash = Contracts::<T>::bare_upload_code(origin.clone().into(), code, storage_deposit)?
			.code_hash;
		let account_id = T::AddressMapper::to_fallback_account_id(&addr);

		#[extrinsic_call]
		_(origin, value, Weight::MAX, storage_deposit, hash, input, Some(salt));

		let deposit =
			T::Currency::balance_on_hold(&HoldReason::StorageDepositReserve.into(), &account_id);
		let code_deposit =
			T::Currency::balance_on_hold(&HoldReason::CodeUploadDepositReserve.into(), &account_id);
		let mapping_deposit =
			T::Currency::balance_on_hold(&HoldReason::AddressMapping.into(), &account_id);
		// value was removed from the caller
		assert_eq!(
			T::Currency::total_balance(&caller),
			caller_funding::<T>() -
				value - deposit -
				code_deposit - mapping_deposit -
				Pallet::<T>::min_balance(),
		);
		// contract has the full value
		assert_eq!(T::Currency::balance(&account_id), value + Pallet::<T>::min_balance());

		Ok(())
	}

	// We just call a dummy contract to measure the overhead of the call extrinsic.
	// The size of the data has no influence on the costs of this extrinsic as long as the contract
	// won't call `seal_call_data_copy` in its constructor to copy the data to contract memory.
	// The dummy contract used here does not do this. The costs for the data copy is billed as
	// part of `seal_call_data_copy`. The costs for invoking a contract of a specific size are not
	// part of this benchmark because we cannot know the size of the contract when issuing a call
	// transaction. See `call_with_code_per_byte` for this.
	#[benchmark(pov_mode = Measured)]
	fn call() -> Result<(), BenchmarkError> {
		let data = vec![42u8; 1024];
		let instance =
			Contract::<T>::with_caller(whitelisted_caller(), WasmModule::dummy(), vec![])?;
		let value = Pallet::<T>::min_balance();
		let origin = RawOrigin::Signed(instance.caller.clone());
		let before = T::Currency::balance(&instance.account_id);
		let storage_deposit = default_deposit_limit::<T>();
		#[extrinsic_call]
		_(origin, instance.address, value, Weight::MAX, storage_deposit, data);
		let deposit = T::Currency::balance_on_hold(
			&HoldReason::StorageDepositReserve.into(),
			&instance.account_id,
		);
		let code_deposit = T::Currency::balance_on_hold(
			&HoldReason::CodeUploadDepositReserve.into(),
			&instance.caller,
		);
		let mapping_deposit =
			T::Currency::balance_on_hold(&HoldReason::AddressMapping.into(), &instance.caller);
		// value and value transferred via call should be removed from the caller
		assert_eq!(
			T::Currency::balance(&instance.caller),
			caller_funding::<T>() -
				value - deposit -
				code_deposit - mapping_deposit -
				Pallet::<T>::min_balance()
		);
		// contract should have received the value
		assert_eq!(T::Currency::balance(&instance.account_id), before + value);
		// contract should still exist
		instance.info()?;

		Ok(())
	}

	// This constructs a contract that is maximal expensive to instrument.
	// It creates a maximum number of metering blocks per byte.
	// `c`: Size of the code in bytes.
	#[benchmark(pov_mode = Measured)]
	fn upload_code(
		c: Linear<0, { limits::code::STATIC_MEMORY_BYTES / limits::code::BYTES_PER_INSTRUCTION }>,
	) {
		let caller = whitelisted_caller();
		T::Currency::set_balance(&caller, caller_funding::<T>());
		let WasmModule { code, hash, .. } = WasmModule::sized(c);
		let origin = RawOrigin::Signed(caller.clone());
		let storage_deposit = default_deposit_limit::<T>();
		#[extrinsic_call]
		_(origin, code, storage_deposit);
		// uploading the code reserves some balance in the callers account
		assert!(T::Currency::total_balance_on_hold(&caller) > 0u32.into());
		assert!(<Contract<T>>::code_exists(&hash));
	}

	// Removing code does not depend on the size of the contract because all the information
	// needed to verify the removal claim (refcount, owner) is stored in a separate storage
	// item (`CodeInfoOf`).
	#[benchmark(pov_mode = Measured)]
	fn remove_code() -> Result<(), BenchmarkError> {
		let caller = whitelisted_caller();
		T::Currency::set_balance(&caller, caller_funding::<T>());
		let WasmModule { code, hash, .. } = WasmModule::dummy();
		let origin = RawOrigin::Signed(caller.clone());
		let storage_deposit = default_deposit_limit::<T>();
		let uploaded =
			<Contracts<T>>::bare_upload_code(origin.clone().into(), code, storage_deposit)?;
		assert_eq!(uploaded.code_hash, hash);
		assert_eq!(uploaded.deposit, T::Currency::total_balance_on_hold(&caller));
		assert!(<Contract<T>>::code_exists(&hash));
		#[extrinsic_call]
		_(origin, hash);
		// removing the code should have unreserved the deposit
		assert_eq!(T::Currency::total_balance_on_hold(&caller), 0u32.into());
		assert!(<Contract<T>>::code_removed(&hash));
		Ok(())
	}

	#[benchmark(pov_mode = Measured)]
	fn set_code() -> Result<(), BenchmarkError> {
		let instance =
			<Contract<T>>::with_caller(whitelisted_caller(), WasmModule::dummy(), vec![])?;
		// we just add some bytes so that the code hash is different
		let WasmModule { code, .. } = WasmModule::dummy_unique(128);
		let origin = RawOrigin::Signed(instance.caller.clone());
		let storage_deposit = default_deposit_limit::<T>();
		let hash =
			<Contracts<T>>::bare_upload_code(origin.into(), code, storage_deposit)?.code_hash;
		assert_ne!(instance.info()?.code_hash, hash);
		#[extrinsic_call]
		_(RawOrigin::Root, instance.address, hash);
		assert_eq!(instance.info()?.code_hash, hash);
		Ok(())
	}

	#[benchmark(pov_mode = Measured)]
	fn map_account() {
		let caller = whitelisted_caller();
		T::Currency::set_balance(&caller, caller_funding::<T>());
		let origin = RawOrigin::Signed(caller.clone());
		assert!(!T::AddressMapper::is_mapped(&caller));
		#[extrinsic_call]
		_(origin);
		assert!(T::AddressMapper::is_mapped(&caller));
	}

	#[benchmark(pov_mode = Measured)]
	fn unmap_account() {
		let caller = whitelisted_caller();
		T::Currency::set_balance(&caller, caller_funding::<T>());
		let origin = RawOrigin::Signed(caller.clone());
		<Contracts<T>>::map_account(origin.clone().into()).unwrap();
		assert!(T::AddressMapper::is_mapped(&caller));
		#[extrinsic_call]
		_(origin);
		assert!(!T::AddressMapper::is_mapped(&caller));
	}

	#[benchmark(pov_mode = Measured)]
	fn dispatch_as_fallback_account() {
		let caller = whitelisted_caller();
		T::Currency::set_balance(&caller, caller_funding::<T>());
		let origin = RawOrigin::Signed(caller.clone());
		let dispatchable = frame_system::Call::remark { remark: vec![] }.into();
		#[extrinsic_call]
		_(origin, Box::new(dispatchable));
	}

	#[benchmark(pov_mode = Measured)]
	fn noop_host_fn(r: Linear<0, API_BENCHMARK_RUNS>) {
		let mut setup = CallSetup::<T>::new(WasmModule::noop());
		let (mut ext, module) = setup.ext();
		let prepared = CallSetup::<T>::prepare_call(&mut ext, module, r.encode(), 0);
		#[block]
		{
			prepared.call().unwrap();
		}
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_caller() {
		let len = H160::len_bytes();
		build_runtime!(runtime, memory: [vec![0u8; len as _], ]);

		let result;
		#[block]
		{
			result = runtime.bench_caller(memory.as_mut_slice(), 0);
		}

		assert_ok!(result);
		assert_eq!(
			<H160 as Decode>::decode(&mut &memory[..]).unwrap(),
			T::AddressMapper::to_address(&runtime.ext().caller().account_id().unwrap())
		);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_origin() {
		let len = H160::len_bytes();
		build_runtime!(runtime, memory: [vec![0u8; len as _], ]);

		let result;
		#[block]
		{
			result = runtime.bench_origin(memory.as_mut_slice(), 0);
		}

		assert_ok!(result);
		assert_eq!(
			<H160 as Decode>::decode(&mut &memory[..]).unwrap(),
			T::AddressMapper::to_address(&runtime.ext().origin().account_id().unwrap())
		);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_is_contract() {
		let Contract { account_id, .. } =
			Contract::<T>::with_index(1, WasmModule::dummy(), vec![]).unwrap();

		build_runtime!(runtime, memory: [account_id.encode(), ]);

		let result;
		#[block]
		{
			result = runtime.bench_is_contract(memory.as_mut_slice(), 0);
		}

		assert_eq!(result.unwrap(), 1);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_to_account_id() {
		// use a mapped address for the benchmark, to ensure that we bench the worst
		// case (and not the fallback case).
		let address = {
			let caller = account("seal_to_account_id", 0, 0);
			T::Currency::set_balance(&caller, caller_funding::<T>());
			T::AddressMapper::map(&caller).unwrap();
			T::AddressMapper::to_address(&caller)
		};

		let len = <T::AccountId as MaxEncodedLen>::max_encoded_len();
		build_runtime!(runtime, memory: [vec![0u8; len], address.0, ]);

		let result;
		#[block]
		{
			result = runtime.bench_to_account_id(memory.as_mut_slice(), len as u32, 0);
		}

		assert_ok!(result);
		assert_ne!(
			memory.as_slice()[20..32],
			[0xEE; 12],
			"fallback suffix found where none should be"
		);
		assert_eq!(
			T::AccountId::decode(&mut memory.as_slice()),
			Ok(runtime.ext().to_account_id(&address))
		);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_code_hash() {
		let contract = Contract::<T>::with_index(1, WasmModule::dummy(), vec![]).unwrap();
		let len = <sp_core::H256 as MaxEncodedLen>::max_encoded_len() as u32;
		build_runtime!(runtime, memory: [vec![0u8; len as _], contract.account_id.encode(), ]);

		let result;
		#[block]
		{
			result = runtime.bench_code_hash(memory.as_mut_slice(), len, 0);
		}

		assert_ok!(result);
		assert_eq!(
			<sp_core::H256 as Decode>::decode(&mut &memory[..]).unwrap(),
			contract.info().unwrap().code_hash
		);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_own_code_hash() {
		let len = <sp_core::H256 as MaxEncodedLen>::max_encoded_len() as u32;
		build_runtime!(runtime, contract, memory: [vec![0u8; len as _], ]);
		let result;
		#[block]
		{
			result = runtime.bench_own_code_hash(memory.as_mut_slice(), 0);
		}

		assert_ok!(result);
		assert_eq!(
			<sp_core::H256 as Decode>::decode(&mut &memory[..]).unwrap(),
			contract.info().unwrap().code_hash
		);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_code_size() {
		let contract = Contract::<T>::with_index(1, WasmModule::dummy(), vec![]).unwrap();
		build_runtime!(runtime, memory: [contract.address.encode(),]);

		let result;
		#[block]
		{
			result = runtime.bench_code_size(memory.as_mut_slice(), 0);
		}

		assert_eq!(result.unwrap(), WasmModule::dummy().code.len() as u64);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_caller_is_origin() {
		build_runtime!(runtime, memory: []);

		let result;
		#[block]
		{
			result = runtime.bench_caller_is_origin(memory.as_mut_slice());
		}
		assert_eq!(result.unwrap(), 1u32);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_caller_is_root() {
		let mut setup = CallSetup::<T>::default();
		setup.set_origin(Origin::Root);
		let (mut ext, _) = setup.ext();
		let mut runtime = crate::wasm::Runtime::new(&mut ext, vec![]);

		let result;
		#[block]
		{
			result = runtime.bench_caller_is_root([0u8; 0].as_mut_slice());
		}
		assert_eq!(result.unwrap(), 1u32);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_address() {
		let len = H160::len_bytes();
		build_runtime!(runtime, memory: [vec![0u8; len as _], ]);

		let result;
		#[block]
		{
			result = runtime.bench_address(memory.as_mut_slice(), 0);
		}
		assert_ok!(result);
		assert_eq!(<H160 as Decode>::decode(&mut &memory[..]).unwrap(), runtime.ext().address());
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_weight_left() {
		// use correct max_encoded_len when new version of parity-scale-codec is released
		let len = 18u32;
		assert!(<Weight as MaxEncodedLen>::max_encoded_len() as u32 != len);
		build_runtime!(runtime, memory: [32u32.to_le_bytes(), vec![0u8; len as _], ]);

		let result;
		#[block]
		{
			result = runtime.bench_weight_left(memory.as_mut_slice(), 4, 0);
		}
		assert_ok!(result);
		assert_eq!(
			<Weight as Decode>::decode(&mut &memory[4..]).unwrap(),
			runtime.ext().gas_meter().gas_left()
		);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_ref_time_left() {
		build_runtime!(runtime, memory: [vec![], ]);

		let result;
		#[block]
		{
			result = runtime.bench_ref_time_left(memory.as_mut_slice());
		}
		assert_eq!(result.unwrap(), runtime.ext().gas_meter().gas_left().ref_time());
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_balance() {
		build_runtime!(runtime, memory: [[0u8;32], ]);
		let result;
		#[block]
		{
			result = runtime.bench_balance(memory.as_mut_slice(), 0);
		}
		assert_ok!(result);
		assert_eq!(U256::from_little_endian(&memory[..]), runtime.ext().balance());
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_balance_of() {
		let len = <sp_core::U256 as MaxEncodedLen>::max_encoded_len();
		let account = account::<T::AccountId>("target", 0, 0);
		let address = T::AddressMapper::to_address(&account);
		let balance = Pallet::<T>::min_balance() * 2u32.into();
		T::Currency::set_balance(&account, balance);

		build_runtime!(runtime, memory: [vec![0u8; len], address.0, ]);

		let result;
		#[block]
		{
			result = runtime.bench_balance_of(memory.as_mut_slice(), len as u32, 0);
		}

		assert_ok!(result);
		assert_eq!(U256::from_little_endian(&memory[..len]), runtime.ext().balance_of(&address));
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_get_immutable_data(n: Linear<1, { limits::IMMUTABLE_BYTES }>) {
		let len = n as usize;
		let immutable_data = vec![1u8; len];

		build_runtime!(runtime, contract, memory: [(len as u32).encode(), vec![0u8; len],]);

		<ImmutableDataOf<T>>::insert::<_, BoundedVec<_, _>>(
			contract.address,
			immutable_data.clone().try_into().unwrap(),
		);

		let result;
		#[block]
		{
			result = runtime.bench_get_immutable_data(memory.as_mut_slice(), 4, 0 as u32);
		}

		assert_ok!(result);
		assert_eq!(&memory[0..4], (len as u32).encode());
		assert_eq!(&memory[4..len + 4], &immutable_data);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_set_immutable_data(n: Linear<1, { limits::IMMUTABLE_BYTES }>) {
		let len = n as usize;
		let mut memory = vec![1u8; len];
		let mut setup = CallSetup::<T>::default();
		let input = setup.data();
		let (mut ext, _) = setup.ext();
		ext.override_export(crate::exec::ExportedFunction::Constructor);

		let mut runtime = crate::wasm::Runtime::<_, [u8]>::new(&mut ext, input);

		let result;
		#[block]
		{
			result = runtime.bench_set_immutable_data(memory.as_mut_slice(), 0, n);
		}

		assert_ok!(result);
		assert_eq!(&memory[..], &<ImmutableDataOf<T>>::get(setup.contract().address).unwrap()[..]);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_value_transferred() {
		build_runtime!(runtime, memory: [[0u8;32], ]);
		let result;
		#[block]
		{
			result = runtime.bench_value_transferred(memory.as_mut_slice(), 0);
		}
		assert_ok!(result);
		assert_eq!(U256::from_little_endian(&memory[..]), runtime.ext().value_transferred());
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_minimum_balance() {
		build_runtime!(runtime, memory: [[0u8;32], ]);
		let result;
		#[block]
		{
			result = runtime.bench_minimum_balance(memory.as_mut_slice(), 0);
		}
		assert_ok!(result);
		assert_eq!(U256::from_little_endian(&memory[..]), runtime.ext().minimum_balance());
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_return_data_size() {
		let mut setup = CallSetup::<T>::default();
		let (mut ext, _) = setup.ext();
		let mut runtime = crate::wasm::Runtime::new(&mut ext, vec![]);
		let mut memory = memory!(vec![],);
		*runtime.ext().last_frame_output_mut() =
			ExecReturnValue { data: vec![42; 256], ..Default::default() };
		let result;
		#[block]
		{
			result = runtime.bench_return_data_size(memory.as_mut_slice());
		}
		assert_eq!(result.unwrap(), 256);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_call_data_size() {
		let mut setup = CallSetup::<T>::default();
		let (mut ext, _) = setup.ext();
		let mut runtime = crate::wasm::Runtime::new(&mut ext, vec![42u8; 128 as usize]);
		let mut memory = memory!(vec![0u8; 4],);
		let result;
		#[block]
		{
			result = runtime.bench_call_data_size(memory.as_mut_slice());
		}
		assert_eq!(result.unwrap(), 128);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_gas_limit() {
		build_runtime!(runtime, memory: []);
		let result;
		#[block]
		{
			result = runtime.bench_gas_limit(&mut memory);
		}
		assert_eq!(result.unwrap(), T::BlockWeights::get().max_block.ref_time());
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_gas_price() {
		build_runtime!(runtime, memory: []);
		let result;
		#[block]
		{
			result = runtime.bench_gas_price(memory.as_mut_slice());
		}
		assert_eq!(result.unwrap(), u64::from(GAS_PRICE));
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_base_fee() {
		build_runtime!(runtime, memory: [[1u8;32], ]);
		let result;
		#[block]
		{
			result = runtime.bench_base_fee(memory.as_mut_slice(), 0);
		}
		assert_ok!(result);
		assert_eq!(U256::from_little_endian(&memory[..]), U256::zero());
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_block_number() {
		build_runtime!(runtime, memory: [[0u8;32], ]);
		let result;
		#[block]
		{
			result = runtime.bench_block_number(memory.as_mut_slice(), 0);
		}
		assert_ok!(result);
		assert_eq!(U256::from_little_endian(&memory[..]), runtime.ext().block_number());
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_block_author() {
		build_runtime!(runtime, memory: [[123u8; 20], ]);

		let mut digest = Digest::default();

		// The pre-runtime digest log is unbounded; usually around 3 items but it can vary.
		// To get safe benchmark results despite that, populate it with a bunch of random logs to
		// ensure iteration over many items (we just overestimate the cost of the API).
		for i in 0..16 {
			digest.push(DigestItem::PreRuntime([i, i, i, i], vec![i; 128]));
			digest.push(DigestItem::Consensus([i, i, i, i], vec![i; 128]));
			digest.push(DigestItem::Seal([i, i, i, i], vec![i; 128]));
			digest.push(DigestItem::Other(vec![i; 128]));
		}

		// The content of the pre-runtime digest log depends on the configured consensus.
		// However, mismatching logs are simply ignored. Thus we construct fixtures which will
		// let the API to return a value in both BABE and AURA consensus.

		// Construct a `Digest` log fixture returning some value in BABE
		let primary_pre_digest = vec![0; <PrimaryPreDigest as MaxEncodedLen>::max_encoded_len()];
		let pre_digest =
			PreDigest::Primary(PrimaryPreDigest::decode(&mut &primary_pre_digest[..]).unwrap());
		digest.push(DigestItem::PreRuntime(BABE_ENGINE_ID, pre_digest.encode()));
		digest.push(DigestItem::Seal(BABE_ENGINE_ID, pre_digest.encode()));

		// Construct a `Digest` log fixture returning some value in AURA
		let slot = Slot::default();
		digest.push(DigestItem::PreRuntime(AURA_ENGINE_ID, slot.encode()));
		digest.push(DigestItem::Seal(AURA_ENGINE_ID, slot.encode()));

		frame_system::Pallet::<T>::initialize(
			&BlockNumberFor::<T>::from(1u32),
			&Default::default(),
			&digest,
		);

		let result;
		#[block]
		{
			result = runtime.bench_block_author(memory.as_mut_slice(), 0);
		}
		assert_ok!(result);

		let block_author = runtime
			.ext()
			.block_author()
			.map(|account| T::AddressMapper::to_address(&account))
			.unwrap_or(H160::zero());
		assert_eq!(&memory[..], block_author.as_bytes());
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_block_hash() {
		let mut memory = vec![0u8; 64];
		let mut setup = CallSetup::<T>::default();
		let input = setup.data();
		let (mut ext, _) = setup.ext();
		ext.set_block_number(BlockNumberFor::<T>::from(1u32));

		let mut runtime = crate::wasm::Runtime::<_, [u8]>::new(&mut ext, input);

		let block_hash = H256::from([1; 32]);
		frame_system::BlockHash::<T>::insert(
			&BlockNumberFor::<T>::from(0u32),
			T::Hash::from(block_hash),
		);

		let result;
		#[block]
		{
			result = runtime.bench_block_hash(memory.as_mut_slice(), 32, 0);
		}
		assert_ok!(result);
		assert_eq!(&memory[..32], &block_hash.0);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_now() {
		build_runtime!(runtime, memory: [[0u8;32], ]);
		let result;
		#[block]
		{
			result = runtime.bench_now(memory.as_mut_slice(), 0);
		}
		assert_ok!(result);
		assert_eq!(U256::from_little_endian(&memory[..]), runtime.ext().now());
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_weight_to_fee() {
		build_runtime!(runtime, memory: [[0u8;32], ]);
		let weight = Weight::from_parts(500_000, 300_000);
		let result;
		#[block]
		{
			result = runtime.bench_weight_to_fee(
				memory.as_mut_slice(),
				weight.ref_time(),
				weight.proof_size(),
				0,
			);
		}
		assert_ok!(result);
		assert_eq!(U256::from_little_endian(&memory[..]), runtime.ext().get_weight_price(weight));
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_copy_to_contract(n: Linear<0, { limits::code::BLOB_BYTES - 4 }>) {
		let mut setup = CallSetup::<T>::default();
		let (mut ext, _) = setup.ext();
		let mut runtime = crate::wasm::Runtime::new(&mut ext, vec![]);
		let mut memory = memory!(n.encode(), vec![0u8; n as usize],);
		let result;
		#[block]
		{
			result = runtime.write_sandbox_output(
				memory.as_mut_slice(),
				4,
				0,
				&vec![42u8; n as usize],
				false,
				|_| None,
			);
		}
		assert_ok!(result);
		assert_eq!(&memory[..4], &n.encode());
		assert_eq!(&memory[4..], &vec![42u8; n as usize]);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_call_data_load() {
		let mut setup = CallSetup::<T>::default();
		let (mut ext, _) = setup.ext();
		let mut runtime = crate::wasm::Runtime::new(&mut ext, vec![42u8; 32]);
		let mut memory = memory!(vec![0u8; 32],);
		let result;
		#[block]
		{
			result = runtime.bench_call_data_load(memory.as_mut_slice(), 0, 0);
		}
		assert_ok!(result);
		assert_eq!(&memory[..], &vec![42u8; 32]);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_call_data_copy(n: Linear<0, { limits::code::BLOB_BYTES }>) {
		let mut setup = CallSetup::<T>::default();
		let (mut ext, _) = setup.ext();
		let mut runtime = crate::wasm::Runtime::new(&mut ext, vec![42u8; n as usize]);
		let mut memory = memory!(vec![0u8; n as usize],);
		let result;
		#[block]
		{
			result = runtime.bench_call_data_copy(memory.as_mut_slice(), 0, n, 0);
		}
		assert_ok!(result);
		assert_eq!(&memory[..], &vec![42u8; n as usize]);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_return(n: Linear<0, { limits::code::BLOB_BYTES - 4 }>) {
		build_runtime!(runtime, memory: [n.to_le_bytes(), vec![42u8; n as usize], ]);

		let result;
		#[block]
		{
			result = runtime.bench_seal_return(memory.as_mut_slice(), 0, 0, n);
		}

		assert!(matches!(
			result,
			Err(crate::wasm::TrapReason::Return(crate::wasm::ReturnData { .. }))
		));
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_terminate() -> Result<(), BenchmarkError> {
		let beneficiary = account::<T::AccountId>("beneficiary", 0, 0);

		build_runtime!(runtime, memory: [beneficiary.encode(),]);

		let result;
		#[block]
		{
			result = runtime.bench_terminate(memory.as_mut_slice(), 0);
		}

		assert!(matches!(result, Err(crate::wasm::TrapReason::Termination)));

		Ok(())
	}

	// Benchmark the overhead that topics generate.
	// `t`: Number of topics
	// `n`: Size of event payload in bytes
	#[benchmark(pov_mode = Measured)]
	fn seal_deposit_event(
		t: Linear<0, { limits::NUM_EVENT_TOPICS as u32 }>,
		n: Linear<0, { limits::PAYLOAD_BYTES }>,
	) {
		let num_topic = t as u32;
		let topics = (0..t).map(|i| H256::repeat_byte(i as u8)).collect::<Vec<_>>();
		let topics_data =
			topics.iter().flat_map(|hash| hash.as_bytes().to_vec()).collect::<Vec<u8>>();
		let data = vec![42u8; n as _];
		build_runtime!(runtime, instance, memory: [ topics_data, data, ]);

		let result;
		#[block]
		{
			result = runtime.bench_deposit_event(
				memory.as_mut_slice(),
				0, // topics_ptr
				num_topic,
				topics_data.len() as u32, // data_ptr
				n,                        // data_len
			);
		}
		assert_ok!(result);

		let events = System::<T>::events();
		let record = &events[events.len() - 1];

		assert_eq!(
			record.event,
			crate::Event::ContractEmitted { contract: instance.address, data, topics }.into(),
		);
	}

	#[benchmark(skip_meta, pov_mode = Measured)]
	fn get_storage_empty() -> Result<(), BenchmarkError> {
		let max_key_len = limits::STORAGE_KEY_BYTES;
		let key = vec![0u8; max_key_len as usize];
		let max_value_len = limits::PAYLOAD_BYTES as usize;
		let value = vec![1u8; max_value_len];

		let instance = Contract::<T>::new(WasmModule::dummy(), vec![])?;
		let info = instance.info()?;
		let child_trie_info = info.child_trie_info();
		info.bench_write_raw(&key, Some(value.clone()), false)
			.map_err(|_| "Failed to write to storage during setup.")?;

		let result;
		#[block]
		{
			result = child::get_raw(&child_trie_info, &key);
		}

		assert_eq!(result, Some(value));
		Ok(())
	}

	#[benchmark(skip_meta, pov_mode = Measured)]
	fn get_storage_full() -> Result<(), BenchmarkError> {
		let max_key_len = limits::STORAGE_KEY_BYTES;
		let key = vec![0u8; max_key_len as usize];
		let max_value_len = limits::PAYLOAD_BYTES;
		let value = vec![1u8; max_value_len as usize];

		let instance = Contract::<T>::with_unbalanced_storage_trie(WasmModule::dummy(), &key)?;
		let info = instance.info()?;
		let child_trie_info = info.child_trie_info();
		info.bench_write_raw(&key, Some(value.clone()), false)
			.map_err(|_| "Failed to write to storage during setup.")?;

		let result;
		#[block]
		{
			result = child::get_raw(&child_trie_info, &key);
		}

		assert_eq!(result, Some(value));
		Ok(())
	}

	#[benchmark(skip_meta, pov_mode = Measured)]
	fn set_storage_empty() -> Result<(), BenchmarkError> {
		let max_key_len = limits::STORAGE_KEY_BYTES;
		let key = vec![0u8; max_key_len as usize];
		let max_value_len = limits::PAYLOAD_BYTES as usize;
		let value = vec![1u8; max_value_len];

		let instance = Contract::<T>::new(WasmModule::dummy(), vec![])?;
		let info = instance.info()?;
		let child_trie_info = info.child_trie_info();
		info.bench_write_raw(&key, Some(vec![42u8; max_value_len]), false)
			.map_err(|_| "Failed to write to storage during setup.")?;

		let val = Some(value.clone());
		let result;
		#[block]
		{
			result = info.bench_write_raw(&key, val, true);
		}

		assert_ok!(result);
		assert_eq!(child::get_raw(&child_trie_info, &key).unwrap(), value);
		Ok(())
	}

	#[benchmark(skip_meta, pov_mode = Measured)]
	fn set_storage_full() -> Result<(), BenchmarkError> {
		let max_key_len = limits::STORAGE_KEY_BYTES;
		let key = vec![0u8; max_key_len as usize];
		let max_value_len = limits::PAYLOAD_BYTES;
		let value = vec![1u8; max_value_len as usize];

		let instance = Contract::<T>::with_unbalanced_storage_trie(WasmModule::dummy(), &key)?;
		let info = instance.info()?;
		let child_trie_info = info.child_trie_info();
		info.bench_write_raw(&key, Some(vec![42u8; max_value_len as usize]), false)
			.map_err(|_| "Failed to write to storage during setup.")?;

		let val = Some(value.clone());
		let result;
		#[block]
		{
			result = info.bench_write_raw(&key, val, true);
		}

		assert_ok!(result);
		assert_eq!(child::get_raw(&child_trie_info, &key).unwrap(), value);
		Ok(())
	}

	// n: new byte size
	// o: old byte size
	#[benchmark(skip_meta, pov_mode = Measured)]
	fn seal_set_storage(
		n: Linear<0, { limits::PAYLOAD_BYTES }>,
		o: Linear<0, { limits::PAYLOAD_BYTES }>,
	) -> Result<(), BenchmarkError> {
		let max_key_len = limits::STORAGE_KEY_BYTES;
		let key = Key::try_from_var(vec![0u8; max_key_len as usize])
			.map_err(|_| "Key has wrong length")?;
		let value = vec![1u8; n as usize];

		build_runtime!(runtime, instance, memory: [ key.unhashed(), value.clone(), ]);
		let info = instance.info()?;

		info.write(&key, Some(vec![42u8; o as usize]), None, false)
			.map_err(|_| "Failed to write to storage during setup.")?;

		let result;
		#[block]
		{
			result = runtime.bench_set_storage(
				memory.as_mut_slice(),
				StorageFlags::empty().bits(),
				0,           // key_ptr
				max_key_len, // key_len
				max_key_len, // value_ptr
				n,           // value_len
			);
		}

		assert_ok!(result);
		assert_eq!(info.read(&key).unwrap(), value);
		Ok(())
	}

	#[benchmark(skip_meta, pov_mode = Measured)]
	fn seal_clear_storage(n: Linear<0, { limits::PAYLOAD_BYTES }>) -> Result<(), BenchmarkError> {
		let max_key_len = limits::STORAGE_KEY_BYTES;
		let key = Key::try_from_var(vec![0u8; max_key_len as usize])
			.map_err(|_| "Key has wrong length")?;
		build_runtime!(runtime, instance, memory: [ key.unhashed(), ]);
		let info = instance.info()?;

		info.write(&key, Some(vec![42u8; n as usize]), None, false)
			.map_err(|_| "Failed to write to storage during setup.")?;

		let result;
		#[block]
		{
			result = runtime.bench_clear_storage(
				memory.as_mut_slice(),
				StorageFlags::empty().bits(),
				0,
				max_key_len,
			);
		}

		assert_ok!(result);
		assert!(info.read(&key).is_none());
		Ok(())
	}

	#[benchmark(skip_meta, pov_mode = Measured)]
	fn seal_get_storage(n: Linear<0, { limits::PAYLOAD_BYTES }>) -> Result<(), BenchmarkError> {
		let max_key_len = limits::STORAGE_KEY_BYTES;
		let key = Key::try_from_var(vec![0u8; max_key_len as usize])
			.map_err(|_| "Key has wrong length")?;
		build_runtime!(runtime, instance, memory: [ key.unhashed(), n.to_le_bytes(), vec![0u8; n as _], ]);
		let info = instance.info()?;

		info.write(&key, Some(vec![42u8; n as usize]), None, false)
			.map_err(|_| "Failed to write to storage during setup.")?;

		let out_ptr = max_key_len + 4;
		let result;
		#[block]
		{
			result = runtime.bench_get_storage(
				memory.as_mut_slice(),
				StorageFlags::empty().bits(),
				0,           // key_ptr
				max_key_len, // key_len
				out_ptr,     // out_ptr
				max_key_len, // out_len_ptr
			);
		}

		assert_ok!(result);
		assert_eq!(&info.read(&key).unwrap(), &memory[out_ptr as usize..]);
		Ok(())
	}

	#[benchmark(skip_meta, pov_mode = Measured)]
	fn seal_contains_storage(
		n: Linear<0, { limits::PAYLOAD_BYTES }>,
	) -> Result<(), BenchmarkError> {
		let max_key_len = limits::STORAGE_KEY_BYTES;
		let key = Key::try_from_var(vec![0u8; max_key_len as usize])
			.map_err(|_| "Key has wrong length")?;
		build_runtime!(runtime, instance, memory: [ key.unhashed(), ]);
		let info = instance.info()?;

		info.write(&key, Some(vec![42u8; n as usize]), None, false)
			.map_err(|_| "Failed to write to storage during setup.")?;

		let result;
		#[block]
		{
			result = runtime.bench_contains_storage(
				memory.as_mut_slice(),
				StorageFlags::empty().bits(),
				0,
				max_key_len,
			);
		}

		assert_eq!(result.unwrap(), n);
		Ok(())
	}

	#[benchmark(skip_meta, pov_mode = Measured)]
	fn seal_take_storage(n: Linear<0, { limits::PAYLOAD_BYTES }>) -> Result<(), BenchmarkError> {
		let max_key_len = limits::STORAGE_KEY_BYTES;
		let key = Key::try_from_var(vec![0u8; max_key_len as usize])
			.map_err(|_| "Key has wrong length")?;
		build_runtime!(runtime, instance, memory: [ key.unhashed(), n.to_le_bytes(), vec![0u8; n as _], ]);
		let info = instance.info()?;

		let value = vec![42u8; n as usize];
		info.write(&key, Some(value.clone()), None, false)
			.map_err(|_| "Failed to write to storage during setup.")?;

		let out_ptr = max_key_len + 4;
		let result;
		#[block]
		{
			result = runtime.bench_take_storage(
				memory.as_mut_slice(),
				StorageFlags::empty().bits(),
				0,           // key_ptr
				max_key_len, // key_len
				out_ptr,     // out_ptr
				max_key_len, // out_len_ptr
			);
		}

		assert_ok!(result);
		assert!(&info.read(&key).is_none());
		assert_eq!(&value, &memory[out_ptr as usize..]);
		Ok(())
	}

	// We use both full and empty benchmarks here instead of benchmarking transient_storage
	// (BTreeMap) directly. This approach is necessary because benchmarking this BTreeMap is very
	// slow. Additionally, we use linear regression for our benchmarks, and the BTreeMap's log(n)
	// complexity can introduce approximation errors.
	#[benchmark(pov_mode = Ignored)]
	fn set_transient_storage_empty() -> Result<(), BenchmarkError> {
		let max_value_len = limits::PAYLOAD_BYTES;
		let max_key_len = limits::STORAGE_KEY_BYTES;
		let key = Key::try_from_var(vec![0u8; max_key_len as usize])
			.map_err(|_| "Key has wrong length")?;
		let value = Some(vec![42u8; max_value_len as _]);
		let mut setup = CallSetup::<T>::default();
		let (mut ext, _) = setup.ext();
		let mut runtime = crate::wasm::Runtime::<_, [u8]>::new(&mut ext, vec![]);
		runtime.ext().transient_storage().meter().current_mut().limit = u32::MAX;
		let result;
		#[block]
		{
			result = runtime.ext().set_transient_storage(&key, value, false);
		}

		assert_eq!(result, Ok(WriteOutcome::New));
		assert_eq!(runtime.ext().get_transient_storage(&key), Some(vec![42u8; max_value_len as _]));
		Ok(())
	}

	#[benchmark(pov_mode = Ignored)]
	fn set_transient_storage_full() -> Result<(), BenchmarkError> {
		let max_value_len = limits::PAYLOAD_BYTES;
		let max_key_len = limits::STORAGE_KEY_BYTES;
		let key = Key::try_from_var(vec![0u8; max_key_len as usize])
			.map_err(|_| "Key has wrong length")?;
		let value = Some(vec![42u8; max_value_len as _]);
		let mut setup = CallSetup::<T>::default();
		setup.set_transient_storage_size(limits::TRANSIENT_STORAGE_BYTES);
		let (mut ext, _) = setup.ext();
		let mut runtime = crate::wasm::Runtime::<_, [u8]>::new(&mut ext, vec![]);
		runtime.ext().transient_storage().meter().current_mut().limit = u32::MAX;
		let result;
		#[block]
		{
			result = runtime.ext().set_transient_storage(&key, value, false);
		}

		assert_eq!(result, Ok(WriteOutcome::New));
		assert_eq!(runtime.ext().get_transient_storage(&key), Some(vec![42u8; max_value_len as _]));
		Ok(())
	}

	#[benchmark(pov_mode = Ignored)]
	fn get_transient_storage_empty() -> Result<(), BenchmarkError> {
		let max_value_len = limits::PAYLOAD_BYTES;
		let max_key_len = limits::STORAGE_KEY_BYTES;
		let key = Key::try_from_var(vec![0u8; max_key_len as usize])
			.map_err(|_| "Key has wrong length")?;

		let mut setup = CallSetup::<T>::default();
		let (mut ext, _) = setup.ext();
		let mut runtime = crate::wasm::Runtime::<_, [u8]>::new(&mut ext, vec![]);
		runtime.ext().transient_storage().meter().current_mut().limit = u32::MAX;
		runtime
			.ext()
			.set_transient_storage(&key, Some(vec![42u8; max_value_len as _]), false)
			.map_err(|_| "Failed to write to transient storage during setup.")?;
		let result;
		#[block]
		{
			result = runtime.ext().get_transient_storage(&key);
		}

		assert_eq!(result, Some(vec![42u8; max_value_len as _]));
		Ok(())
	}

	#[benchmark(pov_mode = Ignored)]
	fn get_transient_storage_full() -> Result<(), BenchmarkError> {
		let max_value_len = limits::PAYLOAD_BYTES;
		let max_key_len = limits::STORAGE_KEY_BYTES;
		let key = Key::try_from_var(vec![0u8; max_key_len as usize])
			.map_err(|_| "Key has wrong length")?;

		let mut setup = CallSetup::<T>::default();
		setup.set_transient_storage_size(limits::TRANSIENT_STORAGE_BYTES);
		let (mut ext, _) = setup.ext();
		let mut runtime = crate::wasm::Runtime::<_, [u8]>::new(&mut ext, vec![]);
		runtime.ext().transient_storage().meter().current_mut().limit = u32::MAX;
		runtime
			.ext()
			.set_transient_storage(&key, Some(vec![42u8; max_value_len as _]), false)
			.map_err(|_| "Failed to write to transient storage during setup.")?;
		let result;
		#[block]
		{
			result = runtime.ext().get_transient_storage(&key);
		}

		assert_eq!(result, Some(vec![42u8; max_value_len as _]));
		Ok(())
	}

	// The weight of journal rollbacks should be taken into account when setting storage.
	#[benchmark(pov_mode = Ignored)]
	fn rollback_transient_storage() -> Result<(), BenchmarkError> {
		let max_value_len = limits::PAYLOAD_BYTES;
		let max_key_len = limits::STORAGE_KEY_BYTES;
		let key = Key::try_from_var(vec![0u8; max_key_len as usize])
			.map_err(|_| "Key has wrong length")?;

		let mut setup = CallSetup::<T>::default();
		setup.set_transient_storage_size(limits::TRANSIENT_STORAGE_BYTES);
		let (mut ext, _) = setup.ext();
		let mut runtime = crate::wasm::Runtime::<_, [u8]>::new(&mut ext, vec![]);
		runtime.ext().transient_storage().meter().current_mut().limit = u32::MAX;
		runtime.ext().transient_storage().start_transaction();
		runtime
			.ext()
			.set_transient_storage(&key, Some(vec![42u8; max_value_len as _]), false)
			.map_err(|_| "Failed to write to transient storage during setup.")?;
		#[block]
		{
			runtime.ext().transient_storage().rollback_transaction();
		}

		assert_eq!(runtime.ext().get_transient_storage(&key), None);
		Ok(())
	}

	// n: new byte size
	// o: old byte size
	#[benchmark(pov_mode = Measured)]
	fn seal_set_transient_storage(
		n: Linear<0, { limits::PAYLOAD_BYTES }>,
		o: Linear<0, { limits::PAYLOAD_BYTES }>,
	) -> Result<(), BenchmarkError> {
		let max_key_len = limits::STORAGE_KEY_BYTES;
		let key = Key::try_from_var(vec![0u8; max_key_len as usize])
			.map_err(|_| "Key has wrong length")?;
		let value = vec![1u8; n as usize];
		build_runtime!(runtime, memory: [ key.unhashed(), value.clone(), ]);
		runtime.ext().transient_storage().meter().current_mut().limit = u32::MAX;
		runtime
			.ext()
			.set_transient_storage(&key, Some(vec![42u8; o as usize]), false)
			.map_err(|_| "Failed to write to transient storage during setup.")?;

		let result;
		#[block]
		{
			result = runtime.bench_set_storage(
				memory.as_mut_slice(),
				StorageFlags::TRANSIENT.bits(),
				0,           // key_ptr
				max_key_len, // key_len
				max_key_len, // value_ptr
				n,           // value_len
			);
		}

		assert_ok!(result);
		assert_eq!(runtime.ext().get_transient_storage(&key).unwrap(), value);
		Ok(())
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_clear_transient_storage(
		n: Linear<0, { limits::PAYLOAD_BYTES }>,
	) -> Result<(), BenchmarkError> {
		let max_key_len = limits::STORAGE_KEY_BYTES;
		let key = Key::try_from_var(vec![0u8; max_key_len as usize])
			.map_err(|_| "Key has wrong length")?;
		build_runtime!(runtime, memory: [ key.unhashed(), ]);
		runtime.ext().transient_storage().meter().current_mut().limit = u32::MAX;
		runtime
			.ext()
			.set_transient_storage(&key, Some(vec![42u8; n as usize]), false)
			.map_err(|_| "Failed to write to transient storage during setup.")?;

		let result;
		#[block]
		{
			result = runtime.bench_clear_storage(
				memory.as_mut_slice(),
				StorageFlags::TRANSIENT.bits(),
				0,
				max_key_len,
			);
		}

		assert_ok!(result);
		assert!(runtime.ext().get_transient_storage(&key).is_none());
		Ok(())
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_get_transient_storage(
		n: Linear<0, { limits::PAYLOAD_BYTES }>,
	) -> Result<(), BenchmarkError> {
		let max_key_len = limits::STORAGE_KEY_BYTES;
		let key = Key::try_from_var(vec![0u8; max_key_len as usize])
			.map_err(|_| "Key has wrong length")?;
		build_runtime!(runtime, memory: [ key.unhashed(), n.to_le_bytes(), vec![0u8; n as _], ]);
		runtime.ext().transient_storage().meter().current_mut().limit = u32::MAX;
		runtime
			.ext()
			.set_transient_storage(&key, Some(vec![42u8; n as usize]), false)
			.map_err(|_| "Failed to write to transient storage during setup.")?;

		let out_ptr = max_key_len + 4;
		let result;
		#[block]
		{
			result = runtime.bench_get_storage(
				memory.as_mut_slice(),
				StorageFlags::TRANSIENT.bits(),
				0,           // key_ptr
				max_key_len, // key_len
				out_ptr,     // out_ptr
				max_key_len, // out_len_ptr
			);
		}

		assert_ok!(result);
		assert_eq!(
			&runtime.ext().get_transient_storage(&key).unwrap(),
			&memory[out_ptr as usize..]
		);
		Ok(())
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_contains_transient_storage(
		n: Linear<0, { limits::PAYLOAD_BYTES }>,
	) -> Result<(), BenchmarkError> {
		let max_key_len = limits::STORAGE_KEY_BYTES;
		let key = Key::try_from_var(vec![0u8; max_key_len as usize])
			.map_err(|_| "Key has wrong length")?;
		build_runtime!(runtime, memory: [ key.unhashed(), ]);
		runtime.ext().transient_storage().meter().current_mut().limit = u32::MAX;
		runtime
			.ext()
			.set_transient_storage(&key, Some(vec![42u8; n as usize]), false)
			.map_err(|_| "Failed to write to transient storage during setup.")?;

		let result;
		#[block]
		{
			result = runtime.bench_contains_storage(
				memory.as_mut_slice(),
				StorageFlags::TRANSIENT.bits(),
				0,
				max_key_len,
			);
		}

		assert_eq!(result.unwrap(), n);
		Ok(())
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_take_transient_storage(
		n: Linear<0, { limits::PAYLOAD_BYTES }>,
	) -> Result<(), BenchmarkError> {
		let n = limits::PAYLOAD_BYTES;
		let max_key_len = limits::STORAGE_KEY_BYTES;
		let key = Key::try_from_var(vec![0u8; max_key_len as usize])
			.map_err(|_| "Key has wrong length")?;
		build_runtime!(runtime, memory: [ key.unhashed(), n.to_le_bytes(), vec![0u8; n as _], ]);
		runtime.ext().transient_storage().meter().current_mut().limit = u32::MAX;
		let value = vec![42u8; n as usize];
		runtime
			.ext()
			.set_transient_storage(&key, Some(value.clone()), false)
			.map_err(|_| "Failed to write to transient storage during setup.")?;

		let out_ptr = max_key_len + 4;
		let result;
		#[block]
		{
			result = runtime.bench_take_storage(
				memory.as_mut_slice(),
				StorageFlags::TRANSIENT.bits(),
				0,           // key_ptr
				max_key_len, // key_len
				out_ptr,     // out_ptr
				max_key_len, // out_len_ptr
			);
		}

		assert_ok!(result);
		assert!(&runtime.ext().get_transient_storage(&key).is_none());
		assert_eq!(&value, &memory[out_ptr as usize..]);
		Ok(())
	}

	// t: with or without some value to transfer
	// i: size of the input data
	#[benchmark(pov_mode = Measured)]
	fn seal_call(t: Linear<0, 1>, i: Linear<0, { limits::code::BLOB_BYTES }>) {
		let Contract { account_id: callee, .. } =
			Contract::<T>::with_index(1, WasmModule::dummy(), vec![]).unwrap();
		let callee_bytes = callee.encode();
		let callee_len = callee_bytes.len() as u32;

		let value: BalanceOf<T> = (1_000_000 * t).into();
		let value_bytes = Into::<U256>::into(value).encode();

		let deposit: BalanceOf<T> = (u32::MAX - 100).into();
		let deposit_bytes = Into::<U256>::into(deposit).encode();
		let deposit_len = deposit_bytes.len() as u32;

		let mut setup = CallSetup::<T>::default();
		setup.set_storage_deposit_limit(deposit);
		setup.set_data(vec![42; i as usize]);
		setup.set_origin(Origin::from_account_id(setup.contract().account_id.clone()));

		let (mut ext, _) = setup.ext();
		let mut runtime = crate::wasm::Runtime::<_, [u8]>::new(&mut ext, vec![]);
		let mut memory = memory!(callee_bytes, deposit_bytes, value_bytes,);

		let result;
		#[block]
		{
			result = runtime.bench_call(
				memory.as_mut_slice(),
				pack_hi_lo(CallFlags::CLONE_INPUT.bits(), 0), // flags + callee
				u64::MAX,                                     // ref_time_limit
				u64::MAX,                                     // proof_size_limit
				pack_hi_lo(callee_len, callee_len + deposit_len), // deposit_ptr + value_pr
				pack_hi_lo(0, 0),                             // input len + data ptr
				pack_hi_lo(0, SENTINEL),                      // output len + data ptr
			);
		}

		assert_ok!(result);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_delegate_call() -> Result<(), BenchmarkError> {
		let Contract { account_id: address, .. } =
			Contract::<T>::with_index(1, WasmModule::dummy(), vec![]).unwrap();

		let address_bytes = address.encode();
		let address_len = address_bytes.len() as u32;

		let deposit: BalanceOf<T> = (u32::MAX - 100).into();
		let deposit_bytes = Into::<U256>::into(deposit).encode();

		let mut setup = CallSetup::<T>::default();
		setup.set_storage_deposit_limit(deposit);
		setup.set_origin(Origin::from_account_id(setup.contract().account_id.clone()));

		let (mut ext, _) = setup.ext();
		let mut runtime = crate::wasm::Runtime::<_, [u8]>::new(&mut ext, vec![]);
		let mut memory = memory!(address_bytes, deposit_bytes,);

		let result;
		#[block]
		{
			result = runtime.bench_delegate_call(
				memory.as_mut_slice(),
				pack_hi_lo(0, 0),        // flags + address ptr
				u64::MAX,                // ref_time_limit
				u64::MAX,                // proof_size_limit
				address_len,             // deposit_ptr
				pack_hi_lo(0, 0),        // input len + data ptr
				pack_hi_lo(0, SENTINEL), // output len + ptr
			);
		}

		assert_ok!(result);
		Ok(())
	}

	// t: value to transfer
	// i: size of input in bytes
	#[benchmark(pov_mode = Measured)]
	fn seal_instantiate(i: Linear<0, { limits::code::BLOB_BYTES }>) -> Result<(), BenchmarkError> {
		let code = WasmModule::dummy();
		let hash = Contract::<T>::with_index(1, WasmModule::dummy(), vec![])?.info()?.code_hash;
		let hash_bytes = hash.encode();

		let value: BalanceOf<T> = 1_000_000u32.into();
		let value_bytes = Into::<U256>::into(value).encode();
		let value_len = value_bytes.len() as u32;

		let deposit: BalanceOf<T> = BalanceOf::<T>::max_value();
		let deposit_bytes = Into::<U256>::into(deposit).encode();
		let deposit_len = deposit_bytes.len() as u32;

		let mut setup = CallSetup::<T>::default();
		setup.set_origin(Origin::from_account_id(setup.contract().account_id.clone()));
		setup.set_balance(value + (Pallet::<T>::min_balance() * 2u32.into()));

		let account_id = &setup.contract().account_id.clone();
		let (mut ext, _) = setup.ext();
		let mut runtime = crate::wasm::Runtime::<_, [u8]>::new(&mut ext, vec![]);

		let input = vec![42u8; i as _];
		let input_len = hash_bytes.len() as u32 + input.len() as u32;
		let salt = [42u8; 32];
		let deployer = T::AddressMapper::to_address(&account_id);
		let addr = crate::address::create2(&deployer, &code.code, &input, &salt);
		let account_id = T::AddressMapper::to_fallback_account_id(&addr);
		let mut memory = memory!(hash_bytes, input, deposit_bytes, value_bytes, salt,);

		let mut offset = {
			let mut current = 0u32;
			move |after: u32| {
				current += after;
				current
			}
		};

		assert!(ContractInfoOf::<T>::get(&addr).is_none());

		let result;
		#[block]
		{
			result = runtime.bench_instantiate(
				memory.as_mut_slice(),
				u64::MAX,                                           // ref_time_limit
				u64::MAX,                                           // proof_size_limit
				pack_hi_lo(offset(input_len), offset(deposit_len)), // deopsit_ptr + value_ptr
				pack_hi_lo(input_len, 0),                           // input_data_len + input_data
				pack_hi_lo(0, SENTINEL),                            // output_len_ptr + output_ptr
				pack_hi_lo(SENTINEL, offset(value_len)),            // address_ptr + salt_ptr
			);
		}

		assert_ok!(result);
		assert!(ContractInfoOf::<T>::get(&addr).is_some());
		assert_eq!(
			T::Currency::balance(&account_id),
			Pallet::<T>::min_balance() +
				Pallet::<T>::convert_evm_to_native(value.into(), ConversionPrecision::Exact)
					.unwrap()
		);
		Ok(())
	}

	// `n`: Input to hash in bytes
	#[benchmark(pov_mode = Measured)]
	fn sha2_256(n: Linear<0, { limits::code::BLOB_BYTES }>) {
		let input = vec![0u8; n as usize];
		let mut call_setup = CallSetup::<T>::default();
		let (mut ext, _) = call_setup.ext();

		let result;
		#[block]
		{
			result = pure_precompiles::Sha256::execute(ext.gas_meter_mut(), &input);
		}
		assert_eq!(sp_io::hashing::sha2_256(&input).to_vec(), result.unwrap().data);
	}

	#[benchmark(pov_mode = Measured)]
	fn identity(n: Linear<0, { limits::code::BLOB_BYTES }>) {
		let input = vec![0u8; n as usize];
		let mut call_setup = CallSetup::<T>::default();
		let (mut ext, _) = call_setup.ext();

		let result;
		#[block]
		{
			result = pure_precompiles::Identity::execute(ext.gas_meter_mut(), &input);
		}
		assert_eq!(input, result.unwrap().data);
	}

	// `n`: Input to hash in bytes
	#[benchmark(pov_mode = Measured)]
	fn ripemd_160(n: Linear<0, { limits::code::BLOB_BYTES }>) {
		use ripemd::Digest;
		let input = vec![0u8; n as usize];
		let mut call_setup = CallSetup::<T>::default();
		let (mut ext, _) = call_setup.ext();

		let result;
		#[block]
		{
			result = pure_precompiles::Ripemd160::execute(ext.gas_meter_mut(), &input);
		}
		let mut expected = [0u8; 32];
		expected[12..32].copy_from_slice(&ripemd::Ripemd160::digest(input));

		assert_eq!(expected.to_vec(), result.unwrap().data);
	}

	// `n`: Input to hash in bytes
	#[benchmark(pov_mode = Measured)]
	fn seal_hash_keccak_256(n: Linear<0, { limits::code::BLOB_BYTES }>) {
		build_runtime!(runtime, memory: [[0u8; 32], vec![0u8; n as usize], ]);

		let result;
		#[block]
		{
			result = runtime.bench_hash_keccak_256(memory.as_mut_slice(), 32, n, 0);
		}
		assert_eq!(sp_io::hashing::keccak_256(&memory[32..]), &memory[0..32]);
		assert_ok!(result);
	}

	// `n`: Input to hash in bytes
	#[benchmark(pov_mode = Measured)]
	fn seal_hash_blake2_256(n: Linear<0, { limits::code::BLOB_BYTES }>) {
		build_runtime!(runtime, memory: [[0u8; 32], vec![0u8; n as usize], ]);

		let result;
		#[block]
		{
			result = runtime.bench_hash_blake2_256(memory.as_mut_slice(), 32, n, 0);
		}
		assert_eq!(sp_io::hashing::blake2_256(&memory[32..]), &memory[0..32]);
		assert_ok!(result);
	}

	// `n`: Input to hash in bytes
	#[benchmark(pov_mode = Measured)]
	fn seal_hash_blake2_128(n: Linear<0, { limits::code::BLOB_BYTES }>) {
		build_runtime!(runtime, memory: [[0u8; 16], vec![0u8; n as usize], ]);

		let result;
		#[block]
		{
			result = runtime.bench_hash_blake2_128(memory.as_mut_slice(), 16, n, 0);
		}
		assert_eq!(sp_io::hashing::blake2_128(&memory[16..]), &memory[0..16]);
		assert_ok!(result);
	}

	// `n`: Message input length to verify in bytes.
	// need some buffer so the code size does not exceed the max code size.
	#[benchmark(pov_mode = Measured)]
	fn seal_sr25519_verify(n: Linear<0, { limits::code::BLOB_BYTES - 255 }>) {
		let message = (0..n).zip((32u8..127u8).cycle()).map(|(_, c)| c).collect::<Vec<_>>();
		let message_len = message.len() as u32;

		let key_type = sp_core::crypto::KeyTypeId(*b"code");
		let pub_key = sp_io::crypto::sr25519_generate(key_type, None);
		let sig =
			sp_io::crypto::sr25519_sign(key_type, &pub_key, &message).expect("Generates signature");
		let sig = AsRef::<[u8; 64]>::as_ref(&sig).to_vec();
		let sig_len = sig.len() as u32;

		build_runtime!(runtime, memory: [sig, pub_key.to_vec(), message, ]);

		let result;
		#[block]
		{
			result = runtime.bench_sr25519_verify(
				memory.as_mut_slice(),
				0,                              // signature_ptr
				sig_len,                        // pub_key_ptr
				message_len,                    // message_len
				sig_len + pub_key.len() as u32, // message_ptr
			);
		}

		assert_eq!(result.unwrap(), ReturnErrorCode::Success);
	}

	#[benchmark(pov_mode = Measured)]
	fn ecdsa_recover() {
		use hex_literal::hex;
		let input = hex!("18c547e4f7b0f325ad1e56f57e26c745b09a3e503d86e00e5255ff7f715d3d1c000000000000000000000000000000000000000000000000000000000000001c73b1693892219d736caba55bdb67216e485557ea6b6af75f37096c9aa6a5a75feeb940b1d03b21e36b0e47e79769f095fe2ab855bd91e3a38756b7d75a9c4549");
		let expected = hex!("000000000000000000000000a94f5374fce5edbc8e2a8697c15331677e6ebf0b");
		let mut call_setup = CallSetup::<T>::default();
		let (mut ext, _) = call_setup.ext();

		let result;

		#[block]
		{
			result = pure_precompiles::ECRecover::execute(ext.gas_meter_mut(), &input);
		}

		assert_eq!(result.unwrap().data, expected);
	}

	#[benchmark(pov_mode = Measured)]
	fn bn128_add() {
		use hex_literal::hex;
		let input = hex!("089142debb13c461f61523586a60732d8b69c5b38a3380a74da7b2961d867dbf2d5fc7bbc013c16d7945f190b232eacc25da675c0eb093fe6b9f1b4b4e107b3625f8c89ea3437f44f8fc8b6bfbb6312074dc6f983809a5e809ff4e1d076dd5850b38c7ced6e4daef9c4347f370d6d8b58f4b1d8dc61a3c59d651a0644a2a27cf");
		let expected = hex!("0a6678fd675aa4d8f0d03a1feb921a27f38ebdcb860cc083653519655acd6d79172fd5b3b2bfdd44e43bcec3eace9347608f9f0a16f1e184cb3f52e6f259cbeb");
		let mut call_setup = CallSetup::<T>::default();
		let (mut ext, _) = call_setup.ext();

		let result;

		#[block]
		{
			result = pure_precompiles::Bn128Add::execute(ext.gas_meter_mut(), &input);
		}

		assert_eq!(result.unwrap().data, expected);
	}

	#[benchmark(pov_mode = Measured)]
	fn bn128_mul() {
		use hex_literal::hex;
		let input = hex!("089142debb13c461f61523586a60732d8b69c5b38a3380a74da7b2961d867dbf2d5fc7bbc013c16d7945f190b232eacc25da675c0eb093fe6b9f1b4b4e107b36ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff");
		let expected = hex!("0bf982b98a2757878c051bfe7eee228b12bc69274b918f08d9fcb21e9184ddc10b17c77cbf3c19d5d27e18cbd4a8c336afb488d0e92c18d56e64dd4ea5c437e6");
		let mut call_setup = CallSetup::<T>::default();
		let (mut ext, _) = call_setup.ext();

		let result;

		#[block]
		{
			result = pure_precompiles::Bn128Mul::execute(ext.gas_meter_mut(), &input);
		}

		assert_eq!(result.unwrap().data, expected);
	}

	// `n`: pairings to perform
	// This is a slow call: We reduce the number of runs to 20 to avoid the benchmark taking too
	// long.
	#[benchmark(pov_mode = Measured)]
	fn bn128_pairing(n: Linear<0, 20>) {
		let input = pure_precompiles::generate_random_ecpairs(n as usize);
		let mut call_setup = CallSetup::<T>::default();
		let (mut ext, _) = call_setup.ext();

		let result;
		#[block]
		{
			result = pure_precompiles::Bn128Pairing::execute(ext.gas_meter_mut(), &input);
		}
		assert_ok!(result);
	}

	// `n`: number of rounds to perform
	#[benchmark(pov_mode = Measured)]
	fn blake2f(n: Linear<0, 1200>) {
		use hex_literal::hex;
		let input = hex!("48c9bdf267e6096a3ba7ca8485ae67bb2bf894fe72f36e3cf1361d5f3af54fa5d182e6ad7f520e511f6c3e2b8c68059b6bbd41fbabd9831f79217e1319cde05b61626300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000001");
		let input = n.to_be_bytes().to_vec().into_iter().chain(input.to_vec()).collect::<Vec<_>>();
		let mut call_setup = CallSetup::<T>::default();
		let (mut ext, _) = call_setup.ext();

		let result;
		#[block]
		{
			result = pure_precompiles::Blake2F::execute(ext.gas_meter_mut(), &input);
		}
		assert_ok!(result);
	}

	// Only calling the function itself for the list of
	// generated different ECDSA keys.
	// This is a slow call: We reduce the number of runs.
	#[benchmark(pov_mode = Measured)]
	fn seal_ecdsa_to_eth_address() {
		let key_type = sp_core::crypto::KeyTypeId(*b"code");
		let pub_key_bytes = sp_io::crypto::ecdsa_generate(key_type, None).0;
		build_runtime!(runtime, memory: [[0u8; 20], pub_key_bytes,]);

		let result;
		#[block]
		{
			result = runtime.bench_ecdsa_to_eth_address(
				memory.as_mut_slice(),
				20, // key_ptr
				0,  // output_ptr
			);
		}

		assert_ok!(result);
		assert_eq!(&memory[..20], runtime.ext().ecdsa_to_eth_address(&pub_key_bytes).unwrap());
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_set_code_hash() -> Result<(), BenchmarkError> {
		let code_hash =
			Contract::<T>::with_index(1, WasmModule::dummy(), vec![])?.info()?.code_hash;

		build_runtime!(runtime, memory: [ code_hash.encode(),]);

		let result;
		#[block]
		{
			result = runtime.bench_set_code_hash(memory.as_mut_slice(), 0);
		}

		assert_ok!(result);
		Ok(())
	}

	// Benchmark the execution of instructions.
	//
	// It benchmarks the absolute worst case by allocating a lot of memory
	// and then accessing it so that each instruction generates two cache misses.
	#[benchmark(pov_mode = Ignored)]
	fn instr(r: Linear<0, 10_000>) {
		use rand::{seq::SliceRandom, SeedableRng};
		use rand_pcg::Pcg64;

		// Ideally, this needs to be bigger than the cache.
		const MEMORY_SIZE: u64 = sp_core::MAX_POSSIBLE_ALLOCATION as u64;

		// This is benchmarked for x86-64.
		const CACHE_LINE_SIZE: u64 = 64;

		// An 8 byte load from this misalignment will reach into the subsequent line.
		const MISALIGNMENT: u64 = 60;

		// We only need one address per cache line.
		// -1 because we skip the first address
		const NUM_ADDRESSES: u64 = (MEMORY_SIZE - MISALIGNMENT) / CACHE_LINE_SIZE - 1;

		assert!(
			u64::from(r) <= NUM_ADDRESSES / 2,
			"If we do too many iterations we run into the risk of loading from warm cache lines",
		);

		let mut setup = CallSetup::<T>::new(WasmModule::instr(true));
		let (mut ext, module) = setup.ext();
		let mut prepared =
			CallSetup::<T>::prepare_call(&mut ext, module, Vec::new(), MEMORY_SIZE as u32);

		assert!(
			u64::from(prepared.aux_data_base()) & (CACHE_LINE_SIZE - 1) == 0,
			"aux data base must be cache aligned"
		);

		// Addresses data will be located inside the aux data.
		let misaligned_base = u64::from(prepared.aux_data_base()) + MISALIGNMENT;

		// Create all possible addresses and shuffle them. This makes sure
		// the accesses are random but no address is accessed more than once.
		// we skip the first address since it is our entry point
		let mut addresses = Vec::with_capacity(NUM_ADDRESSES as usize);
		for i in 1..NUM_ADDRESSES {
			let addr = (misaligned_base + i * CACHE_LINE_SIZE).to_le_bytes();
			addresses.push(addr);
		}
		let mut rng = Pcg64::seed_from_u64(1337);
		addresses.shuffle(&mut rng);

		// The addresses need to be padded to be one cache line apart.
		let mut memory = Vec::with_capacity((NUM_ADDRESSES * CACHE_LINE_SIZE) as usize);
		for address in addresses {
			memory.extend_from_slice(&address);
			memory.resize(memory.len() + CACHE_LINE_SIZE as usize - address.len(), 0);
		}

		// Copies `memory` to `aux_data_base + MISALIGNMENT`.
		// Sets `a0 = MISALIGNMENT` and `a1 = r`.
		prepared
			.setup_aux_data(memory.as_slice(), MISALIGNMENT as u32, r.into())
			.unwrap();

		#[block]
		{
			prepared.call().unwrap();
		}
	}

	#[benchmark(pov_mode = Ignored)]
	fn instr_empty_loop(r: Linear<0, 100_000>) {
		let mut setup = CallSetup::<T>::new(WasmModule::instr(false));
		let (mut ext, module) = setup.ext();
		let mut prepared = CallSetup::<T>::prepare_call(&mut ext, module, Vec::new(), 0);
		prepared.setup_aux_data(&[], 0, r.into()).unwrap();

		#[block]
		{
			prepared.call().unwrap();
		}
	}

	impl_benchmark_test_suite!(
		Contracts,
		crate::tests::ExtBuilder::default().build(),
		crate::tests::Test,
	);
}
