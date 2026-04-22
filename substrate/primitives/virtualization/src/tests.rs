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

use crate::{ExecAction, ExecError, ExecOutcome, MemoryT, Virt, VirtT};

const GAS_MAX: i64 = i64::MAX;

/// Run all tests.
///
/// This is exported even without a test build in order to make it callable from the
/// `sc-runtime-test`. This is necessary in order to compile these tests into a runtime so that
/// the forwarder implementation is used. Otherwise only the native implementation is tested through
/// cargos test framework.
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
}

/// The result of running a program to completion.
enum RunResult {
	/// Execution finished normally.
	Ok,
	/// A syscall handler signalled exit.
	Exit,
	/// Execution returned an error.
	Err(ExecError),
}

/// Drives the execute/resume loop calling `handler` for each syscall.
///
/// The closure receives `(syscall_no, a0, a1, a2, a3, a4, a5)` and returns
/// `Ok(return_value)` to resume or `Err(())` to signal exit (trap).
fn run_loop(
	virt: &mut Virt,
	function: &str,
	gas_left: &mut i64,
	mut handler: impl FnMut(u32, u64, u64, u64, u64, u64, u64) -> Result<u64, ()>,
) -> RunResult {
	let mut action = ExecAction::Execute(function);
	loop {
		let outcome = match virt.run(*gas_left, action) {
			Ok(outcome) => outcome,
			Err(ExecError::OutOfGas) => {
				*gas_left = 0;
				return RunResult::Err(ExecError::OutOfGas);
			},
			Err(err) => return RunResult::Err(err),
		};
		match outcome {
			ExecOutcome::Finished { gas_left: g } => {
				*gas_left = g;
				return RunResult::Ok;
			},
			ExecOutcome::Syscall { gas_left: g, syscall_no, a0, a1, a2, a3, a4, a5 } => {
				*gas_left = g;
				match handler(syscall_no, a0, a1, a2, a3, a4, a5) {
					Ok(result) => action = ExecAction::Resume(result),
					Err(()) => return RunResult::Exit,
				}
			},
		}
	}
}

/// The standard syscall handler for the test fixture.
///
/// Captures `counter` and `memory` from the caller.
fn make_handler<'a>(
	counter: &'a mut u64,
	memory: &'a mut <Virt as VirtT>::Memory,
) -> impl FnMut(u32, u64, u64, u64, u64, u64, u64) -> Result<u64, ()> + 'a {
	move |syscall_no, a0, _a1, _a2, _a3, _a4, _a5| match syscall_no {
		// read_counter
		1 => {
			let buf = counter.to_le_bytes();
			memory.write(a0 as u32, buf.as_ref()).unwrap();
			Ok(syscall_no.into())
		},
		// increment counter
		2 => {
			let mut buf = [0u8; 8];
			memory.read(a0 as u32, buf.as_mut()).unwrap();
			*counter += u64::from_le_bytes(buf);
			Ok(u64::from(syscall_no) << 56)
		},
		// exit
		3 => Err(()),
		_ => panic!("unknown syscall: {:?}", syscall_no),
	}
}

/// Checks memory access and user state functionality.
fn counter_start_at_0(program: &[u8]) {
	let mut instance = Virt::instantiate(program).unwrap();
	let mut gas_left = GAS_MAX;
	let mut counter: u64 = 0;
	let mut memory = instance.memory();
	let result =
		run_loop(&mut instance, "counter", &mut gas_left, make_handler(&mut counter, &mut memory));
	assert!(matches!(result, RunResult::Ok));
	assert_eq!(counter, 8);
}

/// Checks memory access and user state functionality.
fn counter_start_at_7(program: &[u8]) {
	let mut instance = Virt::instantiate(program).unwrap();
	let mut gas_left = GAS_MAX;
	let mut counter: u64 = 7;
	let mut memory = instance.memory();
	let result =
		run_loop(&mut instance, "counter", &mut gas_left, make_handler(&mut counter, &mut memory));
	assert!(matches!(result, RunResult::Ok));
	assert_eq!(counter, 15);
}

/// Makes sure user state is persistent between calls into the same instance.
fn counter_multiple_calls(program: &[u8]) {
	let mut instance = Virt::instantiate(program).unwrap();
	let mut gas_left = GAS_MAX;
	let mut counter: u64 = 7;
	let mut memory = instance.memory();

	let result =
		run_loop(&mut instance, "counter", &mut gas_left, make_handler(&mut counter, &mut memory));
	assert!(matches!(result, RunResult::Ok));
	assert_eq!(counter, 15);

	let result =
		run_loop(&mut instance, "counter", &mut gas_left, make_handler(&mut counter, &mut memory));
	assert!(matches!(result, RunResult::Ok));
	assert_eq!(counter, 23);
}

/// Check the correct status is returned when hitting an `unimp` instruction.
fn panic_works(program: &[u8]) {
	let mut instance = Virt::instantiate(program).unwrap();
	let mut gas_left = GAS_MAX;
	let mut counter: u64 = 0;
	let mut memory = instance.memory();
	let result =
		run_loop(&mut instance, "do_panic", &mut gas_left, make_handler(&mut counter, &mut memory));
	assert!(matches!(result, RunResult::Err(ExecError::Trap)));
	assert_eq!(counter, 0);
}

/// Check that setting exit in a host function aborts the execution.
fn exit_works(program: &[u8]) {
	let mut instance = Virt::instantiate(program).unwrap();
	let mut gas_left = GAS_MAX;
	let mut counter: u64 = 0;
	let mut memory = instance.memory();
	let result =
		run_loop(&mut instance, "do_exit", &mut gas_left, make_handler(&mut counter, &mut memory));
	assert!(matches!(result, RunResult::Exit));
	assert_eq!(counter, 0);
}

/// Increment the counter in an endless loop until we run out of gas.
fn run_out_of_gas_works(program: &[u8]) {
	let mut instance = Virt::instantiate(program).unwrap();
	let mut gas_left: i64 = 100_000;
	let mut counter: u64 = 0;
	let mut memory = instance.memory();
	let result = run_loop(
		&mut instance,
		"increment_forever",
		&mut gas_left,
		make_handler(&mut counter, &mut memory),
	);
	assert!(matches!(result, RunResult::Err(ExecError::OutOfGas)));
	assert_eq!(counter, 14_285);
	assert_eq!(gas_left, 0);
}

/// Call same function with different gas limits and make sure they consume the same amount of gas.
fn gas_consumption_works(program: &[u8]) {
	let gas_limit_0 = GAS_MAX;
	let gas_limit_1 = gas_limit_0 / 2;

	let mut instance = Virt::instantiate(program).unwrap();
	let mut gas_left = gas_limit_0;
	let mut counter: u64 = 0;
	let mut memory = instance.memory();
	let result =
		run_loop(&mut instance, "counter", &mut gas_left, make_handler(&mut counter, &mut memory));
	assert!(matches!(result, RunResult::Ok));
	let gas_consumed = gas_limit_0 - gas_left;

	let mut instance = Virt::instantiate(program).unwrap();
	let mut gas_left = gas_limit_1;
	let mut counter: u64 = 0;
	let mut memory = instance.memory();
	let result =
		run_loop(&mut instance, "counter", &mut gas_left, make_handler(&mut counter, &mut memory));
	assert!(matches!(result, RunResult::Ok));
	assert_eq!(gas_consumed, gas_limit_1 - gas_left);
}

/// Make sure that globals are reset for a new instance.
fn memory_reset_on_instantiate(program: &[u8]) {
	let mut instance = Virt::instantiate(program).unwrap();
	let mut gas_left = GAS_MAX;
	let mut counter: u64 = 0;
	let mut memory = instance.memory();
	let result =
		run_loop(&mut instance, "offset", &mut gas_left, make_handler(&mut counter, &mut memory));
	assert!(matches!(result, RunResult::Ok));
	assert_eq!(counter, 3);

	let mut instance = Virt::instantiate(program).unwrap();
	let mut memory = instance.memory();
	let result =
		run_loop(&mut instance, "offset", &mut gas_left, make_handler(&mut counter, &mut memory));
	assert!(matches!(result, RunResult::Ok));
	assert_eq!(counter, 6);
}

/// Make sure globals are not reset between multiple calls into the same instance.
fn memory_persistent(program: &[u8]) {
	let mut instance = Virt::instantiate(program).unwrap();
	let mut gas_left = GAS_MAX;
	let mut counter: u64 = 0;
	let mut memory = instance.memory();

	let result =
		run_loop(&mut instance, "offset", &mut gas_left, make_handler(&mut counter, &mut memory));
	assert!(matches!(result, RunResult::Ok));
	assert_eq!(counter, 3);

	let result =
		run_loop(&mut instance, "offset", &mut gas_left, make_handler(&mut counter, &mut memory));
	assert!(matches!(result, RunResult::Ok));
	assert_eq!(counter, 7);
}

/// Calls a function that spawns another instance where it calls the `counter` entry point.
fn counter_in_subcall(program: &[u8]) {
	let mut instance = Virt::instantiate(program).unwrap();
	let mut gas_left = GAS_MAX;
	let mut counter: u64 = 0;
	let mut memory = instance.memory();
	let program = program.to_vec();
	let result = run_loop(
		&mut instance,
		"do_subcall",
		&mut gas_left,
		|syscall_no, a0, a1, a2, a3, a4, a5| {
			match syscall_no {
				1..=3 => {
					make_handler(&mut counter, &mut memory)(syscall_no, a0, a1, a2, a3, a4, a5)
				},
				// subcall: spawn a new instance and run counter in it
				4 => {
					let mut sub_instance = Virt::instantiate(program.as_ref()).unwrap();
					let mut sub_gas = GAS_MAX;
					let mut sub_counter: u64 = 0;
					let mut sub_memory = sub_instance.memory();
					let result = run_loop(
						&mut sub_instance,
						"counter",
						&mut sub_gas,
						make_handler(&mut sub_counter, &mut sub_memory),
					);
					assert!(matches!(result, RunResult::Ok));
					assert_eq!(sub_counter, 8);
					Ok(0)
				},
				_ => panic!("unknown syscall: {:?}", syscall_no),
			}
		},
	);
	assert!(matches!(result, RunResult::Ok));
	// sub call should not affect parent state
	assert_eq!(counter, 0);
}

#[cfg(test)]
#[test]
fn tests() {
	sp_tracing::try_init_simple();
	run(sp_virtualization_test_fixture::binary());
}
