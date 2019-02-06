// Copyright 2019 Parity Technologies (UK) Ltd.
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
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

extern crate substrate_client;
extern crate substrate_test_client as test_client;
extern crate sr_primitives as runtime_primitives;
extern crate substrate_state_machine as state_machine;

use test_client::runtime::{TestAPI, DecodeFails};
use runtime_primitives::{generic::BlockId, traits::ProvideRuntimeApi};
use state_machine::ExecutionStrategy;

fn calling_function_with_strat(strat: ExecutionStrategy) {
	let client = test_client::new_with_api_execution_strat(strat);
	let runtime_api = client.runtime_api();
	let block_id = BlockId::Number(client.info().unwrap().chain.best_number);

	assert_eq!(runtime_api.benchmark_add_one(&block_id, &1).unwrap(), 2);
}

#[test]
fn calling_native_runtime_function() {
	calling_function_with_strat(ExecutionStrategy::NativeWhenPossible);
}

#[test]
fn calling_wasm_runtime_function() {
	calling_function_with_strat(ExecutionStrategy::AlwaysWasm);
}

#[test]
#[should_panic(expected = "Could not convert parameter `param` between node and runtime!")]
fn calling_native_runtime_function_with_non_decodable_parameter() {
	let client = test_client::new_with_api_execution_strat(ExecutionStrategy::NativeWhenPossible);
	let runtime_api = client.runtime_api();
	let block_id = BlockId::Number(client.info().unwrap().chain.best_number);
	runtime_api.fail_convert_parameter(&block_id, DecodeFails::new()).unwrap();
}

#[test]
#[should_panic(expected = "Could not convert return value from runtime to node!")]
fn calling_native_runtime_function_with_non_decodable_return_value() {
	let client = test_client::new_with_api_execution_strat(ExecutionStrategy::NativeWhenPossible);
	let runtime_api = client.runtime_api();
	let block_id = BlockId::Number(client.info().unwrap().chain.best_number);
	runtime_api.fail_convert_return_value(&block_id).unwrap();
}

#[test]
fn calling_native_runtime_signature_changed_function() {
	let client = test_client::new_with_api_execution_strat(ExecutionStrategy::NativeWhenPossible);
	let runtime_api = client.runtime_api();
	let block_id = BlockId::Number(client.info().unwrap().chain.best_number);

	assert_eq!(runtime_api.function_signature_changed(&block_id).unwrap(), 1);
}

#[test]
fn calling_wasm_runtime_signature_changed_old_function() {
	let client = test_client::new_with_api_execution_strat(ExecutionStrategy::AlwaysWasm);
	let runtime_api = client.runtime_api();
	let block_id = BlockId::Number(client.info().unwrap().chain.best_number);

	#[allow(deprecated)]
	let res = runtime_api.function_signature_changed_before_version_2(&block_id).unwrap();
	assert_eq!(&res, &[1, 2]);
}
