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
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

// NOTE: we allow missing docs here because arg_enum! creates the function variants without doc
#![allow(missing_docs)]

use structopt::clap::arg_enum;

arg_enum! {
	/// How to execute Wasm runtime code
	#[allow(missing_docs)]
	#[derive(Debug, Clone, Copy)]
	pub enum WasmExecutionMethod {
		// Uses an interpreter.
		Interpreted,
		// Uses a compiled runtime.
		Compiled,
	}
}

impl WasmExecutionMethod {
	/// Returns list of variants that are not disabled by feature flags.
	pub fn enabled_variants() -> Vec<&'static str> {
		Self::variants()
			.iter()
			.cloned()
			.filter(|&name| cfg!(feature = "wasmtime") || name != "Compiled")
			.collect()
	}
}

impl Into<sc_service::config::WasmExecutionMethod> for WasmExecutionMethod {
	fn into(self) -> sc_service::config::WasmExecutionMethod {
		match self {
			WasmExecutionMethod::Interpreted => {
				sc_service::config::WasmExecutionMethod::Interpreted
			}
			#[cfg(feature = "wasmtime")]
			WasmExecutionMethod::Compiled => sc_service::config::WasmExecutionMethod::Compiled,
			#[cfg(not(feature = "wasmtime"))]
			WasmExecutionMethod::Compiled => panic!(
				"Substrate must be compiled with \"wasmtime\" feature for compiled Wasm execution"
			),
		}
	}
}

arg_enum! {
	#[allow(missing_docs)]
	#[derive(Debug, Copy, Clone, PartialEq, Eq)]
	pub enum TracingReceiver {
		Log,
		Telemetry,
	}
}

impl Into<sc_tracing::TracingReceiver> for TracingReceiver {
	fn into(self) -> sc_tracing::TracingReceiver {
		match self {
			TracingReceiver::Log => sc_tracing::TracingReceiver::Log,
			TracingReceiver::Telemetry => sc_tracing::TracingReceiver::Telemetry,
		}
	}
}

arg_enum! {
	#[allow(missing_docs)]
	#[derive(Debug, Copy, Clone, PartialEq, Eq)]
	pub enum NodeKeyType {
		Ed25519
	}
}

arg_enum! {
	/// How to execute blocks
	#[derive(Debug, Clone, Copy, PartialEq, Eq)]
	pub enum ExecutionStrategy {
		// Execute with native build (if available, WebAssembly otherwise).
		Native,
		// Only execute with the WebAssembly build.
		Wasm,
		// Execute with both native (where available) and WebAssembly builds.
		Both,
		// Execute with the native build if possible; if it fails, then execute with WebAssembly.
		NativeElseWasm,
	}
}

impl Into<sc_client_api::ExecutionStrategy> for ExecutionStrategy {
	fn into(self) -> sc_client_api::ExecutionStrategy {
		match self {
			ExecutionStrategy::Native => sc_client_api::ExecutionStrategy::NativeWhenPossible,
			ExecutionStrategy::Wasm => sc_client_api::ExecutionStrategy::AlwaysWasm,
			ExecutionStrategy::Both => sc_client_api::ExecutionStrategy::Both,
			ExecutionStrategy::NativeElseWasm => sc_client_api::ExecutionStrategy::NativeElseWasm,
		}
	}
}

impl ExecutionStrategy {
	/// Returns the variant as `'&static str`.
	pub fn as_str(&self) -> &'static str {
		match self {
			Self::Native => "Native",
			Self::Wasm => "Wasm",
			Self::Both => "Both",
			Self::NativeElseWasm => "NativeElseWasm",
		}
	}
}

/// Default value for the `--execution-syncing` parameter.
pub const DEFAULT_EXECUTION_SYNCING: ExecutionStrategy = ExecutionStrategy::NativeElseWasm;
/// Default value for the `--execution-import-block` parameter.
pub const DEFAULT_EXECUTION_IMPORT_BLOCK: ExecutionStrategy = ExecutionStrategy::NativeElseWasm;
/// Default value for the `--execution-block-construction` parameter.
pub const DEFAULT_EXECUTION_BLOCK_CONSTRUCTION: ExecutionStrategy = ExecutionStrategy::Wasm;
/// Default value for the `--execution-offchain-worker` parameter.
pub const DEFAULT_EXECUTION_OFFCHAIN_WORKER: ExecutionStrategy = ExecutionStrategy::Native;
/// Default value for the `--execution-other` parameter.
pub const DEFAULT_EXECUTION_OTHER: ExecutionStrategy = ExecutionStrategy::Native;
