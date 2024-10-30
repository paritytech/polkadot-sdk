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

//! Benchmarks for the contracts pallet

#![cfg(all(feature = "runtime-benchmarks", feature = "riscv"))]

mod call_builder;
mod code;
use self::{call_builder::CallSetup, code::WasmModule};
use crate::{
	exec::{Key, MomentOf},
	limits,
	storage::WriteOutcome,
	Pallet as Contracts, *,
};
use alloc::{vec, vec::Vec};
use codec::{Encode, MaxEncodedLen};
use frame_benchmarking::v2::*;
use frame_support::{
	self, assert_ok,
	storage::child,
	traits::{fungible::InspectHold, Currency},
	weights::{Weight, WeightMeter},
};
use frame_system::RawOrigin;
use pallet_balances;
use pallet_revive_uapi::{CallFlags, ReturnErrorCode, StorageFlags};
use sp_runtime::traits::{Bounded, Hash};

/// How many runs we do per API benchmark.
///
/// This is picked more or less arbitrary. We experimented with different numbers until
/// the results appeared to be stable. Reducing the number would speed up the benchmarks
/// but might make the results less precise.
const API_BENCHMARK_RUNS: u32 = 1600;

/// How many runs we do per instruction benchmark.
///
/// Same rationale as for [`API_BENCHMARK_RUNS`]. The number is bigger because instruction
/// benchmarks are faster.
const INSTR_BENCHMARK_RUNS: u32 = 5000;

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
	T: Config + pallet_balances::Config,
	BalanceOf<T>: Into<U256> + TryFrom<U256>,
	MomentOf<T>: Into<U256>,
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
			default_deposit_limit::<T>(),
			Code::Upload(module.code),
			data,
			salt,
			DebugInfo::Skip,
			CollectEvents::Skip,
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
					continue
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
		T: Config + pallet_balances::Config,
		MomentOf<T>: Into<U256>,
		<T as frame_system::Config>::RuntimeEvent: From<pallet::Event<T>>,
		<T as Config>::RuntimeCall: From<frame_system::Call<T>>,
		<pallet_balances::Pallet<T> as Currency<T::AccountId>>::Balance: From<BalanceOf<T>>,
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
	// the execution engine. This does **not** include the actual execution for which the gas meter
	// is responsible. This is achieved by generating all code to the `deploy` function
	// which is in the wasm module but not executed on `call`.
	// The results are supposed to be used as `call_with_code_per_byte(c) -
	// call_with_code_per_byte(0)`.
	#[benchmark(pov_mode = Measured)]
	fn call_with_code_per_byte(
		c: Linear<0, { limits::code::BLOB_BYTES }>,
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

	// `c`: Size of the code in bytes.
	// `i`: Size of the input in bytes.
	#[benchmark(pov_mode = Measured)]
	fn instantiate_with_code(
		c: Linear<0, { limits::code::BLOB_BYTES }>,
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
	// won't call `seal_input` in its constructor to copy the data to contract memory.
	// The dummy contract used here does not do this. The costs for the data copy is billed as
	// part of `seal_input`. The costs for invoking a contract of a specific size are not part
	// of this benchmark because we cannot know the size of the contract when issuing a call
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
	fn upload_code(c: Linear<0, { limits::code::BLOB_BYTES }>) {
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
		let prepared = CallSetup::<T>::prepare_call(&mut ext, module, r.encode());
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
		ext.override_export(crate::debug::ExportedFunction::Constructor);

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
	fn seal_input(n: Linear<0, { limits::code::BLOB_BYTES - 4 }>) {
		let mut setup = CallSetup::<T>::default();
		let (mut ext, _) = setup.ext();
		let mut runtime = crate::wasm::Runtime::new(&mut ext, vec![42u8; n as usize]);
		let mut memory = memory!(n.to_le_bytes(), vec![0u8; n as usize],);
		let result;
		#[block]
		{
			result = runtime.bench_input(memory.as_mut_slice(), 4, 0);
		}
		assert_ok!(result);
		assert_eq!(&memory[4..], &vec![42u8; n as usize]);
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
	fn seal_terminate(
		n: Linear<0, { limits::DELEGATE_DEPENDENCIES }>,
	) -> Result<(), BenchmarkError> {
		let beneficiary = account::<T::AccountId>("beneficiary", 0, 0);
		let caller = whitelisted_caller();
		T::Currency::set_balance(&caller, caller_funding::<T>());
		let origin = RawOrigin::Signed(caller);
		let storage_deposit = default_deposit_limit::<T>();

		build_runtime!(runtime, memory: [beneficiary.encode(),]);

		(0..n).for_each(|i| {
			let new_code = WasmModule::dummy_unique(65 + i);
			Contracts::<T>::bare_upload_code(origin.clone().into(), new_code.code, storage_deposit)
				.unwrap();
			runtime.ext().lock_delegate_dependency(new_code.hash).unwrap();
		});

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

	// Benchmark debug_message call
	// Whereas this function is used in RPC mode only, it still should be secured
	// against an excessive use.
	//
	// i: size of input in bytes up to maximum allowed contract memory or maximum allowed debug
	// buffer size, whichever is less.
	#[benchmark]
	fn seal_debug_message(
		i: Linear<0, { (limits::code::BLOB_BYTES).min(limits::DEBUG_BUFFER_BYTES) }>,
	) {
		let mut setup = CallSetup::<T>::default();
		setup.enable_debug_message();
		let (mut ext, _) = setup.ext();
		let mut runtime = crate::wasm::Runtime::<_, [u8]>::new(&mut ext, vec![]);
		// Fill memory with printable ASCII bytes.
		let mut memory = (0..i).zip((32..127).cycle()).map(|i| i.1).collect::<Vec<_>>();

		let result;
		#[block]
		{
			result = runtime.bench_debug_message(memory.as_mut_slice(), 0, i);
		}
		assert_ok!(result);
		assert_eq!(setup.debug_message().unwrap().len() as u32, i);
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

	// We transfer to unique accounts.
	#[benchmark(pov_mode = Measured)]
	fn seal_transfer() {
		let account = account::<T::AccountId>("receiver", 0, 0);
		let value = Pallet::<T>::min_balance();
		assert!(value > 0u32.into());

		let mut setup = CallSetup::<T>::default();
		setup.set_balance(value);
		let (mut ext, _) = setup.ext();
		let mut runtime = crate::wasm::Runtime::<_, [u8]>::new(&mut ext, vec![]);

		let account_bytes = account.encode();
		let account_len = account_bytes.len() as u32;
		let value_bytes = Into::<U256>::into(value).encode();
		let mut memory = memory!(account_bytes, value_bytes,);

		let result;
		#[block]
		{
			result = runtime.bench_transfer(
				memory.as_mut_slice(),
				0,           // account_ptr
				account_len, // value_ptr
			);
		}

		assert_ok!(result);
	}

	// t: with or without some value to transfer
	// i: size of the input data
	#[benchmark(pov_mode = Measured)]
	fn seal_call(t: Linear<0, 1>, i: Linear<0, { limits::code::BLOB_BYTES }>) {
		let Contract { account_id: callee, .. } =
			Contract::<T>::with_index(1, WasmModule::dummy(), vec![]).unwrap();
		let callee_bytes = callee.encode();
		let callee_len = callee_bytes.len() as u32;

		let value: BalanceOf<T> = t.into();
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
				CallFlags::CLONE_INPUT.bits(), // flags
				0,                             // callee_ptr
				0,                             // ref_time_limit
				0,                             // proof_size_limit
				callee_len,                    // deposit_ptr
				callee_len + deposit_len,      // value_ptr
				0,                             // input_data_ptr
				0,                             // input_data_len
				SENTINEL,                      // output_ptr
				0,                             // output_len_ptr
			);
		}

		assert_ok!(result);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_delegate_call() -> Result<(), BenchmarkError> {
		let hash = Contract::<T>::with_index(1, WasmModule::dummy(), vec![])?.info()?.code_hash;

		let mut setup = CallSetup::<T>::default();
		setup.set_origin(Origin::from_account_id(setup.contract().account_id.clone()));

		let (mut ext, _) = setup.ext();
		let mut runtime = crate::wasm::Runtime::<_, [u8]>::new(&mut ext, vec![]);
		let mut memory = memory!(hash.encode(),);

		let result;
		#[block]
		{
			result = runtime.bench_delegate_call(
				memory.as_mut_slice(),
				0,        // flags
				0,        // code_hash_ptr
				0,        // input_data_ptr
				0,        // input_data_len
				SENTINEL, // output_ptr
				0,
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
		let hash_len = hash_bytes.len() as u32;

		let value: BalanceOf<T> = 1u32.into();
		let value_bytes = Into::<U256>::into(value).encode();
		let value_len = value_bytes.len() as u32;

		let deposit: BalanceOf<T> = 0u32.into();
		let deposit_bytes = Into::<U256>::into(deposit).encode();
		let deposit_len = deposit_bytes.len() as u32;

		let mut setup = CallSetup::<T>::default();
		setup.set_origin(Origin::from_account_id(setup.contract().account_id.clone()));
		setup.set_balance(value + (Pallet::<T>::min_balance() * 2u32.into()));

		let account_id = &setup.contract().account_id.clone();
		let (mut ext, _) = setup.ext();
		let mut runtime = crate::wasm::Runtime::<_, [u8]>::new(&mut ext, vec![]);

		let input = vec![42u8; i as _];
		let salt = [42u8; 32];
		let deployer = T::AddressMapper::to_address(&account_id);
		let addr = crate::address::create2(&deployer, &code.code, &input, &salt);
		let account_id = T::AddressMapper::to_fallback_account_id(&addr);
		let mut memory = memory!(hash_bytes, deposit_bytes, value_bytes, input, salt,);

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
				0,                   // code_hash_ptr
				0,                   // ref_time_limit
				0,                   // proof_size_limit
				offset(hash_len),    // deposit_ptr
				offset(deposit_len), // value_ptr
				offset(value_len),   // input_data_ptr
				i,                   // input_data_len
				SENTINEL,            // address_ptr
				SENTINEL,            // output_ptr
				0,                   // output_len_ptr
				offset(i),           // salt_ptr
			);
		}

		assert_ok!(result);
		assert!(ContractInfoOf::<T>::get(&addr).is_some());
		assert_eq!(T::Currency::balance(&account_id), Pallet::<T>::min_balance() + value);
		Ok(())
	}

	// `n`: Input to hash in bytes
	#[benchmark(pov_mode = Measured)]
	fn seal_hash_sha2_256(n: Linear<0, { limits::code::BLOB_BYTES }>) {
		build_runtime!(runtime, memory: [[0u8; 32], vec![0u8; n as usize], ]);

		let result;
		#[block]
		{
			result = runtime.bench_hash_sha2_256(memory.as_mut_slice(), 32, n, 0);
		}
		assert_eq!(sp_io::hashing::sha2_256(&memory[32..]), &memory[0..32]);
		assert_ok!(result);
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
	fn seal_ecdsa_recover() {
		let message_hash = sp_io::hashing::blake2_256("Hello world".as_bytes());
		let key_type = sp_core::crypto::KeyTypeId(*b"code");
		let signature = {
			let pub_key = sp_io::crypto::ecdsa_generate(key_type, None);
			let sig = sp_io::crypto::ecdsa_sign_prehashed(key_type, &pub_key, &message_hash)
				.expect("Generates signature");
			AsRef::<[u8; 65]>::as_ref(&sig).to_vec()
		};

		build_runtime!(runtime, memory: [signature, message_hash, [0u8; 33], ]);

		let result;
		#[block]
		{
			result = runtime.bench_ecdsa_recover(
				memory.as_mut_slice(),
				0,       // signature_ptr
				65,      // message_hash_ptr
				65 + 32, // output_ptr
			);
		}

		assert_eq!(result.unwrap(), ReturnErrorCode::Success);
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

	#[benchmark(pov_mode = Measured)]
	fn lock_delegate_dependency() -> Result<(), BenchmarkError> {
		let code_hash = Contract::<T>::with_index(1, WasmModule::dummy_unique(1), vec![])?
			.info()?
			.code_hash;

		build_runtime!(runtime, memory: [ code_hash.encode(),]);

		let result;
		#[block]
		{
			result = runtime.bench_lock_delegate_dependency(memory.as_mut_slice(), 0);
		}

		assert_ok!(result);
		Ok(())
	}

	#[benchmark]
	fn unlock_delegate_dependency() -> Result<(), BenchmarkError> {
		let code_hash = Contract::<T>::with_index(1, WasmModule::dummy_unique(1), vec![])?
			.info()?
			.code_hash;

		build_runtime!(runtime, memory: [ code_hash.encode(),]);
		runtime.bench_lock_delegate_dependency(memory.as_mut_slice(), 0).unwrap();

		let result;
		#[block]
		{
			result = runtime.bench_unlock_delegate_dependency(memory.as_mut_slice(), 0);
		}

		assert_ok!(result);
		Ok(())
	}

	// Benchmark the execution of instructions.
	#[benchmark(pov_mode = Ignored)]
	fn instr(r: Linear<0, INSTR_BENCHMARK_RUNS>) {
		let mut setup = CallSetup::<T>::new(WasmModule::instr());
		let (mut ext, module) = setup.ext();
		let prepared = CallSetup::<T>::prepare_call(&mut ext, module, r.encode());
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
