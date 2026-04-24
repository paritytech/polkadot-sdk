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

//! Integration tests for runtime interface primitives
#![cfg(test)]

use sp_runtime_interface::*;

use sp_runtime_interface_test_wasm::{test_api::HostFunctions, wasm_binary_unwrap};
use sp_runtime_interface_test_wasm_deprecated::{
	test_api::HostFunctions as DeprecatedHostFunctions,
	wasm_binary_unwrap as wasm_binary_deprecated_unwrap,
};

use sc_executor_common::{runtime_blob::RuntimeBlob, wasm_runtime::AllocationStats};
use sp_wasm_interface::{ExtendedHostFunctions, HostFunctions as HostFunctionsT};

use std::{
	collections::HashSet,
	sync::{Arc, Mutex},
};

type TestExternalities = sp_state_machine::TestExternalities<sp_runtime::traits::BlakeTwo256>;

fn call_wasm_method<HF: HostFunctionsT>(binary: &[u8], method: &str) -> TestExternalities {
	call_wasm_method_with_result::<HF>(binary, method)
		.0
		.expect(&format!("Failed to execute `{}`", method))
}

fn call_wasm_method_with_result<HF: HostFunctionsT>(
	binary: &[u8],
	method: &str,
) -> (Result<TestExternalities, String>, Option<AllocationStats>) {
	let mut ext = TestExternalities::default();
	let mut ext_ext = ext.ext();

	let executor = sc_executor::WasmExecutor::<
		ExtendedHostFunctions<sp_io::SubstrateHostFunctions, HF>,
	>::builder()
	.build();

	let (result, allocation_stats) = executor.uncached_call_with_allocation_stats(
		RuntimeBlob::uncompress_if_needed(binary).expect("Failed to parse binary"),
		&mut ext_ext,
		false,
		method,
		&[],
	);
	let result = result
		.map_err(|e| format!("Failed to execute `{}`: {}", method, e))
		.map(|_| ext);
	(result, allocation_stats)
}

// =========================================================================
// V2 entry point tests (test-wasm, runtime-side allocation)
// =========================================================================

#[test]
fn test_return_data() {
	call_wasm_method::<HostFunctions>(wasm_binary_unwrap(), "test_return_data");
}

#[test]
fn test_set_storage() {
	let mut ext = call_wasm_method::<HostFunctions>(wasm_binary_unwrap(), "test_set_storage");

	let expected = "world";
	assert_eq!(expected.as_bytes(), &ext.ext().storage("hello".as_bytes()).unwrap()[..]);
}

#[test]
fn test_return_value_into_mutable_reference() {
	call_wasm_method::<HostFunctions>(
		wasm_binary_unwrap(),
		"test_return_value_into_mutable_reference",
	);
}

#[test]
fn test_get_and_return_array() {
	call_wasm_method::<HostFunctions>(wasm_binary_unwrap(), "test_get_and_return_array");
}

#[test]
fn test_array_as_mutable_reference() {
	call_wasm_method::<HostFunctions>(wasm_binary_unwrap(), "test_array_as_mutable_reference");
}

#[test]
fn host_function_not_found() {
	let err = call_wasm_method_with_result::<()>(wasm_binary_unwrap(), "test_return_data")
		.0
		.unwrap_err();

	assert!(err.contains("test_return_data"));
	assert!(err.contains(" Failed to create module"));
}

#[test]
fn test_invalid_utf8_data_should_return_an_error() {
	call_wasm_method_with_result::<HostFunctions>(
		wasm_binary_unwrap(),
		"test_invalid_utf8_data_should_return_an_error",
	)
	.0
	.unwrap_err();
}

#[test]
fn test_overwrite_native_function_implementation() {
	call_wasm_method::<HostFunctions>(
		wasm_binary_unwrap(),
		"test_overwrite_native_function_implementation",
	);
}

#[test]
fn test_versioning_with_new_host_works() {
	// We call to the new wasm binary with new host function.
	call_wasm_method::<HostFunctions>(wasm_binary_unwrap(), "test_versioning_works");

	// We call to the old wasm binary with the deprecated host functions.
	// The deprecated wasm uses V1 marshalling strategies (AllocateAndReturn*) which have
	// incompatible signatures with the new V2 host functions, so we use the matching
	// DeprecatedHostFunctions that provide the correct V1 function signatures.
	call_wasm_method::<DeprecatedHostFunctions>(
		wasm_binary_deprecated_unwrap(),
		"test_versioning_works",
	);
}

#[test]
fn test_versioning_register_only() {
	call_wasm_method::<HostFunctions>(wasm_binary_unwrap(), "test_versioning_register_only_works");
}

#[test]
fn test_v2_marshalling_strategies() {
	call_wasm_method::<HostFunctions>(wasm_binary_unwrap(), "test_v2_marshalling_strategies");
}

fn run_test_in_another_process(
	test_name: &str,
	test_body: impl FnOnce(),
) -> Option<std::process::Output> {
	if std::env::var("RUN_FORKED_TEST").is_ok() {
		test_body();
		None
	} else {
		let output = std::process::Command::new(std::env::current_exe().unwrap())
			.arg(test_name)
			.env("RUN_FORKED_TEST", "1")
			.output()
			.unwrap();

		assert!(output.status.success());
		Some(output)
	}
}

#[test]
fn test_tracing() {
	// Run in a different process to ensure that the `Span` is registered with our local
	// `TracingSubscriber`.
	run_test_in_another_process("test_tracing", || {
		use std::fmt;
		use tracing::span::Id as SpanId;
		use tracing_core::field::{Field, Visit};

		#[derive(Clone)]
		struct TracingSubscriber(Arc<Mutex<Inner>>);

		struct FieldConsumer(&'static str, Option<String>);
		impl Visit for FieldConsumer {
			fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
				if field.name() == self.0 {
					self.1 = Some(format!("{:?}", value))
				}
			}
		}

		#[derive(Default)]
		struct Inner {
			spans: HashSet<String>,
		}

		impl tracing::subscriber::Subscriber for TracingSubscriber {
			fn enabled(&self, _: &tracing::Metadata) -> bool {
				true
			}

			fn new_span(&self, span: &tracing::span::Attributes) -> tracing::Id {
				let mut inner = self.0.lock().unwrap();
				let id = SpanId::from_u64((inner.spans.len() + 1) as _);
				let mut f = FieldConsumer("name", None);
				span.record(&mut f);
				inner.spans.insert(f.1.unwrap_or_else(|| span.metadata().name().to_owned()));
				id
			}

			fn record(&self, _: &SpanId, _: &tracing::span::Record) {}

			fn record_follows_from(&self, _: &SpanId, _: &SpanId) {}

			fn event(&self, _: &tracing::Event) {}

			fn enter(&self, _: &SpanId) {}

			fn exit(&self, _: &SpanId) {}
		}

		let subscriber = TracingSubscriber(Default::default());
		let _guard = tracing::subscriber::set_default(subscriber.clone());

		call_wasm_method::<HostFunctions>(wasm_binary_unwrap(), "test_return_data");

		let inner = subscriber.0.lock().unwrap();
		assert!(inner.spans.contains("return_input_version_1"));
	});
}

// =========================================================================
// V1 entry point tests (test-wasm-deprecated, host-side allocation)
// =========================================================================

#[test]
fn test_versioning_with_deprecated_wasm() {
	call_wasm_method::<DeprecatedHostFunctions>(
		wasm_binary_deprecated_unwrap(),
		"test_versioning_works",
	);
}

#[test]
fn test_return_data_v1() {
	call_wasm_method::<DeprecatedHostFunctions>(
		wasm_binary_deprecated_unwrap(),
		"test_return_data",
	);
}

#[test]
fn test_return_option_data_v1() {
	call_wasm_method::<DeprecatedHostFunctions>(
		wasm_binary_deprecated_unwrap(),
		"test_return_option_data",
	);
}

#[test]
fn test_get_and_return_array_v1() {
	call_wasm_method::<DeprecatedHostFunctions>(
		wasm_binary_deprecated_unwrap(),
		"test_get_and_return_array",
	);
}

#[test]
fn test_return_input_public_key_v1() {
	call_wasm_method::<DeprecatedHostFunctions>(
		wasm_binary_deprecated_unwrap(),
		"test_return_input_public_key",
	);
}

#[test]
fn test_return_input_as_tuple_v1() {
	call_wasm_method::<DeprecatedHostFunctions>(
		wasm_binary_deprecated_unwrap(),
		"test_return_input_as_tuple",
	);
}

#[test]
fn test_vec_return_value_memory_is_freed() {
	call_wasm_method::<DeprecatedHostFunctions>(
		wasm_binary_deprecated_unwrap(),
		"test_vec_return_value_memory_is_freed",
	);
}

#[test]
fn test_encoded_return_value_memory_is_freed() {
	call_wasm_method::<DeprecatedHostFunctions>(
		wasm_binary_deprecated_unwrap(),
		"test_encoded_return_value_memory_is_freed",
	);
}

#[test]
fn test_array_return_value_memory_is_freed() {
	call_wasm_method::<DeprecatedHostFunctions>(
		wasm_binary_deprecated_unwrap(),
		"test_array_return_value_memory_is_freed",
	);
}

#[test]
fn test_v1_marshalling_strategies() {
	call_wasm_method::<DeprecatedHostFunctions>(
		wasm_binary_deprecated_unwrap(),
		"test_v1_marshalling_strategies",
	);
}

#[test]
fn test_returning_option_bytes_from_a_host_function_is_efficient() {
	let (result, stats_vec) = call_wasm_method_with_result::<DeprecatedHostFunctions>(
		wasm_binary_deprecated_unwrap(),
		"test_return_option_vec",
	);
	result.unwrap();
	let (result, stats_bytes) = call_wasm_method_with_result::<DeprecatedHostFunctions>(
		wasm_binary_deprecated_unwrap(),
		"test_return_option_bytes",
	);
	result.unwrap();

	let stats_vec = stats_vec.unwrap();
	let stats_bytes = stats_bytes.unwrap();

	// With V1 entry points the host allocator is available. Option<Bytes> should be more
	// efficient than Option<Vec<u8>> due to zero-copy deserialization.
	assert_eq!(stats_bytes.bytes_allocated_sum + 16 * 1024 + 8, stats_vec.bytes_allocated_sum);
}
