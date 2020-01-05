// Copyright 2018-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate. If not, see <http://www.gnu.org/licenses/>.

//! This module provides a means for executing contracts
//! represented in wasm.

use crate::{CodeHash, Schedule, Trait};
use crate::wasm::env_def::FunctionImplProvider;
use crate::exec::{Ext, ExecResult};
use crate::gas::GasMeter;

use sp_std::prelude::*;
use codec::{Encode, Decode};
use sp_sandbox;

#[macro_use]
mod env_def;
mod code_cache;
mod prepare;
mod runtime;

use self::runtime::{to_execution_result, Runtime};
use self::code_cache::load as load_code;

pub use self::code_cache::save as save_code;

/// A prepared wasm module ready for execution.
#[derive(Clone, Encode, Decode)]
pub struct PrefabWasmModule {
	/// Version of the schedule with which the code was instrumented.
	#[codec(compact)]
	schedule_version: u32,
	#[codec(compact)]
	initial: u32,
	#[codec(compact)]
	maximum: u32,
	/// This field is reserved for future evolution of format.
	///
	/// Basically, for now this field will be serialized as `None`. In the future
	/// we would be able to extend this structure with.
	_reserved: Option<()>,
	/// Code instrumented with the latest schedule.
	code: Vec<u8>,
}

/// Wasm executable loaded by `WasmLoader` and executed by `WasmVm`.
pub struct WasmExecutable {
	entrypoint_name: &'static str,
	prefab_module: PrefabWasmModule,
}

/// Loader which fetches `WasmExecutable` from the code cache.
pub struct WasmLoader<'a> {
	schedule: &'a Schedule,
}

impl<'a> WasmLoader<'a> {
	pub fn new(schedule: &'a Schedule) -> Self {
		WasmLoader { schedule }
	}
}

impl<'a, T: Trait> crate::exec::Loader<T> for WasmLoader<'a> {
	type Executable = WasmExecutable;

	fn load_init(&self, code_hash: &CodeHash<T>) -> Result<WasmExecutable, &'static str> {
		let prefab_module = load_code::<T>(code_hash, self.schedule)?;
		Ok(WasmExecutable {
			entrypoint_name: "deploy",
			prefab_module,
		})
	}
	fn load_main(&self, code_hash: &CodeHash<T>) -> Result<WasmExecutable, &'static str> {
		let prefab_module = load_code::<T>(code_hash, self.schedule)?;
		Ok(WasmExecutable {
			entrypoint_name: "call",
			prefab_module,
		})
	}
}

/// Implementation of `Vm` that takes `WasmExecutable` and executes it.
pub struct WasmVm<'a> {
	schedule: &'a Schedule,
}

impl<'a> WasmVm<'a> {
	pub fn new(schedule: &'a Schedule) -> Self {
		WasmVm { schedule }
	}
}

impl<'a, T: Trait> crate::exec::Vm<T> for WasmVm<'a> {
	type Executable = WasmExecutable;

	fn execute<E: Ext<T = T>>(
		&self,
		exec: &WasmExecutable,
		mut ext: E,
		input_data: Vec<u8>,
		gas_meter: &mut GasMeter<E::T>,
	) -> ExecResult {
		let memory =
			sp_sandbox::Memory::new(exec.prefab_module.initial, Some(exec.prefab_module.maximum))
				.unwrap_or_else(|_| {
				// unlike `.expect`, explicit panic preserves the source location.
				// Needed as we can't use `RUST_BACKTRACE` in here.
					panic!(
						"exec.prefab_module.initial can't be greater than exec.prefab_module.maximum;
						thus Memory::new must not fail;
						qed"
					)
				});

		let mut imports = sp_sandbox::EnvironmentDefinitionBuilder::new();
		imports.add_memory("env", "memory", memory.clone());
		runtime::Env::impls(&mut |name, func_ptr| {
			imports.add_host_func("env", name, func_ptr);
		});

		let mut runtime = Runtime::new(
			&mut ext,
			input_data,
			&self.schedule,
			memory,
			gas_meter,
		);

		// Instantiate the instance from the instrumented module code and invoke the contract
		// entrypoint.
		let result = sp_sandbox::Instance::new(&exec.prefab_module.code, &imports, &mut runtime)
			.and_then(|mut instance| instance.invoke(exec.entrypoint_name, &[], &mut runtime));
		to_execution_result(runtime, result)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::collections::HashMap;
	use std::cell::RefCell;
	use sp_core::H256;
	use crate::exec::{Ext, StorageKey, ExecError, ExecReturnValue, STATUS_SUCCESS};
	use crate::gas::{Gas, GasMeter};
	use crate::tests::{Test, Call};
	use crate::wasm::prepare::prepare_contract;
	use crate::CodeHash;
	use wabt;
	use hex_literal::hex;
	use assert_matches::assert_matches;
	use sp_runtime::DispatchError;

	#[derive(Debug, PartialEq, Eq)]
	struct DispatchEntry(Call);

	#[derive(Debug, PartialEq, Eq)]
	struct RestoreEntry {
		dest: u64,
		code_hash: H256,
		rent_allowance: u64,
		delta: Vec<StorageKey>,
	}

	#[derive(Debug, PartialEq, Eq)]
	struct InstantiateEntry {
		code_hash: H256,
		endowment: u64,
		data: Vec<u8>,
		gas_left: u64,
	}

	#[derive(Debug, PartialEq, Eq)]
	struct TransferEntry {
		to: u64,
		value: u64,
		data: Vec<u8>,
		gas_left: u64,
	}

	#[derive(Default)]
	pub struct MockExt {
		storage: HashMap<StorageKey, Vec<u8>>,
		rent_allowance: u64,
		instantiates: Vec<InstantiateEntry>,
		transfers: Vec<TransferEntry>,
		dispatches: Vec<DispatchEntry>,
		restores: Vec<RestoreEntry>,
		// (topics, data)
		events: Vec<(Vec<H256>, Vec<u8>)>,
		next_account_id: u64,

		/// Runtime storage keys works the following way.
		///
		/// - If the test code requests a value and it doesn't exist in this storage map then a
		///   panic happens.
		/// - If the value does exist it is returned and then removed from the map. So a panic
		///   happens if the same value is requested for the second time.
		///
		/// This behavior is used to prevent mixing up an access to unexpected location and empty
		/// cell.
		runtime_storage_keys: RefCell<HashMap<Vec<u8>, Option<Vec<u8>>>>,
	}

	impl Ext for MockExt {
		type T = Test;

		fn get_storage(&self, key: &StorageKey) -> Option<Vec<u8>> {
			self.storage.get(key).cloned()
		}
		fn set_storage(&mut self, key: StorageKey, value: Option<Vec<u8>>)
			-> Result<(), &'static str>
		{
			*self.storage.entry(key).or_insert(Vec::new()) = value.unwrap_or(Vec::new());
			Ok(())
		}
		fn instantiate(
			&mut self,
			code_hash: &CodeHash<Test>,
			endowment: u64,
			gas_meter: &mut GasMeter<Test>,
			data: Vec<u8>,
		) -> Result<(u64, ExecReturnValue), ExecError> {
			self.instantiates.push(InstantiateEntry {
				code_hash: code_hash.clone(),
				endowment,
				data: data.to_vec(),
				gas_left: gas_meter.gas_left(),
			});
			let address = self.next_account_id;
			self.next_account_id += 1;

			Ok((address, ExecReturnValue { status: STATUS_SUCCESS, data: Vec::new() }))
		}
		fn call(
			&mut self,
			to: &u64,
			value: u64,
			gas_meter: &mut GasMeter<Test>,
			data: Vec<u8>,
		) -> ExecResult {
			self.transfers.push(TransferEntry {
				to: *to,
				value,
				data: data.to_vec(),
				gas_left: gas_meter.gas_left(),
			});
			// Assume for now that it was just a plain transfer.
			// TODO: Add tests for different call outcomes.
			Ok(ExecReturnValue { status: STATUS_SUCCESS, data: Vec::new() })
		}
		fn note_dispatch_call(&mut self, call: Call) {
			self.dispatches.push(DispatchEntry(call));
		}
		fn note_restore_to(
			&mut self,
			dest: u64,
			code_hash: H256,
			rent_allowance: u64,
			delta: Vec<StorageKey>,
		) {
			self.restores.push(RestoreEntry {
				dest,
				code_hash,
				rent_allowance,
				delta,
			});
		}
		fn caller(&self) -> &u64 {
			&42
		}
		fn address(&self) -> &u64 {
			&69
		}
		fn balance(&self) -> u64 {
			228
		}
		fn value_transferred(&self) -> u64 {
			1337
		}

		fn now(&self) -> &u64 {
			&1111
		}

		fn minimum_balance(&self) -> u64 {
			666
		}

		fn random(&self, subject: &[u8]) -> H256 {
			H256::from_slice(subject)
		}

		fn deposit_event(&mut self, topics: Vec<H256>, data: Vec<u8>) {
			self.events.push((topics, data))
		}

		fn set_rent_allowance(&mut self, rent_allowance: u64) {
			self.rent_allowance = rent_allowance;
		}

		fn rent_allowance(&self) -> u64 {
			self.rent_allowance
		}

		fn block_number(&self) -> u64 { 121 }

		fn max_value_size(&self) -> u32 { 16_384 }

		fn get_runtime_storage(&self, key: &[u8]) -> Option<Vec<u8>> {
			let opt_value = self.runtime_storage_keys
				.borrow_mut()
				.remove(key);
			opt_value.unwrap_or_else(||
				panic!(
					"{:?} doesn't exist. values that do exist {:?}",
					key,
					self.runtime_storage_keys
				)
			)
		}
	}

	impl Ext for &mut MockExt {
		type T = <MockExt as Ext>::T;

		fn get_storage(&self, key: &[u8; 32]) -> Option<Vec<u8>> {
			(**self).get_storage(key)
		}
		fn set_storage(&mut self, key: [u8; 32], value: Option<Vec<u8>>)
			-> Result<(), &'static str>
		{
			(**self).set_storage(key, value)
		}
		fn instantiate(
			&mut self,
			code: &CodeHash<Test>,
			value: u64,
			gas_meter: &mut GasMeter<Test>,
			input_data: Vec<u8>,
		) -> Result<(u64, ExecReturnValue), ExecError> {
			(**self).instantiate(code, value, gas_meter, input_data)
		}
		fn call(
			&mut self,
			to: &u64,
			value: u64,
			gas_meter: &mut GasMeter<Test>,
			input_data: Vec<u8>,
		) -> ExecResult {
			(**self).call(to, value, gas_meter, input_data)
		}
		fn note_dispatch_call(&mut self, call: Call) {
			(**self).note_dispatch_call(call)
		}
		fn note_restore_to(
			&mut self,
			dest: u64,
			code_hash: H256,
			rent_allowance: u64,
			delta: Vec<StorageKey>,
		) {
			(**self).note_restore_to(
				dest,
				code_hash,
				rent_allowance,
				delta,
			)
		}
		fn caller(&self) -> &u64 {
			(**self).caller()
		}
		fn address(&self) -> &u64 {
			(**self).address()
		}
		fn balance(&self) -> u64 {
			(**self).balance()
		}
		fn value_transferred(&self) -> u64 {
			(**self).value_transferred()
		}
		fn now(&self) -> &u64 {
			(**self).now()
		}
		fn minimum_balance(&self) -> u64 {
			(**self).minimum_balance()
		}
		fn random(&self, subject: &[u8]) -> H256 {
			(**self).random(subject)
		}
		fn deposit_event(&mut self, topics: Vec<H256>, data: Vec<u8>) {
			(**self).deposit_event(topics, data)
		}
		fn set_rent_allowance(&mut self, rent_allowance: u64) {
			(**self).set_rent_allowance(rent_allowance)
		}
		fn rent_allowance(&self) -> u64 {
			(**self).rent_allowance()
		}
		fn block_number(&self) -> u64 {
			(**self).block_number()
		}
		fn max_value_size(&self) -> u32 {
			(**self).max_value_size()
		}
		fn get_runtime_storage(&self, key: &[u8]) -> Option<Vec<u8>> {
			(**self).get_runtime_storage(key)
		}
	}

	fn execute<E: Ext>(
		wat: &str,
		input_data: Vec<u8>,
		ext: E,
		gas_meter: &mut GasMeter<E::T>,
	) -> ExecResult {
		use crate::exec::Vm;

		let wasm = wabt::wat2wasm(wat).unwrap();
		let schedule = crate::Schedule::default();
		let prefab_module =
			prepare_contract::<super::runtime::Env>(&wasm, &schedule).unwrap();

		let exec = WasmExecutable {
			// Use a "call" convention.
			entrypoint_name: "call",
			prefab_module,
		};

		let cfg = Default::default();
		let vm = WasmVm::new(&cfg);

		vm.execute(&exec, ext, input_data, gas_meter)
	}

	const CODE_TRANSFER: &str = r#"
(module
	;; ext_call(
	;;    callee_ptr: u32,
	;;    callee_len: u32,
	;;    gas: u64,
	;;    value_ptr: u32,
	;;    value_len: u32,
	;;    input_data_ptr: u32,
	;;    input_data_len: u32
	;;) -> u32
	(import "env" "ext_call" (func $ext_call (param i32 i32 i64 i32 i32 i32 i32) (result i32)))
	(import "env" "memory" (memory 1 1))
	(func (export "call")
		(drop
			(call $ext_call
				(i32.const 4)  ;; Pointer to "callee" address.
				(i32.const 8)  ;; Length of "callee" address.
				(i64.const 0)  ;; How much gas to devote for the execution. 0 = all.
				(i32.const 12) ;; Pointer to the buffer with value to transfer
				(i32.const 8)  ;; Length of the buffer with value to transfer.
				(i32.const 20) ;; Pointer to input data buffer address
				(i32.const 4)  ;; Length of input data buffer
			)
		)
	)
	(func (export "deploy"))

	;; Destination AccountId to transfer the funds.
	;; Represented by u64 (8 bytes long) in little endian.
	(data (i32.const 4) "\09\00\00\00\00\00\00\00")
	;; Amount of value to transfer.
	;; Represented by u64 (8 bytes long) in little endian.
	(data (i32.const 12) "\06\00\00\00\00\00\00\00")

	(data (i32.const 20) "\01\02\03\04")
)
"#;

	#[test]
	fn contract_transfer() {
		let mut mock_ext = MockExt::default();
		let _ = execute(
			CODE_TRANSFER,
			vec![],
			&mut mock_ext,
			&mut GasMeter::with_limit(50_000, 1),
		).unwrap();

		assert_eq!(
			&mock_ext.transfers,
			&[TransferEntry {
				to: 9,
				value: 6,
				data: vec![1, 2, 3, 4],
				gas_left: 49971,
			}]
		);
	}

	const CODE_INSTANTIATE: &str = r#"
(module
	;; ext_instantiate(
	;;     code_ptr: u32,
	;;     code_len: u32,
	;;     gas: u64,
	;;     value_ptr: u32,
	;;     value_len: u32,
	;;     input_data_ptr: u32,
	;;     input_data_len: u32,
	;; ) -> u32
	(import "env" "ext_instantiate" (func $ext_instantiate (param i32 i32 i64 i32 i32 i32 i32) (result i32)))
	(import "env" "memory" (memory 1 1))
	(func (export "call")
		(drop
			(call $ext_instantiate
				(i32.const 16)   ;; Pointer to `code_hash`
				(i32.const 32)   ;; Length of `code_hash`
				(i64.const 0)    ;; How much gas to devote for the execution. 0 = all.
				(i32.const 4)    ;; Pointer to the buffer with value to transfer
				(i32.const 8)    ;; Length of the buffer with value to transfer
				(i32.const 12)   ;; Pointer to input data buffer address
				(i32.const 4)    ;; Length of input data buffer
			)
		)
	)
	(func (export "deploy"))

	;; Amount of value to transfer.
	;; Represented by u64 (8 bytes long) in little endian.
	(data (i32.const 4) "\03\00\00\00\00\00\00\00")
	;; Input data to pass to the contract being instantiated.
	(data (i32.const 12) "\01\02\03\04")
	;; Hash of code.
	(data (i32.const 16)
		"\11\11\11\11\11\11\11\11\11\11\11\11\11\11\11\11"
		"\11\11\11\11\11\11\11\11\11\11\11\11\11\11\11\11"
	)
)
"#;

	#[test]
	fn contract_instantiate() {
		let mut mock_ext = MockExt::default();
		let _ = execute(
			CODE_INSTANTIATE,
			vec![],
			&mut mock_ext,
			&mut GasMeter::with_limit(50_000, 1),
		).unwrap();

		assert_eq!(
			&mock_ext.instantiates,
			&[InstantiateEntry {
				code_hash: [0x11; 32].into(),
				endowment: 3,
				data: vec![1, 2, 3, 4],
				gas_left: 49947,
			}]
		);
	}

	const CODE_TRANSFER_LIMITED_GAS: &str = r#"
(module
	;; ext_call(
	;;    callee_ptr: u32,
	;;    callee_len: u32,
	;;    gas: u64,
	;;    value_ptr: u32,
	;;    value_len: u32,
	;;    input_data_ptr: u32,
	;;    input_data_len: u32
	;;) -> u32
	(import "env" "ext_call" (func $ext_call (param i32 i32 i64 i32 i32 i32 i32) (result i32)))
	(import "env" "memory" (memory 1 1))
	(func (export "call")
		(drop
			(call $ext_call
				(i32.const 4)  ;; Pointer to "callee" address.
				(i32.const 8)  ;; Length of "callee" address.
				(i64.const 228)  ;; How much gas to devote for the execution.
				(i32.const 12)  ;; Pointer to the buffer with value to transfer
				(i32.const 8)   ;; Length of the buffer with value to transfer.
				(i32.const 20)   ;; Pointer to input data buffer address
				(i32.const 4)   ;; Length of input data buffer
			)
		)
	)
	(func (export "deploy"))

	;; Destination AccountId to transfer the funds.
	;; Represented by u64 (8 bytes long) in little endian.
	(data (i32.const 4) "\09\00\00\00\00\00\00\00")
	;; Amount of value to transfer.
	;; Represented by u64 (8 bytes long) in little endian.
	(data (i32.const 12) "\06\00\00\00\00\00\00\00")

	(data (i32.const 20) "\01\02\03\04")
)
"#;

	#[test]
	fn contract_call_limited_gas() {
		let mut mock_ext = MockExt::default();
		let _ = execute(
			&CODE_TRANSFER_LIMITED_GAS,
			vec![],
			&mut mock_ext,
			&mut GasMeter::with_limit(50_000, 1),
		).unwrap();

		assert_eq!(
			&mock_ext.transfers,
			&[TransferEntry {
				to: 9,
				value: 6,
				data: vec![1, 2, 3, 4],
				gas_left: 228,
			}]
		);
	}

	const CODE_GET_STORAGE: &str = r#"
(module
	(import "env" "ext_get_storage" (func $ext_get_storage (param i32) (result i32)))
	(import "env" "ext_scratch_size" (func $ext_scratch_size (result i32)))
	(import "env" "ext_scratch_read" (func $ext_scratch_read (param i32 i32 i32)))
	(import "env" "ext_return" (func $ext_return (param i32 i32)))
	(import "env" "memory" (memory 1 1))

	(func $assert (param i32)
		(block $ok
			(br_if $ok
				(get_local 0)
			)
			(unreachable)
		)
	)

	(func (export "call")
		(local $buf_size i32)


		;; Load a storage value into the scratch buf.
		(call $assert
			(i32.eq
				(call $ext_get_storage
					(i32.const 4)		;; The pointer to the storage key to fetch
				)

				;; Return value 0 means that the value is found and there were
				;; no errors.
				(i32.const 0)
			)
		)

		;; Find out the size of the scratch buffer
		(set_local $buf_size
			(call $ext_scratch_size)
		)

		;; Copy scratch buffer into this contract memory.
		(call $ext_scratch_read
			(i32.const 36)		;; The pointer where to store the scratch buffer contents,
								;; 36 = 4 + 32
			(i32.const 0)		;; Offset from the start of the scratch buffer.
			(get_local			;; Count of bytes to copy.
				$buf_size
			)
		)

		;; Return the contents of the buffer
		(call $ext_return
			(i32.const 36)
			(get_local $buf_size)
		)

		;; env:ext_return doesn't return, so this is effectively unreachable.
		(unreachable)
	)

	(func (export "deploy"))

	(data (i32.const 4)
		"\11\11\11\11\11\11\11\11\11\11\11\11\11\11\11\11"
		"\11\11\11\11\11\11\11\11\11\11\11\11\11\11\11\11"
	)
)
"#;

	#[test]
	fn get_storage_puts_data_into_scratch_buf() {
		let mut mock_ext = MockExt::default();
		mock_ext
			.storage
			.insert([0x11; 32], [0x22; 32].to_vec());

		let output = execute(
			CODE_GET_STORAGE,
			vec![],
			mock_ext,
			&mut GasMeter::with_limit(50_000, 1),
		).unwrap();

		assert_eq!(output, ExecReturnValue { status: STATUS_SUCCESS, data: [0x22; 32].to_vec() });
	}

	/// calls `ext_caller`, loads the address from the scratch buffer and
	/// compares it with the constant 42.
	const CODE_CALLER: &str = r#"
(module
	(import "env" "ext_caller" (func $ext_caller))
	(import "env" "ext_scratch_size" (func $ext_scratch_size (result i32)))
	(import "env" "ext_scratch_read" (func $ext_scratch_read (param i32 i32 i32)))
	(import "env" "memory" (memory 1 1))

	(func $assert (param i32)
		(block $ok
			(br_if $ok
				(get_local 0)
			)
			(unreachable)
		)
	)

	(func (export "call")
		;; fill the scratch buffer with the caller.
		(call $ext_caller)

		;; assert $ext_scratch_size == 8
		(call $assert
			(i32.eq
				(call $ext_scratch_size)
				(i32.const 8)
			)
		)

		;; copy contents of the scratch buffer into the contract's memory.
		(call $ext_scratch_read
			(i32.const 8)		;; Pointer in memory to the place where to copy.
			(i32.const 0)		;; Offset from the start of the scratch buffer.
			(i32.const 8)		;; Count of bytes to copy.
		)

		;; assert that contents of the buffer is equal to the i64 value of 42.
		(call $assert
			(i64.eq
				(i64.load
					(i32.const 8)
				)
				(i64.const 42)
			)
		)
	)

	(func (export "deploy"))
)
"#;

	#[test]
	fn caller() {
		let _ = execute(
			CODE_CALLER,
			vec![],
			MockExt::default(),
			&mut GasMeter::with_limit(50_000, 1),
		).unwrap();
	}

	/// calls `ext_address`, loads the address from the scratch buffer and
	/// compares it with the constant 69.
	const CODE_ADDRESS: &str = r#"
(module
	(import "env" "ext_address" (func $ext_address))
	(import "env" "ext_scratch_size" (func $ext_scratch_size (result i32)))
	(import "env" "ext_scratch_read" (func $ext_scratch_read (param i32 i32 i32)))
	(import "env" "memory" (memory 1 1))

	(func $assert (param i32)
		(block $ok
			(br_if $ok
				(get_local 0)
			)
			(unreachable)
		)
	)

	(func (export "call")
		;; fill the scratch buffer with the self address.
		(call $ext_address)

		;; assert $ext_scratch_size == 8
		(call $assert
			(i32.eq
				(call $ext_scratch_size)
				(i32.const 8)
			)
		)

		;; copy contents of the scratch buffer into the contract's memory.
		(call $ext_scratch_read
			(i32.const 8)		;; Pointer in memory to the place where to copy.
			(i32.const 0)		;; Offset from the start of the scratch buffer.
			(i32.const 8)		;; Count of bytes to copy.
		)

		;; assert that contents of the buffer is equal to the i64 value of 69.
		(call $assert
			(i64.eq
				(i64.load
					(i32.const 8)
				)
				(i64.const 69)
			)
		)
	)

	(func (export "deploy"))
)
"#;

	#[test]
	fn address() {
		let _ = execute(
			CODE_ADDRESS,
			vec![],
			MockExt::default(),
			&mut GasMeter::with_limit(50_000, 1),
		).unwrap();
	}

	const CODE_BALANCE: &str = r#"
(module
	(import "env" "ext_balance" (func $ext_balance))
	(import "env" "ext_scratch_size" (func $ext_scratch_size (result i32)))
	(import "env" "ext_scratch_read" (func $ext_scratch_read (param i32 i32 i32)))
	(import "env" "memory" (memory 1 1))

	(func $assert (param i32)
		(block $ok
			(br_if $ok
				(get_local 0)
			)
			(unreachable)
		)
	)

	(func (export "call")
		;; This stores the balance in the scratch buffer
		(call $ext_balance)

		;; assert $ext_scratch_size == 8
		(call $assert
			(i32.eq
				(call $ext_scratch_size)
				(i32.const 8)
			)
		)

		;; copy contents of the scratch buffer into the contract's memory.
		(call $ext_scratch_read
			(i32.const 8)		;; Pointer in memory to the place where to copy.
			(i32.const 0)		;; Offset from the start of the scratch buffer.
			(i32.const 8)		;; Count of bytes to copy.
		)

		;; assert that contents of the buffer is equal to the i64 value of 228.
		(call $assert
			(i64.eq
				(i64.load
					(i32.const 8)
				)
				(i64.const 228)
			)
		)
	)
	(func (export "deploy"))
)
"#;

	#[test]
	fn balance() {
		let mut gas_meter = GasMeter::with_limit(50_000, 1);
		let _ = execute(
			CODE_BALANCE,
			vec![],
			MockExt::default(),
			&mut gas_meter,
		).unwrap();
	}

	const CODE_GAS_PRICE: &str = r#"
(module
	(import "env" "ext_gas_price" (func $ext_gas_price))
	(import "env" "ext_scratch_size" (func $ext_scratch_size (result i32)))
	(import "env" "ext_scratch_read" (func $ext_scratch_read (param i32 i32 i32)))
	(import "env" "memory" (memory 1 1))

	(func $assert (param i32)
		(block $ok
			(br_if $ok
				(get_local 0)
			)
			(unreachable)
		)
	)

	(func (export "call")
		;; This stores the gas price in the scratch buffer
		(call $ext_gas_price)

		;; assert $ext_scratch_size == 8
		(call $assert
			(i32.eq
				(call $ext_scratch_size)
				(i32.const 8)
			)
		)

		;; copy contents of the scratch buffer into the contract's memory.
		(call $ext_scratch_read
			(i32.const 8)		;; Pointer in memory to the place where to copy.
			(i32.const 0)		;; Offset from the start of the scratch buffer.
			(i32.const 8)		;; Count of bytes to copy.
		)

		;; assert that contents of the buffer is equal to the i64 value of 1312.
		(call $assert
			(i64.eq
				(i64.load
					(i32.const 8)
				)
				(i64.const 1312)
			)
		)
	)
	(func (export "deploy"))
)
"#;

	#[test]
	fn gas_price() {
		let mut gas_meter = GasMeter::with_limit(50_000, 1312);
		let _ = execute(
			CODE_GAS_PRICE,
			vec![],
			MockExt::default(),
			&mut gas_meter,
		).unwrap();
	}

	const CODE_GAS_LEFT: &str = r#"
(module
	(import "env" "ext_gas_left" (func $ext_gas_left))
	(import "env" "ext_scratch_size" (func $ext_scratch_size (result i32)))
	(import "env" "ext_scratch_read" (func $ext_scratch_read (param i32 i32 i32)))
	(import "env" "ext_return" (func $ext_return (param i32 i32)))
	(import "env" "memory" (memory 1 1))

	(func $assert (param i32)
		(block $ok
			(br_if $ok
				(get_local 0)
			)
			(unreachable)
		)
	)

	(func (export "call")
		;; This stores the gas left in the scratch buffer
		(call $ext_gas_left)

		;; assert $ext_scratch_size == 8
		(call $assert
			(i32.eq
				(call $ext_scratch_size)
				(i32.const 8)
			)
		)

		;; copy contents of the scratch buffer into the contract's memory.
		(call $ext_scratch_read
			(i32.const 8)		;; Pointer in memory to the place where to copy.
			(i32.const 0)		;; Offset from the start of the scratch buffer.
			(i32.const 8)		;; Count of bytes to copy.
		)

		(call $ext_return
			(i32.const 8)
			(i32.const 8)
		)

		(unreachable)
	)
	(func (export "deploy"))
)
"#;

	#[test]
	fn gas_left() {
		let mut gas_meter = GasMeter::with_limit(50_000, 1312);

		let output = execute(
			CODE_GAS_LEFT,
			vec![],
			MockExt::default(),
			&mut gas_meter,
		).unwrap();

		let gas_left = Gas::decode(&mut output.data.as_slice()).unwrap();
		assert!(gas_left < 50_000, "gas_left must be less than initial");
		assert!(gas_left > gas_meter.gas_left(), "gas_left must be greater than final");
	}

	const CODE_VALUE_TRANSFERRED: &str = r#"
(module
	(import "env" "ext_value_transferred" (func $ext_value_transferred))
	(import "env" "ext_scratch_size" (func $ext_scratch_size (result i32)))
	(import "env" "ext_scratch_read" (func $ext_scratch_read (param i32 i32 i32)))
	(import "env" "memory" (memory 1 1))

	(func $assert (param i32)
		(block $ok
			(br_if $ok
				(get_local 0)
			)
			(unreachable)
		)
	)

	(func (export "call")
		;; This stores the value transferred in the scratch buffer
		(call $ext_value_transferred)

		;; assert $ext_scratch_size == 8
		(call $assert
			(i32.eq
				(call $ext_scratch_size)
				(i32.const 8)
			)
		)

		;; copy contents of the scratch buffer into the contract's memory.
		(call $ext_scratch_read
			(i32.const 8)		;; Pointer in memory to the place where to copy.
			(i32.const 0)		;; Offset from the start of the scratch buffer.
			(i32.const 8)		;; Count of bytes to copy.
		)

		;; assert that contents of the buffer is equal to the i64 value of 1337.
		(call $assert
			(i64.eq
				(i64.load
					(i32.const 8)
				)
				(i64.const 1337)
			)
		)
	)
	(func (export "deploy"))
)
"#;

	#[test]
	fn value_transferred() {
		let mut gas_meter = GasMeter::with_limit(50_000, 1);
		let _ = execute(
			CODE_VALUE_TRANSFERRED,
			vec![],
			MockExt::default(),
			&mut gas_meter,
		).unwrap();
	}

	const CODE_DISPATCH_CALL: &str = r#"
(module
	(import "env" "ext_dispatch_call" (func $ext_dispatch_call (param i32 i32)))
	(import "env" "memory" (memory 1 1))

	(func (export "call")
		(call $ext_dispatch_call
			(i32.const 8) ;; Pointer to the start of encoded call buffer
			(i32.const 13) ;; Length of the buffer
		)
	)
	(func (export "deploy"))

	(data (i32.const 8) "\00\01\2A\00\00\00\00\00\00\00\E5\14\00")
)
"#;

	#[test]
	fn dispatch_call() {
		// This test can fail due to the encoding changes. In case it becomes too annoying
		// let's rewrite so as we use this module controlled call or we serialize it in runtime.

		let mut mock_ext = MockExt::default();
		let _ = execute(
			CODE_DISPATCH_CALL,
			vec![],
			&mut mock_ext,
			&mut GasMeter::with_limit(50_000, 1),
		).unwrap();

		assert_eq!(
			&mock_ext.dispatches,
			&[DispatchEntry(
				Call::Balances(pallet_balances::Call::set_balance(42, 1337, 0)),
			)]
		);
	}

	const CODE_RETURN_FROM_START_FN: &str = r#"
(module
	(import "env" "ext_return" (func $ext_return (param i32 i32)))
	(import "env" "memory" (memory 1 1))

	(start $start)
	(func $start
		(call $ext_return
			(i32.const 8)
			(i32.const 4)
		)
		(unreachable)
	)

	(func (export "call")
		(unreachable)
	)
	(func (export "deploy"))

	(data (i32.const 8) "\01\02\03\04")
)
"#;

	#[test]
	fn return_from_start_fn() {
		let output = execute(
			CODE_RETURN_FROM_START_FN,
			vec![],
			MockExt::default(),
			&mut GasMeter::with_limit(50_000, 1),
		).unwrap();

		assert_eq!(output, ExecReturnValue { status: STATUS_SUCCESS, data: vec![1, 2, 3, 4] });
	}

	const CODE_TIMESTAMP_NOW: &str = r#"
(module
	(import "env" "ext_now" (func $ext_now))
	(import "env" "ext_scratch_size" (func $ext_scratch_size (result i32)))
	(import "env" "ext_scratch_read" (func $ext_scratch_read (param i32 i32 i32)))
	(import "env" "memory" (memory 1 1))

	(func $assert (param i32)
		(block $ok
			(br_if $ok
				(get_local 0)
			)
			(unreachable)
		)
	)

	(func (export "call")
		;; This stores the block timestamp in the scratch buffer
		(call $ext_now)

		;; assert $ext_scratch_size == 8
		(call $assert
			(i32.eq
				(call $ext_scratch_size)
				(i32.const 8)
			)
		)

		;; copy contents of the scratch buffer into the contract's memory.
		(call $ext_scratch_read
			(i32.const 8)		;; Pointer in memory to the place where to copy.
			(i32.const 0)		;; Offset from the start of the scratch buffer.
			(i32.const 8)		;; Count of bytes to copy.
		)

		;; assert that contents of the buffer is equal to the i64 value of 1111.
		(call $assert
			(i64.eq
				(i64.load
					(i32.const 8)
				)
				(i64.const 1111)
			)
		)
	)
	(func (export "deploy"))
)
"#;

	#[test]
	fn now() {
		let mut gas_meter = GasMeter::with_limit(50_000, 1);
		let _ = execute(
			CODE_TIMESTAMP_NOW,
			vec![],
			MockExt::default(),
			&mut gas_meter,
		).unwrap();
	}

	const CODE_MINIMUM_BALANCE: &str = r#"
(module
	(import "env" "ext_minimum_balance" (func $ext_minimum_balance))
	(import "env" "ext_scratch_size" (func $ext_scratch_size (result i32)))
	(import "env" "ext_scratch_read" (func $ext_scratch_read (param i32 i32 i32)))
	(import "env" "memory" (memory 1 1))

	(func $assert (param i32)
		(block $ok
			(br_if $ok
				(get_local 0)
			)
			(unreachable)
		)
	)

	(func (export "call")
		(call $ext_minimum_balance)

		;; assert $ext_scratch_size == 8
		(call $assert
			(i32.eq
				(call $ext_scratch_size)
				(i32.const 8)
			)
		)

		;; copy contents of the scratch buffer into the contract's memory.
		(call $ext_scratch_read
			(i32.const 8)		;; Pointer in memory to the place where to copy.
			(i32.const 0)		;; Offset from the start of the scratch buffer.
			(i32.const 8)		;; Count of bytes to copy.
		)

		;; assert that contents of the buffer is equal to the i64 value of 666.
		(call $assert
			(i64.eq
				(i64.load
					(i32.const 8)
				)
				(i64.const 666)
			)
		)
	)
	(func (export "deploy"))
)
"#;

	#[test]
	fn minimum_balance() {
		let mut gas_meter = GasMeter::with_limit(50_000, 1);
		let _ = execute(
			CODE_MINIMUM_BALANCE,
			vec![],
			MockExt::default(),
			&mut gas_meter,
		).unwrap();
	}

	const CODE_RANDOM: &str = r#"
(module
	(import "env" "ext_random" (func $ext_random (param i32 i32)))
	(import "env" "ext_scratch_size" (func $ext_scratch_size (result i32)))
	(import "env" "ext_scratch_read" (func $ext_scratch_read (param i32 i32 i32)))
	(import "env" "ext_return" (func $ext_return (param i32 i32)))
	(import "env" "memory" (memory 1 1))

	(func $assert (param i32)
		(block $ok
			(br_if $ok
				(get_local 0)
			)
			(unreachable)
		)
	)

	(func (export "call")
		;; This stores the block random seed in the scratch buffer
		(call $ext_random
			(i32.const 40) ;; Pointer in memory to the start of the subject buffer
			(i32.const 32) ;; The subject buffer's length
		)

		;; assert $ext_scratch_size == 32
		(call $assert
			(i32.eq
				(call $ext_scratch_size)
				(i32.const 32)
			)
		)

		;; copy contents of the scratch buffer into the contract's memory.
		(call $ext_scratch_read
			(i32.const 8)		;; Pointer in memory to the place where to copy.
			(i32.const 0)		;; Offset from the start of the scratch buffer.
			(i32.const 32)		;; Count of bytes to copy.
		)

		;; return the data from the contract
		(call $ext_return
			(i32.const 8)
			(i32.const 32)
		)
	)
	(func (export "deploy"))

	;; [8,40) is reserved for the result of PRNG.

	;; the subject used for the PRNG. [40,72)
	(data (i32.const 40)
		"\00\01\02\03\04\05\06\07\08\09\0A\0B\0C\0D\0E\0F"
		"\00\01\02\03\04\05\06\07\08\09\0A\0B\0C\0D\0E\0F"
	)
)
"#;

	#[test]
	fn random() {
		let mut gas_meter = GasMeter::with_limit(50_000, 1);

		let output = execute(
			CODE_RANDOM,
			vec![],
			MockExt::default(),
			&mut gas_meter,
		).unwrap();

		// The mock ext just returns the same data that was passed as the subject.
		assert_eq!(
			output,
			ExecReturnValue {
				status: STATUS_SUCCESS,
				data: hex!("000102030405060708090A0B0C0D0E0F000102030405060708090A0B0C0D0E0F").to_vec(),
			},
		);
	}

	const CODE_DEPOSIT_EVENT: &str = r#"
(module
	(import "env" "ext_deposit_event" (func $ext_deposit_event (param i32 i32 i32 i32)))
	(import "env" "memory" (memory 1 1))

	(func (export "call")
		(call $ext_deposit_event
			(i32.const 32) ;; Pointer to the start of topics buffer
			(i32.const 33) ;; The length of the topics buffer.
			(i32.const 8) ;; Pointer to the start of the data buffer
			(i32.const 13) ;; Length of the buffer
		)
	)
	(func (export "deploy"))

	(data (i32.const 8) "\00\01\2A\00\00\00\00\00\00\00\E5\14\00")

	;; Encoded Vec<TopicOf<T>>, the buffer has length of 33 bytes.
	(data (i32.const 32) "\04\33\33\33\33\33\33\33\33\33\33\33\33\33\33\33\33\33\33\33\33\33\33\33"
	"\33\33\33\33\33\33\33\33\33")
)
"#;

	#[test]
	fn deposit_event() {
		let mut mock_ext = MockExt::default();
		let mut gas_meter = GasMeter::with_limit(50_000, 1);
		let _ = execute(
			CODE_DEPOSIT_EVENT,
			vec![],
			&mut mock_ext,
			&mut gas_meter
		).unwrap();

		assert_eq!(mock_ext.events, vec![
			(vec![H256::repeat_byte(0x33)],
			vec![0x00, 0x01, 0x2a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xe5, 0x14, 0x00])
		]);

		assert_eq!(gas_meter.gas_left(), 49934);
	}

	const CODE_DEPOSIT_EVENT_MAX_TOPICS: &str = r#"
(module
	(import "env" "ext_deposit_event" (func $ext_deposit_event (param i32 i32 i32 i32)))
	(import "env" "memory" (memory 1 1))

	(func (export "call")
		(call $ext_deposit_event
			(i32.const 32) ;; Pointer to the start of topics buffer
			(i32.const 161) ;; The length of the topics buffer.
			(i32.const 8) ;; Pointer to the start of the data buffer
			(i32.const 13) ;; Length of the buffer
		)
	)
	(func (export "deploy"))

	(data (i32.const 8) "\00\01\2A\00\00\00\00\00\00\00\E5\14\00")

	;; Encoded Vec<TopicOf<T>>, the buffer has length of 161 bytes.
	(data (i32.const 32) "\14"
"\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01"
"\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02"
"\03\03\03\03\03\03\03\03\03\03\03\03\03\03\03\03\03\03\03\03\03\03\03\03\03\03\03\03\03\03\03\03"
"\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04"
"\05\05\05\05\05\05\05\05\05\05\05\05\05\05\05\05\05\05\05\05\05\05\05\05\05\05\05\05\05\05\05\05")
)
"#;

	#[test]
	fn deposit_event_max_topics() {
		// Checks that the runtime traps if there are more than `max_topic_events` topics.
		let mut gas_meter = GasMeter::with_limit(50_000, 1);

		assert_matches!(
			execute(
				CODE_DEPOSIT_EVENT_MAX_TOPICS,
				vec![],
				MockExt::default(),
				&mut gas_meter
			),
			Err(ExecError { reason: DispatchError::Other("during execution"), buffer: _ })
		);
	}

	const CODE_DEPOSIT_EVENT_DUPLICATES: &str = r#"
(module
	(import "env" "ext_deposit_event" (func $ext_deposit_event (param i32 i32 i32 i32)))
	(import "env" "memory" (memory 1 1))

	(func (export "call")
		(call $ext_deposit_event
			(i32.const 32) ;; Pointer to the start of topics buffer
			(i32.const 129) ;; The length of the topics buffer.
			(i32.const 8) ;; Pointer to the start of the data buffer
			(i32.const 13) ;; Length of the buffer
		)
	)
	(func (export "deploy"))

	(data (i32.const 8) "\00\01\2A\00\00\00\00\00\00\00\E5\14\00")

	;; Encoded Vec<TopicOf<T>>, the buffer has length of 129 bytes.
	(data (i32.const 32) "\10"
"\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01"
"\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02"
"\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01"
"\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04\04")
)
"#;

	#[test]
	fn deposit_event_duplicates() {
		// Checks that the runtime traps if there are duplicates.
		let mut gas_meter = GasMeter::with_limit(50_000, 1);

		assert_matches!(
			execute(
				CODE_DEPOSIT_EVENT_DUPLICATES,
				vec![],
				MockExt::default(),
				&mut gas_meter
			),
			Err(ExecError { reason: DispatchError::Other("during execution"), buffer: _ })
		);
	}

	/// calls `ext_block_number`, loads the current block number from the scratch buffer and
	/// compares it with the constant 121.
	const CODE_BLOCK_NUMBER: &str = r#"
(module
	(import "env" "ext_block_number" (func $ext_block_number))
	(import "env" "ext_scratch_size" (func $ext_scratch_size (result i32)))
	(import "env" "ext_scratch_read" (func $ext_scratch_read (param i32 i32 i32)))
	(import "env" "memory" (memory 1 1))

	(func $assert (param i32)
		(block $ok
			(br_if $ok
				(get_local 0)
			)
			(unreachable)
		)
	)

	(func (export "call")
		;; This stores the block height in the scratch buffer
		(call $ext_block_number)

		;; assert $ext_scratch_size == 8
		(call $assert
			(i32.eq
				(call $ext_scratch_size)
				(i32.const 8)
			)
		)

		;; copy contents of the scratch buffer into the contract's memory.
		(call $ext_scratch_read
			(i32.const 8)		;; Pointer in memory to the place where to copy.
			(i32.const 0)		;; Offset from the start of the scratch buffer.
			(i32.const 8)		;; Count of bytes to copy.
		)

		;; assert that contents of the buffer is equal to the i64 value of 121.
		(call $assert
			(i64.eq
				(i64.load
					(i32.const 8)
				)
				(i64.const 121)
			)
		)
	)

	(func (export "deploy"))
)
"#;

	#[test]
	fn block_number() {
		let _ = execute(
			CODE_BLOCK_NUMBER,
			vec![],
			MockExt::default(),
			&mut GasMeter::with_limit(50_000, 1),
		).unwrap();
	}

	// asserts that the size of the input data is 4.
	const CODE_SIMPLE_ASSERT: &str = r#"
(module
	(import "env" "ext_scratch_size" (func $ext_scratch_size (result i32)))

	(func $assert (param i32)
		(block $ok
			(br_if $ok
				(get_local 0)
			)
			(unreachable)
		)
	)

	(func (export "deploy"))

	(func (export "call")
		(call $assert
			(i32.eq
				(call $ext_scratch_size)
				(i32.const 4)
			)
		)
	)
)
"#;

	#[test]
	fn output_buffer_capacity_preserved_on_success() {
		let mut input_data = Vec::with_capacity(1_234);
		input_data.extend_from_slice(&[1, 2, 3, 4][..]);

		let output = execute(
			CODE_SIMPLE_ASSERT,
			input_data,
			MockExt::default(),
			&mut GasMeter::with_limit(50_000, 1),
		).unwrap();

		assert_eq!(output.data.len(), 0);
		assert_eq!(output.data.capacity(), 1_234);
	}

	#[test]
	fn output_buffer_capacity_preserved_on_failure() {
		let mut input_data = Vec::with_capacity(1_234);
		input_data.extend_from_slice(&[1, 2, 3, 4, 5][..]);

		let error = execute(
			CODE_SIMPLE_ASSERT,
			input_data,
			MockExt::default(),
			&mut GasMeter::with_limit(50_000, 1),
		).err().unwrap();

		assert_eq!(error.buffer.capacity(), 1_234);
	}

	const CODE_RETURN_WITH_DATA: &str = r#"
(module
	(import "env" "ext_scratch_size" (func $ext_scratch_size (result i32)))
	(import "env" "ext_scratch_read" (func $ext_scratch_read (param i32 i32 i32)))
	(import "env" "ext_scratch_write" (func $ext_scratch_write (param i32 i32)))
	(import "env" "memory" (memory 1 1))

	;; Deploy routine is the same as call.
	(func (export "deploy") (result i32)
		(call $call)
	)

	;; Call reads the first 4 bytes (LE) as the exit status and returns the rest as output data.
	(func $call (export "call") (result i32)
		(local $buf_size i32)
		(local $exit_status i32)

		;; Find out the size of the scratch buffer
		(set_local $buf_size (call $ext_scratch_size))

		;; Copy scratch buffer into this contract memory.
		(call $ext_scratch_read
			(i32.const 0)		;; The pointer where to store the scratch buffer contents,
			(i32.const 0)		;; Offset from the start of the scratch buffer.
			(get_local $buf_size)		;; Count of bytes to copy.
		)

		;; Copy all but the first 4 bytes of the input data as the output data.
		(call $ext_scratch_write
			(i32.const 4)		;; Offset from the start of the scratch buffer.
			(i32.sub		;; Count of bytes to copy.
				(get_local $buf_size)
				(i32.const 4)
			)
		)

		;; Return the first 4 bytes of the input data as the exit status.
		(i32.load (i32.const 0))
	)
)
"#;

	#[test]
	fn return_with_success_status() {
		let output = execute(
			CODE_RETURN_WITH_DATA,
			hex!("00112233445566778899").to_vec(),
			MockExt::default(),
			&mut GasMeter::with_limit(50_000, 1),
		).unwrap();

		assert_eq!(output, ExecReturnValue { status: 0, data: hex!("445566778899").to_vec() });
		assert!(output.is_success());
	}

	#[test]
	fn return_with_failure_status() {
		let output = execute(
			CODE_RETURN_WITH_DATA,
			hex!("112233445566778899").to_vec(),
			MockExt::default(),
			&mut GasMeter::with_limit(50_000, 1),
		).unwrap();

		assert_eq!(output, ExecReturnValue { status: 17, data: hex!("5566778899").to_vec() });
		assert!(!output.is_success());
	}

	const CODE_GET_RUNTIME_STORAGE: &str = r#"
(module
	(import "env" "ext_get_runtime_storage"
		(func $ext_get_runtime_storage (param i32 i32) (result i32))
	)
	(import "env" "ext_scratch_size" (func $ext_scratch_size (result i32)))
	(import "env" "ext_scratch_read" (func $ext_scratch_read (param i32 i32 i32)))
	(import "env" "ext_scratch_write" (func $ext_scratch_write (param i32 i32)))
	(import "env" "memory" (memory 1 1))

	(func (export "deploy"))

	(func $assert (param i32)
		(block $ok
			(br_if $ok
				(get_local 0)
			)
			(unreachable)
		)
	)

	(func $call (export "call")
		;; Load runtime storage for the first key and assert that it exists.
		(call $assert
			(i32.eq
				(call $ext_get_runtime_storage
					(i32.const 16)
					(i32.const 4)
				)
				(i32.const 0)
			)
		)

		;; assert $ext_scratch_size == 4
		(call $assert
			(i32.eq
				(call $ext_scratch_size)
				(i32.const 4)
			)
		)

		;; copy contents of the scratch buffer into the contract's memory.
		(call $ext_scratch_read
			(i32.const 4)		;; Pointer in memory to the place where to copy.
			(i32.const 0)		;; Offset from the start of the scratch buffer.
			(i32.const 4)		;; Count of bytes to copy.
		)

		;; assert that contents of the buffer is equal to the i32 value of 0x14144020.
		(call $assert
			(i32.eq
				(i32.load
					(i32.const 4)
				)
				(i32.const 0x14144020)
			)
		)

		;; Load the second key and assert that it doesn't exist.
		(call $assert
			(i32.eq
				(call $ext_get_runtime_storage
					(i32.const 20)
					(i32.const 4)
				)
				(i32.const 1)
			)
		)
	)

	;; The first key, 4 bytes long.
	(data (i32.const 16) "\01\02\03\04")
	;; The second key, 4 bytes long.
	(data (i32.const 20) "\02\03\04\05")
)
"#;

	#[test]
	fn get_runtime_storage() {
		let mut gas_meter = GasMeter::with_limit(50_000, 1);
		let mock_ext = MockExt::default();

		// "\01\02\03\04" - Some(0x14144020)
		// "\02\03\04\05" - None
		*mock_ext.runtime_storage_keys.borrow_mut() = [
			([1, 2, 3, 4].to_vec(), Some(0x14144020u32.to_le_bytes().to_vec())),
			([2, 3, 4, 5].to_vec().to_vec(), None),
		]
		.iter()
		.cloned()
		.collect();
		let _ = execute(
			CODE_GET_RUNTIME_STORAGE,
			vec![],
			mock_ext,
			&mut gas_meter,
		).unwrap();
	}
}
