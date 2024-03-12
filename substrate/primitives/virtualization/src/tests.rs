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

use crate::{ExecError, Memory, MemoryT, SharedState, Virt, VirtT};
use alloc::vec::Vec;

const GAS_MAX: u64 = i64::MAX as u64;

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
	exit_prevents_program_launch(program);
	run_out_of_gas_works(program);
	gas_consumption_works(program);
	memory_reset_on_instantiate(program);
	memory_persistent(program);
	counter_in_subcall(program);
}

#[derive(Default)]
struct State {
	counter: u64,
	memory: Option<Memory>,
	program: Vec<u8>,
}

/// The host function implementation for our test fixture.
extern "C" fn syscall_handler(
	state: &mut SharedState<State>,
	syscall_no: u32,
	a0: u32,
	_a1: u32,
	_a2: u32,
	_a3: u32,
	_a4: u32,
	_a5: u32,
) -> u64 {
	match syscall_no {
		// read_counter
		// memory is used for passing args in order to test memory access
		1 => {
			let buf = state.user.counter.to_le_bytes();
			state.user.memory.as_mut().unwrap().write(a0, buf.as_ref()).unwrap();
			syscall_no.into()
		},
		// increment counter
		// memory is used for passing args in order to test memory access
		2 => {
			let mut buf = [0u8; 8];
			state.user.memory.as_ref().unwrap().read(a0, buf.as_mut()).unwrap();
			state.user.counter += u64::from_le_bytes(buf);
			u64::from(syscall_no) << 56
		},
		// exit
		3 => {
			state.exit = true;
			0
		},
		// call counter function in a new instance
		4 => {
			let instance = Virt::instantiate(state.user.program.as_ref()).unwrap();
			let mut sub_state = SharedState {
				gas_left: GAS_MAX,
				exit: false,
				user: State { memory: Some(instance.memory()), ..Default::default() },
			};
			let ret = instance.execute_and_destroy("counter", syscall_handler, &mut sub_state);
			assert_eq!(ret, Ok(()));
			assert!(!sub_state.exit);
			assert_eq!(sub_state.user.counter, 8);
			0
		},
		_ => panic!("unknown syscall: {:?}", syscall_no),
	}
}

/// Checks memory access and user state functionality.
fn counter_start_at_0(program: &[u8]) {
	let mut instance = Virt::instantiate(program).unwrap();
	let mut state = SharedState {
		gas_left: GAS_MAX,
		exit: false,
		user: State { counter: 0, memory: Some(instance.memory()), ..Default::default() },
	};
	let ret = instance.execute("counter", syscall_handler, &mut state);
	assert_eq!(ret, Ok(()));
	assert!(!state.exit);
	assert_eq!(state.user.counter, 8);
}

/// Checks memory access and user state functionality.
fn counter_start_at_7(program: &[u8]) {
	let mut instance = Virt::instantiate(program).unwrap();
	let mut state = SharedState {
		gas_left: GAS_MAX,
		exit: false,
		user: State { counter: 7, memory: Some(instance.memory()), ..Default::default() },
	};
	let ret = instance.execute("counter", syscall_handler, &mut state);
	assert_eq!(ret, Ok(()));
	assert!(!state.exit);
	assert_eq!(state.user.counter, 15);
}

/// Makes sure user state is persistent between calls into the same instance.
fn counter_multiple_calls(program: &[u8]) {
	let mut instance = Virt::instantiate(program).unwrap();
	let mut state = SharedState {
		gas_left: GAS_MAX,
		exit: false,
		user: State { counter: 7, memory: Some(instance.memory()), ..Default::default() },
	};
	let ret = instance.execute("counter", syscall_handler, &mut state);
	assert_eq!(ret, Ok(()));
	assert!(!state.exit);
	assert_eq!(state.user.counter, 15);

	let ret = instance.execute("counter", syscall_handler, &mut state);
	assert_eq!(ret, Ok(()));
	assert!(!state.exit);
	assert_eq!(state.user.counter, 23);
}

/// Check the correct status is returned when hitting an `unimp` instruction.
fn panic_works(program: &[u8]) {
	let instance = Virt::instantiate(program).unwrap();
	let mut state = SharedState {
		gas_left: GAS_MAX,
		exit: false,
		user: State { counter: 0, memory: Some(instance.memory()), ..Default::default() },
	};
	let ret = instance.execute_and_destroy("do_panic", syscall_handler, &mut state);
	assert_eq!(ret, Err(ExecError::Trap));
	assert!(!state.exit);
	assert_eq!(state.user.counter, 0);
}

/// Check that setting exit in a host function aborts the execution.
fn exit_works(program: &[u8]) {
	let instance = Virt::instantiate(program).unwrap();
	let mut state = SharedState {
		gas_left: GAS_MAX,
		exit: false,
		user: State { counter: 0, memory: Some(instance.memory()), ..Default::default() },
	};
	let ret = instance.execute_and_destroy("do_exit", syscall_handler, &mut state);
	assert_eq!(ret, Err(ExecError::Trap));
	assert!(state.exit);
	assert_eq!(state.user.counter, 0);
}

/// Setting exit to true prevents the program from even launching.
fn exit_prevents_program_launch(program: &[u8]) {
	let instance = Virt::instantiate(program).unwrap();
	let mut state = SharedState {
		gas_left: GAS_MAX,
		exit: true,
		user: State { counter: 7, memory: Some(instance.memory()), ..Default::default() },
	};
	let ret = instance.execute_and_destroy("add_99", syscall_handler, &mut state);
	assert_eq!(ret, Ok(()));
	assert!(state.exit);
	assert_eq!(state.user.counter, 7);
	assert_eq!(state.gas_left, GAS_MAX);
}

/// Increment the counter in an endless loop until we run out of gas.
fn run_out_of_gas_works(program: &[u8]) {
	let instance = Virt::instantiate(program).unwrap();
	let mut state = SharedState {
		gas_left: 100_000,
		exit: false,
		user: State { counter: 0, memory: Some(instance.memory()), ..Default::default() },
	};
	let ret = instance.execute_and_destroy("increment_forever", syscall_handler, &mut state);
	assert_eq!(ret, Err(ExecError::OutOfGas));
	assert!(!state.exit);
	assert_eq!(state.user.counter, 6_666);
	assert_eq!(state.gas_left, 0);
}

/// Call same function with different gas limits and make sure they consume the same amount of gas.
fn gas_consumption_works(program: &[u8]) {
	let gas_limit_0 = GAS_MAX;
	let gas_limit_1 = gas_limit_0 / 2;

	let mut instance = Virt::instantiate(program).unwrap();
	let mut state = SharedState {
		gas_left: gas_limit_0,
		exit: false,
		user: State { counter: 0, memory: Some(instance.memory()), ..Default::default() },
	};
	let ret = instance.execute("counter", syscall_handler, &mut state);
	let gas_consumed = gas_limit_0 - state.gas_left;
	assert_eq!(ret, Ok(()));

	let mut instance = Virt::instantiate(program).unwrap();
	let mut state = SharedState {
		gas_left: gas_limit_1,
		exit: false,
		user: State { counter: 0, memory: Some(instance.memory()), ..Default::default() },
	};
	let ret = instance.execute("counter", syscall_handler, &mut state);
	assert_eq!(ret, Ok(()));
	assert_eq!(gas_consumed, gas_limit_1 - state.gas_left);
}

/// Make sure that globals are reset for a new instance.
fn memory_reset_on_instantiate(program: &[u8]) {
	let mut instance = Virt::instantiate(program).unwrap();
	let mut state = SharedState {
		gas_left: GAS_MAX,
		exit: false,
		user: State { counter: 0, memory: Some(instance.memory()), ..Default::default() },
	};
	let ret = instance.execute("offset", syscall_handler, &mut state);
	assert_eq!(ret, Ok(()));
	assert_eq!(state.user.counter, 3);

	let mut instance = Virt::instantiate(program).unwrap();
	let ret = instance.execute("offset", syscall_handler, &mut state);
	assert_eq!(ret, Ok(()));
	assert_eq!(state.user.counter, 6);
}

/// Make sure globals are not reset between multiple calls into the same instance.
fn memory_persistent(program: &[u8]) {
	let mut instance = Virt::instantiate(program).unwrap();
	let mut state = SharedState {
		gas_left: GAS_MAX,
		exit: false,
		user: State { counter: 0, memory: Some(instance.memory()), ..Default::default() },
	};
	let ret = instance.execute("offset", syscall_handler, &mut state);
	assert_eq!(ret, Ok(()));
	assert_eq!(state.user.counter, 3);

	let ret = instance.execute("offset", syscall_handler, &mut state);
	assert_eq!(ret, Ok(()));
	assert_eq!(state.user.counter, 7);
}

/// Calls a function that spawns another instance where it calls the `counter` entry point.
fn counter_in_subcall(program: &[u8]) {
	let mut instance = Virt::instantiate(program).unwrap();
	let mut state = SharedState {
		gas_left: GAS_MAX,
		exit: false,
		user: State { counter: 0, memory: Some(instance.memory()), program: program.to_vec() },
	};
	let ret = instance.execute("do_subcall", syscall_handler, &mut state);
	assert_eq!(ret, Ok(()));
	assert!(!state.exit);
	// sub call should not affect parent state
	assert_eq!(state.user.counter, 0);
}

#[cfg(test)]
#[test]
fn tests() {
	sp_tracing::try_init_simple();
	run(sp_virtualization_test_fixture::binary());
}
