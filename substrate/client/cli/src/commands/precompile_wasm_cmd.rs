// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::{
    arg_enums::{DEFAULT_WASMTIME_INSTANTIATION_STRATEGY, execution_method_from_cli, WasmExecutionMethod, WasmtimeInstantiationStrategy},
	error::{self, Error},
	params::{DatabaseParams, PruningParams, SharedParams},
    CliConfiguration
};

use clap::Parser;
use sc_client_api::{Backend, HeaderBackend};
use sc_executor::{
	HeapAllocStrategy, DEFAULT_HEAP_ALLOC_PAGES, precompile_and_serialize_versioned_wasm_runtime,
};
use sc_service::ChainSpec;
use sp_core::traits::RuntimeCode;
use sp_runtime::traits::Block as BlockT;
use sp_state_machine::backend::BackendRuntimeCode;
use std::{fmt::Debug, path::PathBuf, sync::Arc};

/// The `precompile-wasm` command used to serialize a precompiled WASM module.
#[derive(Debug, Parser)]
pub struct PrecompileWasmCmd {
	#[allow(missing_docs)]
	#[clap(flatten)]
	pub database_params: DatabaseParams,

	/// The default number of 64KB pages to ever allocate for Wasm execution.
	/// Don't alter this unless you know what you're doing.
	#[arg(long, value_name = "COUNT")]
	pub default_heap_pages: Option<u32>,

    /// path to the directory where precompiled artifact will be written
    #[arg()]
    pub output_dir: PathBuf,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub pruning_params: PruningParams,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub shared_params: SharedParams,

	/// The WASM instantiation method to use.
	/// Only has an effect when `wasm-execution` is set to `compiled`.
	/// The copy-on-write strategies are only supported on Linux.
	/// If the copy-on-write variant of a strategy is unsupported
	/// the executor will fall back to the non-CoW equivalent.
	/// The fastest (and the default) strategy available is `pooling-copy-on-write`.
	/// The `legacy-instance-reuse` strategy is deprecated and will
	/// be removed in the future. It should only be used in case of
	/// issues with the default instantiation strategy.
	#[arg(
		long,
		value_name = "STRATEGY",
		default_value_t = DEFAULT_WASMTIME_INSTANTIATION_STRATEGY,
		value_enum,
	)]
	pub wasmtime_instantiation_strategy: WasmtimeInstantiationStrategy,
}

impl PrecompileWasmCmd {
	/// Run the precompile-wasm command
	pub async fn run<B, BA>(&self, backend: Arc<BA>, spec: Box<dyn ChainSpec>) -> error::Result<()>
	where
        B: BlockT,
        BA: Backend<B>
	{
		let heap_pages = self.default_heap_pages.unwrap_or(DEFAULT_HEAP_ALLOC_PAGES);

		let blockchain_info = backend.blockchain().info();

		if backend.have_state_at(blockchain_info.finalized_hash, blockchain_info.finalized_number) {
			let state = backend.state_at(backend.blockchain().info().finalized_hash)?;

			precompile_and_serialize_versioned_wasm_runtime(
				HeapAllocStrategy::Static { extra_pages: heap_pages },
				&BackendRuntimeCode::new(&state).runtime_code()?,
				execution_method_from_cli(
					WasmExecutionMethod::Compiled,
					self.wasmtime_instantiation_strategy,
				),
				&self.output_dir,
			)
			.map_err(|e| Error::Application(Box::new(e)))?;
		} else {
			let storage = spec.as_storage_builder().build_storage()?;
			if let Some(wasm_bytecode) = storage.top.get(sp_storage::well_known_keys::CODE) {
				let runtime_code = RuntimeCode {
					code_fetcher: &sp_core::traits::WrappedRuntimeCode(
						wasm_bytecode.as_slice().into(),
					),
					hash: sp_core::blake2_256(&wasm_bytecode).to_vec(),
					heap_pages: Some(heap_pages as u64),
				};
				precompile_and_serialize_versioned_wasm_runtime(
					HeapAllocStrategy::Static { extra_pages: heap_pages },
					&runtime_code,
					execution_method_from_cli(
						WasmExecutionMethod::Compiled,
						self.wasmtime_instantiation_strategy,
					),
					&self.output_dir,
				)
				.map_err(|e| Error::Application(Box::new(e)))?;
			}
		}


        Ok(())
    }
}

impl CliConfiguration for PrecompileWasmCmd {
	fn shared_params(&self) -> &SharedParams {
		&self.shared_params
	}

	fn pruning_params(&self) -> Option<&PruningParams> {
		Some(&self.pruning_params)
	}

	fn database_params(&self) -> Option<&DatabaseParams> {
		Some(&self.database_params)
	}
}
