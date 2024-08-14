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
#![cfg(feature = "runtime-benchmarks")]

mod call_builder;
mod code;
mod sandbox;
use self::{
	call_builder::CallSetup,
	code::{body, ImportedMemory, Location, ModuleDefinition, WasmModule},
	sandbox::Sandbox,
};
use crate::{
	exec::{Key, SeedOf},
	migration::{
		codegen::LATEST_MIGRATION_VERSION, v09, v10, v11, v12, v13, v14, v15, v16, MigrationStep,
	},
	wasm::BenchEnv,
	Pallet as Contracts, *,
};
use codec::{Encode, MaxEncodedLen};
use frame_benchmarking::v2::*;
use frame_support::{
	self, assert_ok,
	pallet_prelude::StorageVersion,
	traits::{fungible::InspectHold, Currency},
	weights::{Weight, WeightMeter},
};
use frame_system::RawOrigin;
use pallet_balances;
use pallet_contracts_uapi::{CallFlags, ReturnErrorCode};
use sp_runtime::traits::{Bounded, Hash};
use sp_std::prelude::*;
use wasm_instrument::parity_wasm::elements::{Instruction, Local, ValueType};

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

/// An instantiated and deployed contract.
#[derive(Clone)]
struct Contract<T: Config> {
	caller: T::AccountId,
	account_id: T::AccountId,
	addr: AccountIdLookupOf<T>,
	value: BalanceOf<T>,
}

impl<T> Contract<T>
where
	T: Config + pallet_balances::Config,
	<BalanceOf<T> as HasCompact>::Type: Clone + Eq + PartialEq + Debug + TypeInfo + Encode,
{
	/// Create new contract and use a default account id as instantiator.
	fn new(module: WasmModule<T>, data: Vec<u8>) -> Result<Contract<T>, &'static str> {
		Self::with_index(0, module, data)
	}

	/// Create new contract and use an account id derived from the supplied index as instantiator.
	fn with_index(
		index: u32,
		module: WasmModule<T>,
		data: Vec<u8>,
	) -> Result<Contract<T>, &'static str> {
		Self::with_caller(account("instantiator", index, 0), module, data)
	}

	/// Create new contract and use the supplied `caller` as instantiator.
	fn with_caller(
		caller: T::AccountId,
		module: WasmModule<T>,
		data: Vec<u8>,
	) -> Result<Contract<T>, &'static str> {
		let value = Pallet::<T>::min_balance();
		T::Currency::set_balance(&caller, caller_funding::<T>());
		let salt = vec![0xff];
		let addr = Contracts::<T>::contract_address(&caller, &module.hash, &data, &salt);

		Contracts::<T>::store_code_raw(module.code, caller.clone())?;
		Contracts::<T>::instantiate(
			RawOrigin::Signed(caller.clone()).into(),
			value,
			Weight::MAX,
			None,
			module.hash,
			data,
			salt,
		)?;

		let result =
			Contract { caller, account_id: addr.clone(), addr: T::Lookup::unlookup(addr), value };

		ContractInfoOf::<T>::insert(&result.account_id, result.info()?);

		Ok(result)
	}

	/// Create a new contract with the supplied storage item count and size each.
	fn with_storage(
		code: WasmModule<T>,
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
	fn store(&self, items: &Vec<([u8; 32], Vec<u8>)>) -> Result<(), &'static str> {
		let info = self.info()?;
		for item in items {
			info.write(&Key::Fix(item.0), Some(item.1.clone()), None, false)
				.map_err(|_| "Failed to write storage to restoration dest")?;
		}
		<ContractInfoOf<T>>::insert(&self.account_id, info);
		Ok(())
	}

	/// Get the `ContractInfo` of the `addr` or an error if it no longer exists.
	fn address_info(addr: &T::AccountId) -> Result<ContractInfo<T>, &'static str> {
		ContractInfoOf::<T>::get(addr).ok_or("Expected contract to exist at this point.")
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
	fn code_exists(hash: &CodeHash<T>) -> bool {
		<PristineCode<T>>::contains_key(hash) && <CodeInfoOf<T>>::contains_key(&hash)
	}

	/// Returns `true` iff no storage entry related to code storage exist.
	fn code_removed(hash: &CodeHash<T>) -> bool {
		!<PristineCode<T>>::contains_key(hash) && !<CodeInfoOf<T>>::contains_key(&hash)
	}
}

/// The funding that each account that either calls or instantiates contracts is funded with.
fn caller_funding<T: Config>() -> BalanceOf<T> {
	// Minting can overflow, so we can't abuse of the funding. This value happens to be big enough,
	// but not too big to make the total supply overflow.
	BalanceOf::<T>::max_value() / 10_000u32.into()
}

#[benchmarks(
	where
		<BalanceOf<T> as codec::HasCompact>::Type: Clone + Eq + PartialEq + sp_std::fmt::Debug + scale_info::TypeInfo + codec::Encode,
		T: Config + pallet_balances::Config,
		BalanceOf<T>: From<<pallet_balances::Pallet<T> as Currency<T::AccountId>>::Balance>,
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
		let instance = Contract::<T>::with_storage(
			WasmModule::dummy(),
			k,
			T::Schedule::get().limits.payload_len,
		)?;
		instance.info()?.queue_trie_for_deletion();

		#[block]
		{
			ContractInfo::<T>::process_deletion_queue_batch(&mut WeightMeter::new())
		}

		Ok(())
	}

	// This benchmarks the v9 migration step (update codeStorage).
	#[benchmark(pov_mode = Measured)]
	fn v9_migration_step(c: Linear<0, { T::MaxCodeLen::get() }>) {
		v09::store_old_dummy_code::<T>(c as usize);
		let mut m = v09::Migration::<T>::default();
		#[block]
		{
			m.step(&mut WeightMeter::new());
		}
	}

	// This benchmarks the v10 migration step (use dedicated deposit_account).
	#[benchmark(pov_mode = Measured)]
	fn v10_migration_step() -> Result<(), BenchmarkError> {
		let contract =
			<Contract<T>>::with_caller(whitelisted_caller(), WasmModule::dummy(), vec![])?;

		v10::store_old_contract_info::<T, pallet_balances::Pallet<T>>(
			contract.account_id.clone(),
			contract.info()?,
		);
		let mut m = v10::Migration::<T, pallet_balances::Pallet<T>>::default();

		#[block]
		{
			m.step(&mut WeightMeter::new());
		}

		Ok(())
	}

	// This benchmarks the v11 migration step (Don't rely on reserved balances keeping an account
	// alive).
	#[benchmark(pov_mode = Measured)]
	fn v11_migration_step(k: Linear<0, 1024>) {
		v11::fill_old_queue::<T>(k as usize);
		let mut m = v11::Migration::<T>::default();

		#[block]
		{
			m.step(&mut WeightMeter::new());
		}
	}

	// This benchmarks the v12 migration step (Move `OwnerInfo` to `CodeInfo`,
	// add `determinism` field to the latter, clear `CodeStorage`
	// and repay deposits).
	#[benchmark(pov_mode = Measured)]
	fn v12_migration_step(c: Linear<0, { T::MaxCodeLen::get() }>) {
		v12::store_old_dummy_code::<T, pallet_balances::Pallet<T>>(
			c as usize,
			account::<T::AccountId>("account", 0, 0),
		);
		let mut m = v12::Migration::<T, pallet_balances::Pallet<T>>::default();

		#[block]
		{
			m.step(&mut WeightMeter::new());
		}
	}

	// This benchmarks the v13 migration step (Add delegate_dependencies field).
	#[benchmark(pov_mode = Measured)]
	fn v13_migration_step() -> Result<(), BenchmarkError> {
		let contract =
			<Contract<T>>::with_caller(whitelisted_caller(), WasmModule::dummy(), vec![])?;

		v13::store_old_contract_info::<T>(contract.account_id.clone(), contract.info()?);
		let mut m = v13::Migration::<T>::default();

		#[block]
		{
			m.step(&mut WeightMeter::new());
		}
		Ok(())
	}

	// This benchmarks the v14 migration step (Move code owners' reserved balance to be held
	// instead).
	#[benchmark(pov_mode = Measured)]
	fn v14_migration_step() {
		let account = account::<T::AccountId>("account", 0, 0);
		T::Currency::set_balance(&account, caller_funding::<T>());
		v14::store_dummy_code::<T, pallet_balances::Pallet<T>>(account);
		let mut m = v14::Migration::<T, pallet_balances::Pallet<T>>::default();

		#[block]
		{
			m.step(&mut WeightMeter::new());
		}
	}

	// This benchmarks the v15 migration step (remove deposit account).
	#[benchmark(pov_mode = Measured)]
	fn v15_migration_step() -> Result<(), BenchmarkError> {
		let contract =
			<Contract<T>>::with_caller(whitelisted_caller(), WasmModule::dummy(), vec![])?;

		v15::store_old_contract_info::<T>(contract.account_id.clone(), contract.info()?);
		let mut m = v15::Migration::<T>::default();

		#[block]
		{
			m.step(&mut WeightMeter::new());
		}

		Ok(())
	}

	// This benchmarks the v16 migration step (Remove ED from base_deposit).
	#[benchmark(pov_mode = Measured)]
	fn v16_migration_step() -> Result<(), BenchmarkError> {
		let contract =
			<Contract<T>>::with_caller(whitelisted_caller(), WasmModule::dummy(), vec![])?;

		let info = contract.info()?;
		let base_deposit = v16::store_old_contract_info::<T>(contract.account_id.clone(), &info);
		let mut m = v16::Migration::<T>::default();

		#[block]
		{
			m.step(&mut WeightMeter::new());
		}
		let ed = Pallet::<T>::min_balance();
		let info = v16::ContractInfoOf::<T>::get(&contract.account_id).unwrap();
		assert_eq!(info.storage_base_deposit, base_deposit - ed);
		Ok(())
	}

	// This benchmarks the weight of executing Migration::migrate to execute a noop migration.
	#[benchmark(pov_mode = Measured)]
	fn migration_noop() {
		let version = LATEST_MIGRATION_VERSION;
		StorageVersion::new(version).put::<Pallet<T>>();
		#[block]
		{
			Migration::<T>::migrate(&mut WeightMeter::new());
		}
		assert_eq!(StorageVersion::get::<Pallet<T>>(), version);
	}

	// This benchmarks the weight of dispatching migrate to execute 1 `NoopMigration`
	#[benchmark(pov_mode = Measured)]
	fn migrate() {
		let latest_version = LATEST_MIGRATION_VERSION;
		StorageVersion::new(latest_version - 2).put::<Pallet<T>>();
		<Migration<T, false> as frame_support::traits::OnRuntimeUpgrade>::on_runtime_upgrade();

		#[extrinsic_call]
		_(RawOrigin::Signed(whitelisted_caller()), Weight::MAX);

		assert_eq!(StorageVersion::get::<Pallet<T>>(), latest_version - 1);
	}

	// This benchmarks the weight of running on_runtime_upgrade when there are no migration in
	// progress.
	#[benchmark(pov_mode = Measured)]
	fn on_runtime_upgrade_noop() {
		let latest_version = LATEST_MIGRATION_VERSION;
		StorageVersion::new(latest_version).put::<Pallet<T>>();
		#[block]
		{
			<Migration<T, false> as frame_support::traits::OnRuntimeUpgrade>::on_runtime_upgrade();
		}
		assert!(MigrationInProgress::<T>::get().is_none());
	}

	// This benchmarks the weight of running on_runtime_upgrade when there is a migration in
	// progress.
	#[benchmark(pov_mode = Measured)]
	fn on_runtime_upgrade_in_progress() {
		let latest_version = LATEST_MIGRATION_VERSION;
		StorageVersion::new(latest_version - 2).put::<Pallet<T>>();
		let v = vec![42u8].try_into().ok();
		MigrationInProgress::<T>::set(v.clone());
		#[block]
		{
			<Migration<T, false> as frame_support::traits::OnRuntimeUpgrade>::on_runtime_upgrade();
		}
		assert!(MigrationInProgress::<T>::get().is_some());
		assert_eq!(MigrationInProgress::<T>::get(), v);
	}

	// This benchmarks the weight of running on_runtime_upgrade when there is a migration to
	// process.
	#[benchmark(pov_mode = Measured)]
	fn on_runtime_upgrade() {
		let latest_version = LATEST_MIGRATION_VERSION;
		StorageVersion::new(latest_version - 2).put::<Pallet<T>>();
		#[block]
		{
			<Migration<T, false> as frame_support::traits::OnRuntimeUpgrade>::on_runtime_upgrade();
		}
		assert!(MigrationInProgress::<T>::get().is_some());
	}

	// This benchmarks the overhead of loading a code of size `c` byte from storage and into
	// the sandbox. This does **not** include the actual execution for which the gas meter
	// is responsible. This is achieved by generating all code to the `deploy` function
	// which is in the wasm module but not executed on `call`.
	// The results are supposed to be used as `call_with_code_per_byte(c) -
	// call_with_code_per_byte(0)`.
	#[benchmark(pov_mode = Measured)]
	fn call_with_code_per_byte(
		c: Linear<0, { T::MaxCodeLen::get() }>,
	) -> Result<(), BenchmarkError> {
		let instance = Contract::<T>::with_caller(
			whitelisted_caller(),
			WasmModule::sized(c, Location::Deploy, false),
			vec![],
		)?;
		let value = Pallet::<T>::min_balance();
		let callee = instance.addr;

		#[extrinsic_call]
		call(RawOrigin::Signed(instance.caller.clone()), callee, value, Weight::MAX, None, vec![]);

		Ok(())
	}

	// `c`: Size of the code in bytes.
	// `i`: Size of the input in bytes.
	// `s`: Size of the salt in bytes.
	#[benchmark(pov_mode = Measured)]
	fn instantiate_with_code(
		c: Linear<0, { T::MaxCodeLen::get() }>,
		i: Linear<0, { code::max_pages::<T>() * 64 * 1024 }>,
		s: Linear<0, { code::max_pages::<T>() * 64 * 1024 }>,
	) {
		let input = vec![42u8; i as usize];
		let salt = vec![42u8; s as usize];
		let value = Pallet::<T>::min_balance();
		let caller = whitelisted_caller();
		T::Currency::set_balance(&caller, caller_funding::<T>());
		let WasmModule { code, hash, .. } = WasmModule::<T>::sized(c, Location::Call, false);
		let origin = RawOrigin::Signed(caller.clone());
		let addr = Contracts::<T>::contract_address(&caller, &hash, &input, &salt);
		#[extrinsic_call]
		_(origin, value, Weight::MAX, None, code, input, salt);

		let deposit =
			T::Currency::balance_on_hold(&HoldReason::StorageDepositReserve.into(), &addr);
		// uploading the code reserves some balance in the callers account
		let code_deposit =
			T::Currency::balance_on_hold(&HoldReason::CodeUploadDepositReserve.into(), &caller);
		assert_eq!(
			T::Currency::balance(&caller),
			caller_funding::<T>() - value - deposit - code_deposit - Pallet::<T>::min_balance(),
		);
		// contract has the full value
		assert_eq!(T::Currency::balance(&addr), value + Pallet::<T>::min_balance());
	}

	// `i`: Size of the input in bytes.
	// `s`: Size of the salt in bytes.
	#[benchmark(pov_mode = Measured)]
	fn instantiate(
		i: Linear<0, { code::max_pages::<T>() * 64 * 1024 }>,
		s: Linear<0, { code::max_pages::<T>() * 64 * 1024 }>,
	) -> Result<(), BenchmarkError> {
		let input = vec![42u8; i as usize];
		let salt = vec![42u8; s as usize];
		let value = Pallet::<T>::min_balance();
		let caller = whitelisted_caller();
		T::Currency::set_balance(&caller, caller_funding::<T>());
		let WasmModule { code, hash, .. } = WasmModule::<T>::dummy();
		let addr = Contracts::<T>::contract_address(&caller, &hash, &input, &salt);
		Contracts::<T>::store_code_raw(code, caller.clone())?;

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), value, Weight::MAX, None, hash, input, salt);

		let deposit =
			T::Currency::balance_on_hold(&HoldReason::StorageDepositReserve.into(), &addr);
		// value was removed from the caller
		assert_eq!(
			T::Currency::balance(&caller),
			caller_funding::<T>() - value - deposit - Pallet::<T>::min_balance(),
		);
		// contract has the full value
		assert_eq!(T::Currency::balance(&addr), value + Pallet::<T>::min_balance());

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
		let callee = instance.addr.clone();
		let before = T::Currency::balance(&instance.account_id);
		#[extrinsic_call]
		_(origin, callee, value, Weight::MAX, None, data);
		let deposit = T::Currency::balance_on_hold(
			&HoldReason::StorageDepositReserve.into(),
			&instance.account_id,
		);
		// value and value transferred via call should be removed from the caller
		assert_eq!(
			T::Currency::balance(&instance.caller),
			caller_funding::<T>() - instance.value - value - deposit - Pallet::<T>::min_balance(),
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
	fn upload_code_determinism_enforced(c: Linear<0, { T::MaxCodeLen::get() }>) {
		let caller = whitelisted_caller();
		T::Currency::set_balance(&caller, caller_funding::<T>());
		let WasmModule { code, hash, .. } = WasmModule::<T>::sized(c, Location::Call, false);
		let origin = RawOrigin::Signed(caller.clone());
		#[extrinsic_call]
		upload_code(origin, code, None, Determinism::Enforced);
		// uploading the code reserves some balance in the callers account
		assert!(T::Currency::total_balance_on_hold(&caller) > 0u32.into());
		assert!(<Contract<T>>::code_exists(&hash));
	}

	// Uploading code with [`Determinism::Relaxed`] should be more expensive than uploading code
	// with [`Determinism::Enforced`], as we always try to save the code with
	// [`Determinism::Enforced`] first.
	#[benchmark(pov_mode = Measured)]
	fn upload_code_determinism_relaxed(c: Linear<0, { T::MaxCodeLen::get() }>) {
		let caller = whitelisted_caller();
		T::Currency::set_balance(&caller, caller_funding::<T>());
		let WasmModule { code, hash, .. } = WasmModule::<T>::sized(c, Location::Call, true);
		let origin = RawOrigin::Signed(caller.clone());
		#[extrinsic_call]
		upload_code(origin, code, None, Determinism::Relaxed);
		assert!(T::Currency::total_balance_on_hold(&caller) > 0u32.into());
		assert!(<Contract<T>>::code_exists(&hash));
		// Ensure that the benchmark follows the most expensive path, i.e., the code is saved with
		assert_eq!(CodeInfoOf::<T>::get(&hash).unwrap().determinism(), Determinism::Relaxed);
	}

	// Removing code does not depend on the size of the contract because all the information
	// needed to verify the removal claim (refcount, owner) is stored in a separate storage
	// item (`CodeInfoOf`).
	#[benchmark(pov_mode = Measured)]
	fn remove_code() -> Result<(), BenchmarkError> {
		let caller = whitelisted_caller();
		T::Currency::set_balance(&caller, caller_funding::<T>());
		let WasmModule { code, hash, .. } = WasmModule::<T>::dummy();
		let origin = RawOrigin::Signed(caller.clone());
		let uploaded =
			<Contracts<T>>::bare_upload_code(caller.clone(), code, None, Determinism::Enforced)?;
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
		let WasmModule { code, hash, .. } = <WasmModule<T>>::dummy_with_bytes(128);
		<Contracts<T>>::store_code_raw(code, instance.caller.clone())?;
		let callee = instance.addr.clone();
		assert_ne!(instance.info()?.code_hash, hash);
		#[extrinsic_call]
		_(RawOrigin::Root, callee, hash);
		assert_eq!(instance.info()?.code_hash, hash);
		Ok(())
	}

	#[benchmark(pov_mode = Measured)]
	fn noop_host_fn(r: Linear<0, API_BENCHMARK_RUNS>) {
		let mut setup = CallSetup::<T>::new(WasmModule::noop(r));
		let (mut ext, module) = setup.ext();
		let func = CallSetup::<T>::prepare_call(&mut ext, module, vec![]);
		#[block]
		{
			func.call();
		}
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_caller() {
		let len = <T::AccountId as MaxEncodedLen>::max_encoded_len() as u32;
		build_runtime!(runtime, memory: [len.to_le_bytes(), vec![0u8; len as _], ]);

		let result;
		#[block]
		{
			result = BenchEnv::seal0_caller(&mut runtime, &mut memory, 4, 0);
		}

		assert_ok!(result);
		assert_eq!(
			&<T::AccountId as Decode>::decode(&mut &memory[4..]).unwrap(),
			runtime.ext().caller().account_id().unwrap()
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
			result = BenchEnv::seal0_is_contract(&mut runtime, &mut memory, 0);
		}

		assert_eq!(result.unwrap(), 1);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_code_hash() {
		let contract = Contract::<T>::with_index(1, WasmModule::dummy(), vec![]).unwrap();
		let len = <CodeHash<T> as MaxEncodedLen>::max_encoded_len() as u32;
		build_runtime!(runtime, memory: [len.to_le_bytes(), vec![0u8; len as _], contract.account_id.encode(), ]);

		let result;
		#[block]
		{
			result = BenchEnv::seal0_code_hash(&mut runtime, &mut memory, 4 + len, 4, 0);
		}

		assert_ok!(result);
		assert_eq!(
			<CodeHash<T> as Decode>::decode(&mut &memory[4..]).unwrap(),
			contract.info().unwrap().code_hash
		);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_own_code_hash() {
		let len = <CodeHash<T> as MaxEncodedLen>::max_encoded_len() as u32;
		build_runtime!(runtime, contract, memory: [len.to_le_bytes(), vec![0u8; len as _], ]);
		let result;
		#[block]
		{
			result = BenchEnv::seal0_own_code_hash(&mut runtime, &mut memory, 4, 0);
		}

		assert_ok!(result);
		assert_eq!(
			<CodeHash<T> as Decode>::decode(&mut &memory[4..]).unwrap(),
			contract.info().unwrap().code_hash
		);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_caller_is_origin() {
		build_runtime!(runtime, memory: []);

		let result;
		#[block]
		{
			result = BenchEnv::seal0_caller_is_origin(&mut runtime, &mut memory);
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
			result = BenchEnv::seal0_caller_is_root(&mut runtime, &mut [0u8; 0]);
		}
		assert_eq!(result.unwrap(), 1u32);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_address() {
		let len = <AccountIdOf<T> as MaxEncodedLen>::max_encoded_len() as u32;
		build_runtime!(runtime, memory: [len.to_le_bytes(), vec![0u8; len as _], ]);

		let result;
		#[block]
		{
			result = BenchEnv::seal0_address(&mut runtime, &mut memory, 4, 0);
		}
		assert_ok!(result);
		assert_eq!(
			&<T::AccountId as Decode>::decode(&mut &memory[4..]).unwrap(),
			runtime.ext().address()
		);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_gas_left() {
		// use correct max_encoded_len when new version of parity-scale-codec is released
		let len = 18u32;
		assert!(<Weight as MaxEncodedLen>::max_encoded_len() as u32 != len);
		build_runtime!(runtime, memory: [32u32.to_le_bytes(), vec![0u8; len as _], ]);

		let result;
		#[block]
		{
			result = BenchEnv::seal1_gas_left(&mut runtime, &mut memory, 4, 0);
		}
		assert_ok!(result);
		assert_eq!(
			<Weight as Decode>::decode(&mut &memory[4..]).unwrap(),
			runtime.ext().gas_meter().gas_left()
		);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_balance() {
		let len = <T::Balance as MaxEncodedLen>::max_encoded_len() as u32;
		build_runtime!(runtime, memory: [len.to_le_bytes(), vec![0u8; len as _], ]);
		let result;
		#[block]
		{
			result = BenchEnv::seal0_seal_balance(&mut runtime, &mut memory, 4, 0);
		}
		assert_ok!(result);
		assert_eq!(
			<T::Balance as Decode>::decode(&mut &memory[4..]).unwrap(),
			runtime.ext().balance().into()
		);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_value_transferred() {
		let len = <T::Balance as MaxEncodedLen>::max_encoded_len() as u32;
		build_runtime!(runtime, memory: [len.to_le_bytes(), vec![0u8; len as _], ]);
		let result;
		#[block]
		{
			result = BenchEnv::seal0_value_transferred(&mut runtime, &mut memory, 4, 0);
		}
		assert_ok!(result);
		assert_eq!(
			<T::Balance as Decode>::decode(&mut &memory[4..]).unwrap(),
			runtime.ext().value_transferred().into()
		);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_minimum_balance() {
		let len = <T::Balance as MaxEncodedLen>::max_encoded_len() as u32;
		build_runtime!(runtime, memory: [len.to_le_bytes(), vec![0u8; len as _], ]);
		let result;
		#[block]
		{
			result = BenchEnv::seal0_minimum_balance(&mut runtime, &mut memory, 4, 0);
		}
		assert_ok!(result);
		assert_eq!(
			<T::Balance as Decode>::decode(&mut &memory[4..]).unwrap(),
			runtime.ext().minimum_balance().into()
		);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_block_number() {
		let len = <BlockNumberFor<T> as MaxEncodedLen>::max_encoded_len() as u32;
		build_runtime!(runtime, memory: [len.to_le_bytes(), vec![0u8; len as _], ]);
		let result;
		#[block]
		{
			result = BenchEnv::seal0_seal_block_number(&mut runtime, &mut memory, 4, 0);
		}
		assert_ok!(result);
		assert_eq!(
			<BlockNumberFor<T>>::decode(&mut &memory[4..]).unwrap(),
			runtime.ext().block_number()
		);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_now() {
		let len = <MomentOf<T> as MaxEncodedLen>::max_encoded_len() as u32;
		build_runtime!(runtime, memory: [len.to_le_bytes(), vec![0u8; len as _], ]);
		let result;
		#[block]
		{
			result = BenchEnv::seal0_seal_now(&mut runtime, &mut memory, 4, 0);
		}
		assert_ok!(result);
		assert_eq!(<MomentOf<T>>::decode(&mut &memory[4..]).unwrap(), *runtime.ext().now());
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_weight_to_fee() {
		let len = <T::Balance as MaxEncodedLen>::max_encoded_len() as u32;
		build_runtime!(runtime, memory: [len.to_le_bytes(), vec![0u8; len as _], ]);
		let weight = Weight::from_parts(500_000, 300_000);
		let result;
		#[block]
		{
			result = BenchEnv::seal1_weight_to_fee(
				&mut runtime,
				&mut memory,
				weight.ref_time(),
				weight.proof_size(),
				4,
				0,
			);
		}
		assert_ok!(result);
		assert_eq!(
			<BalanceOf<T>>::decode(&mut &memory[4..]).unwrap(),
			runtime.ext().get_weight_price(weight)
		);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_input(n: Linear<0, { code::max_pages::<T>() * 64 * 1024 - 4 }>) {
		let mut setup = CallSetup::<T>::default();
		let (mut ext, _) = setup.ext();
		let mut runtime = crate::wasm::Runtime::new(&mut ext, vec![42u8; n as usize]);
		let mut memory = memory!(n.to_le_bytes(), vec![0u8; n as usize],);
		let result;
		#[block]
		{
			result = BenchEnv::seal0_input(&mut runtime, &mut memory, 4, 0);
		}
		assert_ok!(result);
		assert_eq!(&memory[4..], &vec![42u8; n as usize]);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_return(n: Linear<0, { code::max_pages::<T>() * 64 * 1024 - 4 }>) {
		build_runtime!(runtime, memory: [n.to_le_bytes(), vec![42u8; n as usize], ]);

		let result;
		#[block]
		{
			result = BenchEnv::seal0_seal_return(&mut runtime, &mut memory, 0, 0, n);
		}

		assert!(matches!(
			result,
			Err(crate::wasm::TrapReason::Return(crate::wasm::ReturnData { .. }))
		));
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_terminate(
		n: Linear<0, { T::MaxDelegateDependencies::get() }>,
	) -> Result<(), BenchmarkError> {
		let beneficiary = account::<T::AccountId>("beneficiary", 0, 0);
		let caller = whitelisted_caller();

		build_runtime!(runtime, memory: [beneficiary.encode(),]);

		T::Currency::set_balance(&caller, caller_funding::<T>());

		(0..n).for_each(|i| {
			let new_code = WasmModule::<T>::dummy_with_bytes(65 + i);
			Contracts::<T>::store_code_raw(new_code.code, caller.clone()).unwrap();
			runtime.ext().lock_delegate_dependency(new_code.hash).unwrap();
		});

		let result;
		#[block]
		{
			result = BenchEnv::seal1_terminate(&mut runtime, &mut memory, 0);
		}

		assert!(matches!(result, Err(crate::wasm::TrapReason::Termination)));

		Ok(())
	}

	// We benchmark only for the maximum subject length. We assume that this is some lowish
	// number (< 1 KB). Therefore we are not overcharging too much in case a smaller subject is
	// used.
	#[benchmark(pov_mode = Measured)]
	fn seal_random() {
		let subject_len = T::Schedule::get().limits.subject_len;
		assert!(subject_len < 1024);

		let output_len =
			<(SeedOf<T>, BlockNumberFor<T>) as MaxEncodedLen>::max_encoded_len() as u32;

		build_runtime!(runtime, memory: [
			output_len.to_le_bytes(),
			vec![42u8; subject_len as _],
			vec![0u8; output_len as _],
		]);

		let result;
		#[block]
		{
			result = BenchEnv::seal0_random(
				&mut runtime,
				&mut memory,
				4,               // subject_ptr
				subject_len,     // subject_len
				subject_len + 4, // output_ptr
				0,               // output_len_ptr
			);
		}

		assert_ok!(result);
		assert_ok!(<(SeedOf<T>, BlockNumberFor<T>)>::decode(&mut &memory[subject_len as _..]));
	}

	// Benchmark the overhead that topics generate.
	// `t`: Number of topics
	// `n`: Size of event payload in bytes
	#[benchmark(pov_mode = Measured)]
	fn seal_deposit_event(
		t: Linear<0, { T::Schedule::get().limits.event_topics }>,
		n: Linear<0, { T::Schedule::get().limits.payload_len }>,
	) {
		let topics = (0..t).map(|i| T::Hashing::hash_of(&i)).collect::<Vec<_>>().encode();
		let topics_len = topics.len() as u32;

		build_runtime!(runtime, memory: [
			n.to_le_bytes(),
			topics,
			vec![0u8; n as _],
		]);

		let result;
		#[block]
		{
			result = BenchEnv::seal0_deposit_event(
				&mut runtime,
				&mut memory,
				4,              // topics_ptr
				topics_len,     // topics_len
				4 + topics_len, // data_ptr
				0,              // data_len
			);
		}

		assert_ok!(result);
	}

	// Benchmark debug_message call
	// Whereas this function is used in RPC mode only, it still should be secured
	// against an excessive use.
	//
	// i: size of input in bytes up to maximum allowed contract memory or maximum allowed debug
	// buffer size, whichever is less.
	#[benchmark]
	fn seal_debug_message(
		i: Linear<
			0,
			{
				(T::Schedule::get().limits.memory_pages * 64 * 1024)
					.min(T::MaxDebugBufferLen::get())
			},
		>,
	) {
		let mut setup = CallSetup::<T>::default();
		setup.enable_debug_message();
		let (mut ext, _) = setup.ext();
		let mut runtime = crate::wasm::Runtime::new(&mut ext, vec![]);
		// Fill memory with printable ASCII bytes.
		let mut memory = (0..i).zip((32..127).cycle()).map(|i| i.1).collect::<Vec<_>>();

		let result;
		#[block]
		{
			result = BenchEnv::seal0_debug_message(&mut runtime, &mut memory, 0, i);
		}
		assert_ok!(result);
		assert_eq!(setup.debug_message().unwrap().len() as u32, i);
	}

	// n: new byte size
	// o: old byte size
	#[benchmark(skip_meta, pov_mode = Measured)]
	fn seal_set_storage(
		n: Linear<0, { T::Schedule::get().limits.payload_len }>,
		o: Linear<0, { T::Schedule::get().limits.payload_len }>,
	) -> Result<(), BenchmarkError> {
		let max_key_len = T::MaxStorageKeyLen::get();
		let key = Key::<T>::try_from_var(vec![0u8; max_key_len as usize])
			.map_err(|_| "Key has wrong length")?;
		let value = vec![1u8; n as usize];

		build_runtime!(runtime, instance, memory: [ key.to_vec(), value.clone(), ]);
		let info = instance.info()?;

		info.write(&key, Some(vec![42u8; o as usize]), None, false)
			.map_err(|_| "Failed to write to storage during setup.")?;

		let result;
		#[block]
		{
			result = BenchEnv::seal2_set_storage(
				&mut runtime,
				&mut memory,
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
	fn seal_clear_storage(
		n: Linear<0, { T::Schedule::get().limits.payload_len }>,
	) -> Result<(), BenchmarkError> {
		let max_key_len = T::MaxStorageKeyLen::get();
		let key = Key::<T>::try_from_var(vec![0u8; max_key_len as usize])
			.map_err(|_| "Key has wrong length")?;
		build_runtime!(runtime, instance, memory: [ key.to_vec(), ]);
		let info = instance.info()?;

		info.write(&key, Some(vec![42u8; n as usize]), None, false)
			.map_err(|_| "Failed to write to storage during setup.")?;

		let result;
		#[block]
		{
			result = BenchEnv::seal1_clear_storage(&mut runtime, &mut memory, 0, max_key_len);
		}

		assert_ok!(result);
		assert!(info.read(&key).is_none());
		Ok(())
	}

	#[benchmark(skip_meta, pov_mode = Measured)]
	fn seal_get_storage(
		n: Linear<0, { T::Schedule::get().limits.payload_len }>,
	) -> Result<(), BenchmarkError> {
		let max_key_len = T::MaxStorageKeyLen::get();
		let key = Key::<T>::try_from_var(vec![0u8; max_key_len as usize])
			.map_err(|_| "Key has wrong length")?;
		build_runtime!(runtime, instance, memory: [ key.to_vec(), n.to_le_bytes(), vec![0u8; n as _], ]);
		let info = instance.info()?;

		info.write(&key, Some(vec![42u8; n as usize]), None, false)
			.map_err(|_| "Failed to write to storage during setup.")?;

		let out_ptr = max_key_len + 4;
		let result;
		#[block]
		{
			result = BenchEnv::seal1_get_storage(
				&mut runtime,
				&mut memory,
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
		n: Linear<0, { T::Schedule::get().limits.payload_len }>,
	) -> Result<(), BenchmarkError> {
		let max_key_len = T::MaxStorageKeyLen::get();
		let key = Key::<T>::try_from_var(vec![0u8; max_key_len as usize])
			.map_err(|_| "Key has wrong length")?;
		build_runtime!(runtime, instance, memory: [ key.to_vec(), ]);
		let info = instance.info()?;

		info.write(&key, Some(vec![42u8; n as usize]), None, false)
			.map_err(|_| "Failed to write to storage during setup.")?;

		let result;
		#[block]
		{
			result = BenchEnv::seal1_contains_storage(&mut runtime, &mut memory, 0, max_key_len);
		}

		assert_eq!(result.unwrap(), n);
		Ok(())
	}

	#[benchmark(skip_meta, pov_mode = Measured)]
	fn seal_take_storage(
		n: Linear<0, { T::Schedule::get().limits.payload_len }>,
	) -> Result<(), BenchmarkError> {
		let max_key_len = T::MaxStorageKeyLen::get();
		let key = Key::<T>::try_from_var(vec![0u8; max_key_len as usize])
			.map_err(|_| "Key has wrong length")?;
		build_runtime!(runtime, instance, memory: [ key.to_vec(), n.to_le_bytes(), vec![0u8; n as _], ]);
		let info = instance.info()?;

		let value = vec![42u8; n as usize];
		info.write(&key, Some(value.clone()), None, false)
			.map_err(|_| "Failed to write to storage during setup.")?;

		let out_ptr = max_key_len + 4;
		let result;
		#[block]
		{
			result = BenchEnv::seal0_take_storage(
				&mut runtime,
				&mut memory,
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

	// We transfer to unique accounts.
	#[benchmark(pov_mode = Measured)]
	fn seal_transfer() {
		let account = account::<T::AccountId>("receiver", 0, 0);
		let value = Pallet::<T>::min_balance();
		assert!(value > 0u32.into());

		let mut setup = CallSetup::<T>::default();
		setup.set_balance(value);
		let (mut ext, _) = setup.ext();
		let mut runtime = crate::wasm::Runtime::new(&mut ext, vec![]);

		let account_bytes = account.encode();
		let account_len = account_bytes.len() as u32;
		let value_bytes = value.encode();
		let value_len = value_bytes.len() as u32;
		let mut memory = memory!(account_bytes, value_bytes,);

		let result;
		#[block]
		{
			result = BenchEnv::seal0_transfer(
				&mut runtime,
				&mut memory,
				0, // account_ptr
				account_len,
				account_len,
				value_len,
			);
		}

		assert_ok!(result);
	}

	// t: with or without some value to transfer
	// i: size of the input data
	#[benchmark(pov_mode = Measured)]
	fn seal_call(t: Linear<0, 1>, i: Linear<0, { code::max_pages::<T>() * 64 * 1024 }>) {
		let Contract { account_id: callee, .. } =
			Contract::<T>::with_index(1, WasmModule::dummy(), vec![]).unwrap();
		let callee_bytes = callee.encode();
		let callee_len = callee_bytes.len() as u32;

		let value: BalanceOf<T> = t.into();
		let value_bytes = value.encode();

		let deposit: BalanceOf<T> = (u32::MAX - 100).into();
		let deposit_bytes = deposit.encode();
		let deposit_len = deposit_bytes.len() as u32;

		let mut setup = CallSetup::<T>::default();
		setup.set_storage_deposit_limit(deposit);
		setup.set_data(vec![42; i as usize]);
		setup.set_origin(Origin::from_account_id(setup.contract().account_id.clone()));

		let (mut ext, _) = setup.ext();
		let mut runtime = crate::wasm::Runtime::new(&mut ext, vec![]);
		let mut memory = memory!(callee_bytes, deposit_bytes, value_bytes,);

		let result;
		#[block]
		{
			result = BenchEnv::seal2_call(
				&mut runtime,
				&mut memory,
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
		let mut runtime = crate::wasm::Runtime::new(&mut ext, vec![]);
		let mut memory = memory!(hash.encode(),);

		let result;
		#[block]
		{
			result = BenchEnv::seal0_delegate_call(
				&mut runtime,
				&mut memory,
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
	// s: size of salt in bytes
	#[benchmark(pov_mode = Measured)]
	fn seal_instantiate(
		t: Linear<0, 1>,
		i: Linear<0, { (code::max_pages::<T>() - 1) * 64 * 1024 }>,
		s: Linear<0, { (code::max_pages::<T>() - 1) * 64 * 1024 }>,
	) -> Result<(), BenchmarkError> {
		let hash = Contract::<T>::with_index(1, WasmModule::dummy(), vec![])?.info()?.code_hash;
		let hash_bytes = hash.encode();
		let hash_len = hash_bytes.len() as u32;

		let value: BalanceOf<T> = t.into();
		let value_bytes = value.encode();
		let value_len = value_bytes.len() as u32;

		let deposit: BalanceOf<T> = 0u32.into();
		let deposit_bytes = deposit.encode();
		let deposit_len = deposit_bytes.len() as u32;

		let mut setup = CallSetup::<T>::default();
		setup.set_origin(Origin::from_account_id(setup.contract().account_id.clone()));
		setup.set_balance(value + (Pallet::<T>::min_balance() * 2u32.into()));

		let account_id = &setup.contract().account_id.clone();
		let (mut ext, _) = setup.ext();
		let mut runtime = crate::wasm::Runtime::new(&mut ext, vec![]);

		let input = vec![42u8; i as _];
		let salt = vec![42u8; s as _];
		let addr = Contracts::<T>::contract_address(&account_id, &hash, &input, &salt);
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
			result = BenchEnv::seal2_instantiate(
				&mut runtime,
				&mut memory,
				0,                   // code_hash_ptr
				0,                   // ref_time_limit
				0,                   // proof_size_limit
				offset(hash_len),    // deposit_ptr
				offset(deposit_len), // value_ptr
				offset(value_len),   // input_data_ptr
				i,                   // input_data_len
				SENTINEL,            // address_ptr
				0,                   // address_len_ptr
				SENTINEL,            // output_ptr
				0,                   // output_len_ptr
				offset(i),           // salt_ptr
				s,                   // salt_len
			);
		}

		assert_ok!(result);
		assert!(ContractInfoOf::<T>::get(&addr).is_some());
		Ok(())
	}

	// `n`: Input to hash in bytes
	#[benchmark(pov_mode = Measured)]
	fn seal_hash_sha2_256(n: Linear<0, { code::max_pages::<T>() * 64 * 1024 }>) {
		build_runtime!(runtime, memory: [[0u8; 32], vec![0u8; n as usize], ]);

		let result;
		#[block]
		{
			result = BenchEnv::seal0_hash_sha2_256(&mut runtime, &mut memory, 32, n, 0);
		}
		assert_eq!(sp_io::hashing::sha2_256(&memory[32..]), &memory[0..32]);
		assert_ok!(result);
	}

	// `n`: Input to hash in bytes
	#[benchmark(pov_mode = Measured)]
	fn seal_hash_keccak_256(n: Linear<0, { code::max_pages::<T>() * 64 * 1024 }>) {
		build_runtime!(runtime, memory: [[0u8; 32], vec![0u8; n as usize], ]);

		let result;
		#[block]
		{
			result = BenchEnv::seal0_hash_keccak_256(&mut runtime, &mut memory, 32, n, 0);
		}
		assert_eq!(sp_io::hashing::keccak_256(&memory[32..]), &memory[0..32]);
		assert_ok!(result);
	}

	// `n`: Input to hash in bytes
	#[benchmark(pov_mode = Measured)]
	fn seal_hash_blake2_256(n: Linear<0, { code::max_pages::<T>() * 64 * 1024 }>) {
		build_runtime!(runtime, memory: [[0u8; 32], vec![0u8; n as usize], ]);

		let result;
		#[block]
		{
			result = BenchEnv::seal0_hash_blake2_256(&mut runtime, &mut memory, 32, n, 0);
		}
		assert_eq!(sp_io::hashing::blake2_256(&memory[32..]), &memory[0..32]);
		assert_ok!(result);
	}

	// `n`: Input to hash in bytes
	#[benchmark(pov_mode = Measured)]
	fn seal_hash_blake2_128(n: Linear<0, { code::max_pages::<T>() * 64 * 1024 }>) {
		build_runtime!(runtime, memory: [[0u8; 16], vec![0u8; n as usize], ]);

		let result;
		#[block]
		{
			result = BenchEnv::seal0_hash_blake2_128(&mut runtime, &mut memory, 16, n, 0);
		}
		assert_eq!(sp_io::hashing::blake2_128(&memory[16..]), &memory[0..16]);
		assert_ok!(result);
	}

	// `n`: Message input length to verify in bytes.
	// need some buffer so the code size does not exceed the max code size.
	#[benchmark(pov_mode = Measured)]
	fn seal_sr25519_verify(n: Linear<0, { T::MaxCodeLen::get() - 255 }>) {
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
			result = BenchEnv::seal0_sr25519_verify(
				&mut runtime,
				&mut memory,
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
			result = BenchEnv::seal0_ecdsa_recover(
				&mut runtime,
				&mut memory,
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
			result = BenchEnv::seal0_ecdsa_to_eth_address(
				&mut runtime,
				&mut memory,
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
			result = BenchEnv::seal0_set_code_hash(&mut runtime, &mut memory, 0);
		}

		assert_ok!(result);
		Ok(())
	}

	#[benchmark(pov_mode = Measured)]
	fn lock_delegate_dependency() -> Result<(), BenchmarkError> {
		let code_hash = Contract::<T>::with_index(1, WasmModule::dummy_with_bytes(1), vec![])?
			.info()?
			.code_hash;

		build_runtime!(runtime, memory: [ code_hash.encode(),]);

		let result;
		#[block]
		{
			result = BenchEnv::seal0_lock_delegate_dependency(&mut runtime, &mut memory, 0);
		}

		assert_ok!(result);
		Ok(())
	}

	#[benchmark]
	fn unlock_delegate_dependency() -> Result<(), BenchmarkError> {
		let code_hash = Contract::<T>::with_index(1, WasmModule::dummy_with_bytes(1), vec![])?
			.info()?
			.code_hash;

		build_runtime!(runtime, memory: [ code_hash.encode(),]);
		BenchEnv::seal0_lock_delegate_dependency(&mut runtime, &mut memory, 0).unwrap();

		let result;
		#[block]
		{
			result = BenchEnv::seal0_unlock_delegate_dependency(&mut runtime, &mut memory, 0);
		}

		assert_ok!(result);
		Ok(())
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_reentrance_count() {
		build_runtime!(runtime, memory: []);
		let result;
		#[block]
		{
			result = BenchEnv::seal0_reentrance_count(&mut runtime, &mut memory)
		}

		assert_eq!(result.unwrap(), 0);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_account_reentrance_count() {
		let Contract { account_id, .. } =
			Contract::<T>::with_index(1, WasmModule::dummy(), vec![]).unwrap();
		build_runtime!(runtime, memory: [account_id.encode(),]);

		let result;
		#[block]
		{
			result = BenchEnv::seal0_account_reentrance_count(&mut runtime, &mut memory, 0);
		}

		assert_eq!(result.unwrap(), 0);
	}

	#[benchmark(pov_mode = Measured)]
	fn seal_instantiation_nonce() {
		build_runtime!(runtime, memory: []);

		let result;
		#[block]
		{
			result = BenchEnv::seal0_instantiation_nonce(&mut runtime, &mut memory);
		}

		assert_eq!(result.unwrap(), 1);
	}

	// We load `i64` values from random linear memory locations and store the loaded
	// values back into yet another random linear memory location.
	// The random addresses are uniformly distributed across the entire span of the linear memory.
	// We do this to enforce random memory accesses which are particularly expensive.
	//
	// The combination of this computation is our weight base `w_base`.
	#[benchmark(pov_mode = Ignored)]
	fn instr_i64_load_store(r: Linear<0, INSTR_BENCHMARK_RUNS>) -> Result<(), BenchmarkError> {
		use rand::prelude::*;

		// We do not need to be secure here. Fixed seed allows for deterministic results.
		let mut rng = rand_pcg::Pcg32::seed_from_u64(8446744073709551615);

		let memory = ImportedMemory::max::<T>();
		let bytes_per_page = 65536;
		let bytes_per_memory = memory.max_pages * bytes_per_page;
		let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
			memory: Some(memory),
			call_body: Some(body::repeated_with_locals_using(
				&[Local::new(1, ValueType::I64)],
				r,
				|| {
					// Instruction sequence to load a `i64` from linear memory
					// at a random memory location and store it back into another
					// location of the linear memory.
					let c0: i32 = rng.gen_range(0..bytes_per_memory as i32);
					let c1: i32 = rng.gen_range(0..bytes_per_memory as i32);
					[
						Instruction::I32Const(c0), // address for `i64.load_8s`
						Instruction::I64Load8S(0, 0),
						Instruction::SetLocal(0),  /* temporarily store value loaded in
						                            * `i64.load_8s` */
						Instruction::I32Const(c1), // address for `i64.store8`
						Instruction::GetLocal(0),  // value to be stores in `i64.store8`
						Instruction::I64Store8(0, 0),
					]
				},
			)),
			..Default::default()
		}));
		#[block]
		{
			sbox.invoke();
		}
		Ok(())
	}

	// This is no benchmark. It merely exist to have an easy way to pretty print the currently
	// configured `Schedule` during benchmark development. Check the README on how to print this.
	#[benchmark(extra, pov_mode = Ignored)]
	fn print_schedule() -> Result<(), BenchmarkError> {
		let max_weight = <T as frame_system::Config>::BlockWeights::get().max_block;
		let (weight_per_key, key_budget) =
			ContractInfo::<T>::deletion_budget(&mut WeightMeter::with_limit(max_weight));
		let schedule = T::Schedule::get();
		log::info!(target: LOG_TARGET, "
		{schedule:#?}
		###############################################
		Lazy deletion weight per key: {weight_per_key}
		Lazy deletion keys per block: {key_budget}
		");
		#[block]
		{}

		Err(BenchmarkError::Skip)
	}

	impl_benchmark_test_suite!(
		Contracts,
		crate::tests::ExtBuilder::default().build(),
		crate::tests::Test,
	);
}
