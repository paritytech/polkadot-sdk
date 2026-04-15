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

use crate::{ExecError, ExecResult, Execution, Instance, Module, ModuleError};

const GAS_MAX: i64 = i64::MAX;

/// Run all tests.
///
/// This is exported even without a test build in order to make it callable from the
/// `sc-runtime-test`. This is necessary in order to compile these tests into a runtime so that
/// the forwarder implementation is used. Otherwise only the native implementation is tested through
/// cargos test framework.
///
/// Each test also has a standalone `#[test]` wrapper in the `tests` submodule below. Tests that
/// need `std`-only dependencies or pre-populated storage are only available as standalone tests.
///
/// The `program` needs to be set to `sp_virtualization_test_fixture::binary()`. It can't be
/// hard coded because when this crate is compiled into a runtime the binary is not available.
/// Instead, we pass it as an argument to the runtime exported function.
pub fn run(program: &[u8]) {
	counter_start_at_0(program);
	counter_start_at_7(program);
	counter_multiple_calls(program);
	panic_works(program);
	exit_works(program);
	run_out_of_gas_works(program);
	gas_consumption_works(program);
	memory_reset_on_instantiate(program);
	memory_persistent(program);
	counter_in_subcall(program);
	from_hash_not_found(program);
}

/// The result of running a program to completion.
enum RunResult {
	/// Execution finished normally. The idle instance is returned for reuse.
	Ok(Instance),
	/// A syscall handler signalled exit.
	Exit,
	/// Execution returned an error.
	Err(ExecError),
}

/// Drives the prepare/run loop calling `handler` for each syscall.
///
/// The closure receives `(execution, syscall_symbol, a0, a1, a2, a3, a4, a5)` and returns
/// `Ok(return_value)` to resume or `Err(())` to signal exit (trap).
fn run_loop(
	mut execution: Execution,
	gas_left: &mut i64,
	mut handler: impl FnMut(&mut Execution, &[u8], u64, u64, u64, u64, u64, u64) -> Result<u64, ()>,
) -> RunResult {
	let mut a0 = 0u64;
	loop {
		match execution.run(*gas_left, a0) {
			ExecResult::Finished { instance, gas_left: g } => {
				*gas_left = g;
				return RunResult::Ok(instance);
			},
			ExecResult::Syscall {
				execution: e,
				gas_left: g,
				syscall_symbol,
				a0: sa0,
				a1,
				a2,
				a3,
				a4,
				a5,
			} => {
				execution = e;
				*gas_left = g;
				match handler(&mut execution, syscall_symbol.as_ref(), sa0, a1, a2, a3, a4, a5) {
					Ok(result) => a0 = result,
					Err(()) => return RunResult::Exit,
				}
			},
			ExecResult::Error { instance: _, error: ExecError::OutOfGas } => {
				*gas_left = 0;
				return RunResult::Err(ExecError::OutOfGas);
			},
			ExecResult::Error { instance: _, error } => return RunResult::Err(error),
		}
	}
}

/// The standard syscall handler for the test fixture.
///
/// Captures `counter` from the caller; memory access goes through the `&mut Execution` passed
/// on each invocation.
fn make_handler<'a>(
	counter: &'a mut u64,
) -> impl FnMut(&mut Execution, &[u8], u64, u64, u64, u64, u64, u64) -> Result<u64, ()> + 'a {
	move |execution, syscall_symbol, a0, _a1, _a2, _a3, _a4, _a5| match syscall_symbol {
		b"read_counter" => {
			let buf = counter.to_le_bytes();
			execution.write_memory(a0 as u32, buf.as_ref()).unwrap();
			Ok(1)
		},
		b"increment_counter" => {
			let mut buf = [0u8; 8];
			execution.read_memory(a0 as u32, buf.as_mut()).unwrap();
			*counter += u64::from_le_bytes(buf);
			Ok(2u64 << 56)
		},
		b"exit" => Err(()),
		_ => panic!("unknown syscall: {:?}", syscall_symbol),
	}
}

/// Checks memory access and user state functionality.
fn counter_start_at_0(program: &[u8]) {
	let instance = Module::from_bytes(program).unwrap().instantiate().unwrap();
	let execution = instance.prepare(b"counter").unwrap();
	let mut gas_left = GAS_MAX;
	let mut counter: u64 = 0;
	let result = run_loop(execution, &mut gas_left, make_handler(&mut counter));
	assert!(matches!(result, RunResult::Ok(_)));
	assert_eq!(counter, 8);
}

/// Checks memory access and user state functionality.
fn counter_start_at_7(program: &[u8]) {
	let instance = Module::from_bytes(program).unwrap().instantiate().unwrap();
	let execution = instance.prepare(b"counter").unwrap();
	let mut gas_left = GAS_MAX;
	let mut counter: u64 = 7;
	let result = run_loop(execution, &mut gas_left, make_handler(&mut counter));
	assert!(matches!(result, RunResult::Ok(_)));
	assert_eq!(counter, 15);
}

/// Makes sure user state is persistent between calls into the same instance.
fn counter_multiple_calls(program: &[u8]) {
	let instance = Module::from_bytes(program).unwrap().instantiate().unwrap();
	let execution = instance.prepare(b"counter").unwrap();
	let mut gas_left = GAS_MAX;
	let mut counter: u64 = 7;

	let instance = match run_loop(execution, &mut gas_left, make_handler(&mut counter)) {
		RunResult::Ok(instance) => instance,
		_ => panic!("expected Ok"),
	};
	assert_eq!(counter, 15);

	let execution = instance.prepare(b"counter").unwrap();
	let result = run_loop(execution, &mut gas_left, make_handler(&mut counter));
	assert!(matches!(result, RunResult::Ok(_)));
	assert_eq!(counter, 23);
}

/// Check the correct status is returned when hitting an `unimp` instruction.
fn panic_works(program: &[u8]) {
	let instance = Module::from_bytes(program).unwrap().instantiate().unwrap();
	let execution = instance.prepare(b"do_panic").unwrap();
	let mut gas_left = GAS_MAX;
	let mut counter: u64 = 0;
	let result = run_loop(execution, &mut gas_left, make_handler(&mut counter));
	assert!(matches!(result, RunResult::Err(ExecError::Trap)));
	assert_eq!(counter, 0);
}

/// Check that setting exit in a host function aborts the execution.
fn exit_works(program: &[u8]) {
	let instance = Module::from_bytes(program).unwrap().instantiate().unwrap();
	let execution = instance.prepare(b"do_exit").unwrap();
	let mut gas_left = GAS_MAX;
	let mut counter: u64 = 0;
	let result = run_loop(execution, &mut gas_left, make_handler(&mut counter));
	assert!(matches!(result, RunResult::Exit));
	assert_eq!(counter, 0);
}

/// Increment the counter in an endless loop until we run out of gas.
fn run_out_of_gas_works(program: &[u8]) {
	let instance = Module::from_bytes(program).unwrap().instantiate().unwrap();
	let execution = instance.prepare(b"increment_forever").unwrap();
	let mut gas_left: i64 = 100_000;
	let mut counter: u64 = 0;
	let result = run_loop(execution, &mut gas_left, make_handler(&mut counter));
	assert!(matches!(result, RunResult::Err(ExecError::OutOfGas)));
	assert_eq!(counter, 793);
	assert_eq!(gas_left, 0);
}

/// Call same function with different gas limits and make sure they consume the same amount of gas.
fn gas_consumption_works(program: &[u8]) {
	let gas_limit_0 = GAS_MAX;
	let gas_limit_1 = gas_limit_0 / 2;

	let instance = Module::from_bytes(program).unwrap().instantiate().unwrap();
	let execution = instance.prepare(b"counter").unwrap();
	let mut gas_left = gas_limit_0;
	let mut counter: u64 = 0;
	let result = run_loop(execution, &mut gas_left, make_handler(&mut counter));
	assert!(matches!(result, RunResult::Ok(_)));
	let gas_consumed = gas_limit_0 - gas_left;

	let instance = Module::from_bytes(program).unwrap().instantiate().unwrap();
	let execution = instance.prepare(b"counter").unwrap();
	let mut gas_left = gas_limit_1;
	let mut counter: u64 = 0;
	let result = run_loop(execution, &mut gas_left, make_handler(&mut counter));
	assert!(matches!(result, RunResult::Ok(_)));
	assert_eq!(gas_consumed, gas_limit_1 - gas_left);
}

/// Make sure that globals are reset for a new instance.
fn memory_reset_on_instantiate(program: &[u8]) {
	let instance = Module::from_bytes(program).unwrap().instantiate().unwrap();
	let execution = instance.prepare(b"offset").unwrap();
	let mut gas_left = GAS_MAX;
	let mut counter: u64 = 0;
	let result = run_loop(execution, &mut gas_left, make_handler(&mut counter));
	assert!(matches!(result, RunResult::Ok(_)));
	assert_eq!(counter, 3);

	let instance = Module::from_bytes(program).unwrap().instantiate().unwrap();
	let execution = instance.prepare(b"offset").unwrap();
	let result = run_loop(execution, &mut gas_left, make_handler(&mut counter));
	assert!(matches!(result, RunResult::Ok(_)));
	assert_eq!(counter, 6);
}

/// Make sure globals are not reset between multiple calls into the same instance.
fn memory_persistent(program: &[u8]) {
	let instance = Module::from_bytes(program).unwrap().instantiate().unwrap();
	let execution = instance.prepare(b"offset").unwrap();
	let mut gas_left = GAS_MAX;
	let mut counter: u64 = 0;

	let instance = match run_loop(execution, &mut gas_left, make_handler(&mut counter)) {
		RunResult::Ok(instance) => instance,
		_ => panic!("expected Ok"),
	};
	assert_eq!(counter, 3);

	let execution = instance.prepare(b"offset").unwrap();
	let result = run_loop(execution, &mut gas_left, make_handler(&mut counter));
	assert!(matches!(result, RunResult::Ok(_)));
	assert_eq!(counter, 7);
}

/// Calls a function that spawns another instance where it calls the `counter` entry point.
fn counter_in_subcall(program: &[u8]) {
	let instance = Module::from_bytes(program).unwrap().instantiate().unwrap();
	let execution = instance.prepare(b"do_subcall").unwrap();
	let mut gas_left = GAS_MAX;
	let mut counter: u64 = 0;
	let program = program.to_vec();
	let result =
		run_loop(execution, &mut gas_left, |execution, syscall_symbol, a0, a1, a2, a3, a4, a5| {
			match syscall_symbol {
				b"read_counter" | b"increment_counter" | b"exit" => {
					make_handler(&mut counter)(execution, syscall_symbol, a0, a1, a2, a3, a4, a5)
				},
				// subcall: spawn a new instance and run counter in it
				b"subcall" => {
					let sub_instance =
						Module::from_bytes(program.as_ref()).unwrap().instantiate().unwrap();
					let sub_execution = sub_instance.prepare(b"counter").unwrap();
					let mut sub_gas = GAS_MAX;
					let mut sub_counter: u64 = 0;
					let result =
						run_loop(sub_execution, &mut sub_gas, make_handler(&mut sub_counter));
					assert!(matches!(result, RunResult::Ok(_)));
					assert_eq!(sub_counter, 8);
					Ok(0)
				},
				_ => panic!("unknown syscall: {:?}", syscall_symbol),
			}
		});
	assert!(matches!(result, RunResult::Ok(_)));
	// sub call should not affect parent state
	assert_eq!(counter, 0);
}

/// Hash not in cache and no code in storage returns NotFound.
fn from_hash_not_found(_program: &[u8]) {
	let hash = [0u8; 32];
	assert!(matches!(Module::from_hash(&hash, b"prefix", b""), Err(ModuleError::NotFound)));
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::ModuleError;

	fn setup() -> sp_io::TestExternalities {
		sp_tracing::try_init_simple();
		let mut ext = sp_io::TestExternalities::default();
		ext.register_extension(crate::VirtManagerExt::default());
		ext
	}

	fn binary() -> &'static [u8] {
		sp_virtualization_test_fixture::binary()
	}

	#[test]
	fn counter_start_at_0() {
		setup().execute_with(|| super::counter_start_at_0(binary()));
	}

	#[test]
	fn counter_start_at_7() {
		setup().execute_with(|| super::counter_start_at_7(binary()));
	}

	#[test]
	fn counter_multiple_calls() {
		setup().execute_with(|| super::counter_multiple_calls(binary()));
	}

	#[test]
	fn panic_works() {
		setup().execute_with(|| super::panic_works(binary()));
	}

	#[test]
	fn exit_works() {
		setup().execute_with(|| super::exit_works(binary()));
	}

	#[test]
	fn run_out_of_gas_works() {
		setup().execute_with(|| super::run_out_of_gas_works(binary()));
	}

	#[test]
	fn gas_consumption_works() {
		setup().execute_with(|| super::gas_consumption_works(binary()));
	}

	#[test]
	fn memory_reset_on_instantiate() {
		setup().execute_with(|| super::memory_reset_on_instantiate(binary()));
	}

	#[test]
	fn memory_persistent() {
		setup().execute_with(|| super::memory_persistent(binary()));
	}

	#[test]
	fn counter_in_subcall() {
		setup().execute_with(|| super::counter_in_subcall(binary()));
	}

	#[test]
	fn from_hash_not_found() {
		setup().execute_with(|| super::from_hash_not_found(binary()));
	}

	/// Compile from bytes, then from_hash should hit the in-memory cache.
	#[test]
	fn from_hash_cache_hit() {
		let program = binary();
		setup().execute_with(|| {
			let _module = Module::from_bytes(program).unwrap();
			let hash = sp_crypto_hashing::keccak_256(program);
			let module = Module::from_hash(&hash, b"", b"").unwrap();
			let instance = module.instantiate().unwrap();
			let execution = instance.prepare(b"counter").unwrap();
			let mut gas_left = GAS_MAX;
			let mut counter: u64 = 0;
			let result = run_loop(execution, &mut gas_left, make_handler(&mut counter));
			assert!(matches!(result, RunResult::Ok(_)));
			assert_eq!(counter, 8);
		});
	}

	/// Load code from main trie storage on cache miss.
	#[test]
	fn from_hash_storage_main_trie() {
		let program = binary();
		let hash = sp_crypto_hashing::keccak_256(program);
		let prefix = b"code:";
		let mut key = prefix.to_vec();
		key.extend_from_slice(&hash);

		let mut ext = setup();
		ext.insert(key, program.to_vec());
		ext.execute_with(|| {
			let module = Module::from_hash(&hash, prefix, b"").unwrap();
			let instance = module.instantiate().unwrap();
			let execution = instance.prepare(b"counter").unwrap();
			let mut gas_left = GAS_MAX;
			let mut counter: u64 = 0;
			let result = run_loop(execution, &mut gas_left, make_handler(&mut counter));
			assert!(matches!(result, RunResult::Ok(_)));
			assert_eq!(counter, 8);

			// Second call should hit the cache now.
			let module = Module::from_hash(&hash, prefix, b"").unwrap();
			let instance = module.instantiate().unwrap();
			let execution = instance.prepare(b"counter").unwrap();
			let mut counter: u64 = 0;
			let result = run_loop(execution, &mut gas_left, make_handler(&mut counter));
			assert!(matches!(result, RunResult::Ok(_)));
			assert_eq!(counter, 8);
		});
	}

	/// Load code from child trie storage on cache miss.
	#[test]
	fn from_hash_storage_child_trie() {
		let program = binary();
		let hash = sp_crypto_hashing::keccak_256(program);
		let prefix = b"code:";
		let child_trie = b"contracts";
		let mut key = prefix.to_vec();
		key.extend_from_slice(&hash);
		let child_info = sp_storage::ChildInfo::new_default(child_trie);

		let mut ext = setup();
		ext.insert_child(child_info, key, program.to_vec());
		ext.execute_with(|| {
			let module = Module::from_hash(&hash, prefix, child_trie).unwrap();
			let instance = module.instantiate().unwrap();
			let execution = instance.prepare(b"counter").unwrap();
			let mut gas_left = GAS_MAX;
			let mut counter: u64 = 0;
			let result = run_loop(execution, &mut gas_left, make_handler(&mut counter));
			assert!(matches!(result, RunResult::Ok(_)));
			assert_eq!(counter, 8);
		});
	}

	/// Code at the storage key does not match the requested hash.
	#[test]
	fn from_hash_hash_mismatch() {
		let program = binary();
		let hash = sp_crypto_hashing::keccak_256(program);
		let prefix = b"code:";
		let mut key = prefix.to_vec();
		key.extend_from_slice(&hash);

		let mut ext = setup();
		ext.insert(key, b"not the real program".to_vec());
		ext.execute_with(|| {
			assert!(matches!(
				Module::from_hash(&hash, prefix, b""),
				Err(ModuleError::HashMismatch)
			));
		});
	}

	/// Code at the storage key has the correct hash but is not valid PolkaVM.
	#[test]
	fn from_hash_invalid_image() {
		let garbage = b"this is not a valid polkavm program";
		let hash = sp_crypto_hashing::keccak_256(garbage);
		let prefix = b"code:";
		let mut key = prefix.to_vec();
		key.extend_from_slice(&hash);

		let mut ext = setup();
		ext.insert(key, garbage.to_vec());
		ext.execute_with(|| {
			assert!(matches!(
				Module::from_hash(&hash, prefix, b""),
				Err(ModuleError::InvalidImage)
			));
		});
	}
}
